#!/usr/bin/env python3
"""PATH EXERCISE, not a reading: tools/l1/s65_score.py run end to end before registration.

    python -I -X utf8 s65_path_exercise.py <tools/l1 dir> <docs/l1-raw/s64/s64-session.tgz>

  A. section 64's committed stage R rows (100 MB, about 12 s of ticks) with the tick floor passed as 0, so the code runs past VOID-SHORT; reproduction gate against
     s64-lean (6d55024c): RB q95 0.1930 over 200 splits; in-order 9/11, lag_E_cli 8/11, lag_E_srv 7/11, rtp_floor_p0 9/11 pairs RC worse;
  B. the same rows under the registered 30 s floor: every row VOID-SHORT;
  C. synthetic ledgers that reach every label and every slot rule.
Nothing here is a reading of F2: section 64's rows are 12 s rows and the synthetic rows are made up."""
import os
import re
import sys
import tarfile
import tempfile

TOOLS, TGZ = sys.argv[1], sys.argv[2]
sys.path.insert(0, os.path.abspath(TOOLS))
import place38_parse as pp  # noqa: E402
import plan64  # noqa: E402
import s65_score as s  # noqa: E402

fails = 0


def check(name, ok):
    global fails
    print(("PASS " if ok else "FAIL ") + name)
    fails += 0 if ok else 1


def run(rows, min_tick=s.MIN_TICK_S, show=True):
    buf = []
    res = s.score([], out=buf.append, min_tick=min_tick, rows=rows)
    if show:
        for x in buf:
            print("    " + x)
    return res, "\n".join(buf)


tmp = tempfile.mkdtemp()
with tarfile.open(TGZ) as t:
    t.extractall(tmp, filter="data")
rows64, _ = pp.rows_of([os.path.join(tmp, "session", "rt64", "ld.log")])

print("PATH EXERCISE (not a reading). A: section 64 stage R rows, tick floor passed as 0")
res, txt = run(rows64, min_tick=0.0)
worse = {m.group(1): (int(m.group(2)), int(m.group(3))) for m in re.finditer(r"^  (\S+)\s+RC worse in\s+(\d+) of (\d+)", txt, re.M)}
check("A: RESOLUTION65 reproduces s64-lean's RB q95 0.1930 over 200 splits (nRB 11)", "q95|x|=0.1930 (splits=200, nRB=11)" in txt)
check("A: 11 usable pairs, the dead RB s42 r4's pair dropped", res["n"] == 11 and "dropped: s42 r4" in txt)
check("A: in-order RC worse in 9 of 11 (TEST65)", "RC worse in 9;" in txt)
check("A: lag_E_cli 8/11, lag_E_srv 7/11, rtp_floor_p0 9/11 (s64-lean read 2)", worse.get("lag_E_cli") == (8, 11) and worse.get("lag_E_srv") == (7, 11) and worse.get("rtp_floor_p0") == (9, 11))
check("A: q95 0.193 > TOL, so NO-REGRESSION is unavailable; the label is not NO-REGRESSION", res["label"] != "NO-REGRESSION")
print()
print("B: the same rows under the registered 30 s tick floor")
res, txt = run(rows64, show=False)
check("B: every completed row is VOID-SHORT and the label is UNRESOLVED-N (0 pairs)", res["label"] == "UNRESOLVED-N" and res["n"] == 0 and txt.count(": VOID-SHORT") == 23)
print()

# C. synthetic ledgers
SLOTS = [(a, sd, rp) for _, _, a, sd, rp in plan64.plan()]


def row(arm, seed, rep, inorder, status=None, dnf=False, items=None):
    return {"cell": "c2-100", "store": "def", "arm": arm, "seed": seed, "rep": rep, "status": status or ("LIVE" if arm == "RB" else "RT-OK"), "dnf": dnf,
            "tick_span_s": 70.0, "bytes": 600000000, "inorder_mean": inorder, "inorder_mean_head": inorder, "mbps": 68.0, "cpu_cli": 80.0, "cpu_srv": 66.0,
            "cli_lag_E": 1300.0, "srv_lag_E": 800.0, "rtp_floor_p0": 10000, "rt_items": items, "bin_sha": "synthetic"}


