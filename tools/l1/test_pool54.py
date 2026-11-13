#!/usr/bin/env python3
"""Tests for the status 54 harness (plan 54's liveness witnesses and plan 54b's store-budget block: tools/l1/pool54_score.py, the place38_parse parts, the battery / run_all wiring).

Parser tests on lines copied from the committed section-53 logs (docs/l1-raw/s53/rep53-diag-ld.tgz: the BASES and CANDS c8-100 rows, the reference of the registration's re-basing rule),
synthetic [DIAG] lines with known answers for every definition of docs/derivations/p0-dwell-diag.md section 8 (each beside the reference script's output), then the liveness rules
and the rehearsal criteria, each with its red case first, the falsifiers (a DNF counted, not dropped), the plan table and the shell side. Run: python3 test_pool54.py

The end-to-end path through loopdiag_run_all.sh (plans 54 and 54b) runs in test_stubrun.py."""
import io
import json
import os
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
from contextlib import redirect_stdout

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
for _k in ("LD_PLAN", "LD_WITNESS54", "LD_SHA", "LD_SHA_BASE", "LD_BASE_SHA", "LD_ROW_BYTES"):
    os.environ.pop(_k, None)
import l1common as lc  # noqa: E402
import place38_parse as pp  # noqa: E402
import pool54_score as pl  # noqa: E402
import snap53_score as sn  # noqa: E402

fails = 0


def check(name, ok, detail=""):
    global fails
    print(("ok   " if ok else "FAIL ") + name + ((" -- " + str(detail)[:1500]) if not ok else ""))
    if not ok:
        fails += 1


# ── synthetic [DIAG] lines with known answers ──
def diag(t, src, win, cap, hand0, tx0, hand1, tx1, rtt0=200, rtt1=100, full0=0, full1=0, srcc=0, ack=0):
    """One client [DIAG] line in the engine's order. The `mpr[... fp/on p0:3/4 p1:5/6]` token and the `rtt=7/wrtt=7` decoy sit BEFORE the ` pN:infl=` blocks: a parse not anchored on
    ` pN:infl=` reads them."""
    return (f"[DIAG] t={t:.2f}s win={win}/{cap} paused=0% good=1Mbit src={src}sym/s cod=0sym/s cum={srcc}/0/{ack} cwnd=1 infl=1 np=2 np_act=2 rtt=15.8ms bdp100=1sym "
            f"mpr[rep=1 seqs=1 fired=1 supp=0/0/0 pf=0/1/2 fp/on p0:3/4 p1:5/6] wnd2=0/0 rtt=7/wrtt=7 "
            f"dgq0[hand={hand0} tx={tx0} full={full0} err=0 sp=1] dgq1[hand={hand1} tx={tx1} full={full1} err=0 sp=1] "
            f"p0:infl=1/sinfl=0/bdp1(cap0) sout=0 khr=1.00 btlbw=1 est=Y pl=0.0000 cmp=- rtt={rtt0}/wrtt={rtt0 // 2}/rtp3ms rtp_us=1 sig_us=1/n1 | ANCHOR sent=0 plu=0.0010 wpl_us=1 "
            f"p1:infl=1/sinfl=0/bdp1(cap0) sout=0 khr=1.00 btlbw=1 est=Y pl=0.0000 cmp=- rtt={rtt1}/wrtt={rtt1 // 2}/rtp3ms rtp_us=1 sig_us=1/n1 | ANCHOR sent=0 plu=0.0010 wpl_us=1")


def run_synth():
    """t = 5.0 .. 30.0 every 0.5 s: source until 28 s, cap 4096 (win 2000), p0 tx +8000/s, p1 +2000/s, rtt0 200 / rtt1 100; q0 = hand0 - tx0 = 500 for t < 20 and 700 after; the in-order
    ack lags the source by exactly 2 s of source (the source curve is 1000 sym/s)."""
    out = []
    for i in range(51):
        t = 5.0 + 0.5 * i
        s = int(1000 * (t - 5.0))                  # cumulative source symbols: 1000 sym/s from t = 5
        a = max(0, int(1000 * (t - 5.0 - 2.0)))    # ack = the source curve 2 s ago
        out.append(diag(t, 1000 if t <= 28 else 0, 2000, 4096, int(8000 * (t - 10)) + (500 if t < 20 else 700), int(8000 * (t - 10)), int(2000 * (t - 10)) + 5, int(2000 * (t - 10)),
                        srcc=s, ack=a, full0=3 if t > 29 else 0, full1=0))
    return out


SY = run_synth()
tk = pl.ticks(SY)
lt = pl.late(tk)
b = pl.late_budget(lt)
check("ticks: every [DIAG] line parses, the cap is the win= DENOMINATOR (4096) and the numerator is kept apart (2000)", len(tk) == 51 and tk[0]["cap"] == 4096 and tk[0]["win"] == 2000, tk[0])
check("ticks: per-path rtt is anchored on ` pN:infl=` (rtt0 200 / rtt1 100), not on the mpr[.. p0:..] token or the `rtt=7/wrtt=7` decoy", tk[0]["rtt"] == {0: 200, 1: 100} and tk[0]["wrtt"] == {0: 100, 1: 50}, tk[0])
check("the LATE window = t >= 15 s with src > 0: ticks at t = 15.0 .. 28.0 (27), the 28.5+ ticks (src 0) and everything before 15 s are out", len(lt) == 27 and lt[0]["t"] == 15.0 and lt[-1]["t"] == 28.0, (len(lt), lt[0]["t"], lt[-1]["t"]))
check("budget: r0 = 8000 /s, r1 = 2000 /s from the first to the last LATE tick; residual = the tick's OWN cap 4096 - (8000 x 200 + 2000 x 100) / 1000 = 2296 (budget2.py's win numerator would give 200)",
      b and abs(b["r"][0] - 8000) < 1e-6 and abs(b["r"][1] - 2000) < 1e-6 and abs(b["resid"] - 2296) < 1e-6 and b["cap"] == 4096, b)
