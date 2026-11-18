#!/usr/bin/env python3
"""Tests for status 62's scorer and tables (tools/l1/plan62.py, s62_score.py; registration docs/derivations/s62-registration.md section 8). The harness side is test_harness62.py, the launch paths run in test_stubrun.py.

  1. the pilot: every row of section 3.2's table (u80 -> W-hat, K1 / K2 / K3, P1) and every case, the cap = p_hi, the clamp, the re-probe's probes, arms(float(printed W)) == the printed arms;
  2. the label function: every branch of section 1.3's table (CONTROL-MOVED, UNSCOREABLE x3, BELOW, ABOVE, UNRESOLVED, FLOOR-SENSITIVE), on a scorer run with a KNOWN answer;
  3. CR62: RULE-FINDING-CR62 at full goodput (not a collapse), the goodput cut on every clause, D rows excluded from every median and from CR62's reference;
  4. every rehearsal go/no-go rule, each NO-GO separately, the re-probe pairs, ABORT-NETEM's witness;
  5. HOST (skipped without the three ledgers: S62_RAW = a docs/l1-raw): the scorer reproduces s62pred.out's numbers on the 61c / 61a / c1s ledgers.
Run: python3 test_s62.py"""
import os
import random
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
for _k in ("LD_PLAN", "LD_KNEE62", "LD_CTL62", "LD_REHEARSE", "LD_NOGO"):
    os.environ.pop(_k, None)
import l1common as lc  # noqa: E402
import plan62  # noqa: E402
import s62_score as s  # noqa: E402

fails = 0


def check(name, ok, detail=""):
    global fails
    print(("ok   " if ok else "FAIL ") + name + ((" -- " + str(detail)[:1800]) if not ok else ""))
    if not ok:
        fails += 1


def near(a, b, tol=1e-9):
    return a is not None and abs(a - b) <= tol


# ═══ 1. the pilot (section 3.2) ═══
TABLE = ((0.78, 97.4, [88, 104, 116], 144), (0.82, 92.7, [84, 100, 112], 136), (0.86, 88.4, [80, 92, 108], 136), (0.90, 84.4, [76, 88, 100], 128), (0.93, 81.7, [76, 88, 100], 120))
for u, w_hat, k, p1 in TABLE:
    d = plan62.pilot_detail(u, 1.0)
    check(f"section 3.2's table: u80 {u} -> W-hat {w_hat}, K {k}, P1 {p1}, case proportional", d is not None and d[0] == w_hat and d[1] == "proportional" and plan62.arms(w_hat) == {"K": k, "P1": p1}, (d, plan62.arms(w_hat)))
    check(f"arms(float(printed W-hat {w_hat})) is the printed arms (the arms derive from the printed value)", plan62.arms(float(f"{d[0]:.1f}")) == plan62.arms(w_hat))
check("the cap is p_hi: u80 0.5 would read 80 * 0.95 / 0.5 = 152 and caps at 128; with probes 112 / 160 it caps at 160 (u 0.5 -> 212 -> 160)", plan62.pilot_detail(0.5, 1.0)[0] == 128.0 and plan62.pilot_detail(0.5, 1.0, 112, 160)[0] == 160.0)
check("every case: proportional (0.8 / 1.0), secant (both below 0.95, u_hi > u_lo: 128 + (0.95 - u_hi) * 48 / (u_hi - u_lo)), degenerate (u_hi <= u_lo), both-above (88... 80 * 0.95 / u_lo), non-monotone (u_lo >= 0.95 > u_hi)",
      plan62.pilot_detail(0.8, 1.0)[1] == "proportional" and plan62.pilot_detail(0.6, 0.9)[1] == "secant" and near(plan62.pilot_detail(0.6, 0.9)[0], 128 + 0.05 * 48 / 0.3, 0.05)
      and plan62.pilot_detail(0.9, 0.8)[1] == "degenerate" and near(plan62.pilot_detail(0.9, 0.8)[0], 128 * 0.95 / 0.8, 0.05) and plan62.pilot_detail(1.0, 1.0)[1] == "both-above" and near(plan62.pilot_detail(1.0, 1.0)[0], 76.0, 0.05)
      and plan62.pilot_detail(1.0, 0.9)[1] == "non-monotone" and near(plan62.pilot_detail(1.0, 0.9)[0], 76.0, 0.05))
check("the clamp [48, 192] and the 0.1 rounding after it; a utilisation <= 0 is None (no division)", plan62.pilot_detail(1.0, 1.2)[0] == 76.0 and plan62.pilot_detail(2.0, 2.0)[0] == 48.0 and plan62.pilot_detail(2.0, 2.0)[2] is True
      and plan62.pilot_detail(0.0, 1.0) is None and plan62.pilot_detail(1.0, 0.0) is None and plan62.pilot_detail(0.3, 1.0, 80, 192)[0] == 192.0)
check("the re-probe's probes: both below 0.95 -> 112 / 160, both at or above -> 64 / 96, a mixed (non-monotone) pair has none", plan62.reprobe_pair(0.5, 0.9) == (112, 160) and plan62.reprobe_pair(1.0, 1.1) == (64, 96) and plan62.reprobe_pair(1.0, 0.9) is None and plan62.reprobe_pair(0.8, 1.0) is None)
check("the rule applies with the re-probe's p_lo / p_hi: probes 112 / 160 with u 0.8 / 1.0 read 112 * 0.95 / 0.8 = 133.0 proportional; probes 64 / 96 with u 0.9 / 1.0 read 67.6", near(plan62.pilot_detail(0.8, 1.0, 112, 160)[0], 133.0, 0.05) and near(plan62.pilot_detail(0.9, 1.0, 64, 96)[0], 67.6, 0.05))
check("arms: K = r4(0.92 / 1.06 / 1.20 x W-hat), P1 = r8(1.5 x W-hat), clamped to [48, 192]; the 7 grid arms are distinct over every W-hat in [48, 192] (0.1 steps)",
      plan62.arms(100.0) == {"K": [92, 108, 120], "P1": 152} and plan62.arms(10) == plan62.arms(48) and plan62.arms(999) == plan62.arms(192) and all(len(set(plan62.grid_ws(k / 10.0))) == 7 for k in range(480, 1921)))

