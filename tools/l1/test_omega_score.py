#!/usr/bin/env python3
"""Unit tests of omega_score.py, omega_parse.py, omega_cells.py and omega_dist.py on synthetic ledgers. No root, no network, no VM.
Exit code = the number of failures (tools/ci/run_py_tests.py convention). Run: python -I test_omega_score.py"""
import json
import os
import random
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import omega_cells as oc  # noqa: E402
import omega_dist as od  # noqa: E402
import omega_parse as op  # noqa: E402
import omega_score as sc  # noqa: E402

fails = 0
BOOT = 150


def check(name, ok, detail=""):
    global fails
    print(("ok   " if ok else "FAIL ") + name + ((" -- " + str(detail)[:900]) if not ok else ""), flush=True)
    if not ok:
        fails += 1


# ───────────── synthetic ledger ─────────────
def mix(mean, n):
    """n non-negative ints whose sum is round(mean * n)."""
    tot = int(round(mean * n))
    base, extra = divmod(tot, n)
    return [base + 1] * extra + [base] * (n - extra)


def ev(D, rank=1, rin=1, free=False, spurious=0, drops=0, valid=True):
    return {"D": D, "idle": 0 if free else D, "retx": 0, "drops": drops, "spurious": spurious, "valid": valid, "rank": rank, "rank_in_rep": rin, "flow": "f"}


def rel_events(d1, dinf, **kw):
    a, b = mix(d1, 8), mix(dinf, 4)
    return [ev(x, 1, i + 1, **kw) for i, x in enumerate(a)] + [ev(x, 2, 9 + i, **kw) for i, x in enumerate(b)]


def hold_events(dh, first=None, **kw):
    ds = mix(dh, 12)
    out = [ev(x, 1, i + 1, **kw) for i, x in enumerate(ds)]
    if first is not None:
        out[0]["D"], out[0]["idle"] = first, first
    return out


def row(cfg, arm, rep, kind, label, dt, **kw):
    r = {"kind": "row", "cfg": cfg, "arm": arm, "armkind": kind, "label": label, "dt_ms": dt, "rep": rep, "seedhalf": 1 if rep <= 4 else 2, "stage": "D",
         "status": "OK", "tau_q_ms": 5.0, "B": 50, "witness": {"ok": True}}
    r.update(kw)
    return r


LABELS = {"k_a=3": 0.35, "t=1": 3.5, "rule=11": 11.0}      # one within-side and one beyond-side hold for Auto; the first is Realtime's


def spec_default(dR=1.0, dH=0.0, dC=0.0):
    return {lab: {"dR": dR, "dRinf": dR, "dH": dH, "dC": dC} for lab in LABELS}


def build(path, specs, reps=8, state="DONE-ALL", header=None, mut=None):
    """specs: {cfg: {label: {dR, dRinf, dH, dC, ...}}}. Writes a ledger; mut(row) may edit each row."""
    lines = [dict({"kind": "header", "d_ms": 11.0, "sigma_R_ms": 4.0, "skew_ms": 20.0}, **(header or {}))]
    for cfg, labs in specs.items():
        crow = lambda rep, lab=None: None
        for rep in range(1, reps + 1):
            ps = {lab: [ev(x, 1, i + 1) for i, x in enumerate(mix(s.get("dC", 0.0), 12))] for lab, s in labs.items()}
            lines.append(row(cfg, "CLEAN", rep, "CLEAN", "CLEAN", 0.0, pseudo=ps, good_bytes=1e8, window_s=12.0))
            for lab, s in labs.items():
                dt = LABELS.get(lab, 3.5)
                kr = s.get("rel_kw", {})
                kh = s.get("hold_kw", {})
                lines.append(row(cfg, "REL@" + lab, rep, "REL", lab, dt, events=rel_events(s["dR"], s["dRinf"], **kr), n_events=12,
                                 good_bytes=s.get("GR", 1e8), window_s=12.0))
                lines.append(row(cfg, "HOLD@" + lab, rep, "HOLD", lab, dt, events=hold_events(s["dH"], first=s.get("first"), **kh), n_events=12,
                                 good_bytes=s.get("GH", 1e8), window_s=12.0))
    if mut:
        for r in lines:
            if r.get("kind") == "row":
                mut(r)
    if state:
        lines.append({"kind": "footer", "state": state})
    with open(path, "w") as f:
        for l in lines:
            f.write(json.dumps(l) + "\n")


CFGS_QT = ["g1000-rtt10-Qt", "g1000-rtt50-Qt", "g10000-rtt10-Qt", "g10000-rtt50-Qt"]