check("q0 = dgq0 hand - tx over the LATE ticks: median of 500 (t < 20) / 700 (t >= 20) = 700 (17 of 27 ticks at 700)", b and b["q"][0] == 700 and b["q"][1] == 5, b and b["q"])
m = pl.row_metrics(SY, ["[RANK] holes=1 tail_overcount=0"], [], 600_000_000, 2048)
check("row_metrics: bind fraction = the share of LATE ticks with b = 2 x pool (4096 = 2 x 2048 -> 1.0; at pool 1024 -> 0.0)", m["bind_frac"] == 1.0 and pl.row_metrics(SY, [], [], 600_000_000, 1024)["bind_frac"] == 0.0, m["bind_frac"])
check("row_metrics: evictions are the LAST [DIAG] line's `full` (p0 3, p1 0) and hand - tx beside them (p0 700, p1 5)",
      (m["evict_full_p0"], m["evict_full_p1"], m["evict_full"], m["hand_minus_tx_p0"], m["hand_minus_tx_p1"]) == (3, 0, 3, 700, 5), (m["evict_full_p0"], m["evict_full"], m["hand_minus_tx_p0"]))
check("in-order delay: the ack curve lags the source curve by 2 s, so inorder_mean_run = inorder_mean_late = 2.0 s (p50 = p95 = 2.0); the late series is the sending ticks at t >= 15 s only",
      abs(m["inorder_mean_run"] - 2.0) < 0.02 and abs(m["inorder_mean_late"] - 2.0) < 0.02 and abs(m["inorder_p50_late"] - 2.0) < 0.02 and abs(m["inorder_p95_late"] - 2.0) < 0.02, (m["inorder_mean_run"], m["inorder_mean_late"]))
m2 = pl.row_metrics([re.sub(r"cum=(\d+)/0/(\d+)", lambda mo: f"cum={mo.group(1)}/0/{max(0, int(mo.group(2)) - 3000) if float(re.search(r't=([0-9.]+)s', ln).group(1)) < 14 else mo.group(2)}", ln) for ln in SY], [], [], 600_000_000, 2048)
check("in-order delay: the late window is NOT the whole run -- delaying the acks of the ticks before 14 s by 3 s of source moves inorder_mean_run (> 2.0) but not inorder_mean_late (2.0)",
      m2["inorder_mean_run"] > m["inorder_mean_run"] + 0.1 and abs(m2["inorder_mean_late"] - 2.0) < 0.02, (m2["inorder_mean_run"], m2["inorder_mean_late"]))
check("the quantile rule is inorder.py's index rule sorted[min(n-1, int(p*(n-1)+0.5))], not a linear interpolation: p50 of [1, 2] is 2 (lc.q gives 1.5), p95 of 1..10 is 10",
      pl.q_idx([1, 2], 0.5) == 2 and lc.q([1, 2], 0.5) == 1.5 and pl.q_idx(list(range(1, 11)), 0.95) == 10 and pl.q_idx([], 0.5) is None)
rk = [f"[RANK] holes={1000 + i} pivots=1 deficit=1 tail_overcount={i} max_deficit=1 empty=0 reports={i}" for i in range(1, 41)]
oh, nrk = pl.old_holes(rk)
check("old holes: median of (holes - tail_overcount) over the readouts[15:-6] -- here 1000 on each of the 19 readouts -- and the readout count", oh == 1000 and nrk == 40, (oh, nrk))
check("old holes: fewer than 22 readouts leave no window -> None (a short run is never a 0)", pl.old_holes(rk[:20])[0] is None and pl.old_holes([])[0] is None)
check("cap W, residual / W: the row's residual relative to its median cap", abs(m["resid_rel"] - 2296 / 4096) < 1e-9 and m["cap_med"] == 4096, (m["resid_rel"], m["cap_med"]))
check("missing fields read None, never raise: no [DIAG] at all -> every block field None", pl.row_metrics([], [], [], 600_000_000, 2048)["resid_late"] is None and pl.row_metrics([], [], [], 600_000_000, 2048)["old_holes"] is None)

