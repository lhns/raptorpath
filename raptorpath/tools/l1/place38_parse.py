#!/usr/bin/env python3
"""Parser and scorer for THE PLACE-WIRE BATTERY (docs/status.md §38 with amendments 1-3; paper §5.7): the wire-clocked
placement base `RWM_PLACE_WIRE=1` (WIRE) against the same binary with the gate unset (MAIN), at the default store law
(c1s, c2 controls; c1d, c8 no-regression) and at static caps where MAIN reproduces the idle-leg lock-out (c1d 1536,
c8 192 / 256 / 512). Derived from threadpd_parse.py (§21's pass rule through l1ab.py).

    place38_parse.py row   <cell> <arm> <seed> <rep> <rc> <wall_s> <drv> <cli> <srv> [cotenant] [sha]   (env P38_STORE=def|<n>)
    place38_parse.py check|smoke <ledger>
    place38_parse.py score <ledger>
    place38_parse.py plan [smoke]            (`cell store arm seed rep` per line; the battery's source)
    place38_parse.py plan41 [smoke] | check41 <ledger> | smoke41 <ledger> | score41 <ledger>
        status §41: §38's battery unchanged plus the arm BASE (main's own binary, 4926822, built fresh) at c1s-400 and
        c1d-400 default. BASE never reaches the ladder; it gets its own gate (MAIN against BASE, WITHIN on every §21
        clause), printed beside it, plus the matched-speed CPU/GB and the per-row contention readouts.

Interpretations the spec leaves open (listed in the harness report):
  * the [LAG] clauses score E = p99 - min_us at RWM_RTOBS=1 (§28/§32's form of §21's [LAG] p99 clause); raw p99 is reported;
  * "[SUMCAP] eng absent or 0" and "win= denominator == n" are witnessed at the static cells only (the default law engages);
  * MAIN rows must show `wpl_us=-` and, when an [ETA] sender line is present, ref_fl/w_cold/w_mv `-` with w_n = w_calls = 0
    (the engine prints the counters as integers, not `-`);
  * cell order within a block is the table order with c8-192 before c8-256 and c8-512.
"""
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import l1common as lc  # noqa: E402
import stage3_parse as sp  # noqa: E402
import storelaw_parse as sl  # noqa: E402  (regexes only)
from l1ab import (  # noqa: E402
    sel, vals, per_byte, reading, spread_rel, blockers, mmm, MIN_LIVE, WITNESS_FAIL_LIMIT,
)
from threadp1_parse import rtp_floor  # noqa: E402
from threadpd_parse import lag_E, lag_max  # noqa: E402

for _a in ("MAIN", "WIRE", "BASE"):
    sp.ARM_SPEC[_a] = ("window", "Rlc", "bulk", "ACTIVE")
ARMS = ["MAIN", "WIRE"]                                  # the ladder's arms; BASE (status §41) never enters it

# (cell, store) in a block's order; store "def" = the shipped default law.
CELLS38 = [("c1s-400", "def"), ("c2-100", "def"), ("c1d-400", "def"), ("c8-100", "def"),
           ("c1d-400", "1536"), ("c8-100", "192"), ("c8-100", "256"), ("c8-100", "512")]
CONTROLS = [("c1s-400", "def"), ("c2-100", "def")]
DEFAULTS = [("c1d-400", "def"), ("c8-100", "def")]
STATIC = [c for c in CELLS38 if c[1] != "def"]
BLOCKS38 = [("42", 1), ("7", 1), ("42", 2), ("7", 2), ("42", 3), ("7", 3)]        # rep-major, as §32
SMOKE38 = [("c8-100", "256", "MAIN", "42", 1), ("c8-100", "256", "WIRE", "42", 1)]
EXPECT_ROWS38 = len(CELLS38) * len(ARMS) * len(BLOCKS38)
# ── status §41 ──
BASE_CELLS = [("c1s-400", "def"), ("c1d-400", "def")]
SMOKE41 = SMOKE38 + [("c1s-400", "def", "BASE", "42", 1)]        # DECISION (reported): a BASE row in the smoke
EXPECT_ROWS41 = EXPECT_ROWS38 + len(BASE_CELLS) * len(BLOCKS38)  # n = 6 per BASE cell: 108 (the pre-registration says 24 / 120)
MATCH_BAND = 0.03
S3_REL = {"c1s-400": {"gp": 0.049, "cpu": 0.024}, "c1d-400": {"gp": 0.056, "cpu": 0.065},
          "c2-100": {"gp": 0.014, "cpu": 0.063}, "c8-100": {"gp": 0.040, "cpu": 0.116}}
DNF_THRESHOLD = 0.20
FEED_BAND = 1.3
LOCK_SHARE, LOCK_RATIO = 0.10, 3.0
ONE_LEG_SHARE = 0.05
SHARE_ABS, SHARE_BAND = 0.03, 2.0
WPL_TOL_US, WPL_FRAC = 1000, 0.95
WARM_S = 1.0
ABORT_TOKENS = ("ABORT-LOCK", "ABORT-CRLF", "ABORT-SHA", "ABORT-BIN", "ABORT-SENTINEL-UNWRITABLE", "ABORT-SMOKE")


def key(cell, store):
    return f"{cell}@{store}"


def plan38(smoke=False):
    """[(cell, store, arm, seed, rep)] in run order: six blocks (rep-major), cells in table order, the two arms rotated
    by block (MAIN first on even blocks)."""
    if smoke:
        return list(SMOKE38)
    out = []
    for b, (seed, rep) in enumerate(BLOCKS38):
        for cell, store in CELLS38:
            arms = ARMS if b % 2 == 0 else ARMS[::-1]
            out += [(cell, store, a, seed, rep) for a in arms]
    return out


