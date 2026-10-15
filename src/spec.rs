//! Protocol of the `center` command between `stata/center2.ado` and the plugin. Change both
//! sides together.
//!
//! `plugin call <handle> <varlist>, center "key=value" ...`
//!
//! The varlist holds, in this order (a variable may appear more than once):
//! 1. keys (`nkey` of them)
//! 2. touse (if `touse=1`)
//! 3. ifin variable (if `ifin=var`)
//! 4. weight (if `wtype` is not `none`)
//! 5. theta variable (if `thetavar=1`)
//! 6. `x1..xk`, 7. `out1..outk`, 8. `mean1..meank` (if `mean=1`), 9. `sd1..sdk` (if `sd=1`)
//!
//! | key | values |
//! |---|---|
//! | `k` | number of `x` |
//! | `nkey` | number of keys |
//! | `keymode` | `none` (one group), `sorted` (data sorted by the keys), `hash` (`by()`) |
//! | `touse` | `0` or `1` |
//! | `ifin` | `all`, `touse` or `var` |
//! | `wtype` | `none`, `fw`, `aw`, `iw` or `pw` |
//! | `theta` | a number (`.` = missing); exclusive with `thetavar=1` |
//! | `std` | `0` or `1` |
//! | `fill` | k characters: `a` (write every observation) or `i` (in place) |
//! | `mean`, `sd` | `0` or `1`; `sd=1` requires `std=1` |
//! | `threads` | 0 = every logical CPU (default) |
//! | `serialstore` | optional: `1` writes on the calling thread only |
//!
//! Per observation i: `s_i` = touse != 0 (true without touse); `w_i` = weight (1 unweighted);
//! valid = `s_i` and x nonmissing. Within each group and variable, over valid observations:
//! `mean = Σw·x / Σw`; with `std`, `sd = sqrt(Σw(x−mean)² / denom)`, `denom = Σw − 1` for fw/iw
//! and `Σw − Σw/n` otherwise. Outputs, with `v = (x − θ_i·mean) / sd` (sd = 1 without `std`):
//! fill `a` writes `s_i ? v : .`; fill `i` writes `v` when `s_i`, `.` when only `ifin_i`, and
//! leaves the cell untouched otherwise; mean/sd outputs hold mean/sd at valid observations and
//! `.` elsewhere. Every non-finite result is stored as missing.

use crate::args::Args;
use crate::error::{Error, Res};
use crate::par::StoreMode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyMode {
    None,
    Sorted,
    Hash,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IfIn {
    All,
    Touse,
    Var(i32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WType {
    None,
    Fw,
    Aw,
    Iw,
    Pw,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Theta {
    Const(f64),
    Var(i32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fill {
    All,
    InPlace,
}

#[derive(Debug)]
pub struct Spec {
    pub k: usize,
    pub keys: Vec<i32>,
    pub keymode: KeyMode,
    pub touse: Option<i32>,
    pub ifin: IfIn,
    pub wtype: WType,
    pub w: Option<i32>,
    pub theta: Theta,
    pub std: bool,
    pub x: Vec<i32>,
    pub out: Vec<i32>,
    pub fill: Vec<Fill>,
    pub mean: Option<Vec<i32>>,
    pub sd: Option<Vec<i32>>,
    pub threads: usize,
    pub store: StoreMode,
}

pub const KEYS: &[&str] = &[
    "k", "nkey", "keymode", "touse", "ifin", "wtype", "theta", "thetavar", "std", "fill", "mean", "sd", "threads",
    "serialstore",
];

fn bad(msg: &str) -> Error {
    Error::syntax(format!("center2 plugin: {msg}"))
}

impl Spec {
    /// Parses and validates the `center` arguments against a varlist of `nvars` variables.
    pub fn parse(a: &Args, nvars: usize) -> Res<Spec> {
        a.only(KEYS)?;
        let k = a.usize_req("k")?;
        let nkey = a.usize_req("nkey")?;
        let keymode = match a.str_req("keymode")? {
            "none" => KeyMode::None,
            "sorted" => KeyMode::Sorted,
            "hash" => KeyMode::Hash,
            _ => return Err(bad("keymode must be none, sorted or hash")),
        };
        if (keymode == KeyMode::None) != (nkey == 0) {
            return Err(bad("keymode and nkey disagree"));
        }
        let has_touse = a.flag_req("touse")?;
        let ifin_kind = a.str_req("ifin")?;
        let wtype = match a.str_req("wtype")? {
            "none" => WType::None,
            "fw" => WType::Fw,
            "aw" => WType::Aw,
            "iw" => WType::Iw,
            "pw" => WType::Pw,
            _ => return Err(bad("wtype must be none, fw, aw, iw or pw")),
        };
        let theta_var = a.flag_or("thetavar", false)?;
        if theta_var == a.str_opt("theta").is_some() {
            return Err(bad("exactly one of theta= and thetavar=1 is required"));
        }
        let std = a.flag_req("std")?;
        let fill: Vec<Fill> = a
            .str_req("fill")?
            .chars()
            .map(|c| match c {
                'a' => Ok(Fill::All),
                'i' => Ok(Fill::InPlace),
                _ => Err(bad("fill characters must be a or i")),
            })
            .collect::<Res<_>>()?;
        let has_mean = a.flag_req("mean")?;
        let has_sd = a.flag_req("sd")?;
        if has_sd && !std {
            return Err(bad("sd=1 requires std=1"));
        }
        if k == 0 || fill.len() != k {
            return Err(bad("fill must have k characters and k must be positive"));
        }
        let threads = a.usize_or("threads", 0)?;
        let serial = match a.str_opt("serialstore") {
            None => None,
            Some(_) => Some(a.flag_or("serialstore", false)?),
        };

        // Varlist positions, in protocol order.
        let mut next: i32 = 1;
        let mut take = |n: usize| -> Vec<i32> {
            let v: Vec<i32> = (next..next + n as i32).collect();
            next += n as i32;
            v
        };
        let keys = take(nkey);
        let touse = has_touse.then(|| take(1)[0]);
        let ifin = match ifin_kind {
            "all" => IfIn::All,
            "touse" if has_touse => IfIn::Touse,
            "touse" => return Err(bad("ifin=touse requires touse=1")),
            "var" => IfIn::Var(take(1)[0]),
            _ => return Err(bad("ifin must be all, touse or var")),
        };
        let w = (wtype != WType::None).then(|| take(1)[0]);
        let theta = if theta_var { Theta::Var(take(1)[0]) } else { Theta::Const(a.f64_req("theta")?) };
        let x = take(k);
        let out = take(k);
        let mean = has_mean.then(|| take(k));
        let sd = has_sd.then(|| take(k));
        let used = (next - 1) as usize;
        if used != nvars {
            return Err(bad(&format!("the varlist has {nvars} variables, the arguments describe {used}")));
        }
        Ok(Spec {
            k,
            keys,
            keymode,
            touse,
            ifin,
            wtype,
            w,
            theta,
            std,
            x,
            out,
            fill,
            mean,
            sd,
            threads,
            store: StoreMode::from_serial(serial),
        })
    }
}
