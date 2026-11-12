#!/usr/bin/env python3
"""Parser and scorer for THE PATH-EVENT TEST (docs/status.md section 46 with amendment 1): does the promoted (wire-clocked)
placement price bring a leg back after a delay step, where the old (app-echo) price keeps it out?

Arms: BASE = main 4926822 (old price); CAND = the merge-candidate build (new price; its SHA is recorded by amendment before launch
and is NOT in this file: the envelope takes it from LD_CAND_SHA and refuses to launch while it is unset or the placeholder).

    place46_parse.py row   <cell> <arm> <seed> <rep> <rc> <wall_s> <drv> <cli> <srv> [cotenant] [sha]
                           env: P46_VARIANT, LD_EVT (injector file), LD_EVTQ (qdisc captures), LD_ONDISK, LD_IO_*, LD_PROCS_*
    place46_parse.py plan [smoke]      `cell store arm seed rep variant` per line (smoke = the two check rows)
    place46_parse.py event <variant>   `<leg>:<delta_ms>` for RWM_EVENT
    place46_parse.py check|smoke <ledger>     smoke = the check rows (qdisc diff, backlog, witnesses); check = the ledger is complete
    place46_parse.py score <ledger>...        the verdict ladder

Readouts (client [DIAG] lines, one per ~250 ms, after the warm-up line), exactly as section 46 + amendment 1:
  * per-window egress = the delta of `dgq<i>[... tx=...]`; hit-leg share = d_hit / (d_0 + d_1); sigma_pre = the pooled share over
    lines with t in [2.0 s, t_on); a line is *idle* on the hit leg when d_hit <= 2 and infl = 0;
  * R (scored): the first 4 consecutive lines that START after t_off (window start = the previous line's t >= t_off) whose pooled
    hit share >= sigma_pre / 2; R = the last one's t - t_off; right-censored at H = 25 s (a censored value ranks above every finite one
    and enters a median as H);
  * gbar_post (scored): the time-weighted mean of `good=` over [t_off, t_off + 25 s] (weights = the gap in t to the previous line);
  * G = 1 - gbar_post / gbar_pre, gbar_pre over [2.0 s, t_on) (reported only); R0 = the first line after t_off with d_hit > 2; T_w =
    the time from t_off until the hit leg's `wrtt` is <= 1.1 x its pre-event median (all censored at H); goodput in the hold.
  * witnesses: the event bites (max hit `wrtt` over [t_on, t_off + 1.5 s] >= pre median + Delta / 2) and the injector's two `tc` calls
    returned 0; the exit witness (some 4 consecutive lines inside [t_on, t_off + 1 s] with pooled share < sigma_pre / 2); the
    `path timed out` / `path recovered` log lines (any, in either log, voids the row).
Interpretations the spec leaves implicit are listed in the harness report.
"""
import json
import math
import os
import re
import statistics
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import l1common as lc  # noqa: E402
import stage3_parse as sp  # noqa: E402
import place38_parse as pp  # noqa: E402

# the two cells of section 46 (500 MB c8 so a one-leg BASE row outlasts the 25 s horizon; 3.5 GB c1d), known to the shared row parser
sp.CELL_SPEC["c8-500"] = ("c2", "c3", "dual", 500000000)
sp.CELL_SPEC["c1d-3500"] = ("c1", "c1", "dual", 3500000000)
sp.N_LEGS["c8-500"] = sp.N_LEGS["c1d-3500"] = 2
for _a in ("BASE", "CAND"):
    sp.ARM_SPEC[_a] = ("window", "Rlc", "bulk", "ACTIVE")

H = 25.0                       # the horizon, s
ONSET_S, HOLD_S = 5.0, 2.0
PRE_FROM = 2.0                 # the pre-event window starts here, s
EVENT_BITE_FRAC = 0.5          # the event bites at half of its Delta (750 ms at the scored 1500 ms, 250 ms at +500)
EXIT_WINDOW = 4
R_FLOOR_S = 1.0
GP_FLOOR_FRAC = 0.04
TW_FACTOR = 1.1
IDLE_TX = 2
BACKLOG_MAX = 900
ABORT_TOKENS = pp.ABORT_TOKENS + ("ABORT-INJECT", "ABORT-CAND", "ABORT-PRECONDITION")
CAND_SHA_PLACEHOLDER = "UNSET"