def base_position(b):
    """DECISION (reported): where BASE sits among the two ladder arms in block b. MAIN and WIRE keep §38's order (MAIN
    first on even blocks), so the plan minus BASE is plan38() exactly; BASE takes position b mod 3 (front, middle, back),
    twice each over the six blocks."""
    return b % 3


def plan41(smoke=False):
    """[(cell, store, arm, seed, rep)] in run order: plan38() plus BASE at BASE_CELLS."""
    if smoke:
        return list(SMOKE41)
    out = []
    for b, (seed, rep) in enumerate(BLOCKS38):
        for cell, store in CELLS38:
            arms = list(ARMS if b % 2 == 0 else ARMS[::-1])
            if (cell, store) in BASE_CELLS:
                arms.insert(base_position(b), "BASE")
            out += [(cell, store, a, seed, rep) for a in arms]
    return out


# ── readers ───────────────────────────────────────────────────────────────
_DIAG_T = re.compile(r"^\[DIAG\] t=([\d.]+)s ")
_LEG = re.compile(r"\bp(\d+):infl=(\d+)/.*?rtt=(\d+)/wrtt=(\d+)/(?:.*?\bwpl_us=(\S+))?")
_ETA = re.compile(r"\[ETA\] site=sender .*?\bcold_r=(\S+) cold_ge=(\S+) .*?ref_fl=(\S+) w_cold=(\S+) w_mv=(\S+) w_n=(\d+) w_calls=(\d+)")
_ETA_P = re.compile(r"\bp(\d+):n=\S+ drop=\S+ tau_us=\S+ e_p50=(\S+) e_p90=(\S+)")


def diag_lines(cli):
    """[(t, {leg: (infl, rtt_ms, wrtt_ms, wpl_us|None)})] for the client's [DIAG] lines that carry leg blocks. The
    leg blocks are split at ` p<i>:infl=` so a field of one leg is never read from the next."""
    out = []
    for ln in cli or []:
        m = _DIAG_T.match(ln)
        if not m:
            continue
        parts = re.split(r"(?= p\d+:infl=)", ln)
        legs = {}
        for part in parts[1:]:
            g = _LEG.search(part)
            if g:
                wpl = g.group(5)
                legs[int(g.group(1))] = (int(g.group(2)), int(g.group(3)), int(g.group(4)),
                                         int(wpl) if (wpl or "").isdigit() else None)
        if legs:
            out.append((float(m.group(1)), legs))
    return out


def eta_last(cli):
    """The last `[ETA] site=sender` line -> {cold_r, cold_ge, ref_fl, w_cold, w_mv, w_n, w_calls, e_p50, e_p90}; None if absent."""
    last = None
    for ln in cli or []:
        if "[ETA] site=sender" in ln:
            last = ln
    if last is None:
        return None
    m = _ETA.search(last)
    if not m:
        return None
    d = {"cold_r": m.group(1), "cold_ge": m.group(2), "ref_fl": m.group(3), "w_cold": m.group(4), "w_mv": m.group(5),
         "w_n": int(m.group(6)), "w_calls": int(m.group(7)), "e_p50": {}, "e_p90": {}}
    for p in _ETA_P.finditer(last):
        d["e_p50"][int(p.group(1))], d["e_p90"][int(p.group(1))] = p.group(2), p.group(3)
    return d


def _f(x):
    try:
        return float(x)
    except (TypeError, ValueError):
        return None


def lock_analysis(diag, shares):
    """Per leg: LOCKED iff the [TRUTH] egress share < 10 % and the median post-warm-up [DIAG] rtt >= 3 x the median wrtt
    (§38). -> {leg: {share, rtt_med, wrtt_med, locked}}"""
    out = {}
    legs = sorted({i for _, L in diag for i in L})
    for i in legs:
        post = [L[i] for t, L in diag if t >= WARM_S and i in L]
        rtt = lc.med([x[1] for x in post]) if post else None
        wrtt = lc.med([x[2] for x in post]) if post else None
        sh = shares.get(i)
        out[i] = {"share": sh, "rtt_med": rtt, "wrtt_med": wrtt,
                  "locked": bool(sh is not None and sh < LOCK_SHARE and rtt is not None and rtt >= LOCK_RATIO * max(wrtt, 1))}
    return out


def first_burst(diag, leg):
    """The first [DIAG] line that carries both legs: the leg's in-flight share and rtt, wrtt (the trigger's reading)."""
    for t, L in diag:
        if len(L) >= 2 and leg in L:
            tot = sum(v[0] for v in L.values())
            return {"t": t, "infl_share": (L[leg][0] / tot) if tot else None, "rtt": L[leg][1], "wrtt": L[leg][2]}
    return None


def idle_decay(diag):
    """Per leg, over post-warm-up lines with infl == 0: the largest wpl_us and the value one second later (nearest line
    within 0.2 s), reported only. -> {leg: {t, wpl_us, later}}"""
    out = {}
    post = [(t, L) for t, L in diag if t >= WARM_S]
    for i in sorted({i for _, L in post for i in L}):
        cand = [(L[i][3], t) for t, L in post if i in L and L[i][0] == 0 and L[i][3] is not None]
        if not cand:
            continue
        w, t0 = max(cand)
        later = [(abs(t - (t0 + 1.0)), L[i][3]) for t, L in post if i in L and L[i][3] is not None and abs(t - (t0 + 1.0)) < 0.2]
        out[i] = {"t": t0, "wpl_us": w, "later": min(later)[1] if later else None}
    return out


