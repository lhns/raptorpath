#!/usr/bin/env python3
"""Parser and scorer for THE V4 VERIFICATION (docs/status.md §6, "V4
verification — pre-registration").

    verify4_parse.py row   <cell> <arm> <seed> <rep> <rc> <wall_s> <drv_out> <cli_log> <srv_log> [cotenant] [bin_sha]
    verify4_parse.py check <ledger>
    verify4_parse.py smoke <ledger> <tun smoke log> [<tail_matrix smoke log>]
    verify4_parse.py cost  <ledger>
    verify4_parse.py score <ledger>
    verify4_parse.py tun   <tun ledger>...
    (the crown is scored by `stage3_parse.py crown`, unchanged)

`row` reuses `stage3_parse.make_row` (same statuses, same gauges, same
two-sided witnesses: `[PIPE]`, `[GATES]`, the RLC auto-select line, the
tri-state estimator-cadence echo, `[GATES] RWM_POOL_ANCHOR=0`) for the V4
arms registered below, and adds the emission-batching witness and the
binary's sha256:

  EMB   `[GATES] RWM_EMIT_BATCH=1` on BOTH endpoints and the "emission
        batching ACTIVE" echo on the client (the sending endpoint of the
        measured direction); every other arm `[GATES] RWM_EMIT_BATCH=0` on
        both and the echo on neither.
  bin   the row's `bin_sha` must be the arm's binary (NEW or OLD sha from
        the ledger header); a mismatch is CONTAMINATED.

Scoring constants are the §5 committed MDE table (relative MDE per cell and
metric) and the §5 control ranges, one definition each below, restated in
docs/status.md §6.
"""
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import l1common as lc  # noqa: E402
import stage3_parse as sp  # noqa: E402

# ── THE DESIGN (docs/status.md §6) ───────────────────────────────────────
# arm -> (pipeline, backend, hint, expected cadence echo); registered into
# stage3_parse so its make_row witnesses them.
V4_ARMS = {
    "NEW": ("window", "Rlc", "bulk", "ACTIVE"),
    "OLD": ("window", "Rlc", "bulk", "NONE"),
    "NEWa": ("window", "Rlc", "auto", "ACTIVE"),
    "OLDa": ("window", "Rlc", "auto", "NONE"),
    "EMB": ("window", "Rlc", "bulk", "ACTIVE"),
}
sp.ARM_SPEC.update(V4_ARMS)
ARM_BIN = {"NEW": "new", "OLD": "old", "NEWa": "new", "OLDa": "old", "EMB": "new"}
PLAN = {
    "c1s-400": ["NEW", "OLD", "EMB"],
    "c1d-400": ["NEW", "OLD"],
    "c2-100": ["NEW", "OLD", "NEWa", "OLDa", "EMB"],
    "c3-25": ["NEW", "OLD", "NEWa", "OLDa", "EMB"],
    "c7-100": ["NEW", "OLD"],
    "c8-100": ["NEW", "OLD"],
}
CELLS = list(PLAN)
A_CELLS = CELLS
B_CELLS = ["c2-100", "c3-25"]
C_CELLS = ["c1s-400", "c2-100", "c3-25"]
SEEDS = ["42", "7"]
EMB_ECHO = "emission batching ACTIVE"

# §5's committed relative MDE (status.md §5 (a), n = 10 + 10, window bulk):
# cell -> {gp, ct, cpu}. Applied to the reference median of every V4
# comparison at that cell; at auto it is a transfer (weaker), as in §5 (b).
S3_REL = {
    "c1s-400": {"gp": 0.049, "ct": 0.051, "cpu": 0.024},
    "c1d-400": {"gp": 0.056, "ct": 0.053, "cpu": 0.065},
    "c2-100": {"gp": 0.014, "ct": 0.014, "cpu": 0.063},
    "c3-25": {"gp": 0.016, "ct": 0.016, "cpu": 0.096},
    "c7-100": {"gp": 0.028, "ct": 0.029, "cpu": 0.036},
    "c8-100": {"gp": 0.040, "ct": 0.041, "cpu": 0.116},
}
# §5's CTL (A1 u A2, window bulk, the OLD binary) goodput: median, [min, max]
# and the absolute goodput MDE (Mbit/s) -- the identity band of the OLD arm.
S3_CTL = {
    "c1s-400": (303.2, 282.1, 311.8, 14.8),
    "c1d-400": (193.9, 187.7, 209.3, 10.8),
    "c2-100": (89.1, 86.9, 89.3, 1.2),
    "c3-25": (17.25, 16.9, 17.5, 0.28),
    "c7-100": (176.1, 169.1, 179.1, 5.0),
    "c8-100": (102.1, 97.5, 105.7, 4.1),
}
# §5's WINa (window auto, the OLD binary): median, [min, max].
S3_WINA = {
    "c2-100": (73.1, 71.9, 74.1),
    "c3-25": (15.22, 14.6, 15.5),
}
DNF_THRESHOLD = 0.20     # §5: max(0.20, 2|dnf A1 - dnf A2|) = 0.20 at every cell
MIN_LIVE = 3             # live rows per (cell, arm) for a comparison
WITNESS_FAIL_LIMIT = 2   # failed rows per (arm, cell) that void the cell
FEED_BAND = 1.3          # EMB's plc/truth within [1/1.3, 1.3] x NEW's
PLU_FLOOR = sp.PLU_FLOOR
ABORT_FIRST_FIVE = sp.ABORT_FIRST_FIVE


