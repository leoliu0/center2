//! Groups as ranges `[starts[g], starts[g+1])` over positions; a position maps to an observation
//! by identity ([`sorted_starts`], or no keys) or through an order vector ([`hash_groups`]).

use crate::error::{Error, Res, vec_filled, vec_with_capacity};
use crate::io::{Io, Rows};
use crate::par::{StoreMode, for_blocks};

/// Positions per work item when scanning keys and touse.
const KEY_CHUNK: usize = 65_536;

/// Hash partitions of [`hash_groups`]; fixed, so group ids do not depend on the thread count.
const PARTS: usize = 64;

/// Group starts over observations `0..n` of data sorted by `keys` (the `by` prefix guarantees
/// it): a group starts where any key differs from the previous observation under f64 `!=`, so
/// −0 equals +0 and `.` differs from `.a`, as in Stata's `by`. Returns `[0, starts.., n]`.
pub fn sorted_starts<I: Io>(io: &I, keys: &[i32], n: usize, threads: usize) -> Res<Vec<usize>> {
    let nk = keys.len();
    let parts = for_blocks(io, n.div_ceil(KEY_CHUNK), threads, StoreMode::Parallel, |c, _emit| {
        let lo = c * KEY_CHUNK;
        let hi = (lo + KEY_CHUNK).min(n);
        let from = lo.saturating_sub(1);
        let mut buf = vec_filled((hi - from) * nk, 0.0)?;
        io.read_block(keys, Rows::Range { first: from, n: hi - from }, &mut buf)?;
        let mut out = Vec::new();
        for i in lo.max(1)..hi {
            let r = i - from;
            if buf[r * nk..(r + 1) * nk] != buf[(r - 1) * nk..r * nk] {
                out.push(i);
            }
        }
        Ok(out)
    })?;
    let mut starts = vec_with_capacity(2 + parts.iter().map(Vec::len).sum::<usize>())?;
    starts.push(0);
    for p in parts {
        starts.extend(p);
    }
    starts.push(n);
    Ok(starts)
}

/// Key values of the sample observations (index `i` = i-th sample observation).
pub struct KeyView {
    kinds: Vec<KeyKind>,
    nnum: usize,
    /// Numeric keys, `nnum` per observation: bits of the value, with −0 mapped to +0.
    num: Vec<u64>,
    strs: Vec<StrCol>,
}

#[derive(Clone, Copy)]
enum KeyKind {
    Num(usize),
    Str(usize),
}

struct StrCol {
    off: Vec<usize>,
    bytes: Vec<u8>,
}

impl StrCol {
    fn get(&self, i: usize) -> &[u8] {
        &self.bytes[self.off[i]..self.off[i + 1]]
    }
}

impl KeyView {
    fn eq(&self, a: usize, b: usize) -> bool {
        let nn = self.nnum;
        self.num[a * nn..(a + 1) * nn] == self.num[b * nn..(b + 1) * nn] && self.strs.iter().all(|s| s.get(a) == s.get(b))
    }
}

/// Hash of the key tuple of sample observation `i`.
pub type HashFn = fn(&KeyView, usize) -> u64;

