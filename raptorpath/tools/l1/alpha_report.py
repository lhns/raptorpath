#!/usr/bin/env python3
"""Scoring pass for THE ALPHA-SWEEP (goal #100 item 2).

    alpha_report.py <ledger.log> [<ledger2.log> ...]

Reads the JSONL rows `alpha_parse.py` writes (one JSON object per line,
embedded in a text log — the same shape `ccand_report.py` scrapes) and prints a
plain-ASCII report. Python 3, stdlib only, exactly as `ccand_report.py` and
`lat_report.py` are: no numpy, no pandas, every median and percentile helper
inline. Every division is guarded. It runs to completion on an EMPTY or PARTIAL
ledger, which is how the smoke pass is checked.

THE SWEEP. `resolved_alpha()` (`net/mod.rs:788`) is the ONE place α enters the
engine, and `RWM_ALPHA_OVERRIDE` is the ONE knob that moves it. Six arms, one
independent variable:

    CTL   null   RWM_QUANTILE_CLOCKS=0, RWM_ALPHA_OVERRIDE ABSENT --
                 the shipped `(2*srtt).clamp(25, 100) ms` clamp
    Q002  0.002  |  Q009  0.009  |  Q050  0.05  |  Q184  0.184  |  Q400  0.40

SECTION ORDER IS PART OF THE CONTRACT, and it is: the VERDICT first, then the
LIVENESS AND ABORT accounting that licenses it, then HEADROOM, then the
REALIZED-W distributions and the SEPARATION RULE, then the cost curve, then the
contract-priced score, then the false alarms, then what none of it establishes.
A reader who stops after the verdict has read the one line the battery is for;
a reader who stops before the liveness table has read a number they are not
entitled to.

THREE RULES THIS REPORTER ENFORCES AND DOES NOT SOFTEN:

  W6 -- THE ARM'S OWN INDEPENDENT VARIABLE. A treatment row must echo its
  COMMANDED alpha on both `[GATES]` lines AND resolve to it in both `[QALPHA]`
  lines; a CTL row must read `unset` on both gates and `quantile=0` at both
  sites. **A row failing W6 is VOID -- its own independent variable did not
  take** -- and a VOID row enters no denominator anywhere below. This is
  MEASUREMENT DISCIPLINE 1 for a sweep: an env var that was read is not a dial
  that reached the law.

  THE SEPARATION RULE. `W(alpha) = srtt + k(alpha)*sigma` is commanded by alpha
  and REALIZED through sigma, and sigma is not a constant -- the plain-window
  pass measured sigma(c8) at 0.191 / 3.140 / 54.836 ms across three reps at
  n ~ 18 000, a 287x spread at converged sample count. So W is a DISTRIBUTION,
  and two arms whose realized W intervals overlap are NOT two arms whatever
  their labels say. Overlap > 0.50 on a pair is declared UNSEPARATED, and an
  unseparated pair supports no comparison between its members.

  W3 IS RETIRED. `cod=0sym/s` does not appear anywhere in this file.
"""
import json
import math
import sys
from collections import defaultdict

# ── THE ARMS. `alpha_cmd` is carried on every row; this table is the
#    INDEPENDENT recomputation the W6 witness checks the rows against.
ARMS = ["CTL", "Q002", "Q009", "Q050", "Q184", "Q400"]
ARM_ALPHA = {"CTL": None, "Q002": 0.002, "Q009": 0.009,
             "Q050": 0.05, "Q184": 0.184, "Q400": 0.40}
CTL = "CTL"

# ── THE QUANTILE-NATIVE AXIS (`RWM_W_FORM`), and its own liveness witness ──
#
# ADDITIVE. Every column and section above and below is unchanged; a ledger from
# the CANTELLI era carries none of these fields, `w7()` returns `None` for it,
# and the W7 column prints `n/a` rather than a failure. A witness that fails on
# ledgers written before it existed is a harness bug reported as a result.
#
# `RWM_W_FORM` is a WORD, not a flag: `cantelli` | `quantile`, read ONLY when
# `RWM_QUANTILE_CLOCKS=1`, and ABSENT or GARBAGE resolves to `cantelli`. The CTL
# arm therefore carries NO TOKEN in its env and EXPECTS `cantelli` in the echo.
ARM_WFORM = {"CTL": "cantelli", "Q002": "quantile", "Q009": "quantile",
             "Q050": "quantile", "Q184": "quantile", "Q400": "quantile"}
#: The arm's expected quantile-window sample count, as the ENGINE prints it.
#: `unavail` on CTL is a REACHABILITY FACT — there is no quantile window to size
#: — and never a zero. Compared as STRINGS, against the engine's own token.
ARM_WINN = {"CTL": "unavail", "Q002": "5000", "Q009": "1112",
            "Q050": "200", "Q184": "55", "Q400": "25"}

# ── PRE-DERIVED, PRE-REGISTERED, AND NOT RECOMPUTED HERE ─────────────────────
#
# THE ARMS ARE PARTLY UNSEPARATED BEFORE A SINGLE PACKET IS SENT, and that is a
# property of the WINDOW SIZES, not of the data. A quantile window of N samples
# with K = 10 order statistics in the tail realizes a tail level whose exact
# 95 % interval is Beta(K, N-K+1). Two arms whose intervals touch cannot be
# separated by ANY amount of measurement at these window sizes.
#
# TRANSCRIBED AS LITERAL CONSTANTS ON PURPOSE. Recomputing them here would make
# the report agree with itself by construction; they belong to the
# pre-registration and this file only reads them.
ARM_WINDOW_N = {"Q002": 5000, "Q009": 1112, "Q050": 200, "Q184": 55, "Q400": 25}
# 95 % CI on the realized tail level, exact Beta(K, N-K+1), K=10
ARM_TAU_CI = {"Q002": (0.000959, 0.003414), "Q009": (0.004321, 0.015308),
              "Q050": (0.024234, 0.083703), "Q184": (0.090791, 0.288030),
              "Q400": (0.211255, 0.574794)}
UNSEPARATED_BY_CONSTRUCTION = [("Q184", "Q400")]   # margin 0.733
MARGINAL_BY_CONSTRUCTION = [("Q050", "Q184")]      # margin 1.085 — the thin one
#: The ordered arms of the construction table, low alpha first. The margin of a
#: pair is `lo(higher arm) / hi(lower arm)`: above 1 the intervals are disjoint.
CONSTRUCTION_ORDER = ["Q002", "Q009", "Q050", "Q184", "Q400"]

#: The cells, TRANSCRIBED from `ccand_battery.sh:202-215`'s own `cell_spec`.
CELLS = ["c1", "c7", "c8", "c8L", "sc2"]
#: `prim_battery_pw.sh:73` -- c1 is the only cell exempt from the W4'/W5 lower
#: bound (realised loss 0.013 %). Transcription, not inference.
LOSSY = {c: (c != "c1") for c in CELLS}

#: Shaped capacity per cell, bits/s, from the cells' OWN definitions
#: (`lib.sh scenario_params`; dual cells SUM their legs). Discipline 16a.
SHAPED_BPS = {
    "c1": 1_000_000_000,      # single, 1000 Mbit
    "c7": 200_000_000,        # dual, 2 x 100 Mbit
    "c8": 120_000_000,        # dual, 100 + 20 Mbit
    "c8L": 120_000_000,       # dual, 100 + 20 Mbit
    "sc2": 100_000_000,       # single, 100 Mbit
}
HEADROOM_BAR = 5.0            # discipline 16c: below this, no throughput target

#: THE OVERLAP BAR. A pair above it is UNSEPARATED and supports no comparison.
OVERLAP_BAR = 0.50