# ── against the committed section-53 logs: the registration's own figures ──
TGZ = os.path.join(HERE, "..", "..", "docs", "l1-raw", "s53", "rep53-diag-ld.tgz")
if os.path.exists(TGZ):
    ref = pl.s53_rows(TGZ)
    R = {a: pl.ranges(ref[a]) for a in ref}
    check("s53 reference: 6 BASES and 6 CANDS c8 rows", {a: len(v) for a, v in ref.items()} == {"BASES": 6, "CANDS": 6}, {a: len(v) for a, v in ref.items()})
    near = lambda rng, lo, hi, tol: abs(rng[0] - lo) <= tol * abs(lo) + 1e-9 and abs(rng[1] - hi) <= tol * abs(hi) + 1e-9  # noqa: E731
    check("s53 BASES goodput 96.3-102.1, tail 2.63-5.44 s, duplicates 1 234-1 802 (the registration's BASES column: 96.3-102.1, 2.63-5.44, 1 230-1 810)",
          near(R["BASES"]["goodput_mbps"], 96.3, 102.1, 0.01) and near(R["BASES"]["tail_s"], 2.63, 5.44, 0.01) and near(R["BASES"]["rx_dup"], 1230, 1810, 0.01), R["BASES"])
    check("s53 CANDS goodput 107.2-108.3, tail 0.23-0.68 s, duplicates 1 101-1 487 (registered CANDS: 107.2-108.3, 0.23-0.68, 1 100-1 490), evictions 0-336 (registered 0-336)",
          near(R["CANDS"]["goodput_mbps"], 107.2, 108.3, 0.01) and near(R["CANDS"]["tail_s"], 0.23, 0.68, 0.02) and near(R["CANDS"]["rx_dup"], 1100, 1490, 0.01)
          and R["CANDS"]["evict_full"] == (0, 336), R["CANDS"])
    check("s53 whole-run in-order means: BASES median 4.54 s and CANDS 1.03 s (the registration's post hoc figures 4.50 and 1.02, whole-run mean, threadp1_parse.inorder_delay_stats)",
          abs(lc.med([m_["inorder_mean_run"] for m_ in ref["BASES"]]) - 4.50) < 0.1 and abs(lc.med([m_["inorder_mean_run"] for m_ in ref["CANDS"]]) - 1.02) < 0.05,
          (lc.med([m_["inorder_mean_run"] for m_ in ref["BASES"]]), lc.med([m_["inorder_mean_run"] for m_ in ref["CANDS"]])))
    check("s53 LATE-window readouts (the selector the registration uses for the block): BASES old holes 1 077-1 527, residual 1 274-1 771; CANDS old holes 0, residual 26-49; bind fraction >= 0.99 on every row",
          near(R["BASES"]["old_holes"], 1077, 1527, 0.01) and near(R["BASES"]["resid_late"], 1274, 1771, 0.01) and R["CANDS"]["old_holes"][1] <= 1 and 20 < R["CANDS"]["resid_late"][0] and R["CANDS"]["resid_late"][1] < 60
          and all(m_["bind_frac"] >= 0.99 for a in ref for m_ in ref[a]), (R["BASES"], R["CANDS"]))
    # the same rows under the REFERENCE scripts' PIN selector (cap == 4096, src > 0, t >= 2; old holes readouts[12:-6]; budget2.py's win numerator): the registered figures reproduce exactly
    with tempfile.TemporaryDirectory() as td, tarfile.open(TGZ) as tf:
        for mem in tf.getmembers():
            n = os.path.basename(mem.name)
            if mem.isfile() and n.startswith("c8-100@def-") and ("-BASES-" in n or "-CANDS-" in n):
                open(os.path.join(td, n), "wb").write(tf.extractfile(mem).read())
        pin = {"BASES": [], "CANDS": []}
        for n in sorted(os.listdir(td)):
            mm = re.match(r"c8-100@def-(\w+)-s\d+-r\d+-a1-c\.log$", n)
            if not mm:
                continue
            cli, srv = lc.read(os.path.join(td, n)), lc.read(os.path.join(td, n[:-6] + "-s.log"))
            tks = [x for x in pl.ticks(cli) if x["cap"] and x["cap"] >= 4096 and (x["src"] or 0) > 0 and x["t"] >= 2]
            bb = pl.late_budget(tks)
            rks = [x for x in srv if x.startswith("[RANK]")]
            old = lc.med([int(re.search(r"holes=(\d+)", x).group(1)) - int(re.search(r"tail_overcount=(\d+)", x).group(1)) for x in rks[12:-6]])
            res = lc.med([x["win"] - bb["r"][0] * x["rtt"][0] / 1000 - bb["r"][1] * x["rtt"][1] / 1000 for x in tks])
            pin[mm.group(1)].append((old, res))
        pr = {a: ((min(o for o, _ in v), max(o for o, _ in v)), (min(r for _, r in v), max(r for _, r in v))) for a, v in pin.items()}
        check("PIN selector reproduces the registered figures exactly: BASES old holes 1 089-1 510 and residual 1 257-1 733 (registered 1 089-1 510, 1 257-1 733); CANDS old holes 0-1",
              round(pr["BASES"][0][0]) == 1089 and round(pr["BASES"][0][1]) == 1510 and round(pr["BASES"][1][0]) == 1257 and round(pr["BASES"][1][1]) == 1733 and pr["CANDS"][0][1] <= 1, pr)
    # the guard's witnesses on the real lines: BASES (the filter on main): hn 11-409 (registered 11-409); BASE / CAND rows of s53 with the filter off sit at 9 786 / 214 482
    check("[HOLD] p0 hn_n of the filter-on rows: BASES 11-409 (the registered collapse), CANDS (lever B + the filter) 47 040-195 767: lever B rows never collapse, so the hn witness is plan 54's guard only",
          R["BASES"]["hn_p0"] == (11, 409) and R["CANDS"]["hn_p0"][0] > 10000, (R["BASES"]["hn_p0"], R["CANDS"]["hn_p0"]))
else:
    print("SKIP no docs/l1-raw/s53/rep53-diag-ld.tgz: the section-53 fixture tests need it")

# ── plan 54: the guard's witnesses ──
SV0 = "[SNAPV] site=sender p0:rep=900 stale=0 pt_st=0 tt_st=0 p1:rep=120 stale=100 pt_st=0 tt_st=0 p-:rep=7 stale=0 pt_st=0 tt_st=0 fa_class=0.0625"
HOLD_HN = "[HOLD] site=sender path=0 gen=0 samp_n=1000 fed=99 hn_n={n} hy_n=3 cx_n=7"
SHA_B, SHA_C = "b" * 64, "c" * 64
p, rd = pl.guard_witness("CAND", "c8-100", [HOLD_HN.format(n=120), SV0], SHA_C, SHA_C, SHA_B)
check("guard witnesses green: a CAND c8 row with the hn collapse (120 < 2000), [SNAPV] present with pt_st + tt_st = 0 and its own sha raises nothing", p == [] and rd == {"hn_p0": 120, "snapv_st": 0, "snapv_present": True, "sha_checked": True}, (p, rd))
p, _ = pl.guard_witness("CAND", "c8-100", [HOLD_HN.format(n=5000), SV0], SHA_C, SHA_C, SHA_B)
check("red: a CAND c8 row WITHOUT the hn collapse (5 000 >= 2 000) is VOID", any(x.startswith("hn-no-collapse") for x in p), p)
p, _ = pl.guard_witness("CAND", "c8-100", [HOLD_HN.format(n=1999), SV0], SHA_C, SHA_C, SHA_B)
p2, _ = pl.guard_witness("CAND", "c8-100", [HOLD_HN.format(n=2000), SV0], SHA_C, SHA_C, SHA_B)
check("the hn bound is strict at 2 000 (1 999 passes, 2 000 voids)", p == [] and any(x.startswith("hn-") for x in p2), (p, p2))
p, _ = pl.guard_witness("CAND", "c8-100", [SV0], SHA_C, SHA_C, SHA_B)
check("red: a CAND c8 row with no [HOLD] line cannot witness the collapse -> VOID", p == ["no-[HOLD]-hn-on-CAND-c8"], p)
p, _ = pl.guard_witness("CAND", "c1d-400", [HOLD_HN.format(n=90000), SV0], SHA_C, SHA_C, SHA_B)
check("the hn witness is c8 only: a CAND c1d row with hn 90 000 is fine", p == [], p)
p, _ = pl.guard_witness("CAND", "c1d-400", [SV0.replace("p1:rep=120 stale=100 pt_st=0", "p1:rep=120 stale=100 pt_st=2")], SHA_C, SHA_C, SHA_B)
check("red: [SNAPV] pt_st + tt_st = 2 on a CAND row (any cell) -> VOID", any(x.startswith("snapv-pt_st+tt_st=2") for x in p), p)
p, _ = pl.guard_witness("CAND", "c2-100", [], SHA_C, SHA_C, SHA_B)
check("red: no [SNAPV] on a CAND row (the gauge is not in the build) -> VOID", "no-[SNAPV]-on-CAND" in p, p)
p, _ = pl.guard_witness("BASE", "c8-100", [HOLD_HN.format(n=9786)], SHA_B, SHA_C, SHA_B)
check("a BASE row: hn 9 786 is reported, never a witness; no [SNAPV] expected", p == [], p)
p, _ = pl.guard_witness("BASE", "c8-100", [HOLD_HN.format(n=9786), SV0], SHA_B, SHA_C, SHA_B)
check("red: a [SNAPV] line on a BASE row (the trees are mixed up) -> VOID", len(p) == 1 and p[0].startswith("[SNAPV]-on-BASE"), p)
p, _ = pl.guard_witness("BASE", "c2-100", [], SHA_C, SHA_C, SHA_B)
p2, _ = pl.guard_witness("CAND", "c2-100", [SV0], SHA_B, SHA_C, SHA_B)
check("red: bin_sha matches the arm's seat -- a BASE row run on the CAND binary and a CAND row on the BASE binary are both VOID", len(p) == 1 and p[0].startswith("bin-sha=") and len(p2) == 1 and p2[0].startswith("bin-sha="), (p, p2))
check("red: a witness that cannot run is a problem, never a silent pass -- an unset seat sha (LD_SHA / LD_SHA_BASE not in the parser's env) or a row without a sha is bin-sha-uncheckable; arms of no seat are not checked",
      pl.sha_problems("CAND", SHA_C, None, SHA_B)[0].startswith("bin-sha-uncheckable") and pl.sha_problems("BASE", SHA_B, SHA_C, None)[0].startswith("bin-sha-uncheckable")
      and pl.sha_problems("CAND", None, SHA_C, SHA_B)[0].startswith("bin-sha-uncheckable") and pl.sha_problems("PWR", "x", SHA_C, SHA_B) == []
      and pl.sha_checked("CAND", SHA_C, SHA_C, SHA_B) and not pl.sha_checked("CAND", SHA_C, None, SHA_B) and not pl.sha_checked("PWR", "x", SHA_C, SHA_B))

