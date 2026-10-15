* test_errors.do -- center2 fails with center's return code on center's errors, leaves the data
* unchanged when it fails (also when the plugin fails after the outputs were added), and returns
* 198 on its own option errors.
version 16
clear all
set more off
adopath ++ "../../stata"

capture program drop c2_samerc
program c2_samerc
    * spec: center's syntax; c2_tospec2 renames the options for center2 (inplace -> replace)
    args spec
    capture frame drop fa
    capture frame drop fb
    frame copy base fa
    frame copy base fb
    frame fa: capture center `spec'
    local rca = _rc
    c2_tospec2 `spec'
    frame fb: capture center2 `spec2'
    local rcb = _rc
    if `rca' == 0 | `rca' != `rcb' {
        di as error `"center `spec': rc `rca', center2 `spec2': rc `rcb'"'
        exit 9
    }
    * center2 checks everything before it changes the data
    c2_cmpframes base fb, tol(0) what(`"center2 `spec2' (after error)"')
    di as txt `"r(`rcb'): `spec2'"'
end

capture program drop c2_rc198
program c2_rc198
    args spec
    capture frame drop fb
    frame copy base fb
    frame fb: capture `spec'
    if _rc != 198 {
        di as error `"`spec': rc `=_rc', expected 198"'
        exit 9
    }
    c2_cmpframes base fb, tol(0) what(`"`spec' (after error)"')
    di as txt `"r(198): `spec'"'
end

c2_testdata
gen float c_x1 = 1
* a binary strL (holds a 0 byte): the plugin cannot group by it
tempfile bin
tempname fh
file open `fh' using "`bin'", write binary replace
file write `fh' %1bu (65) %1bu (0) %1bu (66)
file close `fh'
gen strL bl = fileread("`bin'") in 1/10
sort g id
frame copy default base

c2_samerc `"x1, inplace prefix(k_)"'
c2_samerc `"x1, inplace generate(z)"'
c2_samerc `"x1, generate(z) prefix(k_)"'
c2_samerc `"x1 x2, generate(z)"'
c2_samerc `"x1, theta(foo)"'
c2_samerc `"x1, sdsave"'
c2_samerc `"x1"'
c2_samerc `"x2, prefix(1x)"'
c2_samerc `"x2 x2"'
c2_samerc `"x2 [aw=iw]"'
c2_samerc `"x2 [fw=w]"'
c2_samerc `"x2 [pw=iw], standardize"'
c2_samerc `"s"'

c2_rc198 `"by g: center2 x2, by(s)"'
c2_rc198 `"center2 x2, threads(-1)"'
* center accepts a varlist in theta() and then fails with r(111) while generating
c2_rc198 `"center2 x2, theta(x1 x2)"'
* plugin errors: the new outputs (incl. meansave) are dropped
c2_rc198 `"center2 x2 xf, by(bl) meansave"'
c2_rc198 `"center2 x2 xf, by(bl) replace"'

di as result "TEST_ERRORS passed"