# variant -> (cell, hit leg index, delta ms, kind)
VARIANTS = {
    "SLOW": ("c8-500", 1, 1500, "scored"),       # leg B, cli1, c3
    "FAST": ("c8-500", 0, 1500, "scored"),       # leg A, cli0, c2
    "SLOW500": ("c8-500", 1, 500, "reported"),   # the ordinary hiccup
    "C1D": ("c1d-3500", 1, 1500, "reported"),    # clean-link case, read on R
}
ARMS46 = ["BASE", "CAND"]
SCORED_REPS = (1, 2)
SEEDS46 = ("42", "7")
CHECK_ROWS = [("c8-500", "def", "BASE", "42", 0, "SLOW"), ("c8-500", "def", "CAND", "42", 0, "FAST")]
EXPECT_ROWS46 = 22             # in the battery ledger (ld.log): 16 scored + 4 at +500 ms + 2 c1d; the 2 check rows are the smoke
EXPECT_TOTAL46 = 24


def plan46(smoke=False):
    """[(cell, store, arm, seed, rep, variant)] in run order.
    Check rows first (the smoke; rep 0, one per binary: BASE runs SLOW-HIT, CAND runs FAST-HIT), then the 16 scored rows rep-major
    (rep, then seed 42 before 7, then SLOW-HIT before FAST-HIT; each (rep, seed, variant) block runs both arms, CAND first on odd
    blocks), then the 4 rows at +500 ms (SLOW-HIT, seeds 42 and 7), then the 2 c1d rows (seed 42). The block parity restarts at 0
    for the +500 and c1d sections (a DECISION the spec does not state)."""
    if smoke:
        return list(CHECK_ROWS)
    out = []
    b = 0
    for rep in SCORED_REPS:
        for seed in SEEDS46:
            for var in ("SLOW", "FAST"):
                cell = VARIANTS[var][0]
                for arm in (ARMS46[::-1] if b % 2 else ARMS46):
                    out.append((cell, "def", arm, seed, rep, var))
                b += 1
    for b2, seed in enumerate(SEEDS46):
        for arm in (ARMS46[::-1] if b2 % 2 else ARMS46):
            out.append((VARIANTS["SLOW500"][0], "def", arm, seed, 1, "SLOW500"))
    for arm in ARMS46:
        out.append((VARIANTS["C1D"][0], "def", arm, "42", 1, "C1D"))
    return out


# ── the series ────────────────────────────────────────────────────────────
_T = re.compile(r"^\[DIAG\] t=([\d.]+)s ")
_GOOD = re.compile(r"\bgood=([\d.]+)Mbit")
_DGQ = re.compile(r"\bdgq(\d+)\[hand=\d+ tx=(\d+)")


def series(cli):
    """The client's [DIAG] lines AFTER the warm-up line -> [{t, good, tx{i}, infl{i}, rtt{i}, wrtt{i}, wpl{i}}]."""
    lines = list(cli or [])
    idx = [i for i, ln in enumerate(lines) if '"warmup":true' in ln]
    if idx:
        lines = lines[idx[-1] + 1:]
    out = []
    for ln in lines:
        m = _T.match(ln)
        if not m:
            continue
        legs = pp.diag_lines([ln])
        if not legs:
            continue
        g = _GOOD.search(ln)
        d = {"t": float(m.group(1)), "good": float(g.group(1)) if g else None, "tx": {}, "infl": {}, "rtt": {}, "wrtt": {}, "wpl": {}}
        for q in _DGQ.finditer(ln):
            d["tx"][int(q.group(1))] = int(q.group(2))
        for i, (infl, rtt, wrtt, wpl) in legs[0][1].items():
            d["infl"][i], d["rtt"][i], d["wrtt"][i], d["wpl"][i] = infl, rtt, wrtt, wpl
        out.append(d)
    return out


def med(v):
    v = [x for x in v if x is not None]
    return lc.med(v) if v else None


def _windows(S, nleg=2):
    """Per line k >= 1: {t0, t, d[i]} (the per-leg egress delta over (t0, t])."""
    rows = []
    for k in range(1, len(S)):
        a, b = S[k - 1], S[k]
        if any(i not in a["tx"] or i not in b["tx"] for i in range(nleg)):
            continue
        rows.append({"k": k, "t0": a["t"], "t": b["t"], "d": [max(0, b["tx"][i] - a["tx"][i]) for i in range(nleg)]})
    return rows


def _gbar(S, lo, hi, hi_incl=True):
    num = den = 0.0
    for k in range(1, len(S)):
        t, g = S[k]["t"], S[k]["good"]
        if g is None or t < lo or (t > hi if hi_incl else t >= hi):
            continue
        w = t - S[k - 1]["t"]
        num, den = num + w * g, den + w
    return (num / den) if den > 0 else None