# ── ROW ───────────────────────────────────────────────────────────────────
def make_row(cell, arm, seed, rep, rc, wall_s, drv, cli, srv, cotenant=0, sha=None, store=None):
    store = store or os.environ.get("P38_STORE", "def")
    row = sp.make_row(cell, arm, seed, rep, rc, wall_s, drv, cli, srv, cotenant)
    row["bin_sha"], row["store"] = sha, store
    nleg = sp.N_LEGS[cell]
    row["gate_wire_cli"], row["gate_wire_srv"] = lc.gate_tok(cli, "RWM_PLACE_WIRE"), lc.gate_tok(srv, "RWM_PLACE_WIRE")
    row["gate_store"] = lc.gate_tok(cli, "RWM_STORE")
    cap = [m for m in (sl._SUMCAP.search(ln) for ln in cli) if m]
    row["sumcap_eng"] = int(cap[-1].group(2)) if cap else None
    dens = []
    for ln in cli:
        mt = sl._T.search(ln)
        if mt and float(mt.group(1)) >= WARM_S:
            mw = sl._WIN.search(ln)
            if mw:
                dens.append(int(mw.group(2)))
    row["win_den_min"], row["win_den_max"], row["win_n"] = (min(dens), max(dens), len(dens)) if dens else (None, None, 0)
    diag = diag_lines(cli)
    truth = lc.truth(drv)
    egress = {i: (truth.get(i) or {}).get("egress_dgrams") for i in range(nleg)}
    tot = sum(v for v in egress.values() if v)
    shares = {i: (v / tot if (v is not None and tot) else None) for i, v in egress.items()}
    row["shares"] = [shares[i] for i in range(nleg)]
    lk = lock_analysis(diag, shares)
    row["legs"] = [lk.get(i) for i in range(nleg)]
    row["locked"] = any(v["locked"] for v in lk.values())
    row["one_leg"] = nleg > 1 and (row["locked"] or any((s is not None and s < ONE_LEG_SHARE) for s in shares.values()))
    row["first_burst"] = first_burst(diag, nleg - 1) if nleg > 1 else None
    row["idle_decay"] = idle_decay(diag) if arm == "WIRE" else {}
    eta = eta_last(cli)
    row["eta"] = eta
    row["rtt_wrtt_med"] = [(lk.get(i) or {}).get("rtt_med") for i in range(nleg)]
    for i, v in rtp_floor(cli).items():
        row["rtp_floor_p%d" % i] = v
    for side, lines, win in (("cli", cli, "run=1"), ("srv", srv, "obj=1")):
        p99, mn, e = lag_E(lines, win)
        row[f"{side}_lag_p99_us"], row[f"{side}_lag_min_us"], row[f"{side}_lag_E"] = p99, mn, e
        row[f"{side}_lag_max_us"] = lag_max(lines, win)
    post = [(t, L) for t, L in diag if t >= WARM_S]
    # ── witnesses (a row is void otherwise) ──
    p = []
    want = "1" if arm == "WIRE" else "0"
    if arm == "BASE":
        # Main's own binary (§41) has no RWM_PLACE_WIRE: the token must be absent, and nothing of the arm may print.
        if row["gate_wire_cli"] is not None or row["gate_wire_srv"] is not None:
            p.append(f"gate-place-wire={row['gate_wire_cli']}/{row['gate_wire_srv']} on BASE (want absent)")
    elif row["gate_wire_cli"] != want or row["gate_wire_srv"] != want:
        p.append(f"gate-place-wire={row['gate_wire_cli']}/{row['gate_wire_srv']} (want {want})")
    if store == "def":
        if row["gate_store"] != "unset":
            p.append(f"gate-store={row['gate_store']} (want unset)")
    else:
        if row["gate_store"] != store:
            p.append(f"gate-store={row['gate_store']} (want {store})")
        if row["sumcap_eng"]:
            p.append(f"sumcap-eng={row['sumcap_eng']}")
        if row["win_n"] < 2:
            p.append(f"win-windows={row['win_n']}")
        elif row["win_den_min"] != int(store) or row["win_den_max"] != int(store):
            p.append(f"win-den-not-n={row['win_den_min']}..{row['win_den_max']}")
    if arm == "WIRE":
        if eta is None:
            p.append("no-eta-sender-line")
        elif eta["w_calls"] <= 0:
            p.append(f"w_calls={eta['w_calls']}")
        if nleg > 1:
            nums = [all(L[i][3] is not None for i in range(nleg) if i in L) and len(L) >= nleg for _, L in post]
            if not post or not all(nums):
                p.append("wpl_us-not-numeric-on-every-leg")
            else:
                close = sum(1 for _, L in post if all(abs(L[i][3] - 1000 * L[i][2]) <= WPL_TOL_US for i in range(nleg)))
                row["wpl_close_frac"] = close / len(post)
                if close / len(post) < WPL_FRAC:
                    p.append(f"wpl-within-1ms-of-wrtt={close}/{len(post)}")
            if eta is not None and not (_f(eta["w_mv"]) or 0) > 0:
                p.append(f"w_mv={eta['w_mv']}")
    else:
        if any(L[i][3] is not None for _, L in diag for i in L):
            p.append(f"wpl_us-numeric-on-{arm}")
        if eta is not None and (eta["w_n"] or eta["w_calls"] or any(eta[k] != "-" for k in ("ref_fl", "w_cold", "w_mv"))):
            p.append(f"wire-gauges-on-{arm}")
    row["problems"].extend(p)
    if p and row["status"] == "LIVE":
        row["status"] = "WITNESS-FAIL"
    if os.environ.get("LD_ONDISK", "0") == "1":
        row["status"] = "VOID-ONDISK"
    io = os.environ.get("LD_IO_FULL10")
    try:
        row["io_full10"] = float(io) if io not in (None, "") else None
    except ValueError:
        row["io_full10"] = None
    row["io_high"] = row["io_full10"] is not None and row["io_full10"] >= 20.0
    # §41: the per-row contention covariates, sampled by the battery before and after the invocation (never a void).
    for k, env in (("procs_before", "LD_PROCS_B"), ("procs_after", "LD_PROCS_A"), ("io_before", "LD_IO_B"), ("io_after", "LD_IO_A")):
        try:
            row[k] = float(os.environ[env])
        except (KeyError, ValueError):
            row[k] = None
    return row