# ── ROW ──────────────────────────────────────────────────────────────────
def make_row(cell, arm, seed, rep, rc, wall_s, drv, cli, srv, cotenant=0, bin_sha=None):
    row = sp.make_row(cell, arm, seed, rep, rc, wall_s, drv, cli, srv, cotenant)
    row["bin_sha"] = bin_sha
    row["bin"] = ARM_BIN[arm]
    row["emb_gate_cli"] = lc.gate(cli, "RWM_EMIT_BATCH")
    row["emb_gate_srv"] = lc.gate(srv, "RWM_EMIT_BATCH")
    row["emb_echo_cli"] = any(EMB_ECHO in ln for ln in cli)
    row["emb_echo_srv"] = any(EMB_ECHO in ln for ln in srv)
    eg = [row.get("truth_egress_p%d" % i) for i in range(sp.N_LEGS[cell])]
    row["egress_dgrams"] = sum(eg) if eg and all(e is not None for e in eg) else None
    row["cpu_us_per_dgram"] = (1e6 * row["cpu_cli"] / row["egress_dgrams"]
                               if row["cpu_cli"] is not None and row["egress_dgrams"] else None)
    p = []
    if arm == "EMB":
        if row["emb_gate_cli"] != 1 or row["emb_gate_srv"] != 1:
            p.append(f"emit-gate={row['emb_gate_cli']}/{row['emb_gate_srv']}-on-EMB")
        if not row["emb_echo_cli"]:
            p.append("emit-echo-missing-on-EMB")
    else:
        if row["emb_gate_cli"] != 0 or row["emb_gate_srv"] != 0:
            p.append(f"emit-gate={row['emb_gate_cli']}/{row['emb_gate_srv']}-on-non-EMB")
        if row["emb_echo_cli"] or row["emb_echo_srv"]:
            p.append("emit-echo-on-non-EMB")
    row["problems"].extend(p)
    if row["status"] in ("LIVE", "WITNESS-FAIL", "CONTAMINATED"):
        if p and row["status"] == "LIVE":
            row["status"] = "WITNESS-FAIL"
    return row


def bin_check(rows, shas):
    """A row whose bin_sha is not its arm's binary is CONTAMINATED (applied
    at score time, from the ledger header's shas)."""
    for r in rows:
        want = shas.get(r.get("bin"))
        if want and r.get("bin_sha") and r["bin_sha"] != want and r["status"] == "LIVE":
            r["status"] = "CONTAMINATED"
            r["problems"].append("bin-sha-mismatch")
    return rows


# ── LEDGER ───────────────────────────────────────────────────────────────
_HDR_SHA = re.compile(r"^=== binary (NEW|OLD) \S+ sha256 ([0-9a-f]{64})")


def rows_of(paths):
    rows, tokens, shas = [], [], {}
    for path in paths:
        for ln in lc.read(path):
            m = _HDR_SHA.match(ln)
            if m:
                shas[m.group(1).lower()] = m.group(2)
            if ln.startswith("V4ROW "):
                try:
                    rows.append(json.loads(ln[len("V4ROW "):]))
                except ValueError:
                    tokens.append("MALFORMED-V4ROW")
            for t in ABORT_FIRST_FIVE:
                if ln.startswith(t):
                    tokens.append(t)
            if ln.startswith("TRUNCATED-AT-REP-BOUNDARY"):
                tokens.append("TRUNCATED-AT-REP-BOUNDARY")
    return bin_check(rows, shas), tokens, shas


