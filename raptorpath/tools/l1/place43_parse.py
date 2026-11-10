#!/usr/bin/env python3
"""Parser and scorer for THE ORACLE STORE LAW IN CLOSED LOOP (docs/status.md §43 with amendment 1; paper §6.1 "The
closed-loop oracle arm"): `RWM_STORE_LAW=oracle` started low (SL) and started high (SH, `RWM_ORACLE_HOLD=<H>:15`)
against the shipped law with the same sampler as a readout (MAIN), at c2 (100 Mbit/s) and c1s (1 Gbit/s). Diagnostic;
nothing here ships.

    place43_parse.py row    <cell> <arm> <seed> <rep> <rc> <wall_s> <drv> <cli> <srv> [cotenant] [sha]
    place43_parse.py check|smoke <ledger>
    place43_parse.py score  <ledger>...
    place43_parse.py plan   [smoke]                 (`cell arm seed rep` per line; the battery's source)
    place43_parse.py dryrun <ledger-dir>...          (the scorer's rules on committed [LOOPDIAG] ledger rows)

The rules R1-R10, the per-row labels and the cell ladder are §43's text, one constant or function each. The rules
read ONE neutral per-window series (`Win`). Two readers fill it: `ocap_series` (the `[OCAP]` lines of a live row) and
`ledger_series` (a ledger row that has no `[OCAP]`: the `[DIAG] win=` denominator is n, `[LOOPDIAG]` is the sampler,
the binds are n against the floor and N*knee). The second exists only for the dry-run.

Interpretations the spec leaves open (listed in the harness report; none changes what is measured):
  * "scored" = LIVE, not SHORT, and (SH rows) HOLD-EXPIRED. Every outcome (UNSCOREABLE, UNSTABLE, ñ_SL, the ladder, the
    GOODPUT / DELAY comparisons) reads the scored rows only: SHORT and HOLD-INERT rows are "left out of the outcome".
    A left-out row that would have shown COLLAPSE / OSCILLATION / RUNAWAY is printed (`EXCLUDED-WOULD-TRIGGER`).
  * sd(ln n) and the 2*sigma_pooled comparisons use the sample sd (n-1); sigma_pooled = sqrt(((n1-1)s1^2+(n2-1)s2^2)
    /(n1+n2-2)); the difference is of the arms' means; the guard needs 2 live rows per arm.
  * a bind fraction of a row is the mean over its windows of the per-window fraction of refreshes at which the bound
    bound (`wbind` is 0/1 per window); "over its windows after 5 s" = windows with t >= 5 s.
  * RTprop(t) is the `rtprop_us` of the last window with window time <= t (RTprop(1 s): the first window with t >= 1 s);
    "the hold's last 2 s" = windows with t_h-2 < t <= t_h.
  * the "third"s of R4 / R7 / R8 / R9 are the row's time tertiles (t < t_last/3, < 2t_last/3, the rest).
  * amendment 2: COLLAPSE is the DNF row only (the floor clause is struck: the law cannot reach the floor after Copa's first sample);
    at c1s with >= 2 HOLD-INERT SH rows the scored SL rows carry the cell (label note `SL-ONLY`, SH cannot test bistability).
  * DNF: perf prints `dnf` in the run JSON and exits 0, so rc 0 with `dnf` is a DNF row; any rc != 0 is VOID-RC
    (the battery's ABORT-RC), as before.
"""
import json
import math
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import l1common as lc  # noqa: E402
import stage3_parse as sp  # noqa: E402
import storelaw_parse as sl  # noqa: E402  (regexes only)

ARMS = ["MAIN", "SL", "SH"]
# Amendment 2: two reported-only start-low arms at c2 (they never enter the outcome ladder).
EXTRA_ARMS = ["SL-B0", "SL-G12"]
EXTRA_CELLS = ["c2-100"]
EXTRA_ENV = {"SL-B0": ("2470", "q"), "SL-G12": ("0", "1.2")}     # (b0_us, gain) the run's echo must declare
PRED_EXTRA = {"SL-B0": 195.0, "SL-G12": 184.0}                   # amendment 2's offline-map operating points (reported against)
for _a in ARMS + EXTRA_ARMS:
    sp.ARM_SPEC[_a] = ("window", "Rlc", "bulk", "ACTIVE")

# ── §43's constants ──────────────────────────────────────────────────────
TW = 0.25                    # the window, s
WARM_S = 1.0                 # windows with t >= 1 s are scored
T_H = 15.0                   # the hold's end, s
K = 2.0                      # |ln| tolerance in sd_pred units (RETURNED, d(n))
OSC_MULT = 3.0               # OSCILLATION: s_end > 3 sd_pred
BIND_MIN = 0.25              # RUNAWAY: pin >= 0.25
MIN_SCORED = 2               # scored rows per arm for an outcome
SHORT_S = 30.0               # SL / MAIN under 30 s; SH under T_H + 30
CONSEC = 4                   # t_ret: 4 consecutive windows
LOOPDIAG_CAP = 8192          # what the [LOOPDIAG] cap would have cut
FEW = 8                      # R10: windows with fewer than 8 samples
FLOOR_N = 10
KNEE = 2048
ROUTE_MIN = 0.99             # routing witness: win= denominator == [OCAP] n on >= 99 % of ticks
CELLS = {
    "c2-100": {"L": 9773, "H": 1536, "band": (157.0, 164.0), "sd": 0.08, "bytes": 600_000_000, "nleg": 1},
    "c1s-400": {"L": 97733, "H": 2048, "band": (614.0, 699.0), "sd": 0.11, "bytes": 5_000_000_000, "nleg": 1},
}
CELL_ORDER = ["c2-100", "c1s-400"]
BLOCKS = [("42", 1), ("7", 1), ("42", 2)]
ARM_ROT = [["MAIN", "SL", "SH"], ["SL", "SH", "MAIN"], ["SH", "MAIN", "SL"]]
SMOKE = [("c2-100", "SH", "42", 1), ("c1s-400", "SL", "42", 1)]
EXPECT_ROWS = len(BLOCKS) * (len(CELL_ORDER) * len(ARMS) + len(EXTRA_ARMS))     # 24 (+ the 2 smoke rows = 26 invocations)
ABORT_TOKENS = ("ABORT-LOCK", "ABORT-CRLF", "ABORT-SHA", "ABORT-BIN", "ABORT-SENTINEL-UNWRITABLE", "ABORT-SMOKE",
                "ABORT-BUILD", "ABORT-TESTS", "ABORT-BRINGUP", "ABORT-BUDGET")