# ── LEDGER ────────────────────────────────────────────────────────────────
def rows_of(paths):
    rows, tokens = [], []
    for path in paths:
        for ln in lc.read(path):
            if ln.startswith("P38ROW "):
                try:
                    rows.append(json.loads(ln[len("P38ROW "):]))
                except ValueError:
                    tokens.append("MALFORMED-P38ROW")
            for t in ABORT_TOKENS:
                if ln.startswith(t):
                    tokens.append(t)
    return rows, tokens


fmt = sp.fmt


def cell_rows(live, cell, store, arm):
    return [r for r in live if r["cell"] == cell and r["store"] == store and r["arm"] == arm]


def med_mm(v, nd=3):
    v = [x for x in v if x is not None]
    if not v:
        return "-"
    return f"{lc.med(v):.{nd}f} [{min(v):.{nd}f}-{max(v):.{nd}f}] n={len(v)}"


def clause_readings(xs, rs, cell, out, tag, name="WIRE", ref="MAIN"):
    """§21's pass rule of the WIRE rows `xs` against MAIN rows `rs` at one (cell, store): the readings and the list of
    WORSE clauses (feed moved and the DNF excess included)."""
    rel = S3_REL[cell]
    c = {}
    c["goodput"] = reading(vals(xs, "mbps"), vals(rs, "mbps"), rel["gp"], True, name, ref)
    c["cpu_cli/GB"] = reading(per_byte(xs, "cpu_cli"), per_byte(rs, "cpu_cli"), rel["cpu"], False, name, ref)
    c["cpu_srv/GB"] = reading(per_byte(xs, "cpu_srv"), per_byte(rs, "cpu_srv"), rel["cpu"], False, name, ref)
    for side in ("cli", "srv"):
        k = f"{side}_lag_E"
        rv = vals(rs, k)
        c[f"lag_E_{side}"] = reading(vals(xs, k), rv, spread_rel(rv), False, name, ref)
    feed_bad, feed_unread = [], []
    for i in range(sp.N_LEGS[cell]):
        k = "rtp_floor_p%d" % i
        rv = vals(rs, k)
        c[f"rtp_floor_p{i}"] = reading(vals(xs, k), rv, spread_rel(rv), False, name, ref)
        fk = "feed_ratio_p%d" % i
        fx, fr = lc.med(vals(xs, fk)), lc.med(vals(rs, fk))
        if fx is None or fr is None:
            feed_unread.append(i)
        elif not (fr / FEED_BAND <= fx <= fr * FEED_BAND):
            feed_bad.append(i)
        out(f"    FEED {tag} p{i} plc/truth {name}={fmt(fx, 3)} {ref}={fmt(fr, 3)} -> "
            + ("UNREAD" if fx is None or fr is None else ("SAME" if i not in feed_bad else "FEED-MOVED")))
    dr = (sum(1 for r in rs if r["dnf"]) / len(rs)) if rs else None
    dx = (sum(1 for r in xs if r["dnf"]) / len(xs)) if xs else None
    dnf_bad = dr is not None and dx is not None and dx - dr > DNF_THRESHOLD
    for k, (rd, det) in c.items():
        out(f"    CLAUSE {tag} {k}: {rd} {det}")
    worse = [k for k, (rd, _) in c.items() if rd == "WORSE"] + [f"feed_p{i}" for i in feed_bad] + (["dnf"] if dnf_bad else [])
    return c, worse, feed_unread


def share_clause(xs, rs, band):
    """SHARE-PRESERVED: the WIRE rows' median slow/second-leg share against the median of MAIN's non-LOCKED rows.
    -> (state, detail): state PRESERVED / MISSED / REPORTED (fewer than 2 non-LOCKED MAIN rows) / UNREAD"""
    ms = [r["shares"][-1] for r in rs if not r["locked"] and r["shares"][-1] is not None]
    ws = [r["shares"][-1] for r in xs if r["shares"][-1] is not None]
    if not ws:
        return "UNREAD", "WIRE has no share"
    wm = lc.med(ws)
    if len(ms) < 2:
        return "REPORTED", f"WIRE median {wm:.3f}; MAIN non-LOCKED rows {len(ms)} < 2: not gated"
    mm = lc.med(ms)
    ok = abs(wm - mm) <= SHARE_ABS or (band and mm > 0 and mm / SHARE_BAND <= wm <= mm * SHARE_BAND)
    return ("PRESERVED" if ok else "MISSED"), (f"WIRE median {wm:.3f} vs MAIN non-LOCKED median {mm:.3f} (n={len(ms)}); "
                                                f"{'|d| <= %.2f' % SHARE_ABS}{' or within [1/2, 2] x' if band else ''}")