fmt = sp.fmt
pct = sp.pct


def values(rows, metric):
    key = {"gp": "mbps", "ct": "seconds", "cpu": "cpu_cli", "cpd": "cpu_us_per_dgram",
           "busy": "busy_med"}[metric]
    return [r[key] for r in rows if r["dnf"] is False and r.get(key) is not None]


def dnf_rate(rows):
    return (sum(1 for r in rows if r["dnf"]) / len(rows)) if rows else None


def sel(live, cell, arm, seed=None):
    return [r for r in live if r["cell"] == cell and r["arm"] == arm
            and (seed is None or r["seed"] == seed)]


def compare(x_rows, ref_rows, cell, metric, higher_is_better):
    """X against the reference median on one metric with §5's relative MDE
    of (cell, metric). Returns (reading, detail)."""
    rel = S3_REL[cell][metric]
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


def witness_fails(rows, arm, cell):
    return sum(1 for r in rows if r["arm"] == arm and r["cell"] == cell
               and r["status"] in ("CONTAMINATED", "WITNESS-FAIL"))


def blockers(rows, live, cell, arms):
    why = []
    for a in arms:
        nl = len(sel(live, cell, a))
        if nl < MIN_LIVE:
            why.append(f"{a} live={nl}<{MIN_LIVE}")
        wf = witness_fails(rows, a, cell)
        if wf >= WITNESS_FAIL_LIMIT:
            why.append(f"{a} witness-failed={wf}")
    return why


def pair_verdict(rows, live, cell, x_arm, ref_arm, aborted, out, tag):
    """The (A)/(B) per-cell clause set: X vs REF. Returns BETTER / SAME /
    WORSE / UNSCOREABLE and prints the evidence."""
    xs, rs = sel(live, cell, x_arm), sel(live, cell, ref_arm)
    why = blockers(rows, live, cell, [x_arm, ref_arm]) + (["abort"] if aborted else [])
    res = {m: compare(xs, rs, cell, m, m == "gp") for m in ("gp", "ct", "cpu")}
    dr, dx = dnf_rate(rs), dnf_rate(xs)
    dnf_bad = dr is not None and dx is not None and dx - dr > DNF_THRESHOLD
    worse = [m for m in ("gp", "ct", "cpu") if res[m][0] == "WORSE"] + (["dnf"] if dnf_bad else [])
    better = [m for m in ("gp", "cpu") if res[m][0] == "BETTER"]
    if why:
        v = "UNSCOREABLE"
    elif worse:
        v = "WORSE"
    elif better:
        v = "BETTER"
    else:
        v = "SAME"
    cpd_x, cpd_r = lc.med(values(xs, "cpd")), lc.med(values(rs, "cpd"))
    out(f"  {tag} {cell} {x_arm} n={len(xs)} vs {ref_arm} n={len(rs)} | "
        + " | ".join(f"{m}:{res[m][0]} ({res[m][1]})" for m in ("gp", "ct", "cpu"))
        + f" | dnf {ref_arm}={fmt(dr, 2)} {x_arm}={fmt(dx, 2)} excess>{DNF_THRESHOLD:.2f}:{int(dnf_bad)}"
        + f" | cpu_us/dgram {ref_arm}={fmt(cpd_r, 2)} {x_arm}={fmt(cpd_x, 2)}"
        + (f" ({100 * (cpd_x / cpd_r - 1):+.1f}%)" if cpd_x and cpd_r else "")
        + f" | worse={'+'.join(worse) or '-'} better={'+'.join(better) or '-'}"
        + (f" | UNSCOREABLE({'; '.join(why)})" if why else "")
        + f" => {v}")
    return v


def drift_line(live, cell, arm, table, out, label):
    """The OLD arm against its §5 identity band: [min - MDE, max + MDE]
    (CTL) or [min - MDE, max + MDE] with the bulk MDE (WINa)."""
    g = values(sel(live, cell, arm), "gp")
    if not g:
        out(f"  DRIFT {label} {cell} {arm} n=0 -> UNREAD")
        return "UNREAD"
    m = lc.med(g)
    if table is S3_CTL:
        med5, lo5, hi5, mde = S3_CTL[cell]
    else:
        med5, lo5, hi5 = S3_WINA[cell]
        mde = S3_REL[cell]["gp"] * med5   # the bulk relative goodput MDE x §5 WINa median
    lo, hi = lo5 - mde, hi5 + mde
    st = "IN-BAND" if lo <= m <= hi else "CONTROL-MOVED"
    out(f"  DRIFT {label} {cell} {arm} med={m:.2f} [{min(g):.2f}-{max(g):.2f}] n={len(g)} vs §5 "
        f"med={med5} [{lo5}-{hi5}] band=[{lo:.2f},{hi:.2f}] -> {st}")
    return st