def run(specs, **kw):
    tmp = tempfile.mkdtemp(prefix="omega-test-")
    p = os.path.join(tmp, "ledger.jsonl")
    nboot = kw.pop("nboot", BOOT)
    build(p, specs, **kw)
    return sc.score(p, nboot, 1), p


def text(o):
    return sc.render(o)


def allq(spec, cfgs=CFGS_QT):
    return {c: spec for c in cfgs}


# ───────────── pure functions ─────────────
check("n formula: small variances clamp to 8", sc.n_formula(0.001, 0.001, 5)[0] == 8)
n, raw = sc.n_formula(40.0, 40.0, 5)
check("n formula: large variances clamp to 24 and report the raw n (POWER-LIMITED)", n == 24 and raw > 24, (n, raw))
check("n formula: the chi-square guard inflates a low estimate", sc.n_formula(1.0, 1.0, 3)[1] >= sc.n_formula(1.0, 1.0, 40)[1])
check("design effect is 1 for independent reps and > 1 for clustered ones", sc.design_effect([(3, 10)] * 6) == 1.0 and sc.design_effect([(0, 10), (10, 10)] * 3) > 5)
check("min events constants are the registered ones", (sc.MIN_LIVE_REPS, sc.MIN_EVENTS) == (6, 32))

for lo, hi, want in [(-0.1, 0.004, "OMEGA-Z0"), (0.01, 0.2, "OMEGA-ZB"), (0.3, 0.4, "OMEGA-GAP-BA"), (1.0, 2.0, "OMEGA-ZA"), (30, 40, "OMEGA-GAP-AR"),
                     (100, 200, "OMEGA-ZR"), (2700, 3000, "OMEGA-Z6"), (0.2, 0.3, "OMEGA-EDGE-0.26"), (0.4, 0.6, "OMEGA-EDGE-0.5"),
                     (0.1, 0.6, "NEEDS-MORE-POWER"), (0.0, 0.3, "NEEDS-MORE-POWER")]:
    check("classify [%g, %g] -> %s" % (lo, hi, want), sc.classify(lo, hi) == want, sc.classify(lo, hi))
check("zone edges +-1%: just inside ZB / GAP-BA", sc.classify(0.2574, 0.2574) == "OMEGA-ZB" and sc.classify(0.2626, 0.2626) == "OMEGA-GAP-BA")
check("zone edges +-1%: 0.5 edge", sc.classify(0.495, 0.495) == "OMEGA-GAP-BA" and sc.classify(0.505, 0.505) == "OMEGA-ZA")
d = sc.decide_9_2("OMEGA-EDGE-0.26")
check("EDGE: decided where the adjacent rows agree, NEEDS-MORE-POWER where they differ", d["Bulk"][0] == "NEEDS-MORE-POWER" and d["Realtime"][0] == "RELEASE-AT-DETECT" and d["Auto"][0] == "NEEDS-MORE-POWER", d)
d = sc.decide_9_2("OMEGA-ZA")
check("9.2 ZA: Auto STEEP default change, Bulk keeps order", d["Bulk"] == ("KEEPS-ORDER", False) and d["Auto"] == ("STEEP-AT-Auto", True))
check("9.2 Z0: default change at Bulk and Auto, Realtime as today", sc.decide_9_2("OMEGA-Z0")["Bulk"][1] and not sc.decide_9_2("OMEGA-Z0")["Realtime"][1])
check("9.2 Z6: Realtime holds longer than today", sc.decide_9_2("OMEGA-Z6")["Realtime"][0] == "HOLD-THROUGH-FIRST-REPAIR")

