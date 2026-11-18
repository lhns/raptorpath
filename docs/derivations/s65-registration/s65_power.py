#!/usr/bin/env python3
"""Design computation (before any section 65 row): the pair count for section 65, from section 64's committed rows.

    python -I -X utf8 s65_power.py <tools/l1 dir> <docs/l1-raw/s64/s64-session.tgz>

Noise models (resampled with replacement, centred on their own Hodges-Lehmann shift, so each carries spread only):
  HEAD  the 11 stage R pairs of section 64 (Realtime c2-100 at rho = 1, 100 MB): d = ln(RC / RB) of the in-order mean over ticks t <= 11.5 s (s65_score.row_fields on the rows' own logs);
  LONG  the 12 stage G c2-100 pairs of section 64 (Bulk, 600 MB), paired by slot: d = ln(CAND / BASE) of the whole-run in-order mean; the stand-in for REST (t > 11.5 s of a 70 s row);
  LONGx4  LONG's centred residuals x 4 (sd about 0.10): a sensitivity row, since no Realtime 600 MB pair exists and Bulk may be the quiet case;
  HEADasREST  HEAD's residuals used for REST: the pessimistic bracket (Realtime rest as noisy as section 64's 12 s rows).
For n = 12, 18, 24 pairs and a shift delta added to every resampled d: P(REGRESSION) = P(p <= 0.05 and HL > TOL) at the seen size (the HEAD pairs' own HL shift)
and at half of it, and with no shift the mean U and P(NO-REGRESSION) = P(U <= TOL). 4000 draws per cell, random.Random(65): a design computation, never part of a test."""
import math
import os
import random
import sys
import tarfile
import tempfile

TOOLS, TGZ = sys.argv[1], sys.argv[2]
sys.path.insert(0, os.path.abspath(TOOLS))
import l1common as lc  # noqa: E402
import place38_parse as pp  # noqa: E402
import s65_score as s  # noqa: E402

DRAWS = 4000

tmp = tempfile.mkdtemp()
with tarfile.open(TGZ) as t:
    t.extractall(tmp, filter="data")
S = os.path.join(tmp, "session")

# HEAD: stage R pairs, head from the logs
rrows, _ = pp.rows_of([os.path.join(S, "rt64", "ld.log")])
by = {(r["arm"], str(r["seed"]), str(r["rep"])): r for r in rrows}
head = []
for (a, sd, rp), r in sorted(by.items()):
    if a != "RB" or r["status"] != "LIVE":
        continue
    f = {arm: s.row_fields(lc.read(os.path.join(S, "rt64", "diag-ld", f"c2-100@def-{arm}-s{sd}-r{rp}-a1-c.log")))["inorder_mean_head"] for arm in ("RB", "RC")}
    head.append(math.log(f["RC"] / f["RB"]))
# LONG: stage G c2 pairs
grows, _ = pp.rows_of([os.path.join(S, "guard", "ld.log")])
gb = {(r["arm"], str(r["seed"]), str(r["rep"])): r for r in grows if r["cell"] == "c2-100" and r["status"] == "LIVE"}
long_ = [math.log(gb[("CAND", sd, rp)]["inorder_mean"] / gb[("BASE", sd, rp)]["inorder_mean"]) for (a, sd, rp) in sorted(gb) if a == "BASE" and ("CAND", sd, rp) in gb]


def centred(d):
    h = s.hl_upper(d)[0]
    return [x - h for x in d]


def sd_(v):
    m = sum(v) / len(v)
    return math.sqrt(sum((x - m) ** 2 for x in v) / (len(v) - 1))


print("S65 POWER (design computation before any section 65 row; not a reading)")
for name, d in (("HEAD", head), ("LONG", long_)):
    h, u, _ = s.hl_upper(d)
    print(f"  {name}: n={len(d)} pairs; HL {100 * (math.exp(h) - 1):+.2f} %; sd of d {sd_(d):.4f}; d = " + " ".join(f"{x:+.3f}" for x in d))
SEEN = math.exp(s.hl_upper(head)[0]) - 1
print()
rng = random.Random(65)
print(f"  model       n   P(REG) at {100 * SEEN:+.1f} %   P(REG) at {100 * SEEN / 2:+.1f} %   no shift: mean U    P(NO-REG)   P(REG) (false)")
models = (("HEAD", centred(head)), ("LONG", centred(long_)), ("LONGx4", [4 * x for x in centred(long_)]), ("HEADasREST", centred(head)))
for name, e in models:
    for n in (12, 18, 24):
        res = {}
        for tag, delta in (("seen", math.log(1 + SEEN)), ("half", math.log(1 + SEEN / 2)), ("zero", 0.0)):
            reg = noreg = 0
            us = []
            for _ in range(DRAWS):
                x = [rng.choice(e) + delta for _ in range(n)]
                p, _, _ = s.signed_rank_p(x)
                hh, uu, _ = s.hl_upper(x)
                h_rel, u_rel = math.exp(hh) - 1, math.exp(uu) - 1
                reg += p <= s.ALPHA and h_rel > s.TOL
                noreg += u_rel <= s.TOL
                us.append(u_rel)
            res[tag] = (reg / DRAWS, noreg / DRAWS, sum(us) / DRAWS)
        print(f"  {name:<10} {n:>2}   {res['seen'][0]:>8.3f}            {res['half'][0]:>8.3f}            {100 * res['zero'][2]:+7.2f} %       {res['zero'][1]:.3f}       {res['zero'][0]:.3f}")
