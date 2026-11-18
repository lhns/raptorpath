#!/usr/bin/env python3
"""PATH EXERCISE, not a reading: tools/l1/s65_score.py run end to end before registration.

    python -I -X utf8 s65_path_exercise.py <tools/l1 dir> <docs/l1-raw/s64/s64-session.tgz>

  A. section 64's committed stage R rows (100 MB, about 12 s of ticks; 12 slots), the plan 65 row fields computed from the rows' own client logs (s65_score.row_fields),
     the tick floor passed as 0 so the code runs past VOID-SHORT. Reproduction gate against s64-lean (6d55024c): RB whole-run q95 0.1930 over 200 splits; whole-run in-order 9/11,
     lag_E_cli 8/11, lag_E_srv 7/11, rtp_floor_p0 9/11 pairs RC worse. Its labels are the PREDICTION's source, not evidence (registration section 4);
  B. the same rows under the registered 30 s floor: every completed row VOID-SHORT;
  C. synthetic 24-pair ledgers that reach every label and every slot rule.
Nothing here is a reading of F2."""
import os
import re
import sys
import tarfile
import tempfile

TOOLS, TGZ = sys.argv[1], sys.argv[2]
sys.path.insert(0, os.path.abspath(TOOLS))
import l1common as lc  # noqa: E402
import place38_parse as pp  # noqa: E402
import s65_score as s  # noqa: E402

fails = 0


def check(name, ok):
    global fails
    print(("PASS " if ok else "FAIL ") + name)
    fails += 0 if ok else 1


def run(rows, min_tick=s.MIN_TICK_S, show=True, n_pairs=s.N_PAIRS):
    buf = []
    res = s.score([], out=buf.append, min_tick=min_tick, rows=rows, n_pairs=n_pairs)
    if show:
        for x in buf:
            print("    " + x)
    return res, "\n".join(buf)


tmp = tempfile.mkdtemp()
with tarfile.open(TGZ) as t:
    t.extractall(tmp, filter="data")
S = os.path.join(tmp, "session", "rt64")
rows64, _ = pp.rows_of([os.path.join(S, "ld.log")])
for r in rows64:
    r.update(s.row_fields(lc.read(os.path.join(S, "diag-ld", f"c2-100@def-{r['arm']}-s{r['seed']}-r{r['rep']}-a1-c.log"))))

print("PATH EXERCISE (not a reading). A: section 64 stage R rows (12-slot table), plan 65 fields from the logs, tick floor passed as 0")
res, txt = run(rows64, min_tick=0.0, n_pairs=12)
worse = {m.group(1): (int(m.group(2)), int(m.group(3))) for m in re.finditer(r"^  (\S+)\s+RC worse in\s+(\d+) of (\d+)", txt, re.M)}
check("A: s64-lean's RB whole-run q95 0.1930 over 200 splits (nRB 11)", "RESOLUTION65 whole (RB rows alone, REPORTED ONLY): q95|x|=0.1930 (splits=200, nRB=11)" in txt)
check("A: 11 usable pairs, the dead RB s42 r4's pair dropped; one dead row, not redrawn", res["n"] == 11 and "dropped: s42 r4" in txt
      and "BASE-F2-DEAD65 1 dead RB row(s) over 12 RB slots: redrawn alive 0, dead twice 0, not redrawn 1" in txt)
check("A: whole-run in-order RC worse in 9 of 11", res["whole"]["worse"] == 9 and res["whole"]["n"] == 11)
check("A: lag_E_cli 8/11, lag_E_srv 7/11, rtp_floor_p0 9/11 (s64-lean read 2)", worse.get("lag_E_cli") == (8, 11) and worse.get("lag_E_srv") == (7, 11) and worse.get("rtp_floor_p0") == (9, 11))
check("A: the head window holds every pair (t <= 11.5 s covers the 12 s rows); the rest has too few ticks to pair, so persistence reads open", res["head"]["n"] == 11 and res["rest"]["n"] < 10 and res["label"] == "REPLICATED-PERSISTENCE-OPEN")
print()
print("B: the same rows under the registered 30 s tick floor")
res, txt = run(rows64, show=False, n_pairs=12)
check("B: every completed row is VOID-SHORT, so 0 pairs and both windows UNRESOLVED-N", res["n"] == 0 and txt.count(": VOID-SHORT") == 23 and res["label"] == "UNRESOLVED")
print()

SLOTS = s.plan()
N = len(SLOTS) // 2
PAT = [((j * 7) % N) / (N - 1) * 2 - 1 for j in range(N)]
JIT = [((j * 11 + 5) % N) / (N - 1) * 2 - 1 for j in range(N)]


def row(arm, seed, rep, head, rest, status=None, dnf=False, items=None):
    return {"cell": "c2-100", "store": "def", "arm": arm, "seed": seed, "rep": rep, "status": status or ("LIVE" if arm == "RB" else "RT-OK"), "dnf": dnf,
            "tick_span_s": 70.0, "bytes": 600000000, "inorder_mean_head": head, "inorder_mean_rest": rest,
            "inorder_mean": None if head is None else (11.5 * head + 58.5 * rest) / 70.0, "mbps": 68.0, "cpu_cli": 80.0, "cpu_srv": 66.0,
            "cli_lag_E": 1300.0, "srv_lag_E": 800.0, "rtp_floor_p0": 10000, "qlost_2s": 600, "qsent_2s": 26000, "rt_items": items, "bin_sha": "synthetic"}