# ── status §41: the BASE gate, matched-speed CPU/GB, contention (reported beside the ladder, never part of it) ──
def base_gate(live, out):
    """BASE gate: MAIN (the new binary, RWM_PLACE_WIRE unset) against BASE (main's own binary) must read WITHIN on every
    §21 clause at both BASE cells. Literal: TREND-* and BETTER are not WITHIN, VACUOUS / UNREAD are not a pass.
    -> (state, {cell@store: [non-passing items]}) with state PASS / FAIL / UNREADABLE (< MIN_LIVE rows in either arm)."""
    per, state = {}, "PASS"
    for cell, store in BASE_CELLS:
        k = key(cell, store)
        ms, bs = cell_rows(live, cell, store, "MAIN"), cell_rows(live, cell, store, "BASE")
        out(f"[BASE-GATE {k}]  live MAIN={len(ms)} BASE={len(bs)}")
        if len(ms) < MIN_LIVE or len(bs) < MIN_LIVE:
            per[k] = [f"< {MIN_LIVE} live rows (MAIN {len(ms)}, BASE {len(bs)})"]
            state = "UNREADABLE" if state == "PASS" else state
            out(f"  BASE-GATE {k}: UNREADABLE {per[k][0]}")
            continue
        c, worse, feed_unread = clause_readings(ms, bs, cell, out, "BASE-GATE " + k, "MAIN", "BASE")
        bad = [f"{n}={rd}" for n, (rd, _) in c.items() if rd != "WITHIN"]
        bad += [w for w in worse if w.startswith("feed_") or w == "dnf"]
        bad += [f"feed_p{i}=UNREAD" for i in feed_unread]
        per[k] = bad
        out(f"  BASE-GATE {k}: {'PASS' if not bad else 'FAIL ' + ', '.join(bad)}")
        if bad:
            state = "FAIL"
    return state, per


def matched_speed(live, out):
    """Reported: client / server CPU per GB of WIRE and MAIN rows within +-3 % of the pooled (WIRE + MAIN) median goodput
    at each cell; BASE rows in the same window are printed beside them where the cell has them."""
    for cell, store in CELLS38:
        k = key(cell, store)
        pool = [r for r in live if (r["cell"], r["store"]) == (cell, store) and r["arm"] in ARMS and r.get("mbps") is not None]
        med = lc.med([r["mbps"] for r in pool])
        if med is None:
            out(f"MATCHED-SPEED {k}: no rows")
            continue
        lo, hi = med * (1 - MATCH_BAND), med * (1 + MATCH_BAND)
        out(f"MATCHED-SPEED {k}: pooled WIRE+MAIN median goodput {med:.1f} Mbit/s (n={len(pool)}); window [{lo:.1f}, {hi:.1f}]")
        sel_ = {a: [r for r in cell_rows(live, cell, store, a) if r.get("mbps") is not None and lo <= r["mbps"] <= hi]
                for a in ("MAIN", "WIRE", "BASE")}
        for a in ("MAIN", "WIRE", "BASE"):
            if a == "BASE" and not cell_rows(live, cell, store, "BASE"):
                continue
            out(f"  {a}: n={len(sel_[a])} of {len(cell_rows(live, cell, store, a))}  goodput {mmm(vals(sel_[a], 'mbps'), 1)}  "
                f"client CPU/GB {mmm(per_byte(sel_[a], 'cpu_cli'))}  server CPU/GB {mmm(per_byte(sel_[a], 'cpu_srv'))}")
        for lab, kk in (("client", "cpu_cli"), ("server", "cpu_srv")):
            mx, mr = lc.med(per_byte(sel_["WIRE"], kk)), lc.med(per_byte(sel_["MAIN"], kk))
            out(f"  {lab} CPU/GB WIRE vs MAIN at matched speed: "
                + (f"{100 * (mx / mr - 1):+.1f} % (WIRE {mx:.3f}, MAIN {mr:.3f}; n={len(sel_['WIRE'])}/{len(sel_['MAIN'])})" if mx and mr else "not readable"))


def contention(live_or_all, out):
    """Reported: every c1s row's procs_running and io `full avg10`, before and after the invocation."""
    out("CONTENTION c1s-400@def, every row (status or not): arm seed rep status mbps | procs_running before/after | io full avg10 before/after")
    rs = sorted([r for r in live_or_all if r["cell"] == "c1s-400" and r["store"] == "def"],
                key=lambda r: (r["arm"], r["seed"], r["rep"]))
    for r in rs:
        out(f"  {r['arm']} s{r['seed']} r{r['rep']} {r['status']} mbps={fmt(r.get('mbps'), 1)} | procs_running {fmt(r.get('procs_before'), 0)}/{fmt(r.get('procs_after'), 0)}"
            f" | io {fmt(r.get('io_before'), 2)}/{fmt(r.get('io_after'), 2)}")


