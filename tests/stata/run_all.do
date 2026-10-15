* run_all.do -- center2 Stata test suite (needs stata/center2.plugin, built by
* scripts/build_plugin.sh, and SSC center installed). From tests/stata:
*   stata-mp -b do run_all.do      -> last line of run_all.log: ALL TESTS PASSED
* (stress_store.do is a separate, long-running probe of parallel plugin stores.)
version 16
clear all
set more off
local here `c(pwd)'
foreach t in test_errors test_equiv test_semantics {
    di as text _n "{hline 78}" _n "running `t'.do" _n "{hline 78}"
    quietly cd "`here'"
    do `t'.do
}
quietly cd "`here'"

* finish inside a program so that "ALL TESTS PASSED" is the last line of the batch log
capture program drop c2_finish
program c2_finish
    di as result "ALL TESTS PASSED"
    exit, STATA clear
end
c2_finish