def ledger(eh, er, jh=0.005, jr=0.005, rb_noise=0.02, mods=None):
    """RB = 120 ms (head) / 150 ms (rest) x (1 + rb_noise x PAT); RC = RB x (1 + e) x (1 + j x JIT) per window. mods: {(arm, seed, rep): [rows]} replaces a slot's rows."""
    out = []
    for a, sd, rp in SLOTS:
        j = (int(rp) - 1) * 2 + (0 if sd == "42" else 1)
        bh, br = 120.0 * (1 + rb_noise * PAT[j]), 150.0 * (1 + rb_noise * PAT[j])
        h, r = (bh, br) if a == "RB" else (bh * (1 + eh) * (1 + jh * JIT[j]), br * (1 + er) * (1 + jr * JIT[j]))
        out += (mods or {}).get((a, sd, rp)) or [row(a, sd, rp, h, r)]
    return out


DEAD5 = {("RB", sd, rp): [row("RB", sd, rp, None, None, "BASE-F2-DEAD"), row("RB", sd, rp, None, None, "BASE-F2-DEAD")] for sd, rp in (("42", "1"), ("7", "2"), ("42", "3"), ("7", "4"), ("42", "5"))}
cases = [
    ("C1 quiet null in both windows", dict(eh=0.0, er=0.0), "CLOSED"),
    ("C2 +15 % in both windows", dict(eh=0.15, er=0.15), "BISECT"),
    ("C3 +15 % in the head only", dict(eh=0.15, er=0.0), "START-UP-COST"),
    ("C4 +15 % in the rest only", dict(eh=0.0, er=0.15), "LATE-ONLY"),
    ("C5 STUB: pairs as noisy as section 64's head (+-50 % uniform, sd 0.29 against the measured 0.26) in both windows, no shift: must NOT read NO-REGRESSION",
     dict(eh=0.0, er=0.0, jh=0.50, jr=0.50), "UNRESOLVED"),
    ("C6 a head as noisy as section 64's (+-50 %), quiet rest, no shift", dict(eh=0.0, er=0.0, jh=0.50), "NOT-REPLICATED"),
    ("C7 a resolved +3 % in both windows, bounded under TOL", dict(eh=0.03, er=0.03), "CLOSED"),
    ("C7b +15 % in the head, a rest as noisy as section 64's head (+-50 %): persistence open, never 'not a steady-state one'", dict(eh=0.15, er=0.0, jr=0.50), "REPLICATED-PERSISTENCE-OPEN"),
    ("C7c a noisy head (+-50 %), +15 % in the rest: a steady-state regression with the head open, never 'without the start-up one'", dict(eh=0.0, er=0.15, jh=0.50), "REST-REGRESSION"),
    ("C8 five RB slots dead twice: 19 pairs < 20", dict(eh=0.0, er=0.0, mods=DEAD5), "UNRESOLVED"),
    ("C9 a dead RB row redrawn LIVE takes its slot", dict(eh=0.0, er=0.0, mods={("RB", "42", "4"): [row("RB", "42", "4", None, None, "BASE-F2-DEAD"), row("RB", "42", "4", 120.0, 150.0)]}), "CLOSED"),
    ("C10 an RC RT-FAIL row is an F2-FINDING and drops its pair", dict(eh=0.0, er=0.0, mods={("RC", "7", "5"): [row("RC", "7", "5", 120.0, 150.0, "RT-FAIL", items=[2])]}), "CLOSED"),
]
for name, kw, want in cases:
    print(name)
    res, txt = run(ledger(**kw))
    check(f"{name}: label {want}", res["label"] == want)
    if name.startswith("C5"):
        check("C5: neither window reads NO-REGRESSION (the half-split q95 is reported only; U decides)", res["head"]["label"] != "NO-REGRESSION" and res["rest"]["label"] != "NO-REGRESSION")
    if name.startswith("C8"):
        check("C8: the five dead pairs are dropped, never counted as CAND's win", res["n"] == 19 and res["head"]["label"] == "UNRESOLVED-N"
              and "BASE-F2-DEAD65 10 dead RB row(s) over 24 RB slots: redrawn alive 0, dead twice 5, not redrawn 0" in txt)
    if name.startswith("C9"):
        check("C9: the redraw is recorded and the slot is scored", res["n"] == 24 and "(redrawn: first row BASE-F2-DEAD)" in txt)
    if name.startswith("C10"):
        check("C10: F2-FINDING65 names the row; 23 pairs", res["f2"] == 1 and res["n"] == 23 and "s7 r5 items [2]" in txt)
    print()

print("C11 an RC DNF is the worst rank on its side")
res, txt = run(ledger(eh=0.0, er=0.0, mods={("RC", "42", "2"): [row("RC", "42", "2", 500.0, 500.0, dnf=True)]}), show=False)
check("C11: the DNF pair reads d = +L (rc-dnf)", "(rc-dnf)" in txt)
print()
print(f"PATH EXERCISE {'PASS' if not fails else 'FAIL'} ({fails} failing checks)")
sys.exit(1 if fails else 0)
