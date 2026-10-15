//! `selftest`: store-path probe (`tests/stata/stress_store.do`) and the benchmarks' I/O floor.
//!
//! Varlist `v1..vk t1..tk`; keys `k`, `sign` (1 or -1), `threads`, `serialstore`. For every
//! observation, in blocks of 8192 through [`for_blocks`], writes `t_j = sign * v_j` (missing
//! stays missing).

use crate::args::Args;
use crate::error::{Error, Res, vec_filled};
use crate::io::{Io, Rows};
use crate::par::{OwnedRows, StoreMode, WriteJob, for_blocks};

const BLOCK: usize = 8192;

pub fn run<I: Io>(io: &I, a: &Args) -> Res<()> {
    a.only(&["k", "sign", "threads", "serialstore"])?;
    let k = a.usize_req("k")?;
    let sign = a.f64_req("sign")?;
    if sign != 1.0 && sign != -1.0 {
        return Err(Error::syntax("center2 plugin: selftest sign must be 1 or -1"));
    }
    let threads = a.usize_or("threads", 0)?;
    let store = StoreMode::from_serial(a.str_opt("serialstore").map(|_| a.flag_or("serialstore", false)).transpose()?);
    if k == 0 || io.nvars() != 2 * k {
        return Err(Error::syntax("center2 plugin: selftest needs 2k variables"));
    }
    let src: Vec<i32> = (1..=k as i32).collect();
    let dst: Vec<i32> = (k as i32 + 1..=2 * k as i32).collect();
    let n = io.nobs();
    let miss = io.missval();
    for_blocks(io, n.div_ceil(BLOCK), threads, store, |b, emit| {
        let first = b * BLOCK;
        let m = BLOCK.min(n - first);
        let mut vals = vec_filled(m * k, 0.0)?;
        io.read_block(&src, Rows::Range { first, n: m }, &mut vals)?;
        for v in &mut vals {
            *v = if *v >= miss { f64::NAN } else { sign * *v };
        }
        emit(WriteJob { vars: dst.clone(), rows: OwnedRows::Range { first, n: m }, vals, skip: None })
    })?;
    Ok(())
}
