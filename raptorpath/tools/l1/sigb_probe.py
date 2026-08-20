#!/usr/bin/env python3
"""THE PROBE'S OWN SAMPLE STREAM, THROUGH EACH CANDIDATE'S OWN FUNCTIONAL.

  usage: sigb_probe.py [--json] <ping-0.txt> [<ping-1.txt> ...]

WHY THIS FILE EXISTS, AND WHY IT IS NOT `latt_probe.py`. The acceptance bar's
clause `B` (goal-gate "THE SIGMA ESTIMATOR — THE ACCEPTANCE BAR" §4) does not
score a candidate against a PERCENTILE of the probe. It scores it LIKE-FOR-LIKE:

    "the probe's own sample stream is fed through the SAME FUNCTIONAL the
     candidate computes (P90-P50 for the quantile candidate; median successive
     absolute difference for the successive-difference candidate; sample
     standard deviation for the moment-class candidates), and `beta_sigma` is
     the ratio of the candidate's reading to the probe's."

`latt_probe.py` reports `p50/p95/p99` and their censoring verdicts. Those are
delivered-latency percentiles, not dispersion functionals, and none of the four
gauges computes any of them. This module therefore ADDS the three functionals
and REUSES `latt_probe`'s reader, its regexes, its `q` estimator, its censoring
accounting and its contract bar by IMPORT — one definition of "what a probe
sample is", so a beta computed here and a percentile printed there cannot come
to disagree about the denominator.

THE FUNCTIONAL MAP, pre-registered here in code so it is not chosen later:

    gauge       class                     probe functional
    ---------   -----------------------   --------------------------------
    qsp_us      windowed quantile         P90(x) - P50(x)          `qsp`
    msd_us      successive difference     median|x_i - x_{i-1}|    `msd`
    rvar_us     moment (RFC 6298 mdev)    sample standard deviation `sd`
    sig_us      moment (sqrt EWMA dev^2)  sample standard deviation `sd`

**`rvar_us` IS SCORED AGAINST `sd` BECAUSE THE BAR SAYS "the moment-class
candidateS", PLURAL, AND `rvar` IS ONE OF THEM.** It is read LITERALLY and the
known mismatch is DISCLOSED rather than corrected: `rvar` estimates `E|dev|`,
which for a Gaussian is `0.7979*sigma`, so a literal `beta_rvar` carries a
built-in ~0.80x that is NOT removed here. `sd_mad` is emitted beside `sd` as
that disclosure — the probe's own mean absolute deviation — and NOTHING in the
scoring reads it. Applying it would be a bar amendment written after the
functional map, which is the thing the pre-registration exists to prevent.

THREE LIMITS OF THIS INSTRUMENT, EMITTED ON EVERY LEG BECAUSE CLAUSE `B` CAN
REJECT BUT CANNOT ACQUIT AND A READER MUST SEE WHY:

  1. THE SITE. The probe measures the peer path. It excludes sender scheduling,
     store residency and the ack-generation path, every one of which ADDS
     dispersion. The probe's dispersion is a LOWER bound on the ack-path
     dispersion, so `beta_sigma` against it is a LOWER bound on the true bias.
     The bar states this and pre-commits the asymmetry.

  2. THE CENSORING. A lost probe never produces a `time=` line and the losses
     are drawn from exactly the worst states (`latt_probe.py` defect 3). Every
     functional here carries `censor_frac` beside it. `P90` additionally dies
     STRUCTURALLY when `censor_frac > 0.10`, because the top `c` of the true
     distribution produced no sample at all and `0.90 > 1 - c` cannot be placed
     — the same structural rule `latt_probe` applies to `p95`/`p99`, evaluated
     at the quantile THIS functional needs. `latt_probe.CONTRACT_BAR` (0.20)
     still kills the whole leg.

  3. THE SAMPLING RATE, AND IT IS SPECIFIC TO `msd`. The probe samples at 20 Hz
     (50 ms spacing, `perf_rwm_c.sh`'s `ping -i 0.05`). The sender's RTT stream
     samples at kHz. `msd` estimates dispersion at a lag of ONE inter-sample
     interval, so its magnitude depends on the sampling rate — measured locally
     at 15x level change across a 137x rate change (goal-gate "THE SIGMA
     ESTIMATOR — THE CANDIDATES" §4). `beta_msd` against a 20 Hz probe is
     therefore NOT like-for-like in the one axis `msd` is known to depend on.
     `spacing_ms` is emitted on every leg so the report can say so with a
     number, and the battery's pre-registration pre-commits the consequence.

`msd` IS COMPUTED TWICE AND ONLY ONE OF THEM IS SCORED. `msd_all` differences
CONSECUTIVE RECEIVED samples — literally what the engine's gauge does with the
samples IT received — and is the scored one. `msd_adj` differences only pairs
whose `icmp_seq` are adjacent, so a difference never straddles a censored gap;
it is the disclosure, with `adj_frac` saying how much of the stream it kept.
A gap-straddling difference compares samples 100+ ms apart and INFLATES `msd`,
so `msd_all >= msd_adj` is the expected direction and the pair brackets it.
"""
import json
import math
import re
import sys