def score(paths, out=print, plan41=False):
    rows, tokens = rows_of(paths)
    live = [r for r in rows if r["status"] == "LIVE"]
    aborted = sorted({t for t in tokens if t in ABORT_TOKENS})
    if plan41:
        out("PLACE-WIRE BATTERY (status §41 = §38 + arm BASE) SCORE: MAIN = RWM_PLACE_WIRE unset, WIRE = RWM_PLACE_WIRE=1, one binary; "
            "BASE = main's own binary (4926822), reported beside the ladder")
    else:
        out("PLACE-WIRE BATTERY (status §38) SCORE: MAIN = RWM_PLACE_WIRE unset, WIRE = RWM_PLACE_WIRE=1, one binary")
    out(f"ROWS {len(rows)}/{EXPECT_ROWS41 if plan41 else EXPECT_ROWS38}; ABORT-TOKENS {' '.join(aborted) or 'none'}")
    out("STATUS-TABLE (every invocation row)")
    for st in ("LIVE", "NO_DATA", "VOID-RC", "VOID-COTENANT", "VOID-ONDISK", "CONTAMINATED", "WITNESS-FAIL"):
        out(f"  {st:<13} {sum(1 for r in rows if r['status'] == st)}")
    for r in rows:
        if r["status"] != "LIVE" or r.get("io_high"):
            out(f"  NONLIVE {r['cell']}@{r['store']} {r['arm']} s{r['seed']} rep{r['rep']} {r['status']} rc={r['rc']} "
                f"{' '.join(r['problems'])} io_full10={r.get('io_full10')}")
    out("")
    out("PER-ROW: cell@store arm seed rep status mbps share(slow/2nd) LOCKED one_leg rtt/wrtt(med per leg) w_mv w_calls")
    for r in sorted(rows, key=lambda r: ([k for k in CELLS38].index((r["cell"], r["store"])) if (r["cell"], r["store"]) in CELLS38 else 99,
                                          r["arm"], r["seed"], r["rep"])):
        e = r.get("eta") or {}
        out(f"  {key(r['cell'], r['store'])} {r['arm']} s{r['seed']} r{r['rep']} {r['status']} mbps={fmt(r.get('mbps'), 1)} "
            f"share={'/'.join(fmt(s, 3) for s in r.get('shares') or [])} locked={int(bool(r.get('locked')))} "
            f"one_leg={int(bool(r.get('one_leg')))} rtt/wrtt={'/'.join(fmt(x, 0) for x in r.get('rtt_wrtt_med') or [])} "
            f"w_mv={e.get('w_mv', '-')} w_calls={e.get('w_calls', '-')}")
    out("")
    res = {"cells": {}}
    unsc = []
    clause_by = {}
    for cell, store in CELLS38:
        k = key(cell, store)
        xs, rs = cell_rows(live, cell, store, "WIRE"), cell_rows(live, cell, store, "MAIN")
        out(f"[{k}]  live MAIN={len(rs)} WIRE={len(xs)}")
        if len(xs) < MIN_LIVE or len(rs) < MIN_LIVE:
            unsc.append(k)
        c, worse, feed_unread = clause_readings(xs, rs, cell, out, k)
        clause_by[k] = {"readings": {n: rd for n, (rd, _) in c.items()}, "worse": worse}
        nl = lambda a: sum(1 for r in a if r["locked"])  # noqa: E731
        out(f"  LOCKED MAIN {nl(rs)}/{len(rs)}  WIRE {nl(xs)}/{len(xs)}")
        out(f"  goodput MAIN {med_mm([r.get('mbps') for r in rs], 1)}  WIRE {med_mm([r.get('mbps') for r in xs], 1)}")
        if sp.N_LEGS[cell] > 1:
            out(f"  share (slow/second leg, egress) MAIN {med_mm([r['shares'][-1] for r in rs])}  WIRE {med_mm([r['shares'][-1] for r in xs])}")
        out(f"  worse={'+'.join(worse) or '-'}")
        out(f"  REPORT [LAG] raw p99 cli MAIN {mmm(vals(rs, 'cli_lag_p99_us'), 0)} WIRE {mmm(vals(xs, 'cli_lag_p99_us'), 0)}; "
            f"srv MAIN {mmm(vals(rs, 'srv_lag_p99_us'), 0)} WIRE {mmm(vals(xs, 'srv_lag_p99_us'), 0)}")
        out(f"  REPORT gauges WIRE w_cold={med_mm([_f((r.get('eta') or {}).get('w_cold')) for r in xs], 4)} "
            f"w_mv={med_mm([_f((r.get('eta') or {}).get('w_mv')) for r in xs], 4)} ref_fl={med_mm([_f((r.get('eta') or {}).get('ref_fl')) for r in xs], 4)} "
            f"cold_ge={med_mm([_f((r.get('eta') or {}).get('cold_ge')) for r in xs], 4)} cold_r={med_mm([_f((r.get('eta') or {}).get('cold_r')) for r in xs], 4)} "
            f"w_n={med_mm([(r.get('eta') or {}).get('w_n') for r in xs], 0)} w_calls={med_mm([(r.get('eta') or {}).get('w_calls') for r in xs], 0)}")
        for a, rr in (("MAIN", rs), ("WIRE", xs)):
            for leg in range(sp.N_LEGS[cell]):
                out(f"  REPORT [ETA] e_p50/e_p90 {a} p{leg}: "
                    f"{med_mm([_f(((r.get('eta') or {}).get('e_p50') or {}).get(str(leg), ((r.get('eta') or {}).get('e_p50') or {}).get(leg))) for r in rr], 0)} / "
                    f"{med_mm([_f(((r.get('eta') or {}).get('e_p90') or {}).get(str(leg), ((r.get('eta') or {}).get('e_p90') or {}).get(leg))) for r in rr], 0)}")
        if store != "def" and sp.N_LEGS[cell] > 1:
            for a, rr in (("MAIN", rs), ("WIRE", xs)):
                fb = [r["first_burst"] for r in rr if r.get("first_burst")]
                out(f"  FIRST-BURST {a} (first [DIAG] line with both legs, t ~ 0.3 s) slow/second leg in-flight share "
                    f"{med_mm([f['infl_share'] for f in fb])}  rtt/wrtt {med_mm([(f['rtt'] / max(f['wrtt'], 1)) for f in fb], 2)}")
        dec = [(r['idle_decay']) for r in xs if r.get("idle_decay")]
        if dec:
            for leg in sorted({int(i) for d in dec for i in d}):
                seq = [d.get(str(leg), d.get(leg)) for d in dec if d.get(str(leg), d.get(leg))]
                out(f"  REPORT idle-leg w-hat decay WIRE p{leg}: largest wpl_us on infl=0 {med_mm([s['wpl_us'] for s in seq], 0)}; "
                    f"1 s later {med_mm([s['later'] for s in seq], 0)}")
        res["cells"][k] = {"locked_main": nl(rs), "locked_wire": nl(xs), "n_main": len(rs), "n_wire": len(xs),
                           "worse": worse, "readings": clause_by[k]["readings"]}
    out("")
    # the stated c1d cost and the c8 gain, stratified (reported)
    for cell, store in (("c1d-400", "1536"), ("c1d-400", "def")):
        k = key(cell, store)
        xs, rs = cell_rows(live, cell, store, "WIRE"), cell_rows(live, cell, store, "MAIN")
        one = [r for r in rs if r["one_leg"]]
        two = [r for r in rs if not r["one_leg"]]
        mx, mr = lc.med(vals(xs, "mbps")), lc.med(vals(rs, "mbps"))
        pc = (100 * (mx / mr - 1)) if (mx and mr) else None
        out(f"c1d COST at {k}: WIRE - MAIN goodput {fmt(pc, 1)} % of MAIN's median (WIRE [{fmt(min(vals(xs, 'mbps')), 1) if vals(xs, 'mbps') else '-'}"
            f"-{fmt(max(vals(xs, 'mbps')), 1) if vals(xs, 'mbps') else '-'}] MAIN [{fmt(min(vals(rs, 'mbps')), 1) if vals(rs, 'mbps') else '-'}"
            f"-{fmt(max(vals(rs, 'mbps')), 1) if vals(rs, 'mbps') else '-'}])")
        for name, ref in (("MAIN two-leg rows", two), ("MAIN one-leg rows", one)):
            if ref and xs:
                rd, det = reading(vals(xs, "mbps"), vals(ref, "mbps"), S3_REL[cell]["gp"], True, "WIRE")
                out(f"  stratified: WIRE vs {name} (n={len(ref)}): {rd} {det}")
            else:
                out(f"  stratified: WIRE vs {name}: n={len(ref)} (not readable)")
    for store in ("192", "256", "512"):
        k = key("c8-100", store)
        xs, rs = cell_rows(live, "c8-100", store, "WIRE"), cell_rows(live, "c8-100", store, "MAIN")
        for name, ref in (("MAIN LOCKED rows", [r for r in rs if r["locked"]]), ("MAIN fresh rows", [r for r in rs if not r["locked"]])):
            if ref and xs:
                rd, det = reading(vals(xs, "mbps"), vals(ref, "mbps"), S3_REL["c8-100"]["gp"], True, "WIRE")
                out(f"c8 GAIN at {k}: WIRE vs {name} (n={len(ref)}): {rd} {det}")
            else:
                out(f"c8 GAIN at {k}: WIRE vs {name}: n={len(ref)} (not readable)")
    out("")
    # ── the ladder, in precedence order ──
    verdict, notes = None, []
    if aborted or unsc:
        verdict = "UNSCOREABLE (" + "; ".join(([f"abort {','.join(aborted)}"] if aborted else [])
                                              + ([f"< {MIN_LIVE} live rows in either arm at {','.join(unsc)}"] if unsc else [])) + ")"
    reproduced, not_repro = [], []
    for cell, store in STATIC:
        k = key(cell, store)
        (reproduced if res["cells"][k]["locked_main"] >= 2 else not_repro).append(k)
        out(f"REPRODUCTION {k}: MAIN LOCKED {res['cells'][k]['locked_main']}/{res['cells'][k]['n_main']} -> "
            f"{'reproduced' if k in reproduced else 'NOT reproduced'}")
    if verdict is None and not reproduced:
        verdict = "NOT-REPRODUCED"
    elif not_repro:
        notes.append("NOT-REPRODUCED-AT(" + ",".join(not_repro) + ")")
    reg = []
    if verdict is None:
        for cell, store in CONTROLS + DEFAULTS:
            k = key(cell, store)
            # Amendment 4: §21's goodput clause at c1d default decides CURED vs CURED-AT-COST only (exempt here).
            worse = [w for w in res["cells"][k]["worse"] if not (k == key("c1d-400", "def") and w == "goodput")]
            if worse:
                reg.append(k)
                out(f"REGRESSES? {k}: §21 clause(s) WORSE: {','.join(worse)}")
        for cell, store in CONTROLS:
            k = key(cell, store)
            wm = [_f((r.get("eta") or {}).get("w_mv")) for r in cell_rows(live, cell, store, "WIRE")]
            if any(x not in (None, 0.0) for x in wm):
                reg.append(k)
                out(f"REGRESSES? {k}: w_mv != 0 at a control: {wm}")
        for store, band in (("def", False), ("192", True)):
            k = key("c8-100", store)
            st, det = share_clause(cell_rows(live, "c8-100", store, "WIRE"), cell_rows(live, "c8-100", store, "MAIN"), band)
            out(f"SHARE-PRESERVED {k}: {st} ({det})")
            if st == "MISSED":
                reg.append(k)
        for cell, store in STATIC:
            k = key(cell, store)
            if cell == "c8-100" and k in reproduced and clause_by[k]["readings"].get("goodput") == "WORSE":
                reg.append(k)
                out(f"REGRESSES? {k}: goodput WORSE at a reproduced c8 static cell")
        if reg:
            verdict = "REGRESSES-AT(" + ",".join(sorted(set(reg))) + ")"
    if verdict is None:
        locked_wire = [k for k in reproduced if res["cells"][k]["locked_wire"] > 0]
        for k in locked_wire:
            out(f"NOT-CURED? {k}: {res['cells'][k]['locked_wire']} WIRE row(s) LOCKED")
        if locked_wire:
            verdict = "NOT-CURED"
    if verdict is None:
        xs, rs = cell_rows(live, "c1d-400", "def", "WIRE"), cell_rows(live, "c1d-400", "def", "MAIN")
        gp = clause_by[key("c1d-400", "def")]["readings"].get("goodput")
        mx, mr = lc.med(vals(xs, "mbps")), lc.med(vals(rs, "mbps"))
        pc = (100 * (mx / mr - 1)) if (mx and mr) else None
        if gp in ("WORSE", "TREND-WORSE"):
            verdict = f"CURED-AT-COST({fmt(pc, 1)} %)"
        else:
            verdict = "CURED"
        out(f"COST at c1d-400@def: goodput clause {gp}, WIRE - MAIN {fmt(pc, 1)} %")
    if notes:
        verdict += " [" + "; ".join(notes) + "]"
    out("VERDICT " + verdict)
    res["verdict"], res["reproduced"], res["unscoreable"] = verdict, reproduced, unsc
    if plan41:
        out("")
        out("=== beside the ladder (status §41; none of this enters the verdict) ===")
        state, per = base_gate(live, out)
        out(f"BASE-GATE {state}" + ("" if state == "PASS" else " (the new binary does not reach main even switched off, whatever the ladder says)"
                                    if state == "FAIL" else ""))
        res["base_gate"], res["base_gate_items"] = state, per
        out("")
        matched_speed(live, out)
        out("")
        contention(rows, out)
    out("RESULT " + json.dumps(res, sort_keys=True, default=str))
    return verdict


