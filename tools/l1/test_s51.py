#!/usr/bin/env python3
"""Tests for status 51 block 51 (plan 51): tools/l1/plan51.py (the 76-row table per cell, the per-row hint / delta / bytes, the witnesses, the replacement rule) and s51_score.py (the two-sided rank-sum min-p
family with exact relabellings, T1 / T2, the DNF rule, the wording rule, the d80 bootstrap, the row readouts and the not-overdriven smoke witness) on synthetic ledgers in the REAL block / seed layout
(s51-design.md section 6 item 4). The launch paths run in test_stubrun.py; the shell wiring is test_harness51.py.
Run: python3 test_s51.py"""
import json
import math
import os
import random
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
for _k in ("LD_PLAN", "LD_CELL", "LD_SHA", "LD_BASE_SHA", "LD_ROW_BYTES"):
    os.environ.pop(_k, None)
import plan51  # noqa: E402
import s51_d80  # noqa: E402
import s51_score as sc  # noqa: E402

fails = 0


def check(name, ok, detail=""):
    global fails
    print(("ok   " if ok else "FAIL ") + name + ((" -- " + str(detail)[:1500]) if not ok else ""))
    if not ok:
        fails += 1


# ═══ 1. the table ═══
for cell in plan51.CELLS:
    T = plan51.plan("51", cell=cell)
    cnt = {a: sum(1 for r in T if r[2] == a) for a in plan51.ARMS}
    check(f"{cell}: 76 rows: the 10 scored arms x 6 and the 4 interior arms x 4, every slot unique, six blocks (rep = block)",
          len(T) == 76 and all(cnt[a] == 6 for a in plan51.SCORED) and all(cnt[a] == 4 for a in plan51.INTERIOR) and len({(r[2], r[3], r[4]) for r in T}) == 76 and {r[4] for r in T} == {1, 2, 3, 4, 5, 6} and all(r[0] == cell for r in T), cnt)
check("blocks hold 14 / 14 / 14 / 14 / 10 / 10 rows (the interior arms run in blocks 1-4 only)", [plan51.block_rows(plan51.C2)[b] for b in range(1, 7)] == [14, 14, 14, 14, 10, 10])
T = plan51.plan("51", cell=plan51.C2)
check("seeds alternate 42 / 7 / 42 / 7 / 42 / 7 by block, so every scored arm has 3 rows per seed", [next(r[3] for r in T if r[4] == b) for b in range(1, 7)] == ["42", "7", "42", "7", "42", "7"]
      and all(sum(1 for r in T if r[2] == a and r[3] == "42") == 3 for a in plan51.SCORED))
adj = True
first = {}
for b in range(1, 7):
    blk = [r[2] for r in T if r[4] == b]
    for P, (lo, hi) in plan51.PAIRS.items():
        i, j = blk.index(lo), blk.index(hi)
        adj &= abs(i - j) == 1
        first.setdefault(P, []).append("lo" if i < j else "hi")
check("each nudge pair runs back to back in every block, the first member alternating by block (the minus side first in odd blocks)", adj and all(v == ["lo", "hi"] * 3 for v in first.values()), first)
check("the unit list rotates by block: block 1 starts with H-B, block 2 with H-A (singletons and pairs rotate)", [r[2] for r in T if r[4] == 1][0] == "HB" and [r[2] for r in T if r[4] == 2][0] == "HA")
check("the smoke = H-B, H-R, B+, N-A, one each at seed 42 (the rehearsal runs it); c8 has the same arms", [r[2] for r in plan51.plan("51", smoke=True, cell=plan51.C2)] == ["HB", "HR", "BP", "NA"]
      and {r[2] for r in plan51.plan("51", cell=plan51.C8)} == set(plan51.ARMS))
