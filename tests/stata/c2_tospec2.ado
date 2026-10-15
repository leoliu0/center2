*! c2_tospec2 <center arguments>: c_local spec2 = the same arguments in center2's syntax, whose
*! option names follow Stata's convention: center's replace is force, center's inplace is replace.
program c2_tospec2
    version 16
    local s = subinword(`"`0'"', "replace", "force", .)
    local s = subinword(`"`s'"', "inplace", "replace", .)
    c_local spec2 `"`s'"'
end
