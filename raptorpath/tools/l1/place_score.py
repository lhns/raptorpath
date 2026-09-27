#!/usr/bin/env python3
"""Score the placement battery (Track A; the placement law is paper §5.7) off
its logs.

    place_score.py place-s42.log place-s7.log
    place_score.py ... --emit-eta OUT.log     # synthesise `[ETA]` lines for eta_s4.py

Reads only the battery log. Every `PLACERESULT` row is the JSON
`place_parse.py` emitted at battery time off `/tmp/rwm-{c,s}.log`; the raw
endpoint captures live in the VM's `diag/` directory, so nothing here
re-parses a gauge line -- it aggregates the rows and quotes their log line
numbers. `RUNTIME` and `LIVENESS` lines are joined to their row by
(cell, arm, rep).

The order of the output is the battery's reading order: the facts of the run;
the CTL `[LAT]` decomposition first; the S4 offline score; and only then the
arms against the CTL arm's own rep spread. The script classifies with the
fixed thresholds and prints them; it does not choose.

"Beyond the CTL spread" means the arm's pooled min-max range lies entirely
below (or above) the CTL arm's pooled min-max range. A range that overlaps
CTL's is WITHIN.
"""
import argparse
import json
import os
import re
import sys
from collections import defaultdict

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from l1common import med  # noqa: E402

SHIPPED_SIGMA_OVER_REF = 0.19238
SQRT6_OVER_PI = 6.0 ** 0.5 / 3.141592653589793
DUALS = ("c7", "c8L")
CELLS = ("c7", "c8L", "c1", "c9h")
ARMS = ("CTL", "T0", "TSIG", "HOL", "HOLTSIG")
CLIENT_TIMEOUT_S = 300.0  # the engine-side DNF cutoff read off RUNTIME 302-303 s
CELL_BYTES = {"c7": 200e6, "c8L": 100e6, "c1": 400e6, "c9h": 100e6, "sc2": 100e6, "sc3": 25e6}


def load(path):
    rows, runtime, live = [], {}, {}
    with open(path, errors="replace") as f:
        for no, ln in enumerate(f, 1):
            if ln.startswith("PLACERESULT "):
                r = json.loads(ln[len("PLACERESULT "):])
                r["_line"] = no
                r["_file"] = path
                rows.append(r)
            elif ln.startswith("RUNTIME "):
                m = re.match(r"RUNTIME (\S+) rep=(\d+) (\d+)s", ln)
                if m:
                    runtime[(m.group(1), int(m.group(2)))] = (int(m.group(3)), no)
            elif ln.startswith("LIVENESS "):
                m = re.match(r"LIVENESS (\S+) rep=(\d+) .*recv_final_lines=(\d+)", ln)
                if m:
                    live[(m.group(1), int(m.group(2)))] = (int(m.group(3)), no)
    for r in rows:
        key = (f"{r['cell']}-{r['arm']}" if r["arm"] != "SINGLE" else f"{r['cell']}-SINGLE", r["rep"])
        r["_runtime"], r["_runtime_line"] = runtime.get(key, (None, None))
        r["_recv_final_lines"], r["_live_line"] = live.get(key, (None, None))
    return rows


def classify(sh_xp, sh_ax, sh_rep):
    if sh_xp >= 0.5:
        return "PLACEMENT-INDICTED"
    if sh_ax >= 0.5 and sh_xp <= 0.2:
        return "QUEUE-DOMINATED"
    if sh_rep >= 0.5:
        return "REPAIR-DOMINATED"
    return "MIXED"


def fmt(v, d=4):
    if v is None:
        return "-"
    if isinstance(v, bool):
        return "T" if v else "F"
    if isinstance(v, float):
        return f"{v:.{d}f}" if abs(v) < 1e6 else f"{v:.0f}"
    return str(v)


def rng(vals, d=4):
    vals = [v for v in vals if v is not None]
    if not vals:
        return "-"
    if len(vals) == 1:
        return fmt(vals[0], d)
    return f"{fmt(min(vals), d)}..{fmt(max(vals), d)} (med {fmt(med(vals), d)})"


def worst_path(r, key):
    vals = [p.get(key) for p in r.get("lat_paths", []) if p.get(key) is not None]
    return max(vals) if vals else None


def pooled_share(rows, key):
    """lat_n-weighted mean of a per-row pooled share (the row's own share is
    already weighted over its paths' *_sum fields)."""
    num = den = 0.0
    for r in rows:
        if r.get(key) is not None and r.get("lat_n"):
            num += r[key] * r["lat_n"]
            den += r["lat_n"]
    return num / den if den else None


