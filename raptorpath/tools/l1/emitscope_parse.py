#!/usr/bin/env python3
"""Parser and scorer for THE EMISSION-BATCHING SCOPE BATTERY (docs/status.md
§8, "Emission-batching scope (Law 0) — pre-registration").

    emitscope_parse.py row   <cell> <arm> <seed> <rep> <rc> <wall_s> <drv_out> <cli_log> <srv_log> [cotenant] [bin_sha]
    emitscope_parse.py check <ledger>
    emitscope_parse.py smoke <ledger>
    emitscope_parse.py cost  <ledger>
    emitscope_parse.py score <ledger>...

`row` reuses `stage3_parse.make_row` (statuses, gauges, the two-sided
`[PIPE]`/`[GATES]`/RLC/cadence/pool-anchor witnesses) and adds:

  gate   EB arms (`EB0`, `EBA`): `[GATES] RWM_EMIT_BATCH=1` on BOTH endpoints
         and the "emission batching ACTIVE" echo on the client; `NEW`:
         `RWM_EMIT_BATCH=0` on both and the echo on neither (rule 15c).
  eb     the client's LAST `[DIAG]` burst gauges: `eb_bursts`, `eb_syms`,
         mean depth = syms / bursts, `eb_end=cap:/store:/tokens:/drained:`
         and `eb_maxrun=<pid>:<max>/<mean>,...`. EB rows must read mean
         depth > 1 (a row with depth <= 1 -- duals included -- is
         WITNESS-FAIL `eb-depth<=1`); NEW rows must read `eb_bursts=0`.
  np     the client's LAST `[DIAG]` `np=` must equal the cell's leg count
         (a dual that ran single-path is WITNESS-FAIL).
  bin    the row's sha must be the battery binary's (header) -> else
         CONTAMINATED.

Scoring constants are §8's (the §5 committed relative MDE table, the V4 §6
NEW identity bands); one definition each below.
"""
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import l1common as lc  # noqa: E402
import stage3_parse as sp  # noqa: E402

# ── THE DESIGN (docs/status.md §8) ───────────────────────────────────────
ES_ARMS = {
    "NEW": ("window", "Rlc", "bulk", "ACTIVE"),
    "EB0": ("window", "Rlc", "bulk", "ACTIVE"),
    "EBA": ("window", "Rlc", "bulk", "ACTIVE"),
}
sp.ARM_SPEC.update(ES_ARMS)
EB_ARMS = ("EB0", "EBA")
CELLS = ["c1s-400", "c1d-400", "c2-100", "c8-100"]
PLAN = {c: ["NEW", "EB0"] for c in CELLS}
SEEDS = ["42", "7"]
EMB_ECHO = "emission batching ACTIVE"

# §5's committed relative MDE (as restated in §6 and §8): cell -> {gp, ct, cpu}.
REL = {
    "c1s-400": {"gp": 0.049, "ct": 0.051, "cpu": 0.024},
    "c1d-400": {"gp": 0.056, "ct": 0.053, "cpu": 0.065},
    "c2-100": {"gp": 0.014, "ct": 0.014, "cpu": 0.063},
    "c8-100": {"gp": 0.040, "ct": 0.041, "cpu": 0.116},
}
# V4 (§6) NEW goodput: median, min, max (Mbit/s) -- the control identity band
# is [min - rel_gp*med, max + rel_gp*med].
V4_NEW = {
    "c1s-400": (502.6, 466.7, 536.5),
    "c1d-400": (305.1, 237.5, 390.4),
    "c2-100": (88.86, 87.3, 89.3),
    "c8-100": (104.3, 95.4, 105.2),
}
DNF_THRESHOLD = 0.20
MIN_LIVE = 3
WITNESS_FAIL_LIMIT = 2
FEED_BAND = 1.3
STOP_CELL = "c8-100"
ABORT_FIRST_FIVE = sp.ABORT_FIRST_FIVE

_EB_RUN = re.compile(r"(\d+):(\d+)/([0-9.]+)")