def score(paths, out=print):
    rows, tokens, shas = rows_of(paths)
    live = [r for r in rows if r["status"] == "LIVE"]
    aborted = [t for t in ABORT_FIRST_FIVE if t in tokens]
    out(f"BINARIES new={shas.get('new', '-')} old={shas.get('old', '-')}")
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
    for cell in CELLS:
        out("  " + cell + " " + " ".join(
            f"{a}={len(sel(live, cell, a))}({'/'.join(str(len(sel(live, cell, a, s))) for s in SEEDS)})"
            for a in PLAN[cell]))
    out("")
    verdicts = {}
    # ── (A) NEW vs OLD at bulk ──
    out("(A) NEW vs OLD (window bulk), §5 relative MDE applied to OLD's median")
    for cell in A_CELLS:
        verdicts[("A", cell)] = pair_verdict(rows, live, cell, "NEW", "OLD", aborted, out, "A")
        drift_line(live, cell, "OLD", S3_CTL, out, "A")
    out("VERDICT-A " + " ".join(f"{c}:{verdicts[('A', c)]}" for c in A_CELLS))
    exp = {c: ("BETTER" if c in ("c1s-400", "c1d-400") else "SAME") for c in A_CELLS}
    miss = [c for c in A_CELLS if verdicts[("A", c)] != exp[c]]
    out("PREDICTION-A " + ("MET" if not miss else "MISSED-AT-" + ",".join(
        f"{c}(expected {exp[c]}, got {verdicts[('A', c)]})" for c in miss)))
    out("")
    # ── (B) AUTO ──
    out("(B) AUTO: NEWa vs OLDa in-session (scored), and NEWa vs §5 WINa (historical, scored iff OLDa IN-BAND)")
    for cell in B_CELLS:
        verdicts[("B", cell)] = pair_verdict(rows, live, cell, "NEWa", "OLDa", aborted, out, "B")
        st = drift_line(live, cell, "OLDa", S3_WINA, out, "B")
        g = values(sel(live, cell, "NEWa"), "gp")
        med5 = S3_WINA[cell][0]
        rel = S3_REL[cell]["gp"]
        if st != "IN-BAND" or not g:
            hv = "CONTROL-MOVED" if st == "CONTROL-MOVED" else "UNSCOREABLE"
        else:
            m = lc.med(g)
            hv = ("HIST-WORSE" if m < med5 * (1 - rel) else "HIST-BETTER" if m > med5 * (1 + rel)
                  else "HIST-SAME")
        out(f"  BHIST {cell} NEWa med={fmt(lc.med(g), 2)} vs §5 WINa {med5} band=[{med5 * (1 - rel):.2f},"
            f"{med5 * (1 + rel):.2f}] -> {hv}")
        verdicts[("Bh", cell)] = hv
    out("VERDICT-B " + " ".join(f"{c}:{verdicts[('B', c)]}/{verdicts[('Bh', c)]}" for c in B_CELLS))
    out("")
    # ── (C) EMIT_BATCH ──
    out("(C) EMB (RWM_EMIT_BATCH=1) vs NEW, §5 relative MDE applied to NEW's median")
    c_worse, c_feed, c_unsc, c_better = {}, {}, {}, {}
    for cell in C_CELLS:
        ctl, emb = sel(live, cell, "NEW"), sel(live, cell, "EMB")
        hard = blockers(rows, live, cell, ["NEW", "EMB"]) + (["abort"] if aborted else [])
        res = {m: compare(emb, ctl, cell, m, m == "gp") for m in ("gp", "ct", "cpu")}
        dc, de = dnf_rate(ctl), dnf_rate(emb)
        dnf_bad = dc is not None and de is not None and de - dc > DNF_THRESHOLD
        worse = [m for m in ("gp", "ct", "cpu") if res[m][0] == "WORSE"] + (["dnf"] if dnf_bad else [])
        better = [m for m in ("gp", "cpu") if res[m][0] == "BETTER"]
        why = list(hard)
        feed = []
        for i in range(sp.N_LEGS[cell]):
            k = "feed_ratio_p%d" % i
            rc_ = lc.med([r[k] for r in ctl if r["dnf"] is False and r.get(k) is not None])
            re_ = lc.med([r[k] for r in emb if r["dnf"] is False and r.get(k) is not None])
            tc = lc.med([r["truth_loss_p%d" % i] for r in ctl if r.get("truth_loss_p%d" % i) is not None])
            te = lc.med([r["truth_loss_p%d" % i] for r in emb if r.get("truth_loss_p%d" % i) is not None])
            pc = lc.med([r["plc_p%d" % i] for r in ctl if r.get("plc_p%d" % i) is not None])
            pe = lc.med([r["plc_p%d" % i] for r in emb if r.get("plc_p%d" % i) is not None])
            gc = lc.med([r["truth_gso_p%d" % i] for r in ctl if r.get("truth_gso_p%d" % i) is not None])
            ge = lc.med([r["truth_gso_p%d" % i] for r in emb if r.get("truth_gso_p%d" % i) is not None])
            if rc_ is None or re_ is None:
                why.append(f"feed p{i} unread")
                st = "UNREAD"
            elif rc_ / FEED_BAND <= re_ <= rc_ * FEED_BAND:
                st = "UNCHANGED"
            else:
                st = "MOVED"
                feed.append(f"p{i}")
            out(f"  FEED {cell} p{i} truth NEW={fmt(tc, 5)} EMB={fmt(te, 5)} | plc NEW={fmt(pc, 4)} EMB={fmt(pe, 4)}"
                f" | plc/truth NEW={fmt(rc_, 3)} EMB={fmt(re_, 3)} band=[{fmt(rc_ / FEED_BAND if rc_ else None, 3)},"
                f"{fmt(rc_ * FEED_BAND if rc_ else None, 3)}] {st} | gso NEW={fmt(gc, 2)} EMB={fmt(ge, 2)}")
        cpd_c, cpd_e = lc.med(values(ctl, "cpd")), lc.med(values(emb, "cpd"))
        out(f"  C {cell} NEW n={len(ctl)} EMB n={len(emb)} | "
            + " | ".join(f"{m}:{res[m][0]} ({res[m][1]})" for m in ("gp", "ct", "cpu"))
            + f" | dnf NEW={fmt(dc, 2)} EMB={fmt(de, 2)} excess>{DNF_THRESHOLD:.2f}:{int(dnf_bad)}"
            + f" | cpu_us/dgram NEW={fmt(cpd_c, 2)} EMB={fmt(cpd_e, 2)}"
            + f" | worse={'+'.join(worse) or '-'} better={'+'.join(better) or '-'} feed-moved={'+'.join(feed) or '-'}"
            + (f" | UNSCOREABLE({'; '.join(why)})" if why else ""))
        if worse and not hard:
            c_worse[cell] = worse
        if feed and not hard:
            c_feed[cell] = feed
        if why:
            c_unsc[cell] = why
        if better and not worse and not hard:
            c_better[cell] = better
    if aborted:
        cv = "UNSCOREABLE (" + ", ".join(aborted) + ")"
    elif c_worse:
        cv = "WORSE-AT-" + ",".join(c_worse)
    elif c_feed:
        cv = "FEED-MOVED-AT-" + ",".join(f"{c}:{'+'.join(v)}" for c, v in c_feed.items())
    elif c_unsc:
        cv = "UNSCOREABLE (" + "; ".join(f"{c}: {', '.join(v)}" for c, v in c_unsc.items()) + ")"
    elif c_better:
        cv = "FLIP-RECOMMENDED (better at " + ", ".join(f"{c}:{'+'.join(v)}" for c, v in c_better.items()) + ")"
    else:
        cv = "INERT-AS-DERIVED"
    out(f"VERDICT-C {cv}")
    out("")
    # ── descriptive: plu, feed, coded share, busy (not scored) ──
    out("DESCRIPTIVE (not scored) per cell/arm: plu on-floor share, plc/truth, coded share, busy, util")
    for cell in CELLS:
        for arm in PLAN[cell]:
            rs = sel(live, cell, arm)
            parts = []
            for i in range(sp.N_LEGS[cell]):
                pm = [r.get("plu_med_p%d" % i) for r in rs if r.get("plu_med_p%d" % i) is not None]
                fl = sum(1 for v in pm if PLU_FLOOR[0] <= v <= PLU_FLOOR[1])
                fr = [r["feed_ratio_p%d" % i] for r in rs if r.get("feed_ratio_p%d" % i) is not None]
                tl = [r["truth_loss_p%d" % i] for r in rs if r.get("truth_loss_p%d" % i) is not None]
                gs = [r["truth_gso_p%d" % i] for r in rs if r.get("truth_gso_p%d" % i) is not None]
                parts.append(f"p{i} plu={fmt(lc.med(pm), 4)} floor={fl}/{len(pm)} plc/truth={fmt(lc.med(fr), 3)} "
                             f"truth={fmt(lc.med(tl), 5)} gso={fmt(lc.med(gs), 2)}")
            cs = [r["coded_share"] for r in rs if r.get("coded_share") is not None]
            rb = [r["truth_rcvbuf_drops"] for r in rs if r.get("truth_rcvbuf_drops") is not None]
            out(f"  DESC {cell} {arm} n={len(rs)} " + " ".join(parts)
                + f" coded={fmt(lc.med(cs), 5)} busy={fmt(lc.med(values(rs, 'busy')), 1)}%"
                + f" cpu_srv={fmt(lc.med([r['cpu_srv'] for r in rs if r.get('cpu_srv') is not None]), 2)}"
                + f" rcvbuf_max={max(rb) if rb else '-'}")
    out("")
    out("PER-REP (live; per seed, rep order) cell arm seed: mbps / cpu_cli / cpu_us_per_dgram")
    for cell in CELLS:
        for arm in PLAN[cell]:
            for s in SEEDS:
                rs = sorted(sel(live, cell, arm, s), key=lambda r: r["rep"])
                if rs:
                    out(f"  REPS {cell} {arm} s{s}: "
                        + " ".join("DNF" if r["dnf"] else fmt(r["mbps"]) for r in rs) + " / "
                        + " ".join(fmt(r["cpu_cli"], 2) for r in rs) + " / "
                        + " ".join(fmt(r.get("cpu_us_per_dgram"), 1) for r in rs)
                        + f" | seed-median gp={fmt(lc.med(values(rs, 'gp')))}")
    return verdicts, cv


