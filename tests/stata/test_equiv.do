* test_equiv.do -- center2 gives the same data as center: values, missing codes, storage types,
* formats, labels, variable order and sort order. Every spec runs unprefixed and under
* by g:, by s:, by g s: and by gbig: (groups larger than the plugin's 8192-row blocks).
* Tolerance (reldif): 1e-12 with -double-. Without it, center rounds the group mean to float
* before subtracting it, an absolute error up to |mean|*2^-24; group means here reach ~60
* (xl under by g s:, 30-obs groups), hence 1e-5.
* No -in- with inplace: center applies it after its internal sort (see help center2).
* Specs are in center's syntax; c2_tospec2 renames the options for center2 (inplace -> replace).
version 16
clear all
set more off
adopath ++ "../../stata"

capture program drop c2_equiv
program c2_equiv
    args byp spec dbl
    capture frame drop fa
    capture frame drop fb
    frame copy base fa
    frame copy base fb
    frame fa: quietly `byp' center `spec' `dbl'
    c2_tospec2 `spec'
    frame fb: quietly `byp' center2 `spec2' `dbl'
    local tol = cond("`dbl'"!="", 1e-12, 1e-5)
    c2_cmpframes fa fb, tol(`tol') what(`"`byp' center `spec' `dbl'"')
end

local n 0
foreach s in "" "[aw=w]" "[fw=fw]" "[iw=iw]" "[pw=w]" {
    local spec`++n' `"x1 x2 xf xb `s', standardize"'
}
local spec`++n' `"x1 x2 xf xb,"'
local spec`++n' `"x1 x2 xf xb [aw=w*2],"'
local spec`++n' `"x1 x2 xf xb if u<.7, standardize meansave sdsave"'
local spec`++n' `"x1 x2 xf xb [fw=fw] if u<.7, meansave(mm_) sdsave(ss_) standardize"'
local spec`++n' `"x1 x2 xf xb, casewise standardize"'
local spec`++n' `"x1, generate(z) meansave(mz) sdsave(sz) standardize"'
local spec`++n' `"x1 x2 xf xb, theta(.5)"'
local spec`++n' `"x1 x2 xf xb, theta(th) standardize"'
local spec`++n' `"x1 x2 xf xb, prefix(k_) nolabel"'
local spec`++n' `"x1 x2 xf xb, addtolabel(" foo")"'
local spec`++n' `"x1 x2 xf xb, addtolabel("")"'
local spec`++n' `"x1 x2 xf xb xi xl, inplace"'
local spec`++n' `"x1 x2 xf xb xi xl, inplace standardize meansave"'
local spec`++n' `"x1 x2 xf xb xi xl if u<.5, inplace"'
local spec`++n' `"x1 x2 xf xb xi xl [aw=w] if u<.5, inplace standardize"'
local spec`++n' `"x1 x2 xf xb xi xl, inplace casewise"'

c2_testdata
frame copy default base
local runs 0
foreach byp in "" "by g:" "by s:" "by g s:" "by gbig:" {
    local keys = subinstr(subinstr("`byp'", "by ", "", 1), ":", "", 1)
    frame base: sort `keys' id
    forvalues i = 1/`n' {
        foreach dbl in "" double {
            c2_equiv `"`byp'"' `"`spec`i''"' `dbl'
            local ++runs
        }
    }
    di as txt "`byp' ok"
}
di as result "TEST_EQUIV passed `runs' comparisons"