r = sc.rent("Auto", 11.0, 11.0)
check("rent line: lower CI 1% above the rent -> UNDERPRICED", sc.rent_line("Auto", "beyond", 5, r * 1.01, 9, r, 0.0).startswith("RENT-UNDERPRICED"))
check("rent line: upper CI 1% below the rent -> COVERED", sc.rent_line("Auto", "within", 0, -1, r * 0.99, r, 0.0).startswith("RENT-COVERED"))
check("rent line: CI straddling the rent -> UNDECIDED", sc.rent_line("Auto", "within", r, r * 0.9, r * 1.1, r, 0.0).startswith("RENT-UNDECIDED"))
check("rent line: an A/A floor above the effect blocks UNDERPRICED", sc.rent_line("Auto", "beyond", 1.0, r * 1.5, 3, r, 5.0).startswith("RENT-UNDECIDED"))
check("rent line: no A/A floor, no claim", sc.rent_line("Auto", "beyond", 9, 8, 10, r, None).startswith("RENT-UNDECIDED"))
check("rent: 0.023 symbols at Bulk, dt 50 ms, d 11 ms", abs(sc.rent("Bulk", 50, 11) - 0.0227) < 1e-3)
check("Bulk count rule: COVERED", sc.bulk_line("within", 0.01, 0.0, 0.5, 0.0, 0.03).startswith("RENT-COVERED-Bulk"))
check("Bulk count rule: UNDERPRICED", sc.bulk_line("beyond", 1.0, 0.9, 40.0, 30.0, 0.03).startswith("RENT-UNDERPRICED-Bulk-beyond"))
check("Bulk count rule: UNDECIDED", sc.bulk_line("beyond", 0.9, 0.0, 40.0, 0.0, 0.03).startswith("RENT-UNDECIDED-Bulk"))
check("combine: NO-ARMS for an empty side", sc.combine_lines("Realtime", "beyond", []) == "NO-ARMS-Realtime-beyond")
check("combine: UNDERPRICED at one arm wins", sc.combine_lines("Auto", "beyond", ["RENT-COVERED-Auto-beyond", "RENT-UNDERPRICED-Auto-beyond"]).startswith("RENT-UNDERPRICED"))
check("envelope: one noisy arm cannot lift it", sc.envelope([(1.0, 1.4), (0.9, 1.3), (-50.0, 80.0)]) == (1.0, 80.0) or sc.envelope([(1.0, 1.4), (0.9, 1.3), (-50.0, 1.5)]) == (1.0, 1.5))
check("VARIES-RATE: different single zones, disjoint", sc.varies([("a", (1, 2), "b", (100, 200), "RATE")]) == [("OMEGA-VARIES-RATE", "a", "b")])
check("VARIES: the same zone is not varying", sc.varies([("a", (1, 2), "b", (3, 4), "RTT")]) == [])
check("VARIES: an interval across a boundary is not a single zone", sc.varies([("a", (0.1, 1.0), "b", (100, 200), "Q")]) == [])
check("STEEP-AT-Auto: omega_1 on round 1's edge", sc.steep("Auto", (0.49, 0.51), None, 11.0, 4.0) is not None)
check("no STEEP away from every band", sc.steep("Auto", (10, 11), None, 11.0, 4.0) is None)
check("skew: omega_inf below delta*skew/d reorders at Auto", sc.skew_verdict("Auto", (0.1, 0.3), 11.0) == "REORDERS-EVERY-SLOW-PATH-PACKET" and sc.skew_verdict("Auto", (1.0, 1.4), 11.0) == "KEEPS-ORDER")
check("chi-square 20% quantile close to the table (df=10: 6.18)", abs(sc.chi2_lower(10) - 6.18) < 0.1, sc.chi2_lower(10))

# ───────────── end to end on synthetic ledgers ─────────────
o, p = run(allq(spec_default()))
t = text(o)
check("scored: DELIVERED with c_R, c_H and omega_1 in ZA", o["measurement"] == "DELIVERED" and "OMEGA-ZA" in t and o["exit"] == 0, t)
check("scored: omega = c_R - c_H = MSS/S symbols (D 1)", abs(o["pairs"]["g1000-rtt10-Qt/k_a=3"]["om1"] - sc.K) < 0.01 and abs(o["pairs"]["g1000-rtt10-Qt/k_a=3"]["cH"]) < 1e-9, o["pairs"]["g1000-rtt10-Qt/k_a=3"])
check("scored: RENT-COVERED where c_H = 0", "RENT-COVERED-Auto-within at Q_t" in t and "RENT-COVERED-Auto-beyond at Q_t" in t, t)
check("scored: Realtime has no beyond-side arm -> NO-ARMS", "NO-ARMS-Realtime-beyond at Q_t" in t, t)
check("scored: 9.2 reads Auto = STEEP default change", "9.2 Auto: STEEP-AT-Auto [default change]" in t, t)

for dR, lab in [(0.0, "OMEGA-Z0"), (0.125, "OMEGA-ZB"), (0.375, "OMEGA-GAP-BA"), (1.0, "OMEGA-ZA"), (33.25, "OMEGA-GAP-AR"), (83.0, "OMEGA-ZR"), (2500.0, "OMEGA-Z6")]:
    o, _ = run(allq(spec_default(dR=dR)))
    check("ledger -> %s (omega_1 target %g)" % (lab, dR), "-> " + lab in text(o), text(o))

o, _ = run(allq({lab: {"dR": 0.0, "dRinf": 0.0, "dH": 10.0, "dC": 0.0, "GH": 1e8 - 12 * 1200 * 13} for lab in LABELS}))
t = text(o)
check("c_H 12 symbols beats the Auto rent -> RENT-UNDERPRICED-Auto-beyond at Q_t", "RENT-UNDERPRICED-Auto-beyond at Q_t" in t, t)
check("c_H > c_R -> OMEGA-NEGATIVE", "OMEGA-NEGATIVE" in t, t)
check("with RENT-UNDERPRICED at every side, 9.2 is not read at that preset", "9.2 Auto: NOT-READ" in t, t)

