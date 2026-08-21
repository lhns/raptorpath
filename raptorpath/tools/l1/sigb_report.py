#!/usr/bin/env python3
"""THE ESTIMATOR BATTERY'S SCORER — the acceptance bar, applied LITERALLY.

  usage: sigb_report.py <ledger.log> [<ledger.log> ...]

Scores goal-gate "THE SIGMA ESTIMATOR — THE ACCEPTANCE BAR" and NOTHING ELSE.
Every threshold below is transcribed from that section with its clause named;
no number in this file was chosen here, and the two that were derived HERE
(`MIN_READS`, `QSP_STRUCTURAL_C`) carry their derivation on the line.

THE ORDER OF THE OUTPUT IS THE ORDER OF THE READING, AND IT IS
ABORT-CAUSE-FIRST. Aborts and witnesses print before any statistic, because a
statistic computed over an aborted invocation is not a weak result, it is not a
result. Then clause `C` (can the estimator be USED at all), then clause `S`
(the accept/reject statistic), then the sampling-rate row, then clause `B`,
then the verdict. `B` is last on purpose: it can REJECT but cannot ACQUIT, so
it is never the thing a reader meets first.

WHAT THIS FILE WILL NOT DO. It will not soften a bar, average two seats, drop a
leg that fails, or report a pass for a quantity it could not compute. The legal
outcomes per estimator are exactly four and they are enumerated in `verdict()`.
"""
import json
import math
import re
import sys
from collections import defaultdict

import latt_probe

# ── THE BAR. Every constant transcribed, with its clause. ──────────────────

#: Clause `S`, §3: `R_total <= 6.0` at EVERY cell. 3 of 10 pairs separate;
#: 3.04x headroom on the extreme pair (18.239 / 6.0).
ACCEPT_BAR = 6.0
#: Clause `S`, §3: the PREFER tier and the tie-break. 6 of 10 pairs; the
#: three-point sub-grid {Q002, Q050, Q400} separates completely, which is the
#: coarsest grid on which an INTERIOR optimum can be located at all.
PREFER_BAR = 3.5
#: §16.74.5's k-ratio over the swept grid `[0.002, 0.400]`. Disclosure only.
K_RATIO = 18.239

#: Clause `B`, §4. `beta = sigma_hat_cand / sigma_truth`, derived from the
#: grid's smallest adjacent step `0.400/0.184 = 2.174` at route (b)'s
#: elasticity 2: `2.174^(1/2) = 1.474`. The tighter of the two routes binds.
B_ACCEPT_LO, B_ACCEPT_HI = 0.68, 1.47
B_REJECT_LO, B_REJECT_HI = 0.50, 2.00
#: Two candidates BOTH inside the band that disagree with EACH OTHER by more
#: than the band's own width are a FINDING ABOUT THE PROBE (§4's last
#: sentence), and `B` goes UNRESOLVED at that cell rather than reading as a
#: pass. The width is the band, not a new number: 1.47/0.68.
B_BAND_WIDTH = B_ACCEPT_HI / B_ACCEPT_LO

#: Clause `C1` — the warm-up exclusions, DECLARED PER ESTIMATOR CLASS BEFORE
#: THE RUN and applied HERE, as a scoring rule on the parser (clause `C3`:
#: never a gate in the engine). EWMA class 16 = the first count at which the
#: zero seed is under 1 % of the reading at beta = 1/4. Window class = `L`,
#: because a partly-full window reports a quantile of fewer order statistics
#: than it claims; `msd`'s count is the DIFFERENCE count, so its full set is
#: `L - 1` (`Path::rtt_msd_samples`, and `tests/sigma_candidates_reachability.rs`
#: asserts exactly this pair).
WINDOW_L = 256
N_WARM = {"sig": 16, "rvar": 16, "qsp": WINDOW_L, "msd": WINDOW_L - 1}
CLASS = {"sig": "EWMA", "rvar": "EWMA", "qsp": "window", "msd": "window"}

#: Clause `C2`: `n_warm <= 0.05 * N_cell`, the cell's total RTT-sample count.
C2_FRAC = 0.05

#: DERIVED HERE, AND THE DERIVATION IS THE WHOLE REASON FOR THE NUMBER. The bar
#: refused `sup/inf` because a RANGE statistic grows without bound with sample
#: count. At `n < 20` the nearest-rank `p05`/`p95` ARE the range: `int(0.05*n)`
#: is 0 and `int(0.95*n)` is `n-1` for every `n <= 19`, so `R_total` computed
#: there IS `sup/inf` wearing a quantile's name — the exact statistic the bar
#: rejected. At `n = 20` the indices are 1 and 19, both interior. A leg thinner
#: than that is UNSCOREABLE, never a pass and never a fail.
MIN_READS = 20

#: The `qsp_us` gauge computes `P90 - P50`, so the probe's `P90` cannot be
#: placed when the top `c` of the distribution produced no sample: `0.90 > 1-c`.
QSP_STRUCTURAL_C = 0.10

#: Shaped capacity per cell, TRANSCRIBED from `alpha_report.py:69-75`, which
#: derives it from `lib.sh`'s `scenario_params`. Headroom input only.
SHAPED_BPS = {
    "c1": 1_000_000_000,      # single, 1000 Mbit
    "c7": 200_000_000,        # dual, 2 x 100 Mbit
    "c8": 120_000_000,        # dual, 100 + 20 Mbit
    "c8L": 120_000_000,       # dual, 100 + 20 Mbit
    "sc2": 100_000_000,       # single, 100 Mbit
}
#: MEASUREMENT DISCIPLINE 16c: below this headroom, no throughput target is
#: scoreable. This battery writes no throughput target, so the bar is a
#: disclosure here and licenses nothing.
HEADROOM_BAR = 5.0

