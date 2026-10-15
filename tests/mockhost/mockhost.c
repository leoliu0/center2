/*
    mockhost.c -- a fake Stata host for ABI tests of the center2 plugin on every OS.

    Implements the SPI 3.0 ST_plugin table over an in-memory 20,000-observation dataset, loads the
    plugin with dlopen/LoadLibrary and runs version, selftest and the center command in sorted,
    hash and no-key mode, in place with an ifin variable, and with a bad argument. Expected
    values come from a naive per-group reference in this file.

    Build/run (from the repository root):
        cc -O2 -DSYSTEM=OPUNIX -Ic tests/mockhost/mockhost.c -ldl -lm -o target/mockhost
        target/mockhost stata/center2.plugin
    Exit status: 0 when nothing FAILed.
*/

#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "stplugin.h"

#ifdef _WIN32
#include <windows.h>
#else
#include <dlfcn.h>
#endif

#define NOBS 20000
#define MISSVAL 8.98846567431158e307
/* Stata's .a: just above . */
#define MISS_A (MISSVAL * (1.0 + 1.0 / 4096.0))
#define MAXG 64
#define TOL 1e-12

/* ---------------------------------------------------------------- dataset */

enum {
    V_KEY,   /* numeric, sorted: group sizes cycle 1, 7, 300, 9000 */
    V_SKEY,  /* string, unsorted: 37 groups */
    V_TOUSE, /* 0 every 11th obs */
    V_IFIN,  /* touse, or every 3rd obs */
    V_W,     /* positive */
    V_IW,    /* some negative */
    V_TH,    /* theta variable, missing every 37th obs */
    V_X1,    /* with . and .a */
    V_X2,    /* with . */
    V_C1,    /* copies of x1, x2 changed in place */
    V_C2,
    V_O1, V_O2, V_M1, V_M2, V_S1, V_S2, /* outputs, prefilled with SENTINEL */
    NVAR
};

#define SENTINEL 12345.0

static double num[NVAR][NOBS];
static char skey[NOBS][8];

static double unif(uint64_t i, uint64_t k) {
    uint64_t z = i * 0x9e3779b97f4a7c15ULL + k * 0xbf58476d1ce4e5b9ULL;
    z = (z ^ (z >> 30)) * 0xbf58476d1ce4e5b9ULL;
    z = (z ^ (z >> 27)) * 0x94d049bb133111ebULL;
    z ^= z >> 31;
    return (double)(z >> 11) / 9007199254740992.0;
}

static void make_data(void) {
    static const int sizes[4] = {1, 7, 300, 9000};
    int g = 0, left = sizes[0];
    for (int i = 0; i < NOBS; i++) {
        if (left == 0) {
            g++;
            left = sizes[g % 4];
        }
        left--;
        num[V_KEY][i] = (double)(g + 1);
        snprintf(skey[i], sizeof skey[i], "k%02d", (int)((i * 7919u + i / 3u) % 37u));
        num[V_TOUSE][i] = i % 11 == 0 ? 0.0 : 1.0;
        num[V_IFIN][i] = (num[V_TOUSE][i] != 0.0 || i % 3 == 0) ? 1.0 : 0.0;
        num[V_W][i] = 0.5 + (double)(i % 17) / 16.0;
        num[V_IW][i] = (double)((i % 13) - 3) / 10.0;
        num[V_TH][i] = i % 37 == 0 ? MISSVAL : (double)(i % 10) / 10.0;
        num[V_X1][i] = i % 23 == 0 ? MISSVAL : i % 29 == 0 ? MISS_A : 20.0 * unif(i, 1) - 5.0;
        num[V_X2][i] = i % 19 == 0 ? MISSVAL : 3.0 * unif(i, 2) + 100.0;
        num[V_C1][i] = num[V_X1][i];
        num[V_C2][i] = num[V_X2][i];
        for (int v = V_O1; v <= V_S2; v++) num[v][i] = SENTINEL;
    }
}

/* Current call: varlist (dataset variable per 1-based position). */
static int varlist[32];
static int nvarlist = 0;
static ST_int stopflag = 0;

static int pos_ok(int i, int j) { return i >= 1 && i <= nvarlist && j >= 1 && j <= NOBS; }