def analyze(S, t_on, t_off, hit, nleg=2, delta_ms=1500):
    """All of section 46's per-row readouts from a series and the event's [DIAG]-axis times. Pure; the scorer's input."""
    W = _windows(S, nleg)
    tot = lambda r: sum(r["d"])  # noqa: E731
    pre = [r for r in W if PRE_FROM <= r["t"] < t_on]
    sd = sum(r["d"][hit] for r in pre)
    st = sum(tot(r) for r in pre)
    sigma = (sd / st) if st > 0 else None
    pre_wrtt = med([s["wrtt"].get(hit) for s in S if PRE_FROM <= s["t"] < t_on])
    m = {"sigma_pre": sigma, "pre_wrtt": pre_wrtt, "t_on": t_on, "t_off": t_off, "hit": hit, "n_lines": len(S)}

    def pooled(ws):
        n = sum(tot(r) for r in ws)
        return (sum(r["d"][hit] for r in ws) / n) if n > 0 else None

    # R: first 4 consecutive lines that start at or after t_off with pooled hit share >= sigma/2
    R = None
    if sigma is not None:
        for j in range(len(W) - EXIT_WINDOW + 1):
            ws = W[j:j + EXIT_WINDOW]
            if ws[0]["t0"] < t_off:
                continue
            sh = pooled(ws)
            if sh is not None and sh >= 0.5 * sigma:
                R = ws[-1]["t"] - t_off
                break
    m["R"], m["R_cens"] = (R, False) if (R is not None and R <= H) else (H, True)
    # R0: the first line starting at or after t_off with d_hit > 2
    R0 = next((r["t"] - t_off for r in W if r["t0"] >= t_off and r["d"][hit] > IDLE_TX), None)
    m["R0"], m["R0_cens"] = (R0, False) if (R0 is not None and R0 <= H) else (H, True)
    # T_w
    Tw = None
    if pre_wrtt is not None:
        Tw = next((s["t"] - t_off for s in S if s["t"] >= t_off and s["wrtt"].get(hit) is not None
                   and s["wrtt"][hit] <= TW_FACTOR * pre_wrtt), None)
    m["T_w"], m["T_w_cens"] = (Tw, False) if (Tw is not None and Tw <= H) else (H, True)
    # goodput
    m["g_pre"], m["g_hold"], m["g_post"] = _gbar(S, PRE_FROM, t_on, False), _gbar(S, t_on, t_off), _gbar(S, t_off, t_off + H)
    m["G"] = (1 - m["g_post"] / m["g_pre"]) if (m["g_post"] is not None and m["g_pre"]) else None
    m["short_horizon"] = bool(S) and S[-1]["t"] < t_off + H - 0.5
    # witnesses
    peak = max([s["wrtt"].get(hit) for s in S if t_on <= s["t"] <= t_off + 1.5 and s["wrtt"].get(hit) is not None] or [None], key=lambda x: -1 if x is None else x)
    m["event_peak_wrtt"] = peak
    m["event_ok"] = bool(peak is not None and pre_wrtt is not None and peak >= pre_wrtt + EVENT_BITE_FRAC * delta_ms)
    ex = False
    if sigma is not None:
        for j in range(len(W) - EXIT_WINDOW + 1):
            ws = W[j:j + EXIT_WINDOW]
            if ws[0]["t0"] >= t_on and ws[-1]["t"] <= t_off + 1.0:
                sh = pooled(ws)
                if sh is not None and sh < 0.5 * sigma:
                    ex = True
                    break
    m["exit_ok"] = ex
    hold_lines = [s for s in S if t_on <= s["t"] <= t_off + H]
    m["peak_rtt"] = max([s["rtt"].get(hit) for s in hold_lines if s["rtt"].get(hit) is not None] or [None], key=lambda x: -1 if x is None else x)
    m["peak_wpl_us"] = max([s["wpl"].get(hit) for s in hold_lines if s["wpl"].get(hit) is not None] or [None], key=lambda x: -1 if x is None else x)
    # mechanism witnesses (reported): idle hit-leg lines after t_off, before R0
    lim = (t_off + R0) if R0 is not None else t_off + H
    idle = [r for r in W if r["t0"] >= t_off and r["t"] <= lim and r["d"][hit] <= IDLE_TX and S[r["k"]]["infl"].get(hit) == 0]
    iv = [S[r["k"]] for r in idle]
    rtts = [s["rtt"][hit] for s in iv if s["rtt"].get(hit) is not None]
    wrs = [s["wrtt"][hit] for s in iv if s["wrtt"].get(hit) is not None]
    wps = [s["wpl"][hit] for s in iv if s["wpl"].get(hit) is not None]
    m["idle_lines"] = len(iv)
    m["idle_rtt_range"] = (max(rtts) - min(rtts)) if rtts else None
    m["idle_wrtt_first"], m["idle_wrtt_last"] = (wrs[0], wrs[-1]) if wrs else (None, None)
    m["idle_rtt_frozen"] = bool(len(rtts) >= 2 and max(rtts) - min(rtts) <= 1 and wrs and wrs[-1] < wrs[0])
    m["wpl_numeric_all"] = all(isinstance(s["wpl"].get(i), int) for s in S for i in range(nleg) if i in s["wpl"]) and bool(S)
    m["wpl_steps_before_R0"] = sum(1 for a, b in zip(wps, wps[1:]) if b < a - 1000)
    chg = sum(1 for a, b in zip(wrs, wrs[1:]) if a != b)
    dur = (iv[-1]["t"] - iv[0]["t"]) if len(iv) >= 2 else 0.0
    m["wrtt_changes"], m["idle_span_s"] = chg, dur
    m["refresh_per_2s"] = (chg / (dur / 2.0)) if dur > 0 else None
    at_off = next((s for s in S if s["t"] >= t_off), None)
    at_r0 = next((s for s in S if R0 is not None and s["t"] >= t_off + R0), None)
    for tag, s in (("at_off", at_off), ("at_R0", at_r0)):
        m[tag] = None if s is None else {"t": s["t"], "rtt": s["rtt"].get(hit), "wrtt": s["wrtt"].get(hit), "wpl_us": s["wpl"].get(hit)}
    return m


