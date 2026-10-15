* bench.do -- center vs center2 timings and speed gates. From bench/ (after
* scripts/build_plugin.sh, with SSC center installed):
*   stata-mp -b do bench.do      -> writes RESULTS.md and prints BENCH_GATE lines
*
* Each run starts from a fresh frame copy (not timed). center runs once, center2 best of 3.
* Floors (best of 3, also on fresh copies):
* - add floor (generate scenarios): one nofill st_addvar of the same 5 output variables, the
*   cost Stata charges any implementation for creating them;
* - I/O floor t_io (inplace scenarios): plugin selftest, one parallel read of x1-x5 and one
*   parallel write of 5 double variables.
* The add floor varies by up to ~1 s between identical runs (page faults of Stata's dataset
* relayout), more than G6's 0.4 s budget. So generate scenarios also time "center2 without adds"
* (best of 3): the same command plus replace, with the 5 targets already present with the
* output type (created untimed), which center2 overwrites in place. That is center2's work
* beyond creating variables, measured directly; a generate scenario is at its floor when it is
* <= 0.4 s.
version 16
clear all
set more off
adopath ++ "../stata"
capture program drop _c2p
program _c2p, plugin using("../stata/center2.plugin")
set seed 1

* ---------------------------------------------------------------------------------------------
* data
* ---------------------------------------------------------------------------------------------
capture program drop c2_mkdata
program c2_mkdata
    args n nz
    quietly {
        set obs `n'
        gen long g = floor(runiform()*100000)
        forvalues j = 1/5 {
            gen double x`j' = rnormal()
        }
        gen double u = runiform()
        gen double w = runiform() + .5
        forvalues j = 1/`nz' {
            gen float z`j' = runiform()
        }
        sort g
    }
end
frame create D1
frame D1: c2_mkdata 20000000 0
frame copy D1 D1u
frame D1u: sort u
frame create D2
frame D2: c2_mkdata 5000000 50