specs = allq(spec_default())
for c in ("g1000-rtt10-Qs", "g10000-rtt50-Qs"):
    specs[c] = {lab: dict(s, dH=15.0, GH=1e8 - 12 * 1200 * 20) for lab, s in spec_default().items()}
o, _ = run(specs)
t = text(o)
check("RENT-UNDERPRICED at Q_s only is tagged 'only'", "RENT-UNDERPRICED-Auto-beyond at Q_s only" in t, t)

sp = {c: spec_default() for c in CFGS_QT}
sp["g10000-rtt50-Qt"] = spec_default(dR=100.0)
o, _ = run(sp)
check("OMEGA-VARIES-RATE / -RTT between configurations in different zones", "OMEGA-VARIES-RATE" in text(o) and "OMEGA-VARIES-RTT" in text(o), text(o))
sp = {c: dict(spec_default(), **{"t=1": {"dR": 100.0, "dRinf": 100.0, "dH": 0.0, "dC": 0.0}}) for c in CFGS_QT}
o, _ = run(sp)
check("OMEGA-VARIES-DT within a configuration", "OMEGA-VARIES-DT" in text(o), text(o))
sp = allq(spec_default())
sp["g1000-rtt10-Qs"] = spec_default(dR=100.0)
o, _ = run(sp)
check("OMEGA-VARIES-Q between Q_t and Q_s", "OMEGA-VARIES-Q" in text(o), text(o))

# window short
sp = allq({lab: {"dR": 1.2, "dRinf": 1.2, "dH": 0.0, "dC": 0.0, "GR": 1e8 - 12 * 1200 * 50} for lab in LABELS})
o, _ = run(sp)
check("WINDOW-SHORT when the goodput guard exceeds the slot omega_inf", "WINDOW-SHORT" in text(o), text(o))
o, _ = run(allq(spec_default()))
check("no WINDOW-SHORT when goodput agrees", "WINDOW-SHORT" not in text(o))

# hold-rank
sp = allq({lab: {"dR": 1.2, "dRinf": 1.2, "dH": 20.0, "dC": 0.0, "first": 0} for lab in LABELS})
o, _ = run(sp)
check("HOLD-RANK-DEPENDENT when the first hold differs from later ones", "HOLD-RANK-DEPENDENT" in text(o), text(o))
o, _ = run(allq(spec_default()))
check("no HOLD-RANK-DEPENDENT on equal holds", "HOLD-RANK-DEPENDENT" not in text(o))

# Bulk, end to end: free holds are COVERED within; non-free holds with an excess x are UNDERPRICED
sp = allq({lab: {"dR": 1.2, "dRinf": 1.2, "dH": 0.0, "dC": 0.0, "hold_kw": {"free": True}} for lab in LABELS})
o, _ = run(sp, reps=24)
check("Bulk: free holds -> RENT-COVERED-Bulk-within", "RENT-COVERED-Bulk-within at Q_t" in text(o), text(o))
sp = allq({lab: {"dR": 1.2, "dRinf": 1.2, "dH": 30.0, "dC": 0.0} for lab in LABELS})
o, _ = run(sp, reps=24)
check("Bulk: every hold idles the link -> RENT-UNDERPRICED-Bulk-beyond", "RENT-UNDERPRICED-Bulk-beyond at Q_t" in text(o), text(o))

# failure paths: truncated, footerless, rehearsal, missing, void, under-min, too few configurations
o, p = run(allq(spec_default()), state="TRUNCATED-D1")
t = text(o)
check("truncated ledger with enough reps is scored at its n, every result line marked, never DELIVERED, never PASS",
      "OMEGA_1" in t and o["measurement"] == "TRUNCATED-D1" and "DELIVERED" not in t and "PASS" not in t and o["exit"] == 0
      and all("[TRUNCATED-D1 n_reached=8..8" in l for l in o["lines"][1:]) and o["lines"][0].startswith("TRUNCATED-D1 n_reached"), t)
o, _ = run(allq(spec_default()), state="TRUNCATED-D1", reps=3)
t = text(o)
check("truncated ledger under the live-rep minimum reads UNSCOREABLE, no omega, no zone verdict", "UNSCOREABLE" in t and "OMEGA_1" not in t and "DELIVERED" not in t and o["exit"] == 2 and o["pairs"]["g1000-rtt10-Qt/k_a=3"]["om1"] is None, t)
o, _ = run(allq(spec_default()), state=None)
check("footerless ledger is not scored (a crash is not a truncation)", "no footer" in text(o) and "OMEGA_1" not in text(o) and o["exit"] == 2, text(o))
os.makedirs(os.path.join(os.path.dirname(p), "x"), exist_ok=True)
o, _ = run(allq(spec_default()), header={"rehearsal": True})
check("a rehearsal ledger is never scored", "REHEARSAL-NOT-SCORED" in text(o) and o["exit"] == 2)
o, _ = run(allq(spec_default()), state="FAILED-ALL")
check("a FAILED-ALL ledger is not scored", o["exit"] == 2 and "OMEGA" not in text(o))


