#!/usr/bin/env python3
"""THE T-LAG BATTERY'S SCORER — the acceptance bar, applied LITERALLY, to FIVE
gauges, with clause `B` REBUILT.

  usage: tlagb_report.py <ledger.log> [<ledger.log> ...]
                         [--bpass <ledger.log>]... [--dump <dir|file>]...

Scores goal-gate "THE SIGMA ESTIMATOR — THE ACCEPTANCE BAR" and NOTHING ELSE.
Every threshold below is transcribed from that section with its clause named;
no number in this file was chosen here, and the ones derived HERE (`MIN_READS`,
`QSP_STRUCTURAL_C`) carry their derivation on the line.

WHAT IS DIFFERENT FROM `sigb_report.py`, AND WHY EACH DIFFERENCE EXISTS.

  1. FIVE GAUGES. `tlag_us` (paper §16.75) is scored beside the four, on the
     same rows, with the same bars. `sig`/`rvar`/`qsp`/`msd` ARE NOW CONTROLS:
     they were scored on a committed ledger by the previous battery and their
     `R_total`s are transcribed below. §3c compares them and flags
     `CONTROL-DRIFT`.

     **THE PRE-REGISTERED CONSEQUENCE OF A DRIFTING CONTROL: NO VERDICT IS READ
     FROM THE `tlag` COLUMN AT ALL.** The successor's whole claim is a
     COMPARISON against its four predecessors measured in the same run. If the
     predecessors do not reproduce, the run is not the previous run's peer and
     the comparison has no referent — a `tlag` number read out of such a run
     would be a number about an unknown machine.

  2. `tlag`'s WARM-UP IS `K = 32` PAIRS, not samples (paper §16.75.6 F1),
     applied in `N_WARM` beside the other four and nowhere else. A reading
     resting on fewer than `K` pairs is `UNSCOREABLE-THIN`; a leg on which
     EVERY reading is that thin is reported `UNSCOREABLE-THIN-PAIRS` and is
     NEVER scored, because the alternative — an empty pool reported as
     "no sample" — hides a gauge that fired and was excluded behind a gauge
     that never fired.

  3. CLAUSE `B` IS SCORED FROM THE RAW `[RTTDUMP]` STREAM, NOT FROM THE 20 Hz
     PING PROBE. `tlagb_rttdump.population_functionals` evaluates each gauge's
     OWN functional over the identical samples the gauge consumed, so

         beta = (the gauge's ONLINE reading) / (its own functional, offline)

     is like-for-like BY CONSTRUCTION — same samples, same functional, no
     instrument mismatch and no sampling-rate gap.

     **THE REBUILT B CAN ACQUIT.** The old B was written REJECT-only because
     the probe's dispersion was a LOWER bound on the ack path's; there is no
     such bound here, because there is no second path involved. A candidate
     inside the band is recorded as reading its own functional faithfully, and
     that is a POSITIVE finding rather than an absence of evidence.

     The old `CONFOUNDED-` marking on `msd` is GONE. `msd` was confounded
     against a 20 Hz probe by a 500x sampling-rate gap; against its own stream
     there is no gap and nothing to confound. The old ping-probe beta is still
     printed, beside it, labelled THE SUPERSEDED REFERENCE, and NO VERDICT IS
     TAKEN FROM IT.

     AND THE NARROWING IS RECORDED AS ONE. The rebuilt B asks whether an
     estimator faithfully computes its functional over its own input. It does
     NOT ask whether that input is the true delivered latency. **The instrument
     the previous battery named as missing — a delivered-latency probe at the
     sender's own sample rate — is still missing.**

  4. THE CROSS-FUNCTIONAL LEVEL TABLE (§5b). All five population functionals on
     the SAME dumped stream, side by side, so the previously-unexplained
     90-100x level gap between `msd_us` and `sig_us` is READABLE as the
     functionals genuinely differing or not. REPORTED AND SCORED NOWHERE.

THE ORDER OF THE OUTPUT IS THE ORDER OF THE READING, AND IT IS
ABORT-CAUSE-FIRST. Aborts and witnesses print before any statistic, because a
statistic computed over an aborted invocation is not a weak result, it is not a
result. Then clause `C`, then clause `S`, then the control regression check,
then the sampling-rate row, then clause `B`, then the verdict.

DUMP FILE NAMING, so a leg can be matched to its rows. A dump file is mapped to
`(cell, seed, rep, site)` by its BASENAME, and the pattern is `tlagb_bpass.sh`'s
own capture name, transcribed rather than invented:

    <cell>-s<seed>-r<rep>-c.log[.gz]     the sender  (site = cli)
    <cell>-s<seed>-r<rep>-s.log[.gz]     the receiver (site = srv)
    ...-<cli|srv> and a leading `rttdump-` are accepted too.

`.gz` is read as `.gz` — the B pass compresses every preserved client log after
the run, so a report that could only read `.log` would silently score nothing
the day after the pass.

A file matching nothing is listed as `UNMAPPED-DUMP` and scored nowhere. It is
NEVER guessed at: a dump attributed to the wrong leg would compare one leg's
online reading against another leg's stream, which is the exact class of silent
mis-comparison this whole pass exists to remove.

WHAT THIS FILE WILL NOT DO. It will not soften a bar, average two seats, drop a
leg that fails, or report a pass for a quantity it could not compute.
"""
import json
import math
import os
import re
import sys
from collections import defaultdict

import latt_probe

#: Clause `B`'s new reference. Imported LAZILY (through `pop_module()`) and not
#: at import time, so that every clause this scorer can evaluate without a dump
#: — `S`, `C`, the witnesses, the controls — still evaluates on a machine where
#: only the ledger is present.
_POP = None


def pop_module():
    """`tlagb_rttdump`, or None with the reason printed by the caller."""
    global _POP
    if _POP is None:
        try:
            import tlagb_rttdump as _m
        except ImportError:
            return None
        _POP = _m
    return _POP


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
#: THE BANDS ARE UNCHANGED BY THE REFERENCE SWAP — the reference changed, the
#: bar did not, and widening it here would be a bar amendment written after the
#: instrument that needed it.
B_ACCEPT_LO, B_ACCEPT_HI = 0.68, 1.47
B_REJECT_LO, B_REJECT_HI = 0.50, 2.00
#: Two candidates BOTH inside the band that disagree with EACH OTHER by more
#: than the band's own width are a FINDING ABOUT THE PROBE (§4's last
#: sentence). The width is the band, not a new number: 1.47/0.68.
B_BAND_WIDTH = B_ACCEPT_HI / B_ACCEPT_LO

#: Clause `C1` — the warm-up exclusions, DECLARED PER ESTIMATOR CLASS BEFORE
#: THE RUN and applied HERE, as a scoring rule on the parser (clause `C3`:
#: never a gate in the engine). EWMA class 16 = the first count at which the
#: zero seed is under 1 % of the reading at beta = 1/4. Window class = `L`,
#: because a partly-full window reports a quantile of fewer order statistics
#: than it claims; `msd`'s count is the DIFFERENCE count, so its full set is
#: `L - 1`. `tlag`'s count is the PAIR count `|P(τ)|` and its floor is
#: `K = L/8 = 32` PAIRS — paper §16.75.6 F1, a PARSER rule and deliberately not
#: a threshold anywhere in the engine (`Path::rtt_tlag_samples`' own doc).
WINDOW_L = 256
TLAG_K = WINDOW_L // 8
N_WARM = {"sig": 16, "rvar": 16, "qsp": WINDOW_L, "msd": WINDOW_L - 1,
          "tlag": TLAG_K}