def eb_gauges(cli):
    d = [ln for ln in cli if "[DIAG] t=" in ln]
    g = {"eb_bursts": None, "eb_syms": None, "eb_depth": None, "eb_end": None,
         "eb_maxrun": None, "np_last": None}
    if not d:
        return g
    last = d[-1]
    g["eb_bursts"] = lc.inum(lc.field(last, "eb_bursts"))
    g["eb_syms"] = lc.inum(lc.field(last, "eb_syms"))
    g["np_last"] = lc.inum(lc.field(last, "np"))
    if g["eb_bursts"] is not None and g["eb_syms"] is not None:
        g["eb_depth"] = g["eb_syms"] / g["eb_bursts"] if g["eb_bursts"] > 0 else 0.0
    e = lc.field(last, "eb_end")
    if e:
        ends = {}
        for kv in e.split("/"):
            k, _, v = kv.partition(":")
            ends[k] = lc.inum(v)
        g["eb_end"] = ends
    r = lc.field(last, "eb_maxrun")
    if r:
        g["eb_maxrun"] = {m.group(1): [int(m.group(2)), float(m.group(3))]
                          for m in _EB_RUN.finditer(r)}
    return g


# ── ROW ──────────────────────────────────────────────────────────────────
def make_row(cell, arm, seed, rep, rc, wall_s, drv, cli, srv, cotenant=0, bin_sha=None):
    row = sp.make_row(cell, arm, seed, rep, rc, wall_s, drv, cli, srv, cotenant)
    row["bin_sha"] = bin_sha
    row["emb_gate_cli"] = lc.gate(cli, "RWM_EMIT_BATCH")
    row["emb_gate_srv"] = lc.gate(srv, "RWM_EMIT_BATCH")
    row["emb_echo_cli"] = any(EMB_ECHO in ln for ln in cli)
    row["emb_echo_srv"] = any(EMB_ECHO in ln for ln in srv)
    eg = [row.get("truth_egress_p%d" % i) for i in range(sp.N_LEGS[cell])]
    row["egress_dgrams"] = sum(eg) if eg and all(e is not None for e in eg) else None
    row["cpu_us_per_dgram"] = (1e6 * row["cpu_cli"] / row["egress_dgrams"]
                               if row["cpu_cli"] is not None and row["egress_dgrams"] else None)
    row.update(eb_gauges(cli))
    p = []
    if arm in EB_ARMS:
        if row["emb_gate_cli"] != 1 or row["emb_gate_srv"] != 1:
            p.append(f"emit-gate={row['emb_gate_cli']}/{row['emb_gate_srv']}-on-EB")
        if not row["emb_echo_cli"]:
            p.append("emit-echo-missing-on-EB")
        if row["eb_depth"] is None:
            p.append("eb-gauge-missing")
        elif row["eb_depth"] <= 1.0:
            p.append(f"eb-depth<=1({row['eb_depth']:.2f})")
    else:
        if row["emb_gate_cli"] != 0 or row["emb_gate_srv"] != 0:
            p.append(f"emit-gate={row['emb_gate_cli']}/{row['emb_gate_srv']}-on-NEW")
        if row["emb_echo_cli"] or row["emb_echo_srv"]:
            p.append("emit-echo-on-NEW")
        if row["eb_bursts"] is None:
            p.append("eb-gauge-missing")
        elif row["eb_bursts"] != 0:
            p.append(f"eb-bursts={row['eb_bursts']}-on-NEW")
    if row["summary"] and row["np_last"] != sp.N_LEGS[cell]:
        p.append(f"np={row['np_last']}!={sp.N_LEGS[cell]}")
    row["problems"].extend(p)
    if p and row["status"] == "LIVE":
        row["status"] = "WITNESS-FAIL"
    return row


# ── LEDGER ───────────────────────────────────────────────────────────────
_HDR_SHA = re.compile(r"^=== binary \S+ sha256 ([0-9a-f]{64})")


def rows_of(paths):
    rows, tokens, sha = [], [], None
    for path in paths:
        for ln in lc.read(path):
            m = _HDR_SHA.match(ln)
            if m:
                sha = m.group(1)
            if ln.startswith("ESROW "):
                try:
                    rows.append(json.loads(ln[len("ESROW "):]))
                except ValueError:
                    tokens.append("MALFORMED-ESROW")
            for t in ABORT_FIRST_FIVE:
                if ln.startswith(t):
                    tokens.append(t)
            if ln.startswith("TRUNCATED-AT-REP-BOUNDARY"):
                tokens.append("TRUNCATED-AT-REP-BOUNDARY")
    for r in rows:
        if sha and r.get("bin_sha") and r["bin_sha"] != sha and r["status"] == "LIVE":
            r["status"] = "CONTAMINATED"
            r["problems"].append("bin-sha-mismatch")
    return rows, tokens, sha