def drop_field(r):
    if r.get("arm") == "HOLD@t=1" and r["cfg"] == "g1000-rtt10-Qt" and r["rep"] <= 3:
        for e in r["events"]:
            del e["D"]


o, _ = run(allq(spec_default()), mut=drop_field)
check("missing field: the rows are MISSING, never read as 0 (cell UNSCOREABLE)", any("MISSING-FIELD g1000-rtt10-Qt/HOLD@t=1" in n for n in o["notes"]) and "UNSCOREABLE g1000-rtt10-Qt/t=1" in text(o) and o["pairs"]["g1000-rtt10-Qt/t=1"]["cH"] is None, text(o))


def void_some(r):
    if r.get("arm") == "REL@t=1" and r["cfg"] == "g1000-rtt10-Qt" and r["rep"] <= 3:
        r["status"] = "VOID-WITNESS"


o, _ = run(allq(spec_default()), mut=void_some)
check("void rows leave 5 live reps -> UNSCOREABLE; the other cells still score", o["pairs"]["g1000-rtt10-Qt/t=1"]["om1"] is None and o["pairs"]["g1000-rtt10-Qt/k_a=3"]["om1"] is not None, text(o))


def no_fill(r):
    if r["cfg"] == "g10000-rtt50-Qt" and r["armkind"] == "HOLD":
        r["status"] = "NO_FILL"


o, _ = run(allq(spec_default()), mut=no_fill)
check("NO_FILL reps are excluded; 3 of 4 Q_t configurations still score", o["measurement"] == "DELIVERED" and "g10000-rtt50-Qt" not in " ".join(k for k, v in o["pairs"].items() if v["om1"] is not None), text(o))


def void_stage(r):
    if r["rep"] <= 3:
        r["status"] = "VOID-WITNESS"


o, _ = run(allq(spec_default()), mut=void_stage)
check("a stage whose witness fails at >= 25% of reps is VOID", "VOID stage D" in text(o) and o["measurement"] != "DELIVERED", text(o))
o, _ = run(allq(spec_default()), reps=3)
check("under 6 live reps -> UNSCOREABLE, nothing read", o["measurement"] == "UNSCOREABLE" and "DELIVERED" not in text(o))
o, _ = run({c: spec_default() for c in CFGS_QT[:2]})
check("fewer than 3 of 4 Q_t configurations -> UNSCOREABLE", "UNSCOREABLE fewer than 3 of the 4 Q_t" in text(o) and o["measurement"] == "UNSCOREABLE", text(o))


def few_events(r):
    if r["armkind"] == "REL":
        r["events"] = [e for e in r["events"] if e["rank"] != 1 or e["rank_in_rep"] <= 3]     # 3 rank-1 events per rep x 8 = 24 < 32


o, _ = run(allq(spec_default()), mut=few_events)
check("fewer than 32 valid rank-1 REL events -> UNSCOREABLE", o["measurement"] == "UNSCOREABLE" and "rank-1 REL" in text(o), text(o))


def off_level(r):
    if r["armkind"] == "HOLD":
        for e in r["events"]:
            e["valid"] = False


o, _ = run(allq(spec_default()), mut=off_level)
check("OFF-LEVEL events are excluded: no valid HOLD events -> UNSCOREABLE", o["measurement"] == "UNSCOREABLE" and "valid HOLD events" in text(o), text(o))

# outage check
def out_ledger(r_rel, r_hold):
    rows = [{"kind": "header"}]
    for rep in range(1, 9):
        for arm, d in (("CLEAN", 0), ("OUT-REL", r_rel + (rep % 2)), ("OUT-HOLD", r_hold + (rep % 2))):
            rows.append(row("g10000-rtt50-Qt", arm, rep, arm, arm, 1000.0, stage="OUT", events=[ev(d)], n_events=1, pseudo={"x": [ev(d)]}))
    for cfg in CFGS_QT:
        for rep in range(1, 9):
            rows.append(row(cfg, "CLEAN", rep, "CLEAN", "CLEAN", 0.0, pseudo={"k_a=3": [ev(0)]}))
    rows.append({"kind": "footer", "state": "DONE-ALL"})
    return rows