LAW_ECHO = "oracle store law ACTIVE"
HOLD_RE = re.compile(r"hold=H=(\d+) t_h=([\d.]+)s")
LIST_RE = re.compile(r"L_dgps=\[([^\]]*)\]")
B0_RE = re.compile(r"b0_us=(\S+) gain=(\S+)")


def plan43(smoke=False):
    """[(cell, arm, seed, rep)] in run order: three blocks; the cells alternate (c2 first on even blocks); the arm order
    rotates per block (MAIN SL SH / SL SH MAIN / SH MAIN SL). At c2 the two extra arms (SL-B0, SL-G12) are interleaved:
    the five-arm list rotates by the block number, which keeps the three main arms in the rotation above."""
    if smoke:
        return list(SMOKE)
    out = []
    for b, (seed, rep) in enumerate(BLOCKS):
        cells = CELL_ORDER if b % 2 == 0 else CELL_ORDER[::-1]
        for cell in cells:
            if cell in EXTRA_CELLS:
                five = ARMS + EXTRA_ARMS
                out += [(cell, a, seed, rep) for a in five[b % 5:] + five[:b % 5]]
            else:
                out += [(cell, a, seed, rep) for a in ARM_ROT[b % 3]]
    return out


# ── the neutral series ───────────────────────────────────────────────────
class Win:
    """One scored window of the (single) path. Fields None when not measured."""
    __slots__ = ("t", "n", "ask", "h", "rtprop", "tmin", "tp50", "n_k", "floor", "pin", "hold", "wbind", "nmin", "nmax",
                 "occ", "B")

    def __init__(self, **kw):
        for k in self.__slots__:
            setattr(self, k, kw.get(k))

    def brake(self):
        return None if (self.tmin is None or self.rtprop is None) else self.tmin - self.rtprop


# One literal shared with the engine's format test (net/oracle/tests.rs OCAP_FIXTURE).
OCAP_RE = re.compile(
    r"\[OCAP\] t=([\d.]+)s p(\d+) law=(\w+) n=(\d+) ask=([\d.]+) h=(\d+) rtprop_us=(\S+) tmin_us=(\S+) tp50_us=(\S+) "
    r"s_us=(\d+) brake_us=(\S+) n_k=(\d+) offered=(\d+) r_k=(\S+) floor=([\d.]+) pin=([\d.]+) hold=([\d.]+) wbind=(\d) "
    r"nmin=(\d+) nmax=(\d+) refreshes=(\d+) q=([\d.]+)")


def _opt(x):
    return None if x == "-" else int(x)


def parse_ocap(line):
    """One `[OCAP]` line -> dict, or None."""
    m = OCAP_RE.search(line)
    if not m:
        return None
    g = m.groups()
    return {"tstr": g[0], "t": float(g[0]), "path": int(g[1]), "law": g[2], "n": int(g[3]), "ask": float(g[4]),
            "h": int(g[5]), "rtprop": _opt(g[6]), "tmin": _opt(g[7]), "tp50": _opt(g[8]), "s_us": int(g[9]),
            "brake": _opt(g[10]), "n_k": int(g[11]), "offered": int(g[12]),
            "r_k": None if g[13] == "-" else float(g[13]), "floor": float(g[14]), "pin": float(g[15]),
            "hold": float(g[16]), "wbind": int(g[17]), "nmin": int(g[18]), "nmax": int(g[19]),
            "refreshes": int(g[20]), "q": float(g[21])}


_DGQ = re.compile(r"\bdgq(\d+)\[hand=(\d+) tx=(\d+)")


def diag_ticks(cli):
    """[(tstr, t, win_num, win_den, B|None)] for the client's `[DIAG]` lines. B = Σ over paths of dgq[hand - tx]; None
    when the line has no dgq block."""
    out = []
    for ln in cli:
        mt = sl._T.search(ln)
        if not mt:
            continue
        mw = sl._WIN.search(ln)
        if not mw:
            continue
        dg = _DGQ.findall(ln)
        b = sum(int(h) - int(x) for _, h, x in dg) if dg else None
        out.append((mt.group(1), float(mt.group(1)), int(mw.group(1)), int(mw.group(2)), b))
    return out


def ocap_series(cli, path=0):
    """The row's windows: path `path`'s `[OCAP]` lines joined with the same tick's `[DIAG]` (occupancy, B)."""
    dt = {tstr: (num, den, b) for tstr, _, num, den, b in diag_ticks(cli)}
    out = []
    for ln in cli:
        o = parse_ocap(ln)
        if not o or o["path"] != path:
            continue
        num, den, b = dt.get(o["tstr"], (None, None, None))
        out.append(Win(t=o["t"], n=o["n"], ask=o["ask"], h=o["h"], rtprop=o["rtprop"], tmin=o["tmin"], tp50=o["tp50"],
                       n_k=o["n_k"], floor=o["floor"], pin=o["pin"], hold=o["hold"], wbind=o["wbind"], nmin=o["nmin"],
                       nmax=o["nmax"], occ=(num / o["n"]) if (num is not None and o["n"]) else None, B=b))
    return out


_LD = re.compile(r"\[LOOPDIAG\] t=([\d.]+)s p(\d+) n=(\d+) ov=(\d+) rtt_min_us=(\S+) rtt_p10_us=(\S+) rtt_p50_us=(\S+) "
                 r"rtprop_us=(\S+)")