* ---------------------------------------------------------------------------------------------
* timing
* ---------------------------------------------------------------------------------------------
capture program drop c2_time
program c2_time, rclass
    * best of <reps> runs of <cmd>, each on a fresh copy of frame <fr>; <prep> runs untimed
    args fr reps cmd prep
    local best .
    forvalues r = 1/`reps' {
        capture frame drop work
        frame copy `fr' work
        frame work {
            quietly `prep'
            timer clear 99
            timer on 99
            quietly `cmd'
            timer off 99
        }
        quietly timer list 99
        local best = min(`best', r(t99))
        frame drop work
    }
    return scalar t = `best'
end

capture frame drop res
frame create res str4 id str80 desc str8 kind double(tc t2 tf tn)
capture program drop c2_scen
program c2_scen
    * <cmd> names the command center#: center runs it with # removed, center2 with # -> 2,
    * unless <cmd2> gives center2's command. kind gen: floor = add floor of 5 <otype>
    * variables; kind inplace: floor = t_io.
    args id fr kind otype cmd cmd2
    if `"`cmd2'"' == "" {
        local cmd2 = subinstr(`"`cmd'"', "center#", "center2", 1)
    }
    local cmd = subinstr(`"`cmd'"', "center#", "center", 1)
    c2_time `fr' 1 `"`cmd'"'
    local tc = r(t)
    c2_time `fr' 3 `"`cmd2'"'
    local t2 = r(t)
    local tn .
    if "`kind'" == "gen" {
        c2_time `fr' 3 `"mata: (void) st_addvar("`otype'", "f":+strofreal(1..5), 1)"'
        local tf = r(t)
        local sep = cond(strpos(`"`cmd2'"', ","), " ", ", ")
        c2_time `fr' 3 `"`cmd2'`sep'replace"' `"mata: (void) st_addvar("`otype'", "c_x":+strofreal(1..5), 1)"'
        local tn = r(t)
    }
    else {
        c2_time `fr' 3 `"plugin call _c2p x1-x5 s1-s5, selftest "k=5" "sign=1""' ///
            `"mata: (void) st_addvar("double", "s":+strofreal(1..5), 1)"'
        local tf = r(t)
    }
    local desc = cond(`"`cmd2'"' == subinstr(`"`cmd'"', "center", "center2", 1), ///
        `"`cmd'"', `"`cmd' / `cmd2'"')
    frame post res ("`id'") (`"`fr': `desc'"') ("`kind'") (`tc') (`t2') (`tf') (`tn')
    di as txt `"`id' `fr' `desc': center `tc' s, center2 `t2' s, floor `tf' s, without adds `tn' s"'
end

* ---------------------------------------------------------------------------------------------
* scenarios
* ---------------------------------------------------------------------------------------------
c2_scen A1 D1 gen float "center# x1-x5"
c2_scen A2 D1 gen float "center# x1-x5, standardize"
c2_scen A3 D1 gen float "by g: center# x1-x5"
c2_scen A4 D1 gen float "by g: center# x1-x5, standardize"
c2_scen A5 D1 gen float "center# x1-x5 [aw=w] if u<.5, standardize"
c2_scen A6 D1 inplace . "center# x1-x5, inplace"
c2_scen A7 D1 inplace . "center# x1-x5, inplace standardize"
c2_scen A8 D1 inplace . "by g: center# x1-x5, inplace"
c2_scen A9 D1u gen float "bysort g: center x1-x5, standardize" "center2 x1-x5, by(g) standardize"
c2_scen A10 D1 gen double "center# x1-x5, double"
c2_scen B1 D2 gen float "center# x1-x5"
c2_scen B2 D2 gen float "center# x1-x5, standardize"
c2_scen B3 D2 gen float "by g: center# x1-x5"
c2_scen B4 D2 inplace . "center# x1-x5, inplace standardize"

* ---------------------------------------------------------------------------------------------
* gates and RESULTS.md
* ---------------------------------------------------------------------------------------------
frame change res
gen double speedup = tc / t2
* at floor: generate when center2 without adds <= 0.4 s; inplace within 1.6 t_io (two reads, one write)
gen byte atfloor = cond(kind == "gen", tn <= .4, t2 <= 1.6*tf)

capture program drop c2_gate
program c2_gate
    * c2_gate <gate> <pass 0|1> <floor-bound 0|1> <value> <description> <floor note>
    args gate pass floor value text fnote
    local note
    if !`pass' & `floor' {
        local note " (floor-bound: `fnote')"
    }
    local line "BENCH_GATE `gate' `=cond(`pass', "PASS", "FAIL")' `value'`note' -- `text'"
    di as result "`line'"
    global C2_GATES `"$C2_GATES `"`line'"'"'
end

capture program drop c2_fnote
program c2_fnote
    * c_local fnote = the at-floor numbers of row <r>
    args r
    if kind[`r'] == "gen" {
        c_local fnote "center2 without adds `: display %5.3f tn[`r']' s"
    }
    else {
        c_local fnote "center2 `: display %5.3f t2[`r']' s, t_io `: display %5.3f tf[`r']' s"
    }
end

capture program drop c2_row
program c2_row
    * c_local r = the row of res with id <id>
    args id
    forvalues i = 1/`=_N' {
        if id[`i'] == "`id'" {
            c_local r `i'
        }
    }
end

global C2_GATES
foreach row in "G1 A7 100" "G2 A6 30" "G3 A1 15" "G4 A3 5" "G5 A9 5" {
    tokenize `row'
    c2_row `2'
    c2_fnote `r'
    local v : display %6.1f speedup[`r']
    c2_gate `1' `=speedup[`r'] >= `3'' `=atfloor[`r']' "`=strtrim("`v'")'x" "`2' speedup >= `3'x" "`fnote'"
}
* G6: every generate scenario except A9 within 0.4 s of its add floor
local worst = -1
local worstn = -1
local allfloor 1
forvalues i = 1/`=_N' {
    if kind[`i'] == "gen" & id[`i'] != "A9" {
        if t2[`i'] - tf[`i'] > `worst' {
            local worst = t2[`i'] - tf[`i']
            local r = `i'
        }
        if tn[`i'] > `worstn' {
            local worstn = tn[`i']
            local rn = `i'
        }
        local allfloor = `allfloor' & atfloor[`i']
    }
}
local v : display %5.3f `worst'
c2_gate G6 `=`worst' <= .4' `allfloor' "`=strtrim("`v'")' s" ///
    "max(center2 - add floor) over generate scenarios except A9 (`=id[`r']') <= 0.4 s" ///
    "center2 without adds <= `: display %5.3f `worstn'' s (`=id[`rn']') in all of them"

* RESULTS.md (Stata strings cannot hold backquotes: _char(96))
local fence = 3*"_char(96) "
tempname fh
file open `fh' using "RESULTS.md", write text replace
file write `fh' "# center2 benchmark results" _n _n
file write `fh' "Generated by bench/bench.do on `c(current_date)' `c(current_time)': Stata `c(stata_version)' " ///
    "`c(edition_real)', `c(os)', `c(processors)' processors (`c(processors_mach)' on the machine)." _n _n
file write `fh' "Data: D1 = 20M obs (long g with 100,000 groups, double x1-x5, u, w) sorted by g; " ///
    "D1u = D1 sorted by u; D2 = 5M obs, D1's variables plus 50 float z variables, sorted by g." _n _n
file write `fh' "center runs once, center2 best of 3, each on a fresh frame copy (not timed). " ///
    "Floor: for generate scenarios, one nofill st_addvar of the same 5 output variables; " ///
    "for inplace scenarios, t_io = one parallel read of x1-x5 plus one parallel write of 5 doubles. " ///
    "center2 without adds (generate scenarios): the same command plus replace, with the 5 targets " ///
    "already present with the output type, so center2 overwrites them in place: its work beyond " ///
    "creating variables. The add floor varies by up to ~1 s between identical runs (page faults " ///
    "of Stata's dataset relayout), so center2 minus the add floor is noisy at the 0.4 s scale." _n _n
file write `fh' "| id | scenario | center s | center2 s | speedup | floor s | center2 without adds s |" _n
file write `fh' "|---|---|---:|---:|---:|---:|---:|" _n
forvalues i = 1/`=_N' {
    file write `fh' "| `=id[`i']' | `=desc[`i']' | " %7.3f (tc[`i']) " | " %7.3f (t2[`i']) ///
        " | " %6.1f (speedup[`i']) "x | " %6.3f (tf[`i']) " | " ///
        `"`=cond(missing(tn[`i']), "", strtrim(string(tn[`i'], "%6.3f")))'"' " |" _n
}
file write `fh' _n "Gates (a failing gate is floor-bound when center2 without adds is <= 0.4 s for generate " ///
    "scenarios, or center2 is within 1.6 t_io for inplace scenarios):" _n _n `fence' _n
foreach line of global C2_GATES {
    file write `fh' `"`line'"' _n
}
file write `fh' `fence' _n
file close `fh'
list id desc tc t2 speedup tf tn atfloor, noobs abbreviate(12) string(60)
di as result "BENCH_DONE"