def ledger(rb_noise, effect, rc_jitter, mods=None):
    """RB in-order = 120 ms x (1 + rb_noise x a fixed pattern); RC = RB x (1 + effect) x (1 + rc_jitter x another pattern). mods: {(arm, seed, rep): [rows]} replaces a slot's rows."""
    pat = [-1.0, 0.5, 1.0, -0.5, 0.0, 0.8, -0.8, 0.3, -0.3, 0.6, -0.6, 0.1]
    jit = [0.4, -1.0, 0.7, -0.2, 1.0, -0.6, 0.2, -0.9, 0.5, -0.4, 0.9, -0.7]
    out = []
    for i, (a, sd, rp) in enumerate(SLOTS):
        j = (int(rp) - 1) * 2 + (0 if sd == "42" else 1)
        rb = 120.0 * (1 + rb_noise * pat[j])
        v = rb if a == "RB" else rb * (1 + effect) * (1 + rc_jitter * jit[j])
        key = (a, sd, str(rp))
        out += (mods or {}).get(key) or [row(a, sd, rp, v)]
    return out


cases = [
    ("C1 no effect, quiet BASE", dict(rb_noise=0.02, effect=0.0, rc_jitter=0.005), None, "NO-REGRESSION"),
    ("C2 +15 % on every pair", dict(rb_noise=0.02, effect=0.15, rc_jitter=0.005), None, "REGRESSION"),
    ("C3 a resolved +3 % (bounded under TOL)", dict(rb_noise=0.02, effect=0.03, rc_jitter=0.005), None, "NO-REGRESSION"),
    ("C4 no effect, noisy pairs (+-20 %)", dict(rb_noise=0.02, effect=0.0, rc_jitter=0.20), None, "UNRESOLVED"),
    ("C5 RC = RB exactly but a noisy BASE (+-20 %): a noisier BASE never helps", dict(rb_noise=0.20, effect=0.0, rc_jitter=0.0), None, "UNRESOLVED"),
    ("C6 three RB slots dead twice: 9 pairs", dict(rb_noise=0.02, effect=0.0, rc_jitter=0.005),
     {("RB", sd, rp): [row("RB", sd, rp, None, "BASE-F2-DEAD"), row("RB", sd, rp, None, "BASE-F2-DEAD")] for sd, rp in (("42", "1"), ("7", "2"), ("42", "3"))}, "UNRESOLVED-N"),
    ("C7 a dead RB row redrawn LIVE takes its slot (12 pairs)", dict(rb_noise=0.02, effect=0.0, rc_jitter=0.005),
     {("RB", "42", "4"): [row("RB", "42", "4", None, "BASE-F2-DEAD"), row("RB", "42", "4", 120.0)]}, "NO-REGRESSION"),
    ("C8 an RC RT-FAIL row is an F2-FINDING and drops its pair", dict(rb_noise=0.02, effect=0.0, rc_jitter=0.005),
     {("RC", "7", "5"): [row("RC", "7", "5", 120.0, "RT-FAIL", items=[2])]}, "NO-REGRESSION"),
]
for name, kw, mods, want in cases:
    print(name)
    res, txt = run(ledger(mods=mods, **kw))
    check(f"{name}: label {want}", res["label"] == want)
    if name.startswith("C6"):
        check("C6: the three dead pairs are dropped, never counted as CAND's win", res["n"] == 9 and "REDRAWS BASE-F2-DEAD 3" in txt)
    if name.startswith("C7"):
        check("C7: the redraw is recorded and the slot is scored", res["n"] == 12 and "(redrawn: first row BASE-F2-DEAD)" in txt)
    if name.startswith("C2"):
        check("C2: the head mean carries the same +15 %, so HEAD-FLAG65 is RAISED", "HEAD-FLAG65 RAISED" in txt)
    if name.startswith("C1"):
        check("C1: HEAD-FLAG65 none", "HEAD-FLAG65 none" in txt)
    if name.startswith("C8"):
        check("C8: F2-FINDING65 names the row; 11 pairs", res["f2"] == 1 and res["n"] == 11 and "s7 r5 items [2]" in txt)
    print()

print("C9 an RC DNF is the worst rank on its side")
rows = ledger(rb_noise=0.02, effect=0.0, rc_jitter=0.005, mods={("RC", "42", "2"): [row("RC", "42", "2", 500.0, dnf=True)]})
res, txt = run(rows, show=False)
check("C9: the DNF pair reads d = +L (rc-dnf) and the bound is above the quiet case's", "(rc-dnf)" in txt and res["u_rel"] > 0.0)
print()
print(f"PATH EXERCISE {'PASS' if not fails else 'FAIL'} ({fails} failing checks)")
sys.exit(1 if fails else 0)