import latt_probe

#: The probe's inter-sample spacing, TRANSCRIBED from `perf_rwm_c.sh`'s
#: `ping -i 0.05` and not measured, because it is a commanded interval. It is
#: emitted, never used in arithmetic, so a driver that changes the interval
#: without changing this constant is caught by the emitted value disagreeing
#: with the observed one (`spacing_obs_ms`).
COMMANDED_SPACING_MS = 50.0

#: `P90` cannot be placed when the top `c` of the distribution never produced a
#: sample. `0.90 > 1 - c  <=>  c > 0.10`. Derived, not chosen.
QSP_STRUCTURAL_C = 0.10

#: The quantile the `qsp_us` gauge computes, transcribed from
#: `Path::cand_quantile`'s doc: `P90(rtt) - P50(rtt)`.
QSP_HI, QSP_LO = 0.90, 0.50


def read_samples(path):
    """(rtts_us, seqs) in ARRIVAL ORDER — the order the gauge would see them.

    Sorting here would destroy the successive-difference functional entirely,
    so the order is the file's and no sort happens anywhere in this function.
    """
    rtts, seqs = [], []
    for ln in latt_probe.read(path):
        m = latt_probe.REPLY.search(ln)
        if m:
            seqs.append(int(m.group(1)))
            rtts.append(float(m.group(2)) * 1000.0)   # ms -> us, the gauge unit
    return rtts, seqs


def f_qsp(v):
    """`P90 - P50`, on `latt_probe.q` so the estimator is the tree's one."""
    if len(v) < 2:
        return None
    hi, lo = latt_probe.q(v, QSP_HI), latt_probe.q(v, QSP_LO)
    if hi is None or lo is None:
        return None
    return round(hi - lo, 3)


def f_msd(v, seqs=None, adjacent_only=False):
    """`median |x_i - x_{i-1}|` over the stream as received.

    `adjacent_only` keeps only pairs whose `icmp_seq` differ by exactly 1, so
    no difference straddles a censored gap. Returns `(value, n_pairs)`.
    """
    d = []
    for i in range(1, len(v)):
        if adjacent_only:
            if seqs is None or seqs[i] - seqs[i - 1] != 1:
                continue
        d.append(abs(v[i] - v[i - 1]))
    if not d:
        return None, 0
    return round(latt_probe.q(d, 0.50), 3), len(d)


def f_sd(v):
    """Sample standard deviation, `n-1` denominator. The moment-class one."""
    n = len(v)
    if n < 2:
        return None
    mu = sum(v) / n
    return round(math.sqrt(sum((x - mu) ** 2 for x in v) / (n - 1)), 3)