# the row parser: witnesses apply to a LIVE completed row, never to a DNF row (it keeps its worst rank), never without LD_WITNESS54
CELL = "c8-100"


def mk_row(arm, cli_extra, dnf=False, plan="41d", w54="1", sha=SHA_C):
    """place38_parse.make_row on the committed c1 fixture logs + synthetic lines; env as run_all/battery hand it to the parser."""
    from stubrun_gen import FIX, PD
    fx = lambda x: lc.read(os.path.join(PD, FIX["dual"] + x))  # noqa: E731
    drv, cli, srv = fx("drv.out"), fx("c.log"), fx("s.log")
    if dnf:      # the row's completion state reads the CLIENT log's summary
        cli = [ln for ln in cli if '"summary"' not in ln and '"run":' not in ln] + ['{"bytes":600000000,"dnf":true,"hint":"bulk","proto":"rp-native","run":1}',
                                                                                    '{"bytes":600000000,"dnf":1,"hint":"bulk","proto":"rp-native","runs":1,"summary":true}']
    env = {"LD_PLAN": plan, "LD_WITNESS54": w54, "LD_SHA": SHA_C, "LD_SHA_BASE": SHA_B, "LD_BASE_SHA": "7968c870" + "0" * 32}
    old = {k: os.environ.get(k) for k in env}
    os.environ.update(env)
    try:
        return pp.make_row("c1d-400", arm, "42", 1, 0, 60, drv, cli + cli_extra, srv, 0, sha)
    finally:
        for k, v in old.items():
            if v is None:
                os.environ.pop(k, None)
            else:
                os.environ[k] = v


good = mk_row("CAND", [HOLD_HN.format(n=120), SV0])
check("the comparison executed and is recorded on the row (w54.sha_checked)", good.get("w54", {}).get("sha_checked") is True and mk_row("BASE", [HOLD_HN.format(n=9786)], sha=SHA_B).get("w54", {}).get("sha_checked") is True, good.get("w54"))
check("make_row, plan 41d + LD_WITNESS54=1: a CAND row with every witness holding stays LIVE and records its w54 readings", good["status"] in ("LIVE", "WITNESS-FAIL") and good.get("w54") == {"hn_p0": 120, "snapv_st": 0, "snapv_present": True, "sha_checked": True}
      and not any(x.startswith(("bin-sha", "no-[SNAPV]", "snapv-", "hn-")) for x in good["problems"]), (good["status"], good["problems"], good.get("w54")))
bad = mk_row("CAND", [HOLD_HN.format(n=120)])
check("red: make_row marks a completed CAND row without [SNAPV] WITNESS-FAIL (replaceable, re-run once by the replacement rule) when the gauge is absent",
      bad["status"] == "WITNESS-FAIL" and "no-[SNAPV]-on-CAND" in bad["problems"], (bad["status"], bad["problems"]))
dn = mk_row("CAND", [HOLD_HN.format(n=120)], dnf=True)
check("a DNF CAND row keeps its worst rank: the witnesses are not applied (status stays LIVE with dnf true), exactly as the v2 VOID-SHORT rule",
      dn.get("dnf") is True and dn["status"] == "LIVE" and "no-[SNAPV]-on-CAND" not in dn["problems"] and dn.get("w54", {}).get("snapv_present") is False, (dn["status"], dn.get("dnf"), dn["problems"]))
off = mk_row("CAND", [HOLD_HN.format(n=120)], w54="0")
check("without LD_WITNESS54 the row is plan 41d's, untouched: no w54 key, no witness problem", "w54" not in off and not any("SNAPV" in x for x in off["problems"]), (off["problems"], off.get("w54")))
wrong = mk_row("CAND", [HOLD_HN.format(n=120), SV0], sha=SHA_B)
check("red: a CAND row whose bin_sha is the BASE build is WITNESS-FAIL (bin-sha)", wrong["status"] == "WITNESS-FAIL" and any(x.startswith("bin-sha=") for x in wrong["problems"]), (wrong["status"], wrong["problems"]))