# ── (D) TUNNEL ───────────────────────────────────────────────────────────
_TUNARM = re.compile(r"^TUNARM label=(\S+) cell=(\S+) hint=(\S+) bytes=(\d+) reps=(\d+) seed=(\d+) bin=(\S+)")
_TUNREP = re.compile(r"^TUNREP label=(\S+) rep=(\d+) rc=(-?\d+) (\{.*\})")
_TUNMTU = re.compile(r"^TUNMTU cli=(\S+) srv=(\S+)")
_TUNPIPE = re.compile(r"^TUNPIPE cli=(\S+) srv=(\S+)")
_TUNCAD = re.compile(r"^TUNCAD cli=(\S+) srv=(\S+)")
_TUNSHA = re.compile(r"^TUNSHA ([0-9a-f]{64})")
# the D witnesses per binary (status.md §6 (D)): TUN MTU and the selected pipeline
TUN_MIN = 8   # live transfers per (cell, hint, binary) for a recorded measurement
TUN_EXPECT = {"new": {"mtu": "1196", "pipeline": "window", "cad": "ACTIVE"},
              "old": {"mtu": "1500", "pipeline": "block", "cad": "NONE"}}


def tun_arms(paths, shas=None):
    """Every tunnel bring-up -> dict(label, cell, hint, bin, seed, witnesses,
    status, reps=[mbps...]). `bin` is the label's prefix (new-/old-)."""
    arms = []
    cur = None
    for path in paths:
        for ln in lc.read(path):
            m = _TUNARM.match(ln)
            if m:
                cur = {"label": m.group(1), "cell": m.group(2), "hint": m.group(3),
                       "bytes": int(m.group(4)), "seed": m.group(6),
                       "bin": m.group(1).split("-")[0], "mtu": None, "pipe": None, "cad": None,
                       "sha": None, "reps": [], "nodata": 0, "bringup_fail": False}
                arms.append(cur)
                continue
            if cur is None:
                continue
            for rx, key in ((_TUNMTU, "mtu"), (_TUNPIPE, "pipe"), (_TUNCAD, "cad")):
                m = rx.match(ln)
                if m:
                    cur[key] = (m.group(1), m.group(2))
            m = _TUNSHA.match(ln)
            if m:
                cur["sha"] = m.group(1)
            if ln.startswith("TUN-BRINGUP-FAIL"):
                cur["bringup_fail"] = True
            m = _TUNREP.match(ln)
            if m:
                try:
                    o = json.loads(m.group(4))
                except ValueError:
                    o = {"nodata": True}
                if o.get("mbps") is not None:
                    cur["reps"].append(float(o["mbps"]))
                else:
                    cur["nodata"] += 1
    for a in arms:
        exp = TUN_EXPECT.get(a["bin"])
        prob = []
        if a["bringup_fail"]:
            prob.append("bringup-fail")
        if exp is None:
            prob.append("unknown-binary-label")
        else:
            if a["mtu"] != (exp["mtu"], exp["mtu"]):
                prob.append(f"mtu={a['mtu']}")
            pipe = a["pipe"] or ("-", "-")
            for side, p in zip(("cli", "srv"), pipe):
                f = p.split("/")
                if len(f) != 3 or f[0] != exp["pipeline"] or f[2] != a["hint"]:
                    prob.append(f"pipe-{side}={p}")
            if a["cad"] != (exp["cad"], exp["cad"]):
                prob.append(f"cad={a['cad']}")
            if shas and shas.get(a["bin"]) and a["sha"] != shas[a["bin"]]:
                prob.append("sha-mismatch")
        a["problems"] = prob
        a["status"] = "LIVE" if not prob else ("NO_DATA" if a["bringup_fail"] else "WITNESS-FAIL")
    return arms