Q_BULK = 0.1


def ledger_series(cli, nleg=1, knee=KNEE):
    """The dry-run reader: a ledger row without `[OCAP]`. n is the `[DIAG] win=` denominator; the floor bind is
    n <= 10 and the pin bind n >= N*knee (the bound the cap sits on); the hold is 0; the sampler is `[LOOPDIAG]` path 0
    (n_k = buffered + overflow, tmin / tp50 / rtprop from the line). ask is unknown (None)."""
    ld = {}
    for ln in cli:
        m = _LD.search(ln)
        if m and int(m.group(2)) == 0:
            ld[m.group(1)] = m.groups()
    out = []
    for tstr, t, num, den, b in diag_ticks(cli):
        g = ld.get(tstr)
        tmin = tp50 = rtprop = None
        n_k = None
        if g:
            n_k = int(g[2]) + int(g[3])
            tmin, tp50, rtprop = _opt(g[4]), _opt(g[6]), _opt(g[7])
        wb = None
        if tp50:
            wb = int(tp50 / 1e6 / TW > Q_BULK)
        out.append(Win(t=t, n=den, ask=None, h=0, rtprop=rtprop, tmin=tmin, tp50=tp50, n_k=n_k,
                       floor=1.0 if den <= FLOOR_N else 0.0, pin=1.0 if den >= nleg * knee else 0.0, hold=0.0, wbind=wb,
                       nmin=den, nmax=den, occ=(num / den) if den else None, B=b))
    return out


# ── statistics ───────────────────────────────────────────────────────────
def med(v):
    v = [x for x in v if x is not None]
    return lc.med(v) if v else None


def mean(v):
    v = [x for x in v if x is not None]
    return sum(v) / len(v) if v else None


def sd(v):
    v = [x for x in v if x is not None]
    if len(v) < 2:
        return None
    if min(v) == max(v):
        return 0.0
    m = sum(v) / len(v)
    return math.sqrt(sum((x - m) ** 2 for x in v) / (len(v) - 1))


def sd_ln(ns):
    return sd([math.log(n) for n in ns if n and n > 0])


def tertile(w, tl, k):
    """The k-th (0, 1, 2) time tertile of windows `w`, tl the row's last window time."""
    lo, hi = tl * k / 3.0, tl * (k + 1) / 3.0
    return [x for x in w if x.t >= lo and (k == 2 or x.t < hi)]


def rtprop_at(w, t):
    """RTprop(t): the last window with time <= t."""
    c = [x.rtprop for x in w if x.t <= t and x.rtprop is not None]
    return c[-1] if c else None


def d_band(x, band):
    """d(x): 0 inside the band, else |ln| to the nearer edge."""
    lo, hi = band
    if lo <= x <= hi:
        return 0.0
    return abs(math.log(x / (lo if x < lo else hi)))


# ── the row's readouts R1-R10 and its label inputs ───────────────────────
def readouts(wins, arm, cell, dnf=False):
    """R1-R10 and the label predicates of one row from its neutral series. Windows with t >= 1 s only; the last third
    is the windows with t >= 2/3 of the row's last window time."""
    cfg = CELLS.get(cell) or CELLS["c1s-400"]       # ledger-only cells (c1d, c8) borrow c1s's sd for the dry-run
    W = [w for w in wins if w.t >= WARM_S]
    r = {"windows": len(W)}
    if not W:
        r["empty"] = True
        return r
    tl = W[-1].t
    third = [w for w in W if w.t >= tl * 2.0 / 3.0]
    r["t_last"] = tl
    ns = [w.n for w in third]
    r["n_end"] = med(ns)
    r["s_end"] = sd_ln(ns)
    fr = lambda ws, k: mean([getattr(w, k) for w in ws])  # noqa: E731
    for k in ("floor", "pin", "hold", "wbind"):
        r[f"{k}_all"], r[f"{k}_last"] = fr(W, k), fr(third, k)
    nk = [w.n_k for w in W if w.n_k is not None]
    nk3 = [w.n_k for w in third if w.n_k is not None]
    r["nk_min"], r["nk_med"], r["nk_max"] = (min(nk), med(nk), max(nk)) if nk else (None, None, None)
    r["nk_last_min"], r["nk_last_med"], r["nk_last_max"] = (min(nk3), med(nk3), max(nk3)) if nk3 else (None, None, None)
    r["nk_over_cap"] = (sum(1 for x in nk if x > LOOPDIAG_CAP) / len(nk)) if nk else None       # R6
    r["nk_few"] = (sum(1 for x in nk if x < FEW) / len(nk)) if nk else None                     # R10
    r["tp50_last"] = med([w.tp50 for w in third])
    per3 = lambda f: [med([f(w) for w in tertile(W, tl, k)]) for k in range(3)]  # noqa: E731
    r["brake_3"] = per3(lambda w: w.brake())                                                    # R4
    r["occ_3"] = per3(lambda w: w.occ)                                                          # R9
    r["B_3"] = per3(lambda w: w.B)                                                              # R7
    r["askn_3"] = per3(lambda w: (w.ask / w.n) if (w.ask is not None and w.n) else None)        # R8
    r["rtprop_1s"] = next((w.rtprop for w in W if w.t >= WARM_S and w.rtprop is not None), None)
    r["rtprop_10s"], r["rtprop_end"] = rtprop_at(W, 10.0), rtprop_at(W, tl)
    r["series"] = [[w.t, w.n] for w in W]
    # label inputs (UNSTABLE modes)
    # Amendment 2: the floor clause is struck (the law cannot reach the floor once Copa has a sample); COLLAPSE is the DNF row.
    r["COLLAPSE"] = bool(dnf)
    r["OSCILLATION"] = bool(r["s_end"] is not None and r["s_end"] > OSC_MULT * cfg["sd"])
    r["RUNAWAY_pin"] = bool((r["pin_last"] or 0) >= BIND_MIN)          # RUNAWAY iff SL
    if arm == "SH":
        t_h = T_H
        hold_last2 = [w for w in W if t_h - 2.0 < w.t <= t_h]
        r["rtprop_th"] = rtprop_at(W, t_h)
        tm = [w.tmin for w in hold_last2 if w.tmin is not None]
        r["tmin_hold_last2"] = min(tm) if tm else None
        r["ask_over_H_hold"] = med([(w.ask / cfg["H"]) if w.ask is not None else None for w in hold_last2])        # R3
        r["brake_release"] = next((w.brake() for w in W if w.t >= t_h and w.brake() is not None), None)
        if None in (r["rtprop_th"], r["rtprop_1s"], r["tmin_hold_last2"]):
            r["hold_state"] = None
        else:
            thr = r["rtprop_1s"] + 0.5 * (r["tmin_hold_last2"] - r["rtprop_1s"])
            r["hold_thr_us"] = thr
            r["hold_state"] = "HOLD-EXPIRED" if r["rtprop_th"] >= thr else "HOLD-INERT"
    return r