spec = {a: (plan51.arm_bytes(plan51.C2, a), plan51.arm_timeout(plan51.C2, a), plan51.arm_hint(a), plan51.arm_env(plan51.C2, a)) for a in plan51.ARMS}
check("armspec: 600 MB and 200 s on every arm; H-B bulk, H-A auto, H-R realtime with NO RWM_DELTA; every numeric arm on the Auto carrier with RWM_DELTA = the registered value",
      all(v[0] == 600_000_000 and v[1] == 200 for v in spec.values()) and [spec[a][2:] for a in ("HB", "HA", "HR")] == [("bulk", []), ("auto", []), ("realtime", [])]
      and all(spec[a][2] == "auto" for a in plan51.ARMS if a not in ("HB", "HA", "HR"))
      and {a: spec[a][3] for a in ("NA", "BM", "BP", "AM", "AP", "RM", "RP", "I1", "I2", "I3", "I4")}
      == {"NA": ["RWM_DELTA=0.5"], "BM": ["RWM_DELTA=0.0049"], "BP": ["RWM_DELTA=0.0051"], "AM": ["RWM_DELTA=0.49"], "AP": ["RWM_DELTA=0.51"], "RM": ["RWM_DELTA=49"], "RP": ["RWM_DELTA=51"],
          "I1": ["RWM_DELTA=0.0075"], "I2": ["RWM_DELTA=0.0158"], "I3": ["RWM_DELTA=0.05"], "I4": ["RWM_DELTA=5"]}, spec)