def tun(paths, shas=None, out=print):
    arms = tun_arms(paths, shas)
    out("(D) TUNNEL: inner kernel TCP (cubic, cold connection per rep) through the tunnel; no MDE exists -> MEASUREMENT-RECORDED")
    for a in arms:
        if a["status"] != "LIVE":
            out(f"  TUN-NONLIVE {a['label']} {a['cell']} {a['hint']} s{a['seed']} {a['status']} {' '.join(a['problems'])}")
    keys = sorted({(a["cell"], a["hint"], a["bin"]) for a in arms})
    res = {}
    for k in keys:
        live = [a for a in arms if (a["cell"], a["hint"], a["bin"]) == k and a["status"] == "LIVE"]
        v = [x for a in live for x in a["reps"]]
        nod = sum(a["nodata"] for a in live)
        per_seed = {s: lc.med([x for a in live if a["seed"] == s for x in a["reps"]]) for s in SEEDS}
        res[k] = v
        mtu = live[0]["mtu"][0] if live else "-"
        out(f"  TUN {k[0]} {k[1]} {k[2]} mtu={mtu} bringups={len(live)} n={len(v)} nodata={nod} "
            f"med={fmt(lc.med(v), 2)} [{fmt(min(v, default=None), 2)}-{fmt(max(v, default=None), 2)}] "
            f"per-seed med s42={fmt(per_seed['42'], 2)} s7={fmt(per_seed['7'], 2)} reps={[round(x, 2) for x in v]}")
    for cell in sorted({k[0] for k in keys}):
        for hint in sorted({k[1] for k in keys if k[0] == cell}):
            n, o = res.get((cell, hint, "new"), []), res.get((cell, hint, "old"), [])
            if len(n) < TUN_MIN or len(o) < TUN_MIN:
                out(f"  TUN-OUTCOME {cell} {hint} UNSCOREABLE (live transfers new={len(n)} old={len(o)} < {TUN_MIN})")
            else:
                out(f"  TUN-OUTCOME {cell} {hint} MEASUREMENT-RECORDED (n new={len(n)} old={len(o)})")
            if n and o:
                mn, mo = lc.med(n), lc.med(o)
                overlap = not (min(n) > max(o) or max(n) < min(o))
                out(f"  TUN-DELTA {cell} {hint} new/old median = {mn / mo:.3f} ({100 * (mn / mo - 1):+.1f}%) "
                    f"ranges {'overlap' if overlap else 'DISJOINT'} (descriptive)")
            else:
                out(f"  TUN-DELTA {cell} {hint} UNREAD (new n={len(n)}, old n={len(o)})")
    return res


