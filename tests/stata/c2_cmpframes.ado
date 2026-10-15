*! c2_cmpframes fa fb, tol(#) [what(text) nosortcheck noordercheck]
*! Fails (r(9)) unless frames fa and fb hold the same data:
*! - same N, same variables in the same order (unless noordercheck), same : sortedby (unless
*!   nosortcheck);
*! - per variable: same storage type, display format, variable label and value label;
*! - values: identical missing patterns and missing codes, identical strings, and
*!   reldif <= tol on nonmissing numbers. For float variables the tolerance is at least
*!   1e-6 (one float rounding apart) unless tol is 0, which demands bitwise-equal values.
program c2_cmpframes
    version 16
    syntax namelist(min=2 max=2), tol(real) [what(string) noSORTcheck noORDERcheck]
    gettoken fa fb : namelist
    local fb `fb'
    local cur = c(frame)
    foreach f in a b {
        frame change `f`f''
        quietly ds
        local v`f' `r(varlist)'
        local s`f' : sortedby
        local n`f' = _N
        foreach v of local v`f' {
            local t`f'_`v' : type `v'
            local f`f'_`v' : format `v'
            local l`f'_`v' : variable label `v'
            local vl`f'_`v' : value label `v'
        }
    }
    frame change `cur'
    local fail
    if `na' != `nb' {
        local fail N differs (`na' vs `nb')
    }
    else if "`ordercheck'"=="" & "`va'" != "`vb'" {
        local fail variables differ: (`va') vs (`vb')
    }
    else if "`ordercheck'"!="" & !`: list va === vb' {
        local fail variable sets differ: (`va') vs (`vb')
    }
    else if "`sortcheck'"=="" & "`sa'" != "`sb'" {
        local fail sortedby differs: (`sa') vs (`sb')
    }
    if "`fail'"!="" {
        di as error `"`what': `fail'"'
        exit 9
    }
    foreach v of local va {
        foreach p in t f vl {
            if "``p'a_`v''" != "``p'b_`v''" {
                di as error `"`what': `v': `p' differs (``p'a_`v'' vs ``p'b_`v'')"'
                exit 9
            }
        }
        if `"`la_`v''"' != `"`lb_`v''"' {
            di as error `"`what': `v': label differs (`la_`v'' vs `lb_`v'')"'
            exit 9
        }
        local vtol = `tol'
        if "`ta_`v''"=="float" & `tol' > 0 {
            local vtol = max(`tol', 1e-6)
        }
        mata: c2_cmpvals("`fa'", "`fb'", "`v'", `vtol', `"`what'"')
    }
end

mata:
void c2_cmpvals(string scalar fa, string scalar fb, string scalar v, real scalar tol,
    string scalar what)
{
    string scalar cur
    string colvector sa, sb
    real colvector a, b, ma, mb, d
    real scalar str, bad

    cur = st_framecurrent()
    st_framecurrent(fa)
    str = st_isstrvar(v)
    if (str) sa = st_sdata(., v)
    else a = st_data(., v)
    st_framecurrent(fb)
    if (str) sb = st_sdata(., v)
    else b = st_data(., v)
    st_framecurrent(cur)
    if (str) {
        if (sa != sb) {
            errprintf("%s: %s: strings differ\n", what, v)
            exit(9)
        }
        return
    }
    ma = a :>= .
    mb = b :>= .
    if (ma != mb) {
        errprintf("%s: %s: missing patterns differ in %g obs\n", what, v, sum(ma :!= mb))
        exit(9)
    }
    if (any(select(a, ma) :!= select(b, mb))) {
        errprintf("%s: %s: missing codes differ\n", what, v)
        exit(9)
    }
    if (sum(!ma) == 0) return
    d = reldif(select(a, !ma), select(b, !mb))
    if (tol == 0) bad = any(select(a, !ma) :!= select(b, !mb))
    else bad = max(d) > tol
    if (bad) {
        errprintf("%s: %s: max reldif %g > %g\n", what, v, max(d), tol)
        exit(9)
    }
}
end
