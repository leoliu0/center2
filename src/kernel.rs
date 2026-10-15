//! The centering kernel; [`crate::spec`] documents the semantics.
//!
//! Positions are cut into blocks of [`BLOCK`]. A *segment* is the part of a group inside one
//! block: *complete* when the whole group lies in the block, *crossing* otherwise.
//! - Phase A (parallel over blocks): read the block once; finish every complete group from the
//!   buffer (two-pass mean and variance) and write its rows; keep partial sums of the crossing
//!   segments.
//! - Phase B (calling thread): combine the partial sums of each crossing group in block order;
//!   with `std`, the variance comes from Chan's formula (positive weights) or, for iweights
//!   (possibly negative), from Phase B′: an exact second pass over the crossing rows.
//! - Phase C (parallel over crossing segments): re-read those rows, write their outputs.
//!
//! Every sum runs in position order and partial results combine in block order, so results are
//! bitwise identical at any thread count. Within a block a row is read before it is written and
//! never read again, so outputs may alias inputs (in-place writes, reused targets).

use crate::error::{Res, vec_filled};
use crate::io::{Io, Rows};
use crate::par::{Emit, OwnedRows, WriteJob, for_blocks};
use crate::spec::{Fill, IfIn, Spec, Theta, WType};

/// Positions per block.
pub const BLOCK: usize = 8192;
/// Variables processed per pass over the data.
pub const VAR_BATCH: usize = 32;

/// Positions → observations (identity, or `ord`), and the group boundaries over positions.
#[derive(Clone, Copy)]
pub struct Layout<'a> {
    pub ord: Option<&'a [u32]>,
    pub starts: &'a [usize],
}

impl<'a> Layout<'a> {
    fn npos(&self) -> usize {
        self.starts.last().copied().unwrap_or(0)
    }

    fn rows(&self, lo: usize, hi: usize) -> Rows<'a> {
        match self.ord {
            None => Rows::Range { first: lo, n: hi - lo },
            Some(o) => Rows::List(&o[lo..hi]),
        }
    }

    fn owned_rows(&self, lo: usize, hi: usize) -> OwnedRows {
        match self.ord {
            None => OwnedRows::Range { first: lo, n: hi - lo },
            Some(o) => OwnedRows::List(o[lo..hi].to_vec()),
        }
    }
}

pub fn run<I: Io>(io: &I, spec: &Spec, lay: Layout<'_>) -> Res<()> {
    let mut j0 = 0;
    while j0 < spec.k {
        let kb = VAR_BATCH.min(spec.k - j0);
        Batch::new(io, spec, j0, kb).run(lay)?;
        j0 += kb;
    }
    Ok(())
}

#[derive(Clone, Copy, Default)]
struct Sums {
    s0: f64,
    s1: f64,
    n: f64,
}

impl Sums {
    fn add(&mut self, o: &Sums) {
        self.s0 += o.s0;
        self.s1 += o.s1;
        self.n += o.n;
    }
}

#[derive(Clone, Copy)]
struct Stat {
    mean: f64,
    sd: f64,
}

/// Rows of one block or segment, decoded.
struct Block {
    m: usize,
    s: Vec<bool>,
    ifin: Vec<bool>,
    w: Vec<f64>,
    th: Vec<f64>,
    /// Column-major: `x[j * m + r]`; NaN for missing.
    x: Vec<f64>,
}

impl Block {
    fn sums(&self, j: usize, r0: usize, r1: usize) -> Sums {
        let x = &self.x[j * self.m..(j + 1) * self.m];
        let mut s = Sums::default();
        for r in r0..r1 {
            let v = x[r];
            if self.s[r] && !v.is_nan() {
                let w = self.w[r];
                s.s0 += w;
                s.s1 += w * v;
                s.n += 1.0;
            }
        }
        s
    }

    fn m2(&self, j: usize, r0: usize, r1: usize, mean: f64) -> f64 {
        let x = &self.x[j * self.m..(j + 1) * self.m];
        let mut acc = 0.0;
        for r in r0..r1 {
            let v = x[r];
            if self.s[r] && !v.is_nan() {
                let d = v - mean;
                acc += self.w[r] * d * d;
            }
        }
        acc
    }
}

/// Partial sums of a crossing segment `[lo, hi)` of `group`.
struct Part {
    group: usize,
    lo: usize,
    hi: usize,
    sums: Vec<Sums>,
    /// Segment mean and Σw(x−mean_seg)², kept only for Chan's formula.
    mloc: Vec<f64>,
    m2: Vec<f64>,
}