# ═══ the tables ═══
os.environ["LD_KNEE62"] = "97.4"
A, R = plan62.plan("62a"), plan62.plan("62r0")
check("62a is 44 slots (7 arms x 6 replicates + 2 dump rows), 62r0 is 7, the smoke is the middle knee arm K2 (G104)", (len(A), len(R)) == (44, 7) and plan62.plan("62a", smoke=True) == [(plan62.C3J0, "def", "G104", "42", 1)])
check("62a: floor 12 / 24, K1 / K2 / K3, P1, control 512, six replicates each (a cyclic Latin square, seeds 42 / 7 alternating)", all(sum(1 for r in A if r[2] == "G%d" % w) == 6 for w in (12, 24, 88, 104, 116, 144, 512)))
check("62a: the first replicate runs G12 first, the second rotates by one (a cyclic Latin square) and the seeds alternate per replicate", [r[2] for r in A[:3]] == ["G12", "G24", "G88"] and A[7][2] == "G24" and [A[7 * k][3] for k in range(6)] == ["42", "7", "42", "7", "42", "7"])
check("62a's dump rows: D12 (seed 42, rep 7) and D104 (seed 7, rep 8) after the 42 scored slots", A[42] == (plan62.C3J0, "def", "D12", "42", 7) and A[43] == (plan62.C3J0, "def", "D104", "7", 8) and len({(r[2], r[3], r[4]) for r in A}) == 44)
check("62r0 is R1 G512, R2 / R3 G12 (seeds 42 / 7), R4 / R5 the probes G80 / G128, R6 G512 (seed 7), D1 D12; every slot unique", [r[2] for r in R] == ["G512", "G12", "G12", "G80", "G128", "G512", "D12"] and len({(r[2], r[3], r[4]) for r in R}) == 7)
del os.environ["LD_KNEE62"]
try:
    plan62.plan("62a")
    raised = False
except ValueError:
    raised = True
check("62a without LD_KNEE62 raises (ABORT-PILOT)", raised)
os.environ["LD_KNEE62"] = "300"
try:
    plan62.plan("62a")
    raised = False
except ValueError:
    raised = True
check("LD_KNEE62 = 300 is refused (outside the clamp)", raised)
del os.environ["LD_KNEE62"]
os.environ["LD_KNEE62"] = "97.4"
spec = {a: plan62.arm_bytes(plan62.C3J0, a) for a in ("G12", "G24", "G80", "G128", "G88", "G104", "G144", "G512", "D12", "D104")}
check("bytes: W 12 25 MB, W 24 30 MB (both 150 s), 120 MB at the control and at and above K1 (88), the probes 100 / 115 MB by 61a's formula only below K1... W 80 is below K1 (88) -> the formula",
      spec["G12"] == 25_000_000 and spec["G24"] == 30_000_000 and spec["G512"] == 120_000_000 and spec["G88"] == 120_000_000 and spec["G104"] == 120_000_000 and spec["G144"] == 120_000_000 and spec["D104"] == 120_000_000
      and spec["D12"] == 25_000_000 and spec["G80"] == 100_000_000 and spec["G128"] == 120_000_000, spec)
check("timeouts: 150 s on the floor arms (W 12 / 24), 120 s on every other arm; hint bulk", [plan62.arm_timeout(plan62.C3J0, a) for a in ("G12", "G24", "D12", "G104", "G512", "G80")] == [150, 150, 150, 120, 120, 120] and plan62.arm_hint("G12") == "bulk")
check("env: RWM_SLAW=1 first, RWM_RTOBS=1, the pool knob (W, not at 2 048), RWM_RTT_DUMP=1 only on a D arm", plan62.arm_env(plan62.C3J0, "G12") == ["RWM_SLAW=1", "RWM_RTOBS=1", "RWM_STORE_PATH_POOL=12"]
      and plan62.arm_env(plan62.C3J0, "D104") == ["RWM_SLAW=1", "RWM_RTOBS=1", "RWM_STORE_PATH_POOL=104", "RWM_RTT_DUMP=1"] and "RWM_RTT_DUMP=1" not in plan62.arm_env(plan62.C3J0, "G512"))
del os.environ["LD_KNEE62"]
check("valid slots: G / D arms of the registered W only, only at c3j0-25; the control is never a D arm; every arm the pilot can place is valid (0.1 steps)",
      plan62.valid_slot(plan62.C3J0, "G12") and plan62.valid_slot(plan62.C3J0, "D12") and not plan62.valid_slot("c3-25", "G12") and not plan62.valid_slot(plan62.C3J0, "D512") and not plan62.valid_slot(plan62.C3J0, "G13")
      and all(plan62.valid_slot(plan62.C3J0, "G%d" % w) for k in range(480, 1921) for w in plan62.grid_ws(k / 10.0)) and all(plan62.valid_slot(plan62.C3J0, "G%d" % w) for w in (64, 96, 112, 160)))


# ═══ synthetic rows with a KNOWN answer ═══
def win(n, tmin_of, nk=60, dp50=5.0):
    return [(15.0 + i, nk, tmin_of(i), tmin_of(i) + 1.0, tmin_of(i) + dp50, tmin_of(i) + dp50 + 2.0) for i in range(n)]


def mk(arm, seed, rep, gp, wins, rtt=(), qshare=0.0, loss=0.048, dnf=False, status="LIVE", t62=None):
    cols = list(zip(*wins)) if wins else [[], [], [], [], [], []]
    t61 = {"v": 1, "win": dict(zip(("t", "nk", "tmin", "p10", "p50", "p90"), (list(c) for c in cols))), "rtt": list(rtt), "qshare": qshare, "loss": loss, "n_ticks": len(rtt)}
    return s.Row({"cell": plan62.C3J0, "arm": arm, "seed": str(seed), "rep": rep, "status": status, "dnf": dnf, "mbps": gp, "t60": {"x": 1}, "t61": t61, "t62": t62})


CTL, W95 = 17.8, 100.0
KARMS, P1 = (88, 104, 116), 144


def gp_of(w, ctl=CTL):
    return ctl * min(1.0, 0.95 * w / W95)


