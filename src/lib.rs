//! Stata plugin (SPI 3.0) behind `center2`, a fast drop-in replacement for SSC `center`.
//!
//! Only `pginit` and `stata_call` are exported. Every Stata API call goes through `c/glue.c`
//! (wrapped by [`io`]); worker threads live only inside a call ([`par`]). The ado/plugin
//! protocol is documented in [`spec`].

use std::ffi::{CStr, c_char, c_int, c_void};
use std::panic::{self, AssertUnwindSafe};
use std::sync::{Mutex, Once};

mod args;
mod center;
mod error;
mod group;
mod io;
mod kernel;
mod par;
mod selftest;
mod spec;
#[cfg(test)]
mod testutil;

use args::Args;
use error::{Error, Res};

/// `SD_PLUGINVER` = SF_MAKELONG(3, 0).
const SD_PLUGINVER: c_int = 3;

/// Message of the last panic (filled by the panic hook, consumed by `stata_call`).
static PANIC_MSG: Mutex<Option<String>> = Mutex::new(None);
static INIT: Once = Once::new();

/// Called by Stata when the plugin is loaded.
///
/// # Safety
/// `p` is the `ST_plugin*` table provided by Stata.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pginit(p: *mut c_void) -> c_int {
    unsafe { io::set_plugin(p) };
    INIT.call_once(|| {
        panic::set_hook(Box::new(|info| {
            let msg = match info.payload().downcast_ref::<&str>() {
                Some(s) => (*s).to_string(),
                None => info.payload().downcast_ref::<String>().cloned().unwrap_or_else(|| "unknown panic".into()),
            };
            let loc = info.location().map(|l| format!(" at {}:{}", l.file(), l.line())).unwrap_or_default();
            *PANIC_MSG.lock().unwrap_or_else(|e| e.into_inner()) = Some(format!("{msg}{loc}"));
        }));
    });
    SD_PLUGINVER
}

/// Entry point of `plugin call`; `argv[0]` is the command.
///
/// # Safety
/// `argv` holds `argc` NUL-terminated strings (Stata guarantees this).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn stata_call(argc: c_int, argv: *mut *mut c_char) -> c_int {
    let argv: Vec<String> = (0..argc.max(0) as usize)
        .map(|i| unsafe { CStr::from_ptr(*argv.add(i)) }.to_string_lossy().into_owned())
        .collect();
    match panic::catch_unwind(AssertUnwindSafe(|| run(&argv))) {
        Ok(Ok(())) => 0,
        Ok(Err(e)) => {
            io::error(&format!("{e}\n"));
            e.rc
        }
        Err(payload) => {
            let msg = PANIC_MSG.lock().unwrap_or_else(|e| e.into_inner()).take().unwrap_or_else(|| {
                payload
                    .downcast_ref::<&str>()
                    .map(|s| s.to_string())
                    .or_else(|| payload.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "unknown panic".into())
            });
            io::error(&format!("center2: internal error (panic: {msg}); please report it\n"));
            9999
        }
    }
}

fn run(argv: &[String]) -> Res<()> {
    let a = Args::parse(argv)?;
    match a.cmd.as_str() {
        "version" => {
            a.only(&[])?;
            io::local_save("c2_plugver", env!("CARGO_PKG_VERSION"))
        }
        "selftest" => selftest::run(&io::SpiIo::new(), &a),
        "center" => center::run(&io::SpiIo::new(), &a),
        c => Err(Error::syntax(format!("center2 plugin: unknown command `{c}`"))),
    }
}