# ── the injector's file and the qdisc captures ────────────────────────────
def parse_evt(text):
    e = {"start": None, "on": None, "off": None, "done": False, "abort": None, "stamps": []}
    for ln in (text or "").splitlines():
        if ln.startswith("[EVENT] start"):
            e["start"] = float(re.search(r"wall=([\d.]+)", ln).group(1))
        elif ln.startswith("[EVENT] on ") or ln.startswith("[EVENT] off "):
            kv = dict(re.findall(r"(\w+)=(\S+)", ln))
            d = {"before": float(kv["wall_before"]), "after": float(kv["wall_after"]), "rc": int(kv["rc"]), "delay_ms": int(kv["delay_ms"])}
            e["on" if ln.startswith("[EVENT] on ") else "off"] = d
        elif ln.startswith("[EVENT] done"):
            e["done"] = True
        elif ln.startswith("[EVENT] ABORT"):
            e["abort"] = ln[len("[EVENT] ABORT"):].strip()
        elif ln.startswith("[DIAGSEEN]"):
            m = re.search(r"t=([\d.]+) wall=([\d.]+)", ln)
            if m:
                e["stamps"].append((float(m.group(1)), float(m.group(2))))
    return e


def event_axis(e):
    """-> (t_on, t_off) on the [DIAG] axis, or None: offset = median(wall_seen - t_printed); the event time is the middle of the tc call."""
    if not (e["on"] and e["off"] and e["stamps"]):
        return None
    off = statistics.median(w - t for t, w in e["stamps"])
    mid = lambda d: (d["before"] + d["after"]) / 2  # noqa: E731
    return mid(e["on"]) - off, mid(e["off"]) - off


_DELAY = re.compile(r"\bdelay ([\d.]+)(us|ms|s)\b")


def _ms(v, u):
    return float(v) * {"us": 0.001, "ms": 1.0, "s": 1000.0}[u]


def _tc_tol_ms(v_ms):
    """tc prints netem delays with "%.3g" (iproute2 sprint_time64), so 1505 ms reads back as 1.5s: allow half a unit
    of the third significant digit, and never less than 1 ms."""
    return max(1.0, 0.5 * 10 ** (math.floor(math.log10(v_ms)) - 2)) if v_ms > 0 else 1.0


def qdisc_check(text, delta_ms):
    """The check rows' qdisc diff (section 46): apart from counters the `during` netem line differs from `before` only in `delay`
    (+delta), `after` equals `before`, and the backlog peaks below 900 packets. -> dict(ok, notes, peak_backlog, base_delay_ms)"""
    blocks = {}
    for part in re.split(r"^### ", text or "", flags=re.M)[1:]:
        head, _, body = part.partition("\n")
        blocks.setdefault(head.split()[0], []).append(body)

    def netem_line(body):
        ls = [ln.strip() for ln in body.splitlines() if ln.startswith("qdisc netem")]
        return re.sub(r"\s+", " ", ls[0]) if ls else None

    def backlog(body):
        m = re.search(r"backlog \d+b (\d+)p", body)
        return int(m.group(1)) if m else None

    notes, ok = [], True
    if not (blocks.get("before") and blocks.get("during") and blocks.get("after")):
        return {"ok": False, "notes": ["missing before/during/after captures"], "peak_backlog": None, "base_delay_ms": None}
    b0 = netem_line(blocks["before"][-1])
    aft = netem_line(blocks["after"][-1])
    if b0 is None:
        return {"ok": False, "notes": ["no netem qdisc line in the before capture"], "peak_backlog": None, "base_delay_ms": None}
    mb = _DELAY.search(b0)
    base = _ms(*mb.groups()) if mb else None
    norm = lambda ln: _DELAY.sub("delay X", ln)  # noqa: E731
    for i, body in enumerate(blocks["during"]):
        ln = netem_line(body)
        if ln is None or norm(ln) != norm(b0):
            ok = False
            notes.append(f"during[{i}] differs from before beyond delay")
            break
        md = _DELAY.search(ln)
        if base is None or not md or abs(_ms(*md.groups()) - (base + delta_ms)) > _tc_tol_ms(base + delta_ms):
            ok = False
            notes.append(f"during[{i}] delay is not before + {delta_ms} ms")
            break
    if aft != b0:
        ok = False
        notes.append("after != before")
    bl = [backlog(b) for k in ("before", "during", "after") for b in blocks[k]]
    bl = [x for x in bl if x is not None]
    peak = max(bl) if bl else None
    if peak is None or peak >= BACKLOG_MAX:
        ok = False
        notes.append(f"backlog peak {peak} (need < {BACKLOG_MAX})")
    return {"ok": ok, "notes": notes, "peak_backlog": peak, "base_delay_ms": base}