def duration_s(row):
    """The row's length: perf's seconds, else the last window time (a DNF row has no seconds)."""
    if row.get("seconds"):
        return row["seconds"]
    return row.get("t_last")


def is_short(row):
    """SHORT: an SL or MAIN row under 30 s, an SH row under t_h + 30 s. A DNF row ran to the timeout: not short."""
    if row.get("dnf"):
        return False
    d = duration_s(row)
    if d is None:
        return False
    return d < (T_H + SHORT_S if row["arm"] == "SH" else SHORT_S)


IGNORE_SHORT = False      # the dry-run's second pass only: reads the other rules on rows the SHORT rule would drop


def scored(row):
    """LIVE, not SHORT and, for SH, HOLD-EXPIRED."""
    if row.get("status") != "LIVE" or (row.get("short") and not IGNORE_SHORT):
        return False
    if row["arm"] == "SH" and row.get("hold_state") != "HOLD-EXPIRED":
        return False
    return not row.get("empty")


def unstable_modes(row):
    """The UNSTABLE modes a row shows (the label names the row and the mode)."""
    m = []
    if row.get("COLLAPSE"):
        m.append("COLLAPSE")
    if row.get("OSCILLATION"):
        m.append("OSCILLATION")
    if row["arm"] == "SL" and row.get("RUNAWAY_pin"):
        m.append("RUNAWAY")
    return m if row["arm"] in ("SL", "SH") else []


def t_ret(series, n_sl, cfg, t_h=T_H):
    """R3: the first window after t_h that opens `CONSEC` consecutive windows with |ln(n / n_SL)| <= K sd_pred;
    None = "none"."""
    tol = K * cfg["sd"]
    post = [(t, n) for t, n in series if t > t_h]
    for i in range(len(post) - CONSEC + 1):
        chunk = post[i:i + CONSEC]
        if all(n > 0 and abs(math.log(n / n_sl)) <= tol for _, n in chunk):
            return chunk[0][0]
    return None


def returned(row, n_sl, cfg):
    """RETURNED iff |ln(n_end / n_SL)| <= K sd_pred; else NOT-RETURNED with its direction. -> (label, direction|None)"""
    x = math.log(row["n_end"] / n_sl)
    if abs(x) <= K * cfg["sd"]:
        return "RETURNED", None
    return "NOT-RETURNED", ("HIGH" if x > 0 else "LOW")