def cell_rows(f12=41.0, f24=41.1, need=lambda rep: 0.4, n_ctl=6, rtt_floor=(41.5,) * 40, rtt_ctl=(61.0,) * 40, ctl_gp=CTL, dip=False, kw=KARMS, p1=P1, ctl_loss=0.048):
    rows = []
    for rep in range(1, 7):
        seed = 42 if rep % 2 else 7
        f12_t = (lambda i: 38.0 if (dip and i % 10 == 0) else f12 + (1.0 if dip else 0.0))
        rows.append(mk("G12", seed, rep, gp_of(12), win(120, f12_t), rtt_floor))
        rows.append(mk("G24", seed, rep, gp_of(24), win(120, lambda i: f24), rtt_floor))
        for w in kw + (p1,):
            rows.append(mk("G%d" % w, seed, rep, gp_of(w), win(120, lambda i: f12 + need(rep)), (f12 + need(rep) + 0.5,) * 40))
        if rep <= n_ctl:
            rows.append(mk("G512", seed, rep, ctl_gp, win(120, lambda i: f12 + 5.0, dp50=6.0), rtt_ctl, loss=ctl_loss))
    return rows


def run(rows, kw=KARMS, p1=P1, **kwargs):
    c = s.Cell("c3j0", rows, 12, kw, p1, 512)
    cr = s.cr62(c)
    res = s.scored(c, random.Random(62), **kwargs)
    return c, cr, res


c, cr, res = run(cell_rows())
check("a KNOWN-answer cell (floor 41.0, every knee arm 0.4 ms above it, no jitter): need_0 0.4, Delta_0 = 1/2 x 0.05 x 41.0 = 1.025, D0 0, label BELOW", near(res["need"], 0.4, 1e-9) and near(res["delta0"], 1.025, 1e-9) and near(res["d0"], 0.0, 1e-9)
      and res["label"] == "BELOW" and res["n_ok"] == 6, {k: res.get(k) for k in ("need", "delta0", "d0", "label", "n_ok")})
check("need_0 is an absolute reading: native knee minimum less the W 12 floor's median G(250) group minimum (0.4 on a 41.0 floor), and the intercept reading uses the W 12 / W 24 line (41.0 and 41.1: intercept 40.9 -> 0.5)",
      near(res["need"], 0.4, 1e-9) and near(res["Fi"], 40.9, 1e-9) and near(res["need_i"], 0.5, 1e-9), (res["Fi"], res["need_i"]))
check("ABOVE: knee arms 3.0 ms above the floor (CI above Delta_0 = 1.025)", run(cell_rows(need=lambda rep: 3.0))[2]["label"] == "ABOVE")
check("UNRESOLVED: replicates 0.4 .. 1.9 ms above the floor (median 1.15, the 90 % interval straddles Delta_0 = 1.025)", run(cell_rows(need=lambda rep: 0.4 + 0.3 * (rep - 1)))[2]["label"] == "UNRESOLVED")
fs = run(cell_rows(f12=41.0, f24=43.0, need=lambda rep: 0.5))[2]
check("FLOOR-SENSITIVE: W 12 / W 24 floors 41.0 / 43.0 put the intercept at 40.0 (need_i 1.5 ABOVE) while the W 12 reading (0.5) is BELOW; the two opposite definite labels replace either", fs["label"].startswith("FLOOR-SENSITIVE") and fs["label_w12"] == "BELOW" and fs["label_int"] == "ABOVE", fs.get("label"))
g = run(cell_rows(dip=True))[2]
check("UNSCOREABLE by the gate: a floor whose minimum falls with the count (a dip every 10th window) reads D0 >= 0.5 ms (here 5.0: native 42.0 vs G(2100) 38.0)", g["label"] == "UNSCOREABLE (gate: D0 >= 0.5 ms)" and g["d0"] >= 0.5 and near(g["d0"], 4.0, 0.51), (g["label"], g["d0"]))
check("UNSCOREABLE by replicates: K arms 20 slots above W95 put W95 below K1 in every replicate (fewer than 5 inside [K1, K3])", run(cell_rows(kw=(112, 124, 136), p1=164), kw=(112, 124, 136), p1=164)[2]["label"].startswith("UNSCOREABLE (fewer than 5 replicates"))
check("UNSCOREABLE by floor rows: fewer than 3 valid rows on the W 24 arm (two rows with 2 windows each hold no G(250) group)", (lambda rr: run(rr)[2]["label"].startswith("UNSCOREABLE (floor"))(
      [r for r in cell_rows() if r.arm != "G24"] + [mk("G24", 42, k, gp_of(24), win(2, lambda i: 41.1)) for k in (1, 2)]))
check("CONTROL-MOVED: fewer than 4 unflagged W 512 controls", run(cell_rows(n_ctl=3))[2]["label"] == "CONTROL-MOVED")
check("CONTROL-MOVED (absolute test on the arm as a whole): a control arm whose median goodput is below 0.98 x max(K3, P1) (a degrading arm CR62's own-arm cut cannot see)", run(cell_rows(ctl_gp=CTL * 0.9))[2]["label"] == "CONTROL-MOVED"
      and "0.98" in " ".join(run(cell_rows(ctl_gp=CTL * 0.9))[2]["ctl_why"]))
check("CONTROL-MOVED (loss): a control row's loss truth above 4.76 % + 1 pp", run(cell_rows(ctl_loss=0.07))[2]["label"] == "CONTROL-MOVED")
check("label(): the table's order: UNSCOREABLE before BELOW / ABOVE, BELOW iff the CI's upper bound < Delta_0, ABOVE iff its lower bound > Delta_0, else UNRESOLVED",
      s.label((-0.3, 0.4), 1.05, 0.1, 6) == "BELOW" and s.label((3.0, 4.0), 1.05, 0.1, 6) == "ABOVE" and s.label((0.6, 1.6), 1.05, 0.1, 6) == "UNRESOLVED" and s.label((3.0, 4.0), 1.05, 0.7, 6).startswith("UNSCOREABLE (gate")
      and s.label((3.0, 4.0), 1.05, 0.1, 4).startswith("UNSCOREABLE (fewer than 5") and s.final_label((-0.2, 0.8), (1.3, 2.4), 1.05, 0.1, 6)[0] == "FLOOR-SENSITIVE (W12 BELOW, intercept ABOVE)")

# ═══ 3. CR62 ═══
fi = [x for x in cr["rows"] if x["finding"]]
check("CR62: a control whose ratio fires (E90_own over the floored denominator > 1.5) at FULL goodput prints RULE-FINDING-CR62 and voids nothing (6 findings, 0 flags)", len(fi) == 6 and not any(x["flag"] for x in cr["rows"]) and cr["findings"] == 6
      and all(x["row"].flag is False for x in cr["rows"]), (len(fi), cr["findings"]))
