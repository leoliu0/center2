*! version 0.1.0, Leo Liu, 15oct2026
*! center2: fast drop-in replacement for -center- (Ben Jann, SSC), computed by a compiled plugin.
*! Syntax, options, results and messages follow center 1.10; the plugin protocol is
*! documented in src/spec.rs of the source repository.
program center2, byable(onecall)
    version 16
    local c2_version 0.1.0
    syntax varlist(numeric) [if] [in] [aw iw pw fw/] [, PREfix(string) /*
        */ noLabel Addtolabel(string asis) Replace Double Standardize /*
        */ MEANsave MEANsave2(string) SDsave SDsave2(string) Casewise /*
        */ THeta(string) Generate(string) Inplace BY(varlist) THReads(integer 0) ]

    /* plugin */
    local c2_plugver
    capture plugin call _center2_plugin, version
    if _rc == 199 {
        di as error "center2: could not load center2.plugin; reinstall center2"
        exit 601
    }
    else if _rc {
        exit _rc
    }
    if "`c2_plugver'" != "`c2_version'" {
        di as error "center2: plugin version `c2_plugver' does not match ado version `c2_version'; reinstall center2"
        exit 198
    }

    /* inplace, generate, prefix */
    if "`inplace'"!="" {
        if `"`prefix'"'!="" {
            di as error "prefix() not allowed with inplace"
            exit 198
        }
        if `"`generate'"'!="" {
            di as error "generate() not allowed with inplace"
            exit 198
        }
        local prefix
    }
    else if `"`generate'"'!="" {
        if `"`prefix'"'!="" {
            di as error "prefix() not allowed if generate() is specified"
            exit 198
        }
        local nvars: word count `varlist'
        if `nvars'>1 {
            di as error "too many variables specified"
            exit 103
        }
        confirm name `generate'
        local prefix `"`generate'"'
    }
    else if `"`prefix'"'!="" {
        confirm name `prefix'
    }
    else {
        local prefix "c_"
    }

    /* theta option */
    if `"`theta'"'!="" {
        capture confirm number `theta'
        if _rc {
            capture confirm numeric variable `theta'
            if _rc {
                di as error "invalid specification of theta()"
                exit 198
            }
        }
    }
    else {
        local theta 1
    }

    /* meansave and sdsave */
    if "`meansave'"!="" {
        local meansave "m_`generate'"
    }
    if "`meansave2'"!="" {
        local meansave "`meansave2'"
    }
    if "`meansave'"!="" {
        confirm name `meansave'
    }
    if "`sdsave'"!="" {
        local sdsave "sd_`generate'"
    }
    if "`sdsave2'"!="" {
        local sdsave "`sdsave2'"
    }
    if "`sdsave'"!="" {
        if "`standardize'"=="" {
            di as error "sdsave not allowed unless standardize is specified"
            exit 198
         }
         confirm name `sdsave'
    }

    /* by() and threads() */
    if "`by'"!="" & _by() {
        di as error "by() may not be combined with the by prefix"
        exit 198
    }
    if `threads' < 0 {
        di as error "threads() must be 0 or a positive integer"
        exit 198
    }

    /* targets: T_<role><k> name, H_<role><k> new|reuse|recreate (role out, mean, sd).
       An existing target (replace) is reused in place when it already has the output type
       and is not a sort variable; otherwise a new variable replaces it at the end. With more
       than 32 sources the plugin runs several batches, so nothing is written in place:
       a later batch could otherwise read a variable an earlier batch overwrote. */
    local otype = cond("`double'"!="", "double", c(type))
    local sorted : sortedby
    local nsrc : word count `varlist'
    local onebatch = `nsrc' <= 32
    local roles = cond("`inplace'"=="", "out", "") + cond("`meansave'"!="", " mean", "") /*
        */ + cond("`sdsave'"!="", " sd", "")
    local alltargets
    local k 0
    foreach name of local varlist {
        local ++k
        if "`generate'"=="" {
            local nname `name'
        }
        local lab`k' : var label `name'
        if `"`lab`k''"'=="" {
            local lab`k' `name'
        }
        foreach role of local roles {
            local pre = cond("`role'"=="out", "prefix", cond("`role'"=="mean", "meansave", "sdsave"))
            local t ``pre''`nname'
            local how new
            capture confirm new variable `t'
            if _rc {
                capture confirm variable `t', exact
                if _rc | "`replace'"=="" {
                    di as error "`t' already defined or invalid name"
                    exit 110
                }
                local how recreate
                if "`: type `t''"=="`otype'" & !`: list t in sorted' & `onebatch' {
                    local how reuse
                }
            }
            /* a repeated target, or (inplace) a target that is also modified as a source */
            local insrc 0
            if "`inplace'"!="" {
                local insrc : list t in varlist
            }
            if `: list t in alltargets' | `insrc' {
                di as error "`t' already defined or invalid name"
                exit 110
            }
            local alltargets `alltargets' `t'
            local T_`role'`k' `t'
            local H_`role'`k' `how'
        }
    }

    /* addtolabel option */
    if "`label'"=="" {
        if `"`addtolabel'"'==`""""' {
            local addtolabel
        }
        else if `"`addtolabel'"'==`""' {
            if "`standardize'"!="" {
                local addtolabel " (standardized)"
            }
            else {
                local addtolabel " (centered)"
            }
        }
        else {
            _center2_stripq, addtolabel(`addtolabel')
        }
        local llength=80-length(`"`addtolabel'"')
    }

    /* mark sample: without if, in, weights and casewise every observation is used */
    local touse
    if `"`if'`in'`weight'`casewise'"'!="" {
        if "`casewise'"=="" {
            marksample touse, novarlist
        }
        else {
            marksample touse
        }
    }

    /* weights: a numeric variable is passed as is, an expression through a double tempvar */
    local wtype none
    local wvar
    if "`weight'"!="" {
        local wtype = substr("`weight'",1,1) + "w"
        capture unab wvar : `exp', max(1)
        if !_rc {
            capture confirm numeric variable `wvar', exact
        }
        if _rc {
            tempvar wvar
            quietly gen double `wvar' = (`exp') if `touse'
        }
    }

    /* theta */
    local thetavar
    capture confirm number `theta'
    if !_rc {
        local thstr = strtrim(string(`theta', "%21.17g"))
        local thetaarg theta=`thstr'
    }
    else {
        capture unab thetavar : `theta', max(1)
        if _rc {
            di as error "invalid specification of theta()"
            exit 198
        }
        local thetaarg thetavar=1
    }

    /* groups: by prefix (data sorted; string keys replaced by a 0/1 parity marker), by(), or none */
    local keys
    local keymode none
    if _by() {
        local keymode sorted
        local keys `_byvars'
        foreach v of local _byvars {
            capture confirm string variable `v'
            if !_rc {
                tempvar par
                quietly by `_byvars': gen byte `par' = _n==1
                quietly replace `par' = mod(sum(`par'),2)
                local keys `par'
                continue, break
            }
        }
    }
    else if "`by'"!="" {
        local keymode hash
        local keys `by'
    }
    local nkey : word count `keys'

    /* inplace: observations written with missing outside the sample (ifin) */
    local ifin all
    local ifinvar
    if "`inplace'"!="" & "`touse'"!="" {
        if "`weight'`casewise'"=="" {
            local ifin touse
        }
        else if `"`if'`in'"'!="" {
            tempvar ifinvar
            mark `ifinvar' `if' `in'
            local ifin var
        }
    }

    /* outputs: P_<role><k> is the variable the plugin writes, created in one nofill st_addvar
       call (per source: mean, sd, out, as center orders them). New targets get their final
       names: renaming a variable after adding double variables costs a dataset pass (0.3 s on
       20M obs). Recreated targets are tempvars, renamed after the old variable is dropped, so
       the old one stays readable as an input. An inplace source is written directly when it is
       float or double, not a sort variable, and the run is one batch; otherwise through a double
       tempvar and a native replace afterwards (type promotion and sort marker as with center). */
    local addtypes
    local addnames
    local newnames
    local fill
    local slow
    local k 0
    foreach name of local varlist {
        local ++k
        foreach role in mean sd out {
            if "`T_`role'`k''"=="" {
                continue
            }
            if "`H_`role'`k''"=="reuse" {
                local P_`role'`k' `T_`role'`k''
                continue
            }
            if "`H_`role'`k''"=="new" {
                local P_`role'`k' `T_`role'`k''
                local newnames `newnames' `T_`role'`k''
            }
            else {
                tempvar tmp
                local P_`role'`k' `tmp'
            }
            local addtypes `addtypes' `otype'
            local addnames `addnames' `P_`role'`k''
        }
        if "`inplace'"=="" {
            local fill `fill'a
        }
        else if inlist("`: type `name''", "float", "double") & !`: list name in sorted' & `onebatch' {
            local P_out`k' `name'
            local fill `fill'i
        }
        else {
            tempvar tmp
            local P_out`k' `tmp'
            local addtypes `addtypes' double
            local addnames `addnames' `tmp'
            local fill `fill'a
            local slow `slow' `k'
        }
    }
    local outs
    local means
    local sds
    forvalues k = 1/`nsrc' {
        local outs `outs' `P_out`k''
        local means `means' `P_mean`k''
        local sds `sds' `P_sd`k''
    }
    local hastouse = "`touse'"!=""
    local std = "`standardize'"!=""
    local hasmean = "`meansave'"!=""
    local hassd = "`sdsave'"!=""
    local ifcond
    if "`ifin'"=="touse" {
        local ifcond if `touse'
    }
    else if "`ifin'"=="var" {
        local ifcond if `ifinvar'
    }
    local newvars
    local modvars
    local written = "`inplace'"!="" & `nsrc' > `: word count `slow''
    local fmt = cond("`otype'"=="double", "%10.0g", "%9.0g")

    /* From adding the outputs to labeling them, Break is deferred (the values of nofill
       variables are undefined until the plugin has written them); on a plugin error the new
       variables are dropped. */
    local rc 0
    nobreak {
        if "`addnames'"!="" {
            mata: (void) st_addvar(tokens("`addtypes'"), tokens("`addnames'"), 1)
        }
        capture noisily plugin call _center2_plugin `keys' `touse' `ifinvar' `wvar' `thetavar' /*
            */ `varlist' `outs' `means' `sds', center "k=`nsrc'" "nkey=`nkey'" /*
            */ "keymode=`keymode'" "touse=`hastouse'" "ifin=`ifin'" "wtype=`wtype'" /*
            */ "`thetaarg'" "std=`std'" "fill=`fill'" "mean=`hasmean'" "sd=`hassd'" /*
            */ "threads=`threads'"
        local rc = _rc
        if `rc' & "`newnames'"!="" {
            drop `newnames'
        }
        if !`rc' {
            /* slow inplace sources */
            local k 0
            foreach name of local varlist {
                local ++k
                if `: list k in slow' {
                    quietly replace `name' = `P_out`k'' `ifcond'
                }
            }
            /* targets and labels */
            local k 0
            foreach name of local varlist {
                local ++k
                if "`inplace'"!="" {
                    local modvars `modvars' `name'
                }
                foreach role of local roles {
                    local t `T_`role'`k''
                    if "`H_`role'`k''"=="reuse" {
                        /* look like a new variable */
                        local written 1
                        format `t' `fmt'
                        label values `t'
                        local chars : char `t'[]
                        foreach c of local chars {
                            char `t'[`c']
                        }
                        if "`label'"!="" {
                            label variable `t' ""
                        }
                    }
                    else if "`H_`role'`k''"=="recreate" {
                        drop `t'
                        rename `P_`role'`k'' `t'
                    }
                    local newvars `newvars' `t'
                }
                if "`label'"=="" {
                    local lab=substr(`"`lab`k''"',1,`llength')
                    if "`inplace'"!="" {
                        lab var `name' `"`lab'`addtolabel'"'
                    }
                    else {
                        lab var `T_out`k'' `"`lab'`addtolabel'"'
                    }
                    if "`meansave'"!="" {
                        lab var `T_mean`k'' `"mean(s) of `name'"'
                    }
                    if "`sdsave'"!="" {
                        lab var `T_sd`k'' `"std. deviation(s) of `name'"'
                    }
                }
            }
            if `written' {
                mata: st_updata(1)
            }
        }
    }
    if `rc' {
        exit `rc'
    }

    /* output note */
    if `"`modvars'"'!="" {
        di as txt `"(modified variables: `modvars')"'
    }
    if `"`newvars'"'!="" {
        di as txt `"(generated variables: `newvars')"'
    }
end

program _center2_stripq
    syntax [ , addtolabel(string) ]
    c_local addtolabel `"`addtolabel'"'
end

capture program _center2_plugin, plugin using("center2.plugin")