# ── ROW ───────────────────────────────────────────────────────────────────
def count_path_events(lines):
    n_in = sum(1 for ln in lines if "path timed out" in ln and "marking inactive" in ln)
    n_rec = sum(1 for ln in lines if "path recovered" in ln and "marking active" in ln)
    return n_in, n_rec


def make_row(cell, arm, seed, rep, rc, wall_s, drv, cli, srv, cotenant=0, sha=None, variant=None, evt_text=None, evtq_text=None):
    variant = variant or os.environ.get("P46_VARIANT", "SLOW")
    vcell, hit, delta, kind = VARIANTS[variant]
    row = pp.make_row(cell, arm, seed, rep, rc, wall_s, drv, cli, srv, cotenant, sha, store="def")
    row["variant"], row["kind"] = variant, ("check" if int(rep) == 0 else kind)
    row["hit_leg"], row["delta_ms"] = hit, delta
    p = []
    e = parse_evt(evt_text)
    row["evt"] = {k: e[k] for k in ("start", "on", "off", "done", "abort")}
    inj_ok = bool(e["on"] and e["off"] and e["on"]["rc"] == 0 and e["off"]["rc"] == 0 and e["abort"] is None)
    row["inject_ok"] = inj_ok
    if not inj_ok:
        p.append("inject-failed" + (f"({e['abort']})" if e["abort"] else ""))
    S = series(cli)
    ax = event_axis(e)
    m = None
    if ax is None:
        p.append("no-event-axis")
    else:
        row["t_on"], row["t_off"] = ax
        m = analyze(S, ax[0], ax[1], hit, delta_ms=delta)
        if m["sigma_pre"] is None:
            p.append("no-pre-event-share")
        if not m["event_ok"]:
            p.append(f"event-did-not-bite(peak wrtt {m['event_peak_wrtt']} vs pre {m['pre_wrtt']} + {EVENT_BITE_FRAC * delta:g})")
    row["m"] = m
    n_in_c, n_rec_c = count_path_events(cli)
    n_in_s, n_rec_s = count_path_events(srv)
    row["path_inactive"], row["path_recovered"] = n_in_c + n_in_s, n_rec_c + n_rec_s
    if row["path_inactive"] or row["path_recovered"]:
        p.append(f"path-inactive/recovered={row['path_inactive']}/{row['path_recovered']}")
    if evtq_text is not None:
        row["qdisc"] = qdisc_check(evtq_text, delta)
    row["problems"].extend(p)
    if p and row["status"] == "LIVE":
        row["status"] = "WITNESS-FAIL"
    return row


# ── LEDGER ────────────────────────────────────────────────────────────────
def rows_of(paths):
    rows, tokens = [], []
    for path in paths:
        for ln in lc.read(path):
            if ln.startswith("P46ROW "):
                try:
                    rows.append(json.loads(ln[len("P46ROW "):]))
                except ValueError:
                    tokens.append("MALFORMED-P46ROW")
            for t in ABORT_TOKENS:
                if ln.startswith(t):
                    tokens.append(t)
    return rows, tokens


# ── the disjoint rule ─────────────────────────────────────────────────────
def comb(n, k):
    return math.comb(n, k)