rows_low = cell_rows()
for r in rows_low:
    if r.arm == "G512" and r.rep == 1:
        r.r["mbps"] = CTL * 0.5
c2_, cr2, _ = run(rows_low)
check("CR62: the same row with its goodput below 0.95 x the arm's median is COLLAPSED (flag), the other five stay findings: the goodput cut is on every clause", sum(1 for x in cr2["rows"] if x["flag"]) == 1 and cr2["findings"] == 5)
check("CR62's X_min binds when X_own is small: floor ticks 41.5 against tmin 41.0 give X_own near 0.5 < X_min = 0.05 x 41 = 2.05 (X_use 2.05), and the control's floored denominator is printed", near(cr["x_min"], 2.05, 1e-9) and cr["x_use"] == cr["x_min"] and cr["binds"] >= 0)
cd = [mk("D12", 42, 7, 0.1, win(120, lambda i: 30.0), [90.0] * 40), mk("D104", 7, 8, gp_of(104), win(120, lambda i: 90.0), [200.0] * 40)]
c3_, cr3, res3 = run(cell_rows() + cd)
c4_, cr4, res4 = run(cell_rows())
check("D rows (kind D) are excluded from every scored median, from CR62's reference and from the arm medians: adding two wild D rows moves nothing (need_0, F_0, X_use, the control median)",
      len(c3_.rows) == len(c4_.rows) and len(c3_.dump_rows) == 2 and res3["need"] == res4["need"] and res3["F0"] == res4["F0"] and cr3["x_use"] == cr4["x_use"] and res3["ctl_m"] == res4["ctl_m"] and res3["ci"] == res4["ci"]
      and all(r.kind == "G" for r in c3_.rows))

# ═══ 4. the rehearsal go/no-go ═══
U = 0.70


def reh(floor_dip=False, few_groups=False, collapse=False, f_gp=None, ctl2=CTL, loss=0.048, probes=(CTL * U, CTL), short=None, p_arms=(80, 128), d1=True, dnf12=False):
    rows = [mk("G512", 42, 1, CTL, win(120, lambda i: 46.0, dp50=6.0), (52.0,) * 40, loss=loss), mk("G512", 7, 2, ctl2, win(120, lambda i: 46.0, dp50=6.0), (52.0,) * 40, loss=loss)]
    f12_t = (lambda i: 38.0 if (floor_dip and i % 10 == 0) else (42.0 if floor_dip else 41.0))
    for sd, rp in ((42, 1), (7, 2)):
        wins = win(120, f12_t)
        if few_groups:      # 100 windows of 10 samples (4 G(250) groups) and one of 3 000: 5 groups < 6, yet a G(2 100) group exists and 120 native windows remain
            wins = win(100, f12_t, nk=10) + [(115.0, 3000, 41.0, 42.0, 46.0, 48.0)]
        last = (sd, rp) == (7, 2)
        rows.append(mk("G12", sd, rp, (0.3 if (collapse and last) else (f_gp if f_gp is not None else 0.8)), wins, (60.0,) * 40 if (collapse and last) else (41.5,) * 40, dnf=dnf12 and last))
    for w, gp in zip(p_arms, probes):
        rows.append(mk("G%d" % w, 42, 1, gp, win(short or 120, lambda i: 46.0)))
    if d1:
        rows.append(mk("D12", 42, 3, 0.8, win(120, lambda i: 41.0), t62={"dump": {"n": 100000, "diag_n": 100100, "coverage": 0.999, "cap": False, "iqr": 3.1}}))
    return rows


out = []
dec, w_hat, failing = s.rehearsal(reh(), out.append)
text = "\n".join(out)
check("rehearsal GO: floor pair valid, D0 0, probes u 0.70 / 1.00 (proportional), controls healthy, windows long -> `LABEL62 GO-c3j0 GO`, PILOT62 W=108.6 with the printed arms and case=proportional",
      failing == [] and "LABEL62 GO-c3j0 GO" in text and "PILOT62 W=108.6 " in text and "case=proportional" in text and f"arms={plan62.arms(108.6)}" in text and w_hat == 108.6, text[-1500:])
check("the first readings are printed before the decision: D0 on the pair with F_native / F_G250 / F_G2100, u_lo / u_hi, E90_own and p90 - tmin, X_own and whether X_min binds, the dump's coverage",
      all(k in text for k in ("FIRST-READING D0", "F_native", "F_G250", "F_G2100", "FIRST-READING u80", "FIRST-READING G12", "E90_own", "p90 - tmin", "FIRST-READING X_own", "FIRST-READING dump D12", "coverage 0.999")) and text.index("FIRST-READING D0") < text.index("LABEL62 GO-c3j0"))
for name, kw_, rule in (("NO-GO-FLOOR (a floor row holds 5 G(250) groups, fewer than 6)", dict(few_groups=True), "NO-GO-FLOOR"),
                        ("NO-GO-FLOOR (a floor row DNF)", dict(dnf12=True), "NO-GO-FLOOR"),
                        ("NO-GO-FLOOR (a floor row COLLAPSED under CR62: its own goodput below 0.95 x the pair's and its e_own above X_use)", dict(collapse=True), "NO-GO-FLOOR"),
                        ("NO-GO-GATE (the floor's minimum falls with the count: D0 >= 0.5 ms)", dict(floor_dip=True), "NO-GO-GATE"),
                        ("NO-GO-PILOT (both probes at or above 0.95: case both-above)", dict(probes=(CTL, CTL)), "NO-GO-PILOT"),
                        ("NO-GO-PILOT (both probes below 0.95: secant)", dict(probes=(CTL * 0.5, CTL * 0.7)), "NO-GO-PILOT"),
                        ("NO-GO-CONTROL (R1 / R6 differ by more than 1 %)", dict(ctl2=CTL * 0.97), "NO-GO-CONTROL"),
                        ("NO-GO-CONTROL (the controls' loss truth above 4.76 % + 1 pp)", dict(loss=0.07), "NO-GO-CONTROL"),
                        ("NO-GO-SHORT (a probe row has fewer than 100 native LATE windows)", dict(short=60), "NO-GO-SHORT")):
    out = []
    dec, w_hat, failing = s.rehearsal(reh(**kw_), out.append)
    text = "\n".join(out)
    check(f"{name}: exactly that rule fails and the label line names it", failing == [rule] and f"LABEL62 GO-c3j0 {rule}" in text and "LABEL62 GO-c3j0 GO" not in text, (failing, text[-700:]))
