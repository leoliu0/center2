* stress_store.do -- parallel-store probe and store throughput of the center2 plugin.
* Run from tests/stata after scripts/build_plugin.sh:   stata-mp -b do stress_store.do
*
* Part 1: store throughput of selftest (t_j = v_j over 2e7 obs x 8 vars): writes on the
*         calling thread only (serialstore=1), parallel writes (serialstore=0), one thread.
* Part 2: 200 reps of parallel-store selftest calls (all threads, alternating sign so a missed
*         write is detected), each checked by Stata itself. Parallel stores become the plugin
*         default (src/par.rs PARALLEL_STORE_DEFAULT) only if all 200 reps match without a crash.

version 16
clear all
set more off
set seed 20261015

capture program drop _c2p
program _c2p, plugin using("../../stata/center2.plugin")

set obs 20000000
* a leading string variable makes varlist positions differ from dataset positions
gen str1 pad = "a"
forvalues j = 1/8 {
    gen double v`j' = rnormal() * `j'
}
replace v3 = . in 5/100
forvalues j = 1/4 {
    gen float t`j' = .
}
forvalues j = 5/8 {
    gen double t`j' = .
}

* ---------------------------------------------------------------- part 1
timer clear
forvalues r = 1/3 {
    timer on 1
    plugin call _c2p v1-v8 t1-t8, selftest "k=8" "sign=1" "threads=0" "serialstore=1"
    timer off 1
    timer on 2
    plugin call _c2p v1-v8 t1-t8, selftest "k=8" "sign=1" "threads=0" "serialstore=0"
    timer off 2
    timer on 3
    plugin call _c2p v1-v8 t1-t8, selftest "k=8" "sign=1" "threads=1"
    timer off 3
}
quietly timer list
di as result "STORE_THROUGHPUT serial=" %7.4f r(t1)/3 " parallel=" %7.4f r(t2)/3 " onethread=" %7.4f r(t3)/3 ///
    " (seconds per 2e7 x 8 read + write)"

* ---------------------------------------------------------------- part 2
local reps 200
local fails 0
forvalues r = 1/`reps' {
    local s = cond(mod(`r', 2), -1, 1)
    plugin call _c2p v1-v8 t1-t8, selftest "k=8" "sign=`s'" "threads=0" "serialstore=0"
    local bad 0
    forvalues j = 1/4 {
        capture assert t`j' == float(`s' * v`j')
        if _rc local bad 1
    }
    forvalues j = 5/8 {
        capture assert t`j' == `s' * v`j'
        if _rc local bad 1
    }
    if `bad' {
        local ++fails
        di as err "rep `r': parallel stores differ from the expected values"
    }
    if `r' == 1 | mod(`r', 50) == 0 {
        di as txt "rep `r' done"
    }
}
di as result "STRESS_STORE_RESULT fails=`fails' reps=`reps'"