def rule(cand, base, better_low, floor, med_unit=1.0):
    """CAND against BASE by section 46's noise rule: BETTER iff every CAND value is on the better side of every BASE value (ties,
    including two censored values, break disjointness) AND the medians differ by more than the floor; WORSE is the mirror image.
    `cand` / `base` are lists of (value, censored). -> (reading, detail incl. the one-sided permutation p of the disjoint split)"""
    if not cand or not base:
        return "UNREAD", "no values"
    key = lambda x: (x[0], 1 if x[1] else 0)  # noqa: E731
    cv, bv = sorted(cand, key=key), sorted(base, key=key)
    mc, mb = statistics.median([x[0] for x in cand]), statistics.median([x[0] for x in base])
    p = 1.0 / comb(len(cand) + len(base), len(cand))
    lo_c, hi_c, lo_b, hi_b = key(cv[0]), key(cv[-1]), key(bv[0]), key(bv[-1])
    cand_below = hi_c < lo_b           # every CAND strictly below every BASE
    cand_above = lo_c > hi_b
    if better_low:
        better, worse = cand_below and (mb - mc) > floor, cand_above and (mc - mb) > floor
    else:
        better, worse = cand_above and (mc - mb) > floor, cand_below and (mb - mc) > floor
    det = (f"CAND {[round(x[0], 2) if not x[1] else '>25' for x in cand]} median {mc:.2f} vs BASE "
           f"{[round(x[0], 2) if not x[1] else '>25' for x in base]} median {mb:.2f}; floor {floor:.2f}; p(disjoint)={p:.3f} (n={len(cand)} vs {len(base)})")
    return ("BETTER" if better else "WORSE" if worse else "WITHIN"), det


def live_rows(rows, variant, arm, kinds=("scored",)):
    return [r for r in rows if r.get("variant") == variant and r["arm"] == arm and r.get("kind") in kinds and r["status"] == "LIVE"]


def fmtf(x, nd=2):
    return "-" if x is None else f"{x:.{nd}f}"


def row_line(r):
    m = r.get("m") or {}
    R = ">25" if m.get("R_cens") else fmtf(m.get("R"))
    R0 = ">25" if m.get("R0_cens") else fmtf(m.get("R0"))
    Tw = ">25" if m.get("T_w_cens") else fmtf(m.get("T_w"))
    return (f"  {r['variant']:<7} {r['arm']:<4} s{r['seed']} r{r['rep']} {r['status']:<12} R={R} R0={R0} T_w={Tw} gpost={fmtf(m.get('g_post'), 1)} "
            f"G={fmtf(None if m.get('G') is None else 100 * m['G'], 1)}% ghold={fmtf(m.get('g_hold'), 1)} sigma={fmtf(m.get('sigma_pre'), 3)} "
            f"event={'Y' if m.get('event_ok') else 'N'} exit={'Y' if m.get('exit_ok') else 'N'} "
            f"peak rtt/wpl/wrtt={m.get('peak_rtt')}/{m.get('peak_wpl_us')}/{m.get('event_peak_wrtt')} idle_lines={m.get('idle_lines')} "
            f"refresh/2s={fmtf(m.get('refresh_per_2s'), 2)} " + ' '.join(r.get('problems') or []))


def qcwnd_line(rs):
    """Reported witness (status 47): the share of [DIAG] lines whose per-path qcwnd is below the cell's BDP -> text for a set of rows."""
    rs = [r for r in rs if r.get("qcwnd_below")]
    if not rs:
        return "-"
    nl = len(rs[0]["qcwnd_below"]["per_leg"])
    legs = " ".join(f"leg{i} {pp.med_mm([r['qcwnd_below']['per_leg'][i] for r in rs], 3)}" for i in range(nl))
    return f"{legs}; any leg {pp.med_mm([r['qcwnd_below']['any'] for r in rs], 3)}"