/* ---------------------------------------------------------------- ST_plugin functions */

#define MAXREG 16
static struct { char name[64]; char value[512]; } macros[MAXREG];
static int nmacros = 0;
static char last_err[1024];

static const char *macro_get(const char *name) {
    for (int k = 0; k < nmacros; k++) if (!strcmp(macros[k].name, name)) return macros[k].value;
    return NULL;
}

static ST_int m_spoutsml(char *s) { fputs(s, stdout); return 0; }
static ST_int m_spouterr(char *s) {
    fputs(s, stdout);
    snprintf(last_err, sizeof last_err, "%s", s);
    return 0;
}
static void m_spoutflush(void) { fflush(stdout); }
static ST_int m_poll(void) { return 0; }
static ST_int m_macresave(char *name, char *value) {
    int k = 0;
    while (k < nmacros && strcmp(macros[k].name, name)) k++;
    if (k == nmacros) { if (nmacros == MAXREG) return 920; nmacros++; }
    snprintf(macros[k].name, sizeof macros[k].name, "%s", name);
    snprintf(macros[k].value, sizeof macros[k].value, "%s", value);
    return 0;
}
static ST_int m_vdata(ST_int i, ST_int j, ST_double *z) {
    if (!pos_ok(i, j) || varlist[i - 1] == V_SKEY) return 198;
    *z = num[varlist[i - 1]][j - 1];
    return 0;
}
static ST_int m_store(ST_int i, ST_int j, ST_double v) {
    if (!pos_ok(i, j) || varlist[i - 1] == V_SKEY) return 198;
    num[varlist[i - 1]][j - 1] = v;
    return 0;
}
static ST_int m_sdata(ST_int i, ST_int j, char *s) {
    if (!pos_ok(i, j) || varlist[i - 1] != V_SKEY) return 198;
    strcpy(s, skey[j - 1]);
    return 0;
}
static ST_int m_sstore(ST_int i, ST_int j, char *s) { (void)i; (void)j; (void)s; return 198; }
static ST_boolean m_isstr(ST_int i) { return i >= 1 && i <= nvarlist && varlist[i - 1] == V_SKEY; }
static ST_boolean m_isstrl(ST_int i) { (void)i; return 0; }
static ST_boolean m_isbinary(ST_int i, ST_int j) { (void)i; (void)j; return 0; }
static ST_int m_sdatalen(ST_int i, ST_int j) { return pos_ok(i, j) ? (ST_int)strlen(skey[j - 1]) : -198; }
static ST_int m_strldata(ST_int i, ST_int j, char *s, ST_int len) { (void)i; (void)j; (void)s; (void)len; return -198; }
static ST_boolean m_selobs(ST_int j) { return j >= 1 && j <= NOBS; }
static ST_int m_nobs1(void) { return 1; }
static ST_int m_nobs2(void) { return NOBS; }
static ST_int m_nobs(void) { return NOBS; }
static ST_int m_nvar(void) { return NVAR; }
static ST_int m_nvars(void) { return nvarlist; }
static ST_boolean m_ismissing(ST_double z) { return z >= MISSVAL; }

static ST_plugin table;

static void init_table(void) {
    memset(&table, 0, sizeof table);
    table.spoutsml = m_spoutsml;
    table.spoutnosml = m_spoutsml;
    table.spoutflush = m_spoutflush;
    table.pollstd = m_poll;
    table.pollnow = m_poll;
    table.macresave = m_macresave;
    table.vdata = m_vdata;
    table.safevdata = m_vdata;
    table.store = m_store;
    table.safestore = m_store;
    table.sdata = m_sdata;
    table.sstore = m_sstore;
    table.isstr = m_isstr;
    table.isstrl = m_isstrl;
    table.isbinary = m_isbinary;
    table.sdatalen = m_sdatalen;
    table.strldata = m_strldata;
    table.selobs = m_selobs;
    table.nobs1 = m_nobs1;
    table.nobs2 = m_nobs2;
    table.nobs = m_nobs;
    table.nvar = m_nvar;
    table.nvars = m_nvars;
    table.missval = MISSVAL;
    table.ismissing = m_ismissing;
    table.stopflag = &stopflag;
    table.spouterr = m_spouterr;
    table.major = 3;
    table.minor = 0;
}

