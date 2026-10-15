//! Arguments of a `plugin call`: `argv[0]` is the command, every other token is `key=value`.
//! The ado quotes each token, because unquoted arguments are split by Stata at `= | ; * -`.

use std::collections::HashMap;

use crate::error::{Error, Res};

pub struct Args {
    pub cmd: String,
    map: HashMap<String, String>,
    tokens: Vec<String>,
}

fn bad(tok: &str) -> Error {
    Error::syntax(format!("center2 plugin: bad argument `{tok}`"))
}

impl Args {
    pub fn parse(argv: &[String]) -> Res<Args> {
        let cmd = argv.first().cloned().unwrap_or_default();
        let mut map = HashMap::new();
        let tokens: Vec<String> = argv.iter().skip(1).cloned().collect();
        for tok in &tokens {
            let Some((k, v)) = tok.split_once('=') else {
                return Err(bad(tok));
            };
            if k.is_empty() || map.insert(k.to_string(), v.to_string()).is_some() {
                return Err(bad(tok));
            }
        }
        Ok(Args { cmd, map, tokens })
    }

    /// Fails on any key outside `allowed` (catches ado/plugin protocol drift).
    pub fn only(&self, allowed: &[&str]) -> Res<()> {
        for tok in &self.tokens {
            let key = tok.split_once('=').map_or(tok.as_str(), |(k, _)| k);
            if !allowed.contains(&key) {
                return Err(bad(tok));
            }
        }
        Ok(())
    }

    pub fn str_opt(&self, key: &str) -> Option<&str> {
        self.map.get(key).map(String::as_str)
    }

    pub fn str_req(&self, key: &str) -> Res<&str> {
        self.str_opt(key).ok_or_else(|| Error::syntax(format!("center2 plugin: missing argument `{key}`")))
    }

    fn value_err(&self, key: &str) -> Error {
        bad(&format!("{key}={}", self.str_opt(key).unwrap_or("")))
    }

    pub fn usize_req(&self, key: &str) -> Res<usize> {
        self.str_req(key)?.parse().map_err(|_| self.value_err(key))
    }

    pub fn usize_or(&self, key: &str, default: usize) -> Res<usize> {
        match self.str_opt(key) {
            None => Ok(default),
            Some(s) => s.parse().map_err(|_| self.value_err(key)),
        }
    }

    /// `0` or `1`; `default` when absent.
    pub fn flag_or(&self, key: &str, default: bool) -> Res<bool> {
        match self.str_opt(key) {
            None => Ok(default),
            Some("0") => Ok(false),
            Some("1") => Ok(true),
            Some(_) => Err(self.value_err(key)),
        }
    }

    pub fn flag_req(&self, key: &str) -> Res<bool> {
        self.str_req(key)?;
        self.flag_or(key, false)
    }

    /// A real number; `.` (Stata missing) is NaN.
    pub fn f64_req(&self, key: &str) -> Res<f64> {
        let s = self.str_req(key)?.trim();
        if s == "." {
            return Ok(f64::NAN);
        }
        s.parse::<f64>().map_err(|_| self.value_err(key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(t: &[&str]) -> Vec<String> {
        t.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn rejects_duplicates_unknown_keys_and_bad_values() {
        assert!(Args::parse(&argv(&["center", "k=1", "k=2"])).is_err());
        assert!(Args::parse(&argv(&["center", "novalue"])).is_err());
        let a = Args::parse(&argv(&["center", "k=3", "theta=.", "std=2"])).unwrap();
        assert_eq!(a.usize_req("k").unwrap(), 3);
        assert!(a.f64_req("theta").unwrap().is_nan());
        assert!(a.flag_req("std").is_err());
        assert!(a.only(&["k", "theta"]).is_err());
        assert!(a.only(&["k", "theta", "std"]).is_ok());
    }
}
