//! Test support: deterministic data, the `center` protocol over [`MemIo`], and a naive reference
//! implementation of the semantics documented in `spec`.

use std::collections::HashMap;

use crate::args::Args;
use crate::center;
use crate::io::{MISS, MemIo};

/// Extended missing value `.a` (Stata stores it just above `.`).
pub fn miss_a() -> f64 {
    MISS * (1.0 + 2f64.powi(-12))
}

pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Uniform on [0, 1).
    pub fn unif(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}

pub struct Data {
    pub n: usize,
    /// Group key per observation (sorted: groups are contiguous runs).
    pub key: Vec<f64>,
    pub touse: Vec<f64>,
    pub ifin: Vec<f64>,
    pub w: Vec<f64>,
    pub th: Vec<f64>,
    pub x: Vec<Vec<f64>>,
}

/// Groups of the given sizes (in order, keys 1, 2, ...), `k` variables, weights of `wtype`.
pub fn make_data(sizes: &[usize], k: usize, wtype: &str, seed: u64) -> Data {
    let mut rng = Rng::new(seed);
    let n: usize = sizes.iter().sum();
    let mut key = Vec::with_capacity(n);
    for (g, &s) in sizes.iter().enumerate() {
        key.extend(std::iter::repeat_n((g + 1) as f64, s));
    }
    let touse: Vec<f64> = (0..n).map(|_| if rng.unif() < 0.1 { 0.0 } else { 1.0 }).collect();
    let ifin: Vec<f64> = (0..n).map(|i| if touse[i] != 0.0 || rng.unif() < 0.5 { 1.0 } else { 0.0 }).collect();
    let w: Vec<f64> = (0..n)
        .map(|_| match wtype {
            "fw" => (1 + rng.next_u64() % 4) as f64,
            "iw" => rng.unif() * 1.2 - 0.2,
            _ => 0.5 + rng.unif(),
        })
        .collect();
    let th: Vec<f64> = (0..n).map(|_| if rng.unif() < 0.05 { MISS } else { rng.unif() }).collect();
    let x = (0..k)
        .map(|j| {
            (0..n)
                .map(|_| {
                    let u = rng.unif();
                    if u < 0.05 {
                        MISS
                    } else if u < 0.08 {
                        miss_a()
                    } else {
                        (rng.unif() - 0.5) * 10.0 * (j + 1) as f64 + 3.0
                    }
                })
                .collect()
        })
        .collect();
    Data { n, key, touse, ifin, w, th, x }
}

#[derive(Clone)]
pub struct Opts {
    pub keymode: &'static str,
    pub wtype: &'static str,
    pub std: bool,
    /// `None`: theta variable; `Some(t)`: constant.
    pub theta: Option<f64>,
    pub inplace: bool,
    pub ifin_var: bool,
    pub mean: bool,
    pub sd: bool,
    pub threads: usize,
    pub serial: Option<bool>,
}

impl Default for Opts {
    fn default() -> Self {
        Opts {
            keymode: "sorted",
            wtype: "none",
            std: false,
            theta: Some(1.0),
            inplace: false,
            ifin_var: false,
            mean: true,
            sd: false,
            threads: 4,
            serial: None,
        }
    }
}

pub const SENTINEL: f64 = 12345.0;

/// Output columns after a run: out (the x columns themselves when in place), mean, sd.
pub struct Outcome {
    pub out: Vec<Vec<f64>>,
    pub mean: Vec<Vec<f64>>,
    pub sd: Vec<Vec<f64>>,
}

/// Runs the `center` command on `d` through the protocol, over an in-memory dataset.
pub fn run_center(d: &Data, o: &Opts) -> Outcome {
    let k = d.x.len();
    let mut io = MemIo::new(d.n);
    let c_key = io.add_num(&d.key);
    let c_touse = io.add_num(&d.touse);
    let c_ifin = io.add_num(&d.ifin);
    let c_w = io.add_num(&d.w);
    let c_th = io.add_num(&d.th);
    let c_x: Vec<usize> = d.x.iter().map(|v| io.add_num(v)).collect();
    let sentinel = vec![SENTINEL; d.n];
    let c_out: Vec<usize> = if o.inplace { c_x.clone() } else { (0..k).map(|_| io.add_num(&sentinel)).collect() };
    let c_mean: Vec<usize> = if o.mean { (0..k).map(|_| io.add_num(&sentinel)).collect() } else { Vec::new() };
    let c_sd: Vec<usize> = if o.sd { (0..k).map(|_| io.add_num(&sentinel)).collect() } else { Vec::new() };

    let keyed = o.keymode != "none";
    if keyed {
        io.push_var(c_key);
    }
    io.push_var(c_touse);
    let ifin = if o.ifin_var {
        io.push_var(c_ifin);
        "var"
    } else if o.inplace {
        "touse"
    } else {
        "all"
    };
    if o.wtype != "none" {
        io.push_var(c_w);
    }
    if o.theta.is_none() {
        io.push_var(c_th);
    }
    for c in c_x.iter().chain(&c_out).chain(&c_mean).chain(&c_sd) {
        io.push_var(*c);
    }

    let mut argv: Vec<String> = vec![
        "center".into(),
        format!("k={k}"),
        format!("nkey={}", usize::from(keyed)),
        format!("keymode={}", o.keymode),
        "touse=1".into(),
        format!("ifin={ifin}"),
        format!("wtype={}", o.wtype),
        format!("std={}", u8::from(o.std)),
        format!("fill={}", if o.inplace { "i" } else { "a" }.repeat(k)),
        format!("mean={}", u8::from(o.mean)),
        format!("sd={}", u8::from(o.sd)),
        format!("threads={}", o.threads),
    ];
    match o.theta {
        Some(t) => argv.push(format!("theta={t:.17e}")),
        None => argv.push("thetavar=1".into()),
    }
    if let Some(s) = o.serial {
        argv.push(format!("serialstore={}", u8::from(s)));
    }
    center::run(&io, &Args::parse(&argv).expect("valid argv")).expect("center runs");
    Outcome {
        out: c_out.iter().map(|&c| io.col(c)).collect(),
        mean: c_mean.iter().map(|&c| io.col(c)).collect(),
        sd: c_sd.iter().map(|&c| io.col(c)).collect(),
    }
}

