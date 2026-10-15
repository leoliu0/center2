//! Errors handed back to Stata: a return code plus a one-line message.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub rc: i32,
    pub msg: String,
}

pub type Res<T> = Result<T, Error>;

impl Error {
    pub fn new(rc: i32, msg: impl Into<String>) -> Self {
        Self { rc, msg: msg.into() }
    }

    /// r(198): a malformed plugin call (an ado/plugin mismatch, never user input) or bad data.
    pub fn syntax(msg: impl Into<String>) -> Self {
        Self::new(198, msg)
    }

    pub fn oom() -> Self {
        Self::new(909, "center2: out of memory")
    }

    pub fn spi(rc: i32, what: &str) -> Self {
        Self::new(rc, format!("center2: Stata returned r({rc}) while {what}"))
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.msg)
    }
}

/// `len` copies of `v`, or r(909) when the allocation fails.
pub fn vec_filled<T: Clone>(len: usize, v: T) -> Res<Vec<T>> {
    let mut out = Vec::new();
    out.try_reserve_exact(len).map_err(|_| Error::oom())?;
    out.resize(len, v);
    Ok(out)
}

/// An empty vector with room for `cap` elements, or r(909).
pub fn vec_with_capacity<T>(cap: usize) -> Res<Vec<T>> {
    let mut out = Vec::new();
    out.try_reserve_exact(cap).map_err(|_| Error::oom())?;
    Ok(out)
}