def out_run(a, b):
    pth = os.path.join(tempfile.mkdtemp(prefix="omega-test-"), "l.jsonl")
    rows = out_ledger(a, b)
    for r in rows:
        if r.get("kind") == "row" and r["armkind"] == "CLEAN" and r["stage"] == "OUT":
            r["armkind"] = "CLEAN"
    with open(pth, "w") as f:
        f.write("\n".join(json.dumps(r) for r in rows) + "\n")
    return text(sc.score(pth, BOOT, 1))


check("outage: OUT-REL much costlier -> OUT-RELEASE-WORSE", "OUT-RELEASE-WORSE" in out_run(40, 2))
check("outage: OUT-REL cheaper -> OUT-RELEASE-BETTER", "OUT-RELEASE-BETTER" in out_run(2, 40))
check("outage: equal -> OUT-NULL", "OUT-NULL" in out_run(10, 10))

# stage T
def stage_t_direct(spur):
    cells = {}
    mk = lambda cfg, arm, kind, spu: [row(cfg, arm, i, kind, arm.split("@")[-1], 3.0, stage="T" if cfg == "T" else "D", events=[ev(1, spurious=spu)] * 4) for i in range(1, 9)]
    for cfg in ("T", "g10000-rtt10-Qt"):
        stage = "T" if cfg == "T" else "D"
        cl = [row(cfg, "CLEAN", i, "CLEAN", "CLEAN", 0.0, stage=stage, pseudo={"t=1": [ev(1)] * 4}) for i in range(1, 9)]
        cells[(stage, cfg, "CLEAN")] = cl
        for kind in ("REL", "HOLD"):
            cells[(stage, cfg, "%s@t=1" % kind)] = mk(cfg, "%s@t=1" % kind, kind, spur if (cfg == "T" and kind == "REL") else 0)
    return sc.stage_t(cells, {})


check("stage T: equal excess counts -> T-CONFIRMS", [l for l in stage_t_direct(0) if "@t=1" in l and "REL" in l and l.startswith("T-CONFIRMS")] != [], stage_t_direct(0))
check("stage T: excess spurious retransmissions differ by 3 -> T-DIVERGES", [l for l in stage_t_direct(3) if "REL@t=1" in l and l.startswith("T-DIVERGES")] != [], stage_t_direct(3))

# calibration
sp = {"g1000-rtt10-Qt": {"REL@k_f=8": 0, "HOLD@k_f=8": 0}}
pth = os.path.join(tempfile.mkdtemp(prefix="omega-test-"), "l.jsonl")
rng = random.Random(3)
with open(pth, "w") as f:
    for rep in range(1, 7):
        f.write(json.dumps(row("g1000-rtt10-Qt", "REL@k_f=8", rep, "REL", "k_f=8", 28.0, stage="calib", events=rel_events(1.0 + rng.random(), 1.0))) + "\n")
        f.write(json.dumps(row("g1000-rtt10-Qt", "HOLD@k_f=8", rep, "HOLD", "k_f=8", 28.0, stage="calib", events=hold_events(rng.random()))) + "\n")
cal = sc.calibrate(pth)
check("calibrate: n within [8, 24] with s2 and DE reported", 8 <= cal["g1000-rtt10-Qt"]["n"] <= 24 and "DE" in cal["g1000-rtt10-Qt"], cal)

# ───────────── cells ─────────────
cnt = oc.count_cells()
check("design: 130 + 100 + 3 + 5 = 238 cells (registration 6.2, 6.5, 6.6)", (cnt["Qt"], cnt["Qs"], cnt["outage"], cnt["stageT"], cnt["total"]) == (130, 100, 3, 5, 238), cnt)
check("design: Delta-t of k_a = 1 at 10000/s is 0.15 ms, t = 1 at RTT 50 is 12.5 ms",
      abs(next(a for a in oc.dt_arms("g10000-rtt10-Qt") if a["label"] == "k_a=1")["dt_ms"] - 0.15) < 1e-6 and abs(next(a for a in oc.dt_arms("g1000-rtt50-Qt") if a["label"] == "t=1")["dt_ms"] - 12.5) < 1e-6)
