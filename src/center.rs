//! The `center` command: parse the protocol, build the groups, run the kernel.

use crate::args::Args;
use crate::error::Res;
use crate::group;
use crate::io::Io;
use crate::kernel::{self, Layout};
use crate::spec::{KeyMode, Spec};

pub fn run<I: Io>(io: &I, a: &Args) -> Res<()> {
    let spec = Spec::parse(a, io.nvars())?;
    let n = io.nobs();
    match spec.keymode {
        KeyMode::None => kernel::run(io, &spec, Layout { ord: None, starts: &[0, n] }),
        KeyMode::Sorted => {
            let starts = group::sorted_starts(io, &spec.keys, n, spec.threads)?;
            kernel::run(io, &spec, Layout { ord: None, starts: &starts })
        }
        KeyMode::Hash => {
            let g = group::hash_groups(io, &spec.keys, spec.touse, spec.threads, group::default_hash)?;
            kernel::run(io, &spec, Layout { ord: Some(&g.ord), starts: &g.starts })
        }
    }
}