CLASS = {"sig": "EWMA", "rvar": "EWMA", "qsp": "window", "msd": "window",
         "tlag": "tau-band"}
#: What each gauge's `n` COUNTS. It is not decoration: `tlag`'s floor is 32
#: PAIRS and `msd`'s is 255 DIFFERENCES, and a reader comparing the two columns
#: without this line would be comparing two different denominators.
UNIT = {"sig": "samples", "rvar": "samples", "qsp": "samples",
        "msd": "differences", "tlag": "pairs"}

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

GAUGES = ("sig", "rvar", "qsp", "msd", "tlag")
#: The four gauges the previous battery scored. They are CONTROLS here.
CONTROLS = ("sig", "rvar", "qsp", "msd")

#: THE COMMITTED CONTROL VALUES, transcribed from goal-gate "THE SIGMA
#: ESTIMATOR — THE SCORED RESULT". Left column: `R_total` at the WORST leg
#: (the statistic the bar binds on). Right column: `R_total` on the DATA-PATH
#: leg (§3b's most generous domain). Nothing here was measured by this file.
CONTROL_R_WORST = {"sig": 256.3, "rvar": 351.3, "qsp": 78.6, "msd": 34.6}
CONTROL_R_DATAPATH = {"sig": 86.6, "rvar": 103.9, "qsp": 78.6, "msd": 8.667}
#: A control that moves by more than this FACTOR in either direction is
#: `CONTROL-DRIFT`. 2x is the pre-registered figure and it is deliberately
#: coarse: the controls' own spread across cells is larger than any subtle
#: effect this check could resolve, so a tighter bound would fire on noise and
#: a looser one would not fire on a changed machine.
CONTROL_DRIFT_X = 2.0

#: THE SUPERSEDED REFERENCE'S functional map — the 20 Hz ping probe's. Kept
#: ONLY to print the old beta as a disclosure column. `tlag` has no entry
#: because the probe never computed that functional and inventing one now would
#: manufacture a comparison the old instrument cannot make.
PROBE_FUNC = {"sig": "sd", "rvar": "sd", "qsp": "qsp", "msd": "msd",
              "tlag": None}

#: CLAUSE B'S SCORED MAP: each gauge against ITS OWN functional over its own
#: samples. Transcribed from `tlagb_rttdump.POP_FUNC`; §5 asserts the two agree
#: rather than assuming it.
POP_FUNC = {"sig": "sd", "rvar": "mad", "qsp": "qsp", "msd": "msd",
            "tlag": "tlag"}

# ── LEDGER PARSING ─────────────────────────────────────────────────────────

READ = re.compile(
    r"^TLAGBREAD (\S+) (\S+) (\S+) (\S+) p(\d+) blk=(\d+) rtp=(\d+) "
    r"sig=(-|\d+)/(\d+) rvar=(-|\d+)/(\d+) qsp=(-|\d+)/(\d+) "
    r"msd=(-|\d+)/(\d+) tlag=(-|\d+)/(\d+)")
PROBE = re.compile(r"^TLAGBPROBE (\S+) (\S+) (\S+) (\{.*\})$")
META = re.compile(r"^TLAGBMETA (\S+) (\S+) (\S+) (\{.*\})$")
WIT = re.compile(r"^TLAGBWITNESS (\{.*\})$")
#: The DRIVER's own witness row. Same shape, same `(cell, seed, rep)` keys,
#: same `gen_plateau` field — see `apply_voids`.
BAND = re.compile(r"^TLAGBBAND (\{.*\})$")
FAILMARK = re.compile(
    r"^(ABORT|ABORT-GEN-PLATEAU|OUT-OF-BAND|SUBSTRATE-FAIL|INSTRUMENT-FAIL-GATE"
    r"|INSTRUMENT-FAIL-PROBE|TLAGB-PARSE-FAIL|TLAGB-DUMP-ON-FAIL|W7-FAIL-CLI"
    r"|CELL-VANISHED|QCAP-MISSING|UNKNOWN-CELL)\b")