check("design: per-flow arms Delta-t = (k_f + 1/2) F / g: 28 ms at 1000/s, k_f 3", abs(next(a for a in oc.dt_arms("g1000-rtt10-Qt") if a["label"] == "k_f=3")["dt_ms"] - 28.0) < 1e-6)
check("design: the degenerate (1000/s, RTT 10, t=0.25) cell is not run", all(a["label"] != "t=0.25" for a in oc.dt_arms("g1000-rtt10-Qt")) and any(a["label"] == "t=0.25" for a in oc.dt_arms("g10000-rtt10-Qt")))
check("design: Q_s limit = max(ceil(W/4), 4)", oc.q_limit_pkts("g1000-rtt10-Qs", {}) == 4 and oc.q_limit_pkts("g10000-rtt50-Qs", {}) == 125)
check("design: 4 D sessions at n = 8 with a deep B_t", oc.plan_sessions({"B_t_pkts": {"1000": 4596, "10000": 4596}}) is not None and len(oc.plan_sessions({})) == 4, len(oc.plan_sessions({})))
check("design: every session fits 4.5 h", all(sum(oc.config_cost_s(c, hi - lo + 1) for c, lo, hi in s) <= oc.SESSION_CAP_S + 1 for s in oc.plan_sessions({})))
check("design: seeded order is reproducible and balanced per round", oc.config_rows("g1000-rtt10-Qt", {}) == oc.config_rows("g1000-rtt10-Qt", {}) and len({r[1] for r in oc.config_rows("g1000-rtt10-Qt", {}, (1, 1))}) == len(oc.arms("g1000-rtt10-Qt")))
check("design: p quantised to L: 1 s at 10000/s is 2 entries of 16384; S3's 4 s is not representable", od.entries_for(10000, 1.0, 16384) == 2 and od.entries_for(10000, 4.0, 16384) is None and od.entries_for(1000, 1.0, 16384) == 16)
check("design: SKIPPED-TABLE when L is too small", oc.event_spacing("g10000-rtt10-Qt", 1.0, {}, 4)[0] is None)
tb = od.table(16, 16384)
check("table: exactly the asked share of entries is delayed, spread evenly", tb.count(od.SCALE) == 16 and tb.index(od.SCALE) < 1100 and len(tb) == 16384)

# ───────────── parse ─────────────
def pcap_text(pk):
    return "".join("%.6f IP (tos 0x0, ttl 64, id %d, offset 0, flags [DF], proto TCP (6), length 1500)\n    10.77.0.1.%d > 10.77.0.2.5201: Flags [.], seq %d:%d, ack 1, win 502, length 1448\n"
                   % (t, ipid, 40000 + f, seq, seq + 1448) for t, f, seq, ipid in pk)