def beyond(arm_vals, ctl_vals):
    a = [v for v in arm_vals if v is not None]
    c = [v for v in ctl_vals if v is not None]
    if not a or not c:
        return "n/a"
    if max(a) < min(c):
        return "BELOW-SPREAD"
    if min(a) > max(c):
        return "ABOVE-SPREAD"
    return "WITHIN"


def section(title):
    print()
    print("=" * 78)
    print(title)
    print("=" * 78)


def main(argv=None):
    ap = argparse.ArgumentParser()
    ap.add_argument("ledgers", nargs="+")
    ap.add_argument("--emit-eta", help="write synthesised `[ETA] site=sender` lines here for eta_s4.py")
    args = ap.parse_args(argv)

    rows = []
    for p in args.ledgers:
        rows.extend(load(p))
    seeds = sorted({r["seed"] for r in rows})
    by = defaultdict(list)
    for r in rows:
        by[(r["cell"], r["arm"])].append(r)

    def sel(cell, arm, seed=None):
        return [r for r in by[(cell, arm)] if seed is None or r["seed"] == seed]

    # ── 0. FACTS ─────────────────────────────────────────────────────────
    section("0 -- THE FACTS OF THE RUN")
    print(f"rows: {len(rows)} over {len(args.ledgers)} ledgers; seeds {seeds}")
    print("| cell-arm | " + " | ".join(f"n s{s} / dnf / abort" for s in seeds) + " | recv_final_lines | lat_final | eta_final(sender) | runtime s |")
    print("|---|" + "---|" * (len(seeds) + 4))
    for cell in CELLS + ("sc2", "sc3"):
        for arm in (ARMS if cell in CELLS else ("SINGLE",)):
            rr = sel(cell, arm)
            if not rr:
                continue
            cells = []
            for s in seeds:
                ss = sel(cell, arm, s)
                cells.append(f"{len(ss)} / {sum(1 for r in ss if r.get('dnf'))} / {sum(1 for r in ss if r.get('abort'))}")
            rfl = defaultdict(int)
            for r in rr:
                rfl[r.get("_recv_final_lines")] += 1
            rt = [r["_runtime"] for r in rr if r.get("_runtime") is not None]
            print(f"| {cell}-{arm} | " + " | ".join(cells)
                  + f" | {dict(rfl)} | {sum(1 for r in rr if r.get('lat_final'))}/{len(rr)}"
                  + f" | {sum(1 for r in rr if r.get('eta_final'))}/{len(rr)}"
                  + f" | {rng([float(x) for x in rt], 0)} |")
    cv = [r for r in rows if r.get("control_violated")]
    print(f"\ncontrol_violated rows: {len(cv)}")
    notfinal = [r for r in rows if not r.get("lat_final")]
    for r in notfinal:
        print(f"lat_final=False: {r['cell']}-{r['arm']} s{r['seed']} r{r['rep']} line {r['_line']} recv_final_lines={r.get('_recv_final_lines')} lat_present={r.get('lat_present')} lat_reading={r.get('lat_reading')} succ_final={r.get('succ_final')} eta_recv_final={r.get('eta_recv_final')}")
    dnf_lat = [r for r in rows if r.get("dnf")]
    print(f"DNF rows with a [LAT] reading: {sum(1 for r in dnf_lat if r.get('lat_present'))}/{len(dnf_lat)}; with lat_final: {sum(1 for r in dnf_lat if r.get('lat_final'))}")

    # ── 1. FIRST READOUT ────────────────────────────────────────────────
    section("1 -- THE FIRST READOUT: CTL [LAT] DECOMPOSITION (read before any arm)")
    for cell in ("c7", "c8L", "c9h", "c1"):
        rr = sel(cell, "CTL")
        print(f"\n### {cell} CTL (n={len(rr)})")
        print("| seed | rep | line | dnf | lat_n | sh_ax | sh_xp | sh_sp | sh_rep | reading | p95(rw_xp) worst | p95(A_x) worst | tot_p50 worst | tot_p95 worst | tot_p99 worst | rw_xp n | xp_n/det |")
        print("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|")
        for r in sorted(rr, key=lambda r: (r["seed"], r["rep"])):
            print(f"| {r['seed']} | {r['rep']} | {r['_line']} | {fmt(r.get('dnf'))} | {fmt(r.get('lat_n'),0)} | {fmt(r.get('sh_ax'))} | {fmt(r.get('sh_xp'))} | {fmt(r.get('sh_sp'))} | {fmt(r.get('sh_rep'))} | {r.get('lat_reading','-')} | {fmt(r.get('rwxp_p95_worst'),0)} | {fmt(r.get('ax_p95_worst'),0)} | {fmt(worst_path(r,'tot_p50'),0)} | {fmt(worst_path(r,'tot_p95'),0)} | {fmt(r.get('tot_p99_worst'),0)} | {fmt(r.get('lat_rwxp_n'),0)} | {fmt(r.get('xp_over_det'))} |")
        for s in seeds + [None]:
            ss = [r for r in rr if (s is None or r["seed"] == s) and r.get("sh_xp") is not None]
            if not ss:
                continue
            tag = f"seed {s}" if s else "POOLED"
            pa, px, psp, prep = (pooled_share(ss, k) for k in ("sh_ax", "sh_xp", "sh_sp", "sh_rep"))
            print(f"{tag}: n={len(ss)} sh_ax {rng([r['sh_ax'] for r in ss])} | sh_xp {rng([r['sh_xp'] for r in ss])} | sh_sp {rng([r['sh_sp'] for r in ss])} | sh_rep {rng([r['sh_rep'] for r in ss])}")
            print(f"   lat_n-weighted pooled shares: ax={pa:.4f} xp={px:.4f} sp={psp:.4f} rep={prep:.4f} => {classify(px, pa, prep)}; per-rep readings: {sorted(set(r['lat_reading'] for r in ss))}")
            print(f"   p95(rw_xp) worst {rng([r.get('rwxp_p95_worst') for r in ss],0)} | p95(A_x) worst {rng([r.get('ax_p95_worst') for r in ss],0)} | tot_p99 worst {rng([r.get('tot_p99_worst') for r in ss],0)} | xp_n/det {rng([r.get('xp_over_det') for r in ss])}")

    # ── 2. S4 ────────────────────────────────────────────────────────────
    section("2 -- THE S4 OFFLINE SCORE: sigma_hat_e / ref off [ETA] (before any arm verdict)")
    print(f"shipped claim: sigma/ref = {SHIPPED_SIGMA_OVER_REF} at every cell; band +/-20% = [{0.8*SHIPPED_SIGMA_OVER_REF:.5f}, {1.2*SHIPPED_SIGMA_OVER_REF:.5f}]")
    print("routes: (a) engine t_eff inversion, TSIG/HOLTSIG rows only, ref = the law's own min SRTT; (b) sender RMS sig_us / min tau_us (tau_us = RTprop when known); (c) sender RMS sig_us / min [DIAG] rtt (SRTT surrogate); (r) receiver max sig_us / [DIAG] rtt")
    for cell in ("c7", "c8L", "c9h", "c1", "sc2", "sc3"):
        arms = ("CTL",) if cell in CELLS else ("SINGLE",)
        print(f"\n### {cell}")
        for arm in arms + (("TSIG", "HOLTSIG") if cell in CELLS else ()):
            rr = [r for r in sel(cell, arm) if r.get("eta_present")]
            if not rr:
                continue
            print(f"  [{arm}] n={len(rr)}")
            for s in seeds + [None]:
                ss = [r for r in rr if s is None or r["seed"] == s]
                tag = f"s{s}" if s else "pooled"
                ta = [r.get("s4_from_teff") for r in ss]
                tb = [r.get("sig_ref_sender_tau") for r in ss]
                tc = [r.get("sig_ref_sender") for r in ss]
                trc = [r.get("sig_ref_recv") for r in ss]
                print(f"    {tag}: (a) t_eff-route {rng(ta)} | (b) tau_us-route {rng(tb)} | (c) DIAG-route {rng(tc)} | (r) recv/DIAG {rng(trc)}")
                if s is None:
                    print(f"    pooled: t_eff {rng([r.get('t_eff') for r in ss])} | t_cold {rng([r.get('t_cold') for r in ss])} | ref_us(DIAG) {rng([r.get('ref_us') for r in ss],0)} | tau_ref_us {rng([r.get('tau_ref_us') for r in ss],0)} | sender sig RMS us {rng([r.get('sig_sender_rms_us') for r in ss],0)} | recv sig us {rng([r.get('sig_recv_us') for r in ss],0)} | bind max {rng([r.get('eta_bind_max') for r in ss])} | eta_zero {rng([r.get('eta_zero') for r in ss])}")
                    pairs = [p["pairs"] for r in ss for p in r.get("eta_paths", [])]
                    wit = [r.get("witness_sender_ge_recv") for r in ss if r.get("witness_sender_ge_recv") is not None]
                    print(f"    pooled: pairs per path {rng([float(x) for x in pairs],0)} | witness sender>=recv holds {sum(1 for w in wit if w)}/{len(wit)} | per-path sig/tau: " + "; ".join(
                        f"{p['path']} {rng([q['sig_us']/q['tau_us'] for r in ss for q in r.get('eta_paths',[]) if q['path']==p['path'] and q.get('sig_us') and q.get('tau_us')])}"
                        for p in ss[0].get("eta_paths", [])))
                    for lab, vals in (("(a)", ta), ("(b)", tb), ("(c)", tc)):
                        v = [x for x in vals if x is not None]
                        if v:
                            inband = sum(1 for x in v if abs(x - SHIPPED_SIGMA_OVER_REF) <= 0.2 * SHIPPED_SIGMA_OVER_REF)
                            print(f"    {lab} in the 0.19238+/-20% band: {inband}/{len(v)}; median/0.19238 = {med(v)/SHIPPED_SIGMA_OVER_REF:.2f}x; T_eff = {rng([SQRT6_OVER_PI*x for x in v])}")

    # ── 3. ARMS ─────────────────────────────────────────────────────────
    section("3 -- THE ARMS AGAINST THE CTL SPREAD (min..max over the CTL reps, pooled over seeds)")
    metrics = [("sh_xp", "sh_xp", 4), ("rwxp_p95_worst", "p95(rw_xp)", 0), ("ax_p95_worst", "p95(A_x)", 0),
               ("tot_p99_worst", "tot_p99", 0), ("sh_ax", "sh_ax", 4), ("sh_rep", "sh_rep", 4),
               ("xp_over_det", "xp_n/det", 4), ("mbps", "goodput Mbit/s", 2)]
    for cell in CELLS:
        ctl = sel(cell, "CTL")
        print(f"\n### {cell}")
        for arm in ARMS:
            rr = sel(cell, arm)
            print(f"\n  [{cell}-{arm}] n={len(rr)} (s42 {len(sel(cell,arm,42))}, s7 {len(sel(cell,arm,7))}); DNF {sum(1 for r in rr if r.get('dnf'))}; lines {[r['_line'] for r in sorted(rr, key=lambda r:(r['seed'],r['rep']))]}")
            print(f"    bind: t_n {rng([r.get('t_n') for r in rr],0)} | t_eff {rng([r.get('t_eff') for r in rr])} | t_cold {rng([r.get('t_cold') for r in rr])} | hol_calls {rng([r.get('hol_calls') for r in rr],0)} | hol_mv {rng([r.get('hol_mv') for r in rr])} | hol_sh {rng([r.get('hol_sh') for r in rr])} | hol_w {rng([r.get('hol_w') for r in rr],6)} | cold_r {rng([r.get('cold_r') for r in rr])} | cold_ge {rng([r.get('cold_ge') for r in rr])} | zero {rng([r.get('eta_zero') for r in rr])} | place_n {rng([r.get('place_n') for r in rr],0)}")
            print(f"    control: succ_xp_n {rng([r.get('succ_xp_n') for r in rr],0)} | lat_rwxp_n {rng([r.get('lat_rwxp_n') for r in rr],0)} | lat_reading {sorted(set(r.get('lat_reading','-') for r in rr))}")
            if arm == "CTL":
                for key, lab, d in metrics:
                    print(f"    {lab}: pooled {rng([r.get(key) for r in rr], d)} | s42 {rng([r.get(key) for r in sel(cell,arm,42)], d)} | s7 {rng([r.get(key) for r in sel(cell,arm,7)], d)}")
                continue
            for key, lab, d in metrics:
                av = [r.get(key) for r in rr]
                cv_ = [r.get(key) for r in ctl]
                per_seed = []
                for s in seeds:
                    a_s = [r.get(key) for r in sel(cell, arm, s) if r.get(key) is not None]
                    c_s = [r.get(key) for r in sel(cell, "CTL", s) if r.get(key) is not None]
                    if a_s and c_s:
                        per_seed.append(f"s{s} {beyond(a_s, c_s)} (med {fmt(med(a_s),d)} vs CTL {fmt(med(c_s),d)})")
                print(f"    {lab}: {rng(av, d)} vs CTL {rng(cv_, d)} => {beyond(av, cv_)}; " + "; ".join(per_seed))

    # ── 4. AGGREGATION GUARD ────────────────────────────────────────────
    section("4 -- THE AGGREGATION GUARD (same-session singles)")
    for s in seeds:
        for sc in ("sc2", "sc3"):
            rr = sel(sc, "SINGLE", s)
            print(f"seed {s} {sc}: mbps {[r.get('mbps') for r in rr]} dnf {[r.get('dnf') for r in rr]} runtime {[r.get('_runtime') for r in rr]} lines {[r['_line'] for r in rr]}")
    def single_bound(sc, s):
        rr = sel(sc, "SINGLE", s)
        vals = []
        for r in rr:
            if r.get("mbps") is not None:
                vals.append(r["mbps"])
            elif r.get("dnf"):
                vals.append(CELL_BYTES[sc] * 8 / CLIENT_TIMEOUT_S / 1e6)  # UPPER bound
        return max(vals) if vals else None, any(r.get("dnf") for r in rr)
    for s in seeds:
        sc2, sc2dnf = single_bound("sc2", s)
        sc3, sc3dnf = single_bound("sc3", s)
        print(f"seed {s}: sc2 max {sc2:.3f}{' (UPPER BOUND, DNF)' if sc2dnf else ''}; sc3 max {sc3:.3f}{' (UPPER BOUND, DNF)' if sc3dnf else ''}")
        print(f"  c7 guard 0.97 x 2 x sc2 = {0.97*2*sc2:.3f}; c8L guard 0.87 x (sc2+sc3) = {0.87*(sc2+sc3):.3f}")
        for cell, thr in (("c7", 0.97 * 2 * sc2), ("c8L", 0.87 * (sc2 + sc3))):
            for arm in ARMS:
                rr = sel(cell, arm, s)
                vals = [r.get("mbps") for r in rr if r.get("mbps") is not None]
                print(f"  {cell}-{arm}: goodput {rng(vals,2)}; min >= guard {thr:.2f}: {min(vals) >= thr if vals else 'n/a'}")

    # ── 5. c1 CONTROL ───────────────────────────────────────────────────
    section("5 -- THE c1 MUST-NOT-MOVE CONTROL (and the singles)")
    for cell in ("c1", "sc2", "sc3"):
        rr = [r for r in rows if r["cell"] == cell]
        print(f"{cell}: rows {len(rr)}; succ_xp_n values {sorted(set(r.get('succ_xp_n') for r in rr))}; lat_rwxp_n values {sorted(set(r.get('lat_rwxp_n') for r in rr))}; control_violated {sum(1 for r in rr if r.get('control_violated'))}; sh_xp values {sorted(set(r.get('sh_xp') for r in rr))}")

    # ── emit [ETA] for eta_s4.py ────────────────────────────────────────
    # CTL / single rows only (the S4 score is an offline reading of a stream
    # the CTL arm itself produces), sender lines only: the row carries the
    # sender's per-path sig_us / pairs / tau_us verbatim, but only the max of
    # the receiver's per-path sig_us and bind, so a receiver line cannot be
    # rebuilt faithfully. The receiver's `bind=` (worst path) is carried onto
    # each sender path so eta_s4.py's UNREADABLE limb can read it -- that is
    # a synthesis and is said so here. `t_eff=` is `-` on CTL by construction.
    if args.emit_eta:
        with open(args.emit_eta, "w") as f:
            for r in sorted(rows, key=lambda r: (r["cell"], r["seed"], r["rep"])):
                if r["arm"] not in ("CTL", "SINGLE") or not r.get("eta_paths"):
                    continue
                f.write(f"CROWNSPOT stage seed={r['seed']} cell={r['cell']}\n")
                f.write(f"--- {r['arm']}-r{r['rep']} {int(CELL_BYTES.get(r['cell'],0))}B\n")
                head = (f"[ETA] site=sender fhat_us=0 n={fmt(r.get('eta_stamped'),0)} zero={fmt(r.get('eta_zero'))} "
                        f"place_n={fmt(r.get('place_n'),0)} cold_r={fmt(r.get('cold_r'))} cold_ge={fmt(r.get('cold_ge'))} "
                        f"t_eff=- t_cold=- t_n=0 hol_sh=- hol_n=0 hol_mv=- hol_calls=0 hol_w=-")
                parts = []
                for p in r["eta_paths"]:
                    sig = "-" if p.get("sig_us") is None else f"{p['sig_us']:.0f}"
                    parts.append(f" {p['path']}:n={fmt(p.get('matched'),0)}/{fmt(p.get('stamped'),0)} drop={fmt(p.get('drop'),0)} "
                                 f"tau_us={fmt(p.get('tau_us'),0)} bind={fmt(r.get('eta_bind_max'))} e_p50=0 e_p90=0 e_p99=0 e_mx=0 late=0 "
                                 f"sig_us={sig}/n{p.get('pairs',0)}")
                f.write(head + "".join(parts) + "\n")
        print(f"\nwrote synthesised sender [ETA] lines (CTL/SINGLE rows) to {args.emit_eta}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