/* ---------------------------------------------------------------- plugin loading */

typedef ST_retcode (*pginit_fn)(ST_plugin *);
typedef ST_retcode (*call_fn)(int, char **);
static call_fn stata_call_p;

static int load_plugin(const char *path) {
    void *pg, *sc;
#ifdef _WIN32
    HMODULE lib = LoadLibraryA(path);
    if (!lib) { fprintf(stderr, "cannot load %s (error %lu)\n", path, GetLastError()); return 0; }
    pg = (void *)GetProcAddress(lib, "pginit");
    sc = (void *)GetProcAddress(lib, "stata_call");
#else
    void *lib = dlopen(path, RTLD_NOW | RTLD_LOCAL);
    if (!lib) { fprintf(stderr, "cannot load %s: %s\n", path, dlerror()); return 0; }
    pg = dlsym(lib, "pginit");
    sc = dlsym(lib, "stata_call");
#endif
    if (!pg || !sc) { fprintf(stderr, "plugin lacks pginit/stata_call\n"); return 0; }
    ST_retcode ver = ((pginit_fn)pg)(&table);
    if (ver != SD_PLUGINVER) { fprintf(stderr, "pginit returned %d, expected %d\n", ver, SD_PLUGINVER); return 0; }
    stata_call_p = (call_fn)sc;
    return 1;
}

/* Calls the plugin: vars = dataset variables (terminated by -1), args = argv (NULL-terminated). */
static int call(const int *vars, const char **args) {
    nvarlist = 0;
    while (vars[nvarlist] >= 0) { varlist[nvarlist] = vars[nvarlist]; nvarlist++; }
    int argc = 0;
    char *argv[32];
    while (args[argc]) { argv[argc] = (char *)args[argc]; argc++; }
    last_err[0] = 0;
    return stata_call_p(argc, argv);
}

/* ---------------------------------------------------------------- reference */

enum { W_NONE, W_FW, W_AW, W_IW, W_PW };

static int gid[NOBS];

static void groups_none(void) { for (int i = 0; i < NOBS; i++) gid[i] = 0; }

static void groups_sorted(void) {
    int g = 0;
    for (int i = 0; i < NOBS; i++) {
        if (i > 0 && num[V_KEY][i] != num[V_KEY][i - 1]) g++;
        gid[i] = g;
    }
}

static void groups_string(void) {
    static char names[MAXG][8];
    int n = 0;
    for (int i = 0; i < NOBS; i++) {
        int g = 0;
        while (g < n && strcmp(names[g], skey[i])) g++;
        if (g == n) strcpy(names[n++], skey[i]);
        gid[i] = g;
    }
}

static double val(double z) { return z >= MISSVAL ? NAN : z; }
static double fin(double z) { return isfinite(z) ? z : MISSVAL; }

/* Expected outputs for variable x: out (fill a, or in place over `init` with ifin), mean, sd. */
static void reference(int x, int wtype, int wvar, int thvar, double theta, int std, int inplace,
                      const double *init, double *eo, double *em, double *es) {
    double s0[MAXG] = {0}, s1[MAXG] = {0}, n[MAXG] = {0}, m2[MAXG] = {0}, mean[MAXG], sd[MAXG];
    for (int i = 0; i < NOBS; i++) {
        double xi = val(num[x][i]), w = wtype == W_NONE ? 1.0 : num[wvar][i];
        if (num[V_TOUSE][i] == 0.0 || isnan(xi)) continue;
        s0[gid[i]] += w;
        s1[gid[i]] += w * xi;
        n[gid[i]] += 1.0;
    }
    for (int g = 0; g < MAXG; g++) mean[g] = s1[g] / s0[g];
    for (int i = 0; i < NOBS; i++) {
        double xi = val(num[x][i]), w = wtype == W_NONE ? 1.0 : num[wvar][i];
        if (num[V_TOUSE][i] == 0.0 || isnan(xi)) continue;
        m2[gid[i]] += w * (xi - mean[gid[i]]) * (xi - mean[gid[i]]);
    }
    for (int g = 0; g < MAXG; g++) {
        double denom = (wtype == W_FW || wtype == W_IW) ? s0[g] - 1.0 : s0[g] - s0[g] / n[g];
        sd[g] = std ? sqrt(m2[g] / denom) : 1.0;
    }
    for (int i = 0; i < NOBS; i++) {
        int s = num[V_TOUSE][i] != 0.0, g = gid[i];
        double xi = val(num[x][i]), th = thvar >= 0 ? val(num[thvar][i]) : theta;
        double v = fin((xi - th * mean[g]) / sd[g]);
        if (!inplace) eo[i] = s ? v : MISSVAL;
        else eo[i] = s ? v : num[V_IFIN][i] != 0.0 ? MISSVAL : init[i];
        em[i] = s && !isnan(xi) ? fin(mean[g]) : MISSVAL;
        es[i] = s && !isnan(xi) ? fin(sd[g]) : MISSVAL;
    }
}