/// Expected raw values (missing stored as `MISS`, untouched cells keep their value), computed
/// group by group in observation order with plain two-pass sums.
pub fn reference(d: &Data, o: &Opts) -> Outcome {
    let k = d.x.len();
    let num = |v: f64| if v >= MISS { f64::NAN } else { v };
    let s = |i: usize| d.touse[i] != 0.0;
    // Group members (all observations of a key), observation order.
    let mut groups: HashMap<u64, Vec<usize>> = HashMap::new();
    for i in 0..d.n {
        let key = if o.keymode == "none" { 0 } else { d.key[i].to_bits() };
        groups.entry(key).or_default().push(i);
    }
    let fw_iw = matches!(o.wtype, "fw" | "iw");
    let wt = |i: usize| if o.wtype == "none" { 1.0 } else { d.w[i] };
    let mut out = vec![vec![0.0; d.n]; k];
    let mut mean = vec![vec![MISS; d.n]; k];
    let mut sd = vec![vec![MISS; d.n]; k];
    for members in groups.values() {
        for j in 0..k {
            let x = |i: usize| num(d.x[j][i]);
            let valid: Vec<usize> = members.iter().copied().filter(|&i| s(i) && !x(i).is_nan()).collect();
            let (mut s0, mut s1) = (0.0, 0.0);
            for &i in &valid {
                s0 += wt(i);
                s1 += wt(i) * x(i);
            }
            let m = s1 / s0;
            let sdev = if o.std {
                let mut m2 = 0.0;
                for &i in &valid {
                    m2 += wt(i) * (x(i) - m) * (x(i) - m);
                }
                let nv = valid.len() as f64;
                let denom = if fw_iw { s0 - 1.0 } else { s0 - s0 / nv };
                (m2 / denom).sqrt()
            } else {
                1.0
            };
            let fin = |v: f64| if v.is_finite() { v } else { MISS };
            for &i in members {
                let th = match o.theta {
                    Some(t) => t,
                    None => num(d.th[i]),
                };
                let v = fin((x(i) - th * m) / sdev);
                out[j][i] = if s(i) {
                    v
                } else if !o.inplace || (if o.ifin_var { d.ifin[i] != 0.0 } else { false }) {
                    MISS
                } else {
                    d.x[j][i]
                };
                if s(i) && !x(i).is_nan() {
                    mean[j][i] = fin(m);
                    sd[j][i] = fin(sdev);
                }
            }
        }
    }
    Outcome { out, mean, sd }
}

/// Missing cells must match bit for bit; others within `tol` (reldif).
pub fn assert_close(what: &str, got: &[f64], want: &[f64], tol: f64) {
    assert_eq!(got.len(), want.len());
    for (i, (&g, &w)) in got.iter().zip(want).enumerate() {
        if w >= MISS || g >= MISS {
            assert!(g.to_bits() == w.to_bits(), "{what}[{i}]: got {g:e}, want {w:e}");
        } else {
            let rd = (g - w).abs() / (w.abs() + 1.0);
            assert!(rd <= tol, "{what}[{i}]: got {g:e}, want {w:e} (reldif {rd:e})");
        }
    }
}

pub fn assert_outcome(got: &Outcome, want: &Outcome, o: &Opts, tol: f64) {
    for j in 0..got.out.len() {
        assert_close(&format!("out{j}"), &got.out[j], &want.out[j], tol);
        if o.mean {
            assert_close(&format!("mean{j}"), &got.mean[j], &want.mean[j], tol);
        }
        if o.sd {
            assert_close(&format!("sd{j}"), &got.sd[j], &want.sd[j], tol);
        }
    }
}

/// Groups around the block size, so that complete and crossing groups both occur.
pub const SIZES: &[usize] = &[3, 1, 2, 8191, 8192, 8193, 30000, 5, 17];
