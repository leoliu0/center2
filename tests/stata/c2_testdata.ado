*! c2_testdata: builds the center2 test dataset (30,000 obs, sorted by id) in the current frame.
*! Keys: id, g (40 groups), gbig (3 groups, so groups cross the plugin's 8192-row blocks),
*! str5 s (25 groups) and its strL copy sl. Values: x1 (with .a), x2 (~10% missing), float xf,
*! byte xb, int xi, long xl. Weights: w (aw/pw), int fw (1-4), iw (some negative). th, u.
program c2_testdata
    version 16
    clear
    set seed 20261015
    quietly {
        set obs 30000
        gen long id = _n
        gen int g = ceil(runiform()*40)
        gen byte gbig = ceil(runiform()*3)
        gen str5 s = "s" + string(ceil(runiform()*25))
        gen strL sl = s
        gen double x1 = rnormal()*10 + 5
        replace x1 = .a if runiform() < .03
        gen double x2 = rnormal()*3 - 2 if runiform() > .1
        gen float xf = rnormal()*4 + 1
        gen byte xb = max(min(round(rnormal()*20), 100), -100)
        gen int xi = round(rnormal()*50)
        gen long xl = round(rnormal()*100)
        gen double w = runiform() + .5
        gen int fw = ceil(runiform()*4)
        gen double iw = runiform() - .2
        gen double th = runiform()
        gen double u = runiform()
    }
    label variable x1 "first x"
    label variable x2 "a long variable label, longer than sixty-five characters, to test truncation"
    label variable xb "byte x"
    sort id
end
