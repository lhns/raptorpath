#!/usr/bin/env python3
"""PATH EXERCISE, not a reading: tools/l1/s65_score.py run end to end before registration.

    python -I -X utf8 s65_path_exercise.py <tools/l1 dir> <docs/l1-raw/s64/s64-session.tgz>

  A. section 64's committed stage R rows: the configuration section 65 repeats, at its 12-slot table (pair floor scaled 18/24 -> 9), the early-loss field computed from the rows'
     own client logs (s65_score.row_fields). Reproduction gate against s64-lean (6d55024c): RB q95 0.1930 over 200 splits; in-order 9/11, lag_E_cli 8/11, lag_E_srv 7/11,
     rtp_floor_p0 9/11 pairs RC worse. Its label is the source of the prediction, not evidence (registration section 5);
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


def run(rows, show=True, n_pairs=s.N_PAIRS, min_pairs=s.MIN_PAIRS):
    buf = []
    res = s.score([], out=buf.append, rows=rows, n_pairs=n_pairs, min_pairs=min_pairs)
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

print("PATH EXERCISE (not a reading). A: section 64 stage R rows, 12-slot table, pair floor 9")
res, txt = run(rows64, n_pairs=12, min_pairs=9)
worse = {m.group(1): (int(m.group(2)), int(m.group(3))) for m in re.finditer(r"^  (\S+)\s+RC worse in\s+(\d+) of (\d+)", txt, re.M)}
check("A: s64-lean's RB q95 0.1930 over 200 splits (nRB 11)", "q95|x|=0.1930 (splits=200, nRB=11)" in txt)
check("A: 11 usable pairs, the dead RB s42 r4's pair dropped; one dead row, not redrawn (section 64 never redrew it)", res["n"] == 11 and "dropped: s42 r4" in txt
      and "BASE-F2-DEAD65 1 dead RB row(s) over 12 RB slots: redrawn alive 0, dead twice 0, not redrawn 1" in txt)
check("A: in-order RC worse in 9 of 11", res["test"]["worse"] == 9 and res["test"]["n"] == 11)
check("A: lag_E_cli 8/11, lag_E_srv 7/11, rtp_floor_p0 9/11 (s64-lean read 2)", worse.get("lag_E_cli") == (8, 11) and worse.get("lag_E_srv") == (7, 11) and worse.get("rtp_floor_p0") == (9, 11))
check("A: the early-loss field is read on every row (both seats) and on the dead row", "s42r1 (RB first): RB 620/26726 | RC 663/26152" in txt and "s42r4 4/22" in txt)
print()

SLOTS = s.plan()
N = len(SLOTS) // 2
PAT = [((j * 7) % N) / (N - 1) * 2 - 1 for j in range(N)]
JIT = [((j * 11 + 5) % N) / (N - 1) * 2 - 1 for j in range(N)]


def row(arm, seed, rep, v, status=None, dnf=False, items=None):
    return {"cell": "c2-100", "store": "def", "arm": arm, "seed": seed, "rep": rep, "status": status or ("LIVE" if arm == "RB" else "RT-OK"), "dnf": dnf,
            "tick_span_s": 11.5, "bytes": 100000000, "inorder_mean": v, "mbps": 68.0, "cpu_cli": 13.0, "cpu_srv": 11.0,
            "cli_lag_E": 1300.0, "srv_lag_E": 800.0, "rtp_floor_p0": 10000, "qlost_2s": 600, "qsent_2s": 26000, "rt_items": items, "bin_sha": "synthetic"}


def ledger(e, j=0.005, rb_noise=0.02, mods=None):
    """RB = 120 ms x (1 + rb_noise x PAT); RC = RB x (1 + e) x (1 + j x JIT). mods: {(arm, seed, rep): [rows]} replaces a slot's rows."""
    out = []
    for a, sd, rp in SLOTS:
        k = (int(rp) - 1) * 2 + (0 if sd == "42" else 1)
        b = 120.0 * (1 + rb_noise * PAT[k])
        out += (mods or {}).get((a, sd, rp)) or [row(a, sd, rp, b if a == "RB" else b * (1 + e) * (1 + j * JIT[k]))]
    return out


DEAD7 = {("RB", sd, rp): [row("RB", sd, rp, None, "BASE-F2-DEAD"), row("RB", sd, rp, None, "BASE-F2-DEAD")]
         for sd, rp in (("42", "1"), ("7", "2"), ("42", "3"), ("7", "4"), ("42", "5"), ("7", "6"), ("42", "7"))}
cases = [
    ("C1 quiet null", dict(e=0.0), "NOT-REPLICATED"),
    ("C2 +25 % on every pair (the seen size)", dict(e=0.25), "REPLICATED"),
    ("C3 a resolved +3 % (p small, HL under TOL)", dict(e=0.03), "NOT-REPLICATED"),
    ("C4 STUB: pairs as noisy as section 64's (+-50 % uniform), no shift", dict(e=0.0, j=0.50), "NOT-REPLICATED"),
    ("C5 seven RB slots dead twice: 17 pairs < 18", dict(e=0.25, mods=DEAD7), "UNRESOLVED-N"),
    ("C6 a dead RB row redrawn LIVE takes its slot", dict(e=0.25, mods={("RB", "42", "4"): [row("RB", "42", "4", None, "BASE-F2-DEAD"), row("RB", "42", "4", 120.0)]}), "REPLICATED"),
    ("C7 an RC RT-FAIL row is an F2-FINDING and drops its pair", dict(e=0.0, mods={("RC", "7", "5"): [row("RC", "7", "5", 120.0, "RT-FAIL", items=[2])]}), "NOT-REPLICATED"),
]
for name, kw, want in cases:
    print(name)
    res, txt = run(ledger(**kw))
    check(f"{name}: label {want}", res["label"] == want)
    if name.startswith("C1"):
        check("C1: the bound U is printed as the cost still possible", "the cost still possible is U = +" in txt)
    if name.startswith("C4"):
        check("C4: U is wide (above TOL), so NOT-REPLICATED prints a large possible cost", res["test"]["u_rel"] > s.TOL)
    if name.startswith("C5"):
        check("C5: the seven dead pairs are dropped, never counted as CAND's win", res["n"] == 17
              and "BASE-F2-DEAD65 14 dead RB row(s) over 24 RB slots: redrawn alive 0, dead twice 7, not redrawn 0" in txt)
    if name.startswith("C6"):
        check("C6: the redraw is recorded and the slot is scored", res["n"] == 24 and "(redrawn: first row BASE-F2-DEAD)" in txt)
    if name.startswith("C7"):
        check("C7: F2-FINDING65 names the row; 23 pairs", res["f2"] == 1 and res["n"] == 23 and "s7 r5 items [2]" in txt)
    print()

print("C8 an RC DNF is the worst rank on its side")
res, txt = run(ledger(e=0.0, mods={("RC", "42", "2"): [row("RC", "42", "2", 500.0, dnf=True)]}), show=False)
check("C8: the DNF pair reads d = +L (rc-dnf)", "(rc-dnf)" in txt)
print()
print(f"PATH EXERCISE {'PASS' if not fails else 'FAIL'} ({fails} failing checks)")
sys.exit(1 if fails else 0)