def score(paths, out=print):
    rows, tokens = rows_of(paths)
    aborted = sorted({t for t in tokens if t in ABORT_TOKENS})
    scored = [r for r in rows if r.get("kind") == "scored"]
    out("PATH-EVENT TEST (status section 46 + amendment 1): BASE = main 4926822 (old price), CAND = the merge-candidate build (new price)")
    out(f"ROWS {len(rows)} (scored {len(scored)}, +500 ms {sum(1 for r in rows if r.get('kind') == 'reported' and r['variant'] == 'SLOW500')}, "
        f"c1d {sum(1 for r in rows if r.get('variant') == 'C1D')}, check {sum(1 for r in rows if r.get('kind') == 'check')}); ABORT-TOKENS {' '.join(aborted) or 'none'}")
    out("STATUS-TABLE (every invocation row)")
    for st in ("LIVE", "NO_DATA", "VOID-RC", "VOID-COTENANT", "VOID-ONDISK", "CONTAMINATED", "WITNESS-FAIL"):
        out(f"  {st:<13} {sum(1 for r in rows if r['status'] == st)}")
    for r in rows:
        if r["status"] != "LIVE":
            out(f"  NONLIVE {r['variant']} {r['arm']} s{r['seed']} rep{r['rep']} {r['status']} rc={r['rc']} {' '.join(r['problems'])}")
    out("")
    out("PER-ROW (variant arm seed rep status R R0 T_w gbar_post G gbar_hold sigma_pre event exit peaks idle refresh): R/R0/T_w in s from t_off, '>25' = censored")
    for r in sorted(rows, key=lambda r: (r.get("kind") != "check", r.get("variant", ""), r["arm"], r["seed"], r["rep"])):
        out(row_line(r))
    out("")
    out("QCWND-BELOW-BDP (reported only): share of [DIAG] lines (measured run, t >= 1 s) whose per-path qcwnd < the cell's BDP; "
        "BDP per leg = link x 2 x one-way delay (c8: leg 0 = c2 125 000 B, leg 1 = c3 100 000 B; c1d: 250 000 B per leg)")
    for var in VARIANTS:
        for arm in ARMS46:
            rs = [r for r in rows if r.get("variant") == var and r["arm"] == arm and r["status"] == "LIVE" and r.get("kind") != "check"]
            if rs:
                out(f"  {var:<7} {arm:<4} BDP {pp.bdp_bytes(VARIANTS[var][0])} B: {qcwnd_line(rs)}")
    out("")
    # check rows
    check_fail = []
    for r in [r for r in rows if r.get("kind") == "check"]:
        q = r.get("qdisc") or {}
        out(f"CHECK-ROW {r['variant']} {r['arm']}: status={r['status']} qdisc_ok={q.get('ok')} backlog_peak={q.get('peak_backlog')} {' '.join(q.get('notes') or [])}")
        if r["status"] != "LIVE" or not q.get("ok"):
            check_fail.append(f"{r['arm']}/{r['variant']}")
    # per variant
    res = {"variants": {}}
    unsc = []
    calls = {}
    provoked = {}
    for var in ("SLOW", "FAST"):
        C, B = live_rows(rows, var, "CAND"), live_rows(rows, var, "BASE")
        out(f"[{var}]  live CAND={len(C)} BASE={len(B)}")
        if len(C) < 3 or len(B) < 3:
            unsc.append(var)
        Rr = rule([(r["m"]["R"], r["m"]["R_cens"]) for r in C], [(r["m"]["R"], r["m"]["R_cens"]) for r in B], True, R_FLOOR_S)
        bmed = statistics.median([r["m"]["g_post"] for r in B if r["m"]["g_post"] is not None]) if B else 0.0
        Gp = rule([(r["m"]["g_post"], False) for r in C if r["m"]["g_post"] is not None], [(r["m"]["g_post"], False) for r in B if r["m"]["g_post"] is not None],
                  False, GP_FLOOR_FRAC * bmed)
        out(f"  R (s from t_off, scored, lower is better): {Rr[0]}  {Rr[1]}")
        out(f"  gbar_post (Mbit/s over [t_off, t_off+25 s], scored, higher is better): {Gp[0]}  {Gp[1]}")
        for lab, key in (("G (%, reported)", "G"), ("R0 (s, reported)", "R0"), ("T_w (s, reported)", "T_w")):
            f = lambda rs: [(100 * r["m"][key] if key == "G" else r["m"][key]) for r in rs if r["m"].get(key) is not None]  # noqa: E731
            out(f"  {lab}: CAND median {fmtf(med(f(C)))} {['%.1f' % x for x in f(C)]}; BASE median {fmtf(med(f(B)))} {['%.1f' % x for x in f(B)]}")
        n_exit = sum(1 for r in B if r["m"]["exit_ok"])
        provoked[var] = n_exit >= 3
        out(f"  exit witness on BASE live rows: {n_exit}/{len(B)} -> {'PROVOKED' if provoked[var] else 'NOT provoked'}")
        calls[var] = {"R": Rr[0], "gpost": Gp[0]}
        res["variants"][var] = {"R": Rr[0], "gpost": Gp[0], "provoked": provoked[var], "live": [len(C), len(B)]}
    out("")
    # reported-only sections
    for var, title in (("SLOW500", "+500 ms at SLOW-HIT (reported only)"), ("C1D", "c1d, leg 1 hit (reported only, read on R)")):
        C, B = live_rows(rows, var, "CAND", ("reported",)), live_rows(rows, var, "BASE", ("reported",))
        out(f"[{title}]  live CAND={len(C)} BASE={len(B)}")
        for r in sorted(C + B, key=lambda r: (r["arm"], r["seed"])):
            out(row_line(r))
        if C and B:
            Rr = rule([(r["m"]["R"], r["m"]["R_cens"]) for r in C], [(r["m"]["R"], r["m"]["R_cens"]) for r in B], True, R_FLOOR_S)
            out(f"  R reading (not scored): {Rr[0]}  {Rr[1]}")
    out("")
    # the ladder
    verdict = None
    if aborted or check_fail or unsc:
        verdict = "UNSCOREABLE (" + "; ".join(([f"abort {','.join(aborted)}"] if aborted else []) + ([f"check row failed: {','.join(check_fail)}"] if check_fail else [])
                                              + ([f"< 3 live rows in either arm at {','.join(unsc)}"] if unsc else [])) + ")"
    if verdict is None:
        worse = [f"{v} {k}" for v in calls for k, c in calls[v].items() if c == "WORSE"]
        if worse:
            verdict = "CAND-WORSE (" + "; ".join(worse) + ")"
    if verdict is None:
        r_better = [v for v in calls if provoked[v] and calls[v]["R"] == "BETTER"]
        if not provoked["SLOW"] and not r_better:
            verdict = "NOT-PROVOKED"
        elif r_better:
            verdict = "CAND-RECOVERS-FASTER (" + ",".join(r_better) + ")"
        else:
            verdict = "NO-DIFFERENCE"
    out("VERDICT " + verdict)
    res["verdict"] = verdict
    out("RESULT " + json.dumps(res, sort_keys=True, default=str))
    return verdict


