# center2

Fast replacement for Ben Jann's [`center`](http://ideas.repec.org/c/boc/bocode/s444102.html) (SSC) for
Stata 16 or newer: center or standardize variables, optionally by group. It gives `center`'s results and
messages; the work runs in a compiled Rust plugin (Linux, macOS, Windows) that reads and writes the data in
parallel and never sorts.

## Install

```stata
net install center2, from("https://leoliu0.github.io/center2")
```

Update with `net install center2, from("https://leoliu0.github.io/center2") replace`. Then see `help center2`.

## Use

```stata
center2 x1-x5                          // new variables c_x1 ... c_x5
center2 x1-x5, standardize replace     // standardize x1-x5 in place
center2 x1-x5, by(firm)                // within groups; the data need not be sorted
bysort firm: center2 x1-x5             // the by prefix works too
```

Options are `center`'s, except two names that follow Stata's convention (as in `destring` and `tostring`):
`replace` changes the variables in place (`center`'s `inplace`), and `force` overwrites existing variables
(`center`'s `replace`). New options: `by(varlist)` and `threads(#)`.

## Speed

20 million observations, 5 variables, StataNow 19.5 MP on 32 threads ([bench/RESULTS.md](bench/RESULTS.md)):

| `center` command | `center2` command | `center` s | `center2` s | speedup |
|---|---|---:|---:|---:|
| `x1-x5, inplace standardize` | `x1-x5, replace standardize` | 28.98 | 0.097 | 298.8x |
| `x1-x5, inplace` | `x1-x5, replace` | 10.68 | 0.097 | 110.1x |
| `x1-x5, standardize` | same | 31.88 | 0.531 | 60.0x |
| `x1-x5` | same | 12.37 | 0.528 | 23.4x |
| `bysort g: center x1-x5, standardize` (unsorted) | `x1-x5, by(g) standardize` | 9.43 | 0.876 | 10.8x |

When `center2` creates new variables, most of its time is Stata rewriting the dataset for each added
variable, a cost any implementation pays.

## Build from source

Needs Rust 1.85+ and a C compiler. `scripts/build_plugin.sh` builds `stata/center2.plugin`;
`scripts/install_local.sh` copies it with the ado and help files into your PERSONAL ado directory.
Tests: `cargo test --release`, and `stata-mp -b do run_all.do` in `tests/stata` (needs SSC `center`).

MIT license. `center2` reimplements `center` by Ben Jann.