g, dt = 1000.0, 0.0105
pre, post = [], []
for i in range(400):
    t = 1000.0 + i / g
    f, seq, ipid = i % 2, (i // 2) * 1448, i + 10
    pre.append((t, f, seq, ipid))
    post.append((t + 0.0001 + (dt if i == 100 else 0.0), f, seq, ipid))
post.sort()
meta = {"cfg": "g1000-rtt10-Qt", "arm": "REL@k_a=8", "kind": "REL", "label": "k_a=8", "dt_ms": 10.5, "rep": 1, "seedhalf": 1, "stage": "D", "g": 1000, "rtt": 10, "q": "Qt",
        "limit": 100, "B": 50, "tau_q_ms": 50.0, "t_event": 1000.0,
        "witness": {"mid1_sent_delta": 400, "cli0_sent_delta": 400, "reorderer_in_force": True, "bottleneck_in_force": True, "sysctl_ok": True, "warm_ok": True}}
bl = "".join("%.3f 50\n" % (1000.0 + j / 100.0) for j in range(0, 500))
rowd = op.digest(meta, pcap_text(pre), pcap_text(post), bl)
e = rowd["events"]
check("parse: one delayed packet is one REL event with 10 overtakers", len(e) == 1 and e[0]["k_a"] == 10 and rowd["status"] == "OK", e)
check("parse: the event is valid at the standing level and ranked 1 in its flow", e[0]["valid"] is True and e[0]["rank"] == 1)
check("parse: no loss, no retransmission: D_e counts the idle slots of A_e only", e[0]["retx"] == 0 and e[0]["D"] == e[0]["idle"] and e[0]["drops"] == 0, e[0])
check("parse: an off-level backlog marks the event not valid", op.digest(meta, pcap_text(pre), pcap_text(post), bl.replace(" 50", " 5"))["events"][0]["valid"] is False)
check("parse: the witness passes (event delayed by dt)", op.row_void_reason(rowd) is None, rowd["witness"])
nw = op.digest(dict(meta, witness={"mid1_sent_delta": 400}), pcap_text(pre), pcap_text(post), bl)
check("parse: a missing witness field is reported missing, and the row is void (never read as true)", "cli0_sent_delta" in nw["witness_missing"] and "witness missing" in op.row_void_reason(nw), nw.get("witness_missing"))
nw = op.digest(dict(meta, witness=dict(meta["witness"], mid1_sent_delta=0)), pcap_text(pre), pcap_text(post), bl)
check("parse: a tc counter that did not move voids the row (the impairment was not in force)", "did not move" in (op.row_void_reason(nw) or ""))
def tcq(conf, pk, dr=0):
    return "qdisc netem 8001: root refcnt 2 %s\n Sent %d bytes %d pkt (dropped %d, overlimits 0 requeues 0)\n" % (conf, pk * 10, pk, dr)


w = op.witness_of("HOLD", 1000, 100, tcq("limit 100000 delay 0us 5ms rate 120Mbit", 1), tcq("limit 100000 delay 0us 5ms rate 120Mbit", 9),
                  tcq("limit 100 rate 12Mbit", 1), tcq("limit 100 rate 12Mbit", 9, 2))
check("witness: tc counters before and after prove the HOLD (rate) reorderer and the limit-100 bottleneck", w["reorderer_in_force"] and w["bottleneck_in_force"] and w["mid1_sent_delta"] == 8 and w["cli0_drops_delta"] == 2, w)
w = op.witness_of("REL", 1000, 100, "", "", "", "")
check("witness: absent tc output leaves every field absent", w == {}, w)
# HOLD: head plus 20 queued packets is one event
post_h = []
for i in range(400):
    t = 1000.0 + i / g
    d = (dt - (i - 100) / g) if 100 <= i <= 110 else 0.0
    post_h.append((t + 0.0001 + max(d, 0.0), i % 2, (i // 2) * 1448, i + 10))
post_h.sort()
mh = dict(meta, kind="HOLD", arm="HOLD@k_a=8")
rh = op.digest(mh, pcap_text(pre), pcap_text(post_h), bl)
check("parse: a HOLD stall of one head and queued successors is one event", len(rh["events"]) == 1, rh["events"])
rh["armkind"] = "HOLD"
check("parse: a HOLD rep with an overtaker is VOID-WITNESS", op.row_void_reason(dict(rh, witness={"delay_ok": True, "hold_overtakers": 3})) is not None)
# retransmission: original dropped upstream vs original seen
pre2 = list(pre)
post2 = list(post)
pre2.append((1000.0 + 0.5, 0, 0, 900))        # a segment already seen (seq 0 of flow 0): spurious
post2.append((1000.5 + 0.0001, 0, 0, 900))
pre2.sort()
post2.sort()
mr = dict(meta, dt_ms=10.5)
rr = op.digest(mr, pcap_text(pre2), pcap_text(post2), bl)
cl = op.classify_retx(op.keep_data_flows(op.parse_pcap_text(pcap_text(pre2))))
check("parse: a retransmission whose original reached mid0 is spurious", cl.count("spurious") == 1 and cl.count("drop") == 0, cl)
gap = [p for p in pre if not (p[1] == 0 and p[2] == 50 * 1448)]
gap.append((1000.45, 0, 50 * 1448, 901))       # the hole is refilled after the frontier moved on, and the original never reached mid0
gap.sort()
cl = op.classify_retx(op.keep_data_flows(op.parse_pcap_text(pcap_text(gap))))
check("parse: a retransmission whose original never reached mid0 is a bottleneck drop", cl.count("drop") == 1 and cl.count("spurious") == 0, cl)
t, st = op.warmup([(1000 + i / 100.0, 10.0 if i < 150 else 40.0) for i in range(400)], 50, 1.0, 60.0)
check("warm-up witness: median backlog at 0.75 B ends warm-up", st == "WITNESS" and t > 1001.5, (t, st))
t, st = op.warmup([(1000 + i / 100.0, 10.0) for i in range(400)], 50, 1.0, 3.0)
check("warm-up witness: never reaching 0.75 B is NO_FILL", st == "NO_FILL" and t is None)
clean_row = op.digest(dict(meta, kind="CLEAN", arm="CLEAN", label="CLEAN", dt_ms=0.0, pseudo_dts={"k_a=8": (8.5, 1.0)}), pcap_text(pre), pcap_text(pre), bl)
check("parse: CLEAN digests pseudo-events per dt arm", "k_a=8" in clean_row["pseudo"] and clean_row["witness"]["clean_delayed"] == 0, clean_row)
check("parse: a CLEAN rep with a delayed packet is VOID-WITNESS", op.row_void_reason({"armkind": "CLEAN", "witness": {"clean_delayed": 2}}) is not None)
check("parse: missing capture -> NO_DATA", op.digest(meta, "", "", bl)["status"] == "NO_DATA")
ok, why = op.r0_check("REL", 10.5, 1000, 10, pcap_text(pre), pcap_text(post))
check("R0: REL with the expected overtakers passes", ok, why)
ok, why = op.r0_check("REL", 10.5, 1000, 3, pcap_text(pre), pcap_text(post))
check("R0: REL with the wrong k is refuted (ABORT-BENCH)", not ok, why)

sys.exit(fails)