def check(path):
    rows, tokens, _ = rows_of([path])
    if not rows or "MALFORMED-V4ROW" in tokens:
        print(f"CHECK-FAIL {path} rows={len(rows)} tokens={tokens}")
        return 1
    print(f"CHECK-OK {path} rows={len(rows)} live={sum(1 for r in rows if r['status'] == 'LIVE')}")
    return 0


SMOKE_ARMS = {"NEW", "OLD", "NEWa", "OLDa", "EMB"}


def smoke(path, tun_log=None, tm_log=None):
    """rc 0 iff every smoke row is LIVE with every gauge the scorer reads
    (CPU line; [TRUTH] per leg; [DIAG] plc/plu/busy/cum), every V4 arm
    appears, the tunnel smoke has one LIVE bring-up per binary with >= 1 rep,
    and (when given) the tail_matrix smoke produced a `ship` rep line."""
    rows, _, shas = rows_of([path])
    bad = []
    for r in rows:
        miss = []
        if r["status"] != "LIVE":
            miss.append(r["status"])
        if r["cpu_cli"] is None:
            miss.append("no-CPU-line")
        for i in range(sp.N_LEGS[r["cell"]]):
            if r.get("truth_loss_p%d" % i) is None:
                miss.append(f"no-TRUTH-p{i}")
            for k in ("plc_p%d" % i, "plu_med_p%d" % i):
                if r.get(k) is None:
                    miss.append(f"no-{k}")
        if r.get("busy_med") is None:
            miss.append("no-busy")
        if r.get("coded_share") is None:
            miss.append("no-cum")
        if r.get("cpu_us_per_dgram") is None:
            miss.append("no-cpu-per-dgram")
        print(f"SMOKE-ROW {r['cell']} {r['arm']} bin={r['bin']} rc={r['rc']} status={r['status']} wall={r['wall_s']} "
              f"pipe={r['pipe_cli']}/{r['pipe_srv']} cad={int(r['cad_cli'])}/{int(r['cad_srv'])} "
              f"cadoff={int(r['cadoff_cli'])}/{int(r['cadoff_srv'])} pa={r['pa_cli']}/{r['pa_srv']} "
              f"emb_gate={r['emb_gate_cli']}/{r['emb_gate_srv']} emb_echo={int(r['emb_echo_cli'])}/{int(r['emb_echo_srv'])} "
              f"mbps={r['mbps']} dnf={r['dnf']} cpu={r['cpu_cli']} cpd={fmt(r.get('cpu_us_per_dgram'), 2)} "
              f"busy={r.get('busy_med')} {' '.join(r['problems'] + miss)}")
        if miss:
            bad.append(r)
    arms = {r["arm"] for r in rows}
    ok = bool(rows) and not bad and arms == SMOKE_ARMS
    if tun_log is not None:
        ta = tun_arms([tun_log], shas)
        for a in ta:
            print(f"SMOKE-TUN {a['label']} {a['cell']} {a['hint']} status={a['status']} mtu={a['mtu']} "
                  f"pipe={a['pipe']} cad={a['cad']} reps={a['reps']} {' '.join(a['problems'])}")
        live_bins = {a["bin"] for a in ta if a["status"] == "LIVE" and a["reps"]}
        ok = ok and live_bins == {"new", "old"}
    if tm_log is not None:
        reps = [ln for ln in lc.read(tm_log) if re.match(r"^\s*ship \d+B rep\d+: p50=", ln)]
        print(f"SMOKE-CROWN tail_matrix ship rep lines={len(reps)}")
        ok = ok and len(reps) >= 1
    if not ok:
        print("ABORT-SMOKE " + (f"{len(bad)} rows missing witnesses/gauges; arms={sorted(arms)}"
                                if rows else "no rows"))
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
        print("V4ROW " + json.dumps(row, sort_keys=True))
        return 0
    if cmd == "check":
        return check(a[0])
    if cmd == "smoke":
        return smoke(a[0], a[1] if len(a) > 1 else None, a[2] if len(a) > 2 else None)
    if cmd == "cost":
        return sp.cost(a[0])
    if cmd == "score":
        score(a)
        return 0
    if cmd == "tun":
        shas = {}
        for p in a:
            for ln in lc.read(p):
                m = re.match(r"^=== TUNBIN (new|old) ([0-9a-f]{64})", ln)
                if m:
                    shas[m.group(1)] = m.group(2)
        tun(a, shas)
        return 0
    print(f"unknown command {cmd}")
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