fmt = sp.fmt


def values(rows, metric):
    key = {"gp": "mbps", "ct": "seconds", "cpu": "cpu_cli", "cpd": "cpu_us_per_dgram"}[metric]
    return [r[key] for r in rows if r["dnf"] is False and r.get(key) is not None]


def dnf_rate(rows):
    return (sum(1 for r in rows if r["dnf"]) / len(rows)) if rows else None


def sel(live, cell, arm, seed=None):
    return [r for r in live if r["cell"] == cell and r["arm"] == arm
            and (seed is None or r["seed"] == seed)]


def compare(x_rows, ref_rows, cell, metric, higher_is_better):
    rel = REL[cell][metric]
    xv, rv = values(x_rows, metric), values(ref_rows, metric)
    if not rv:
        return "VACUOUS", "reference completed none"
    if not xv:
        return "WORSE", "X completed none"
    xm, rm = lc.med(xv), lc.med(rv)
    lo, hi = rm * (1 - rel), rm * (1 + rel)
    det = f"X={xm:.3f} ref={rm:.3f} ({100 * (xm / rm - 1):+.1f}%) band=[{lo:.3f},{hi:.3f}] rel={rel:.3f}"
    if higher_is_better:
        return ("WORSE" if xm < lo else "BETTER" if xm > hi else "WITHIN"), det
    return ("WORSE" if xm > hi else "BETTER" if xm < lo else "WITHIN"), det


def blockers(rows, live, cell, arms):
    why = []
    for a in arms:
        nl = len(sel(live, cell, a))
        if nl < MIN_LIVE:
            why.append(f"{a} live={nl}<{MIN_LIVE}")
        wf = sum(1 for r in rows if r["arm"] == a and r["cell"] == cell
                 and r["status"] in ("CONTAMINATED", "WITNESS-FAIL"))
        if wf >= WITNESS_FAIL_LIMIT:
            why.append(f"{a} witness-failed={wf}")
    return why


def eb_line(rs):
    d = [r["eb_depth"] for r in rs if r.get("eb_depth") is not None]
    ends = {"cap": 0, "store": 0, "tokens": 0, "drained": 0}
    nb = 0
    for r in rs:
        for k, v in (r.get("eb_end") or {}).items():
            ends[k] = ends.get(k, 0) + (v or 0)
        nb += r.get("eb_bursts") or 0
    frac = " ".join(f"{k}={ends[k] / nb:.3f}" if nb else f"{k}=-" for k in ("cap", "store", "tokens", "drained"))
    runs = {}
    for r in rs:
        for pid, (mx, mean) in (r.get("eb_maxrun") or {}).items():
            runs.setdefault(pid, ([], []))
            runs[pid][0].append(mx)
            runs[pid][1].append(mean)
    rtxt = " ".join(f"p{p}:max_med={fmt(lc.med(v[0]), 0)}/max_max={max(v[0])}/mean_med={fmt(lc.med(v[1]), 2)}"
                    for p, v in sorted(runs.items())) or "-"
    return (f"depth med={fmt(lc.med(d), 2)} [{fmt(min(d, default=None), 2)}-{fmt(max(d, default=None), 2)}] "
            f"n={len(d)} | bind-fraction {frac} | maxrun {rtxt}")