/// One crossing group: totals, variance accumulator, final statistics.
struct Cross {
    tot: Vec<Sums>,
    m2: Vec<f64>,
    stats: Vec<Stat>,
}

struct Batch<'a, I: Io> {
    io: &'a I,
    spec: &'a Spec,
    j0: usize,
    kb: usize,
    read_vars: Vec<i32>,
    c_touse: Option<usize>,
    c_ifin: Option<usize>,
    c_w: Option<usize>,
    c_th: Option<usize>,
    c_x: usize,
    out_vars: Vec<i32>,
    nout: usize,
    miss: f64,
    theta: f64,
    fw_iw: bool,
    /// `std` with weights that are all positive: crossing variances by Chan's formula.
    chan: bool,
}

impl<'a, I: Io> Batch<'a, I> {
    fn new(io: &'a I, spec: &'a Spec, j0: usize, kb: usize) -> Self {
        let mut read_vars = Vec::new();
        let col = |v: i32, rv: &mut Vec<i32>| {
            rv.push(v);
            rv.len() - 1
        };
        let c_touse = spec.touse.map(|v| col(v, &mut read_vars));
        let c_ifin = match spec.ifin {
            IfIn::Var(v) => Some(col(v, &mut read_vars)),
            _ => None,
        };
        let c_w = spec.w.map(|v| col(v, &mut read_vars));
        let (c_th, theta) = match spec.theta {
            Theta::Var(v) => (Some(col(v, &mut read_vars)), 0.0),
            Theta::Const(t) => (None, t),
        };
        let c_x = read_vars.len();
        read_vars.extend_from_slice(&spec.x[j0..j0 + kb]);
        let mut out_vars = spec.out[j0..j0 + kb].to_vec();
        if let Some(m) = &spec.mean {
            out_vars.extend_from_slice(&m[j0..j0 + kb]);
        }
        if let Some(s) = &spec.sd {
            out_vars.extend_from_slice(&s[j0..j0 + kb]);
        }
        Batch {
            io,
            spec,
            j0,
            kb,
            read_vars,
            c_touse,
            c_ifin,
            c_w,
            c_th,
            c_x,
            nout: out_vars.len(),
            out_vars,
            miss: io.missval(),
            theta,
            fw_iw: matches!(spec.wtype, WType::Fw | WType::Iw),
            chan: spec.std && spec.wtype != WType::Iw,
        }
    }

    fn load(&self, rows: Rows<'_>) -> Res<Block> {
        let m = rows.len();
        let nr = self.read_vars.len();
        let mut buf = vec_filled(m * nr, 0.0)?;
        self.io.read_block(&self.read_vars, rows, &mut buf)?;
        let miss = self.miss;
        let num = |v: f64| if v >= miss { f64::NAN } else { v };
        let mut b = Block {
            m,
            s: vec_filled(m, true)?,
            ifin: vec_filled(m, true)?,
            w: vec_filled(m, 1.0)?,
            th: vec_filled(m, self.theta)?,
            x: vec_filled(self.kb * m, 0.0)?,
        };
        for r in 0..m {
            let row = &buf[r * nr..(r + 1) * nr];
            if let Some(c) = self.c_touse {
                b.s[r] = row[c] != 0.0;
            }
            b.ifin[r] = match self.spec.ifin {
                IfIn::All => true,
                IfIn::Touse => b.s[r],
                IfIn::Var(_) => row[self.c_ifin.expect("ifin column")] != 0.0,
            };
            if let Some(c) = self.c_w {
                b.w[r] = num(row[c]);
            }
            if let Some(c) = self.c_th {
                b.th[r] = num(row[c]);
            }
            for j in 0..self.kb {
                b.x[j * m + r] = num(row[self.c_x + j]);
            }
        }
        Ok(b)
    }

    fn denom(&self, s: &Sums) -> f64 {
        if self.fw_iw { s.s0 - 1.0 } else { s.s0 - s.s0 / s.n }
    }