#: RACK's own published false-alarm budget, RFC 8985 6.2 Step 4: 1/16.
FA_CLASS = 0.0625

#: THE CONTRACT'S OWN THREE NAMED DIAL POINTS, and NO NEW CONSTANT IS
#: INTRODUCED BY USING THEM. `scheduler/mod.rs:47` COPA_DELTA = 0.5 and
#: `delta(hint) = COPA_DELTA / zeta(hint)` with zeta in {0.01, 1, 100}
#: (`ProtocolHint::tail_loss_scale`), so the three named points are exactly:
DELTAS = [("Bulk", 0.005), ("Auto", 0.5), ("Realtime", 50.0)]
#: The hint the harness actually runs. The other two are a DECLARED-DIAL
#: SENSITIVITY, printed so a reader can see how much of any verdict is the
#: dial's choice rather than the data's.
HARNESS_DELTA = ("Bulk", 0.005)

#: THE VERDICT MAP, TRANSCRIBED FROM THE PRE-REGISTRATION AND NOT FROM THE MEMO.
#:
#: goal-gate "THE ALPHA-SWEEP -- PRE-REGISTRATION" section 3 recomputes both
#: routes on the PLAIN-WINDOW MEASURED inputs at c8 (nu = 0.03776, d = 3.298 ms,
#: p = 0.011215, sigma = 3.140 ms), and the answer is dial-point dependent:
#:
#:   route (d), alpha^1.5 (1-alpha)^0.5 = delta*p*sigma / (2*nu*d):
#:        Bulk  (delta 0.005)  -> alpha = 0.0080   -> Q009
#:        Auto  (delta 0.5)    -> alpha = 0.1829   -> Q184
#:        Realtime (delta 50)  -> CORNER, alpha -> 1 (no interior optimum)
#:   route (b), alpha = sigma^2 / (sigma^2 + D(delta)^2), D = b(delta)*RTprop:
#:        Bulk                 -> alpha = 0.00170  -> Q002
#:        Auto                 -> alpha = 0.00678  -> Q009
#:        Realtime             -> alpha = 0.02659
#:
#: THE HARNESS RUNS THE BULK HINT ON EVERY INVOCATION, so the scored map is the
#: Bulk row: (b) -> Q002, (d) -> Q009. At AUTO the two routes coincide (0.1829
#: vs 0.1843, 0.8 % apart) and BOTH land on Q184 -- which is why the
#: pre-registration says in as many words that the route question is answerable
#: at Bulk and NOWHERE ELSE in this sweep. `Q184` therefore maps to neither
#: route at the scored delta, and an alpha* landing there is `BOTH REFUTED`,
#: correctly: the Auto point is not the point this battery ran.
ARM_OF_OPTION = {"Q002": "(b) WINS", "Q009": "(d) WINS"}

#: The same map at the two dial points the battery REPORTS but does not RUN,
#: printed as a sensitivity so a reader can see how much of the verdict is the
#: dial's choice. Auto is deliberately degenerate -- see above.
ARM_OF_OPTION_BY_DELTA = {
    "Bulk": {"Q002": "(b)", "Q009": "(d)"},
    "Auto": {"Q184": "(b) AND (d) -- DEGENERATE, the routes coincide here"},
    "Realtime": {"Q050": "(b)"},
}