# ── ROW ──────────────────────────────────────────────────────────────────
def make_row(cell, arm, seed, rep, rc, wall_s, drv, cli, srv, cotenant=0, sha=None):
    cfg = CELLS[cell]
    row = sp.make_row(cell, arm, seed, rep, rc, wall_s, drv, cli, srv, cotenant)
    row["bin_sha"] = sha
    row["gate_store"] = lc.gate_tok(cli, "RWM_STORE")
    row["gate_diag"] = lc.gate_tok(cli, "RWM_DIAG")
    cap = [m for m in (sl._SUMCAP.search(ln) for ln in cli) if m]
    row["sumcap_eng"] = int(cap[-1].group(2)) if cap else None
    law_cli = [ln for ln in cli if LAW_ECHO in ln]
    law_srv = [ln for ln in srv if LAW_ECHO in ln]
    row["law_echo_cli"], row["law_echo_srv"] = bool(law_cli), bool(law_srv)
    row["readout_echo_cli"] = any("[OCAP] sampler readout ACTIVE" in ln for ln in cli)
    wins = ocap_series(cli)
    allp = {o["path"] for o in (parse_ocap(ln) for ln in cli) if o}
    row["ocap_paths"] = sorted(allp)
    ticks = diag_ticks(cli)
    row["dnf"] = bool(row.get("dnf"))
    ro = readouts(wins, arm, cell, dnf=row["dnf"])
    row.update(ro)
    ax = [m for m in (sl._AX.search(ln) for ln in srv if "[LAT]" in ln) if m]
    row["ax_p50_us"] = int(ax[-1].group(1)) if ax else None
    # ── witnesses ──
    p, bad_sampler = [], []
    exp_l = str(float(cfg["L"]))
    if arm == "MAIN":
        if row["law_echo_cli"] or row["law_echo_srv"]:
            p.append("oracle-law-echo-on-MAIN")
        if not row["sumcap_eng"]:
            p.append(f"sumcap-eng={row['sumcap_eng']} (want >0)")
        if not row["readout_echo_cli"]:
            p.append("no-sampler-readout-echo-on-MAIN")
    else:
        if not row["law_echo_cli"]:
            p.append("no-oracle-law-echo")
        else:
            line = law_cli[-1]
            lm = LIST_RE.search(line)
            if not lm or [x.strip() for x in lm.group(1).split(",")] != [exp_l]:
                p.append(f"oracle-L-list={lm.group(1) if lm else None} (want [{exp_l}])")
            hm = HOLD_RE.search(line)
            if arm == "SH" and not (hm and int(hm.group(1)) == cfg["H"] and float(hm.group(2)) == T_H):
                p.append(f"oracle-hold={hm.groups() if hm else None} (want {cfg['H']}:{T_H})")
            if arm != "SH" and "hold=none" not in line:
                p.append("oracle-hold-on-" + arm)
    echo_line = (law_cli[-1] if law_cli else next((ln for ln in reversed(cli) if "[OCAP] sampler readout ACTIVE" in ln), None))
    want_b0, want_gain = EXTRA_ENV.get(arm, ("0", "q"))
    bm = B0_RE.search(echo_line) if echo_line else None
    row["echo_b0_gain"] = list(bm.groups()) if bm else None
    if echo_line and (not bm or bm.groups() != (want_b0, want_gain)):
        p.append(f"echo-b0-gain={row['echo_b0_gain']} (want {[want_b0, want_gain]})")
    if row["gate_store"] != "unset":
        p.append(f"gate-store={row['gate_store']} (want unset)")
    if row["gate_diag"] != "1":
        p.append(f"gate-diag={row['gate_diag']} (want 1)")
    if row["ocap_paths"] != [0]:
        p.append(f"ocap-paths={row['ocap_paths']} (want [0])")
    post_ticks = [tk for tk in ticks if tk[1] >= WARM_S]
    have = {w.t for w in wins}
    missing = [tk[0] for tk in post_ticks if tk[1] not in have]
    if missing or not post_ticks:
        p.append(f"ocap-missing-windows={len(missing)}/{len(post_ticks)}")
    if any(tk[4] is None for tk in post_ticks):
        p.append("dgq-missing")
    # Σ n_k == offered, per [OCAP] line
    cum, off_bad = 0, 0
    for ln in cli:
        o = parse_ocap(ln)
        if o and o["path"] == 0:
            cum += o["n_k"]
            if cum != o["offered"]:
                off_bad += 1
    if off_bad or cum == 0:
        bad_sampler.append(f"sum-nk-ne-offered={off_bad} lines (sum {cum})")
    if arm != "MAIN":
        dd = {tk[0]: tk[3] for tk in post_ticks}
        hit = [1 for w in wins if w.t >= WARM_S and dd.get(f"{w.t:.1f}") == w.n]
        tot = len([w for w in wins if w.t >= WARM_S])
        row["route_frac"] = (len(hit) / tot) if tot else None
        if tot == 0 or row["route_frac"] < ROUTE_MIN:
            p.append(f"routing-win-den-eq-n={row['route_frac']}")
    row["problems"].extend(p + bad_sampler)
    if row["status"] == "LIVE":
        if bad_sampler:
            row["status"] = "VOID-SAMPLER"
        elif p:
            row["status"] = "WITNESS-FAIL"
    if os.environ.get("LD_ONDISK", "0") == "1":
        row["status"] = "VOID-ONDISK"
    io = os.environ.get("LD_IO_FULL10")
    try:
        row["io_full10"] = float(io) if io not in (None, "") else None
    except ValueError:
        row["io_full10"] = None
    row["io_high"] = row["io_full10"] is not None and row["io_full10"] >= 20.0
    row["short"] = bool(row["status"] == "LIVE" and is_short(row))
    return row


# ── LEDGER ───────────────────────────────────────────────────────────────
def rows_of(paths):
    rows, tokens = [], []
    for path in paths:
        for ln in lc.read(path):
            if ln.startswith("P43ROW "):
                try:
                    rows.append(json.loads(ln[len("P43ROW "):]))
                except ValueError:
                    tokens.append("MALFORMED-P43ROW")
            for t in ABORT_TOKENS:
                if ln.startswith(t):
                    tokens.append(t)
    return rows, tokens


fmt = sp.fmt


def rid(r):
    return f"{r['cell']} {r['arm']} s{r['seed']} r{r['rep']}"


def cell_arm(rows, cell, arm, pred=lambda r: True):
    return [r for r in rows if r["cell"] == cell and r["arm"] == arm and pred(r)]


def pooled_cmp(x, y, lower_is_better):
    """x against y by 2 sigma_pooled: BETTER / WITHIN / WORSE; UNSCOREABLE with fewer than 2 values in an arm."""
    x = [v for v in x if v is not None]
    y = [v for v in y if v is not None]
    if len(x) < 2 or len(y) < 2:
        return "UNSCOREABLE", None, None
    sx, sy = sd(x), sd(y)
    sp_ = math.sqrt(((len(x) - 1) * sx ** 2 + (len(y) - 1) * sy ** 2) / (len(x) + len(y) - 2))
    diff = sum(x) / len(x) - sum(y) / len(y)
    tol = 2 * sp_
    if abs(diff) <= tol:
        return "WITHIN", diff, tol
    better = (diff < 0) if lower_is_better else (diff > 0)
    return ("BETTER" if better else "WORSE"), diff, tol