out = []
s.rehearsal(reh(floor_dip=True, probes=(CTL, CTL)), out.append)
text = "\n".join(out)
check("several failing rules: the label line names the first in the table's order (NO-GO-GATE before NO-GO-PILOT) and NO-GO-LIST names all", "LABEL62 GO-c3j0 NO-GO-GATE" in text and "NO-GO-LIST NO-GO-GATE,NO-GO-PILOT" in text, text[-600:])
check("CR62's ratio / qshare clauses are not go/no-go items: a control row whose ratio fires at the rehearsal prints no NO-GO for it (the floor pair's CR62 is the only collapse test)", (lambda o: (s.rehearsal(reh(), o.append), "NO-GO" not in "\n".join(o))[1])([]))
rr = [s.Row(r.r) for r in reh(probes=(CTL, CTL))]
check("re-probe: both probes at or above 0.95 -> 64 / 96; both below -> 112 / 160; a non-monotone pair and a proportional pair -> none; once the re-probe rows are in the ledger -> none",
      s.reprobe_rows(rr) == (64, 96) and s.reprobe_rows([s.Row(r.r) for r in reh(probes=(CTL * 0.5, CTL * 0.7))]) == (112, 160) and s.reprobe_rows([s.Row(r.r) for r in reh(probes=(CTL, CTL * 0.5))]) is None
      and s.reprobe_rows([s.Row(r.r) for r in reh()]) is None and s.reprobe_rows(rr + [mk("G64", 42, 1, CTL * 0.8, win(120, lambda i: 46.0)), mk("G96", 42, 1, CTL, win(120, lambda i: 46.0))]) is None)
rows_re = reh(probes=(CTL, CTL)) + [mk("G64", 42, 1, CTL * 0.8, win(120, lambda i: 46.0)), mk("G96", 42, 1, CTL, win(120, lambda i: 46.0))]
out = []
dec, w_hat, failing = s.rehearsal([s.Row(r.r) for r in rows_re], out.append)
text = "\n".join(out)
check("after the re-probe the SAME rule runs on the new probes: G64 / G96 with u 0.80 / 1.00 read proportional (W = 64 * 0.95 / 0.8 = 76.0, probes=64/96) and the rehearsal GOes; a second non-proportional case stays NO-GO-PILOT",
      "probes=64/96" in text and "PILOT62 W=76.0 " in text and "NO-GO-PILOT" not in failing, text[-800:])
out = []
dec, w_hat, failing = s.rehearsal([s.Row(r.r) for r in reh(probes=(CTL, CTL)) + [mk("G64", 42, 1, CTL, win(120, lambda i: 46.0)), mk("G96", 42, 1, CTL, win(120, lambda i: 46.0))]], out.append)
check("a second non-proportional case (the re-probe both-above again) is NO-GO-PILOT", "NO-GO-PILOT" in failing)

# ═══ ABORT-NETEM's witness (l1common) ═══
GOOD = "qdisc netem 8001: root refcnt 2 limit 1000 delay 20ms loss gemodel p 2% r 40% 1-h 100% 1-k 0% rate 20Mbit seed 42"
rowtxt = lambda pre=GOOD, post=GOOD, srvpre=GOOD, srvpost=GOOD: [f"[NETEM] tag=pre leg=cli dev=cli0 qdisc: {pre}", f"[NETEM] tag=pre leg=srv dev=srv0 qdisc: {srvpre}", f"[NETEM] tag=post leg=cli dev=cli0 qdisc: {post}", f"[NETEM] tag=post leg=srv dev=srv0 qdisc: {srvpost}"]  # noqa: E731
check("the live-netem witness: delay 20ms with no jitter term on both legs, before and after, is clean", lc.netem_witness(rowtxt())[1] == [])
check("a jitter term (`delay 20ms  5ms`, the c3 line) on the data leg before the first row is refused", lc.netem_witness(rowtxt(pre=GOOD.replace("delay 20ms", "delay 20ms  5ms")))[1] == ["pre/cli: jitter term 5ms"], lc.netem_witness(rowtxt(pre=GOOD.replace("delay 20ms", "delay 20ms  5ms")))[1])
check("a jitter term on the ACK leg after the last row is refused; a wrong delay (c2's 5ms), a missing netem root and a missing witness are refused",
      lc.netem_witness(rowtxt(srvpost=GOOD.replace("delay 20ms", "delay 20ms  3.5ms")))[1] == ["post/srv: jitter term 3.5ms"] and lc.netem_witness(rowtxt(post=GOOD.replace("delay 20ms", "delay 5ms")))[1][0].startswith("post/cli: delay 5ms")
      and lc.netem_witness(rowtxt(pre="qdisc noqueue 0: root refcnt 2"))[1] == ["pre/cli: no netem qdisc"] and len(lc.netem_witness(rowtxt()[:2])[1]) == 2 and len(lc.netem_witness([])[1]) == 4)
check("tc's decimal form `delay 20.0ms` reads as 20 ms; `delay 20.5ms`, `delay 200ms` and a `us` jitter (`delay 20ms 500us`) are refused", lc.netem_delay_ok("netem delay 20.0ms rate 1")[0] and not lc.netem_delay_ok("netem delay 20.5ms")[0]
      and not lc.netem_delay_ok("netem delay 200ms")[0] and not lc.netem_delay_ok("netem delay 20ms 500us")[0])

# ═══ the scorer's guards, the dump's count test, the netem rule on rows ═══
x_live = mk("G12", 42, 1, 0.8, win(120, lambda i: 41.0), t62={"netem_problems": [], "netem_wrong": [], "netem_missing": []})
x_wrong = mk("G12", 42, 1, 0.8, win(120, lambda i: 41.0), t62={"netem_problems": ["pre/cli: jitter term 5ms"], "netem_wrong": ["pre/cli: jitter term 5ms"], "netem_missing": []})
x_miss = mk("G12", 42, 1, 0.8, win(120, lambda i: 41.0), t62={"netem_problems": ["post/srv: no [NETEM] witness"], "netem_wrong": [], "netem_missing": ["post/srv: no [NETEM] witness"]})
x_miss_void = mk("G12", 42, 1, 0.8, win(120, lambda i: 41.0), status="WITNESS-FAIL", t62={"netem_problems": ["post/srv: no [NETEM] witness"], "netem_wrong": [], "netem_missing": ["post/srv: no [NETEM] witness"]})
check("ABORT-NETEM on rows: a clean row is fine; a WRONG read always counts; a MISSING read counts on a live row only (a void row never reached the post read); a row with no t62 counts",
      s.netem_bad(x_live) == [] and s.netem_bad(x_wrong) == ["pre/cli: jitter term 5ms"] and s.netem_bad(x_miss) == ["post/srv: no [NETEM] witness"] and s.netem_bad(x_miss_void) == [] and s.netem_bad(mk("G12", 42, 1, 0.8, win(120, lambda i: 41.0))) == ["no t62 netem witness"])