class Ledger:
    def __init__(self):
        #: (cell, site, pid) -> gauge -> list of (seed, rep, value)
        self.reads = defaultdict(lambda: defaultdict(list))
        #: (cell, site, pid) -> gauge -> list of (seed, rep, n) — every row,
        #: warm-up INCLUDED, so the exclusion is auditable from this object.
        self.raw_n = defaultdict(lambda: defaultdict(list))
        #: (cell, site, pid) -> list of (seed, rep, rtprop_ms). THE TLAG BAND'S
        #: τ, per leg, read off the block that produced the reading.
        self.rtp = defaultdict(list)
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
                    self.rtp[key].append((seed, rep, int(m.group(7))))
                    vals = m.groups()[7:]
                    for i, g in enumerate(GAUGES):
                        v, n = vals[2 * i], int(vals[2 * i + 1])
                        self.raw_n[key][g].append((seed, rep, n))
                        # CLAUSE C1 APPLIED HERE, AND ONLY HERE. `tlag`'s
                        # K = 32 PAIRS is one entry of the same table.
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
                m = WIT.match(ln) or BAND.match(ln)
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
        denominator."* The previous report printed the marker and went on
        pooling that invocation's gauge readings into `R_total` anyway, which
        is exactly the denominator the clause forbids. A single VOID rep at
        `c8`, where a leg carries ~9 readings, is ~6 % of a pooled quantile.

        THE VOID SET IS BUILT FROM THE WITNESS ROWS, NOT FROM THE MARKER TEXT,
        because a witness row carries `(cell, seed, rep)` while the marker line
        does not carry the seed — and two seeds pool here, so a marker matched
        by cell and rep alone would void the innocent seed's rep too. Both
        witness-row kinds are read: the PARSER's `TLAGBWITNESS` and the
        DRIVER's `TLAGBBAND`, which carry the same three keys and the same
        `gen_plateau` field. Neither is marker text; a rep aborted before the
        parser ran has only the driver's row, and leaving it in the denominator
        would be the very defect this method exists to close.

        AND THE PLATEAU IS NOT ITSELF THE VOID CAUSE — THE WITNESSES ARE.
        This is the amendment's §7, committed BEFORE the VM was touched, and
        it is a repair to a rule this tree wrote and then convicted. The
        previous battery hardened the goodput plateau into an unconditional
        abort, voided a rep on it, and then recorded in its own §2 that the
        hardening was WRONG:

            "a goodput band cannot discriminate a configuration:
             generation-on and 'this rep lost badly and retransmitted hard'
             both land at ~30 Mbit/s, and only W1/W2 tell them apart. The
             alpha-sweep's design -- witnesses primary, band secondary -- was
             right, and this battery's section 8 hardening of the band into an
             abort was wrong."

        So the precedence here is WITNESS-FIRST: a plateau reading whose `W1`
        reads `gen=0` AND whose `[PFRAC]` count is 0 is generation-OFF by
        direct engine echo, and is an OUT-OF-BAND **RESULT, RETAINED**, with
        its `gen=0` witness carried explicitly into the report. A plateau
        reading WITHOUT clean witnesses is a configuration fault and voids as
        before.

        **Retaining is also the honest direction for THIS battery specifically**
        -- a plateau rep is a heavy-loss, heavy-retransmit rep, and a
        high-dispersion rep is exactly the rep an ESTIMATOR battery must not
        discard. Discarding it would flatter every gauge's `R_total`.

        THE FALLBACK IS CONSERVATIVE AND IS STATED. The driver's `TLAGBBAND`
        row carries `gen_plateau` but no `W1`/`W2`; only the parser's
        `TLAGBWITNESS` carries those. So the evidence is MERGED per
        `(cell, seed, rep)` across both row kinds, and a rep that aborted
        before the parser ran has no witness at all -- unknown is NOT clean,
        and it voids.

        Returns the void set so the report can print it before any statistic.
        """
        # Merge both row kinds per (cell, seed, rep): the DRIVER's row carries
        # the plateau flag, the PARSER's carries the witnesses, and the rule
        # needs both.
        ev = {}
        for w in self.wit:
            k = (w["cell"], str(w["seed"]), str(w["rep"]))
            e = ev.setdefault(k, {"plateau": False, "w1": None, "w2": None,
                                  "mbps": None})
            if w.get("gen_plateau"):
                e["plateau"] = True
            if w.get("mbps") is not None:
                e["mbps"] = w.get("mbps")
            if "W1_rfa_gen" in w:
                e["w1"] = w.get("W1_rfa_gen")
            if "W2_pfrac_lines" in w:
                e["w2"] = w.get("W2_pfrac_lines")

        void = set()
        #: Plateau reps RETAINED by the witness-first rule, each with the
        #: `gen=0` witness that acquitted it. Printed by the report; a
        #: retention that is not visible is indistinguishable from an
        #: exclusion nobody noticed.
        self.plateau_retained = []
        for k, e in sorted(ev.items()):
            if not e["plateau"]:
                continue
            if e["w1"] == 0 and e["w2"] == 0:
                self.plateau_retained.append((k, e["mbps"], e["w1"], e["w2"]))
            else:
                void.add(k)
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
        for key in list(self.rtp):
            self.rtp[key] = [t for t in self.rtp[key]
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


def s_verdict(st, g=None, raw_ns=None):
    """Clause `S` at one leg.

    `g` and `raw_ns` are optional and they change ONE outcome: an empty pool
    whose RAW rows all sat below the gauge's own `n_warm` is reported
    `UNSCOREABLE-THIN-*` rather than `UNSCOREABLE-NO-SAMPLE`. The two states are
    different findings — a gauge that fired and was excluded is not a gauge
    that never fired — and for `tlag`, whose floor is 32 PAIRS on a band that
    can legitimately be empty at a sparse leg, collapsing them would hide the
    exclusion the paper declared.
    """
    if st is None or st["n"] == 0:
        if g is not None and raw_ns:
            floor = N_WARM[g]
            if all(n < floor for n in raw_ns):
                return ("UNSCOREABLE-THIN-%s(every one of %d reading(s) below "
                        "n_warm=%d %s)"
                        % ("PAIRS" if g == "tlag" else "WARMUP",
                           len(raw_ns), floor, UNIT[g]))
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


def b_verdict_probe(beta):
    """THE SUPERSEDED REFERENCE'S verdict function, UNCHANGED and UNREAD.

    The 20 Hz ping probe could REJECT but never ACQUIT, so a candidate inside
    its band was recorded `NOT-SHOWN-BIASED`, never `unbiased`. That wording is
    preserved here EXACTLY, because the disclosure column is a quotation of the
    previous battery's instrument and a quotation that changed its verdict
    vocabulary would not be one. NOTHING reads this in the verdict.
    """
    if beta is None:
        return "UNSCOREABLE"
    if B_ACCEPT_LO <= beta <= B_ACCEPT_HI:
        return "NOT-SHOWN-BIASED"
    if B_REJECT_LO <= beta <= B_REJECT_HI:
        return "ADMISSIBLE-BIAS-CARRIED"
    return "REJECT"


def b_verdict(beta):
    """THE REBUILT CLAUSE B'S verdict, AND IT CAN ACQUIT.

    beta is the gauge's online reading over ITS OWN functional evaluated on the
    IDENTICAL samples. There is no second path, no second instrument and no
    sampling-rate gap, so the old lower-bound asymmetry that made B
    REJECT-only does not exist here. Inside the band is a POSITIVE finding:
    the online implementation reads the same magnitude as the functional it
    claims to compute.
    """
    if beta is None:
        return "UNSCOREABLE"
    if B_ACCEPT_LO <= beta <= B_ACCEPT_HI:
        return "ACQUIT"
    if B_REJECT_LO <= beta <= B_REJECT_HI:
        return "ADMISSIBLE-BIAS-CARRIED"
    return "REJECT"


# ── THE DUMP SIDE ──────────────────────────────────────────────────────────

#: THE B PASS'S OWN CAPTURE NAME, transcribed from `tlagb_bpass.sh`:
#: `$DUMPDIR/${cell}-s${SEED}-r${REP}-c.log`, `-s.log` for the receiver, and
#: `.gz` after the post-run compression pass. `c`/`s` are the driver's letters
#: for the two seats; `cli`/`srv` are accepted as well so a hand-assembled dump
#: directory reads too. The alternation is LONGEST-FIRST so `cli` never matches
#: as `c` and leaves `li` behind.
DUMP_NAME = re.compile(
    r"^(?:rttdump[-_])?(?P<cell>[A-Za-z0-9]+)[-_.]s(?P<seed>\d+)"
    r"[-_.]r(?P<rep>\d+)[-_.](?P<site>cli|srv|c|s)\b")
DUMP_NAME_LOOSE = re.compile(
    r"(?P<cell>[A-Za-z0-9]+)[-_.]s(?P<seed>\d+)[-_.]r(?P<rep>\d+)"
    r"[-_.](?P<site>cli|srv|c|s)\b")
SITE_LETTER = {"c": "cli", "s": "srv", "cli": "cli", "srv": "srv"}


def map_dump_name(base):
    m = DUMP_NAME.match(base) or DUMP_NAME_LOOSE.search(base)
    if not m:
        return None
    return (m.group("cell"), m.group("seed"), m.group("rep"),
            SITE_LETTER[m.group("site")])


def dump_lines(fp):
    """The capture, decompressed if it is one. `tlagb_bpass.sh` gzips every
    preserved client log after the run (they are megabytes), so a report that
    could only read `.log` would silently score nothing the day after the pass.
    `parse_dump` takes an iterable of lines, so no temporary file is made.
    """
    if fp.endswith(".gz"):
        import gzip
        return gzip.open(fp, "rt", errors="replace")
    return open(fp, "r", errors="replace")


def collect_dumps(paths):
    """(mapped, unmapped) — `mapped` is {(cell,seed,rep,site): [file, ...]}."""
    files = []
    for p in paths:
        if os.path.isdir(p):
            for name in sorted(os.listdir(p)):
                fp = os.path.join(p, name)
                if os.path.isfile(fp):
                    files.append(fp)
        elif os.path.isfile(p):
            files.append(p)
    mapped, unmapped = defaultdict(list), []
    for fp in files:
        k = map_dump_name(os.path.basename(fp))
        if k is None:
            unmapped.append(fp)
        else:
            mapped[k].append(fp)
    return mapped, unmapped


def leg_tau_us(BL, key, seed=None, rep=None):
    """τ FOR THIS LEG, IN µs, from the leg's own `rtp<floor>ms` rows.

    The invocation's own rows bind when they exist; the leg's rows across the
    pass are the fallback and the caller says which it used. τ is NEVER
    defaulted to a constant — the engine gauge has no fallback either
    (paper §16.75.6 F2), and a τ invented offline would select a different pair
    set from the one the online reading rests on.
    """
    rows = BL.rtp.get(key, [])
    own = [v for s, r, v in rows if (seed is None or s == seed)
           and (rep is None or r == rep)]
    src = "invocation"
    if not own:
        own, src = [v for _, _, v in rows], "leg (all reps)"
    if not own:
        return None, "none"
    return int(round(median(own) * 1000.0)), src


# ── THE REPORT ─────────────────────────────────────────────────────────────

def parse_argv(argv):
    ledgers, bpass, dumps = [], [], []
    i = 0
    while i < len(argv):
        a = argv[i]
        if a == "--bpass":
            bpass.append(argv[i + 1])
            i += 2
        elif a == "--dump":
            dumps.append(argv[i + 1])
            i += 2
        elif a.startswith("--"):
            print("unknown option %s" % a, file=sys.stderr)
            return None
        else:
            ledgers.append(a)
            i += 1
    return ledgers, bpass, dumps


def main(argv):
    parsed = parse_argv(argv)
    if not parsed or not (parsed[0] or parsed[1]):
        print(__doc__.splitlines()[3].strip(), file=sys.stderr)
        return 2
    ledgers, bpass, dumps = parsed
    L = Ledger()
    for p in ledgers:
        L.load(p)
    VOID = L.apply_voids()

    # The clause-B pass is a SEPARATE ledger with the dump ON, because the dump
    # perturbs the very quantity clause S measures. When none is given, the S/C
    # ledger is used for B's online readings and the report says so — a
    # DIFFERENT and weaker statement, made visibly.
    if bpass:
        BL = Ledger()
        for p in bpass:
            BL.load(p)
        BVOID = BL.apply_voids()
        bsrc = ", ".join(bpass)
    else:
        BL, BVOID, bsrc = L, VOID, "(none — clause B reads the S/C ledger)"

    P = print
    P("=" * 78)
    P("THE T-LAG ESTIMATOR BATTERY — SCORED AGAINST 'THE SIGMA ESTIMATOR — THE")
    P("ACCEPTANCE BAR' AND AGAINST NOTHING ELSE. FIVE GAUGES: FOUR CONTROLS")
    P("AND ONE CANDIDATE (`tlag`, paper §16.75).")
    P("ledgers:  %s" % (", ".join(ledgers) or "(none)"))
    P("B pass:   %s" % bsrc)
    P("dumps:    %s" % (", ".join(dumps) or "(none — clause B UNSCOREABLE)"))
    P("bars: S accept R_total<=%.1f, prefer <=%.1f (k-ratio %.3f) | "
      "B accept [%.2f,%.2f], reject outside [%.2f,%.2f] | C n_warm<=%.0f%% N"
      % (ACCEPT_BAR, PREFER_BAR, K_RATIO, B_ACCEPT_LO, B_ACCEPT_HI,
         B_REJECT_LO, B_REJECT_HI, 100 * C2_FRAC))
    P("warm-up (clause C1, applied by THIS parser): %s"
      % " ".join("%s>=%d %s(%s)" % (g, N_WARM[g], UNIT[g], CLASS[g])
                 for g in GAUGES))
    P("=" * 78)

    # ── 1. ABORT-CAUSE FIRST ────────────────────────────────────────────
    P("\n## 1 — ABORTS AND WITNESSES, BEFORE ANY STATISTIC\n")
    allfails = L.fails + [f for f in BL.fails if BL is not L]
    if allfails:
        P("  %d abort/fail marker(s):" % len(allfails))
        for f in allfails:
            P("    %s" % f)
    else:
        P("  0 abort/fail markers.")

    # ── 1a. THE PLATEAU RETENTIONS, BEFORE THE VOIDS ────────────────────
    # A retention that is not visible is indistinguishable from an exclusion
    # nobody noticed, so these print FIRST and carry the witness that acquitted
    # them on their face.
    retained = list(getattr(L, "plateau_retained", []))
    if BL is not L:
        retained += list(getattr(BL, "plateau_retained", []))
    P("\n  GOODPUT-PLATEAU READINGS, RETAINED BY THE WITNESS-FIRST RULE")
    P("  (amendment §7, committed BEFORE the VM was touched). A reading inside")
    P("  the 26.8-34.1 Mbit/s plateau with W1 gen=0 AND zero [PFRAC] lines is")
    P("  generation-OFF by DIRECT ENGINE ECHO, so it is an OUT-OF-BAND RESULT")
    P("  and NOT a configuration abort. A goodput band cannot discriminate a")
    P("  configuration -- generation-on and 'this rep lost badly and")
    P("  retransmitted hard' both land at ~30 Mbit/s, and only W1/W2 tell them")
    P("  apart. Retaining is also the honest direction here: a plateau rep is a")
    P("  heavy-loss, heavy-retransmit rep, and a HIGH-DISPERSION rep is exactly")
    P("  the rep an ESTIMATOR battery must not discard.")
    if retained:
        for (c, s, r), mb, w1, w2 in retained:
            P("    RETAINED %s seed=%s rep=%s mbps=%s "
              "[W1 gen=%s, W2 pfrac=%s — generation definitively OFF]"
              % (c, s, r, mb, w1, w2))
        P("  %d plateau reading(s) retained as RESULTS." % len(retained))
    else:
        P("    none — no reading landed in the plateau with clean witnesses.")

    P("\n  VOIDED INVOCATIONS — dropped from EVERY container before any")
    P("  statistic below, per §8: an aborted invocation is no datum and is NOT")
    P("  in any denominator. A plateau reading voids HERE only when its")
    P("  witnesses are dirty or absent — unknown is not clean.")
    if VOID or BVOID:
        for c, s, r in sorted(set(VOID) | set(BVOID)):
            P("    VOID %s seed=%s rep=%s" % (c, s, r))
        P("  %d invocation(s) voided. Their gauge readings, probe rows, rtp"
          % len(set(VOID) | set(BVOID)))
        P("  rows and meta rows are ABSENT from clause S, clause B and the")
        P("  rate row.")
    else:
        P("    none.")

    n_inv = len(L.wit)
    bad = {"W1": [], "W2": [], "W4": [], "W5": [], "W7": [], "WDUMP": [],
           "PLATEAU": []}
    for w in L.wit:
        if "W7_group_misses_cli" not in w:
            continue                       # a TLAGBBAND row: void source only
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
        if w.get("W_dump_batches_cli"):
            bad["WDUMP"].append("%s batches=%s" % (tag, w["W_dump_batches_cli"]))
        if w.get("gen_plateau"):
            bad["PLATEAU"].append("%s mbps=%s" % (tag, w.get("mbps")))
    P("\n  %d witness row(s)." % n_inv)
    for k in ("W1", "W2", "W4", "W5", "W7", "WDUMP", "PLATEAU"):
        P("    %-8s %s" % (k, "clean" if not bad[k]
                           else "FAIL at %d: %s" % (len(bad[k]),
                                                    "; ".join(bad[k][:6]))))
    P("\n  W7 is THIS BATTERY'S OWN reachability gate: all FIVE gauge tokens")
    P("  with their /n counts, plus the block's own rtp<>ms, on every path")
    P("  entry of every [DIAG] block, both endpoints. A miss is the")
    P("  MEASUREMENT failing, not a column absent.")
    P("  W-DUMP: the S/C ledger's invocations must carry ZERO [RTTDUMP]")
    P("  batches. The dump writes megabytes of SENDER stderr and sender-side")
    P("  dispersion is the quantity clause S measures, so a scored invocation")
    P("  with the dump live was measuring its own instrument.")

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
            claim = ("NO-THROUGHPUT-TARGET — headroom < %.0f%% (discipline 16c)"
                     % HEADROOM_BAR)
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
    legN = {}
    for key in sorted(L.raw_n):
        ns = [n for _, _, n in L.raw_n[key]["sig"]]
        legN[key] = max(ns) if ns else 0

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
                c_fail[g].append("%s (N_cell=%d, bar=%.0f, n_warm=%d %s)"
                                 % (c, N, bar, N_WARM[g], UNIT[g]))
        P("  %-6s %14d %10.0f   %s" % (c, N, bar, " ".join(verd)))
    P("\n  Clause C2's binding cell was pre-registered as c8 at N ~ 17 660,")
    P("  giving n_warm <= 883. The table above uses THIS battery's own counts.")
    P("  `tlag`'s n_warm is %d PAIRS and the C2 bar is a SAMPLE count, so its"
      % TLAG_K)
    P("  row is CONSERVATIVE: |P(τ)| <= the sample count by construction (one")
    P("  pair per anchor at most), so a `tlag` row that clears C2 read against")
    P("  samples clears it against its own denominator too.")

    P("\n  DISCLOSURE — THE PER-LEG COUNTS, WHICH ARE NOT A CLAUSE-C VERDICT.")
    P("  A leg whose N cannot reach a gauge's own n_warm never produces a")
    P("  post-warm-up reading there, and clause S reports that as")
    P("  UNSCOREABLE — the honest statement, since the gauge was not measured")
    P("  at that leg rather than measured and found wanting.\n")
    P("  %-6s %-5s %-4s %10s %10s %8s   %s"
      % ("cell", "site", "path", "N (max n)", "5% of N", "rtp ms",
         "window class reachable?"))
    for key in sorted(legN):
        cell, site, pid = key
        N = legN[key]
        tau_ms = median([v for _, _, v in L.rtp.get(key, [])])
        reach = ("qsp=%s msd=%s tlag(max pairs %d)=%s"
                 % ("yes" if N >= N_WARM["qsp"] else "NO",
                    "yes" if N >= N_WARM["msd"] else "NO",
                    max([n for _, _, n in L.raw_n[key]["tlag"]] or [0]),
                    "yes" if max([n for _, _, n in L.raw_n[key]["tlag"]]
                                 or [0]) >= TLAG_K else "NO"))
        P("  %-6s %-5s p%-3d %10d %10.0f %8s   %s"
          % (cell, site, pid, N, C2_FRAC * N,
             tau_ms if tau_ms is not None else "-", reach))

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
            v = s_verdict(st, g, [n for _, _, n in L.raw_n[key][g]])
            if st is None:
                P("      %-6s %8d %8s %8s %8s %9s %9s  %s"
                  % (g, 0, "-", "-", "-", "-", "-", v))
            else:
                P("      %-6s %8d %8s %8s %8s %9s %9s  %s"
                  % (g, st["n"], st["p05"], st["p50"], st["p95"],
                     st["R_total"], st["sup_inf"], v))

    # ── 3b. IS THE VERDICT AN ARTEFACT OF THE SCORING DOMAIN? ───────────
    P("\n## 3b — THE SAME CLAUSE S ON THE BAR'S OWN MOST GENEROUS DOMAIN:")
    P("##      THE DATA-PATH LEG OF EACH CELL (the leg `N_cell` refers to).\n")
    dp = {}
    for c in sorted({k[0] for k in L.raw_n}):
        legs = [k for k in L.raw_n if k[0] == c]
        best = max(legs, key=lambda k: max(
            [n for _, _, n in L.raw_n[k]["sig"]] or [0]))
        dp[c] = best
    P("  %-6s %-12s %9s %9s %9s %9s %9s"
      % ("cell", "data-path leg", *GAUGES))
    dp_worst = {g: (None, 0.0) for g in GAUGES}
    for c, key in dp.items():
        row_ = []
        for g in GAUGES:
            st = S.get((key, g))
            r = (st["R_total"] if (st and st["n"] >= MIN_READS and st["R_total"])
                 else None)
            row_.append("%.2f" % r if r else "-")
            if r and r > dp_worst[g][1]:
                dp_worst[g] = (c, r)
        P("  %-6s %-12s %9s %9s %9s %9s %9s"
          % (c, "%s/p%d" % (key[1], key[2]), *row_))
    P("\n  worst DATA-PATH cell per gauge, against the accept bar of %.1f:"
      % ACCEPT_BAR)
    for g in GAUGES:
        c, r = dp_worst[g]
        P("    %-6s %-5s R_total = %-9s %s"
          % (g, c or "-", ("%.3f" % r) if r else "-",
             "CLEARS THE BAR" if (r and r <= ACCEPT_BAR)
             else ("FAILS by %.2fx" % (r / ACCEPT_BAR) if r else "unscoreable")))

    # ── 3c. THE CONTROL REGRESSION CHECK ────────────────────────────────
    P("\n## 3c — THE CONTROL REGRESSION CHECK. THE FOUR OLD GAUGES ARE CONTROLS")
    P("##      HERE, AND THIS SECTION IS THE ONLY THING THAT LICENSES READING")
    P("##      THE `tlag` COLUMN AT ALL.\n")
    P("  `tlag`'s entire claim is a COMPARISON against four predecessors")
    P("  measured in the SAME run from the SAME sample stream. If the")
    P("  predecessors do not reproduce their committed readings, this run is")
    P("  not the previous run's peer, the comparison has no referent, and a")
    P("  `tlag` number taken out of it would be a number about an unknown")
    P("  machine.\n")
    P("  PRE-REGISTERED CONSEQUENCE, STATED BEFORE THE NUMBERS: **IF ANY")
    P("  CONTROL MOVES BY MORE THAN %.1fx IN EITHER DIRECTION, NO VERDICT IS"
      % CONTROL_DRIFT_X)
    P("  READ FROM THE `tlag` COLUMN.** Not a softened one, not a provisional")
    P("  one — none.\n")
    worstR = {}
    for g in GAUGES:
        cand = [S[(k, g)]["R_total"] for k in L.raw_n
                if S.get((k, g)) and S[(k, g)]["n"] >= MIN_READS
                and S[(k, g)]["R_total"]]
        worstR[g] = max(cand) if cand else None
    P("  %-6s %14s %12s %8s   %14s %12s %8s   %s"
      % ("gauge", "committed worst", "this worst", "x", "committed d-path",
         "this d-path", "x", "verdict"))
    drift = []
    for g in CONTROLS:
        now_w, now_d = worstR[g], dp_worst[g][1] or None
        cw, cd = CONTROL_R_WORST[g], CONTROL_R_DATAPATH[g]
        xw = (now_w / cw) if now_w else None
        xd = (now_d / cd) if now_d else None
        flags = []
        for nm, x in (("worst", xw), ("data-path", xd)):
            if x is None:
                flags.append("%s UNREPRODUCED" % nm)
            elif x > CONTROL_DRIFT_X or x < 1.0 / CONTROL_DRIFT_X:
                flags.append("%s %.2fx" % (nm, x))
        v = "reproduces" if not flags else "CONTROL-DRIFT (%s)" % "; ".join(flags)
        if flags:
            drift.append("%s: %s" % (g, "; ".join(flags)))
        P("  %-6s %14.3f %12s %8s   %14.3f %12s %8s   %s"
          % (g, cw, ("%.3f" % now_w) if now_w else "-",
             ("%.2f" % xw) if xw else "-", cd,
             ("%.3f" % now_d) if now_d else "-",
             ("%.2f" % xd) if xd else "-", v))
    CONTROL_DRIFT = bool(drift)
    P("")
    if CONTROL_DRIFT:
        P("  ==> CONTROL-DRIFT. %d control reading(s) moved by more than %.1fx:"
          % (len(drift), CONTROL_DRIFT_X))
        for d in drift:
            P("        %s" % d)
        P("  ==> THE PRE-REGISTERED CONSEQUENCE APPLIES: NO VERDICT IS READ")
        P("      FROM THE `tlag` COLUMN. Its statistics are still printed, and")
        P("      they are printed as MEASUREMENTS OF AN UNKNOWN MACHINE.")
    else:
        P("  ==> THE CONTROLS REPRODUCE within %.1fx. The comparison has its"
          % CONTROL_DRIFT_X)
        P("      referent and the `tlag` column is readable.")
    P("\n  A control being UNREPRODUCED (no scoreable leg at all) counts as")
    P("  drift. An absent control is not a passing one — the same rule clause")
    P("  B applies to itself.")

    # ── 4. THE SAMPLING-RATE ROW ────────────────────────────────────────
    P("\n## 4 — THE SAMPLING-RATE ROW, FIRST-CLASS. `msd` ESTIMATES DISPERSION")
    P("##     AT A LAG OF ONE INTER-SAMPLE INTERVAL, SO ITS MAGNITUDE DEPENDS")
    P("##     ON THE SAMPLING RATE, WHICH IS NOT A PROPERTY OF THE LINK. THIS")
    P("##     IS THE DEFECT `tlag` EXISTS TO REMOVE, SO ITS COLUMN SITS HERE.\n")
    P("  The previous battery measured R_total tracking the sample rate at")
    P("  rho = -0.548 over eight sender legs, with the two thinnest legs the")
    P("  two worst readings. `tlag` selects pairs by ELAPSED TIME, so its own")
    P("  rho is the direct reading of whether that was fixed.\n")
    P("  %-6s %-5s %-4s %10s %9s %9s %9s %9s %9s"
      % ("cell", "site", "path", "samp/s", "msd p50", "sig p50", "tlag p50",
         "msd R_tot", "tlag R_tot"))
    rate_x, rtot_y, tl_x, tl_y = [], [], [], []
    for key in sorted(L.raw_n):
        cell, site, pid = key
        ns = [n for _, _, n in L.raw_n[key]["sig"]]
        N = max(ns) if ns else 0
        secs = [m["seconds"] for k, m in L.meta.items()
                if k[0] == cell and m.get("seconds")]
        wall = median(secs) if secs else None
        rate = (N / wall) if (wall and N) else None
        smsd, ssig, stl = S[(key, "msd")], S[(key, "sig")], S[(key, "tlag")]
        P("  %-6s %-5s p%-3d %10s %9s %9s %9s %9s %9s"
          % (cell, site, pid,
             ("%.1f" % rate) if rate else "-",
             smsd["p50"] if smsd else "-", ssig["p50"] if ssig else "-",
             stl["p50"] if stl else "-",
             smsd["R_total"] if smsd else "-",
             stl["R_total"] if stl else "-"))
        if rate and smsd and smsd["R_total"] and smsd["n"] >= MIN_READS:
            rate_x.append(rate)
            rtot_y.append(smsd["R_total"])
        if rate and stl and stl["R_total"] and stl["n"] >= MIN_READS:
            tl_x.append(rate)
            tl_y.append(stl["R_total"])
    P("\n  Spearman rank correlation, rate vs R_total:")
    P("    msd   over %d scoreable leg(s)   rho = %s"
      % (len(rate_x), spearman(rate_x, rtot_y)))
    P("    tlag  over %d scoreable leg(s)   rho = %s"
      % (len(tl_x), spearman(tl_x, tl_y)))
    for seat in ("cli", "srv"):
        sx, sy, tx, ty = [], [], [], []
        for key in sorted(L.raw_n):
            if key[1] != seat:
                continue
            ns = [n for _, _, n in L.raw_n[key]["sig"]]
            N = max(ns) if ns else 0
            secs = [m["seconds"] for k, m in L.meta.items()
                    if k[0] == key[0] and m.get("seconds")]
            wall = median(secs) if secs else None
            if not (wall and N):
                continue
            smsd, stl = S.get((key, "msd")), S.get((key, "tlag"))
            if smsd and smsd["R_total"] and smsd["n"] >= MIN_READS:
                sx.append(N / wall)
                sy.append(smsd["R_total"])
            if stl and stl["R_total"] and stl["n"] >= MIN_READS:
                tx.append(N / wall)
                ty.append(stl["R_total"])
        P("    [seat=%s] msd %d leg(s) rho = %s | tlag %d leg(s) rho = %s"
          % (seat, len(sx), spearman(sx, sy), len(tx), spearman(tx, ty)))
    P("  A |rho| near 1 says the RATE, not the cell, orders the gauge. NO")
    P("  VERDICT IS TAKEN FROM rho ALONE — it is reported beside the per-leg")
    P("  clause-S verdicts, which are what binds.")

    # ── 5. CLAUSE B, REBUILT ────────────────────────────────────────────
    P("\n## 5 — CLAUSE B, REBUILT: beta vs THE GAUGE'S OWN FUNCTIONAL OVER THE")
    P("##     GAUGE'S OWN SAMPLES, FROM THE RAW [RTTDUMP] STREAM.")
    P("##     **THE REBUILT B CAN ACQUIT.**\n")
    P("  beta = (the gauge's ONLINE reading) / (its own functional, offline,")
    P("  over the IDENTICAL samples). Same stream, same functional, no second")
    P("  instrument, no sampling-rate gap — like-for-like BY CONSTRUCTION.")
    P("  The old B was written REJECT-only because a 20 Hz ICMP probe's")
    P("  dispersion is a LOWER bound on the ack path's. There is no such bound")
    P("  here because there is no second path, so a candidate inside the band")
    P("  is a POSITIVE finding: the online implementation reads the same")
    P("  magnitude as the functional it claims to compute. It ACQUITS.\n")
    P("  THE OLD `CONFOUNDED-` MARKING ON `msd` IS GONE. `msd` was confounded")
    P("  against the 20 Hz probe by a ~500x sampling-rate gap. Against its own")
    P("  stream there is no gap and nothing to confound.\n")
    P("  AND THE NARROWING IS RECORDED AS ONE. B now asks whether an estimator")
    P("  faithfully computes ITS functional over ITS input. It does NOT ask")
    P("  whether that input is the true delivered latency. **THE INSTRUMENT")
    P("  THE PREVIOUS BATTERY NAMED AS MISSING — a delivered-latency probe at")
    P("  the sender's own sample rate — IS STILL MISSING.** The 20 Hz probe's")
    P("  beta is printed beside every row as THE SUPERSEDED REFERENCE and NO")
    P("  VERDICT IS TAKEN FROM IT.\n")

    POP = pop_module()
    mapped, unmapped = collect_dumps(dumps)
    if POP is None:
        P("  CLAUSE B UNSCOREABLE: `tlagb_rttdump` could not be imported.")
    elif POP.POP_FUNC != POP_FUNC:
        P("  CLAUSE B UNSCOREABLE: the functional map in this file and the one")
        P("  in `tlagb_rttdump` DISAGREE. A beta computed across a map")
        P("  disagreement measures the disagreement and nothing else.")
        P("    here: %s" % POP_FUNC)
        P("    there: %s" % POP.POP_FUNC)
        POP = None
    elif not mapped:
        P("  CLAUSE B UNSCOREABLE: no dump file was mapped to a leg. Pass")
        P("  `--bpass <ledger> --dump <dir>` from the clause-B pass.")
    if unmapped:
        P("  %d UNMAPPED-DUMP file(s), scored NOWHERE (a dump attributed to the"
          % len(unmapped))
        P("  wrong leg would compare one leg's reading to another leg's")
        P("  stream):")
        for fp in unmapped[:8]:
            P("    UNMAPPED-DUMP %s" % fp)

    #: (cell, site, pid) -> gauge -> list of (beta, verdict), ONE ENTRY PER
    #: DUMPED INVOCATION. A list and not a scalar because two dumped reps of
    #: one leg are two betas, and keeping only the last would silently drop a
    #: REJECT behind an ACQUIT that happened to be read second.
    B = defaultdict(list)
    POPVALS = {}          # (key, seed, rep) -> (pop dict, dumpinfo)
    if POP is not None:
        for (cell, seed, rep, site) in sorted(mapped):
            files = mapped[(cell, seed, rep, site)]
            per_path = {}
            for fp in files:
                with dump_lines(fp) as fh:
                    parsed_paths = POP.parse_dump(fh)
                for pid, s in parsed_paths.items():
                    if pid not in per_path:
                        per_path[pid] = s
                    else:
                        per_path[pid]["series"].extend(s["series"])
                        per_path[pid]["emitted"] += s["emitted"]
                        per_path[pid]["capped"] |= s["capped"]
            for pid in sorted(per_path):
                s = per_path[pid]
                s["series"].sort(key=lambda t: t[0])
                key = (cell, site, pid)
                tau_us, tau_src = leg_tau_us(BL, key, seed, rep)
                pop = POP.population_functionals(s["series"], tau_us)
                POPVALS[(key, seed, rep)] = (pop, s, tau_us, tau_src)

                P("  --- %s site=%s p%d  seed=%s rep=%s ---"
                  % (cell, site, pid, seed, rep))
                P("      dump: n=%d emitted=%d capped=%s tau=%s us (from %s)"
                  % (pop["n"], s["emitted"], s["capped"],
                     tau_us if tau_us is not None else "-", tau_src))
                if s["capped"]:
                    P("      PREFIX-SCORED: the per-path cap bound, so every")
                    P("      functional above is over a contiguous TIME PREFIX")
                    P("      of the run. B stays like-for-like — every")
                    P("      functional is over the SAME prefix — but the leg's")
                    P("      B is a statement about that prefix.")
                plegs = BL.probe.get((cell, pid), [])
                P("      %-6s %10s %10s %10s %9s  %-24s %s"
                  % ("gauge", "online", "pop func", "pop val", "beta",
                     "verdict", "SUPERSEDED 20Hz beta"))
                cur = {}
                for g in GAUGES:
                    fn = POP_FUNC[g]
                    rows = [(sd, rp, v) for sd, rp, v in BL.reads[key][g]
                            if sd == seed and rp == rep]
                    if not rows:
                        rows = BL.reads[key][g]
                    cand_rep = per_rep_median(rows)
                    cand = median([v for v in cand_rep.values()
                                   if v is not None])
                    pv = pop.get(fn)
                    beta = (round(cand / pv, 4) if (cand and pv) else None)
                    v = b_verdict(beta)
                    if g == "tlag" and tau_us is None:
                        v = "UNSCOREABLE(no tau for this leg)"
                    elif g == "tlag" and not pop.get("tlag_pairs"):
                        v = "UNSCOREABLE(no tau-band pair in the dumped stream)"
                    # THE SUPERSEDED COLUMN. Read by nothing.
                    ofn = PROBE_FUNC[g]
                    opv = (median([d[ofn] for d in plegs if d.get(ofn) is not None])
                           if ofn else None)
                    obeta = (round(cand / opv, 4) if (cand and opv) else None)
                    B[(key, g)].append((beta, v))
                    cur[g] = beta
                    P("      %-6s %10s %10s %10s %9s  %-24s %s"
                      % (g, cand if cand is not None else "-", fn,
                         ("%.3f" % pv) if pv is not None else "-",
                         beta if beta is not None else "-", v,
                         ("%s [%s, NOT READ]"
                          % (obeta, b_verdict_probe(obeta)))
                         if obeta is not None
                         else ("- (the 20 Hz probe computes no such "
                               "functional)" if ofn is None
                               else "- (no probe row for this leg)")))
                # §4's LAST SENTENCE, mechanised.
                inside = [(g, cur[g]) for g in GAUGES
                          if cur.get(g) is not None
                          and B_ACCEPT_LO <= cur[g] <= B_ACCEPT_HI]
                for i in range(len(inside)):
                    for j in range(i + 1, len(inside)):
                        a, b = inside[i], inside[j]
                        r = max(a[1], b[1]) / min(a[1], b[1])
                        if r > B_BAND_WIDTH:
                            P("      B-UNRESOLVED: %s and %s are both inside "
                              "the band but disagree with each other by %.2fx "
                              "> %.2fx. Per §4 that is a FINDING ABOUT THE "
                              "REFERENCE, and B is UNRESOLVED at this leg "
                              "rather than read as a pass."
                              % (a[0], b[0], r, B_BAND_WIDTH))

        # ── 5b. THE CROSS-FUNCTIONAL LEVEL TABLE ─────────────────────────
        P("\n## 5b — THE CROSS-FUNCTIONAL LEVEL TABLE. ALL FIVE POPULATION")
        P("##      FUNCTIONALS ON THE SAME DUMPED STREAM, PER LEG. REPORTED,")
        P("##      AND SCORED NOWHERE.\n")
        P("  The previous battery closed with `msd`'s 90-100x level gap")
        P("  against `sig_us` UNEXPLAINED, and 'unexplained' is not 'fine'.")
        P("  These columns come from ONE stream, so the gap is readable here")
        P("  as the functionals genuinely differing — or not.\n")
        P("  %-6s %-5s %-4s %8s %9s %10s %10s %10s %10s %10s %8s %8s"
          % ("cell", "site", "path", "n", "rate Hz", "sd", "mad", "qsp",
             "msd", "tlag", "msd/sd", "tlag/sd"))
        for (key, seed, rep) in sorted(POPVALS):
            pop, s, tau_us, _src = POPVALS[(key, seed, rep)]
            f = lambda k: ("%.1f" % pop[k]) if pop.get(k) is not None else "-"
            ratio = lambda k: ("%.4f" % (pop[k] / pop["sd"])
                               if pop.get(k) is not None and pop.get("sd")
                               else "-")
            P("  %-6s %-5s p%-3d %8d %9s %10s %10s %10s %10s %10s %8s %8s"
              % (key[0], key[1], key[2], pop["n"],
                 ("%.1f" % pop["rate_hz"]) if pop.get("rate_hz") else "-",
                 f("sd"), f("mad"), f("qsp"), f("msd"), f("tlag"),
                 ratio("msd"), ratio("tlag")))
        P("\n  NO BAR IS APPLIED TO ANY COLUMN ABOVE. Five functionals of one")
        P("  distribution are five different quantities, and a ratio between")
        P("  two of them is a property of the distribution, not a defect in")
        P("  either estimator.")

        # ── 5c. DUMP COVERAGE ────────────────────────────────────────────
        P("\n## 5c — DUMP COVERAGE PER LEG. `emitted` FROM THE DUMP AGAINST THE")
        P("##      LEG'S OWN FINAL sig_us n — THE NUMBER OF SAMPLES THE")
        P("##      ESTIMATORS ACTUALLY SAW.\n")
        P("  %-6s %-5s %-4s %10s %10s %9s   %s"
          % ("cell", "site", "path", "emitted", "sig n", "coverage", "state"))
        for (key, seed, rep) in sorted(POPVALS):
            pop, s, _tau, _src = POPVALS[(key, seed, rep)]
            ns = [n for sd, rp, n in BL.raw_n[key]["sig"]
                  if sd == seed and rp == rep]
            if not ns:
                ns = [n for _, _, n in BL.raw_n[key]["sig"]]
            N = max(ns) if ns else 0
            cov = (s["emitted"] / N) if N else None
            state = "PREFIX-SCORED (cap bound)" if s["capped"] else "full run"
            if cov is not None and cov < 0.99 and not s["capped"]:
                state += " | tail partial batch (<256/path) not written"
            P("  %-6s %-5s p%-3d %10d %10d %9s   %s"
              % (key[0], key[1], key[2], s["emitted"], N,
                 ("%.3f" % cov) if cov is not None else "-", state))
        P("\n  Coverage below 1 is EXPECTED and BOUNDED: the tail partial batch")
        P("  (< 256 samples per path per run) is never written. A capped leg is")
        P("  marked PREFIX-SCORED and its B is a statement about that prefix.")

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
        worst = (max(scoreable, key=lambda t: t[1]["R_total"])
                 if scoreable else None)
        best = (min(scoreable, key=lambda t: t[1]["R_total"])
                if scoreable else None)
        cfail = c_fail.get(g, [])
        # CLAUSE B IS SCORED AT EVERY DUMPED LEG, SEAT INCLUDED, AND THAT IS A
        # CHANGE THE NEW REFERENCE EARNS. The old B was sender-only because the
        # clock is at the sender and the probe measured a second path. The
        # rebuilt B compares a leg's estimator against THAT LEG'S OWN samples,
        # so a receiver leg's beta is exactly as like-for-like as a sender's.
        bkeys = sorted({k for (k, gg) in B if gg == g})
        breject = [k for k in bkeys
                   if any(vv == "REJECT" for _, vv in B[(k, g)])]
        bunsc = [k for k in bkeys
                 if all(vv.startswith("UNSCOREABLE") for _, vv in B[(k, g)])]
        bacquit = [k for k in bkeys
                   if any(vv == "ACQUIT" for _, vv in B[(k, g)])]
        nb = [b for k in bkeys for b, _ in B[(k, g)] if b is not None]

        P("  === %s (%s class, n_warm=%d %s)%s ==="
          % (g, CLASS[g], N_WARM[g], UNIT[g],
             "  [CONTROL]" if g in CONTROLS else "  [CANDIDATE]"))
        if worst is None:
            v = "UNSCOREABLE-NO-SCOREABLE-LEG"
            P("    S: no leg reached %d pooled post-warm-up readings."
              % MIN_READS)
        else:
            P("    S: worst leg %s/%s/p%d  R_total = %.3f   "
              "(best %s/%s/p%d = %.3f)"
              % (worst[0][0], worst[0][1], worst[0][2], worst[1]["R_total"],
                 best[0][0], best[0][1], best[0][2], best[1]["R_total"]))
            P("       %d scoreable leg(s), %d unscoreable-thin"
              % (len(scoreable), len(thin)))
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
                # AN UNEVALUATED CLAUSE IS NOT A PASSED ONE, and "no dump at
                # all" is the same state as "every dumped leg unscoreable".
                v = ("ADMISSIBLE-ON-S, B-UNSCOREABLE at every leg — NOT an "
                     "ACCEPT. B could not be evaluated, and an unevaluated "
                     "clause is not a passed one.")
            else:
                v = "ACCEPT (plain-window seat)"
                if worst[1]["R_total"] <= PREFER_BAR:
                    v = ("PREFER (plain-window seat) — R_total <= %.1f at "
                         "every leg" % PREFER_BAR)
        P("    C: %s" % ("clean at every leg" if not cfail
                         else "FAIL at %d leg(s): %s"
                         % (len(cfail), "; ".join(cfail[:3]))))
        P("    B: %d beta(s), median %s, %d ACQUIT, %d REJECT, %d unscoreable"
          % (len(nb), median(nb) if nb else "-", len(bacquit), len(breject),
             len(bunsc)))
        if g == "tlag" and CONTROL_DRIFT:
            v = ("TLAG-VERDICT-WITHHELD-CONTROL-DRIFT — the controls did not "
                 "reproduce (§3c), so NO verdict is read from this column. "
                 "The statistics above stand as measurements of an unknown "
                 "machine and as nothing else.")
        P("    ==> %s\n" % v)
        order.append((g, v, worst[1]["R_total"] if worst else None))

    P("  --- THE TIE-BREAK, PRE-COMMITTED (§3 rule 2) ---")
    adm = [(g, r) for g, v, r in order if v.startswith(("ACCEPT", "PREFER"))]
    if not adm:
        P("  NO CANDIDATE ACCEPTS AT EVERY LEG. Per the battery's own")
        P("  pre-commitment, the goal closes NEEDS-MORE with the failing")
        P("  clause named above. THE BAR IS NOT SOFTENED and no candidate is")
        P("  promoted on a partial clause.")
    elif len(adm) == 1:
        P("  ONE admissible candidate: %s (R_total %.3f). No tie to break."
          % adm[0])
    else:
        adm.sort(key=lambda t: t[1])
        P("  %d admissible. Tie broken by the PREFER tier (R_total <= %.1f)"
          % (len(adm), PREFER_BAR))
        for g, r in adm:
            P("    %-6s worst R_total %.3f  %s"
              % (g, r, "PREFER tier" if r <= PREFER_BAR else "accept tier only"))
    if CONTROL_DRIFT:
        P("\n  AND THE TIE-BREAK ABOVE EXCLUDES `tlag` BY PRE-REGISTRATION:")
        P("  the controls drifted, so the candidate column carries no verdict")
        P("  to break a tie with.")
    P("\n  SCOPE, STANDING: every verdict above is at the PLAIN-WINDOW SEAT.")
    P("  §16.74.5 requirement 3 names the generation seat as a SECOND seat and")
    P("  this battery did not run it. An estimator qualified at one seat is")
    P("  NOT qualified at the other, by the requirement's own words.")
    P("  AND THE REBUILT CLAUSE B IS A NARROWING: it establishes that an")
    P("  estimator computes its own functional over its own input faithfully.")
    P("  The delivered-latency instrument at the sender's sample rate is STILL")
    P("  MISSING, and no ACCEPT above stands in for it.")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