def cell_outcome(rows, cell, out=print):
    """The cell's label by the ladder, plus SPREAD / GOODPUT / DELAY. -> dict"""
    cfg = CELLS[cell]
    res = {"cell": cell}
    allr = [r for r in rows if r["cell"] == cell]
    sc = lambda arm: [r for r in allr if r["arm"] == arm and scored(r)]  # noqa: E731
    sl_, sh_, mn_ = sc("SL"), sc("SH"), sc("MAIN")
    out(f"[{cell}] rows: MAIN {len(cell_arm(allr, cell, 'MAIN'))} SL {len(cell_arm(allr, cell, 'SL'))} "
        f"SH {len(cell_arm(allr, cell, 'SH'))}; scored MAIN {len(mn_)} SL {len(sl_)} SH {len(sh_)}")
    for r in allr:
        flags = [r["status"]] + (["SHORT"] if r.get("short") else []) + ([r["hold_state"]] if r.get("hold_state") else []) \
            + (["DNF"] if r.get("dnf") else []) + (["IO-HIGH"] if r.get("io_high") else [])
        out(f"  ROW {rid(r)}: {' '.join(flags)} dur={fmt(duration_s(r), 1)}s n_end={fmt(r.get('n_end'), 1)} "
            f"s_end={fmt(r.get('s_end'), 3)} floor/pin last={fmt(r.get('floor_last'), 3)}/{fmt(r.get('pin_last'), 3)} "
            f"mbps={fmt(r.get('mbps'), 1)} {' '.join(r.get('problems') or [])}")
        if not scored(r) and r["status"] == "LIVE" and unstable_modes(r):
            out(f"  EXCLUDED-WOULD-TRIGGER {rid(r)}: {','.join(unstable_modes(r))} (left out of the outcome: "
                f"{'SHORT' if r.get('short') else r.get('hold_state')})")
    n_sl = med([r["n_end"] for r in sl_]) if sl_ else None
    res["n_SL"] = n_sl
    # 1 UNSCOREABLE
    # Amendment 2: at c1s the start-low rows carry the cell when its SH rows read HOLD-INERT (>= 2 of them): SH cannot test bistability there.
    inert = [r for r in allr if r["arm"] == "SH" and r["status"] == "LIVE" and not r.get("short") and r.get("hold_state") == "HOLD-INERT"]
    carry = cell == "c1s-400" and len(sl_) >= MIN_SCORED and len(sh_) < MIN_SCORED and len(inert) >= MIN_SCORED
    res["sl_only"] = carry
    if len(sl_) < MIN_SCORED or (len(sh_) < MIN_SCORED and not carry):
        res["label"] = "UNSCOREABLE"
        res["why"] = f"scored SL {len(sl_)}, scored SH {len(sh_)} (need {MIN_SCORED} each)"
    else:
        # 2 UNSTABLE
        un = [(rid(r), m) for r in sl_ + sh_ for m in unstable_modes(r)]
        shr = []
        for r in sh_:
            lab, dirn = returned(r, n_sl, cfg)
            r["_ret"] = (lab, dirn)
            r["_tret"] = t_ret(r["series"], n_sl, cfg)
            shr.append((rid(r), lab, dirn, r["_tret"]))
        res["sh_rows"] = shr
        if un:
            res["label"], res["why"] = "UNSTABLE", "; ".join(f"{a}:{m}" for a, m in un)
        else:
            nr = [x for x in shr if x[1] == "NOT-RETURNED"]
            if len(nr) >= 2:
                res["label"], res["why"] = "BISTABLE", f"{len(nr)} SH rows NOT-RETURNED " + ",".join(f"{a}:{d}" for a, _, d, _ in nr)
            elif len(nr) == 1:
                res["label"], res["why"] = "MIXED", f"1 SH row NOT-RETURNED: {nr[0][0]}:{nr[0][2]}"
            else:
                d = d_band(n_sl, cfg["band"])
                res["d_nSL"] = d
                if d <= K * cfg["sd"]:
                    res["label"], res["why"] = "CONVERGES-ONE-POINT", f"n_SL {n_sl:.1f} d={d:.3f} <= {K * cfg['sd']:.3f}"
                else:
                    res["label"], res["why"] = "ONE-POINT-OFF-PREDICTION", f"n_SL {n_sl:.1f} d={d:.3f} > {K * cfg['sd']:.3f}"
        if carry:
            res["why"] += f" [SL-ONLY: {len(inert)} SH rows HOLD-INERT; SH cannot test bistability at {cell}]"
    # reported beside the label
    s_end = med([r["s_end"] for r in sl_])
    if s_end is None:
        res["SPREAD"] = "UNSCOREABLE"
    else:
        ratio = s_end / cfg["sd"]
        res["SPREAD"] = f"{'AS-PREDICTED' if 0.5 <= ratio <= 2.0 else 'OFF-PREDICTION'} (median s_end {s_end:.3f} / {cfg['sd']} = {ratio:.2f})"
    for key, name, lower in (("mbps", "GOODPUT", False), ("ax_p50_us", "DELAY_Ax", True), ("tp50_last", "DELAY_Tp50", True)):
        tok, diff, tol = pooled_cmp([r.get(key) for r in sl_], [r.get(key) for r in mn_], lower)
        res[name] = tok
        res[name + "_detail"] = None if diff is None else f"SL-MAIN {diff:+.2f} vs 2sigma_pooled {tol:.2f}"
    # R-readouts reported per arm
    for arm, rr in (("MAIN", mn_), ("SL", sl_), ("SH", sh_)):
        for key, nm in (("n_end", "n_end"), ("s_end", "s_end"), ("floor_last", "floor"), ("pin_last", "pin"),
                        ("hold_last", "hold"), ("wbind_last", "wbind"), ("nk_min", "nk_min"), ("nk_over_cap", "nk>8192"),
                        ("nk_few", "nk<8"), ("tp50_last", "Tp50_last_us")):
            out(f"  R {arm} {nm}: {[None if r.get(key) is None else round(r[key], 3) for r in rr]}")
        for key, nm in (("brake_3", "R4 brake_us per third"), ("B_3", "R7 B per third"), ("occ_3", "R9 occupancy per third"),
                        ("askn_3", "R8 ask/n per third")):
            out(f"  R {arm} {nm}: {[[None if x is None else round(x, 3) for x in r[key]] for r in rr if r.get(key)]}")
    for r in sh_:
        out(f"  R3 {rid(r)}: {r['_ret'][0]}{('/' + r['_ret'][1]) if r['_ret'][1] else ''} t_ret={r['_tret']} "
            f"ask/H(hold last 2 s)={fmt(r.get('ask_over_H_hold'), 3)} brake@release={r.get('brake_release')} "
            f"RTprop 1s/10s/t_h/end={r.get('rtprop_1s')}/{r.get('rtprop_10s')}/{r.get('rtprop_th')}/{r.get('rtprop_end')} "
            f"hold-thr={fmt(r.get('hold_thr_us'), 0)}us")
    # Amendment 2: the extra start-low arms are REPORTED ONLY (never in sl_/sh_/the ladder): R1-R10 as for SL rows,
    # goodput and delay against the scored MAIN rows, and n_end against the offline-map point.
    res["extra"] = {}
    for arm in EXTRA_ARMS:
        xr = [r for r in allr if r["arm"] == arm and scored(r)]
        if not [r for r in allr if r["arm"] == arm]:
            continue
        e = {"scored": len(xr), "n_end": med([r["n_end"] for r in xr])}
        out(f"  EXTRA {arm} (reported only): scored {len(xr)}/{len([r for r in allr if r['arm'] == arm])}")
        for key, nm in (("n_end", "n_end"), ("s_end", "s_end"), ("floor_last", "floor"), ("pin_last", "pin"),
                        ("wbind_last", "wbind"), ("nk_min", "nk_min"), ("nk_over_cap", "nk>8192"), ("nk_few", "nk<8"),
                        ("tp50_last", "Tp50_last_us"), ("mbps", "mbps"), ("ax_p50_us", "A_x p50 us")):
            out(f"  R {arm} {nm}: {[None if r.get(key) is None else round(r[key], 3) for r in xr]}")
        for key, nm in (("brake_3", "R4 brake_us per third"), ("B_3", "R7 B per third"), ("occ_3", "R9 occupancy per third"),
                        ("askn_3", "R8 ask/n per third")):
            out(f"  R {arm} {nm}: {[[None if x is None else round(x, 3) for x in r[key]] for r in xr if r.get(key)]}")
        for key, name, lower in (("mbps", "GOODPUT", False), ("ax_p50_us", "DELAY_Ax", True), ("tp50_last", "DELAY_Tp50", True)):
            tok, diff, tol = pooled_cmp([r.get(key) for r in xr], [r.get(key) for r in mn_], lower)
            e[name] = tok
            out(f"  {arm} {name} vs MAIN: {tok} {'' if diff is None else f'{arm}-MAIN {diff:+.2f} vs 2sigma_pooled {tol:.2f}'}")
        if e["n_end"]:
            e["ln_vs_pred"] = math.log(e["n_end"] / PRED_EXTRA[arm])
            out(f"  {arm} n_end {e['n_end']:.1f} vs the offline-map point {PRED_EXTRA[arm]:.0f}: ln ratio {e['ln_vs_pred']:+.3f} (reported)")
        res["extra"][arm] = e
    out(f"  LABEL {cell}: {res['label']} ({res.get('why')})")
    out(f"  SPREAD {res['SPREAD']}")
    for name in ("GOODPUT", "DELAY_Ax", "DELAY_Tp50"):
        out(f"  {name} {res[name]} {res.get(name + '_detail') or ''}")
    return res