GAUGES = ("sig", "rvar", "qsp", "msd")
#: Clause `B` §4's functional map: which probe functional each gauge is scored
#: against, LIKE-FOR-LIKE. `rvar` is a moment-class candidate and the bar says
#: "the moment-class candidateS", so it reads `sd` LITERALLY. The known
#: mismatch (`rvar` estimates `E|dev| = 0.7979*sigma` for a Gaussian) is
#: DISCLOSED in the output and is NOT corrected for — correcting it would be a
#: bar amendment written after the functional map.
PROBE_FUNC = {"sig": "sd", "rvar": "sd", "qsp": "qsp", "msd": "msd"}

# ── LEDGER PARSING ─────────────────────────────────────────────────────────

READ = re.compile(
    r"^SIGBREAD (\S+) (\S+) (\S+) (\S+) p(\d+) blk=(\d+) "
    r"sig=(-|\d+)/(\d+) rvar=(-|\d+)/(\d+) qsp=(-|\d+)/(\d+) msd=(-|\d+)/(\d+)")
PROBE = re.compile(r"^SIGBPROBE (\S+) (\S+) (\S+) (\{.*\})$")
META = re.compile(r"^SIGBMETA (\S+) (\S+) (\S+) (\{.*\})$")
WIT = re.compile(r"^SIGBWITNESS (\{.*\})$")
FAILMARK = re.compile(
    r"^(ABORT|ABORT-GEN-PLATEAU|OUT-OF-BAND|SUBSTRATE-FAIL|INSTRUMENT-FAIL-GATE"
    r"|INSTRUMENT-FAIL-PROBE|SIGB-PARSE-FAIL|W7-FAIL-CLI|CELL-VANISHED"
    r"|QCAP-MISSING|UNKNOWN-CELL)\b")


class Ledger:
    def __init__(self):
        #: (cell, site, pid) -> gauge -> list of (seed, rep, value)
        self.reads = defaultdict(lambda: defaultdict(list))
        #: (cell, site, pid) -> gauge -> list of (seed, rep, n) — every row,
        #: warm-up INCLUDED, so the exclusion is auditable from this object.
        self.raw_n = defaultdict(lambda: defaultdict(list))
        #: (cell, leg) -> list of probe dicts, one per rep
        self.probe = defaultdict(list)
        #: (cell, seed, rep) -> meta dict
        self.meta = {}
        self.wit = []
        self.fails = []
        self.reps = defaultdict(set)

    def load(self, path):
        with open(path, "r", errors="replace") as fh:
            for ln in fh:
                ln = ln.rstrip("\n")
                m = READ.match(ln)
                if m:
                    cell, seed, rep, site, pid = (m.group(1), m.group(2),
                                                  m.group(3), m.group(4),
                                                  int(m.group(5)))
                    key = (cell, site, pid)
                    vals = m.groups()[6:]
                    for i, g in enumerate(GAUGES):
                        v, n = vals[2 * i], int(vals[2 * i + 1])
                        self.raw_n[key][g].append((seed, rep, n))
                        # CLAUSE C1 APPLIED HERE, AND ONLY HERE.
                        if v != "-" and n >= N_WARM[g]:
                            self.reads[key][g].append((seed, rep, int(v)))
                    self.reps[cell].add((seed, rep))
                    continue
                m = PROBE.match(ln)
                if m:
                    d = json.loads(m.group(4))
                    d["_seed"], d["_rep"] = m.group(2), m.group(3)
                    self.probe[(m.group(1), d.get("leg"))].append(d)
                    continue
                m = META.match(ln)
                if m:
                    self.meta[(m.group(1), m.group(2), m.group(3))] = \
                        json.loads(m.group(4))
                    continue
                m = WIT.match(ln)
                if m:
                    self.wit.append(json.loads(m.group(1)))
                    continue
                if FAILMARK.match(ln):
                    self.fails.append(ln)

    def apply_voids(self):
        """DROP EVERY ROW OF AN ABORTED INVOCATION, FROM EVERY CONTAINER.

        THE RULE IS THE PRE-REGISTRATION'S OWN, AND IT WAS NOT IMPLEMENTED
        UNTIL THE SCORED RUN PRODUCED ITS FIRST ABORT. §8: an aborted
        invocation is *"no datum, no liveness verdict, and NOT in any
        denominator."* The driver marks the abort in the ledger and the report
        printed the marker — but it went on pooling that invocation's gauge
        readings into `R_total` anyway, which is exactly the denominator the
        clause forbids. A single VOID rep at `c8`, where a leg carries ~9
        readings, is ~6 % of a pooled quantile.

        THE VOID SET IS BUILT FROM THE WITNESS ROWS, NOT FROM THE MARKER TEXT,
        because the witness row carries `(cell, seed, rep)` while the marker
        line does not carry the seed — and two seeds pool here, so a marker
        matched by cell and rep alone would void the innocent seed's rep too.

        Returns the void set so the report can print it before any statistic.
        """
        void = {(w["cell"], str(w["seed"]), str(w["rep"]))
                for w in self.wit if w.get("gen_plateau")}
        if not void:
            return void
        keep = lambda s, r, cell: (cell, str(s), str(r)) not in void
        for key in list(self.reads):
            for g in list(self.reads[key]):
                self.reads[key][g] = [t for t in self.reads[key][g]
                                      if keep(t[0], t[1], key[0])]
        for key in list(self.raw_n):
            for g in list(self.raw_n[key]):
                self.raw_n[key][g] = [t for t in self.raw_n[key][g]
                                      if keep(t[0], t[1], key[0])]
        for key in list(self.probe):
            self.probe[key] = [d for d in self.probe[key]
                               if keep(d["_seed"], d["_rep"], key[0])]
        for k in list(self.meta):
            if (k[0], str(k[1]), str(k[2])) in void:
                del self.meta[k]
        return void