def check(path, plan41=False):
    rows, tokens = rows_of([path])
    t = open(path, errors="replace").read()
    want = EXPECT_ROWS41 if plan41 else EXPECT_ROWS38
    ok = "LD-BATTERY-DONE" in t and len(rows) == want
    print(f"CHECK{'41' if plan41 else '38'} done={'LD-BATTERY-DONE' in t} rows={len(rows)}/{want}")
    return 0 if ok else 1


def smoke(path, plan41=False):
    """rc 0 iff both smoke rows (MAIN and WIRE at c8-100@256) are LIVE with the gauges the scorer reads and WIRE has w_calls > 0.
    Plan 41 adds one BASE row (c1s-400 default): LIVE with the one-leg clause inputs."""
    rows, _ = rows_of([path])
    bad = []
    got = {(r["cell"], r["store"], r["arm"]): r for r in rows}
    for cell, store, arm, _, _ in (SMOKE41 if plan41 else SMOKE38):
        r = got.get((cell, store, arm))
        if r is None:
            bad.append(f"{arm}: no row")
            continue
        miss = [] if r["status"] == "LIVE" else [r["status"]]
        legs = range(sp.N_LEGS[cell])
        for k in ["cpu_cli", "cpu_srv", "mbps", "cli_lag_E", "srv_lag_E"] + [f"feed_ratio_p{i}" for i in legs] + [f"rtp_floor_p{i}" for i in legs]:
            if r.get(k) is None:
                miss.append(f"no-{k}")
        if arm == "WIRE" and not ((r.get("eta") or {}).get("w_calls") or 0) > 0:
            miss.append("w_calls=0")
        e = r.get("eta") or {}
        print(f"SMOKE38 {cell}@{store} {arm}: status={r['status']} mbps={r.get('mbps')} share={r.get('shares')} locked={r.get('locked')} "
              f"w_mv={e.get('w_mv')} w_calls={e.get('w_calls')} w_cold={e.get('w_cold')} {' '.join(r['problems'] + miss)}")
        if miss:
            bad.append(f"{arm}: {miss}")
    if bad:
        print("SMOKE-FAIL " + "; ".join(bad))
        return 1
    print("SMOKE-PASS")
    return 0


