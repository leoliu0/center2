* test_semantics.do -- center2 behavior beyond plain equivalence with center: by() on unsorted
* data, determinism across thread counts, reuse of existing targets under force, aliasing,
* sort markers, in with replace (in place), messages, the help examples, more than 32 variables
* (several plugin batches) and empty samples. Specs passed to both commands are in center's
* syntax; c2_tospec2 renames the options for center2 (replace -> force, inplace -> replace).
version 16
clear all
set more off
adopath ++ "../../stata"

c2_testdata

* ---------------------------------------------------------------------------------------------
* by() on unsorted data equals bysort ...: center, aligned on id; order and sortedby unchanged
* ---------------------------------------------------------------------------------------------
sort u
gen long ord0 = _n
* the observations that in 1001/9000 selects in this order
gen byte inr = inrange(_n, 1001, 9000)
frame copy default base

capture program drop c2_byeq
program c2_byeq
    * spec: center's syntax for both sides; aspec: the center side if different
    * (bysort keys: center aspec)
    args keys spec aspec
    if `"`aspec'"' == "" {
        local aspec `"`spec'"'
    }
    c2_tospec2 `spec'
    capture frame drop fa
    capture frame drop fb
    frame copy base fa
    frame copy base fb
    frame fa: quietly bysort `keys': center `aspec' double
    frame fb {
        quietly center2 `spec2' by(`keys') double
        assert ord0 == _n
        if "`: sortedby'" != "u" {
            di as error "by(`keys') `spec': sortedby changed to `: sortedby'"
            exit 9
        }
        sort id
    }
    frame fa: sort id
    c2_cmpframes fa fb, tol(1e-12) nosortcheck what(`"center2 `spec' by(`keys')"')
end

foreach keys in g s sl "g s" gbig {
    c2_byeq "`keys'" "x1 x2 xf xb,"
    c2_byeq "`keys'" "x1 x2 [aw=w], standardize meansave sdsave"
    c2_byeq "`keys'" "x1 x2 [iw=iw], standardize"
    c2_byeq "`keys'" "x1 x2 xf xb xi, inplace standardize"
    c2_byeq "`keys'" "x1 x2 if u<.6,"
    di as txt "by(`keys') ok"
}
* by() with in (not possible with the by prefix): equals bysort ...: center if <same obs>
c2_byeq g "x1 x2 in 1001/9000," "x1 x2 if inr,"
c2_byeq s "x1 x2 xf in 1001/9000, inplace" "x1 x2 xf if inr, inplace"
di as txt "by() with in ok"

* ---------------------------------------------------------------------------------------------
* threads(1) and threads(0) give bitwise-identical data (240,000 obs: 30 blocks)
* ---------------------------------------------------------------------------------------------
frame copy base big
frame big {
    expand 8
    sort gbig id
}
capture program drop c2_det
program c2_det
    args cmd
    capture frame drop fa
    capture frame drop fb
    frame copy big fa
    frame copy big fb
    frame fa: quietly `cmd' threads(1)
    frame fb: quietly `cmd' threads(0)
    c2_cmpframes fa fb, tol(0) what(`"`cmd' threads(1) vs threads(0)"')
end
c2_det "by gbig: center2 x1 x2 [aw=w], standardize meansave sdsave double"
c2_det "center2 x1 x2, by(s) standardize double"
c2_det "center2 x1 x2 [iw=iw], standardize double"
c2_det "center2 x1 x2 xf, replace standardize"
di as txt "determinism ok"

* ---------------------------------------------------------------------------------------------
* force: a float target is reused in place (keeps its position, looks new); a double target
* with float output, or a sort variable, is recreated at the end
* ---------------------------------------------------------------------------------------------
frame copy base r
frame r {
    quietly center2 x1, prefix(f_)
    gen float c_x1 = 99
    gen byte after = 1
    format c_x1 %5.2f
    label define yn 0 no 1 yes
    label values c_x1 yn
    note c_x1: a note
    char c_x1[foo] bar
    label variable c_x1 "old"
    quietly ds
    local vars `r(varlist)'
    local pos0 : list posof "c_x1" in vars
    center2 x1, force
    quietly ds
    local vars `r(varlist)'
    assert `: list posof "c_x1" in vars' == `pos0'
    assert c_x1 == f_x1
    assert "`: format c_x1'" == "%9.0g"
    assert "`: value label c_x1'" == ""
    assert `"`: char c_x1[]'"' == ""
    assert `"`: variable label c_x1'"' == "first x (centered)"
    center2 x1, force nolabel
    assert `"`: variable label c_x1'"' == ""

    drop c_x1
    gen double c_x1 = 1
    order after, last
    center2 x1, force
    quietly ds
    assert word("`r(varlist)'", -1) == "c_x1"
    assert "`: type c_x1'" == "float" & c_x1 == f_x1

    drop c_x1
    gen float c_x1 = -_n
    sort c_x1
    order after, last
    center2 x1, force
    quietly ds
    assert word("`r(varlist)'", -1) == "c_x1"
    assert "`: sortedby'" == "" & c_x1 == f_x1
}
di as txt "force ok"

* ---------------------------------------------------------------------------------------------
* aliasing: a variable may be a source and a reused target; values come from pre-command data
* ---------------------------------------------------------------------------------------------
frame copy base al
frame al {
    gen float c_x1 = x2
    gen double old = c_x1
    quietly center2 x1 old, prefix(e_) standardize
    center2 x1 c_x1, force standardize
    assert c_x1 == e_x1
    assert c_c_x1 == float(e_old)
}
di as txt "aliasing ok"