# ── STATISTICS. One quantile estimator in the tree, imported not rewritten. ─

def stats(vals):
    """The bar's own functional, and the disclosure it requires beside it."""
    if not vals:
        return None
    p05, p50, p95 = (latt_probe.q(vals, 0.05), latt_probe.q(vals, 0.50),
                     latt_probe.q(vals, 0.95))
    lo, hi = min(vals), max(vals)
    return {
        "n": len(vals), "p05": p05, "p50": p50, "p95": p95,
        "min": lo, "max": hi,
        "R_total": (round(p95 / p05, 3) if p05 else None),
        "sup_inf": (round(hi / lo, 3) if lo else None),
    }


def median(v):
    return latt_probe.q(v, 0.50) if v else None


def per_rep_median(rows):
    """rows = [(seed, rep, value)] -> {(seed,rep): median} — the run-window
    reading, so clause `B` compares like windows and not like sample counts."""
    by = defaultdict(list)
    for s, r, v in rows:
        by[(s, r)].append(v)
    return {k: median(v) for k, v in by.items()}


def spearman(xs, ys):
    """Rank correlation, written out because there is no scipy on the VM and a
    number is required rather than an impression."""
    n = len(xs)
    if n < 3:
        return None

    def rank(v):
        order = sorted(range(n), key=lambda i: v[i])
        r = [0.0] * n
        i = 0
        while i < n:
            j = i
            while j + 1 < n and v[order[j + 1]] == v[order[i]]:
                j += 1
            avg = (i + j) / 2.0 + 1.0
            for k in range(i, j + 1):
                r[order[k]] = avg
            i = j + 1
        return r

    rx, ry = rank(xs), rank(ys)
    mx, my = sum(rx) / n, sum(ry) / n
    num = sum((a - mx) * (b - my) for a, b in zip(rx, ry))
    dx = math.sqrt(sum((a - mx) ** 2 for a in rx))
    dy = math.sqrt(sum((b - my) ** 2 for b in ry))
    return round(num / (dx * dy), 3) if dx and dy else None


def s_verdict(st):
    if st is None or st["n"] == 0:
        return "UNSCOREABLE-NO-SAMPLE"
    if st["n"] < MIN_READS:
        return "UNSCOREABLE-THIN(n=%d<%d)" % (st["n"], MIN_READS)
    if st["R_total"] is None:
        return "UNSCOREABLE-ZERO-P05"
    if st["R_total"] <= PREFER_BAR:
        return "PASS-PREFER"
    if st["R_total"] <= ACCEPT_BAR:
        return "PASS-ACCEPT"
    return "FAIL"


def b_verdict(beta):
    if beta is None:
        return "UNSCOREABLE"
    if B_ACCEPT_LO <= beta <= B_ACCEPT_HI:
        return "NOT-SHOWN-BIASED"          # NEVER "unbiased" — §4's rule
    if B_REJECT_LO <= beta <= B_REJECT_HI:
        return "ADMISSIBLE-BIAS-CARRIED"
    return "REJECT"


# ── THE REPORT ─────────────────────────────────────────────────────────────