/* ---------------------------------------------------------------- checks */

static int fails = 0;

static void pass(const char *what) { printf("PASS    %s\n", what); }
static void fail(const char *what, const char *why) { printf("FAIL    %s: %s\n", what, why); fails++; }

/* Missing values must match bit for bit, others within reldif TOL. Returns 1 when equal. */
static int same(const char *what, int var, const double *want) {
    for (int i = 0; i < NOBS; i++) {
        double g = num[var][i], w = want[i];
        int bad = (g >= MISSVAL || w >= MISSVAL) ? g != w : fabs(g - w) / (fabs(w) + 1.0) > TOL;
        if (bad) {
            char why[160];
            snprintf(why, sizeof why, "obs %d: got %.17g, want %.17g", i + 1, g, w);
            fail(what, why);
            return 0;
        }
    }
    return 1;
}

static void reset_outputs(void) {
    for (int i = 0; i < NOBS; i++)
        for (int v = V_O1; v <= V_S2; v++) num[v][i] = SENTINEL;
}

static double eo[NOBS], em[NOBS], es[NOBS], init[NOBS];

/* ---------------------------------------------------------------- tests */

static void test_version(void) {
    const char *args[] = {"version", NULL};
    int vars[] = {-1};
    int rc = call(vars, args);
    const char *v = macro_get("_c2_plugver");
    if (rc || !v || !*v) { fail("version", "no _c2_plugver local"); return; }
    printf("        plugin version %s\n", v);
    pass("version");
}

static void test_selftest(void) {
    reset_outputs();
    const char *args[] = {"selftest", "k=2", "sign=-1", "threads=4", NULL};
    int vars[] = {V_X1, V_X2, V_O1, V_O2, -1};
    int rc = call(vars, args);
    if (rc) { fail("selftest", last_err); return; }
    for (int j = 0; j < 2; j++) {
        int x = j ? V_X2 : V_X1;
        for (int i = 0; i < NOBS; i++) eo[i] = num[x][i] >= MISSVAL ? MISSVAL : -num[x][i];
        if (!same("selftest", j ? V_O2 : V_O1, eo)) return;
    }
    pass("selftest");
}

static void test_sorted_aw_theta_std(void) {
    const char *what = "center sorted, aw, theta variable, std, mean and sd";
    reset_outputs();
    groups_sorted();
    const char *args[] = {"center", "k=2", "nkey=1", "keymode=sorted", "touse=1", "ifin=all", "wtype=aw",
                          "thetavar=1", "std=1", "fill=aa", "mean=1", "sd=1", "threads=4", NULL};
    int vars[] = {V_KEY, V_TOUSE, V_W, V_TH, V_X1, V_X2, V_O1, V_O2, V_M1, V_M2, V_S1, V_S2, -1};
    int rc = call(vars, args);
    if (rc) { fail(what, last_err); return; }
    for (int j = 0; j < 2; j++) {
        reference(j ? V_X2 : V_X1, W_AW, V_W, V_TH, 0.0, 1, 0, NULL, eo, em, es);
        if (!same(what, j ? V_O2 : V_O1, eo) || !same(what, j ? V_M2 : V_M1, em) || !same(what, j ? V_S2 : V_S1, es))
            return;
    }
    pass(what);
}