def score(paths, out=print):
    rows, tokens = rows_of(paths)
    aborted = sorted({t for t in tokens if t in ABORT_TOKENS})
    out("ORACLE STORE LAW IN CLOSED LOOP (status §43) SCORE: MAIN = shipped law + sampler readout, SL = oracle from boot, "
        "SH = oracle with the 15 s hold; SL-B0 / SL-G12 = reported-only c2 start-low arms (amendment 2)")
    out(f"ROWS {len(rows)}/{EXPECT_ROWS}; ABORT-TOKENS {' '.join(aborted) or 'none'}")
    out("STATUS-TABLE (every invocation row)")
    for st in ("LIVE", "NO_DATA", "VOID-RC", "VOID-COTENANT", "VOID-ONDISK", "VOID-SAMPLER", "CONTAMINATED", "WITNESS-FAIL"):
        out(f"  {st:<13} {sum(1 for r in rows if r['status'] == st)}")
    res = {"cells": {}}
    for cell in CELL_ORDER:
        res["cells"][cell] = cell_outcome([r for r in rows if r["cell"] == cell], cell, out)
    out("")
    labs = {c: v["label"] for c, v in res["cells"].items()}
    if aborted:
        out("ABORTED: " + ",".join(aborted))
    out("DECISION " + decision(labs))
    res["labels"] = labs
    out("RESULT " + json.dumps(res, sort_keys=True, default=str))
    return labs


def decision(labs):
    """§43's decision rule, as a pointer; nothing ships, and every item goes to the user as a proposal."""
    v = set(labs.values())
    if "UNSTABLE" in v:
        return "UNSTABLE at " + ",".join(c for c, x in labs.items() if x == "UNSTABLE") + ": §42's step 3 (the split diagnostic) comes first"
    if v & {"BISTABLE", "MIXED"}:
        return "BISTABLE/MIXED at " + ",".join(c for c, x in labs.items() if x in ("BISTABLE", "MIXED")) \
            + ": the anchor becomes the user's design decision (lifetime minimum, drain or another brake); no engine L-hat work first"
    if "ONE-POINT-OFF-PREDICTION" in v:
        return "ONE-POINT-OFF-PREDICTION at " + ",".join(c for c, x in labs.items() if x == "ONE-POINT-OFF-PREDICTION") \
            + ": that cell's map is re-derived from this battery's uncapped windows first"
    if "UNSCOREABLE" in v:
        return "UNSCOREABLE at " + ",".join(c for c, x in labs.items() if x == "UNSCOREABLE") + ": the cell's question stays open; a re-run needs an amendment"
    return "CONVERGES-ONE-POINT at both cells: the shipped anchor gives one operating point in practice; next steps go to the user as a proposal"


def check(path):
    rows, tokens = rows_of([path])
    t = open(path, errors="replace").read()
    ok = "LD-BATTERY-DONE" in t and len(rows) == EXPECT_ROWS
    print(f"CHECK43 done={'LD-BATTERY-DONE' in t} rows={len(rows)}/{EXPECT_ROWS}")
    return 0 if ok else 1