def main(argv):
    if not argv:
        print(__doc__.splitlines()[2].strip(), file=sys.stderr)
        return 2
    L = Ledger()
    for p in argv:
        L.load(p)
    VOID = L.apply_voids()

    P = print
    P("=" * 78)
    P("THE ESTIMATOR BATTERY — SCORED AGAINST 'THE SIGMA ESTIMATOR — THE")
    P("ACCEPTANCE BAR' AND AGAINST NOTHING ELSE.")
    P("ledgers: %s" % ", ".join(argv))
    P("bars: S accept R_total<=%.1f, prefer <=%.1f (k-ratio %.3f) | "
      "B accept [%.2f,%.2f], reject outside [%.2f,%.2f] | C n_warm<=%.0f%% N"
      % (ACCEPT_BAR, PREFER_BAR, K_RATIO, B_ACCEPT_LO, B_ACCEPT_HI,
         B_REJECT_LO, B_REJECT_HI, 100 * C2_FRAC))
    P("warm-up (clause C1, applied by THIS parser): %s"
      % " ".join("%s>=%d(%s)" % (g, N_WARM[g], CLASS[g]) for g in GAUGES))
    P("=" * 78)

    # ── 1. ABORT-CAUSE FIRST ────────────────────────────────────────────
    P("\n## 1 — ABORTS AND WITNESSES, BEFORE ANY STATISTIC\n")
    if L.fails:
        P("  %d abort/fail marker(s):" % len(L.fails))
        for f in L.fails:
            P("    %s" % f)
    else:
        P("  0 abort/fail markers.")

    P("\n  VOIDED INVOCATIONS — dropped from EVERY container before any")
    P("  statistic below, per §8: an aborted invocation is no datum and is NOT")
    P("  in any denominator.")
    if VOID:
        for c, s, r in sorted(VOID):
            P("    VOID %s seed=%s rep=%s" % (c, s, r))
        P("  %d invocation(s) voided. Their gauge readings, probe rows and" % len(VOID))
        P("  meta rows are ABSENT from clause S, clause B and the rate row.")
    else:
        P("    none.")

    n_inv = len(L.wit)
    bad = {"W1": [], "W2": [], "W4": [], "W5": [], "W7": [], "PLATEAU": []}
    for w in L.wit:
        tag = "%s-s%s-r%s" % (w["cell"], w["seed"], w["rep"])
        lossy = w["cell"] != "c1"
        if w.get("W1_rfa_gen") != 0:
            bad["W1"].append("%s gen=%s" % (tag, w.get("W1_rfa_gen")))
        if w.get("W2_pfrac_lines"):
            bad["W2"].append("%s pfrac=%s" % (tag, w["W2_pfrac_lines"]))
        if lossy and not w.get("W4_retx_max"):
            bad["W4"].append("%s retx_max=0" % tag)
        fa = w.get("W5_rack_fa")
        if lossy and (not fa or fa.split("/")[-1] == "0"):
            bad["W5"].append("%s fa=%s" % (tag, fa))
        if w.get("W7_group_misses_cli") or w.get("W7_group_misses_srv"):
            bad["W7"].append("%s misses=%s/%s" % (tag, w["W7_group_misses_cli"],
                                                  w["W7_group_misses_srv"]))
        if w.get("gen_plateau"):
            bad["PLATEAU"].append("%s mbps=%s" % (tag, w.get("mbps")))
    P("\n  %d invocation(s) with a witness row." % n_inv)
    for k in ("W1", "W2", "W4", "W5", "W7", "PLATEAU"):
        P("    %-8s %s" % (k, "clean" if not bad[k]
                           else "FAIL at %d: %s" % (len(bad[k]),
                                                    "; ".join(bad[k][:6]))))
    P("\n  W7 is THIS BATTERY'S OWN reachability gate: all four gauge tokens")
    P("  with their /n counts on every path entry of every [DIAG] block, both")
    P("  endpoints. A miss is the MEASUREMENT failing, not a column absent.")

    # ── 1b. HEADROOM (MEASUREMENT DISCIPLINE 16) ─────────────────────────
    P("\n  HEADROOM, from the qdisc capture on EVERY cell and EVERY invocation")
    P("  — the omission discipline 16 exists to prevent. The denominator is")
    P("  the TRANSFER wall, NEVER the invocation wall (which runs 1.12-2.11x")
    P("  the transfer and read c7 at 77.6%% when the cell was at 96.9%%).\n")
    P("    %-6s %12s %8s %8s %9s %11s   %s"
      % ("cell", "shaped bps", "xfer_s", "INVOC_S", "util %", "headroom %",
         "claims permitted"))
    bycell = defaultdict(list)
    for (cell, _s, _r), m in L.meta.items():
        bycell[cell].append(m)
    for cell in sorted(bycell):
        cap = SHAPED_BPS.get(cell)
        u = [100.0 * m["tc_bytes"] * 8.0 / (m["seconds"] * cap)
             for m in bycell[cell]
             if m.get("tc_bytes") and m.get("seconds") and cap]
        util = median(u)
        hr = None if util is None else 100.0 - util
        xs = median([m["seconds"] for m in bycell[cell] if m.get("seconds")])
        iv = median([m["tc_s"] for m in bycell[cell] if m.get("tc_s")])
        if util is None:
            claim = "(no tc datum — headroom UNKNOWN)"
        elif hr < HEADROOM_BAR:
            claim = "NO-THROUGHPUT-TARGET — headroom < %.0f%% (discipline 16c)" % HEADROOM_BAR
        else:
            claim = "headroom exists"
        P("    %-6s %12s %8s %8s %9s %11s   %s"
          % (cell, cap or "-", round(xs, 2) if xs else "-", iv or "-",
             round(util, 1) if util is not None else "-",
             round(hr, 1) if hr is not None else "-", claim))
    P("\n  THIS BATTERY WRITES NO GOODPUT CLAUSE ANYWHERE, so the headroom")
    P("  table cannot license one. It is here because discipline 16 requires")
    P("  it on every cell of every pass, and because a cell running at its")
    P("  ceiling produces a DIFFERENT RTT sample process from one that is not")
    P("  — which is a property of the estimator's INPUT and therefore of the")
    P("  thing under test.")

    # ── 2. CLAUSE C ─────────────────────────────────────────────────────
    P("\n## 2 — CLAUSE C: IS THE ESTIMATOR USABLE FOR THE RUN, NOT MERELY AT")
    P("##     ITS END?  n_warm <= %.0f%% x N_cell, from the RECORDED n counts.\n"
      % (100 * C2_FRAC))
    # ── PER-LEG N, the input to both readings below ──────────────────────
    legN = {}
    for key in sorted(L.raw_n):
        ns = [n for _, _, n in L.raw_n[key]["sig"]]
        legN[key] = max(ns) if ns else 0

    # ── C2 AS THE BAR WRITES IT — PER CELL, ON THE CELL'S DATA-PATH COUNT.
    #
    # THE BAR SAYS `N_cell` AND MEANS THE CELL'S RTT-SAMPLE COUNT: its own
    # worked example is "the binding cell is c8 ... at N ~ 17 660, the smallest
    # converged sample count in the primitives table", and that number is the
    # DATA-PATH leg's count in that table. The data path identifies itself by
    # sample count and no judgement call is needed — the same discriminator the
    # c8 sigma pass and the local characterization both used (a 137x separation
    # there, and a 21x sender-side leg asymmetry at the duals here).
    #
    # THIS IS THE BINDING READING. The battery's pre-registration §6 glossed
    # C2 as "per leg"; that gloss is STRICTER than the clause it glosses, and
    # where a gloss conflicts with the bar it glosses, THE BAR WINS — this pass
    # scores the acceptance bar and nothing else, including nothing of its own.
    # The per-leg table is kept below as a DISCLOSURE, because what it actually
    # shows is not a clause-C failure but a fact about where a window-class
    # gauge can be measured at all — and clause S already says that, correctly,
    # as UNSCOREABLE-NO-SAMPLE rather than as a fail.
    cells = sorted({k[0] for k in legN})
    cellN = {c: max([legN[k] for k in legN if k[0] == c] or [0]) for c in cells}
    c_fail = defaultdict(list)
    P("  SCORED — C2 AS THE BAR WRITES IT: per CELL, on the cell's DATA-PATH")
    P("  RTT-sample count (the leg with the most samples, which is how the")
    P("  data path identifies itself in this tree).\n")
    P("  %-6s %14s %10s   %s" % ("cell", "N_cell (data)", "C2 bar", "per class"))
    for c in cells:
        N, bar = cellN[c], C2_FRAC * cellN[c]
        verd = []
        for g in GAUGES:
            ok = N > 0 and N_WARM[g] <= bar
            verd.append("%s=%s" % (g, "ok" if ok else "FAIL"))
            if not ok:
                c_fail[g].append("%s (N_cell=%d, bar=%.0f, n_warm=%d)"
                                 % (c, N, bar, N_WARM[g]))
        P("  %-6s %14d %10.0f   %s" % (c, N, bar, " ".join(verd)))
    P("\n  Clause C2's binding cell was pre-registered as c8 at N ~ 17 660,")
    P("  giving n_warm <= 883. The table above uses THIS battery's own counts.")

    P("\n  DISCLOSURE — THE PER-LEG COUNTS, WHICH ARE NOT A CLAUSE-C VERDICT.")
    P("  A leg whose N cannot reach a gauge's own n_warm never produces a")
    P("  post-warm-up reading there, and clause S reports that as")
    P("  UNSCOREABLE-NO-SAMPLE — the honest statement, since the gauge was not")
    P("  measured at that leg rather than measured and found wanting.\n")
    P("  %-6s %-5s %-4s %10s %10s   %s"
      % ("cell", "site", "path", "N (max n)", "5% of N", "window class reachable?"))
    for key in sorted(legN):
        cell, site, pid = key
        N = legN[key]
        reach = ("qsp=%s msd=%s"
                 % ("yes" if N >= N_WARM["qsp"] else "NO",
                    "yes" if N >= N_WARM["msd"] else "NO"))
        P("  %-6s %-5s p%-3d %10d %10.0f   %s"
          % (cell, site, pid, N, C2_FRAC * N, reach))

    # ── 3. CLAUSE S ─────────────────────────────────────────────────────
    P("\n## 3 — CLAUSE S: R_total = p95/p05 OVER THE POOLED READINGS OF ALL")
    P("##     REPS AT ONE CELL, ONE LEG. sup/inf is the DISCLOSURE column and")
    P("##     is NOT the accept/reject statistic (§2: it grows with rep count).\n")
    S = {}
    for key in sorted(L.raw_n):
        cell, site, pid = key
        P("  --- %s  site=%s  p%d ---" % (cell, site, pid))
        P("      %-6s %8s %8s %8s %8s %9s %9s  %s"
          % ("gauge", "n_read", "p05", "p50", "p95", "R_total", "sup/inf",
             "verdict"))
        for g in GAUGES:
            vals = [v for _, _, v in L.reads[key][g]]
            st = stats(vals)
            S[(key, g)] = st
            v = s_verdict(st)
            if st is None:
                P("      %-6s %8d %8s %8s %8s %9s %9s  %s"
                  % (g, 0, "-", "-", "-", "-", "-", v))
            else:
                P("      %-6s %8d %8s %8s %8s %9s %9s  %s"
                  % (g, st["n"], st["p05"], st["p50"], st["p95"],
                     st["R_total"], st["sup_inf"], v))

    # ── 3b. IS THE VERDICT AN ARTEFACT OF THE SCORING DOMAIN? ───────────
    #
    # THE BATTERY SCORES PER LEG, WHICH IS STRICTER THAN THE BAR'S "worst
    # cell binds". So before any REJECT is published, the SAME statistic is
    # re-read on the bar's OWN most generous domain — the DATA-PATH leg of
    # each cell, the leg the bar's `N_cell` refers to — and the two readings
    # are printed side by side. If a candidate clears 6.0 on the data path and
    # fails only on a sparse leg, that is a materially different finding from
    # one that fails on the data path too, and a reader is entitled to see
    # which without re-deriving it.
    P("\n## 3b — THE SAME CLAUSE S ON THE BAR'S OWN MOST GENEROUS DOMAIN:")
    P("##      THE DATA-PATH LEG OF EACH CELL (the leg `N_cell` refers to).\n")
    dp = {}
    for c in sorted({k[0] for k in L.raw_n}):
        legs = [k for k in L.raw_n if k[0] == c]
        best = max(legs, key=lambda k: max(
            [n for _, _, n in L.raw_n[k]["sig"]] or [0]))
        dp[c] = best
    P("  %-6s %-12s %10s %10s %10s %10s"
      % ("cell", "data-path leg", "sig", "rvar", "qsp", "msd"))
    dp_worst = {g: (None, 0.0) for g in GAUGES}
    for c, key in dp.items():
        row_ = []
        for g in GAUGES:
            st = S.get((key, g))
            r = st["R_total"] if (st and st["n"] >= MIN_READS and st["R_total"]) else None
            row_.append("%.2f" % r if r else "-")
            if r and r > dp_worst[g][1]:
                dp_worst[g] = (c, r)
        P("  %-6s %-12s %10s %10s %10s %10s"
          % (c, "%s/p%d" % (key[1], key[2]), *row_))
    P("\n  worst DATA-PATH cell per gauge, against the accept bar of %.1f:" % ACCEPT_BAR)
    for g in GAUGES:
        c, r = dp_worst[g]
        P("    %-6s %-5s R_total = %-9s %s"
          % (g, c or "-", ("%.3f" % r) if r else "-",
             "CLEARS THE BAR" if (r and r <= ACCEPT_BAR)
             else ("FAILS by %.2fx" % (r / ACCEPT_BAR) if r else "unscoreable")))
    P("\n  IF EVERY GAUGE FAILS HERE TOO, THE VERDICT IS NOT AN ARTEFACT OF")
    P("  THIS BATTERY'S STRICTER PER-LEG DOMAIN, and the closure does not")
    P("  rest on a choice this pass made rather than one the bar made.")

    # ── 4. THE SAMPLING-RATE ROW ────────────────────────────────────────
    P("\n## 4 — THE SAMPLING-RATE ROW, FIRST-CLASS. `msd` ESTIMATES DISPERSION")
    P("##     AT A LAG OF ONE INTER-SAMPLE INTERVAL, SO ITS MAGNITUDE DEPENDS")
    P("##     ON THE SAMPLING RATE, WHICH IS NOT A PROPERTY OF THE LINK.\n")
    P("  If `msd`'s advantage tracks the RATE rather than the CELL, it is an")
    P("  artefact. The local pass measured 15x level and 2.8x R_local across a")
    P("  137x rate gap ON ONE HOST, and it moved R the WRONG WAY.\n")
    P("  %-6s %-5s %-4s %12s %10s %10s %10s %10s"
      % ("cell", "site", "path", "samp/s", "msd p50", "sig p50",
         "msd/sig", "msd R_tot"))
    rate_x, rtot_y, lvl_y = [], [], []
    for key in sorted(L.raw_n):
        cell, site, pid = key
        ns = [n for _, _, n in L.raw_n[key]["sig"]]
        N = max(ns) if ns else 0
        secs = [m["seconds"] for k, m in L.meta.items()
                if k[0] == cell and m.get("seconds")]
        wall = median(secs) if secs else None
        rate = (N / wall) if (wall and N) else None
        smsd, ssig = S[(key, "msd")], S[(key, "sig")]
        lvl = (round(smsd["p50"] / ssig["p50"], 4)
               if smsd and ssig and ssig["p50"] else None)
        P("  %-6s %-5s p%-3d %12s %10s %10s %10s %10s"
          % (cell, site, pid,
             ("%.1f" % rate) if rate else "-",
             smsd["p50"] if smsd else "-", ssig["p50"] if ssig else "-",
             lvl if lvl is not None else "-",
             smsd["R_total"] if smsd else "-"))
        if rate and smsd and smsd["R_total"] and smsd["n"] >= MIN_READS:
            rate_x.append(rate)
            rtot_y.append(smsd["R_total"])
            if lvl is not None:
                lvl_y.append(lvl)
    rho_r = spearman(rate_x, rtot_y)
    rho_l = spearman(rate_x[:len(lvl_y)], lvl_y)
    P("\n  Spearman rank correlation over the %d scoreable legs:" % len(rate_x))
    P("    rate vs msd R_total   rho = %s" % rho_r)
    P("    rate vs msd/sig level rho = %s" % rho_l)

    # ── THE SAME CORRELATION, SPLIT BY SEAT — A DISCLOSURE, NOT A VERDICT.
    # The pooled rho mixes two seats whose sample rates barely overlap (the
    # sender runs at kHz, the receiver at tens of Hz) and whose orderings run
    # OPPOSITE ways. A pooled rank correlation over a bimodal predictor is a
    # number about the mixture, not about either seat, so it is reported
    # beside the per-seat ones rather than instead of them. The seat is
    # already a first-class axis here (§16.74.5 requirement 3 and the leg's
    # own definition), so this is a slice of the pre-registered frame, not a
    # new one — AND NO VERDICT IS TAKEN FROM IT.
    for seat in ("cli", "srv"):
        sx, sy = [], []
        for key in sorted(L.raw_n):
            if key[1] != seat:
                continue
            ns = [n for _, _, n in L.raw_n[key]["sig"]]
            N = max(ns) if ns else 0
            secs = [m["seconds"] for k, m in L.meta.items()
                    if k[0] == key[0] and m.get("seconds")]
            wall = median(secs) if secs else None
            smsd = S.get((key, "msd"))
            if wall and N and smsd and smsd["R_total"] and smsd["n"] >= MIN_READS:
                sx.append(N / wall)
                sy.append(smsd["R_total"])
        P("    [seat=%s] %d leg(s), rate vs msd R_total   rho = %s"
          % (seat, len(sx), spearman(sx, sy)))
    P("  A |rho| near 1 on either row says the RATE, not the cell, orders the")
    P("  gauge. NO VERDICT IS TAKEN FROM rho ALONE — it is reported beside the")
    P("  per-leg clause-S verdicts, which are what binds.")

    # ── 5. CLAUSE B ─────────────────────────────────────────────────────
    P("\n## 5 — CLAUSE B: beta vs THE DELIVERED-LATENCY PROBE, LIKE-FOR-LIKE.")
    P("##     **B CAN REJECT. B CANNOT ACQUIT.** A candidate inside the band is")
    P("##     recorded as NOT-SHOWN-BIASED, never as unbiased.\n")
    P("  THE PROBE'S OWN LIMITS, RESTATED BEFORE ITS NUMBERS:")
    P("   1. SITE. It measures the peer path and excludes sender scheduling,")
    P("      store residency and the ack-generation path — all of which ADD")
    P("      dispersion. Its dispersion is a LOWER bound on the ack path's, so")
    P("      every beta below is a LOWER bound on the true bias.")
    P("   2. CENSORING. A lost probe never produces a sample and the losses")
    P("      come from exactly the worst states. censor_frac prints beside")
    P("      every functional; >%.0f%% kills the leg (latt_probe contract bar),"
      % (100 * latt_probe.CONTRACT_BAR))
    P("      and P90 dies structurally above %.0f%% (0.90 > 1-c)."
      % (100 * QSP_STRUCTURAL_C))
    P("   3. SAMPLING RATE, AND IT IS SPECIFIC TO `msd`. The probe runs at")
    P("      20 Hz; the sender's RTT stream runs at kHz. `msd` is MEASURABLY")
    P("      rate-dependent, so beta_msd against this probe is NOT like-for-")
    P("      like in the one axis `msd` is known to depend on. It is printed")
    P("      and it is marked CONFOUNDED; it is not read as a pass or a fail.")
    P("   4. rvar reads against `sd` LITERALLY, per §4's 'moment-class")
    P("      candidateS'. rvar estimates E|dev| = 0.7979*sigma for a Gaussian,")
    P("      so beta_rvar carries a built-in ~0.80x. DISCLOSED, NOT CORRECTED.")
    P("")
    B = {}
    for key in sorted(L.raw_n):
        cell, site, pid = key
        legs = L.probe.get((cell, pid), [])
        if not legs:
            continue
        P("  --- %s  site=%s  p%d   (probe leg %d, %d rep(s)) ---"
          % (cell, site, pid, pid, len(legs)))
        cens = [d["censor_frac"] for d in legs if d.get("censor_frac") is not None]
        cmed = median(cens)
        dead_leg = any(d.get("leg_unscoreable") for d in legs)
        qsp_dead = (cmed is not None and cmed > QSP_STRUCTURAL_C)
        P("      probe: censor median %s%%  leg_unscoreable=%s  P90-dead=%s  "
          "spacing=%sms"
          % (round(100 * cmed, 2) if cmed is not None else "-",
             dead_leg, qsp_dead, legs[0].get("spacing_ms")))
        P("      %-6s %10s %12s %10s %10s  %s"
          % ("gauge", "cand p50", "probe func", "probe val", "beta", "verdict"))
        for g in GAUGES:
            fn = PROBE_FUNC[g]
            cand_rep = per_rep_median(L.reads[key][g])
            cand = median([v for v in cand_rep.values() if v is not None])
            pv = median([d[fn] for d in legs if d.get(fn) is not None])
            beta = (round(cand / pv, 4) if (cand and pv) else None)
            note = ""
            if dead_leg:
                v, note = "UNSCOREABLE", "(probe leg over contract bar)"
            elif g == "qsp" and qsp_dead:
                v, note = "UNSCOREABLE", "(P90 inside the censored tail)"
            elif g == "msd":
                v = b_verdict(beta)
                v, note = "CONFOUNDED-" + v, "(20 Hz probe vs kHz sender)"
            else:
                v = b_verdict(beta)
            if site != "cli":
                note += " [DISCLOSURE: srv seat, the clock is at the sender]"
                v = "DISCLOSURE-" + v
            B[(key, g)] = (beta, v)
            P("      %-6s %10s %12s %10s %10s  %s %s"
              % (g, cand if cand is not None else "-", fn,
                 pv if pv is not None else "-",
                 beta if beta is not None else "-", v, note))
        # §4's LAST SENTENCE, mechanised: two candidates both inside the band
        # that disagree with EACH OTHER by more than the band's own width are a
        # FINDING ABOUT THE PROBE and B goes UNRESOLVED at that cell.
        inside = [(g, B[(key, g)][0]) for g in GAUGES
                  if B.get((key, g)) and B[(key, g)][0] is not None
                  and B_ACCEPT_LO <= B[(key, g)][0] <= B_ACCEPT_HI]
        for i in range(len(inside)):
            for j in range(i + 1, len(inside)):
                a, b = inside[i], inside[j]
                r = max(a[1], b[1]) / min(a[1], b[1])
                if r > B_BAND_WIDTH:
                    P("      B-UNRESOLVED: %s and %s are both inside the band "
                      "but disagree with each other by %.2fx > %.2fx. Per §4 "
                      "that is a FINDING ABOUT THE PROBE, and B is UNRESOLVED "
                      "at this leg rather than read as a pass."
                      % (a[0], b[0], r, B_BAND_WIDTH))

    # ── 6. THE VERDICT ──────────────────────────────────────────────────
    P("\n## 6 — THE VERDICT, PER ESTIMATOR. FOUR LEGAL OUTCOMES AND NO FIFTH.\n")
    P("  ACCEPT              S at every leg, C at every cell, B not REJECT")
    P("                      and B scoreable. Qualified at the PLAIN-WINDOW")
    P("                      SEAT ONLY — §16.74.5 req 3's generation seat is")
    P("                      not run by this battery and no verdict here")
    P("                      transports to it.")
    P("  PREFER              ACCEPT and R_total <= %.1f at every leg." % PREFER_BAR)
    P("  REJECT-<clause>     the failing clause and the binding leg, named.")
    P("  UNSCOREABLE-<gate>  the gate that could not be evaluated, named. NOT")
    P("                      a pass.\n")
    order = []
    for g in GAUGES:
        legs = [(k, S[(k, g)]) for k in sorted(L.raw_n) if (k, g) in S]
        scoreable = [(k, st) for k, st in legs
                     if st and st["n"] >= MIN_READS and st["R_total"]]
        thin = [k for k, st in legs
                if not st or st["n"] < MIN_READS or not st["R_total"]]
        worst = max(scoreable, key=lambda t: t[1]["R_total"]) if scoreable else None
        best = min(scoreable, key=lambda t: t[1]["R_total"]) if scoreable else None
        cfail = c_fail.get(g, [])
        # CLAUSE B IS SCORED AT THE SENDER ONLY — the clock is there. The
        # receiver rows print as DISCLOSURE and take no verdict, so they are
        # excluded from every count below rather than diluting them.
        bkeys = [k for k in sorted(L.raw_n) if k[1] == "cli" and B.get((k, g))]
        breject = [k for k in bkeys if B[(k, g)][1] == "REJECT"]
        bunsc = [k for k in bkeys
                 if B[(k, g)][1].startswith(("UNSCOREABLE", "CONFOUNDED"))]
        nb = [B[(k, g)][0] for k in bkeys if B[(k, g)][0] is not None]

        P("  === %s (%s class, n_warm=%d) ===" % (g, CLASS[g], N_WARM[g]))
        if worst is None:
            v = "UNSCOREABLE-NO-SCOREABLE-LEG"
            P("    S: no leg reached %d pooled post-warm-up readings." % MIN_READS)
        else:
            P("    S: worst leg %s/%s/p%d  R_total = %.3f   (best %s/%s/p%d = %.3f)"
              % (worst[0][0], worst[0][1], worst[0][2], worst[1]["R_total"],
                 best[0][0], best[0][1], best[0][2], best[1]["R_total"]))
            P("       %d scoreable leg(s), %d unscoreable-thin" % (len(scoreable), len(thin)))
            if worst[1]["R_total"] > ACCEPT_BAR:
                v = ("REJECT-S (R_total %.3f > %.1f at %s/%s/p%d; the worst "
                     "leg binds, §3 rule 1)"
                     % (worst[1]["R_total"], ACCEPT_BAR, worst[0][0],
                        worst[0][1], worst[0][2]))
            elif cfail:
                v = "REJECT-C (n_warm over the C2 bar at %s)" % "; ".join(cfail[:3])
            elif breject:
                v = ("REJECT-B (beta outside [%.2f, %.2f] at %s)"
                     % (B_REJECT_LO, B_REJECT_HI,
                        "; ".join("%s/%s/p%d" % k for k in breject[:3])))
            elif not bkeys or (bunsc and len(bunsc) == len(bkeys)):
                # AN UNEVALUATED CLAUSE IS NOT A PASSED ONE, and "no probe row
                # at all" is the same state as "every probe row unscoreable".
                # Without this branch a battery whose probe never ran would
                # hand out an ACCEPT on two clauses out of three.
                v = ("ADMISSIBLE-ON-S, B-UNSCOREABLE at every sender leg — NOT "
                     "an ACCEPT. B could not be evaluated, and an unevaluated "
                     "clause is not a passed one.")
            else:
                v = "ACCEPT (plain-window seat)"
                if worst[1]["R_total"] <= PREFER_BAR:
                    v = "PREFER (plain-window seat) — R_total <= %.1f at every leg" % PREFER_BAR
        P("    C: %s" % ("clean at every leg" if not cfail
                         else "FAIL at %d leg(s): %s" % (len(cfail), "; ".join(cfail[:3]))))
        P("    B: %d beta(s) computed, median %s, %d REJECT, %d unscoreable/confounded"
          % (len(nb), median(nb) if nb else "-", len(breject), len(bunsc)))
        P("    ==> %s\n" % v)
        order.append((g, v, worst[1]["R_total"] if worst else None))

    P("  --- THE TIE-BREAK, PRE-COMMITTED (§3 rule 2) ---")
    adm = [(g, r) for g, v, r in order if v.startswith(("ACCEPT", "PREFER"))]
    if not adm:
        P("  NO CANDIDATE ACCEPTS AT EVERY LEG. Per the battery's own")
        P("  pre-commitment, goal #101 item 2 closes NEEDS-MORE with the")
        P("  failing clause named above. THE BAR IS NOT SOFTENED and no")
        P("  candidate is promoted on a partial clause.")
    elif len(adm) == 1:
        P("  ONE admissible candidate: %s (R_total %.3f). No tie to break." % adm[0])
    else:
        adm.sort(key=lambda t: t[1])
        P("  %d admissible. Tie broken by the PREFER tier (R_total <= %.1f)"
          % (len(adm), PREFER_BAR))
        for g, r in adm:
            P("    %-6s worst R_total %.3f  %s"
              % (g, r, "PREFER tier" if r <= PREFER_BAR else "accept tier only"))
    P("\n  SCOPE, STANDING: every verdict above is at the PLAIN-WINDOW SEAT.")
    P("  §16.74.5 requirement 3 names the generation seat as a SECOND seat and")
    P("  this battery did not run it. An estimator qualified at one seat is")
    P("  NOT qualified at the other, by the requirement's own words.")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