# ── stdlib-only statistics. `ccand_report.py`'s definitions, kept. ───────
def med(v):
    v = sorted(x for x in v if x is not None)
    if not v:
        return None
    n = len(v)
    return v[n // 2] if n % 2 else (v[n // 2 - 1] + v[n // 2]) / 2.0


def pct(v, p):
    """Nearest-rank on the sorted survivors, clamped -- `ccand_parse.q`'s
    estimator, so a percentile computed here pools with one computed there."""
    v = sorted(x for x in v if x is not None)
    if not v:
        return None
    return v[min(len(v) - 1, int(round(p * (len(v) - 1))))]


def mean(v):
    v = [x for x in v if x is not None]
    return sum(v) / len(v) if v else None


def sd(v):
    v = [x for x in v if x is not None]
    if len(v) < 2:
        return None
    m = sum(v) / len(v)
    return math.sqrt(sum((x - m) ** 2 for x in v) / (len(v) - 1))


def fmt(x, p=1):
    return "-" if x is None else f"{x:.{p}f}"


def fmti(x):
    return "-" if x is None else f"{int(x)}"


def safediv(a, b):
    if a is None or b is None or b == 0:
        return None
    return a / b


def safeln(a, b):
    """ln(a/b), or None. Guarded on BOTH ends: a zero or negative goodput is a
    dead rep, not a minus-infinity utility."""
    r = safediv(a, b)
    if r is None or r <= 0.0:
        return None
    return math.log(r)


# ── LOAD ─────────────────────────────────────────────────────────────────
rows = []
for path in [a for a in sys.argv[1:] if not a.startswith("--")]:
    try:
        f = open(path, errors="replace")
    except OSError:
        print(f"  (could not open {path} -- skipped)")
        continue
    with f:
        for ln in f:
            # THE `ALPHARESULT ` PREFIX IS LOAD-BEARING AND IS TRIED FIRST.
            # `alpha_battery.sh:263` writes an `ALPHAWITNESS {"cell":...}` row
            # into the SAME ledger, and a bare `{"cell"` scan would parse those
            # as results: they carry no `gates_lines_*` and no `n_runs`, so
            # every one of them would land in the ABORT count and inflate it by
            # exactly the number of successful invocations. The prefix is what
            # separates the two, and a witness row is skipped explicitly rather
            # than left to a field test that a future column could satisfy.
            i = ln.find("ALPHARESULT ")
            if i >= 0:
                j = ln.find("{", i)
            elif "ALPHAWITNESS" in ln:
                continue
            else:
                j = ln.find('{"cell"')
            if j < 0:
                continue
            try:
                rows.append(json.loads(ln[j:]))
            except ValueError:
                pass


def is_abort(r):
    """No `[GATES]` on EITHER endpoint and no run: the invocation died before
    the engine started. Contributes no datum. `lat_report.is_abort`, verbatim
    -- a DNF is a transfer that RAN and did not finish."""
    return (not r.get("gates_lines_cli") and not r.get("gates_lines_srv")
            and not r.get("n_runs"))


def tok_alpha(t):
    """The `[GATES] RWM_ALPHA_OVERRIDE=` token as a number, or None for
    `unset` / absent / garbage."""
    if t is None or t == "unset":
        return None
    try:
        return float(t)
    except ValueError:
        return None


def w6(r):
    """W6 -- THE ARM'S OWN ARM-LIVENESS WITNESS. Returns (ok, why).

    A treatment arm's row must echo its COMMANDED alpha on BOTH `[GATES]`
    lines and RESOLVE to it at BOTH `[QALPHA]` sites, to 1e-9. A CTL row must
    read `unset` on both gates and `quantile=0` at both sites.

    A row failing this is VOID. Not "excluded from one statistic" -- VOID: its
    own independent variable did not take, so it is not a rep of the arm whose
    name it carries, and folding it in would average two treatments under one
    label. This is the sweep's form of MEASUREMENT DISCIPLINE 1."""
    a = r.get("arm")
    want = ARM_ALPHA.get(a, r.get("alpha_cmd"))
    gc, gs = tok_alpha(r.get("gate_alpha_cli")), tok_alpha(r.get("gate_alpha_srv"))
    qc, qs = r.get("qalpha_cli"), r.get("qalpha_srv")
    if want is None:
        if r.get("gate_alpha_cli") != "unset" or r.get("gate_alpha_srv") != "unset":
            return False, "gate not unset"
        if r.get("qalpha_quantile_cli") != 0 or r.get("qalpha_quantile_srv") != 0:
            return False, "quantile!=0"
        return True, ""
    for v, why in ((gc, "gate_cli"), (gs, "gate_srv")):
        if v is None or abs(v - want) > 1e-9:
            return False, why
    for v, why in ((qc, "QALPHA_cli"), (qs, "QALPHA_srv")):
        if v is None or abs(v - want) > 1e-9:
            return False, why
    return True, ""


def w7(r):
    """W7 -- THE WINDOW-FORM ARM-LIVENESS WITNESS. Returns (ok, why), where
    `ok` is `None` for NOT APPLICABLE.

    `form` must equal the arm's expected token at BOTH endpoints. `win_n` is
    asserted at the SENDER always and at the RECEIVER on a Q arm ONLY, and that
    asymmetry is a documented property of the engine rather than a softened
    gate: the protocol hint is NOT plumbed to the receiver task -- the same fact
    that makes CTL's two `[QALPHA]` sites disagree about the contract alpha --
    so an UNOVERRIDDEN receiver resolves a different contract alpha and
    therefore a different window size. On a Q arm both sites carry a NUMBER
    override, so both must read the expected `win_n`.

    A LEDGER FROM BEFORE `RWM_W_FORM` EXISTED CARRIES NONE OF THESE FIELDS and
    returns `None` -- NOT a failure. A witness that fails on ledgers written
    before it existed reports a harness fact as a result.

    W7 VOIDS A ROW, EXACTLY AS W6 DOES, AND THE RULE IS MECHANICAL RATHER THAN
    A JUDGEMENT CALL. The quantile-native pre-registration's section 9 states
    it: a row whose W FORM did not take is not a rep of the arm whose name it
    carries, any more than a row whose alpha did not take is. Leaving the
    consequence to the reader would put a scoring decision after the data, and
    the whole point of pre-registering the witness is that it is applied the
    same way whichever direction it fires.

    `None` (a pre-`RWM_W_FORM` ledger) NEVER voids: it is the ABSENCE of the
    axis, not a failure on it, and the CANTELLI-era ledgers this file still
    reads must score exactly as they did before."""
    a = r.get("arm")
    wf, wn = ARM_WFORM.get(a), ARM_WINN.get(a)
    fc, fs = r.get("qalpha_form_cli"), r.get("qalpha_form_srv")
    nc, ns = r.get("qalpha_winn_cli"), r.get("qalpha_winn_srv")
    if wf is None or wn is None:
        return None, "unknown arm"
    if fc is None and fs is None and nc is None and ns is None:
        return None, "n/a"          # a pre-RWM_W_FORM ledger
    if fc != wf:
        return False, "form_cli"
    if fs != wf:
        return False, "form_srv"
    if nc is None or str(nc) != wn:
        return False, "win_n_cli"
    if a != CTL and (ns is None or str(ns) != wn):
        return False, "win_n_srv"
    return True, ""


ABORTS = [r for r in rows if is_abort(r)]
LIVEROWS = [r for r in rows if not is_abort(r)]
for r in LIVEROWS:
    r["_w6"], r["_w6why"] = w6(r)
    r["_w7"], r["_w7why"] = w7(r)

def _took(r):
    """Did this row's arm actually take, on BOTH of its axes? `_w7 is None` is
    a pre-`RWM_W_FORM` ledger and is not a failure — see `w7`."""
    return bool(r["_w6"]) and r["_w7"] is not False


VOID = [r for r in LIVEROWS if not _took(r)]
SCORED = [r for r in LIVEROWS if _took(r)]

by_all = defaultdict(list)      # every live row, void included: the accounting
by = defaultdict(list)          # SCORED rows only: everything else
for r in LIVEROWS:
    by_all[(r.get("cell"), r.get("arm"))].append(r)
for r in SCORED:
    by[(r.get("cell"), r.get("arm"))].append(r)

#: Cells and arms ACTUALLY present, in the canonical order, with anything
#: unexpected appended rather than dropped -- a ledger carrying a cell this
#: table does not know about must still be visible.
PCELLS = [c for c in CELLS if any(k[0] == c for k in by_all)]
PCELLS += sorted({k[0] for k in by_all} - set(CELLS))
PARMS = [a for a in ARMS if any(k[1] == a for k in by_all)]
PARMS += sorted({k[1] for k in by_all} - set(ARMS))


# ── SECTION 4's ARITHMETIC, computed here because the VERDICT needs it ───
def arm_w_interval(cell, arm):
    """The arm's REALIZED-W interval at a cell: the MEDIAN OVER REPS of the
    per-rep p05 and p95. Median over reps, never a pooled percentile of
    pooled samples -- a rep is the unit the sweep randomised."""
    rs = by.get((cell, arm), [])
    lo = med([r.get("qclk_cli_w_us_p05") for r in rs])
    hi = med([r.get("qclk_cli_w_us_p95") for r in rs])
    return lo, hi


def overlap(a, b):
    """|[lo_a,hi_a] ^ [lo_b,hi_b]| / min(width_a, width_b), or None.

    Normalising by the NARROWER width is what makes the statistic answer the
    question actually asked: "could these two arms be the same treatment?" A
    narrow interval fully inside a wide one reads 1.0 and is UNSEPARATED, which
    is correct -- every W the narrow arm realized, the wide arm realized too."""
    if None in a or None in b:
        return None
    wa, wb = a[1] - a[0], b[1] - b[0]
    inter = max(0.0, min(a[1], b[1]) - max(a[0], b[0]))
    denom = min(wa, wb)
    if denom <= 0:
        # A degenerate (zero-width) interval: separated iff it sits outside the
        # other. Stated rather than divided by zero.
        return 1.0 if inter > 0 or (a[0] == b[0]) else 0.0
    return inter / denom


PAIRS = {}          # (cell, armA, armB) -> (overlap, separated?)
for c in PCELLS:
    for i, a in enumerate(PARMS):
        for b in PARMS:
            if a == b:
                continue
            ov = overlap(arm_w_interval(c, a), arm_w_interval(c, b))
            PAIRS[(c, a, b)] = (ov, (None if ov is None else ov <= OVERLAP_BAR))


# ── SECTION 6's ARITHMETIC, likewise ─────────────────────────────────────
def du_rows(cell, arm, delta):
    """Per-rep dU against the CELL's OWN CTL, using Copa's declared utility
    `U = log(throughput) - delta*log(delay)`:

        dU = ln(mbps(arm)/mbps(CTL)) - delta * ln(ping_p95(arm)/ping_p95(CTL))

    The CTL reference is the cell's MEDIAN over CTL reps, so a per-rep dU is a
    treatment rep against a stable baseline rather than an arbitrary pairing.
    Rows missing either term contribute NOTHING -- they are not zeroes."""
    ctl = by.get((cell, CTL), [])
    m0 = med([r.get("mean_mbps", r.get("mbps")) for r in ctl])
    p0 = med([r.get("ping_p95") for r in ctl])
    out = []
    for r in by.get((cell, arm), []):
        lm = safeln(r.get("mean_mbps", r.get("mbps")), m0)
        lp = safeln(r.get("ping_p95"), p0)
        if lm is None or lp is None:
            continue
        out.append(lm - delta * lp)
    return out


DU = {}             # (cell, arm, delta_name) -> (median dU, sd, n)
for c in PCELLS:
    for a in PARMS:
        for dn, dv in DELTAS:
            v = du_rows(c, a, dv)
            DU[(c, a, dn)] = (med(v), sd(v), len(v))

POOLED_SD = {}      # (cell, delta_name) -> pooled within-arm sd of per-rep dU
BEST = {}           # (cell, delta_name) -> arm maximising dU
for c in PCELLS:
    for dn, _ in DELTAS:
        var = [DU[(c, a, dn)][1] ** 2 for a in PARMS
               if DU[(c, a, dn)][1] is not None]
        POOLED_SD[(c, dn)] = (math.sqrt(sum(var) / len(var)) if var else None)
        cand = [(DU[(c, a, dn)][0], a) for a in PARMS
                if a != CTL and DU[(c, a, dn)][0] is not None]
        BEST[(c, dn)] = (max(cand)[1] if cand else None)


# ── THE VERDICT, COMPUTED. Never hard-coded; see ARM_OF_OPTION. ──────────
def compute_verdict():
    """Returns (verdict, reasons[]).

    THE SEPARATION GATE COMES FIRST and it can only produce NO VERDICT. If more
    than half the scored ordered pairs at the cells that have data are
    UNSEPARATED, the arms are not arms and nothing downstream is entitled to a
    winner -- that is the whole point of measuring realized W rather than
    trusting the commanded label.

    Then FLAT CURVE: if no treatment arm's dU clears the pooled within-arm 2
    sigma at any cell, the curve is flat over the swept range and the answer is
    that alpha did not matter here -- a legal, pre-registered outcome and not a
    failure.

    Then the winner, by MAJORITY OF CELLS on `alpha*` at the harness's own
    delta. An `alpha*` that is neither (b)'s nor (d)'s arm refutes both."""
    reasons = []
    if not SCORED:
        return "NO VERDICT -- NO SCORED ROWS", [
            "The ledger carries no row that passes W6. Nothing is scored.",
        ]
    judged = [(k, v) for k, v in PAIRS.items() if v[1] is not None]
    if not judged:
        return "NO VERDICT -- UNSEPARATED", [
            "No pair of arms carries a realized-W interval at any cell "
            "([QCLK] never reported), so separation cannot be established "
            "and no arm may be compared to another.",
        ]
    unsep = sum(1 for _, v in judged if v[1] is False)
    frac = unsep / len(judged)
    reasons.append(f"separation: {unsep}/{len(judged)} ordered arm pairs "
                   f"UNSEPARATED (overlap > {OVERLAP_BAR:.2f}) = {frac:.2f}")
    if frac > 0.50:
        return "NO VERDICT -- UNSEPARATED", reasons + [
            "More than half the judged pairs realize overlapping W. The arms "
            "are labels, not treatments, and no ordering between them is a "
            "measurement.",
        ]
    dn, _dv = HARNESS_DELTA
    moved = []
    for c in PCELLS:
        s = POOLED_SD[(c, dn)]
        for a in PARMS:
            if a == CTL:
                continue
            d = DU[(c, a, dn)][0]
            if d is not None and s is not None and abs(d) > 2.0 * s:
                moved.append((c, a))
    reasons.append(f"effect: {len(moved)} (cell, arm) cells clear 2 sigma "
                   f"pooled within-arm on dU at delta = {_dv} ({dn})")
    if not moved:
        return "FLAT CURVE", reasons + [
            "No treatment arm's dU clears the pooled within-arm 2 sigma at any "
            "cell. Over the swept range alpha did not move the contract-priced "
            "utility. This is a LEGAL OUTCOME, pre-registered as one.",
        ]
    tally = defaultdict(int)
    for c in PCELLS:
        b = BEST[(c, dn)]
        if b is not None:
            tally[b] += 1
    if not tally:
        return "NO VERDICT -- UNSEPARATED", reasons + [
            "No cell produced an alpha* at the harness delta.",
        ]
    top = max(tally.items(), key=lambda kv: (kv[1], kv[0]))
    reasons.append("alpha* by cell: " + ", ".join(
        f"{c}={BEST[(c, dn)]}" for c in PCELLS) +
        f"  -> majority {top[0]} ({top[1]}/{len(PCELLS)})")
    return ARM_OF_OPTION.get(top[0], "BOTH REFUTED"), reasons


VERDICT, VREASONS = compute_verdict()

# ── 1. THE VERDICT, FIRST ────────────────────────────────────────────────
print("=" * 100)
print("THE ALPHA-SWEEP -- SCORING PASS   (goal #100 item 2)")
print("6 arms x up to 5 cells, paired, arms innermost.  arm -> commanded alpha:")
print("  " + "  ".join(f"{a}={('null' if ARM_ALPHA[a] is None else ARM_ALPHA[a])}"
                        for a in ARMS))
print("=" * 100)
print()
print("### 1. VERDICT")
print()
print(f"  VERDICT: {VERDICT}")
print()
for r in VREASONS:
    print(f"    - {r}")
print()
print("  THE FOUR PRE-REGISTERED LEGAL OUTCOMES, and the fifth thing that is")
print("  not an outcome at all:")
print("    (d) WINS       alpha* is the SYMMETRIC / POWER route's arm at the")
print("                   HARNESS'S OWN dial point (Bulk): Q009, alpha = 0.0080")
print("                   on the plain-window measured inputs")
print("    (b) WINS       alpha* is the LATENCY-CONSERVATIVE route's arm at the")
print("                   same dial point: Q002, alpha = 0.00170 on the same")
print("                   measured sigma")
print("    BOTH REFUTED   alpha* is neither -- the curve peaks somewhere the")
print("                   cost-ratio memo did not name, and both mappings are")
print("                   wrong about WHERE, not merely about how much")
print("    FLAT CURVE     no arm clears the noise: alpha did not matter here")
print("    NO VERDICT --  the arms' REALIZED W overlap. Two arms commanded at")
print("    UNSEPARATED    different alpha that realize the same clock are one")
print("                   arm run twice, and no ordering between them exists")
print("                   to be measured. This outranks every number below.")
print()
print("  THE MAP FROM alpha* TO A ROUTE IS TRANSCRIBED FROM THE PRE-REGISTRATION")
print("  (goal-gate 'THE ALPHA-SWEEP -- PRE-REGISTRATION' section 3), NOT from")
print("  the memo: both routes are recomputed there on the PLAIN-WINDOW MEASURED")
print("  inputs, and the answer is dial-point dependent. At BULK -- the hint")
print("  every invocation runs -- (b) is Q002 and (d) is Q009. At AUTO the two")
print("  routes COINCIDE at 0.1829 vs 0.1843 and both land on Q184, so the route")
print("  question is answerable AT BULK AND NOWHERE ELSE in this sweep. An")
print("  alpha* at Q184 is therefore BOTH REFUTED at the scored delta, correctly:")
print("  the Auto point is not the point this battery ran.")
print("  The OUTCOME above is COMPUTED from the rows. It is the pre-registration")
print("  that decides what it means, never this file.")

# ── 2. LIVENESS AND ABORTS ───────────────────────────────────────────────
print()
print("=" * 100)
print("### 2. LIVENESS AND ABORTS, read before any number.")
print("=" * 100)
print()
print(f"  invocations={len(rows)}  live={len(LIVEROWS)}  ABORTS={len(ABORTS)}"
      f"  VOID(W6 failed)={len(VOID)}  SCORED={len(SCORED)}")
print("  ABORT = no [GATES] on EITHER endpoint and no run: the invocation died")
print("  before the engine started, and it is in NO denominator anywhere.")
print("  VOID  = live, but the arm's OWN independent variable did not take.")
print()
_W4H = "W4'"
print(f"  {'cell-arm':<10} {'rows':>5} {'ABORT':>6} {'live':>5} {'VOID':>5} "
      f"{'DNF':>4} {'W1':>7} {'W2':>7} {_W4H:>8} {'W5':>9} {'W6':>7} {'W7':>7}")
LIVENESS_CLEAN = True
for c in PCELLS:
    for a in PARMS:
        allr = [r for r in rows if r.get("cell") == c and r.get("arm") == a]
        if not allr:
            continue
        lv = by_all[(c, a)]
        n = len(lv)
        vd = sum(1 for r in lv if not _took(r))
        dnf = sum(1 for r in lv if r.get("dnf"))
        # W1: the RECEIVER's [RFA] gen= field -- the only DIRECT echo of
        # window_generation. This battery runs RWM_GEN=0, so gen=0 is the pass.
        w1 = sum(1 for r in lv if r.get("w1_rfa_gen") == "0")
        # W2: [PFRAC] presence on the sender IS generation. 0 lines is the pass.
        w2 = sum(1 for r in lv if r.get("w2_pfrac_lines") == 0)
        # W4': THE MAXIMUM retx OVER ALL [DIAG] LINES, never the last -- see
        # alpha_parse.py. The gap-driven retransmit loop must have run at every
        # LOSSY cell; c1 is exempt (realised loss 0.013 %).
        if LOSSY.get(c, True):
            w4 = sum(1 for r in lv if (r.get("w4_retx_max") or 0) > 0)
            w4s = f"{w4}/{n}"
        else:
            w4s = "n/a(c1)"
        # W5: record_fire's only call site -- fired > 0 at every LOSSY cell.
        if LOSSY.get(c, True):
            w5 = sum(1 for r in lv if (r.get("rack_fired") or 0) > 0)
            w5s = f"{w5}/{n}"
        else:
            w5s = "n/a(c1)"
        w6n = n - vd
        # W7: `form` AND `win_n` agreement at both endpoints against the arm's
        # own expectation. A ledger predating `RWM_W_FORM` yields `None` at
        # every row and prints `n/a` -- absence of the axis, not failure on it.
        w7j = [r for r in lv if r["_w7"] is not None]
        if not w7j:
            w7s = "n/a"
        else:
            w7s = f"{sum(1 for r in w7j if r['_w7'])}/{len(w7j)}"
            if any(not r["_w7"] for r in w7j):
                LIVENESS_CLEAN = False
        if vd or (LOSSY.get(c, True) and w6n and w1 != n):
            LIVENESS_CLEAN = False
        print(f"  {c + '-' + a:<10} {len(allr):>5} {len(allr) - n:>6} {n:>5} "
              f"{vd:>5} {dnf:>4} {f'{w1}/{n}':>7} {f'{w2}/{n}':>7} {w4s:>8} "
              f"{w5s:>9} {f'{w6n}/{n}':>7} {w7s:>7}")
if not by_all:
    print("  (no live rows in this ledger)")
print()
print("  W1  [RFA] gen=0 on the RECEIVER -- the only DIRECT echo of")
print("      window_generation. This battery runs RWM_GEN=0.")
print("  W2  no [PFRAC] lines on the sender -- [PFRAC] presence IS generation.")
print("  W4' [DIAG] retx > 0 at every LOSSY cell, read as the MAXIMUM over ALL")
print("      [DIAG] lines. `retx=` in the DIAG tail is an INTERVAL counter, and")
print("      reading it off the LAST line mis-reported this witness at 5 of 15")
print("      reps in the plain-window primitives pass.")
print("  W5  [RACK] fa=<spur>/<fired> with fired > 0 at every LOSSY cell.")
print("  W6  THIS BATTERY'S OWN ARM-LIVENESS WITNESS, and it is the one that")
print("      can void a row. Treatment arm: gate_alpha_cli AND gate_alpha_srv")
print("      equal the commanded alpha, AND qalpha_cli / qalpha_srv equal it to")
print("      1e-9. CTL: both gates read `unset` and qalpha_quantile_* read 0.")
print("      A ROW FAILING W6 IS VOID -- its own independent variable did not")
print("      take, so it is not a rep of the arm whose name it carries.")
print("  W7  THE WINDOW-FORM WITNESS, for the QUANTILE-NATIVE axis. `[QALPHA]")
print("      form=` must equal the arm's expected token (`cantelli` on CTL,")
print("      where RWM_W_FORM is ABSENT and the engine RESOLVES it, `quantile`")
print("      on every Q arm) at BOTH endpoints, and `win_n=` must equal the")
print("      arm's expected window size. `win_n` IS ASSERTED AT THE SENDER")
print("      ALWAYS AND AT THE RECEIVER ON A Q ARM ONLY: the protocol hint is")
print("      not plumbed to the receiver task, so an UNOVERRIDDEN CTL receiver")
print("      resolves a different contract alpha and therefore a different")
print("      window. On a Q arm BOTH sites carry a NUMBER override and both")
print("      are checked. `n/a` means the ledger predates the axis -- absence")
print("      of the axis, never failure on it. W7 DOES NOT VOID A ROW: it is")
print("      reported here and the pre-registration decides what it costs.")
print("  W3 (cod=) IS RETIRED and appears nowhere in this report.")
_W7BAD = [r for r in LIVEROWS if r["_w7"] is False]
if _W7BAD:
    print()
    print("  W7 FAILURES, with the field that failed:")
    seen7 = defaultdict(int)
    for r in _W7BAD:
        seen7[(r.get("cell"), r.get("arm"), r["_w7why"])] += 1
    for (c, a, why), k in sorted(seen7.items()):
        print(f"    {c + '-' + a:<10} {why:<14} x{k}")
if VOID:
    print()
    print("  VOID ROWS, with the field that failed:")
    seen = defaultdict(int)
    for r in VOID:
        seen[(r.get("cell"), r.get("arm"), r["_w6why"])] += 1
    for (c, a, why), k in sorted(seen.items()):
        print(f"    {c + '-' + a:<10} {why:<14} x{k}")
if not LIVENESS_CLEAN:
    print()
    print("  *** LIVENESS IS NOT CLEAN. Every number below rests on it. ***")

# ── 3. HEADROOM ──────────────────────────────────────────────────────────
print()
print("=" * 100)
print("### 3. HEADROOM (MEASUREMENT DISCIPLINE 16) -- the CTL arm's own")
print("###    utilisation of each cell's shaped capacity, from the qdisc capture.")
print("=" * 100)
print()
print("  DENOMINATOR = THE TRANSFER WALL (`seconds`), NEVER `INVOCATION_S`:")
print("  INVOCATION_S is the whole script's wall (namespace bring-up, netem/tbf")
print("  setup, the verification pings, teardown) and dividing shaped-device")
print("  bytes by it understates utilisation enough to LICENSE an unsatisfiable")
print("  throughput target -- which is exactly what discipline 16 forbids.")
print()
print(f"  {'cell':<6} {'shaped':>10} {'xfer_s':>8} {'INVOC_S':>8} {'util %':>8} "
      f"{'headroom %':>11}   claims permitted")
for c in PCELLS:
    rs = by.get((c, CTL), [])
    u = [100.0 * r["tc_bytes"] * 8.0 / (r["seconds"] * SHAPED_BPS[c])
         for r in rs
         if r.get("tc_bytes") and r.get("seconds") and SHAPED_BPS.get(c)]
    util = med(u)
    hr = None if util is None else 100.0 - util
    if hr is None:
        claim = "(no tc datum -- headroom UNKNOWN, no throughput target scoreable)"
    elif hr < HEADROOM_BAR:
        claim = "NO-THROUGHPUT-TARGET -- headroom < 5% (discipline 16c)"
    else:
        claim = "throughput targets permitted"
    cap = SHAPED_BPS.get(c)
    print(f"  {c:<6} {(fmti(cap // 1_000_000) + ' Mb') if cap else '-':>10} "
          f"{fmt(med([r.get('seconds') for r in rs]), 2):>8} "
          f"{fmt(med([r.get('tc_s') for r in rs]), 1):>8} "
          f"{fmt(util):>8} {fmt(hr):>11}   {claim}")
if not any(by.get((c, CTL)) for c in PCELLS):
    print("  (no scored CTL row -- headroom cannot be computed)")

# ── 3b. UNSEPARATED-BY-CONSTRUCTION, READ BEFORE THE SEPARATION RULE ─────
#
# THIS SECTION IS PRE-DERIVED AND IS NOT RECOMPUTED FROM THE ROWS. It comes
# BEFORE the empirical separation rule on purpose: some of what section 4 is
# about to measure was decided by the WINDOW SIZES before a packet was sent, and
# a reader who meets the empirical overlap first will read a construction as a
# finding.
print()
print("=" * 100)
print("### 3b. UNSEPARATED-BY-CONSTRUCTION (PRE-DERIVED)")
print("=" * 100)
print()
print("  A quantile window of N samples with K = 10 order statistics in the")
print("  tail realizes a TAIL LEVEL whose exact 95 % interval is Beta(K,")
print("  N-K+1). Where two arms' intervals TOUCH, no amount of measurement at")
print("  these window sizes can separate them: the arms are one arm run twice,")
print("  by construction and not by outcome.")
print()
print("  THESE NUMBERS ARE TRANSCRIBED FROM THE PRE-REGISTRATION AS LITERAL")
print("  CONSTANTS AND ARE NOT RECOMPUTED HERE. A report that re-derived them")
print("  would agree with itself by construction; the pre-registration owns")
print("  them and this file only reads them.")
print()
print(f"  {'pair':<12} {'N_lo':>6} {'N_hi':>6} {'tau CI (low arm)':>22} "
      f"{'tau CI (high arm)':>22} {'margin':>8}   label")
for _i in range(len(CONSTRUCTION_ORDER) - 1):
    _a, _b = CONSTRUCTION_ORDER[_i], CONSTRUCTION_ORDER[_i + 1]
    _ca, _cb = ARM_TAU_CI[_a], ARM_TAU_CI[_b]
    # margin = lo(higher arm) / hi(lower arm). > 1 = disjoint intervals.
    _m = safediv(_cb[0], _ca[1])
    if (_a, _b) in UNSEPARATED_BY_CONSTRUCTION:
        _lab = "UNSEPARATED-BY-CONSTRUCTION"
    elif (_a, _b) in MARGINAL_BY_CONSTRUCTION:
        _lab = "MARGINAL"
    else:
        _lab = "separated"
    _sa = "[%.6f, %.6f]" % _ca
    _sb = "[%.6f, %.6f]" % _cb
    print(f"  {_a + '-' + _b:<12} {ARM_WINDOW_N[_a]:>6} {ARM_WINDOW_N[_b]:>6} "
          f"{_sa:>22} {_sb:>22} {fmt(_m, 3):>8}   {_lab}")
print()
print("  margin = lo(higher arm) / hi(lower arm). Above 1 the two tail-level")
print("  intervals are DISJOINT; at or below 1 they overlap and the pair is")
print("  UNSEPARATED BEFORE THE EXPERIMENT RUNS.")
print()
for _a, _b in UNSEPARATED_BY_CONSTRUCTION:
    print(f"  *** {_a}-{_b} IS UNSEPARATED BY CONSTRUCTION. ***")
    print(f"      An empirical non-separation of {_a}-{_b} in section 4 is")
    print("      EXPECTED (no verdict may rest on it). It is not a finding, it")
    print("      is not evidence about alpha, and it is not a failure of the")
    print("      run -- it is the window sizes, restated by the data.")
for _a, _b in MARGINAL_BY_CONSTRUCTION:
    print(f"  {_a}-{_b} IS MARGINAL BY CONSTRUCTION -- the thin one. Its")
    print("      intervals are disjoint by a hair, so an empirical separation")
    print("      there carries far less weight than its overlap number looks")
    print("      like, and an empirical NON-separation is close to expected.")
_BYC = set(UNSEPARATED_BY_CONSTRUCTION)
if PCELLS:
    print()
    print("  THE PRE-DERIVED PAIRS AS THE ROWS ACTUALLY REALIZED THEM, marked:")
    for _c in PCELLS:
        for _a, _b in list(UNSEPARATED_BY_CONSTRUCTION) + list(MARGINAL_BY_CONSTRUCTION):
            _ov, _sep = PAIRS.get((_c, _a, _b), (None, None))
            if _ov is None:
                _note = "no realized-W interval at this cell"
            elif _sep:
                _note = "separated empirically"
            else:
                _note = ("UNSEPARATED empirically -- EXPECTED "
                         "(no verdict may rest on it)" if (_a, _b) in _BYC
                         else "UNSEPARATED empirically -- MARGINAL by construction")
            _ovs = "-" if _ov is None else "%.2f" % _ov
            print(f"    {_c:<5} {_a + '-' + _b:<12} overlap={_ovs:>6}   {_note}")

# ── 4. THE REALIZED-W DISTRIBUTIONS AND THE SEPARATION RULE ──────────────
print()
print("=" * 100)
print("### 4. THE REALIZED-W DISTRIBUTIONS AND THE SEPARATION RULE.")
print("=" * 100)
print()
print("  W(alpha) = srtt + k(alpha)*sigma is COMMANDED by alpha and REALIZED")
print("  through sigma, and sigma is not a constant: the plain-window pass")
print("  measured sigma(c8) at 0.191 / 3.140 / 54.836 ms across three reps at")
print("  n ~ 18 000 -- a 287x spread at CONVERGED sample count. So W is a")
print("  DISTRIBUTION, and a sweep that reads only the commanded alpha is")
print("  reading a LABEL, not the treatment.")
print()
print(f"  {'cell-arm':<10} {'n':>3} {'alpha':>7} {'k':>9} {'evals':>8} "
      f"{'w_p05 us':>9} {'w_p50 us':>9} {'w_p95 us':>9} {'p50 spread [min,max]':>24} "
      f"{'sigma us':>9}")
for c in PCELLS:
    for a in PARMS:
        rs = by.get((c, a), [])
        if not rs:
            continue
        p50s = [r.get("qclk_cli_w_us_p50") for r in rs if r.get("qclk_cli_w_us_p50") is not None]
        spread = (f"[{min(p50s):.0f}, {max(p50s):.0f}]" if p50s else "-")
        lo, hi = arm_w_interval(c, a)
        print(f"  {c + '-' + a:<10} {len(rs):>3} "
              f"{fmt(med([r.get('qalpha_cli') for r in rs]), 4):>7} "
              f"{fmt(med([r.get('qalpha_k_cli') for r in rs]), 4):>9} "
              f"{fmti(med([r.get('qclk_cli_evals') for r in rs])):>8} "
              f"{fmt(lo, 0):>9} "
              f"{fmt(med([r.get('qclk_cli_w_us_p50') for r in rs]), 0):>9} "
              f"{fmt(hi, 0):>9} {spread:>24} "
              f"{fmt(med([r.get('qclk_cli_sigma_us_mean') for r in rs]), 1):>9}")
print()
print("  [QCLK] evals = 0 (or an ABSENT line) is an UNREACHED EVALUATION SITE,")
print("  never W = 0: the gauge stays silent on Drop at zero evals precisely so")
print("  that absence can only be read as a reachability fact.")
print()
print("  THE PAIR MATRIX. overlap = |[p05_a,p95_a] ^ [p05_b,p95_b]| /")
print(f"  min(width_a, width_b), on each arm's MEDIAN-OVER-REPS p05 and p95.")
print(f"  A pair with overlap > {OVERLAP_BAR:.2f} is declared UNSEPARATED and")
print("  supports NO comparison between its two members, at any n.")
print()
for c in PCELLS:
    present = [a for a in PARMS if by.get((c, a))]
    if not present:
        continue
    print(f"  --- {c} ---")
    print("  " + " " * 8 + "".join(f"{b:>12}" for b in present))
    for a in present:
        cells = []
        for b in present:
            if a == b:
                cells.append(f"{'.':>12}")
                continue
            ov, sep = PAIRS.get((c, a, b), (None, None))
            if ov is None:
                cells.append(f"{'-':>12}")
            else:
                cells.append(f"{ov:>7.2f}{'  ok' if sep else ' UNSEP'}")
        print(f"  {a:<8}" + "".join(cells))
    print()

# ── 5. THE COST CURVE ────────────────────────────────────────────────────
print("=" * 100)
print("### 5. THE COST CURVE.")
print("=" * 100)
print()
print("  `q_p50` AND `ping_*` ARE DIFFERENT QUANTITIES AND ARE NEVER AVERAGED.")
print("  q_p50 is median(max(0, rtt - rtp)) computed BY THE CODE UNDER TEST from")
print("  the sender's OWN estimate of its OWN path -- the engine's self-reported")
print("  standing queue. ping_* is DELIVERED round-trip time for an unrelated")
print("  flow, measured by the kernel, through the WHOLE shaped path. They may")
print("  legitimately move in OPPOSITE directions, and nothing here promotes")
print("  either over the other.")
print()
print(f"  {'cell-arm':<10} {'n':>3} {'mbps p50':>9} {'mbps p05':>9} {'mbps p95':>9} "
      f"{'ping_p50':>9} {'ping_p95':>9} {'censor':>8} {'q_p50':>9}")
for c in PCELLS:
    for a in PARMS:
        rs = by.get((c, a), [])
        if not rs:
            continue
        mb = [r.get("mean_mbps", r.get("mbps")) for r in rs]
        cf = med([r.get("ping_censor_frac") for r in rs])
        cfs = "-" if cf is None else f"{100.0 * cf:.2f}%"
        print(f"  {c + '-' + a:<10} {len(rs):>3} {fmt(med(mb), 2):>9} "
              f"{fmt(pct(mb, 0.05), 2):>9} {fmt(pct(mb, 0.95), 2):>9} "
              f"{fmt(med([r.get('ping_p50') for r in rs])):>9} "
              f"{fmt(med([r.get('ping_p95') for r in rs])):>9} {cfs:>8} "
              f"{fmt(med([r.get('q_p50') for r in rs])):>9}")
print()
print("  `censor` is the fraction of probes that NEVER PRODUCED A SAMPLE. Every")
print("  censored sample is drawn from EXACTLY the worst states (the bad GE")
print("  state, the full queue), so a percentile over the survivors is BIASED")
print("  LOW -- in the direction that makes a latency claim look better than it")
print("  is. It is printed beside the percentile because a number whose error")
print("  bar points in a KNOWN direction and is not written down is not a")
print("  measurement (latt_probe.py).")
# Per-leg delivered latency, where the driver probed more than one leg.
_legmax = max([r.get("legs_probed") or 0 for r in SCORED], default=0)
if _legmax > 1:
    print()
    print("  PER LEG -- a two-leg system's delivered latency is NOT the mean of")
    print("  its legs, and the arms load the legs DIFFERENTLY.")
    print()
    hdr = "".join(f"{'leg%d p50' % i:>11}{'leg%d p95' % i:>11}{'leg%d cen' % i:>11}"
                  for i in range(_legmax))
    print(f"  {'cell-arm':<10}" + hdr)
    for c in PCELLS:
        for a in PARMS:
            rs = by.get((c, a), [])
            if not rs:
                continue
            cells = ""
            for i in range(_legmax):
                cf = med([r.get("leg%d_censor_frac" % i) for r in rs])
                cells += (f"{fmt(med([r.get('leg%d_p50' % i) for r in rs])):>11}"
                          f"{fmt(med([r.get('leg%d_p95' % i) for r in rs])):>11}"
                          + (f"{'-':>11}" if cf is None else f"{100.0 * cf:>10.2f}%"))
            print(f"  {c + '-' + a:<10}" + cells)

# ── 6. THE CONTRACT-PRICED SCORE ─────────────────────────────────────────
print()
print("=" * 100)
print("### 6. THE CONTRACT-PRICED SCORE -- Copa's OWN declared utility.")
print("=" * 100)
print()
print("    U  = log(throughput) - delta * log(delay)")
print("    dU = ln(mbps(arm)/mbps(CTL)) - delta * ln(ping_p95(arm)/ping_p95(CTL))")
print()
print("  NO NEW CONSTANT IS INTRODUCED. delta(hint) = COPA_DELTA / zeta(hint)")
print("  with COPA_DELTA = 0.5 (scheduler/mod.rs:47) and zeta in {0.01, 1, 100}")
print("  (ProtocolHint::tail_loss_scale), so the contract's OWN three named")
print("  points are Bulk 0.005, Auto 0.5, Realtime 50. The harness runs BULK;")
print("  the other two are a DECLARED-DIAL SENSITIVITY, printed so a reader can")
print("  see how much of the answer is the dial's choice rather than the data's.")
print()
for dn, dv in DELTAS:
    tag = "  <- THE HINT THE HARNESS RUNS" if (dn, dv) == HARNESS_DELTA else ""
    print(f"  --- delta = {dv} ({dn}){tag}")
    print(f"  {'cell':<6} " + "".join(f"{a:>12}" for a in PARMS)
          + f" {'pooled sd':>10} {'alpha*':>8}")
    for c in PCELLS:
        cells = ""
        for a in PARMS:
            d, _s, n = DU[(c, a, dn)]
            cells += (f"{'-':>12}" if d is None else f"{d:>+11.4f} ")
        ps = POOLED_SD[(c, dn)]
        best = BEST[(c, dn)]
        mark = ""
        if best is not None and ps is not None:
            bd = DU[(c, best, dn)][0]
            if bd is not None and abs(bd) <= 2.0 * ps:
                mark = "*"      # inside the noise: alpha* is not separated
        print(f"  {c:<6} " + cells + f" {fmt(ps, 4):>10} "
              f"{(str(best) + mark) if best else '-':>8}")
    print()
print("  `pooled sd` is the POOLED WITHIN-ARM standard deviation of PER-REP dU")
print("  at that cell -- sqrt of the mean of the arms' own variances. An")
print("  `alpha*` marked `*` did not clear 2x it and is a RANKING, not a WIN.")
print("  CTL's own dU is 0 by construction (it is the reference) and its column")
print("  is printed so that is visible rather than assumed.")

# ── 7. FALSE ALARMS, COMMANDED AND REALIZED ──────────────────────────────
print()
print("=" * 100)
print("### 7. FALSE ALARMS, COMMANDED AND REALIZED.")
print("=" * 100)
print()
print("  COMMANDED is [RACK]'s own sender-site fa_frac. REALIZED is the")
print("  RECEIVER's [RFA] class breakdown, AS A BRACKET:")
print()
print("    lo = false_frac as printed        -- a LOWER bound: `fill_src`")
print("         counts a REORDERED ORIGINAL as a successful repair, inflating")
print("         the denominator with arrivals no repair caused")
print("    hi = false / (false + fill_coded) -- the CEILING, which removes those")
print("         reordered originals from the denominator entirely")
print()
print("  The realized false-repair fraction lies in [lo, hi]. Stating either end")
print("  alone reports a BOUND as if it were a measurement.")
print()
print(f"  {'cell-arm':<10} {'n':>3} {'alpha_cmd':>10} {'fired':>8} {'spur':>7} "
      f"{'fa_frac':>8} {'x fa_class':>11} {'<=alpha?':>9} "
      f"{'realized [lo, hi]':>22}   realized-vs-commanded")
FA = {}
for c in PCELLS:
    for a in PARMS:
        rs = [r for r in by.get((c, a), []) if r.get("rack_fired")]
        n_all = len(by.get((c, a), []))
        if not by.get((c, a)):
            continue
        if not rs:
            # `fa = 0/0` fired no recovery round: an INSTRUMENT-FAIL for the
            # rep and NEVER `fa_frac = 0`. The row is printed so the exclusion
            # is visible rather than silent.
            print(f"  {c + '-' + a:<10} {0:>3} "
                  f"{fmt(ARM_ALPHA.get(a), 4) if ARM_ALPHA.get(a) is not None else 'null':>10} "
                  f"{'0/0':>8} {'-':>7} {'-':>8} {'-':>11} {'-':>9} "
                  f"{'-':>22}   INSTRUMENT-FAIL (fa=0/0 in all "
                  f"{n_all} rows: no recovery round fired)")
            continue
        ff = med([r.get("rack_fa_frac") for r in rs])
        lo = med([r.get("rfa_bracket_lo") for r in rs])
        hi = med([r.get("rfa_bracket_hi") for r in rs])
        want = ARM_ALPHA.get(a, med([r.get("alpha_cmd") for r in rs]))
        FA[(c, a)] = ff
        # THE SENSE OF THIS COLUMN IS THE OPPOSITE OF THE OBVIOUS ONE, and the
        # plain-window pass is where it was settled. The bracket is what this
        # instrument can say about the REALIZED fraction. If the COMMANDED
        # value lies INSIDE it, the instrument cannot say whether realized
        # exceeds commanded or falls short of it -- that is UNRESOLVED, and
        # the plain-window pass reported four cells of five that way rather
        # than quoting the 0.13-0.55x the printed column would have suggested.
        # It RESOLVES only when commanded sits OUTSIDE the bracket, because
        # then the direction holds at BOTH ends: at c1 commanded 0.0911 sat
        # below a floor of 0.187, so realized exceeded commanded by at least
        # 2x, and that is a measurement.
        inside = "-"
        if ff is not None and lo is not None and hi is not None:
            straddles = min(lo, hi) <= ff <= max(lo, hi)
            inside = "UNRESOLVED" if straddles else (
                "RESOLVED realized>cmd" if ff < min(lo, hi) else "RESOLVED realized<cmd")
        cant = "-"
        if ff is not None and want is not None:
            cant = "YES" if ff <= want else "NO"
        br = "-" if (lo is None and hi is None) else f"[{fmt(lo, 4)}, {fmt(hi, 4)}]"
        print(f"  {c + '-' + a:<10} {len(rs):>3} "
              f"{(fmt(want, 4) if want is not None else 'null'):>10} "
              f"{fmti(med([r.get('rack_fired') for r in rs])):>8} "
              f"{fmti(med([r.get('rack_spurious') for r in rs])):>7} "
              f"{fmt(ff, 4):>8} {fmt(safediv(ff, FA_CLASS), 2):>11} {cant:>9} "
              f"{br:>22}   {inside}")
print()
print(f"  fa_class = {FA_CLASS} = 1/16, RFC 8985 6.2 Step 4's OWN published")
print("  budget, carried as the SCALE every fa_frac is reported against.")
print()
print("  THE CANTELLI-BOUND HONESTY CHECK is the `<=alpha?` column. Cantelli")
print("  gives P(X - mu >= k*sigma) <= 1/(1 + k^2) = alpha for ANY distribution,")
print("  so an arm commanded at alpha whose MEASURED false-alarm fraction")
print("  exceeds alpha has not violated Cantelli -- it has shown that the")
print("  quantity the bound is taken over is not the quantity the recovery")
print("  clock fires on. `NO` is therefore a finding about the MAPPING, which is")
print("  exactly the thing 16.69 refuted, and never about the inequality.")
print()
print("  MONOTONICITY OF fa_frac IN alpha across the six arms:")
_ORD = [a for a in ARMS if ARM_ALPHA[a] is not None]
for c in PCELLS:
    seq = [(ARM_ALPHA[a], FA.get((c, a)), a) for a in _ORD if FA.get((c, a)) is not None]
    if len(seq) < 2:
        print(f"    {c:<6} (fewer than two arms carry a false-alarm datum -- "
              f"monotonicity is not defined)")
        continue
    ups = sum(1 for i in range(1, len(seq)) if seq[i][1] > seq[i - 1][1])
    dns = sum(1 for i in range(1, len(seq)) if seq[i][1] < seq[i - 1][1])
    verd = ("MONOTONE INCREASING" if dns == 0 and ups > 0 else
            "MONOTONE DECREASING" if ups == 0 and dns > 0 else
            "FLAT" if ups == 0 and dns == 0 else "NOT MONOTONE")
    print(f"    {c:<6} " + " ".join(f"{a}:{v:.4f}" for _al, v, a in seq)
          + f"   -> {verd} ({ups} up, {dns} down)")

# ── 8. WHAT THIS REPORT DOES NOT ESTABLISH ───────────────────────────────
print()
print("=" * 100)
print("### 8. WHAT THIS REPORT DOES NOT ESTABLISH")
print("=" * 100)
print("""
  * IT FLIPS NO DEFAULT. `RWM_ALPHA_OVERRIDE` is an EXPERIMENT KNOB and an
    OVERRIDE, not a law (net/mod.rs:784-787): `None` is byte-identical to the
    engine before it existed, nothing continuous in (delta, rho, r) is
    expressed through it, and NOTHING MAY SHIP READING IT. A shipped alpha must
    be DERIVED from the triangle -- the decision this sweep informs and does
    not take.

  * IT DOES NOT REPAIR 16.69's REFUTATION. The refutation is of what FEEDS
    alpha -- a CATEGORY ERROR in the mapping alpha = target_tail_loss * zeta --
    not of the Cantelli construction W(alpha) = srtt + sqrt((1-alpha)/alpha) *
    sigma, which was never the defective part. A sweep over alpha measures the
    curve; it does not supply the mapping.

  * AN UNSEPARATED PAIR IS NOT A NULL RESULT. It is the finding that the two
    arms realized the same clock. Nothing about the ORDERING of alpha follows
    from it, in either direction, at any n.

  * A VOID ROW IS NOT A FAILED TREATMENT. Its independent variable did not
    take. It says something about the harness and nothing about alpha.

  * `q_p50` IS NOT DELIVERED LATENCY and no claim here is stated in it. It is
    the engine's self-report about its own path, from the code under test.

  * THE CENSORED TAIL IS NOT MEASURED. Where `censor` is non-zero the top of
    the delivered-latency distribution never produced a sample, and no
    arithmetic on the survivors can place it.

  * SIGMA IS THE ARM LIST'S LARGEST FREE INPUT AND IT IS NOT RESOLVED. Route
    (b)'s alpha at c8 Bulk is 0.00170 on the MEASURED plain-window sigma and
    0.0537 on the memo's ESTIMATED one -- a factor of 32 that is entirely an
    INPUT, not a construction. The measured sigma itself spans 287x across
    three reps at converged n. No arm's LABEL is more precise than that.

  * THE ROUTES ARE SEPARATED AT BULK AND NOWHERE ELSE IN THIS SWEEP. At Auto
    they coincide within 0.8 % on the measured inputs (0.1829 vs 0.1843), so
    nothing here bears on the Auto point at all -- in either direction.

  * nu IS NOW ON THE RECORD (0.03776 at c8, plain window) and the (b)-equals-(d)
    coincidence at Auto SURVIVED it. That is a fact about the two formulas at
    one cell's measured inputs, not evidence that either is correct.
""")
print("=" * 100)
