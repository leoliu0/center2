//! Worker threads: the only place where threads are spawned. Work items run on
//! `std::thread::scope` workers that pull indices from an atomic counter. Results come back
//! ordered by item index, so callers stay deterministic whatever the scheduling.
//!
//! Writes go through the `emit` callback handed to each item. Under [`StoreMode::Parallel`] the
//! worker writes directly; under [`StoreMode::Serial`] the job travels through a bounded channel
//! to the calling thread, which performs every write.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::thread::ScopedJoinHandle;

use crate::error::{Error, Res, vec_with_capacity};
use crate::io::{Io, Rows};

/// Default store mode: parallel stores into disjoint observations are safe. Decided by
/// `tests/stata/stress_store.do` on StataNow 19.5 MP (Linux): 200 reps of 2e7 x 8 parallel
/// stores, 0 mismatches, no crash; 0.16 s parallel vs 0.64 s serial writes per rep.
pub const PARALLEL_STORE_DEFAULT: bool = true;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreMode {
    Parallel,
    Serial,
}

impl StoreMode {
    /// From the plugin key `serialstore` (absent: [`PARALLEL_STORE_DEFAULT`]).
    pub fn from_serial(serial: Option<bool>) -> StoreMode {
        match serial {
            Some(true) => StoreMode::Serial,
            Some(false) => StoreMode::Parallel,
            None if PARALLEL_STORE_DEFAULT => StoreMode::Parallel,
            None => StoreMode::Serial,
        }
    }
}

/// Observations of a write job (owned, so a job can cross threads).
pub enum OwnedRows {
    Range { first: usize, n: usize },
    List(Vec<u32>),
}

impl OwnedRows {
    pub fn as_rows(&self) -> Rows<'_> {
        match self {
            OwnedRows::Range { first, n } => Rows::Range { first: *first, n: *n },
            OwnedRows::List(l) => Rows::List(l),
        }
    }
}

/// One `write_block` call: `vals`/`skip` are row-major over `rows` x `vars`.
pub struct WriteJob {
    pub vars: Vec<i32>,
    pub rows: OwnedRows,
    pub vals: Vec<f64>,
    pub skip: Option<Vec<u8>>,
}

pub type Emit<'a> = dyn FnMut(WriteJob) -> Res<()> + 'a;

/// `threads` = 0 means every logical CPU; never more threads than work items.
pub fn resolve_threads(threads: usize, items: usize) -> usize {
    let t = if threads == 0 { std::thread::available_parallelism().map_or(1, |n| n.get()) } else { threads };
    t.min(items).max(1)
}

fn write_job<I: Io + ?Sized>(io: &I, job: &WriteJob) -> Res<()> {
    io.write_block(&job.vars, job.rows.as_rows(), &job.vals, job.skip.as_deref())
}

fn join<T>(h: ScopedJoinHandle<'_, T>) -> T {
    h.join().unwrap_or_else(|p| std::panic::resume_unwind(p))
}

/// Runs `f(item, emit)` for every item in `0..nitems` and returns the results in item order.
/// The first error stops further item pickup and is returned.
pub fn for_blocks<I, T, F>(io: &I, nitems: usize, threads: usize, store: StoreMode, f: F) -> Res<Vec<T>>
where
    I: Io + ?Sized,
    T: Send,
    F: Fn(usize, &mut Emit<'_>) -> Res<T> + Sync,
{
    if nitems == 0 {
        return Ok(Vec::new());
    }
    let t = resolve_threads(threads, nitems);
    if t == 1 {
        let mut out = vec_with_capacity(nitems)?;
        let mut emit = |job: WriteJob| write_job(io, &job);
        for i in 0..nitems {
            out.push(f(i, &mut emit)?);
        }
        return Ok(out);
    }

    let next = AtomicUsize::new(0);
    let failed = AtomicBool::new(false);
    let worker = |emit: &mut Emit<'_>| -> Res<Vec<(usize, T)>> {
        let mut done = Vec::new();
        while !failed.load(Ordering::Relaxed) {
            let i = next.fetch_add(1, Ordering::Relaxed);
            if i >= nitems {
                break;
            }
            match f(i, emit) {
                Ok(v) => done.push((i, v)),
                Err(e) => {
                    failed.store(true, Ordering::Relaxed);
                    return Err(e);
                }
            }
        }
        Ok(done)
    };
    let worker = &worker;

    let results: Vec<Res<Vec<(usize, T)>>> = match store {
        StoreMode::Parallel => std::thread::scope(|s| {
            let hs: Vec<_> = (0..t)
                .map(|_| {
                    s.spawn(move || {
                        let mut emit = |job: WriteJob| write_job(io, &job);
                        worker(&mut emit)
                    })
                })
                .collect();
            hs.into_iter().map(join).collect()
        }),
        StoreMode::Serial => std::thread::scope(|s| {
            let (tx, rx) = mpsc::sync_channel::<WriteJob>(2 * t);
            let hs: Vec<_> = (0..t)
                .map(|_| {
                    let tx = tx.clone();
                    s.spawn(move || {
                        let mut emit =
                            |job: WriteJob| tx.send(job).map_err(|_| Error::new(9999, "center2: the writer thread stopped"));
                        worker(&mut emit)
                    })
                })
                .collect();
            drop(tx);
            // The calling thread performs every write; after a failed write it keeps draining
            // the channel so that no worker blocks on a full channel.
            let mut write_err = None;
            for job in rx.iter() {
                if write_err.is_none()
                    && let Err(e) = write_job(io, &job)
                {
                    failed.store(true, Ordering::Relaxed);
                    write_err = Some(e);
                }
            }
            let mut rs: Vec<_> = hs.into_iter().map(join).collect();
            if let Some(e) = write_err {
                rs.insert(0, Err(e));
            }
            rs
        }),
    };

    let mut slots: Vec<Option<T>> = Vec::new();
    slots.try_reserve_exact(nitems).map_err(|_| Error::oom())?;
    slots.resize_with(nitems, || None);
    for r in results {
        for (i, v) in r? {
            slots[i] = Some(v);
        }
    }
    Ok(slots.into_iter().map(|s| s.expect("every item was processed")).collect())
}