    /// Output values of rows `r0..r1` of `b` into row-major `vals`/`skip` (rows indexed like `b`).
    fn fill(&self, b: &Block, r0: usize, r1: usize, stats: &[Stat], vals: &mut [f64], skip: &mut [u8]) {
        let (kb, nout, m) = (self.kb, self.nout, b.m);
        let has_mean = self.spec.mean.is_some();
        let has_sd = self.spec.sd.is_some();
        for r in r0..r1 {
            let base = r * nout;
            for j in 0..kb {
                let x = b.x[j * m + r];
                let st = stats[j];
                let v = (x - b.th[r] * st.mean) / st.sd;
                match self.spec.fill[self.j0 + j] {
                    Fill::All => vals[base + j] = if b.s[r] { v } else { f64::NAN },
                    Fill::InPlace => {
                        if b.s[r] {
                            vals[base + j] = v;
                        } else if b.ifin[r] {
                            vals[base + j] = f64::NAN;
                        } else {
                            skip[base + j] = 1;
                        }
                    }
                }
                let valid = b.s[r] && !x.is_nan();
                let mut c = kb;
                if has_mean {
                    vals[base + c + j] = if valid { st.mean } else { f64::NAN };
                    c += kb;
                }
                if has_sd {
                    vals[base + c + j] = if valid { st.sd } else { f64::NAN };
                }
            }
        }
    }

    fn phase_a(&self, lay: Layout<'_>, b_idx: usize, emit: &mut Emit<'_>) -> Res<Vec<Part>> {
        let lo = b_idx * BLOCK;
        let hi = (lo + BLOCK).min(lay.npos());
        let m = hi - lo;
        let blk = self.load(lay.rows(lo, hi))?;
        let starts = lay.starts;
        let mut g = starts.partition_point(|&s| s <= lo) - 1;
        let mut vals = vec_filled(m * self.nout, 0.0)?;
        let mut skip = vec_filled(m * self.nout, 0u8)?;
        let mut stats = vec![Stat { mean: 0.0, sd: 1.0 }; self.kb];
        let mut parts = Vec::new();
        let mut any_complete = false;
        while g + 1 < starts.len() && starts[g] < hi {
            let (gs, ge) = (starts[g], starts[g + 1]);
            if ge > gs {
                let (r0, r1) = (gs.max(lo) - lo, ge.min(hi) - lo);
                if gs >= lo && ge <= hi {
                    for (j, st) in stats.iter_mut().enumerate() {
                        let s = blk.sums(j, r0, r1);
                        let mean = s.s1 / s.s0;
                        let sd = if self.spec.std { (blk.m2(j, r0, r1, mean) / self.denom(&s)).sqrt() } else { 1.0 };
                        *st = Stat { mean, sd };
                    }
                    self.fill(&blk, r0, r1, &stats, &mut vals, &mut skip);
                    any_complete = true;
                } else {
                    let mut p = Part {
                        group: g,
                        lo: r0 + lo,
                        hi: r1 + lo,
                        sums: Vec::with_capacity(self.kb),
                        mloc: Vec::new(),
                        m2: Vec::new(),
                    };
                    for j in 0..self.kb {
                        let s = blk.sums(j, r0, r1);
                        if self.chan {
                            let ml = s.s1 / s.s0;
                            p.mloc.push(ml);
                            p.m2.push(if s.n > 0.0 { blk.m2(j, r0, r1, ml) } else { 0.0 });
                        }
                        p.sums.push(s);
                    }
                    skip[r0 * self.nout..r1 * self.nout].fill(1);
                    parts.push(p);
                }
            }
            g += 1;
        }
        if any_complete {
            emit(WriteJob { vars: self.out_vars.clone(), rows: lay.owned_rows(lo, hi), vals, skip: Some(skip) })?;
        }
        Ok(parts)
    }