def score_cell(rows, live, cell, x_arm, aborted, out):
    """One cell, X = x_arm vs REF = NEW. Returns (worse, better, feed_moved, why)."""
    ctl, xs = sel(live, cell, "NEW"), sel(live, cell, x_arm)
    hard = blockers(rows, live, cell, ["NEW", x_arm]) + (["abort"] if aborted else [])
    res = {m: compare(xs, ctl, cell, m, m == "gp") for m in ("gp", "ct", "cpu")}
    dc, dx = dnf_rate(ctl), dnf_rate(xs)
    dnf_bad = dc is not None and dx is not None and dx - dc > DNF_THRESHOLD
    worse = [m for m in ("gp", "ct", "cpu") if res[m][0] == "WORSE"] + (["dnf"] if dnf_bad else [])
    better = [m for m in ("gp", "cpu") if res[m][0] == "BETTER"]
    why = list(hard)
    feed = []
    for i in range(sp.N_LEGS[cell]):
        k = "feed_ratio_p%d" % i

        def m_(rs, key):
            return lc.med([r[key] for r in rs if r["dnf"] is False and r.get(key) is not None])
        rc_, rx_ = m_(ctl, k), m_(xs, k)
        tc, tx = m_(ctl, "truth_loss_p%d" % i), m_(xs, "truth_loss_p%d" % i)
        pc, px = m_(ctl, "plc_p%d" % i), m_(xs, "plc_p%d" % i)
        gc, gx = m_(ctl, "truth_gso_p%d" % i), m_(xs, "truth_gso_p%d" % i)
        if rc_ is None or rx_ is None:
            why.append(f"feed p{i} unread")
            st = "UNREAD"
        elif rc_ / FEED_BAND <= rx_ <= rc_ * FEED_BAND:
            st = "UNCHANGED"
        else:
            st = "MOVED"
            feed.append(f"p{i}")
        out(f"  FEED {cell} p{i} truth NEW={fmt(tc, 5)} {x_arm}={fmt(tx, 5)} | plc NEW={fmt(pc, 4)} {x_arm}={fmt(px, 4)}"
            f" | plc/truth NEW={fmt(rc_, 3)} {x_arm}={fmt(rx_, 3)} band=[{fmt(rc_ / FEED_BAND if rc_ else None, 3)},"
            f"{fmt(rc_ * FEED_BAND if rc_ else None, 3)}] {st} | gso NEW={fmt(gc, 2)} {x_arm}={fmt(gx, 2)}")
    rb = {a: [r["truth_rcvbuf_drops"] for r in rs if r.get("truth_rcvbuf_drops") is not None]
          for a, rs in (("NEW", ctl), (x_arm, xs))}
    out(f"  RCVBUF {cell} " + " ".join(
        f"{a}: med={fmt(lc.med(v), 0)} max={max(v) if v else '-'} rows>0={sum(1 for x in v if x > 0)}/{len(v)}"
        for a, v in rb.items()))
    cpd_c, cpd_x = lc.med(values(ctl, "cpd")), lc.med(values(xs, "cpd"))
    out(f"  EB {cell} {x_arm} " + eb_line(xs))
    out(f"  EB {cell} NEW " + eb_line(ctl))
    out(f"  C {cell} NEW n={len(ctl)} {x_arm} n={len(xs)} | "
        + " | ".join(f"{m}:{res[m][0]} ({res[m][1]})" for m in ("gp", "ct", "cpu"))
        + f" | dnf NEW={fmt(dc, 2)} {x_arm}={fmt(dx, 2)} excess>{DNF_THRESHOLD:.2f}:{int(dnf_bad)}"
        + f" | cpu_us/dgram NEW={fmt(cpd_c, 2)} {x_arm}={fmt(cpd_x, 2)}"
        + f" | worse={'+'.join(worse) or '-'} better={'+'.join(better) or '-'} feed-moved={'+'.join(feed) or '-'}"
        + (f" | UNSCOREABLE({'; '.join(why)})" if why else ""))
    return worse, better, feed, why, hard


def control_identity(live, cell, out):
    g = values(sel(live, cell, "NEW"), "gp")
    if not g:
        out(f"  CONTROL {cell} NEW n=0 -> UNREAD")
        return "UNREAD"
    med4, lo4, hi4 = V4_NEW[cell]
    mde = REL[cell]["gp"] * med4
    lo, hi = lo4 - mde, hi4 + mde
    m = lc.med(g)
    st = "IN-BAND" if lo <= m <= hi else "CONTROL-MOVED"
    out(f"  CONTROL {cell} NEW med={m:.2f} [{min(g):.2f}-{max(g):.2f}] n={len(g)} vs V4 NEW med={med4} "
        f"[{lo4}-{hi4}] band=[{lo:.2f},{hi:.2f}] -> {st}")
    return st