check("row_extras reads the live qdisc of a row's driver output: clean -> [] / [] / []; a jitter term -> wrong; a missing read -> missing", s.row_extras([], [], rowtxt(), "c3j0-25")["netem_problems"] == [] and s.row_extras([], [], rowtxt(pre=GOOD.replace("delay 20ms", "delay 20ms  5ms")), "c3j0-25")["netem_wrong"] == ["pre/cli: jitter term 5ms"]
      and s.row_extras([], [], rowtxt()[:3], "c3j0-25")["netem_missing"] == ["post/srv: no [NETEM] witness"] and s.row_extras([], [], rowtxt()[:3], "c3j0-25")["netem_wrong"] == [])
check("control_check is fail-closed on the loss truth: a control row without a [TRUTH] loss is CONTROL-MOVED (as the rehearsal's clause)", run(cell_rows(ctl_loss=None))[2]["label"] == "CONTROL-MOVED" and "loss truth missing" in " ".join(run(cell_rows(ctl_loss=None))[2]["ctl_why"]))
fl_short = [mk("G12", 42 if k % 2 else 7, k, gp_of(12), win(30, lambda i: 41.0)) for k in range(1, 7)]      # 30 windows x 60 samples: 7 G(250) groups, no G(2 100) group
res_s = run([r for r in cell_rows() if r.arm != "G12"] + fl_short)[2]
check("scored() does not crash when six G(250) groups hold no G(2 100) group: UNSCOREABLE (floor: no group at a count)", res_s["label"].startswith("UNSCOREABLE (floor: no group at a count"), res_s.get("label"))
slaw = lambda k, nk, p=0: f"[SLAW] t={0.25 * k:.2f} k={k} p={p} n=12 W=12 aw=0.5000 nk={nk} tmin=41.000"      # noqa: E731
dl = ['{"warmup":true}'] + [slaw(k, 100) for k in range(1, 11)] + [slaw(1, 55, p=1)] + ["[RTTDUMP] p=0 t0=1000000 n=256 d=" + ";".join(f"{0 if i == 0 else 10000},{40000 + i}" for i in range(256))] * 3 + ["[RTTDUMP-CAP] p=0 emitted=768"] \
    + ["[DIAG] t=2.5s win=1/1 p0:infl=3 rtt=40/wrtt=40/rtp5ms rtp_us=40 sig_us=38/n1000"]
dr = s.dump_reading(dl)
check("the dump's count test: dumped samples (3 x 256) against the [SLAW] path-0 windows' own sample count (10 x 100; path 1 and the pre-warm-up windows excluded): ratio 0.768; coverage against the [DIAG] sig n (1 000); the cap is seen",
      dr is not None and dr["n"] == 768 and dr["slaw_nk"] == 1000 and near(dr["count_ratio"], 0.768, 1e-9) and near(dr["coverage"], 0.768, 1e-9) and dr["cap"] is True)
dl2 = ['{"warmup":true}', slaw(1, 100), "junk", '{"warmup":true}'] + [slaw(k, 100) for k in range(1, 3)] + ["[RTTDUMP] p=0 t0=1000000 n=256 d=" + ";".join(f"{0 if i == 0 else 10000},{40000}" for i in range(256))]
check("the [SLAW] sample count restarts at the LAST warm-up marker (200, not 300); a log with no dump has no dump reading", s.dump_reading(dl2)["slaw_nk"] == 200 and s.dump_reading(["[DIAG] x", slaw(1, 5)]) is None)
rows_rep = cell_rows() + [mk("D12", 42, 7, 0.1, win(120, lambda i: 41.0), t62={"dump": dr}), mk("D104", 7, 8, gp_of(104), win(120, lambda i: 41.4), t62={"dump": dict(dr, iqr=3.0)})]
cc, ccr, rres = run(rows_rep)
o = []
s.reported(cc, rres, ccr, o.append)
txt = "\n".join(o)
check("section 6's reported items are all printed: need at the three counts and the intercept, J = 5's need beside them, S_F(J0) native and G(250), A0 and D, the p10 / p50 rises, the dump's count test and coverage, D0 at W 24, reordering not measured, and the dump-IQR rise",
      all(k in txt for k in ("REPORTED need_0 at the W12 floor read native", "REPORTED J = 5 need (61c", "REPORTED J = 5 need (61a", "REPORTED S_F(J0)", "A0 = S_F(J0, G250) + q_B F0", "REPORTED p10 - tmin", "REPORTED p50 - tmin", "REPORTED dump count test D12",
                                         "REPORTED dump D12", "REPORTED D0 at W24", "reordering: not measured")), txt[-1500:])