# ── plan 54b: the pool witness (a garbled request silently falls back to 2048) ──
G = "[GATES] RWM_STORE_PATHS=1 RWM_STORE_PATH_POOL={n} RWM_STORE=unset RWM_DIAG=1"
INFO = "INFO raptorpath::net: path-scaled outstanding pool ACTIVE (RWM_STORE_PATHS: cap = clamp(gain*bdp, floor, n_live*pool)) pool_per_path={n}"
ok_cli, ok_srv = [G.format(n=640), INFO.format(n=640)], [G.format(n=640), INFO.format(n=640)]
check("pool witness green: B640 with 640 in [GATES] on both ends and pool_per_path=640 on both -> no problem", pl.pool_problems("B640", ok_cli, ok_srv) == [], pl.pool_problems("B640", ok_cli, ok_srv))
check("pool witness: the controls expect 2048 (the default echoed, not absent): B2048 / L2048 with 2048 pass", pl.pool_problems("B2048", [G.format(n=2048), INFO.format(n=2048)], [G.format(n=2048)]) == []
      and pl.pool_problems("L2048", [G.format(n=2048), INFO.format(n=2048)], [G.format(n=2048), INFO.format(n=2048)]) == [])
fb = pl.pool_problems("B640", [G.format(n=2048), INFO.format(n=2048)], [G.format(n=2048), INFO.format(n=2048)])
check("red: B640 whose engine fell back to 2048 (a garbled value) fails on the [GATES] echo of both ends AND on pool_per_path", sum(x.startswith("gate-pool") for x in fb) == 2 and any(x.startswith("pool_per_path-cli") for x in fb), fb)
check("red: a missing sender INFO line (pool_per_path) fails; a server without the line is fine", any(x.startswith("pool_per_path-cli=None") for x in pl.pool_problems("B640", [G.format(n=640)], [G.format(n=640)]))
      and pl.pool_problems("B640", [G.format(n=640), INFO.format(n=640)], [G.format(n=640)]) == [])
check("red: an ambient 1024 leaking into an L640 row (echo 1024) fails", pl.pool_problems("L640", [G.format(n=1024), INFO.format(n=1024)], [G.format(n=1024), INFO.format(n=1024)]) != [])
check("red: no [GATES] echo at all fails on both ends", sum(x.startswith("gate-pool") for x in pl.pool_problems("B1024", [INFO.format(n=1024)], [])) == 2)
check("row_problems54b adds the binary seat: B* is the BASE seat, L* the CAND seat (a B row on the CAND sha, an L row on the BASE sha fail)",
      pl.row_problems54b("B640", ok_cli, ok_srv, SHA_B, SHA_C, SHA_B) == [] and pl.row_problems54b("L640", ok_cli, ok_srv, SHA_C, SHA_C, SHA_B) == []
      and any(x.startswith("bin-sha") for x in pl.row_problems54b("B640", ok_cli, ok_srv, SHA_C, SHA_C, SHA_B)) and any(x.startswith("bin-sha") for x in pl.row_problems54b("L640", ok_cli, ok_srv, SHA_B, SHA_C, SHA_B)))
check("the tail witness at W = 4096 is printed per row, never voided: B tail >= 2 s holds, L tail <= 0.8 s holds (1.4 s does not), reduced W -> None",
      pl.tail_witness("B2048", {"tail_s": 3.0}) is True and pl.tail_witness("L2048", {"tail_s": 0.4}) is True and pl.tail_witness("L2048", {"tail_s": 1.4}) is False and pl.tail_witness("B640", {"tail_s": 3.0}) is None)

# ── the plan table ──
tbl = pp.plan54b()
check("plan 54b: 36 rows, c8-100, six groups of six arms, each (arm, group) once; arms 6 x 6 rows", len(tbl) == 36 and {r[0] for r in tbl} == {"c8-100"} and {(r[3], r[4]) for r in tbl} == {(s, p_) for s in ("42", "7") for p_ in (1, 2, 3)}
      and all(sum(1 for r in tbl if r[2] == a) == 6 for a in pp.ARMS54B), len(tbl))
groups = [tbl[i:i + 6] for i in range(0, 36, 6)]
check("plan 54b: rotated per group -- every arm leads exactly one group and sits at every position once (a Latin square), no replacement", sorted(g[0][2] for g in groups) == sorted(pp.ARMS54B)
      and all(len({g[i][2] for g in groups}) == 6 for i in range(6)) and all(sorted(r[2] for r in g) == sorted(pp.ARMS54B) for g in groups))
check("plan 54b smoke: one row per arm (6)", [r[2] for r in pp.plan54b(smoke=True)] == list(pp.ARMS54B) and pp.expect_rows54b() == 36)
check("pool54_score's arm table equals place38_parse's (arm -> seat, requested pool) and the set-env map: the controls set nothing, the reduced arms set 1024 / 640",
      pl.ARM_POOL54 == pp.ARM_POOL54 and pl.ARMS54B == pp.ARMS54B and pl.ARM_SET_ENV == {"B2048": None, "B1024": 1024, "B640": 640, "L2048": None, "L1024": 1024, "L640": 640})

# ── the shell side ──
bat = open(os.path.join(HERE, "loopdiag_battery.sh"), encoding="utf-8").read()
ra = open(os.path.join(HERE, "loopdiag_run_all.sh"), encoding="utf-8").read()
lib = open(os.path.join(HERE, "lib.sh"), encoding="utf-8").read()
check("battery: the env scrub withholds RWM_STORE_PATH_POOL (an ambient value never leaks into a 'default' row) -- missing on main", "-u RWM_RECOV_SNAPVER -u RWM_STORE_PATH_POOL" in bat)
check("lib.sh forwards RWM_STORE_PATH_POOL to both ends (the knob reaches the server too)", re.search(r"RWM_FORWARD=\(.*RWM_STORE_PATH_POOL", lib, re.S) is not None)
bash = shutil.which("bash")
if bash:
    arms_sh = {}
    m_arm = re.search(r"B2048\|B1024\|B640\|L2048\|L1024\|L640\) .*?\n(.*?)    CAND\)", bat, re.S)
    check("battery: the arm table has the six arms; B* runs the BASE binary (RBIN=$BIN_BASE), 1024 / 640 set RWM_STORE_PATH_POOL, the controls set nothing",
          m_arm is not None and 'B*) RBIN="$BIN_BASE"; RWANT="$WANT_BASE"' in m_arm.group(1) and "*1024) arm_env=(RWM_STORE_PATH_POOL=1024)" in m_arm.group(1) and "*640) arm_env=(RWM_STORE_PATH_POOL=640)" in m_arm.group(1), m_arm and m_arm.group(1))
    fns = "\n".join(ln for ln in bat.splitlines() if re.match(r"p54b_bytes\(\)", ln))
    got = {c: subprocess.run([bash, "-c", fns + f"\np54b_bytes {c}"], capture_output=True, text=True).stdout.strip() for c in ("c8-100", "c1d-400")}
    check("battery p54b_bytes (the shell) == place38_parse's 600 MB at c8-100, nothing else", got == {"c8-100": "600000000", "c1d-400": ""} and pp.BYTES52 == 600000000, got)
