#!/usr/bin/env python3
"""Design computation (before any section 65 row): the pair count and the pair floor for section 65, the exact replication of section 64's stage R.

    python -I -X utf8 s65_power.py <tools/l1 dir> <docs/l1-raw/s64/s64-session.tgz>

Noise model: section 64's 11 stage R pairs (Realtime c2-100, rho = 1, 100 MB; the configuration section 65 repeats), d = ln(RC / RB) of the whole-row in-order mean
(the rows' own inorder_mean), centred on its Hodges-Lehmann shift so it carries spread only, resampled with replacement. For n = 12, 18, 24 pairs and a shift added to every
resampled d: P(REPLICATED) = P(p <= 0.05 and HL > TOL) at the seen size (the pairs' own HL) and at half of it; with no shift the mean U and the false P(REPLICATED).
4000 draws per cell, random.Random(65): a design computation, never part of a test."""
import math
import os
import random
import sys
import tarfile
import tempfile

TOOLS, TGZ = sys.argv[1], sys.argv[2]
sys.path.insert(0, os.path.abspath(TOOLS))
import place38_parse as pp  # noqa: E402
import s65_score as s  # noqa: E402

DRAWS = 4000

tmp = tempfile.mkdtemp()
with tarfile.open(TGZ) as t:
    t.extractall(tmp, filter="data")
rows, _ = pp.rows_of([os.path.join(tmp, "session", "rt64", "ld.log")])
by = {(r["arm"], str(r["seed"]), str(r["rep"])): r for r in rows}
d = [math.log(by[("RC", sd, rp)]["inorder_mean"] / r["inorder_mean"]) for (a, sd, rp), r in sorted(by.items()) if a == "RB" and r["status"] == "LIVE"]
h0 = s.hl_upper(d)[0]
seen = math.exp(h0) - 1
e = [x - h0 for x in d]
m = sum(d) / len(d)
print("S65 POWER (design computation before any section 65 row; not a reading)")
print(f"  model: section 64 stage R, n={len(d)} pairs; HL {100 * seen:+.2f} %; sd of d {math.sqrt(sum((x - m) ** 2 for x in d) / (len(d) - 1)):.4f}; d = " + " ".join(f"{x:+.3f}" for x in d))
print()
rng = random.Random(65)
print(f"  n   P(REPLICATED) at {100 * seen:+.1f} %   P(REPLICATED) at {100 * seen / 2:+.1f} %   no shift: mean U   no shift: P(REPLICATED) (false)")
for n in (12, 18, 24):
    res = {}
    for tag, delta in (("seen", h0), ("half", math.log(1 + seen / 2)), ("zero", 0.0)):
        rep = 0
        us = []
        for _ in range(DRAWS):
            x = [rng.choice(e) + delta for _ in range(n)]
            p, _, _ = s.signed_rank_p(x)
            hh, uu, _ = s.hl_upper(x)
            rep += p <= s.ALPHA and math.exp(hh) - 1 > s.TOL
            us.append(math.exp(uu) - 1)
        res[tag] = (rep / DRAWS, sum(us) / DRAWS)
    print(f"  {n:>2}  {res['seen'][0]:>10.3f}               {res['half'][0]:>10.3f}               {100 * res['zero'][1]:+7.2f} %          {res['zero'][0]:.3f}")