static void test_hash_string_std(void) {
    const char *what = "center hash on a string key, std";
    reset_outputs();
    groups_string();
    const char *args[] = {"center", "k=2", "nkey=1", "keymode=hash", "touse=1", "ifin=all", "wtype=none",
                          "theta=1", "std=1", "fill=aa", "mean=0", "sd=0", "threads=3", NULL};
    int vars[] = {V_SKEY, V_TOUSE, V_X1, V_X2, V_O1, V_O2, -1};
    int rc = call(vars, args);
    if (rc) { fail(what, last_err); return; }
    for (int j = 0; j < 2; j++) {
        reference(j ? V_X2 : V_X1, W_NONE, -1, -1, 1.0, 1, 0, NULL, eo, em, es);
        if (!same(what, j ? V_O2 : V_O1, eo)) return;
    }
    pass(what);
}

static void test_none_iw_std(void) {
    const char *what = "center without keys, iw (negative weights), theta 0.5, std, sd";
    reset_outputs();
    groups_none();
    const char *args[] = {"center", "k=2", "nkey=0", "keymode=none", "touse=1", "ifin=all", "wtype=iw",
                          "theta=0.5", "std=1", "fill=aa", "mean=0", "sd=1", "threads=0", NULL};
    int vars[] = {V_TOUSE, V_IW, V_X1, V_X2, V_O1, V_O2, V_S1, V_S2, -1};
    int rc = call(vars, args);
    if (rc) { fail(what, last_err); return; }
    for (int j = 0; j < 2; j++) {
        reference(j ? V_X2 : V_X1, W_IW, V_IW, -1, 0.5, 1, 0, NULL, eo, em, es);
        if (!same(what, j ? V_O2 : V_O1, eo) || !same(what, j ? V_S2 : V_S1, es)) return;
    }
    pass(what);
}

static void test_inplace_ifin(void) {
    const char *what = "center in place with an ifin variable (untouched cells keep their values)";
    reset_outputs();
    groups_sorted();
    const char *args[] = {"center", "k=2", "nkey=1", "keymode=sorted", "touse=1", "ifin=var", "wtype=none",
                          "theta=1", "std=0", "fill=ii", "mean=1", "sd=0", "threads=4", NULL};
    int vars[] = {V_KEY, V_TOUSE, V_IFIN, V_C1, V_C2, V_C1, V_C2, V_M1, V_M2, -1};
    static double before[2][NOBS];
    for (int i = 0; i < NOBS; i++) { before[0][i] = num[V_C1][i]; before[1][i] = num[V_C2][i]; }
    int rc = call(vars, args);
    if (rc) { fail(what, last_err); return; }
    for (int j = 0; j < 2; j++) {
        /* the reference reads x from the pre-call copy */
        int c = j ? V_C2 : V_C1;
        static double got[NOBS];
        memcpy(got, num[c], sizeof got);
        memcpy(num[c], before[j], sizeof got);
        memcpy(init, before[j], sizeof init);
        reference(c, W_NONE, -1, -1, 1.0, 0, 1, init, eo, em, es);
        memcpy(num[c], got, sizeof got);
        if (!same(what, c, eo) || !same(what, j ? V_M2 : V_M1, em)) return;
    }
    pass(what);
}

static void test_bad_argument(void) {
    const char *args[] = {"center", "k=1", "nkey=0", "keymode=none", "touse=0", "ifin=all", "wtype=none",
                          "theta=1", "std=0", "fill=a", "mean=0", "sd=0", "bogus=1", NULL};
    int vars[] = {V_X1, V_O1, -1};
    int rc = call(vars, args);
    if (rc != 198 || !strstr(last_err, "bogus")) {
        char why[1100];
        snprintf(why, sizeof why, "rc %d, message \"%s\"", rc, last_err);
        fail("bad argument gives r(198)", why);
        return;
    }
    pass("bad argument gives r(198)");
}

int main(int argc, char **argv) {
    if (argc < 2) {
        fprintf(stderr, "usage: mockhost <center2 plugin>\n");
        return 2;
    }
    make_data();
    init_table();
    if (!load_plugin(argv[1])) return 2;
    test_version();
    test_selftest();
    test_sorted_aw_theta_std();
    test_hash_string_std();
    test_none_iw_std();
    test_inplace_ifin();
    test_bad_argument();
    printf("%s: %d failed\n", fails ? "MOCKHOST FAILED" : "MOCKHOST PASSED", fails);
    return fails ? 1 : 0;
}