* ---------------------------------------------------------------------------------------------
* sort markers: an in-place change of a sort variable clears sortedby (native replace); other
* writes keep it
* ---------------------------------------------------------------------------------------------
frame copy base so
frame so {
    sort x1
    center2 x1, replace
    assert "`: sortedby'" == ""
    sort g
    by g: center2 x2
    assert "`: sortedby'" == "g"
    center2 x2 xf, by(gbig) replace
    assert "`: sortedby'" == "g"
}
di as txt "sort markers ok"

* ---------------------------------------------------------------------------------------------
* replace (in place) with in: only obs 1-100 change, using the mean of obs 1-100 (center gets
* this wrong)
* ---------------------------------------------------------------------------------------------
frame copy base ip
frame ip {
    sort id
    summarize x1 in 1/100, meanonly
    gen double e = cond(_n <= 100, x1 - r(mean), x1)
    center2 x1 in 1/100, replace
    assert x1 == e | (_n <= 100 & reldif(x1, e) < 1e-12)
}
di as txt "replace with in ok"

* ---------------------------------------------------------------------------------------------
* messages equal center's
* ---------------------------------------------------------------------------------------------
capture program drop c2_msg
program c2_msg
    args cmd
    capture frame drop fm
    frame copy base fm
    tempfile f
    frame fm {
        quietly log using "`f'", text name(c2m)
        `cmd'
        quietly log close c2m
    }
    * the note lines: "(modified variables: ...)", "(generated variables: ...)"
    mata: l = cat(st_local("f")); l = select(l, substr(l, 1, 1) :== "(" :& strpos(l, " variables: ") :> 0)
    mata: st_global("C2_MSG", rows(l) ? invtokens(l', "|") : "")
end
foreach spec in "x1 x2 xf, meansave sdsave standardize" "x1 xb, inplace meansave" ///
    "x1, generate(z) meansave(mz)" {
    c2_msg "center `spec'"
    local ma "$C2_MSG"
    c2_tospec2 `spec'
    c2_msg "center2 `spec2'"
    if `"`ma'"' != "$C2_MSG" | `"`ma'"' == "" {
        di as error `"messages differ: `ma' vs $C2_MSG"'
        exit 9
    }
}
di as txt "messages ok"

* ---------------------------------------------------------------------------------------------
* help examples (in double storage so that center's mean is not rounded to float)
* ---------------------------------------------------------------------------------------------
set type double
capture frame drop ha
capture frame drop hb
frame create ha
frame ha: sysuse auto
frame copy ha hb
local i 0
foreach ex in "center# mpg price weight" "center# mpg price weight, prefix(z_) standardize" ///
    "bysort rep78: center# mpg price weight, replace" "center# mpg, generate(mpg0)" {
    frame ha: `=subinstr("`ex'", "#", "", 1)'
    c2_tospec2 `=subinstr("`ex'", "#", "2", 1)'
    frame hb: `spec2'
    * from the replace (center2: force) example on, center2 keeps reused variables in place
    local order = cond(`++i' >= 3, "noordercheck", "")
    * bysort orders ties differently in each frame; align on make (unique)
    frame ha: sort make
    frame hb: sort make
    c2_cmpframes ha hb, tol(1e-12) `order' what(`"`ex'"')
}
set type float
di as txt "help examples ok"

* ---------------------------------------------------------------------------------------------
* more than 32 variables: several plugin batches; nothing is written in place, so even
* force and replace give center's variable order
* ---------------------------------------------------------------------------------------------
capture frame drop wide
frame create wide
frame wide {
    set obs 20000
    gen long id = _n
    gen int g = ceil(runiform()*50)
    forvalues j = 1/40 {
        gen double v`j' = rnormal()*`j' + `j'
    }
    forvalues j = 1/40 {
        gen float c_v`j' = 0
    }
    sort g id
}
foreach spec in "v1-v40, standardize double meansave replace" "v1-v40, inplace standardize double" {
    foreach byp in "" "by g:" {
        capture frame drop fa
        capture frame drop fb
        frame copy wide fa
        frame copy wide fb
        frame fa: quietly `byp' center `spec'
        c2_tospec2 `spec'
        frame fb: quietly `byp' center2 `spec2'
        c2_cmpframes fa fb, tol(1e-12) what(`"`byp' center `spec'"')
    }
}
di as txt "more than 32 variables ok"

* ---------------------------------------------------------------------------------------------
* empty samples
* ---------------------------------------------------------------------------------------------
foreach cmd in "center# x1 x2 if u > 2, standardize meansave" "by g: center# x1 if u > 2" {
    capture frame drop fa
    capture frame drop fb
    frame copy base fa
    frame copy base fb
    frame fa {
        sort g id
        quietly `=subinstr("`cmd'", "#", "", 1)'
    }
    frame fb {
        sort g id
        quietly `=subinstr("`cmd'", "#", "2", 1)'
    }
    c2_cmpframes fa fb, tol(0) what(`"`cmd'"')
}
* no observations at all (center fails here with r(111))
capture frame drop fe
frame create fe
frame fe {
    gen x = .
    center2 x, standardize meansave sdsave
    confirm float variable c_x m_x sd_x
}
di as txt "empty samples ok"

di as result "TEST_SEMANTICS passed"