def score(paths, out=print, x_arm="EB0", cells=None):
    cells = cells or CELLS
    rows, tokens, sha = rows_of(paths)
    live = [r for r in rows if r["status"] == "LIVE"]
    aborted = [t for t in ABORT_FIRST_FIVE if t in tokens]
    out(f"BINARY sha256={sha or '-'}")
    out("STATUS-TABLE (every invocation row)")
    for st in ("LIVE", "NO_DATA", "VOID-RC", "VOID-COTENANT", "CONTAMINATED", "WITNESS-FAIL"):
        out(f"  {st:<13} {sum(1 for r in rows if r['status'] == st)}")
    for r in rows:
        if r["status"] != "LIVE":
            out(f"  NONLIVE {r['cell']} {r['arm']} s{r['seed']} rep{r['rep']} {r['status']} "
                f"rc={r['rc']} {' '.join(r['problems'])}")
    out(f"  ABORT-TOKENS {' '.join(aborted) or 'none'}; truncated="
        f"{int('TRUNCATED-AT-REP-BOUNDARY' in tokens)}")
    out("")
    out("ARM-N (live rows per cell/arm, and per seed)")
    for cell in cells:
        out("  " + cell + " " + " ".join(
            f"{a}={len(sel(live, cell, a))}({'/'.join(str(len(sel(live, cell, a, s))) for s in SEEDS)})"
            for a in ("NEW", x_arm)))
    out("")
    out(f"CLAUSES {x_arm} vs NEW, §5 relative MDE applied to NEW's median")
    c_worse, c_feed, c_unsc, c_better, ctl_st = {}, {}, {}, {}, {}
    for cell in cells:
        worse, better, feed, why, hard = score_cell(rows, live, cell, x_arm, aborted, out)
        ctl_st[cell] = control_identity(live, cell, out)
        if worse and not hard:
            c_worse[cell] = worse
        if feed and not hard:
            c_feed[cell] = feed
        if why:
            c_unsc[cell] = why
        if better and not worse and not feed and not hard:
            c_better[cell] = better
    out("")
    # ── outcome, in the pre-registered precedence ──
    stop = None
    if STOP_CELL in cells and not aborted and STOP_CELL not in c_unsc:
        sw = [m for m in c_worse.get(STOP_CELL, []) if m in ("gp", "ct")]
        sf = c_feed.get(STOP_CELL, [])
        if sw or sf:
            stop = "+".join(sw + [f"feed:{x}" for x in sf])
    if aborted:
        v = "UNSCOREABLE (" + ", ".join(aborted) + ")"
    elif stop:
        v = f"SCOPE-REFUTED-AT-C8 ({stop})"
    elif c_worse or c_feed:
        bad = sorted(set(c_worse) | set(c_feed), key=cells.index)
        v = "WORSE-AT-" + ",".join(bad) + " (" + "; ".join(
            f"{c}: {'+'.join(c_worse.get(c, []) + ['feed:' + x for x in c_feed.get(c, [])])}" for c in bad) + ")"
    elif c_unsc:
        v = "UNSCOREABLE (" + "; ".join(f"{c}: {', '.join(w)}" for c, w in c_unsc.items()) + ")"
    elif c_better:
        v = "FLIP-RECOMMENDED (better at " + ", ".join(f"{c}:{'+'.join(b)}" for c, b in c_better.items()) + ")"
    else:
        v = "INERT-AS-DERIVED"
    moved = [c for c, s in ctl_st.items() if s == "CONTROL-MOVED"]
    out(f"VERDICT {v}")
    out(f"CONTROL-IDENTITY " + (" ".join(f"{c}:{s}" for c, s in ctl_st.items()))
        + (f" (CONTROL-MOVED named beside: {','.join(moved)})" if moved else ""))
    out("")
    out("PER-REP (live; per seed, rep order) cell arm seed: mbps / cpu_cli / cpu_us_per_dgram / eb_depth / rcvbuf")
    for cell in cells:
        for arm in ("NEW", x_arm):
            for s in SEEDS:
                rs = sorted(sel(live, cell, arm, s), key=lambda r: r["rep"])
                if rs:
                    out(f"  REPS {cell} {arm} s{s}: "
                        + " ".join("DNF" if r["dnf"] else fmt(r["mbps"]) for r in rs) + " / "
                        + " ".join(fmt(r["cpu_cli"], 2) for r in rs) + " / "
                        + " ".join(fmt(r.get("cpu_us_per_dgram"), 1) for r in rs) + " / "
                        + " ".join(fmt(r.get("eb_depth"), 2) for r in rs) + " / "
                        + " ".join(str(r.get("truth_rcvbuf_drops", "-")) for r in rs)
                        + f" | seed-median gp={fmt(lc.med(values(rs, 'gp')))}")
    return v