    fn run(&self, lay: Layout<'_>) -> Res<()> {
        let (threads, store, kb) = (self.spec.threads, self.spec.store, self.kb);
        let nblocks = lay.npos().div_ceil(BLOCK);

        // Phase A
        let per_block = for_blocks(self.io, nblocks, threads, store, |b, emit| self.phase_a(lay, b, emit))?;
        let parts: Vec<Part> = per_block.into_iter().flatten().collect();
        if parts.is_empty() {
            return Ok(());
        }

        // Phase B: combine each crossing group's parts in block order.
        let mut crosses: Vec<Cross> = Vec::new();
        let mut part_cross = Vec::with_capacity(parts.len());
        for (pi, p) in parts.iter().enumerate() {
            if pi == 0 || parts[pi - 1].group != p.group {
                crosses.push(Cross {
                    tot: vec![Sums::default(); kb],
                    m2: vec![0.0; kb],
                    stats: vec![Stat { mean: 0.0, sd: 1.0 }; kb],
                });
            }
            let c = crosses.last_mut().expect("a cross group");
            for (t, s) in c.tot.iter_mut().zip(&p.sums) {
                t.add(s);
            }
            part_cross.push(crosses.len() - 1);
        }
        for c in &mut crosses {
            for (st, t) in c.stats.iter_mut().zip(&c.tot) {
                st.mean = t.s1 / t.s0;
            }
        }
        if self.spec.std {
            if self.chan {
                for (pi, p) in parts.iter().enumerate() {
                    let c = &mut crosses[part_cross[pi]];
                    for j in 0..kb {
                        if p.sums[j].n > 0.0 {
                            let d = p.mloc[j] - c.stats[j].mean;
                            c.m2[j] += p.m2[j] + p.sums[j].s0 * d * d;
                        }
                    }
                }
            } else {
                // Phase B′: exact second pass over the crossing rows (iweights may be negative).
                let crosses_ref = &crosses;
                let part_m2 = for_blocks(self.io, parts.len(), threads, store, |pi, _emit| {
                    let p = &parts[pi];
                    let blk = self.load(lay.rows(p.lo, p.hi))?;
                    let st = &crosses_ref[part_cross[pi]].stats;
                    Ok((0..kb).map(|j| blk.m2(j, 0, blk.m, st[j].mean)).collect::<Vec<f64>>())
                })?;
                for (pi, v) in part_m2.iter().enumerate() {
                    for (acc, x) in crosses[part_cross[pi]].m2.iter_mut().zip(v) {
                        *acc += x;
                    }
                }
            }
            for c in &mut crosses {
                for j in 0..kb {
                    c.stats[j].sd = (c.m2[j] / self.denom(&c.tot[j])).sqrt();
                }
            }
        }

        // Phase C: write the crossing rows.
        let crosses = &crosses;
        for_blocks(self.io, parts.len(), threads, store, |pi, emit| {
            let p = &parts[pi];
            let blk = self.load(lay.rows(p.lo, p.hi))?;
            let mut vals = vec_filled(blk.m * self.nout, 0.0)?;
            let mut skip = vec_filled(blk.m * self.nout, 0u8)?;
            self.fill(&blk, 0, blk.m, &crosses[part_cross[pi]].stats, &mut vals, &mut skip);
            emit(WriteJob { vars: self.out_vars.clone(), rows: lay.owned_rows(p.lo, p.hi), vals, skip: Some(skip) })
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::testutil::*;

    #[test]
    fn matches_reference_for_every_weight_type() {
        for wtype in ["none", "fw", "aw", "iw", "pw"] {
            let d = make_data(SIZES, 2, wtype, 7);
            for std in [false, true] {
                for theta in [Some(0.7), None] {
                    let o = Opts { wtype, std, theta, sd: std, ..Opts::default() };
                    assert_outcome(&run_center(&d, &o), &reference(&d, &o), &o, 1e-13);
                }
            }
        }
    }

    #[test]
    fn without_keys_the_sample_is_one_group() {
        let d = make_data(SIZES, 2, "aw", 8);
        let o = Opts { keymode: "none", wtype: "aw", std: true, sd: true, ..Opts::default() };
        assert_outcome(&run_center(&d, &o), &reference(&d, &o), &o, 1e-13);
    }

    #[test]
    fn bitwise_identical_across_thread_counts_and_store_modes() {
        let d = make_data(SIZES, 3, "aw", 11);
        let base = Opts { wtype: "aw", std: true, sd: true, threads: 1, ..Opts::default() };
        let one = run_center(&d, &base);
        for (threads, serial) in [(3, Some(false)), (32, Some(false)), (5, Some(true))] {
            let r = run_center(&d, &Opts { threads, serial, ..base.clone() });
            for (a, b) in [(&one.out, &r.out), (&one.mean, &r.mean), (&one.sd, &r.sd)] {
                for (ca, cb) in a.iter().zip(b) {
                    assert!(ca.iter().zip(cb).all(|(x, y)| x.to_bits() == y.to_bits()), "threads={threads}");
                }
            }
        }
    }

    #[test]
    fn in_place_writes_follow_ifin_and_read_inputs_first() {
        let d = make_data(SIZES, 2, "none", 3);
        for ifin_var in [false, true] {
            let o = Opts { inplace: true, ifin_var, std: true, sd: true, theta: None, ..Opts::default() };
            assert_outcome(&run_center(&d, &o), &reference(&d, &o), &o, 1e-13);
        }
    }

    #[test]
    fn iweights_with_a_negative_segment_take_the_exact_pass() {
        let mut d = make_data(SIZES, 2, "iw", 5);
        // Block 4 ([32768, 40960)) lies inside the 30000-observation group [24582, 54582).
        for w in &mut d.w[32768..40960] {
            *w = -0.5;
        }
        let o = Opts { wtype: "iw", std: true, sd: true, ..Opts::default() };
        assert_outcome(&run_center(&d, &o), &reference(&d, &o), &o, 1e-12);
    }
}