def main(argv):
    if not argv:
        print(__doc__)
        return 2
    cmd, a = argv[0], argv[1:]
    if cmd == "row":
        cell, arm, seed, rep, rc, wall, drv, cli, srv = a[:9]
        cot = int(a[9]) if len(a) > 9 else 0
        sha = a[10] if len(a) > 10 else None
        print("P38ROW " + json.dumps(make_row(cell, arm, seed, rep, rc, wall, lc.read(drv), lc.read(cli), lc.read(srv), cot, sha), sort_keys=True))
        return 0
    if cmd == "plan":
        for c, s, ar, seed, rep in plan38(smoke=bool(a) and a[0] == "smoke"):
            print(f"{c} {s} {ar} {seed} {rep}")
        return 0
    if cmd == "plan41":
        for c, s, ar, seed, rep in plan41(smoke=bool(a) and a[0] == "smoke"):
            print(f"{c} {s} {ar} {seed} {rep}")
        return 0
    if cmd == "check":
        return check(a[0])
    if cmd == "check41":
        return check(a[0], plan41=True)
    if cmd == "smoke":
        return smoke(a[0])
    if cmd == "smoke41":
        return smoke(a[0], plan41=True)
    if cmd == "score":
        score(a)
        return 0
    if cmd == "score41":
        score(a, plan41=True)
        return 0
    print(f"unknown command {cmd}")
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