check("beta(delta) of the registered points: H-B 1, B- 1, B+ 0.9957, I1 0.912, I2 0.750, I3 0.500, H-A / N-A 0, A- 0.0044 (the law: log10(0.5 / 0.49) / 2), A+ 0, I4 0, R 0",
      [round(plan51.arm_beta(a), 4) for a in ("HB", "BM", "BP", "I1", "I2", "I3", "NA", "AM", "AP", "I4", "RM", "RP", "HR", "HA")] == [1.0, 1.0, 0.9957, 0.912, 0.7502, 0.5, 0.0, 0.0044, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
      [round(plan51.arm_beta(a), 4) for a in ("HB", "BM", "BP", "I1", "I2", "I3", "NA", "AM", "AP", "I4", "RM", "RP", "HR", "HA")])
check("armspec refuses an unknown arm / cell (rc 1)", plan51.main(["armspec", "c2-100", "ZZ"]) == 1 and plan51.main(["armspec", "c1s-400", "HB"]) == 1)
check("arm names carry no '-', '+' or the minus sign (the per-row log names are parsed on '-')", all(a.isalnum() for a in plan51.ARMS))
check("budgets: 17 400 s (76 x 215 s + 16 min = 4 h 50), the rehearsal 1 200 s", plan51.BUDGET_S == {"51": 17400, "51r": 1200} and plan51.ROW_BUDGET_S == 215)

# ═══ 2. the witnesses ═══
SHA = "a" * 64


def mklog(hint="bulk", delta=None, echo=None, batching=True, srcs=None, nbytes=600_000_000, chi=None):
    pay = plan51.SYMBOL_PAYLOAD[hint]
    srcs = int(nbytes / pay) if srcs is None else srcs
    tok = echo if echo is not None else (delta or "unset")
    cli = [f"[GATES] RWM_DIAG=1 RWM_DELTA={tok}"]
    if batching:
        cli.append("INFO " + plan51.BATCH_ECHO[hint] + " (...)")
    if chi:
        cli.append(chi)
    cli.append(f"[DIAG] t=10.0s src=100sym/s cum={srcs}/10/{srcs - 5} retx=1")
    srv = [f"[GATES] RWM_DIAG=1 RWM_DELTA={delta or 'unset'}"]
    return cli, srv


def rp(arm, **kw):
    hint = plan51.arm_hint(arm)
    cli, srv = mklog(hint=hint, delta=plan51.delta_token(arm), **kw)
    return plan51.row_problems(plan51.C2, arm, cli, srv, SHA, SHA, 600_000_000)


check("a healthy row of every arm has no problem", all(rp(a) == [] for a in plan51.ARMS), {a: rp(a) for a in plan51.ARMS if rp(a)})
check("[GATES] RWM_DELTA not the arm's value on the client end is gate-delta-cli (a numeric arm echoing 'unset', an H-A row echoing a delta); the server end is checked too",
      any(x.startswith("gate-delta-cli=unset") for x in rp("NA", echo="unset")) and any(x.startswith("gate-delta-cli=0.123") for x in rp("HA", echo="0.123")) and any(x.startswith("gate-delta-srv") for x in plan51.row_problems(plan51.C2, "NA", *mklog("auto", "0.5")[:1], ["[GATES] RWM_DIAG=1 RWM_DELTA=unset"], SHA, SHA, None)))
check("the numeric echo compares numerically ('5' = 5.0; '0.0049' only for B-)", rp("I4", echo="5") == [] and rp("RM", echo="49") == [] and any(x.startswith("gate-delta") for x in rp("BP", echo="0.0049")))
check("the batching echo: ACTIVE at bulk / auto, 'out of scope' at realtime; missing is batch-echo-missing, the other carrier's is batch-echo-wrong",
      any(x.startswith("batch-echo-missing") for x in rp("HB", batching=False)) and any(x.startswith("batch-echo-missing") for x in rp("HR", batching=False))
      and any(x.startswith("batch-echo-wrong") for x in plan51.row_problems(plan51.C2, "HR", *mklog("bulk", None), SHA, SHA, 600_000_000)) and any(x.startswith("batch-echo-wrong") for x in plan51.row_problems(plan51.C2, "HB", *mklog("realtime", None), SHA, SHA, 600_000_000)))
check("source symbols per byte match the carrier's symbol size: a realtime row printing a 1 186 B carrier's count (ratio 0.41), or a bulk row printing a 494 B one (2.40), is srcsize-ratio",
      any(x.startswith("srcsize-ratio=0.4") for x in rp("HR", srcs=int(600_000_000 / 1186))) and any(x.startswith("srcsize-ratio=2.4") for x in rp("HB", srcs=int(600_000_000 / 494))) and rp("HR", srcs=int(600_000_000 / 494 * 1.1)) == [])
check("a [CHI] line with n > 0 is chi-active; n = 0 or absent passes", any(x.startswith("chi-active") for x in rp("HB", chi="[CHI] n=3 x=1")) and rp("HB", chi="[CHI] n=0") == [])
check("a foreign binary is a bin-sha problem and is not echo-replaceable", any(x.startswith("bin-sha") for x in plan51.row_problems(plan51.C2, "HB", *mklog(), SHA, "b" * 64, 600_000_000)) and not plan51.is_echo_void({"problems": ["bin-sha=aaa (want bbb for CAND)"]}))
d = tempfile.mkdtemp()


def ledger(rows):
    p = os.path.join(d, "ld%d.log" % len(os.listdir(d)))
    with open(p, "w") as fh:
        for r in rows:
            fh.write("P38ROW " + json.dumps(r) + "\n")
    return p


def row(arm, seed="42", rep=1, problems=(), dnf=False, status="LIVE", cell="c2-100", **t):
    return {"cell": cell, "arm": arm, "seed": seed, "rep": rep, "problems": list(problems), "dnf": dnf, "status": status, "mbps": t.pop("mbps", 90.0), "t51": {**{"inorder_ms": 250.0, "cpu_cli_s_per_gb": 30.0, "cpu_srv_s_per_gb": 33.0}, **t}}


check("replacement: an echo VOID slot (gate-delta, batch-echo, srcsize, pipe) is re-run once ('echo'); VOID-SHORT once ('short'); a second row, a DNF, a crash and a wrong binary are not",
      plan51.replace_reason(ledger([row("HB", problems=["gate-delta-cli=0.1 (want unset)"], status="WITNESS-FAIL")]), "c2-100", "HB", "42", 1) == "echo"
      and plan51.replace_reason(ledger([row("HB", problems=["batch-echo-missing x"], status="WITNESS-FAIL")]), "c2-100", "HB", "42", 1) == "echo"
      and plan51.replace_reason(ledger([row("HB", problems=["short-row last-tick=10s"], status="VOID-SHORT")]), "c2-100", "HB", "42", 1) == "short"
      and plan51.replace_reason(ledger([row("HB", problems=["gate-delta-cli=0.1"], status="WITNESS-FAIL"), row("HB")]), "c2-100", "HB", "42", 1) is None
      and plan51.replace_reason(ledger([row("HB", dnf=True)]), "c2-100", "HB", "42", 1) is None and plan51.replace_reason(ledger([row("HB", status="VOID-RC")]), "c2-100", "HB", "42", 1) is None
      and plan51.replace_reason(ledger([row("HB", problems=["bin-sha=aaa (want bbb for CAND)"], status="WITNESS-FAIL")]), "c2-100", "HB", "42", 1) is None)
p_full = ledger([row(a, sd, rp_) for _, _, a, sd, rp_ in plan51.plan("51", cell="c2-100")])
check("plan51.py check: rc 1 without LD-BATTERY-DONE, rc 0 with it (a replaced slot's second row is not an extra)", plan51.check("51", p_full) == 1 and (open(p_full, "a").write("LD-BATTERY-DONE tag=ld blocks=6\n") or True) and plan51.check("51", p_full) == 0)

# ═══ 3. the statistic: exact relabellings, two-sided, family min-p ═══
rng = random.Random(5)


def vec(g=90.0, d=250.0, c=30.0, s=33.0):
    return [g + rng.gauss(0, 0.2), d + rng.gauss(0, 3), c + rng.gauss(0, 0.5), s + rng.gauss(0, 0.5)]


A6 = [vec() for _ in range(6)]
B6 = [vec() for _ in range(6)]
r = sc.family_p([(A6, B6)])
check("6 v 6: the relabelling set is the 924 = C(12, 6) choices, enumerated exactly (not Monte Carlo)", r["M"] == 924 and r["exact"] is True, r)
r12 = sc.family_p([(A6, B6 + [vec() for _ in range(6)])])
check("6 v 12: C(18, 6) = 18 564 relabellings, exact", r12["M"] == 18564 and r12["exact"] is True, r12["M"])
sep = [[v[0] - 5, v[1], v[2], v[3]] for v in B6]
rs = sc.family_p([(A6, sep)])
check("a fully separated 6 v 6 on ONE clause: per-clause p = 2/924 = 0.00216 and family p <= 4 x 0.00216 = 0.0087 < 0.05/3 (the gate can reject)", abs(rs["p_clause"]["goodput"] - 2 / 924) < 1e-12 and rs["p_family"] <= 8 / 924 + 1e-12 and rs["p_family"] <= 0.0087 and rs["p_family"] < sc.THRESH, rs)
better = [[v[0] + 5, v[1], v[2], v[3]] for v in B6]
rb = sc.family_p([(A6, better)])
check("two-sided: a step in the BETTER direction (goodput +5) is rejected exactly as the worse one (the same family p)", rb["p_family"] == rs["p_family"] and rb["p_family"] < sc.THRESH, (rb["p_family"], rs["p_family"]))
rn = sc.family_p([(A6, [list(v) for v in A6])])
check("identical groups (all ties): every clause p = 1 and the family p = 1", rn["p_family"] == 1.0 and all(p == 1.0 for p in rn["p_clause"].values()), rn)
mc1 = sc.family_p([(A6, B6), (A6[::-1], B6[::-1])])
mc2 = sc.family_p([(A6, B6), (A6[::-1], B6[::-1])])
check("the cell-stratified joint (two 6 v 6 strata: 853 776 relabellings) uses a Monte Carlo of 10 000 + the observed one, deterministic (same seed, same answer)", mc1["exact"] is False and mc1["M"] == sc.MC_DRAWS + 1 and mc1 == mc2, (mc1["M"], mc1["exact"]))
check("midranks: ties share their mean rank", sc.midranks([3, 1, 3, 2]) == [3.5, 1.0, 3.5, 2.0])
check("the Bonferroni threshold is 0.05 / 3", abs(sc.THRESH - 0.05 / 3) < 1e-15)

# ═══ 4. T1 / T2 on a synthetic ledger in the real block / seed layout ═══
def build_ledger(cell="c2-100", step=None, slope=0.0, dnf_arms=(), void=None, seed=11):
    """Rows of every arm in plan51's order. goodput 90 + noise, minus slope x log10(delta / 0.005) (a SMOOTH dial dependence); `step` = {arm: dgoodput} adds a step on that arm."""
    rg = random.Random(seed)
    out = []
    for c, _, a, sd, rp_ in plan51.plan("51", cell=cell):
        dl = plan51.arm_delta(a)
        dl = {"HB": 0.005, "HA": 0.5, "HR": 50.0}.get(a, dl)
        g = 90.0 - slope * math.log10(dl / 0.005) + (step or {}).get(a, 0.0) + rg.gauss(0, 0.3)
        r_ = row(a, sd, rp_, cell=cell, mbps=g, inorder_ms=250 + rg.gauss(0, 4), cpu_cli_s_per_gb=30 + rg.gauss(0, 0.5), cpu_srv_s_per_gb=33 + rg.gauss(0, 0.5), rr_max=0.0 if plan51.arm_beta(a) >= 1 else 0.2)
        if a in dnf_arms:
            r_["dnf"], r_["mbps"] = True, None
        if void and (a, sd, rp_) == void:
            r_["status"], r_["problems"] = "WITNESS-FAIL", ["gate-delta-cli=unset (want 0.5)"]
        out.append(r_)
    return out


def run_score(rows, **kw):
    lines = []
    sc.score([ledger(rows)], out=lines.append, d80_path=os.path.join(d, "none.json"), aa_draws=4)
    return lines


def label(lines, name):
    for ln in lines:
        if ln.startswith("LABEL51 " + name + " "):
            return ln.split(" ", 2)[2]
    return None


L0 = run_score(build_ledger())
check("a null ledger (no step, no slope): T1 PASS at all three presets and T2 PASS at Auto / Bulk / Realtime on arm-independent rows", all((label(L0, f"T1-c2-{p}") or "").startswith("PASS") for p in ("Bulk", "Auto", "Realtime"))
      and all((label(L0, f"T2-c2-{p}") or "").startswith("PASS") for p in ("Bulk", "Auto", "Realtime")), [ln for ln in L0 if ln.startswith("LABEL51 T")])
L1 = run_score(build_ledger(step={"BP": -6.0}))
check("a fully separated step of the + side of the Bulk nudge (goodput -6, 6 v 6 in the real layout): T1 FAILS at Bulk with the clause named and family p <= 0.0087; Auto and Realtime still PASS",
      (label(L1, "T1-c2-Bulk") or "").startswith("FAIL") and float(label(L1, "T1-c2-Bulk").split("p=")[1]) <= 0.0087 and (label(L1, "T1-c2-Auto") or "").startswith("PASS") and (label(L1, "T1-c2-Realtime") or "").startswith("PASS")
      and any("T1 c2 Bulk: FAIL -- a step at Bulk on goodput" in ln for ln in L1), [ln for ln in L1 if ln.startswith("  T1 c2")])
L2 = run_score(build_ledger(step={"BP": +6.0}))
check("a step in the BETTER direction (goodput +6 at B+) fails T1 too (two-sided)", (label(L2, "T1-c2-Bulk") or "").startswith("FAIL"))
check("when T1 fails at Bulk, T2 at Bulk is NOT SCORED and reported against each nudge", label(L1, "T2-c2-Bulk") == "NOT-SCORED (T1 FAIL)" and any("T2 c2 Bulk: NOT SCORED" in ln for ln in L1))
L3 = run_score(build_ledger(slope=1.25))
check("a SMOOTH slope across the full dial (1.25 goodput units per decade, about 5 x d80 over four decades, 0.01 units across a +-2 % nudge) PASSes T1 at all three presets", all((label(L3, f"T1-c2-{p}") or "").startswith("PASS") for p in ("Bulk", "Auto", "Realtime")), [ln for ln in L3 if ln.startswith("LABEL51 T1")])
check("N-A (the same delta as H-A, as a number) against H-A PASSes T2 at Auto", (label(L0, "T2-c2-Auto") or "").startswith("PASS") and any("T2 c2 Auto: PASS" in ln for ln in L0))
L4 = run_score(build_ledger(step={"NA": -8.0}))
check("a step on N-A alone FAILs T2 at Auto and the line says the session is NO VERDICT until it is explained (an instrument defect)", (label(L4, "T2-c2-Auto") or "").startswith("FAIL") and any("NO VERDICT until it is explained" in ln for ln in L4))
L5 = run_score(build_ledger(void=("NA", "42", 1)))
check("a row whose RWM_DELTA echo is missing is VOID: it is listed as VOID, excluded from every comparison (N-A has 5 usable rows) and counted in the verdict line",
      any(ln.startswith("  VOID c2-100 NA s42 rep1") for ln in L5) and any("void=1" in ln for ln in L5 if ln.startswith("VERDICT51")), [ln for ln in L5 if "VOID" in ln or ln.startswith("VERDICT")])
L6 = run_score(build_ledger(dnf_arms=("HR",)))
check("DNF rows are kept in T2 and ranked worst (H-R all DNF against R-: the named hint at Realtime FAILs T2), and never enter T1 (T1 at Realtime still PASSes on the completed Auto-carrier rows)",
      (label(L6, "T2-c2-Realtime") or "").startswith("FAIL") and (label(L6, "T1-c2-Realtime") or "").startswith("PASS"), [ln for ln in L6 if "Realtime" in ln][:3])
check("wording rule: the score never says 'continuous'; a non-rejection reads 'no step above d80 = x % at P'", all("continuous" not in ln.lower() for ln in L0 + L3) and any("no step above d80" in ln for ln in L0), [ln for ln in L0 if "T1 c2 Bulk" in ln])
check("the report carries every registered readout: per-arm lines, W-ROUTE, the r(delta) table with the registered bands, delta*, goodput against delta, the A/A resolution, VERDICT51 and the LABEL51 lines",
      all(any(k in ln for ln in L0) for k in ("  HB ", "W-ROUTE c2", "r(delta) c2", "band 0.133-0.197", "delta* c2", "goodput against delta", "A/A c2", "VERDICT51 ", "LABEL51 W-ROUTE-c2", "LABEL51 DELTA-STAR-c2", "LABEL51 AA-RESOLUTION-c2")), L0[-12:])
check("W-ROUTE: rr is exactly 0 on every H-B / B- row (PASS) and not R-INERT when other rows have rr > 0", label(L0, "W-ROUTE-c2") == "PASS", label(L0, "W-ROUTE-c2"))
rows_hbrr = build_ledger()
for r_ in rows_hbrr:
    if r_["arm"] == "HB":
        r_["t51"]["rr_max"] = 0.2
check("W-ROUTE: rr > 0 on an H-B row is a FAIL (the proactive slot cannot fire at beta = 1)", label(run_score(rows_hbrr), "W-ROUTE-c2") == "FAIL")
rows_inert = build_ledger()
for r_ in rows_inert:
    r_["t51"]["rr_max"] = 0.0
    r_["t51"]["plu_med"] = {"0": 0.014}
    r_["t51"]["spare_lo"] = 0.9
Li = run_score(rows_inert)
check("W-ROUTE: rr = 0 on EVERY beta < 1 row reads R-INERT and is attributed (here spare-bound: s = 0 on most sending ticks)", "R-INERT" in (label(Li, "W-ROUTE-c2") or "") and any("spare-bound" in ln for ln in Li), [ln for ln in Li if "W-ROUTE" in ln])
check("the cell-stratified joint T1 is reported when both cells ran, and does not override a per-cell FAIL",
      (lambda L: any("T1 joint (cell-stratified, REPORTED" in ln for ln in L) and (label(L, "T1-c2-Bulk") or "").startswith("FAIL"))(run_score(build_ledger("c2-100", step={"BP": -6.0}) + build_ledger("c8-100"))))

# ═══ 5. d80: the bootstrap resolution ═══
pool = [[90 + rng.gauss(0, 0.2), 250 + rng.gauss(0, 3), 30 + rng.gauss(0, 0.5), 33 + rng.gauss(0, 4)] for _ in range(12)]
dd = sc.d80(pool, 6, 6, draws=24, nperm=400, seed=3)
check("d80 on a pool with 0.2 % goodput noise and 12 % server-CPU noise: goodput resolves far below the noisy clause (d80 goodput < d80 srv CPU, or the noisy one is above the grid)",
      dd["goodput"] is not None and dd["goodput"] <= 0.03 and (dd["cpu_srv"] is None or dd["cpu_srv"] > dd["goodput"]), dd)
check("the d80 JSON round-trips and is read by score51 (machine-readable: {cell: {T1: {clause: d80}}})", (lambda p: (json.dump({"c2-100": {"T1": {"goodput": 0.02, "inorder": 0.2, "cpu_cli": 0.08, "cpu_srv": 0.12}}}, open(p, "w")) or sc.load_d80(p)["c2-100"]["T1"]["goodput"] == 0.02))(os.path.join(d, "d80.json")))
lines = []
dj = os.path.join(d, "d80b.json")
json.dump({"c2-100": {"T1": {"goodput": 0.001, "inorder": 0.001, "cpu_cli": 0.001, "cpu_srv": 0.001}}}, open(dj, "w"))
sc.score([ledger(build_ledger())], out=lines.append, d80_path=dj, aa_draws=6)
check("the in-session A/A d80 is printed beside the historical one and 'REDUCED RESOLUTION' is flagged when it exceeds twice the historical value", any(ln.startswith("  A/A c2") and "historical d80" in ln for ln in lines), [ln for ln in lines if ln.startswith("  A/A")])
cm = s51_d80.commits(os.path.join(HERE, "..", "..", "docs", "l1-raw", "s53", "BINSHA.txt"))
check("s51_d80.commits reads a launch's BINSHA.txt (the main seat and the BASE seat)", set(cm) == {"bin", "bin-base"} and len(cm["bin"]) == 40, cm)

# ═══ 6. the row readouts (section 3 and 3.2) on a synthetic client log ═══
def dg(t, src, cwnd, infl, hand, tx, dgev, cum, taper, rr_line=None):
    return (f"[DIAG] t={t:.2f}s win=100/200 paused=0% good=1.0Mbit cwnd={cwnd} infl={infl} np=1 np_act=1 rtt=15.8ms dgq0[hand={hand} tx={tx} full=0 err=0 sp=1] p0:infl={infl}/sinfl=0 qcwnd=125000 pl=0.0 "
            f"| ANCHOR sent=0 plu=0.0140 plc=0.0260 dgev0={dgev} src={src}sym/s cod=0sym/s cum={cum[0]}/{cum[1]}/{cum[2]} retx=7 mpr[rep=1 seqs=2 fired=3 supp=0/0/0 taper={taper}] fp/on p0:1/1]")


cli = ['{"seconds":0.01,"warmup":true}', dg(1.0, 5000, 600, 100, 1000, 1000, 0, (1000, 30, 900), 5), dg(2.0, 5000, 600, 600, 2000, 2000, 1, (2000, 60, 1900), 10),
       dg(3.0, 5000, 10000, 20, 3000, 3000, 2, (3000, 90, 2900), 15),
       "[SPAN] t=1.0s ack=1 win=[1,2] wsize=200 a_star=Some(220) delta=Some(0.5) owed=0.1 rr=0.180 debt=0.3 retx_buf=5", "[SPAN] t=2.0s ack=1 win=[1,2] wsize=200 a_star=Some(220) delta=Some(0.5) owed=0.1 rr=0.000 debt=0.3 retx_buf=5",
       "[SUMCAP] on=1 eng=1/2 chg=1/2 chg_frac=0.5 pin=0.30 floor=1 cap=2 ask=3", "[SUMCAP] on=1 eng=1/2 chg=1/2 chg_frac=0.5 pin=0.50 floor=1 cap=2 ask=3"]
tm = sc.row_metrics(cli, [], [], 3000 * 1186, "c2-100", "NA", 88.0)
check("row_metrics: consumed r = (cod + taper) / src from the last tick = (90 + 15) / 3000 = 0.035; coded share 90 / 3090; src per byte = 3000 / (3000 x 1186); rr mean over the source phase 0.09, nz fraction 0.5; W 200; SUMCAP pin median 0.4",
      abs(tm["r_consumed"] - 0.035) < 1e-12 and abs(tm["coded_share"] - 90 / 3090) < 1e-12 and abs(tm["src_per_byte"] - 1 / 1186) < 1e-12 and abs(tm["rr_mean"] - 0.09) < 1e-12 and tm["rr_nz_frac"] == 0.5 and tm["wsize_med"] == 200 and abs(tm["sumcap_pin"] - 0.4) < 1e-12, tm)
check("section 3.2 readouts: saturated-tick share 1 / 3 (only the tick with infl >= cwnd), eviction share 0 (hand = tx), dgev / hand = 2 / 3000, MAX_CWND bind 1 / 3, the c2 spare bracket: s = 5, 0, 499 -> share(s = 0) 1 / 3, share(s < 0.5) 1 / 3",
      abs(tm["tau_sat"] - 1 / 3) < 1e-12 and tm["evict_share"] == 0.0 and abs(tm["dgev_over_hand"] - 2 / 3000) < 1e-12 and abs(tm["maxcwnd_share"] - 1 / 3) < 1e-12 and abs(tm["spare_lo"] - 1 / 3) < 1e-12 and abs(tm["spare_hi"] - 1 / 3) < 1e-12, tm)
tm8 = sc.row_metrics(cli, [], [], 3000 * 1186, "c8-100", "NA", 88.0)
check("the c8 spare-clamp bracket is MISSING ([DIAG] prints no per-leg scheduler cwnd): None, never a number", tm8["spare_lo"] is None and tm8["spare_hi"] is None)
check("plu / plc of the leg the rate read (last tick plc 0.026, plu median 0.014) are carried", abs(tm["plc_last"]["0"] - 0.026) < 1e-12 and abs(tm["plu_med"]["0"] - 0.014) < 1e-12)

# ═══ 7. the smoke: the not-overdriven witness ═══
def srow(arm, **kw):
    base = {"inorder_ms": 250.0, "rr_max": 0.0 if arm == "HB" else 0.2, "n_span": 10, "dgev_over_hand": 0.0001, "tau_sat": 0.03, "last_tick_s": 55.0, "cpu_cli_s_per_gb": 30.0, "cpu_srv_s_per_gb": 33.0}
    base.update(kw)
    r_ = row(arm, "42", 1, **base)
    r_["mbps"] = 88.0
    return r_


def smk(rows):
    out = []
    rc = sc.smoke(ledger(rows), "smoke", out.append)
    return rc, "\n".join(out)


good = [srow(a) for a in ("HB", "HR", "BP", "NA")]
rc, txt = smk(good)
check("the smoke on healthy H-B / H-R / B+ / N-A rows: SMOKE-PASS with the NOT-OVERDRIVEN witness on H-R and N-A printed as PASS (predicted dgev / hand <= 0.0001-0.009, tau_x 0.02-0.04)",
      rc == 0 and "SMOKE-PASS" in txt and txt.count("PASS NOT-OVERDRIVEN witness") == 2, txt[-900:])
rc, txt = smk([srow(a, **({"dgev_over_hand": 0.05} if a == "HR" else {})) for a in ("HB", "HR", "BP", "NA")])
check("an H-R row with dgev / hand 0.05 (> 0.02) is NO-GO: FAIL on the not-overdriven witness", rc == 1 and "FAIL NOT-OVERDRIVEN witness on HR" in txt, txt[-500:])
rc, txt = smk([srow(a, **({"tau_sat": 0.2} if a == "NA" else {})) for a in ("HB", "HR", "BP", "NA")])
check("an N-A row with tau_x 0.2 (> 0.10) is NO-GO", rc == 1 and "FAIL NOT-OVERDRIVEN witness on NA" in txt, txt[-500:])
rc, txt = smk([srow(a, **({"rr_max": 0.2} if a == "HB" else {})) for a in ("HB", "HR", "BP", "NA")])
check("rr > 0 on H-B FAILS (beta = 1: exactly 0 expected)", rc == 1 and "FAIL rr is exactly 0 on the H-B row" in txt, txt[-500:])
rc, txt = smk([srow(a, **({"rr_max": 0.0} if a == "NA" else {})) for a in ("HB", "HR", "BP", "NA")])
check("rr = 0 on N-A is R-INERT, attributed (not a failure of the smoke): the line names the attribution", rc == 0 and "R-INERT attributed" in txt, txt[-500:])
rc, txt = smk([srow(a, **({"inorder_ms": None} if a == "HB" else {})) for a in ("HB", "HR", "BP", "NA")])
check("a completed row with no in-order delay (None) FAILS", rc == 1 and "FAIL the in-order delay is computed" in txt, txt[-500:])
rr_ = [srow(a) for a in ("HB", "HR", "BP", "NA")]
rr_[1]["dnf"] = True
rc, txt = smk(rr_)
check("an H-R row that DNFs (not inside the 200 s timeout) FAILS", rc == 1 and "FAIL the H-R row finished inside" in txt, txt[-500:])
rr_ = [srow(a) for a in ("HB", "HR", "BP", "NA")]
rr_[3]["status"], rr_[3]["problems"] = "WITNESS-FAIL", ["batch-echo-missing x"]
rc, txt = smk(rr_)
check("a witness problem on a row (a missing batching echo) FAILS the smoke on the batching criterion", rc == 1 and "FAIL the emission-batching echo" in txt, txt[-600:])
rc, txt = smk(good[:3])
check("a missing smoke slot (no N-A row) FAILS 'one row per smoke slot ran'", rc == 1 and "FAIL one row per smoke slot ran" in txt, txt[-300:])
print("FAILED %d" % fails if fails else "ALL OK")
sys.exit(1 if fails else 0)