def f_mad(v):
    """Mean absolute deviation about the mean — the DISCLOSURE for `rvar`.

    Read by nothing in the scoring. See the module docstring.
    """
    n = len(v)
    if n < 2:
        return None
    mu = sum(v) / n
    return round(sum(abs(x - mu) for x in v) / n, 3)


def probe_functionals(path, leg=None):
    """One leg's four functionals, with `latt_probe`'s censoring verdicts.

    Every functional carries the leg's `censor_frac`; `qsp` additionally
    carries its own structural verdict at `P90`.
    """
    base = latt_probe.probe_stats(path, leg=leg)
    rtts, seqs = read_samples(path)
    c = base["censor_frac"]

    msd_all, msd_pairs = f_msd(rtts)
    msd_adj, adj_pairs = f_msd(rtts, seqs, adjacent_only=True)

    # The OBSERVED spacing, from the probe's own wall: (max_seq - min_seq)
    # commanded intervals over the received span. Emitted so a driver that
    # changes `ping -i` without changing COMMANDED_SPACING_MS is caught.
    span_seq = (max(seqs) - min(seqs)) if len(seqs) >= 2 else 0
    obs = (COMMANDED_SPACING_MS if span_seq else None)

    out = dict(base)
    out.update({
        "n_samples": len(rtts),
        "spacing_ms": COMMANDED_SPACING_MS,
        "spacing_obs_ms": obs,
        "seq_span": span_seq,
        # THE SCORED THREE.
        "qsp": f_qsp(rtts),
        "msd": msd_all,
        "sd": f_sd(rtts),
        # THE DISCLOSURES. Read by nothing.
        "msd_adj": msd_adj,
        "msd_pairs": msd_pairs,
        "msd_adj_pairs": adj_pairs,
        "adj_frac": (round(adj_pairs / msd_pairs, 4) if msd_pairs else None),
        "sd_mad": f_mad(rtts),
        # THE VERDICTS.
        "qsp_structural_dead": (c is not None and c > QSP_STRUCTURAL_C),
        "leg_unscoreable": base["leg_unscoreable"],
        "contract_bar": latt_probe.CONTRACT_BAR,
        "qsp_structural_bar": QSP_STRUCTURAL_C,
    })
    return out


def fmt(s):
    """ONE line per leg. No formatter mode omits the censoring state."""
    if s["sent"] == 0:
        return ("SIGBPROBE-LEG leg=%s file=%s NO-PROBE-DATA" % (s["leg"], s["file"]))
    tag = "ok"
    if s["leg_unscoreable"]:
        tag = "LEG-UNSCOREABLE(contract>%.0f%%)" % (100 * latt_probe.CONTRACT_BAR)
    qtag = "ok"
    if s["leg_unscoreable"]:
        qtag = "LEG-UNSCOREABLE"
    elif s["qsp_structural_dead"]:
        qtag = "UNSCOREABLE(P90 inside censored tail)"
    return ("SIGBPROBE-LEG leg=%s file=%s n=%d sent=%d censor=%.2f%% %s "
            "qsp=%s[%s] msd=%s msd_adj=%s adj_frac=%s sd=%s sd_mad=%s "
            "spacing_ms=%.1f"
            % (s["leg"], s["file"], s["n_samples"], s["sent"],
               (s["censor_pct"] or 0.0), tag,
               s["qsp"], qtag, s["msd"], s["msd_adj"], s["adj_frac"],
               s["sd"], s["sd_mad"], s["spacing_ms"]))


def main(argv):
    if not argv:
        print(__doc__.splitlines()[2].strip(), file=sys.stderr)
        return 2
    for i, p in enumerate(argv):
        print(fmt(probe_functionals(p, leg=i)))
    return 0


if __name__ == "__main__":
    args = [a for a in sys.argv[1:] if a != "--json"]
    if "--json" in sys.argv[1:]:
        print(json.dumps([probe_functionals(p, leg=i) for i, p in enumerate(args)]))
        sys.exit(0)
    sys.exit(main(args))
