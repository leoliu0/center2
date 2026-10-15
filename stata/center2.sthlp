{smcl}
{* 15oct2026}{...}
{hline}
help for {hi:center2}
{hline}

{title:Center (or, optionally, standardize) variables, fast}

{p 8 15 2} {cmd:center2}
    {it:varlist}
    [{it:weight}]
    [{cmd:if} {it:exp}]
    [{cmd:in} {it:range}]
    [,
    {cmdab:c:asewise}
    {bind:{ {cmdab:r:eplace} | {cmdab:pre:fix(}{it:prefix}{cmd:)} | {cmdab:g:enerate(}{it:newvar}{cmd:)} }}
    {cmdab:th:eta:(}{it:#}|{it:varname}{cmd:)}
    {cmdab:s:tandardize}
    {opt force}
    {cmdab:d:ouble}
    {cmdab:nol:abel}
    {cmdab:a:ddtolabel(}{it:string}{cmd:)}
    {cmdab:mean:save}[{cmd:(}{it:prefix}|{it:newvar}{cmd:)}]
    {cmdab:sd:save}[{cmd:(}{it:prefix}|{it:newvar}{cmd:)}]
    {cmd:by(}{it:varlist}{cmd:)}
    {cmdab:thr:eads(}{it:#}{cmd:)}
    ]

{p 4 4 2}
{cmd:by} {it:...} {cmd::} may be used with {cmd:center2}; see help {help by}.

{p 4 4 2}
{cmd:aweight}s, {cmd:pweight}s, {cmd:iweight}s, and {cmd:fweight}s are allowed; see help {help weights}.


{title:Description}

{p 4 4 2} {cmd:center2} is a fast replacement for {cmd:center} by Ben Jann
(available from SSC). It gives the same results and messages, and computes in
a compiled plugin that reads and writes the data in parallel. It does not
sort the data.

{p 4 4 2} Its options are {cmd:center}'s, except two names, which follow
Stata's convention (as in {helpb destring} and {helpb tostring}):
{cmd:replace} changes the variables in place ({cmd:center}'s {cmd:inplace}),
and {cmd:force} permits overwriting existing variables ({cmd:center}'s
{cmd:replace}).

{p 4 4 2} {cmd:center2} centers (or, optionally, standardizes) the
variables in {it:varlist}. For each {it:varname} in {it:varlist}, a
new variable {cmd:c_}{it:varname} is created containing the centered (or
standardized) values of that variable.

{p 4 4 2} Use the {help by} prefix, or the {cmd:by()} option on unsorted data,
to center/standardize group-wise.

{p 4 4 2} Technical note about the computation of standard deviations in the
presence of weights: For {cmd:iweight}s and {cmd:fweight}s, the standard
deviations are computed as

            sd(x) = sqrt( sum(w_i*(x_i - mu)^2) / (W - 1) )

{p 4 4 2} where mu = sum(w_i*x_i)/W is the mean of x and W = sum(w_i) is the
sum of weights. For {cmd:aweight}s and {cmd:pweight}s, the standard deviations
are computed as

            sd(x) = sqrt( sum(w_i*(x_i - mu)^2) / (W - W/N) )

{p 4 4 2} where N is the sample size. These rules are in line with
how official Stata's {bf:{help summarize}} handles weights.


{title:Options}

{p 4 8 2} {cmd:addtolabel(}{it:string}{cmd:)} may be used to specify text which
is to be added to the end of the labels of the new variables. The default is
{bind:" (centered)"} or {bind:" (standardized)"}, depending on whether the
{cmd:standardize} option is specified. Type {cmd:addtolabel("")} to omit the suffix.

{p 4 8 2} {cmd:by(}{it:varlist}{cmd:)} centers/standardizes within groups of
{it:varlist} (numeric or string) without requiring the data to be sorted;
it gives the same results as {cmd:bysort} {it:varlist}{cmd:: center2}, but
neither sorts nor changes the sort order. {cmd:by()} may not be combined with
the {cmd:by} prefix. Unlike the prefix, it may be combined with {cmd:in}.

{p 4 8 2} {cmd:casewise} specifies that cases with missing values be excluded
listwise, i.e., that the centering/standardization be based on the sample that
is not missing for any of the variables in {it:varlist}. The default is to use
all the nonmissing values for each variable.

{p 4 8 2} {cmd:double} enforces storage type {cmd:double} for newly created
variables.

{p 4 8 2} {cmd:force} permits {cmd:center2} to overwrite existing variables.
An existing variable that already has the storage type of the new variable
(and is not a sort variable) is overwritten in place, so it keeps its position
in the dataset; its display format, value label, characteristics and notes are
reset as for a new variable. Otherwise it is dropped and created anew at the
end of the dataset, as {cmd:center} does. ({cmd:center} calls this option
{cmd:replace}.)

{p 4 8 2} {cmd:generate(}{it:newvar}{cmd:)} specifies the name of the
new variable to be created. Note that {cmd:center2} may only be applied to
one variable at a time if {cmd:generate()} is specified.

{p 4 8 2} {cmd:meansave}[{cmd:(}{it:prefix}{cmd:)}] saves
variables containing the means. The new variables will be named
{it:prefix}{it:varname}. The default {it:prefix} is {cmd:m_}. Note that
the saved means will not be affected by {cmd:theta()}. If
{cmd:generate()} is specified, {cmd:meansave(}{it:newvar}{cmd:)}
saves the mean(s) in {it:newvar}.

{p 4 8 2} {cmd:nolabel} suppresses the assignment of labels to the new
variables. The default is to use the label (or name) of the original variable
with some text added to the end of it (see {cmd:addtolabel}).

{p 4 8 2} {cmd:prefix(}{it:prefix}{cmd:)} allows the user to supply a prefix
for the centered/standardized variables. The default prefix is {cmd:c_}, thus
the new variables will be named {cmd:c_}{it:varname}. {cmd:prefix()}
is not allowed if {cmd:generate()} is specified.

{p 4 8 2} {cmd:replace} causes the variables to be centered in place. That is,
the centered values will replace the original values without creating
new variables. {cmd:prefix()} and {cmd:generate()} are not allowed with
{cmd:replace}. ({cmd:center} calls this option {cmd:inplace}.)

{p 4 8 2} {cmd:sdsave}[{cmd:(}{it:prefix}{cmd:)}] saves variables
containing the standard deviations (allowed only if the {cmd:standardize} option is
specified). The new variables will be named {it:prefix}{it:varname}. The
default {it:prefix} is {cmd:sd_}. If
{cmd:generate()} is specified, {cmd:sdsave(}{it:newvar}{cmd:)}
saves the standard deviation(s) in {it:newvar}.

{p 4 8 2} {cmd:standardize} creates a variable containing the standardized
values (zero sample mean and unit sample variance). Default is to create a
variable containing the centered values (zero sample mean).

{p 4 8 2} {cmd:theta(}{it:#}|{it:varname}{cmd:)} may be used for
quasi-demeaning. Before subtraction, the means will be scaled by
{it:#} or by the values of {it:varname} respectively. This is sometimes
used in the context of panel data models (see Wooldridge, 2002:287, and the
Methods and Formulas Section in {hi:[XT] xtreg}).

{p 4 8 2} {cmd:threads(}{it:#}{cmd:)} sets the number of threads the plugin
uses; the default, 0, uses all logical CPUs. Results are identical for any
number of threads. {cmd:threads(1)} runs fully serially.


{title:Differences from center}

{p 4 8 2} - Option names: {cmd:replace} is {cmd:center}'s {cmd:inplace}, and
{cmd:force} is {cmd:center}'s {cmd:replace}.

{p 4 8 2} - Means, standard deviations and results are computed in double
precision; {cmd:center} rounds the mean and standard deviation to the storage
type ({cmd:float} unless {cmd:double} is specified) before using them.
Results agree within float precision.

{p 4 8 2} - With {cmd:replace}, {cmd:if} and {cmd:in} are evaluated once,
before any variable is modified. ({cmd:center} applies {cmd:in} after sorting
the data, and re-evaluates {cmd:if} after earlier variables were modified.)

{p 4 8 2} - All input values are read before any variable is modified, so a
variable may be both a source and a target, e.g.
{cmd:center2 x c_x, force}.

{p 4 8 2} - Variables overwritten under {cmd:force} may keep their position
(see {cmd:force}).

{p 4 8 2} - Variables changed in place are written by the plugin, which does
not clear the sort order; this is why sort variables and integer variables
(which Stata's {help replace} command promotes to {cmd:float} or {cmd:double})
are changed through that command, as {cmd:center} does.


{title:Examples}

        {inp}. sysuse auto

        . center2 mpg price weight

        . center2 mpg price weight, prefix(z_) standardize

        . bysort rep78: center2 mpg price weight, force

        . center2 mpg price weight, by(rep78) force

        . center2 price, replace standardize

        . center2 mpg, generate(mpg0)
        {txt}


{title:References}

{p 4 8 2} Wooldridge, J.M. 2002. Econometric Analysis of Cross Section and
Panel Data. Cambridge (Mass.): The MIT Press.


{title:Author}

{p 4 4 2}
Leo Liu, auleo.liu@gmail.com

{p 4 4 2}
{cmd:center2} reimplements {cmd:center} by Ben Jann (Institute of Sociology,
University of Bern), whose syntax, options and help text it follows:

{p 8 8 2}
Jann, B. (2004). center: Stata module to center (or standardize)
variables. Available from
{browse "http://ideas.repec.org/c/boc/bocode/s444102.html"}.


{title:Also see}

{p 4 13 2}
Manual: {hi:[R] summarize}

{p 4 13 2}
On-line: help for {help summarize}, {help center} (if installed)