check("battery: INSTR_ENV for plan 54b = plan 52's with RWM_PERF_TIMEOUT_S=300 (plan 54 / 41d keep 200) and RWM_DIAG=1 is on every row (the guard's rows too: the answer to the RWM_DIAG-in-guard question)",
      '[ "$P54B" = 1 ] && INSTR_ENV=(RWM_RTOBS=1 RWM_RDIAG=1 RWM_PERF_TIMEOUT_S=300)' in bat and "RWM_GEN=0 RWM_DIAG=1" in bat)
check("battery: plan 54's [SNAPV] preflight (CAND has the gauge, BASE does not) and plan 54b's (both builds, and the pool knob); W54 outside the plan-41d stage aborts",
      'if [ "$W54" = 1 ]; then' in bat and "has no [SNAPV] gauge: it does not contain the snapshot-order law" in bat and 'if [ "$P54B" = 1 ]; then' in bat and "ABORT-PLAN LD_WITNESS54=1 is defined for the plan-41d stage only" in bat)
check("run_all: LD_PLAN=54 IS plan 41d (PLAN rewritten before every branch) with W54=1 carried in SENV; plan 54b has its own stage rep54b",
      'W54=0; [ "$PLAN" = "54" ] && { W54=1; PLAN=41d; }' in ra and 'LD_WITNESS54="$W54"' in ra and "stage rep54b 54b" in ra and "smk=smoke54; scr=score54;" in ra)
check("run_all: plans 54 / 54b are accepted by the plan case, the rehearsal case and the harness table", "54b) P_SMOKE=smoke54b" in ra and "41c|41d|46|47|48|52|53|54b)" in ra and {"54", "54b"} <= set(__import__("harness_paths").HARNESS_PATHS))

# ── the rehearsal criteria and the report, on synthetic ledgers ──
def led_row(arm, seed=42, rep=1, status="LIVE", dnf=False, gp=100.0, tail=3.0, old=1200, resid=100, cap=4096, snapv=True, pool=None, mutate=None, rc=0, extra_problems=()):
    pool = pool or pl.ARM_POOL54[arm][1]
    t = {"goodput_mbps": None if dnf else gp, "tail_s": tail, "H0": 800, "lag_end": 1000, "rx_dup": 1300, "retx_per_gb": 5000, "cpu_cli_s_per_gb": 30.0, "cpu_srv_s_per_gb": 40.0,
         "tx_sum_late": 11800, "q0_late": 900, "rtt0_late": 200, "rtt0_minus_wrtt0": 90, "evict_full_p0": 0, "evict_full_p1": 0, "evict_full": 0, "hand_minus_tx_p0": 900, "hand_minus_tx_p1": 5,
         "inorder_mean_run": 4.5, "inorder_p95_run": 7.0, "inorder_mean_late": 5.0, "inorder_p50_late": 5.0, "inorder_p95_late": 7.0, "old_holes": old, "resid_late": resid, "resid_rel": resid / cap,
         "cap_med": cap, "bind_frac": 1.0, "lat_cov": 0.8, "hn_p0": 10, "snapv": {"0": {"rep": 9, "stale": 0, "pt_st": 0, "tt_st": 0}, "1": {"rep": 5, "stale": 3, "pt_st": 0, "tt_st": 0}} if snapv else None}
    if mutate:
        mutate(t)
    return {"cell": "c8-100", "store": "def", "arm": arm, "seed": str(seed), "rep": rep, "rc": rc, "status": status, "dnf": dnf, "mbps": None if dnf else gp, "seconds": None if dnf else 50.0,
            "summary": True, "bytes": 600000000, "wall_s": 55, "problems": list(extra_problems), "t54": t, "gate_pool_cli": str(pool), "gate_pool_srv": str(pool), "pool_info_cli": str(pool), "pool_info_srv": str(pool)}


def ledger(rows, tmp, name="ld.log", done=True):
    path = os.path.join(tmp, name)
    with open(path, "w", encoding="utf-8", newline="\n") as fh:
        fh.write("=== LOOP-DIAG BATTERY\n")
        for r in rows:
            fh.write("P38ROW " + json.dumps(r, sort_keys=True) + "\n")
        if done:
            fh.write("LD-BATTERY-DONE tag=ld blocks=1\n")
    return path


def run_smoke(rows, tmp):
    buf = io.StringIO()
    with redirect_stdout(buf):
        rc = pl.smoke(ledger(rows, tmp, "smoke.log"))
    return rc, buf.getvalue()