def smoke(path):
    """rc 0 iff both smoke rows (c2 SH, c1s SL, full size) are LIVE with the witnesses the scorer reads (sampler, routing,
    dgq, the law echo) and a non-empty [OCAP] series."""
    rows, _ = rows_of([path])
    got = {(r["cell"], r["arm"]): r for r in rows}
    bad = []
    for cell, arm, _, _ in SMOKE:
        r = got.get((cell, arm))
        if r is None:
            bad.append(f"{cell} {arm}: no row")
            continue
        miss = [] if r["status"] == "LIVE" else [r["status"]]
        for k in ("n_end", "s_end", "nk_min", "mbps"):
            if r.get(k) is None:
                miss.append(f"no-{k}")
        if arm == "SH" and r.get("hold_state") is None:
            miss.append("no-hold-state")
        print(f"SMOKE43 {cell} {arm}: status={r['status']} dur={duration_s(r)} mbps={r.get('mbps')} n_end={r.get('n_end')} "
              f"hold_state={r.get('hold_state')} route={r.get('route_frac')} nk_min={r.get('nk_min')} "
              f"{' '.join(r['problems'] + miss)}")
        if miss:
            bad.append(f"{cell} {arm}: {miss}")
    if bad:
        print("SMOKE-FAIL " + "; ".join(bad))
        return 1
    print("SMOKE-PASS")
    return 0


# ── the dry-run on committed ledger rows ─────────────────────────────────
def ledger_row(cell, label, cli, drv, arm_as, nleg=1):
    """A neutral row from a ledger log pair. arm_as is the arm whose rules it stands in for (SL / SH / MAIN)."""
    wins = ledger_series(cli, nleg)
    base = cell if cell in CELLS else "c1s-400"
    ro = readouts(wins, arm_as, base)
    objs = sp._jsons(drv)
    summ = [o for o in objs if o.get("summary") is True]
    mbps = sec = None
    dnf = False
    if summ:
        dnf = bool(summ[-1].get("dnf"))
        mbps, sec = summ[-1].get("mean_mbps"), summ[-1].get("median_s")
    row = {"cell": cell, "arm": arm_as, "label": label, "status": "LIVE", "seed": "-", "rep": 0, "dnf": dnf,
           "mbps": mbps, "seconds": sec, "problems": []}
    row.update(ro)
    row["short"] = is_short(row)
    return row


LEDGER_NAME = re.compile(r"(c1s-400|c2-100|c1d-400|c8-100)-([A-Za-z0-9]+)-s(\d+)-r(\d+)-a\d+-c\.log$")


def ledger_rows(dirs):
    """Neutral rows (arm_as = SL: the SL predicates are the superset) from every `*-c.log` under `dirs`."""
    import glob
    rows = []
    for d in dirs:
        for c in sorted(glob.glob(os.path.join(d, "**", "*-c.log"), recursive=True)):
            m = LEDGER_NAME.search(os.path.basename(c))
            if not m:
                continue
            cell, arm, seed, rep = m.groups()
            nleg = 2 if cell in ("c1d-400", "c8-100") else 1
            r = ledger_row(cell, arm, lc.read(c), lc.read(c[:-len("-c.log")] + "-drv.out"), "SL", nleg)
            r["seed"], r["rep"], r["src"] = seed, int(rep), os.path.relpath(c, d).replace("\\", "/")
            rows.append(r)
    return rows


def dryrun(dirs, out=print):
    """The scorer's per-row rules on committed ledger rows (the dry-run of amendment 1). Each row is read as an SL row:
    SHORT, COLLAPSE, OSCILLATION, RUNAWAY. 'returned' is tested leave-one-out against the median n_end of the other rows
    of the same (cell, label) group. Returns the rows."""
    rows = ledger_rows(dirs)
    out("DRYRUN rows=%d (ledger rows have no [OCAP]: n = [DIAG] win= denominator, sampler = [LOOPDIAG], pin = n >= N*2048)" % len(rows))
    out("cell label seed rep dur_s SHORT n_end s_end floor_last pin_last modes returned(vs others)")
    for r in rows:
        grp = [x for x in rows if x is not r and x["cell"] == r["cell"] and x["label"] == r["label"] and x.get("n_end")]
        ref = med([x["n_end"] for x in grp]) if len(grp) >= 2 else None
        cfg = CELLS.get(r["cell"]) or CELLS["c1s-400"]
        ret = returned(r, ref, cfg)[0] if (ref and r.get("n_end")) else "-"
        r["_ret"] = ret
        out(f"{r['cell']} {r['label']} s{r['seed']} r{r['rep']} {fmt(duration_s(r), 1)} {'SHORT' if r['short'] else '-'} "
            f"{fmt(r.get('n_end'), 1)} {fmt(r.get('s_end'), 3)} {fmt(r.get('floor_last'), 3)} {fmt(r.get('pin_last'), 3)} "
            f"{','.join(unstable_modes(r)) or '-'} {ret}")
    return rows


def main(argv):
    if not argv:
        print(__doc__)
        return 2
    cmd, a = argv[0], argv[1:]
    if cmd == "row":
        cell, arm, seed, rep, rc, wall, drv, cli, srv = a[:9]
        cot = int(a[9]) if len(a) > 9 else 0
        sha = a[10] if len(a) > 10 else None
        r = make_row(cell, arm, seed, rep, rc, wall, lc.read(drv), lc.read(cli), lc.read(srv), cot, sha)
        print("P43ROW " + json.dumps(r, sort_keys=True))
        return 0
    if cmd == "plan":
        for c, ar, seed, rep in plan43(smoke=bool(a) and a[0] == "smoke"):
            print(f"{c} {ar} {seed} {rep}")
        return 0
    if cmd == "check":
        return check(a[0])
    if cmd == "smoke":
        return smoke(a[0])
    if cmd == "score":
        score(a)
        return 0
    if cmd == "dryrun":
        dryrun(a)
        return 0
    print(f"unknown command {cmd}")
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