# ═══ 5. HOST: the scorer reproduces s62pred.out on the 61c / 61a / c1s ledgers ═══
RAW = os.environ.get("S62_RAW") or os.path.join(HERE, "..", "..", "docs", "l1-raw")
TGZ = {"61c": os.path.join(RAW, "s61", "rep61c-diag-ld.tgz"), "61a": os.path.join(RAW, "s61", "rep61a-diag-ld.tgz"), "c1s": os.path.join(RAW, "s60", "rep60b-diag-ld.tgz")}
if all(os.path.exists(p) for p in TGZ.values()):
    import json
    import re
    import tarfile
    import plan61
    NAME = re.compile(r"diag-ld/(c3-25|c1s-400)@def-(G)(\d+)-s(\d+)-r(\d+)-a\d+-(c\.log|drv\.out)$")
    plan61.LATE_T["c1s-400"] = 5.0       # r68allow's LATE_T for c1s (s6.LATE_T has it; row_extras reads plan61's)

    def load(path):
        raw = {}
        with tarfile.open(path) as tf:
            for mb in tf:
                g_ = NAME.search(mb.name)
                if g_ and mb.isfile():
                    raw.setdefault((g_.group(1), int(g_.group(3)), g_.group(4), int(g_.group(5))), {})[g_.group(6)] = tf.extractfile(mb).read().decode("utf-8", "replace").splitlines()
        rows = []
        for (cell, w_, sd_, rp_), dd in sorted(raw.items()):
            cli_, drv_ = dd["c.log"], dd.get("drv.out", [])
            gp_ = next((json.loads(ln).get("mean_mbps") for ln in drv_ if ln.startswith("{") and '"summary":true' in ln), None)
            rows.append(s.Row({"cell": cell, "arm": "G%d" % w_, "seed": sd_, "rep": rp_, "status": "LIVE", "dnf": False, "mbps": gp_, "t60": s.s6.row_metrics(cli_, [], drv_, None, cell, "G%d" % w_, gp_),
                               "t61": s.s1.row_extras(cli_, [], drv_, cell)}))
        return rows
    import time
    t0 = time.time()
    r5, rA, r1 = load(TGZ["61c"]), load(TGZ["61a"]), load(TGZ["c1s"])
    print(f"   (loaded the three ledgers in {time.time() - t0:.0f} s)")
    J5 = s.Cell("J5", r5, 12, (100, 108, 120), 152, 512)
    J0 = s.Cell("J0", rA, 12, (104, 112, 124), 136, 2048)
    CS = s.Cell("c1s", r1, 64, (2048,), None, 2048)
    cr5, crA, crC = s.cr62(J5), s.cr62(J0), s.cr62(CS)
    k = lambda x: (round(x["x_own"], 2), round(x["e50"], 2), round(x["e90"], 2), round(x["x_min"], 2), x["binds"])      # noqa: E731
    check("s62pred.out section 1: X_own / E50 / E90 / X_min and the floored-denominator count of 61c, 61a, c1s", (k(cr5), k(crA), k(crC)) == ((16.50, 5.76, 9.34, 1.94, 0), (17.49, 5.94, 9.79, 1.94, 0), (1.59, 0.44, 0.83, 0.12, 0)), (k(cr5), k(crA), k(crC)))

    def per_arm(cx, cc):
        by = {}
        for x in cc["rows"]:
            a = by.setdefault(x["row"].W, [0, 0, 0])
            a[0] += 1
            a[1] += x["raw_flag"]
            a[2] += x["flag"]
        return {w: (a[1], a[2], a[0]) for w, a in by.items()}
    pa5, paA, paC = per_arm(J5, cr5), per_arm(J0, crA), per_arm(CS, crC)
    check("s62pred.out section 1: flagged per arm, raw s61 form / CR62: 61c none; 61a W2048 6/6 -> 0/6 (all RULE-FINDING-CR62); c1s W64 2/6 -> 2/6, W128 0/6, W192 1/6 -> 1/6, W2048 6/12 -> 5/12",
          all(v[0] == 0 and v[1] == 0 for v in pa5.values()) and paA[2048] == (6, 0, 6) and all(v[:2] == (0, 0) for w, v in paA.items() if w != 2048) and crA["findings"] == 6 and (paC[64], paC[128], paC[192], paC[2048]) == ((2, 2, 6), (0, 0, 6), (1, 1, 6), (6, 5, 12)),
          (pa5, paA, paC))
    rnd = random.Random(62)
    s5 = s.scored(J5, rnd)
    sA = s.scored(J0, rnd)
    sA2 = s.scored(J0, rnd, ctl_by_goodput=True)
    r3 = lambda v: round(v, 3)      # noqa: E731
    check("s62pred.out section 2, J5 (61c): control 17.82; W12 floor native 42.751 | G250 38.778 | G2100 34.319 -> D0 8.433; W24 native 42.267 | 40.198 | 37.323 -> D0 4.944",
          round(s5["ctl_m"], 2) == 17.82 and (r3(s5["f1"]["nat"]), r3(s5["f1"]["g250"]), r3(s5["f1"]["g2100"]), r3(s5["d0"])) == (42.751, 38.778, 34.319, 8.433)
          and (r3(s5["f2"]["nat"]), r3(s5["f2"]["g250"]), r3(s5["f2"]["g2100"]), r3(s5["d0b"])) == (42.267, 40.198, 37.323, 4.944), (s5["f1"], s5["f2"], s5["d0"], s5["d0b"]))
    check("s62pred.out section 2, J5: the six knees (W95 110.1 / 107.4 / 109.8 / 106.6 / 109.2 / 108.2, Tmin 46.385 / 46.281 / 46.478 / 45.670 / 46.035 / 46.184)",
          [round(w, 1) for _, w, _ in s5["knees"]] == [110.1, 107.4, 109.8, 106.6, 109.2, 108.2] and [r3(t) for _, _, t in s5["knees"]] == [46.385, 46.281, 46.478, 45.670, 46.035, 46.184], s5["knees"])
    check("s62pred.out section 2, J5: need_0 7.455 [6.620, 7.716]; at the intercept (F 37.791) 8.441 [7.329, 9.294]; Delta_0 0.969; native S_F 5.801; need at native 3.481 | G250 7.455 | G2100 11.914",
          (r3(s5["need"]), r3(s5["ci"][0]), r3(s5["ci"][1]), r3(s5["Fi"]), r3(s5["need_i"]), r3(s5["cii"][0]), r3(s5["cii"][1]), r3(s5["delta0"]), r3(s5["S_nat"])) == (7.455, 6.620, 7.716, 37.791, 8.441, 7.329, 9.294, 0.969, 5.801)
          and (r3(s5["n3"]["nat"]), r3(s5["n3"]["g250"]), r3(s5["n3"]["g2100"])) == (3.481, 7.455, 11.914), (s5["need"], s5["ci"], s5["need_i"], s5["cii"]))
    check("s62pred.out section 2, J0 (61a, control W2048): need_0 7.232 [6.829, 7.886]; intercept (F 37.711) 8.415 [7.516, 9.334]; D0 7.072 / 5.008; floors native 42.356 | 38.894 | 35.285 and 41.852 | 39.827 | 36.844; native S_F 5.941; need 3.770 | 7.232 | 10.841",
          (r3(sA["need"]), r3(sA["ci"][0]), r3(sA["ci"][1]), r3(sA["Fi"]), r3(sA["need_i"]), r3(sA["cii"][0]), r3(sA["cii"][1]), r3(sA["d0"]), r3(sA["d0b"]), r3(sA["S_nat"])) == (7.232, 6.829, 7.886, 37.711, 8.415, 7.516, 9.334, 7.072, 5.008, 5.941)
          and (r3(sA["f1"]["nat"]), r3(sA["f1"]["g250"]), r3(sA["f1"]["g2100"])) == (42.356, 38.894, 35.285) and (r3(sA["f2"]["nat"]), r3(sA["f2"]["g250"]), r3(sA["f2"]["g2100"])) == (41.852, 39.827, 36.844)
          and (r3(sA["n3"]["nat"]), r3(sA["n3"]["g250"]), r3(sA["n3"]["g2100"])) == (3.770, 7.232, 10.841), (sA["need"], sA["ci"], sA["cii"]))
    check("s62pred.out section 2, J0 with CR62 lifted on the control only (the prototype's exercise): the intervals [6.861, 7.889] and intercept [7.540, 9.312]",
          (r3(sA2["ci"][0]), r3(sA2["ci"][1]), r3(sA2["cii"][0]), r3(sA2["cii"][1])) == (6.861, 7.889, 7.540, 9.312), (sA2["ci"], sA2["cii"]))
    check("s62pred.out section 3: both stand-ins read UNSCOREABLE (gate: D0 >= 0.5 ms); the gate forced open gives ABOVE on both (the label function, the same inputs)",
          s5["label"] == "UNSCOREABLE (gate: D0 >= 0.5 ms)" and sA["label"] == "UNSCOREABLE (gate: D0 >= 0.5 ms)" and s.final_label(s5["ci"], s5["cii"], s5["delta0"], 0.0, s5["n_ok"])[0] == "ABOVE" and s.final_label(sA["ci"], sA["cii"], sA["delta0"], 0.0, sA["n_ok"])[0] == "ABOVE")
    # the c1s floor's D0 and the rehearsal's gate pre-read on a pair (section 2 / 5)
    fl_c = [r for r in CS.rows if r.W == 64 and not r.flag]
    d0c = s.med([s.s1.reading(r.w61(1), s.NF)[1] for r in fl_c]) - s.med([s.s1.reading(r.w61(1), s.NK)[1] for r in fl_c])
    check("s62pred.out section 2: c1s floor W64 valid 4, D0 0.000 ms (the windows hold ~5 366 samples: G(250) and G(2100) are one window)", len(fl_c) == 4 and round(d0c, 3) == 0.0)
    pre = lambda cx: (lambda pair: round(max(s.med([r.native_tmin() for r in pair]), s.med([s.s1.reading(r.w61(1), s.NF)[1] for r in pair])) - s.med([s.s1.reading(r.w61(1), s.NK)[1] for r in pair]), 3))([r for r in cx.rows if r.W == cx.floor_w][:2])      # noqa: E731
    check("s62pred.out section 5: the gate's pre-read on a floor pair is D0 8.502 (61c) / 7.166 (61a): NO-GO-GATE", (pre(J5), pre(J0)) == (8.502, 7.166))
    # section 6: the pilot on 61c's curve
    ctl5 = s5["ctl_m"]
    for W_, u_want, wh_want in ((80, 0.743, 102.2), (88, 0.811, 103.1)):
        u = s.med([r.gp_() / ctl5 for r in J5.rows if r.W == W_])
        raw_w = min(W_ * 0.95 / u, 128)
        ks = plan62.arms(raw_w)["K"]
        ws = [w for _, w, _ in s5["knees"]]
        check(f"s62pred.out section 6: 61c W{W_}: u {u_want} -> W-hat {wh_want}; K 96 / 108 / 124, 6 of 6 W95 inside [K1, K3]", round(u, 3) == u_want and round(raw_w, 1) == wh_want and ks == [96, 108, 124] and sum(1 for w in ws if ks[0] <= w <= ks[2]) == 6, (u, raw_w, ks))
    sp_of = lambda cx: round(s.med([s.med([w["p90"] - w["p10"] for w in r.w61(s.s1.NK_NATIVE) if w["p90"] is not None and w["p10"] is not None]) for r in cx.rows if r.W == 12]), 3)      # noqa: E731
    check("J = 5's floor-window p90 - p10 constants are the ledgers' own (61c 8.169, 61a 8.285 ms; s62pred.out does not print them) and p50 - min equals the s62pred.out constants",
          (sp_of(J5), sp_of(J0)) == (s.J5_SPREAD["61c"], s.J5_SPREAD["61a"]) and (round(s5["S_nat"], 3), round(sA["S_nat"], 3)) == (s.J5_S_F_NATIVE["61c"], s.J5_S_F_NATIVE["61a"]), (sp_of(J5), sp_of(J0)))

    def first_qdisc(path, cell):
        with tarfile.open(path) as tf:
            for mb in tf:
                if mb.isfile() and mb.name.endswith("-drv.out") and ("/" + cell + "@") in mb.name:
                    for ln in tf.extractfile(mb).read().decode("utf-8", "replace").splitlines():
                        if "QDISC cli0:" in ln:
                            return ln.split("QDISC cli0:", 1)[1].strip()
        return None
    q_c3, q_c1 = first_qdisc(TGZ["61c"], "c3-25"), first_qdisc(TGZ["c1s"], "c1s-400")
    check("the live-netem witness on REAL tc text: 61c's c3 line (`delay 20ms  5ms`) reads `jitter term 5ms`; the same line with the jitter removed (what c3j0 prints) is clean; c1s's line (`delay 1ms`) is a wrong delay",
          q_c3 is not None and lc.netem_delay_ok(q_c3) == (False, "jitter term 5ms") and lc.netem_delay_ok(q_c3.replace("delay 20ms  5ms", "delay 20ms"))[0] is True and q_c1 is not None and lc.netem_delay_ok(q_c1)[0] is False
          and lc.netem_delay_ok(q_c1)[1].startswith("delay 1ms"), (q_c3, q_c1))
else:
    print("skip HOST: the scorer vs s62pred.out (the 61c / 61a / c1s ledgers are not here: set S62_RAW to a docs/l1-raw)")

print("FAILED %d" % fails if fails else "ALL OK")
print(f"\ntest_s62: {fails} fail(s)")
sys.exit(1 if fails else 0)