fn mix(mut z: u64) -> u64 {
    // splitmix64 finalizer
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// splitmix64 of each numeric key, FNV-1a 64 of each string key, folded by `h = mix(h ^ k)`.
pub fn default_hash(k: &KeyView, i: usize) -> u64 {
    let mut h: u64 = 0x9e37_79b9_7f4a_7c15;
    for kind in &k.kinds {
        let kh = match *kind {
            KeyKind::Num(c) => mix(k.num[i * k.nnum + c]),
            KeyKind::Str(c) => fnv1a(k.strs[c].get(i)),
        };
        h = mix(h ^ kh);
    }
    h
}

/// Observations grouped by key tuple: `ord` maps positions to observations; sample observations
/// come first, grouped (groups in partition order, first-appearance order inside a partition,
/// observations ascending inside a group), then every non-sample observation as one final range.
pub struct Grouping {
    pub ord: Vec<u32>,
    pub starts: Vec<usize>,
}

/// Groups unsorted data by `keys` (numeric, str# or strL) without touching Stata's sort order.
/// Only observations with touse != 0 (all observations without touse) are grouped.
pub fn hash_groups<I: Io>(io: &I, keys: &[i32], touse: Option<i32>, threads: usize, hash: HashFn) -> Res<Grouping> {
    let n = io.nobs();
    if n > u32::MAX as usize {
        return Err(Error::syntax("center2: by() supports at most 4294967295 observations"));
    }

    // 1. Sample and non-sample observations, ascending.
    let (samp, rest): (Vec<u32>, Vec<u32>) = match touse {
        None => ((0..n as u32).collect(), Vec::new()),
        Some(tv) => {
            let chunks = for_blocks(io, n.div_ceil(KEY_CHUNK), threads, StoreMode::Parallel, |c, _emit| {
                let lo = c * KEY_CHUNK;
                let hi = (lo + KEY_CHUNK).min(n);
                let mut buf = vec_filled(hi - lo, 0.0)?;
                io.read_block(&[tv], Rows::Range { first: lo, n: hi - lo }, &mut buf)?;
                let (mut s, mut r) = (Vec::new(), Vec::new());
                for (i, &t) in buf.iter().enumerate() {
                    if t != 0.0 { s.push((lo + i) as u32) } else { r.push((lo + i) as u32) }
                }
                Ok((s, r))
            })?;
            let mut samp = vec_with_capacity(chunks.iter().map(|c| c.0.len()).sum())?;
            let mut rest = vec_with_capacity(chunks.iter().map(|c| c.1.len()).sum())?;
            for (s, r) in chunks {
                samp.extend(s);
                rest.extend(r);
            }
            (samp, rest)
        }
    };
    let ns = samp.len();

    // 2. Keys of the sample observations.
    let num_keys: Vec<i32> = keys.iter().copied().filter(|&v| !io.is_str(v)).collect();
    let nnum = num_keys.len();
    let mut kinds = Vec::with_capacity(keys.len());
    let (mut inum, mut istr) = (0, 0);
    for &v in keys {
        if io.is_str(v) {
            kinds.push(KeyKind::Str(istr));
            istr += 1;
        } else {
            kinds.push(KeyKind::Num(inum));
            inum += 1;
        }
    }
    let num = if nnum == 0 {
        Vec::new()
    } else {
        let chunks = for_blocks(io, ns.div_ceil(KEY_CHUNK), threads, StoreMode::Parallel, |c, _emit| {
            let lo = c * KEY_CHUNK;
            let hi = (lo + KEY_CHUNK).min(ns);
            let mut buf = vec_filled((hi - lo) * nnum, 0.0)?;
            io.read_block(&num_keys, Rows::List(&samp[lo..hi]), &mut buf)?;
            Ok(buf.iter().map(|&v| if v == 0.0 { 0.0f64.to_bits() } else { v.to_bits() }).collect::<Vec<u64>>())
        })?;
        let mut num = vec_with_capacity(ns * nnum)?;
        for c in chunks {
            num.extend(c);
        }
        num
    };
    let mut strs = Vec::new();
    for &v in keys.iter().filter(|&&v| io.is_str(v)) {
        let mut off = vec_with_capacity(ns + 1)?;
        let mut bytes = Vec::new();
        off.push(0);
        for &o in &samp {
            io.read_str(v, o as usize, &mut bytes)?;
            off.push(bytes.len());
        }
        strs.push(StrCol { off, bytes });
    }
    let kv = KeyView { kinds, nnum, num, strs };

    // 3. Hashes.
    let chunks = for_blocks(io, ns.div_ceil(KEY_CHUNK), threads, StoreMode::Parallel, |c, _emit| {
        let lo = c * KEY_CHUNK;
        let hi = (lo + KEY_CHUNK).min(ns);
        Ok((lo..hi).map(|i| hash(&kv, i)).collect::<Vec<u64>>())
    })?;
    let mut h = vec_with_capacity(ns)?;
    for c in chunks {
        h.extend(c);
    }

    // 4. Stable bucketing of the sample indices by partition (top 6 hash bits).
    let part_of = |i: usize| (h[i] >> 58) as usize;
    let mut pcount = [0usize; PARTS];
    for i in 0..ns {
        pcount[part_of(i)] += 1;
    }
    let mut pstart = [0usize; PARTS + 1];
    for p in 0..PARTS {
        pstart[p + 1] = pstart[p] + pcount[p];
    }
    let mut fillp = pstart;
    let mut pidx = vec_filled(ns, 0u32)?;
    for i in 0..ns {
        let p = part_of(i);
        pidx[fillp[p]] = i as u32;
        fillp[p] += 1;
    }

    // 5. Per partition: open-addressing table (first-appearance ids), then a stable counting
    //    sort of the partition's observations by id.
    let per_part = for_blocks(io, PARTS, threads, StoreMode::Parallel, |p, _emit| {
        let members = &pidx[pstart[p]..pstart[p + 1]];
        let cap = (2 * members.len()).next_power_of_two().max(16);
        let mask = cap - 1;
        const EMPTY: u32 = u32::MAX;
        let mut table: Vec<(u64, u32, u32)> = vec_filled(cap, (0, EMPTY, 0))?;
        let mut lid = vec_filled(members.len(), 0u32)?;
        let mut sizes: Vec<u32> = Vec::new();
        for (m, &i) in members.iter().enumerate() {
            let hi = h[i as usize];
            let mut slot = hi as usize & mask;
            loop {
                let (eh, rep, id) = table[slot];
                if rep == EMPTY {
                    let id = sizes.len() as u32;
                    table[slot] = (hi, i, id);
                    sizes.push(1);
                    lid[m] = id;
                    break;
                }
                if eh == hi && kv.eq(rep as usize, i as usize) {
                    sizes[id as usize] += 1;
                    lid[m] = id;
                    break;
                }
                slot = (slot + 1) & mask;
            }
        }
        let mut off = vec_filled(sizes.len() + 1, 0usize)?;
        for (g, &s) in sizes.iter().enumerate() {
            off[g + 1] = off[g] + s as usize;
        }
        let mut ord = vec_filled(members.len(), 0u32)?;
        for (m, &i) in members.iter().enumerate() {
            let g = lid[m] as usize;
            ord[off[g]] = samp[i as usize];
            off[g] += 1;
        }
        Ok((ord, sizes))
    })?;

    // 6. Assemble positions: grouped sample, then the non-sample range.
    let ngroups: usize = per_part.iter().map(|p| p.1.len()).sum();
    let mut ord = vec_with_capacity(n)?;
    let mut starts = vec_with_capacity(ngroups + 2)?;
    starts.push(0);
    let mut acc = 0;
    for (o, sizes) in per_part {
        ord.extend(o);
        for s in sizes {
            acc += s as usize;
            starts.push(acc);
        }
    }
    if !rest.is_empty() || starts.len() == 1 {
        ord.extend(rest);
        starts.push(n);
    }
    Ok(Grouping { ord, starts })
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::io::{MISS, MemIo};
    use crate::testutil::*;

    #[test]
    fn hash_path_matches_the_reference_on_shuffled_data() {
        let d = make_data(SIZES, 2, "aw", 9);
        // Shuffle the observations so that groups are not contiguous.
        let mut rng = Rng::new(42);
        let mut perm: Vec<usize> = (0..d.n).collect();
        for i in (1..d.n).rev() {
            perm.swap(i, (rng.next_u64() % (i as u64 + 1)) as usize);
        }
        let pick = |v: &Vec<f64>| perm.iter().map(|&p| v[p]).collect::<Vec<f64>>();
        let ds = Data {
            n: d.n,
            key: pick(&d.key),
            touse: pick(&d.touse),
            ifin: pick(&d.ifin),
            w: pick(&d.w),
            th: pick(&d.th),
            x: d.x.iter().map(pick).collect(),
        };
        for (std, theta, inplace, ifin_var) in [(false, Some(1.0), false, false), (true, None, false, false), (true, Some(0.3), true, true)] {
            let o = Opts { keymode: "hash", wtype: "aw", std, sd: std, theta, inplace, ifin_var, ..Opts::default() };
            assert_outcome(&run_center(&ds, &o), &reference(&ds, &o), &o, 1e-13);
        }
    }

    #[derive(Hash, PartialEq, Eq, Clone, Debug)]
    enum K {
        N(u64),
        S(Vec<u8>),
    }

    /// Checks `g` against the key tuples: sample observations grouped (one tuple per group,
    /// distinct tuples, ascending observations), non-sample observations last.
    fn check(g: &Grouping, tuples: &[Vec<K>], touse: &[bool]) -> usize {
        let n = tuples.len();
        let mut seen = vec![false; n];
        for &o in &g.ord {
            assert!(!seen[o as usize]);
            seen[o as usize] = true;
        }
        assert!(seen.iter().all(|&s| s) && g.ord.len() == n);
        let ns = touse.iter().filter(|&&t| t).count();
        let mut distinct = HashSet::new();
        let mut groups = 0;
        for w in g.starts.windows(2) {
            let members = &g.ord[w[0]..w[1]];
            if w[0] >= ns {
                assert!(members.iter().all(|&o| !touse[o as usize]));
                continue;
            }
            groups += 1;
            assert!(members.windows(2).all(|p| p[0] < p[1]));
            let t = &tuples[members[0] as usize];
            assert!(members.iter().all(|&o| touse[o as usize] && &tuples[o as usize] == t));
            assert!(distinct.insert(t.clone()), "tuple {t:?} split over two groups");
        }
        groups
    }

    fn key_data() -> (MemIo, Vec<i32>, Vec<Vec<K>>, Vec<bool>) {
        let num = [0.0, -0.0, MISS, miss_a(), 1.5, 2.0];
        let strs: [&[u8]; 3] = [b"", b"ab", b"abc"];
        let mut rng = Rng::new(3);
        let n = 5000;
        let (mut a, mut b, mut s, mut t) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for _ in 0..n {
            a.push(num[(rng.next_u64() % 6) as usize]);
            b.push((rng.next_u64() % 3) as f64);
            s.push(strs[(rng.next_u64() % 3) as usize].to_vec());
            t.push(if rng.unif() < 0.2 { 0.0 } else { 1.0 });
        }
        let canon = |v: f64| if v == 0.0 { 0.0f64.to_bits() } else { v.to_bits() };
        let tuples = (0..n).map(|i| vec![K::N(canon(a[i])), K::N(canon(b[i])), K::S(s[i].clone())]).collect();
        let touse = t.iter().map(|&x| x != 0.0).collect();
        let mut io = MemIo::new(n);
        let (ca, cb, cs, ct) = (io.add_num(&a), io.add_num(&b), io.add_str(s), io.add_num(&t));
        let keys = vec![io.push_var(ca), io.push_var(cb), io.push_var(cs)];
        io.push_var(ct);
        (io, keys, tuples, touse)
    }

    #[test]
    fn groups_by_numeric_and_string_keys_like_stata_by() {
        let (io, keys, tuples, touse) = key_data();
        let g = hash_groups(&io, &keys, Some(4), 4, default_hash).unwrap();
        // −0 equals +0, `.` differs from `.a`: 5 first-key values x 3 x 3 strings.
        assert_eq!(check(&g, &tuples, &touse), 45);
        let all = vec![true; touse.len()];
        let g = hash_groups(&io, &keys, None, 4, default_hash).unwrap();
        assert_eq!(check(&g, &tuples, &all), 45);
    }

    #[test]
    fn forced_collisions_still_separate_every_key() {
        let (io, keys, tuples, touse) = key_data();
        let g = hash_groups(&io, &keys, Some(4), 4, |_, _| 0).unwrap();
        assert_eq!(check(&g, &tuples, &touse), 45);
    }
}