def check(path):
    rows, tokens, _ = rows_of([path])
    if not rows or "MALFORMED-ESROW" in tokens:
        print(f"CHECK-FAIL {path} rows={len(rows)} tokens={tokens}")
        return 1
    print(f"CHECK-OK {path} rows={len(rows)} live={sum(1 for r in rows if r['status'] == 'LIVE')}")
    return 0


def smoke(path):
    """rc 0 iff every smoke row is LIVE with every gauge the scorer reads,
    both arms appear, and at least one DUAL EB0 row shows depth > 1."""
    rows, _, _ = rows_of([path])
    bad = []
    dual_eb = False
    for r in rows:
        miss = []
        if r["status"] != "LIVE":
            miss.append(r["status"])
        if r["cpu_cli"] is None:
            miss.append("no-CPU-line")
        for i in range(sp.N_LEGS[r["cell"]]):
            if r.get("truth_loss_p%d" % i) is None:
                miss.append(f"no-TRUTH-p{i}")
            if r.get("plc_p%d" % i) is None:
                miss.append(f"no-plc_p{i}")
        if r.get("truth_rcvbuf_drops") is None:
            miss.append("no-rcvbuf")
        if r.get("cpu_us_per_dgram") is None:
            miss.append("no-cpu-per-dgram")
        if r.get("eb_depth") is None:
            miss.append("no-eb")
        if r["arm"] in EB_ARMS and sp.N_LEGS[r["cell"]] == 2 and (r.get("eb_depth") or 0) > 1.0 \
                and r["status"] == "LIVE":
            dual_eb = True
        print(f"SMOKE-ROW {r['cell']} {r['arm']} rc={r['rc']} status={r['status']} wall={r['wall_s']} "
              f"emb_gate={r['emb_gate_cli']}/{r['emb_gate_srv']} emb_echo={int(r['emb_echo_cli'])}/{int(r['emb_echo_srv'])} "
              f"np={r.get('np_last')} eb_bursts={r.get('eb_bursts')} eb_syms={r.get('eb_syms')} "
              f"eb_depth={fmt(r.get('eb_depth'), 2)} eb_end={r.get('eb_end')} eb_maxrun={r.get('eb_maxrun')} "
              f"mbps={r['mbps']} cpu={r['cpu_cli']} rcvbuf={r.get('truth_rcvbuf_drops')} "
              f"{' '.join(r['problems'] + miss)}")
        if miss:
            bad.append(r)
    arms = {r["arm"] for r in rows}
    ok = bool(rows) and not bad and {"NEW", "EB0"} <= arms and dual_eb
    if not ok:
        print("ABORT-SMOKE " + (f"{len(bad)} rows missing witnesses/gauges; arms={sorted(arms)}; "
                                f"dual-EB-depth>1={int(dual_eb)}" if rows else "no rows"))
        return 1
    print(f"SMOKE-PASS rows={len(rows)}")
    return 0


def main(argv):
    if not argv:
        print(__doc__)
        return 2
    cmd, a = argv[0], argv[1:]
    if cmd == "row":
        cell, arm, seed, rep, rc, wall, drv, cli, srv = a[:9]
        cot = a[9] if len(a) > 9 else 0
        bsha = a[10] if len(a) > 10 else None
        row = make_row(cell, arm, seed, rep, rc, wall, lc.read(drv), lc.read(cli), lc.read(srv), cot, bsha)
        print("ESROW " + json.dumps(row, sort_keys=True))
        return 0
    if cmd == "check":
        return check(a[0])
    if cmd == "smoke":
        return smoke(a[0])
    if cmd == "cost":
        return sp.cost(a[0])
    if cmd == "score":
        score(a)
        return 0
    if cmd == "scoreA":   # the Law-A fallback arm (only if the stop rule fired)
        score(a, x_arm="EBA", cells=["c8-100", "c1d-400"])
        return 0
    print(f"unknown command {cmd}")
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