with tempfile.TemporaryDirectory() as tmp:
    green = [led_row(a) for a in pp.ARMS54B]
    rc, txt = run_smoke(green, tmp)
    check("smoke green: six arms, pools echoed, [SNAPV] present, nothing None, 600 MB rows inside 300 s -> SMOKE-PASS, every criterion a PASS line, none FAIL",
          rc == 0 and "SMOKE-PASS" in txt and txt.count("\nPASS ") == 6 and "FAIL" not in txt, txt[-900:])
    check("smoke prints the registered criteria by name", all(x in txt for x in ("one row per block arm ran", "each arm's pool is in [GATES] on both ends", "[SNAPV] parsed on every row", "in-order delay (run and late), evictions, old holes and the budget residual",
                                                                           "600 MB rows finished inside RWM_PERF_TIMEOUT_S=300")), txt[:500])
    rc, txt = run_smoke(green[:5], tmp)
    check("red: a missing arm row -> FAIL one row per block arm ran, rc 1", rc == 1 and re.search(r"^FAIL one row per block arm ran: L640: no row", txt, re.M), txt[-400:])
    rows = [dict(r) for r in green]
    rows[2] = led_row("B640", extra_problems=["gate-pool-cli=2048 (want 640)", "pool_per_path-cli=2048 (want 640)"], status="WITNESS-FAIL")
    rc, txt = run_smoke(rows, tmp)
    check("red: B640 echoing 2048 -> FAIL the pool criterion, rc 1", rc == 1 and re.search(r"^FAIL each arm's pool is in \[GATES\]", txt, re.M), txt[-500:])
    rows = [led_row(a, snapv=(a != "L1024")) for a in pp.ARMS54B]
    rc, txt = run_smoke(rows, tmp)
    check("red: no [SNAPV] on one row -> FAIL [SNAPV] parsed", rc == 1 and re.search(r"^FAIL \[SNAPV\] parsed on every row", txt, re.M), txt[-500:])
    for fld in ("inorder_mean_late", "old_holes", "resid_late", "evict_full_p0", "inorder_mean_run"):
        rows = [led_row(a, mutate=(lambda t, f=fld: t.update({f: None})) if a == "L1024" else None) for a in pp.ARMS54B]
        rc, txt = run_smoke(rows, tmp)
        check(f"red: {fld} None on a row -> FAIL the no-None criterion naming it", rc == 1 and re.search(r"^FAIL in-order delay .*" + fld, txt, re.M), txt[-400:])
    rows = [led_row(a, dnf=(a == "L2048")) for a in pp.ARMS54B]
    rc, txt = run_smoke(rows, tmp)
    check("red: a DNF L2048 row -> FAIL the 300 s criterion", rc == 1 and re.search(r"^FAIL the c8 600 MB rows finished inside", txt, re.M), txt[-400:])
    rows = [led_row(a, dnf=(a == "B640"), rc=(1 if a == "B640" else 0)) for a in pp.ARMS54B]
    rc, txt = run_smoke(rows, tmp)
    check("red: a B640 crash (rc 1) fails the 300 s criterion", rc == 1 and re.search(r"^FAIL the c8 600 MB rows finished inside", txt, re.M), txt[-400:])
    rows = [led_row(a, dnf=(a == "B640")) for a in pp.ARMS54B]
    rc, txt = run_smoke(rows, tmp)
    check("B640 gets no exemption at 300 s: a clean B640 DNF in the smoke FAILS the criterion (no NOTE)", rc == 1 and "NOTE" not in txt and re.search(r"^FAIL the c8 600 MB rows finished inside RWM_PERF_TIMEOUT_S=300", txt, re.M), txt[-500:])
    rows = [led_row(a, gp=22.0) for a in pp.ARMS54B]
    rows[2]["seconds"] = 218.0
    rc, txt = run_smoke(rows, tmp)
    check("B640 finishing at 218 s (20 Mbit/s floor, under the H-hold reading) passes inside 300 s", rc == 0 and "SMOKE-PASS" in txt, txt[-500:])

    # the report
    def arm_rows(gp, old, resid=50, dnf_n=0, cap=4096):
        out = {}
        for a, g in gp.items():
            out[a] = [led_row(a, seed=s_, rep=r_, gp=g, old=old.get(a, 0), resid=resid, dnf=(i < dnf_n and a == "B640")) for i, (s_, r_) in enumerate([(42, 1), (7, 1), (42, 2), (7, 2), (42, 3), (7, 3)])]
        return out
    GP = {"B2048": 98.0, "B1024": 98.0, "B640": 50.0, "L2048": 108.0, "L1024": 108.0, "L640": 107.5}
    OLD = {"B2048": 1300, "B1024": 1000, "B640": 900, "L2048": 0, "L1024": 0, "L640": 0}
    by = arm_rows(GP, OLD)
    fal = pl.falsifiers(by)
    st = {i: s for i, _, s, _ in fal}
    check("falsifiers, mechanism-confirmed shape: B640 half of B2048 with 900 old holes, L640 = L2048 -> F1 / F2 / F3 / F4 all NOT-FIRED, decision rule 'confirmed'",
          st == {1: "NOT-FIRED", 2: "NOT-FIRED", 3: "NOT-FIRED", 4: "NOT-FIRED"} and pl.decision_rule(fal, by).startswith("confirmed"), (st, pl.decision_rule(fal, by)))
    GP2 = dict(GP, B640=97.0)
    fal = pl.falsifiers(arm_rows(GP2, OLD))
    check("F1 fires: B640 goodput 97 >= 0.95 x 98 with old holes 900 >= 700 -> refuted", {i: s for i, _, s, _ in fal}[1] == "FIRES" and {i: s for i, _, s, _ in fal}[2] == "NOT-FIRED"
          and pl.decision_rule(fal, arm_rows(GP2, OLD)).startswith("refuted"), [(i, s) for i, _, s, _ in fal])
    OLD2 = dict(OLD, B640=250)
    fal = pl.falsifiers(arm_rows(GP2, OLD2))
    check("F2 fires: B640 goodput >= 0.95 x B2048 with old holes 250 <= 300 -> H-elastic, F1 not fired", {i: s for i, _, s, _ in fal}[2] == "FIRES" and {i: s for i, _, s, _ in fal}[1] == "NOT-FIRED"
          and pl.decision_rule(fal, arm_rows(GP2, OLD2)).startswith("H-elastic"), [(i, s) for i, _, s, _ in fal])
    OLD3 = dict(OLD, B640=500)
    check("between the bands (old holes 500, goodput held): neither F1 nor F2 fires", all(s == "NOT-FIRED" for i, _, s, _ in pl.falsifiers(arm_rows(GP2, OLD3)) if i in (1, 2)))
    fal = pl.falsifiers(arm_rows(dict(GP, L640=104.0), OLD))
    check("F3 fires: L640 goodput 104 < 0.97 x 108 = 104.76", {i: s for i, _, s, _ in fal}[3] == "FIRES", [(i, s, d) for i, _, s, d in fal])
    fal = pl.falsifiers(arm_rows(GP, OLD, resid=500))
    check("F4 fires: |residual| 500 > 0.1 x 4096 = 409.6 on the L rows (and the B rows are not read by it)", {i: s for i, _, s, _ in fal}[4] == "FIRES" and "L2048" in [d for i, _, s, d in fal if i == 4][0], [(i, s, d) for i, _, s, d in fal])
    fal = pl.falsifiers(arm_rows(GP, OLD, resid=-500))
    check("F4 reads the ABSOLUTE residual (-500 fires too)", {i: s for i, _, s, _ in fal}[4] == "FIRES")
    # a DNF counts as goodput 0, never dropped: 4 of 6 B640 rows DNF -> its median (incl. DNF as 0) is 0, not the survivors' 97
    byd = arm_rows(GP2, OLD, dnf_n=4)
    gp_b640 = lc.med(pl.goodput_dnf0(byd["B640"]))
    fal = pl.falsifiers(byd)
    check("DNF rows are counted: B640 with 4 DNF of 6 reads goodput median 0 (not the survivors' 97), so F1 / F2 cannot fire on survivors only", gp_b640 == 0.0 and {i: s for i, _, s, _ in fal}[1] == "NOT-FIRED", (gp_b640, [(i, s) for i, _, s, _ in fal]))
    check("falsifiers on an empty ledger are UNDEFINED, not silently NOT-FIRED", all(s == "UNDEFINED" for _, _, s, _ in pl.falsifiers({a: [] for a in pp.ARMS54B})))

    # score(): the whole report on a full synthetic ledger, with the re-basing check against the s53 logs
    allrows = []
    for g, (seed, rep) in enumerate(pp.BLOCKS52):
        for a in pp._rotate(pp.ARMS54B, g):
            allrows.append(led_row(a, seed=seed, rep=rep, gp=GP[a], old=OLD[a], resid=50))
    lp = ledger(allrows, tmp)
    buf = io.StringIO()
    with redirect_stdout(buf):
        pl.score([lp], s53_src=TGZ if os.path.exists(TGZ) else None)
    sc = buf.getvalue()
    check("score: ROWS 36/36, every arm's readouts (goodput, tail, H0, duplicates, retx/GB, CPU/GB, q0, rtt0, evictions, hand-tx, inorder run and late, old holes, residual, bind fraction, [SNAPV]) printed",
          "ROWS 36/36" in sc and all(x in sc for x in ("goodput Mbit/s", "completion tail s", "H0 holes", "receiver duplicates", "retransmits per GB", "client CPU s/GB", "server CPU s/GB", "p0 backlog q0", "p0 app-echo rtt0",
                                                      "quinn evictions dgq0 full (last [DIAG])", "dgq0 hand - tx (last [DIAG])", "inorder_mean_run", "inorder_mean_late", "inorder_p95_late", "lag", "old holes",
                                                      "budget residual", "cap bind fraction", "[SNAPV] p1 stale", "[LAT] coverage")), sc[:600])
    check("score: the four falsifiers and the decision-rule reading are printed; the verdict line is REPORTED-ONLY", all(f"F{i} " in sc for i in (1, 2, 3, 4)) and "DECISION-RULE reading" in sc and "VERDICT54B REPORTED-ONLY rows=36/36 live=36" in sc, sc[-700:])
    if os.path.exists(TGZ):
        check("score: the re-basing check is printed; controls shaped like section 53's BASES / CANDS are not outside their ranges on goodput / tail / old holes / duplicates only when they really are inside",
              "REBASE B2048 against section 53's BASES" in sc and "REBASE L2048 against section 53's CANDS" in sc and "REBASE-NEEDED:" in sc, sc[-1500:])
        ctl = [led_row("B2048", gp=98.0, old=1300, tail=3.0), led_row("L2048", gp=108.0, old=0, tail=0.4)]
        for r in ctl:
            r["t54"]["rx_dup"] = 1300 if r["arm"] == "B2048" else 1300
        buf = io.StringIO()
        by_ = {"B2048": [ctl[0]], "L2048": [ctl[1]]}
        with redirect_stdout(buf):
            need = pl.rebase_check(by_, TGZ, print)
        check("rebase: controls inside BASES' / CANDS' ranges -> REBASE-NEEDED: NO", need is False and "REBASE-NEEDED: NO" in buf.getvalue(), buf.getvalue())
        out = io.StringIO()
        by_ = {"B2048": [led_row("B2048", gp=80.0, old=1300)], "L2048": [led_row("L2048", gp=108.0, old=0, tail=0.4)]}
        with redirect_stdout(out):
            need = pl.rebase_check(by_, TGZ, print)
        check("rebase: a B2048 row at 80 Mbit/s (BASES: 96.3-102.1) is OUTSIDE -> REBASE-NEEDED: YES, naming the row and the readout", need is True and "OUTSIDE B2048" in out.getvalue() and "goodput 80.00" in out.getvalue(), out.getvalue())
        out = io.StringIO()
        with redirect_stdout(out):
            need = pl.rebase_check({"B2048": [], "L2048": []}, os.path.join(tmp, "absent.tgz"), print)
        check("rebase: no section-53 logs -> REBASE-CHECK UNAVAILABLE (None), never a silent NO", need is None and "REBASE-CHECK UNAVAILABLE" in out.getvalue(), out.getvalue())

    # a ledger with a void row: it is listed, does not enter the readouts
    rows = [dict(r) for r in allrows]
    rows[0] = dict(rows[0], status="WITNESS-FAIL", problems=["gate-pool-cli=2048 (want 640)"])
    lp = ledger(rows, tmp, "void.log")
    buf = io.StringIO()
    with redirect_stdout(buf):
        pl.score([lp], s53_src=None)
    check("score: a WITNESS-FAIL row is listed (NONLIVE ... gate-pool) and leaves the readouts: live = 35", "NONLIVE" in buf.getvalue() and "gate-pool-cli=2048" in buf.getvalue() and "live=35" in buf.getvalue(), buf.getvalue()[-900:])

    # the dispatch through loopdiag_parse.py, as run_all calls it
    lp = ledger(allrows, tmp, "ld54b.log")
    cp = {c: subprocess.run([sys.executable, os.path.join(HERE, "loopdiag_parse.py"), c, lp], capture_output=True, text=True, errors="replace") for c in ("check54b", "score54b", "smoke54b")}
    check("loopdiag_parse dispatches check54b (36 rows), score54b (the report) and smoke54b (rc 1: 36 rows are not the smoke's 6, so the ran-criteria pass but the table size is read from the arms)",
          cp["check54b"].returncode == 0 and "rows=36/36" in cp["check54b"].stdout and cp["score54b"].returncode == 0 and "VERDICT54B" in cp["score54b"].stdout, (cp["check54b"].stdout, cp["score54b"].stderr[-300:]))
    plan_out = subprocess.run([sys.executable, os.path.join(HERE, "place38_parse.py"), "plan54b"], capture_output=True, text=True).stdout.split("\n")
    check("place38_parse plan54b prints the 36-row table the battery reads", len([x for x in plan_out if x]) == 36 and plan_out[0].startswith("c8-100 def B2048 42 1"), plan_out[:2])

print("FAILED %d" % fails if fails else "ALL OK")
sys.exit(1 if fails else 0)
