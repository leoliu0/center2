//! Data access. [`Io`] abstracts Stata's dataset: [`SpiIo`] reaches it through `c/glue.c`, and
//! `MemIo` (unit tests) is an in-memory dataset. Variables are 1-based varlist positions (the
//! Stata convention); observations are 0-based.
//!
//! `read_block` and `write_block` may run concurrently on disjoint observations; every other
//! method runs on the calling (Stata) thread only.

use std::ffi::{CString, c_char, c_int, c_uint, c_void};

use crate::error::{Error, Res};

/// Observations of one block: a contiguous range or an explicit (0-based) list.
#[derive(Clone, Copy)]
pub enum Rows<'a> {
    Range { first: usize, n: usize },
    List(&'a [u32]),
}

impl Rows<'_> {
    pub fn len(&self) -> usize {
        match self {
            Rows::Range { n, .. } => *n,
            Rows::List(l) => l.len(),
        }
    }

    /// The 0-based observation of row `r`.
    #[cfg(test)]
    pub fn obs(&self, r: usize) -> usize {
        match self {
            Rows::Range { first, .. } => first + r,
            Rows::List(l) => l[r] as usize,
        }
    }
}

pub trait Io: Sync {
    /// Observations in the dataset.
    fn nobs(&self) -> usize;
    /// Variables in the `plugin call` varlist.
    fn nvars(&self) -> usize;
    /// Stata's system missing value; every missing value (`.`, `.a`-`.z`) is `>=` it.
    fn missval(&self) -> f64;
    /// Raw values, row-major: `out[r * vars.len() + j]` = `vars[j]` at row `r`.
    fn read_block(&self, vars: &[i32], rows: Rows<'_>, out: &mut [f64]) -> Res<()>;
    /// Writes `vals` (row-major as in `read_block`); non-finite values become missing (`.`);
    /// `skip[r * vars.len() + j] != 0` leaves that cell untouched.
    fn write_block(&self, vars: &[i32], rows: Rows<'_>, vals: &[f64], skip: Option<&[u8]>) -> Res<()>;
    fn is_str(&self, var: i32) -> bool;
    /// Appends the bytes of string variable `var` at `obs` to `out`.
    fn read_str(&self, var: i32, obs: usize, out: &mut Vec<u8>) -> Res<()>;
}

unsafe extern "C" {
    fn c2_set_plugin(p: *mut c_void);
    fn c2_nobs() -> c_int;
    fn c2_nvars() -> c_int;
    fn c2_missval() -> f64;
    fn c2_read_block(vars: *const c_int, nv: c_int, obs: *const c_uint, first: c_int, n: c_int, out: *mut f64) -> c_int;
    fn c2_write_block(
        vars: *const c_int,
        nv: c_int,
        obs: *const c_uint,
        first: c_int,
        n: c_int,
        vals: *const f64,
        skip: *const u8,
    ) -> c_int;
    fn c2_is_str(var: c_int) -> c_int;
    fn c2_is_strl(var: c_int) -> c_int;
    fn c2_read_str(var: c_int, obs: c_int, buf: *mut c_char) -> c_int;
    fn c2_strl_len(var: c_int, obs: c_int) -> c_int;
    fn c2_read_strl(var: c_int, obs: c_int, buf: *mut c_char, cap: c_int) -> c_int;
    fn c2_macro_save(name: *const c_char, value: *const c_char) -> c_int;
    fn c2_error(s: *const c_char);
}

/// Buffer size for `str#` values (str2045 + terminator).
const STR_BUF: usize = 2046;

fn cstr(s: &str) -> CString {
    CString::new(s.replace('\0', " ")).expect("no interior NUL")
}

/// Stores the plugin table handed to `pginit`.
///
/// # Safety
/// `p` must be the `ST_plugin*` Stata passes to `pginit`.
pub unsafe fn set_plugin(p: *mut c_void) {
    unsafe { c2_set_plugin(p) }
}

/// Sets local macro `name` in the program that ran `plugin call`.
pub fn local_save(name: &str, value: &str) -> Res<()> {
    let n = cstr(&format!("_{name}"));
    let v = cstr(value);
    let rc = unsafe { c2_macro_save(n.as_ptr(), v.as_ptr()) };
    if rc != 0 { Err(Error::spi(rc, &format!("saving local {name}"))) } else { Ok(()) }
}

/// Prints an error message (red) in the Results window.
pub fn error(s: &str) {
    let c = cstr(s);
    unsafe { c2_error(c.as_ptr()) };
}

/// Stata's dataset, through the SPI.
pub struct SpiIo {
    nobs: usize,
    nvars: usize,
    missval: f64,
}

impl SpiIo {
    pub fn new() -> SpiIo {
        unsafe { SpiIo { nobs: c2_nobs().max(0) as usize, nvars: c2_nvars().max(0) as usize, missval: c2_missval() } }
    }
}

fn split_rows(rows: Rows<'_>) -> (*const c_uint, c_int, c_int) {
    match rows {
        Rows::Range { first, n } => (std::ptr::null(), first as c_int, n as c_int),
        Rows::List(l) => (l.as_ptr(), 0, l.len() as c_int),
    }
}

impl Io for SpiIo {
    fn nobs(&self) -> usize {
        self.nobs
    }

    fn nvars(&self) -> usize {
        self.nvars
    }

    fn missval(&self) -> f64 {
        self.missval
    }

    fn read_block(&self, vars: &[i32], rows: Rows<'_>, out: &mut [f64]) -> Res<()> {
        assert_eq!(out.len(), rows.len() * vars.len());
        let (obs, first, n) = split_rows(rows);
        let rc = unsafe { c2_read_block(vars.as_ptr(), vars.len() as c_int, obs, first, n, out.as_mut_ptr()) };
        if rc != 0 { Err(Error::spi(rc, "reading variables")) } else { Ok(()) }
    }

    fn write_block(&self, vars: &[i32], rows: Rows<'_>, vals: &[f64], skip: Option<&[u8]>) -> Res<()> {
        assert_eq!(vals.len(), rows.len() * vars.len());
        if let Some(s) = skip {
            assert_eq!(s.len(), vals.len());
        }
        let (obs, first, n) = split_rows(rows);
        let skip = skip.map_or(std::ptr::null(), <[u8]>::as_ptr);
        let rc = unsafe { c2_write_block(vars.as_ptr(), vars.len() as c_int, obs, first, n, vals.as_ptr(), skip) };
        if rc != 0 { Err(Error::spi(rc, "storing variables")) } else { Ok(()) }
    }

    fn is_str(&self, var: i32) -> bool {
        unsafe { c2_is_str(var) != 0 }
    }

    fn read_str(&self, var: i32, obs: usize, out: &mut Vec<u8>) -> Res<()> {
        let o = obs as c_int + 1;
        if unsafe { c2_is_strl(var) } != 0 {
            let len = unsafe { c2_strl_len(var, o) };
            if len < 0 {
                return Err(Error::syntax("center2: binary strL values cannot be used in by()"));
            }
            let len = len as usize;
            let mut buf = vec![0u8; len + 1];
            let rc = unsafe { c2_read_strl(var, o, buf.as_mut_ptr().cast(), (len + 1) as c_int) };
            if rc < 0 {
                return Err(Error::spi(-rc, "reading a strL variable"));
            }
            let end = buf[..len].iter().position(|&b| b == 0).unwrap_or(len);
            out.extend_from_slice(&buf[..end]);
        } else {
            let mut buf = [0u8; STR_BUF];
            let len = unsafe { c2_read_str(var, o, buf.as_mut_ptr().cast()) };
            if len < 0 {
                return Err(Error::spi(-len, "reading a string variable"));
            }
            out.extend_from_slice(&buf[..len as usize]);
        }
        Ok(())
    }
}

#[cfg(test)]
pub use mem::MemIo;

#[cfg(test)]
mod mem {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::{Io, Rows};
    use crate::error::Res;

    /// Stata's system missing value (2^1023).
    pub const MISS: f64 = 8.98846567431158e307;

    pub enum Col {
        Num(Vec<AtomicU64>),
        Str(Vec<Vec<u8>>),
    }

    /// An in-memory dataset: columns plus a varlist mapping positions (1-based) to columns, so
    /// one column may appear at several positions (aliasing, as in Stata).
    pub struct MemIo {
        pub cols: Vec<Col>,
        pub varlist: Vec<usize>,
        pub n: usize,
    }

    impl MemIo {
        pub fn new(n: usize) -> MemIo {
            MemIo { cols: Vec::new(), varlist: Vec::new(), n }
        }

        /// Adds a numeric column; returns its column index.
        pub fn add_num(&mut self, v: &[f64]) -> usize {
            assert_eq!(v.len(), self.n);
            self.cols.push(Col::Num(v.iter().map(|x| AtomicU64::new(x.to_bits())).collect()));
            self.cols.len() - 1
        }

        pub fn add_str(&mut self, v: Vec<Vec<u8>>) -> usize {
            assert_eq!(v.len(), self.n);
            self.cols.push(Col::Str(v));
            self.cols.len() - 1
        }

        /// Appends column `c` to the varlist; returns its 1-based position.
        pub fn push_var(&mut self, c: usize) -> i32 {
            self.varlist.push(c);
            self.varlist.len() as i32
        }

        pub fn col(&self, c: usize) -> Vec<f64> {
            match &self.cols[c] {
                Col::Num(v) => v.iter().map(|x| f64::from_bits(x.load(Ordering::Relaxed))).collect(),
                Col::Str(_) => panic!("string column"),
            }
        }

        fn num(&self, var: i32) -> &[AtomicU64] {
            match &self.cols[self.varlist[var as usize - 1]] {
                Col::Num(v) => v,
                Col::Str(_) => panic!("string column read as numeric"),
            }
        }
    }

    impl Io for MemIo {
        fn nobs(&self) -> usize {
            self.n
        }

        fn nvars(&self) -> usize {
            self.varlist.len()
        }

        fn missval(&self) -> f64 {
            MISS
        }

        fn read_block(&self, vars: &[i32], rows: Rows<'_>, out: &mut [f64]) -> Res<()> {
            let nv = vars.len();
            for r in 0..rows.len() {
                let o = rows.obs(r);
                for (j, &v) in vars.iter().enumerate() {
                    out[r * nv + j] = f64::from_bits(self.num(v)[o].load(Ordering::Relaxed));
                }
            }
            Ok(())
        }

        fn write_block(&self, vars: &[i32], rows: Rows<'_>, vals: &[f64], skip: Option<&[u8]>) -> Res<()> {
            let nv = vars.len();
            for r in 0..rows.len() {
                let o = rows.obs(r);
                for (j, &v) in vars.iter().enumerate() {
                    if skip.is_some_and(|s| s[r * nv + j] != 0) {
                        continue;
                    }
                    let z = vals[r * nv + j];
                    let z = if z.is_finite() { z } else { MISS };
                    self.num(v)[o].store(z.to_bits(), Ordering::Relaxed);
                }
            }
            Ok(())
        }

        fn is_str(&self, var: i32) -> bool {
            matches!(self.cols[self.varlist[var as usize - 1]], Col::Str(_))
        }

        fn read_str(&self, var: i32, obs: usize, out: &mut Vec<u8>) -> Res<()> {
            match &self.cols[self.varlist[var as usize - 1]] {
                Col::Str(v) => out.extend_from_slice(&v[obs]),
                Col::Num(_) => panic!("numeric column read as string"),
            }
            Ok(())
        }
    }
}

#[cfg(test)]
pub use mem::MISS;