def check(path):
    rows, _ = rows_of([path])
    t = open(path, errors="replace").read()
    ok = "LD-BATTERY-DONE" in t and len(rows) == EXPECT_ROWS46
    print(f"CHECK46 done={'LD-BATTERY-DONE' in t} rows={len(rows)}/{EXPECT_ROWS46}")
    return 0 if ok else 1


def smoke(path):
    """The check rows (BASE at SLOW-HIT, CAND at FAST-HIT): LIVE (event witness, no inactive / recovered lines, injector rc 0), the
    qdisc diff and backlog pass, and BASE's exit witness holds at SLOW-HIT. Prints the row's witnesses. rc 0 iff all pass."""
    rows, _ = rows_of([path])
    bad = []
    got = {(r["arm"], r["variant"]): r for r in rows if r.get("kind") == "check"}
    for cell, store, arm, seed, rep, var in CHECK_ROWS:
        r = got.get((arm, var))
        if r is None:
            bad.append(f"{arm}/{var}: no row")
            continue
        m, q = r.get("m") or {}, r.get("qdisc") or {}
        miss = [] if r["status"] == "LIVE" else [r["status"]]
        if not q.get("ok"):
            miss.append("qdisc:" + ";".join(q.get("notes") or ["none"]))
        if arm == "BASE" and var == "SLOW" and not m.get("exit_ok"):
            miss.append("exit-witness-failed-on-BASE-SLOW")
        print(f"SMOKE46 {arm} {var}: status={r['status']} event_witness={m.get('event_ok')} exit_witness={m.get('exit_ok')} "
              f"peak_rtt={m.get('peak_rtt')} peak_wpl_us={m.get('peak_wpl_us')} peak_wrtt={m.get('event_peak_wrtt')} "
              f"inactive={r.get('path_inactive')} recovered={r.get('path_recovered')} refresh_per_2s={fmtf(m.get('refresh_per_2s'))} "
              f"idle_lines={m.get('idle_lines')} qdisc_ok={q.get('ok')} backlog_peak={q.get('peak_backlog')} inject_ok={r.get('inject_ok')} "
              f"qcwnd_below_bdp={qcwnd_line([r])} "
              f"{' '.join(r.get('problems') or [])}")
        if miss:
            bad.append(f"{arm}/{var}: {miss}")
    if bad:
        print("SMOKE-FAIL " + "; ".join(bad))
        print("ABORT-INJECT")
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

        def rd(env):
            pth = os.environ.get(env)
            return "\n".join(lc.read(pth)) if pth and os.path.exists(pth) else None
        row = make_row(cell, arm, seed, rep, rc, wall, lc.read(drv), lc.read(cli), lc.read(srv), cot, sha, evt_text=rd("LD_EVT"), evtq_text=rd("LD_EVTQ"))
        if os.environ.get("LD_ONDISK", "0") == "1":
            row["status"] = "VOID-ONDISK"
        print("P46ROW " + json.dumps(row, sort_keys=True, default=str))
        if not row["inject_ok"]:
            print(f"ABORT-INJECT {cell} {arm} s{seed} r{rep} {row['variant']}: {' '.join(row['problems'])}")
        return 0
    if cmd == "plan":
        for c, s, ar, seed, rep, var in plan46(smoke=bool(a) and a[0] == "smoke"):
            print(f"{c} {s} {ar} {seed} {rep} {var}")
        return 0
    if cmd == "event":
        _, leg, delta, _ = VARIANTS[a[0]]
        print(f"{leg}:{delta}")
        return 0
    if cmd == "check":
        return check(a[0])
    if cmd == "smoke":
        return smoke(a[0])
    if cmd == "score":
        score(a)
        return 0
    print(f"unknown command {cmd}")
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
