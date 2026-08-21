#!/usr/bin/env python3
"""CLAUSE B'S NEW REFERENCE: the POPULATION functionals, computed offline over
the sender's own raw RTT sample stream.

WHY THIS EXISTS — the defect it repairs, in the words of the record.

The scored estimator battery (goal-gate, "THE SIGMA ESTIMATOR — THE SCORED
RESULT" section 7) rejected three of four candidates on clause B and then
convicted its own reference in the same breath:

    "A uniform 30-90x gap across ALL FOUR gauges, including the estimator this
     tree has shipped and trusted for its whole history, is not four
     independent biases; it is one property of the COMPARISON."

Clause B's reference was `latt_probe.py`, a 20 Hz ICMP probe riding the whole
shaped path -- netem's delay, its jitter, its rate serialization, ITS queue,
and our own bytes queued in front of the probe -- compared against a kHz
sender's smoothed estimate of its own ack path.  Different quantities.  B was
therefore written REJECT-only (it could convict, never acquit), and for
`msd_us` -- the one candidate that came near the bar -- it was UNSCOREABLE
outright, because comparing a lag-dependent statistic across a 500x
sampling-rate gap is not a comparison.  The battery closed with the
consequence stated plainly: "msd's 90-100x level gap against sig_us is
unexplained, and 'unexplained' is not 'fine'."

WHAT THIS MODULE CHANGES.

Clause B exists to catch ONE failure: an estimator that is stable because it
measures something SMALLER.  That is a question about an estimator against
**the sample stream it consumes**, not about the sender's RTT against some
other path's latency.  `RWM_RTT_DUMP` emits that stream.  This module computes,
over the identical samples, the exact functional each estimator itself names:

    gauge      claims to be                          population functional here
    ---------  ------------------------------------  --------------------------
    sig_us     sigma_rtt, the RTT's standard          sd   = sample stdev of rtt
               deviation (W = mean + k(alpha)*sigma)       about its own mean
    rvar_us    RFC 6298 RTTVAR, the MEAN DEVIATION    mad  = E|rtt - mean(rtt)|
    qsp_us     P90(rtt) - P50(rtt)                    qsp  = the same, no window
    msd_us     median|rtt_i - rtt_{i-1}|              msd  = the same, whole leg
    tlag_us    median|rtt(t_i) - rtt(t_j)|,           tlag = the same, whole leg
               lag in [tau, 2*tau], tau = RTprop

    beta = (the gauge's ONLINE reading) / (its own functional, offline)

So beta asks: does the windowed, smoothed, decimated ONLINE implementation read
the same magnitude as its own defining functional evaluated over the whole leg?
That is precisely "is it stable because it measures something smaller", and it
is now **exact and like-for-like BY CONSTRUCTION** -- same samples, same
functional, no instrument mismatch and no sampling-rate gap.

**THE REBUILT B CAN THEREFORE ACQUIT.**  The old B's asymmetry came entirely
from the probe's dispersion being a LOWER bound on the ack path's; there is no
such bound here, because there is no second path involved.  A candidate inside
the band is now recorded as reading its own functional faithfully, and that is
a positive finding rather than an absence of evidence.

AND THIS IS A NARROWING, RECORDED AS ONE.  The rebuilt B asks whether an
estimator faithfully computes its functional over its own input.  It does NOT
ask whether that input is the true delivered latency -- the question the probe
was reaching for and answering badly.  **The instrument the battery named as
missing, a delivered-latency probe at the sender's own sample rate, is still
missing.**  The 20 Hz probe's beta stays in the report as a DISCLOSURE column,
labelled as the superseded reference, and no verdict is taken from it.

THE SECOND THING IT SETTLES, IN ONE TABLE.  Because all five functionals are
evaluated on ONE stream, the cross-functional level table says exactly how much
of the 90-100x gap between msd_us and sig_us is the two functionals genuinely
differing.  That table is REPORTED AND SCORED NOWHERE.

THE WIRE FORMAT, and its one non-obvious property.

    [RTTDUMP] p=<path> t0=<us> n=<k> d=<dt,rtt;dt,rtt;...>
    [RTTDUMP-CAP] p=<path> emitted=<k> seen=<n> ...

Each batch is SELF-CONTAINED: `t0` is the absolute stamp of the batch's first
sample, every `dt` is a delta from the previous sample **within the batch**,
and the first `dt` of a batch is always 0.  Absolute stamps are therefore
`t_k = t0 + cumsum(dt)_k` with no cross-batch state, which is what makes a
truncated or interleaved log still parse into a correct timeline.

Batches from different paths interleave in the log and batches of one path
arrive in emission order; this module sorts each path's series by timestamp
before computing anything, so no assumption about log ordering is load-bearing.

BOUNDED LOSSES, each declared and each detectable:
  1. the tail partial batch (< 256 samples per path per run) is never written;
  2. the per-path cap `RWM_RTT_DUMP_MAX`, which announces itself once via
     `[RTTDUMP-CAP]` and makes the leg PREFIX-SCORED;
  3. both are checkable against the gauges' own denominators -- the final
     `[DIAG]` `sig_us=.../n<count>` is what the estimators saw, so
     `emitted / n` is the dump's coverage and the report prints it rather than
     assuming it is 1.

CLI
    tlagb_rttdump.py [--json] [--tau-us <us>] <log> [<log> ...]

Prints one `RTTDUMP-POP` row per path, or a JSON dict with --json.  With no
--tau-us the `tlag` functional is None (there is no fallback constant, exactly
as the engine gauge has none -- paper section 16.75.6 F2).
"""

import json
import math
import re
import sys

# The engine's own band width `c` (scheduler/mod.rs SIGMA_TLAG_BAND_C) and the
# parser-side UNSCOREABLE-THIN floor K (paper 16.75.6 F1). Restated here
# because a python tool cannot see a Rust constant; if the engine's values move
# these must move with them and the report's own consistency check will say so.
TLAG_BAND_C = 2
K_THIN = 32

# The engine's quantile convention: NEAREST-RANK on round((len-1)*q), no
# interpolation. `Path::cand_quantile`, `net::QuantileClockGauge::quantile`,
# `latt_probe.q`. Using anything else here would make beta measure the two
# tools' quantile conventions rather than the estimator.
def q(sorted_vals, p):
    if not sorted_vals:
        return None
    idx = int(round((len(sorted_vals) - 1) * p))
    return float(sorted_vals[max(0, min(idx, len(sorted_vals) - 1))])


DUMP_RE = re.compile(r"\[RTTDUMP\]\s+p=(\d+)\s+t0=(\d+)\s+n=(\d+)\s+d=(\S*)")
CAP_RE = re.compile(r"\[RTTDUMP-CAP\]\s+p=(\d+)\s+emitted=(\d+)\s+seen=(\d+)")


def parse_dump(src):
    """Parse `[RTTDUMP]` batches into per-path timelines.

    `src` may be a path (str), an open file, or an iterable of lines.  Returns
    {path_id: {"series": [(t_us, rtt_us), ...] sorted by t_us,
               "capped": bool, "emitted": int, "seen": int|None,
               "batches": int, "malformed": int}}.

    A malformed entry is COUNTED and skipped, never silently dropped: a dump
    that half-parsed would make clause B a scoring over an unknown subset,
    which is the exact class of defect this whole pass exists to remove.
    """
    if isinstance(src, str):
        # `tlagb_bpass.sh` gzips its megabyte-scale client captures after the
        # last invocation, so the on-disk artefact is normally `.log.gz`.
        # Opening one as text yields zero matches and a SILENTLY EMPTY clause
        # B -- a leg scored over nothing looks identical to a leg with no
        # dump. Sniff the magic rather than trusting the extension.
        with open(src, "rb") as probe:
            gz = probe.read(2) == b"\x1f\x8b"
        if gz:
            import gzip

            with gzip.open(src, "rt", errors="replace") as fh:
                return parse_dump(fh)
        with open(src, "r", errors="replace") as fh:
            return parse_dump(fh)

    out = {}

    def slot(pid):
        return out.setdefault(
            pid,
            {
                "series": [],
                "capped": False,
                "emitted": 0,
                "seen": None,
                "batches": 0,
                "malformed": 0,
            },
        )

    for line in src:
        if "[RTTDUMP" not in line:
            continue
        m = CAP_RE.search(line)
        if m:
            s = slot(int(m.group(1)))
            s["capped"] = True
            s["seen"] = int(m.group(3))
            continue
        m = DUMP_RE.search(line)
        if not m:
            continue
        pid, t0, _n, payload = (
            int(m.group(1)),
            int(m.group(2)),
            int(m.group(3)),
            m.group(4),
        )
        s = slot(pid)
        s["batches"] += 1
        t = t0
        first = True
        for ent in payload.split(";"):
            if not ent:
                continue
            try:
                dt_s, rtt_s = ent.split(",")
                dt, rtt = int(dt_s), int(rtt_s)
            except ValueError:
                s["malformed"] += 1
                continue
            # The first dt of a batch is 0 by construction, so t0 IS the first
            # sample's own stamp. Applying the delta unconditionally is correct
            # either way; the branch exists only to make the invariant explicit
            # and to catch a producer that ever stops honouring it.
            if first:
                if dt != 0:
                    s["malformed"] += 1
                first = False
            t += dt
            s["series"].append((t, rtt))
            s["emitted"] += 1

    for s in out.values():
        s["series"].sort(key=lambda p: p[0])
    return out


def _tlag_pairs(series, tau_us):
    """The engine's pair set P(tau), transcribed (paper 16.75.0).

    For each anchor i, its partner is the MOST RECENT earlier sample at least
    `tau` older; the pair is admitted iff that partner is no more than
    `c*tau` older.  One pair per anchor, so `len()` is a count of anchors.

    This is the same two-pointer sweep `Path::tlag_diffs` runs, at the same
    band, so any difference between the online reading and this one is the
    online gauge's WINDOW and DECIMATION -- which is exactly what beta is
    supposed to measure -- and never a difference of definition.
    """
    if not tau_us or tau_us <= 0 or len(series) < 2:
        return []
    hi = tau_us * TLAG_BAND_C
    out = []
    j = 0
    for i in range(len(series)):
        ti, vi = series[i]
        while j + 1 < i and ti - series[j + 1][0] >= tau_us:
            j += 1
        if j < i:
            tj, vj = series[j]
            lag = ti - tj
            if tau_us <= lag <= hi:
                out.append(abs(vi - vj))
    return out


def population_functionals(series, tau_us=None):
    """The five estimators' own functionals, over the WHOLE stream.

    No window, no EWMA, no decimation, no warm-up exclusion -- those are
    properties of the ONLINE implementations, and beta exists to measure them.
    Returns µs floats, or None where undefined.
    """
    n = len(series)
    res = {
        "n": n,
        "sd": None,
        "mad": None,
        "qsp": None,
        "msd": None,
        "tlag": None,
        "tlag_pairs": 0,
        "tau_us": tau_us,
        "span_s": None,
        "rate_hz": None,
    }
    if n == 0:
        return res
    vals = [float(v) for _, v in series]

    # sig_us claims to be sigma_rtt -- the RTT's standard deviation, the
    # quantity `W = mean + k(alpha)*sigma` multiplies. Sample stdev (n-1),
    # matching sigb_probe.py's `sd`.
    mean = sum(vals) / n
    if n >= 2:
        var = sum((v - mean) ** 2 for v in vals) / (n - 1)
        res["sd"] = math.sqrt(var)
    # rvar_us is RFC 6298's RTTVAR: a MEAN DEVIATION, not a stdev.
    res["mad"] = sum(abs(v - mean) for v in vals) / n

    sv = sorted(vals)
    p90, p50 = q(sv, 0.90), q(sv, 0.50)
    if p90 is not None and p50 is not None:
        res["qsp"] = p90 - p50

    if n >= 2:
        d = sorted(abs(vals[i] - vals[i - 1]) for i in range(1, n))
        res["msd"] = q(d, 0.50)

    pairs = _tlag_pairs(series, tau_us)
    res["tlag_pairs"] = len(pairs)
    if pairs:
        res["tlag"] = q(sorted(pairs), 0.50)

    span = (series[-1][0] - series[0][0]) / 1e6
    res["span_s"] = span
    if span > 0:
        res["rate_hz"] = n / span
    return res


# The mapping the report scores beta through. One entry per gauge, and the
# right-hand side is the gauge's OWN claim -- never a common reference, which
# is the mistake the 20 Hz probe embodied.
POP_FUNC = {
    "sig": "sd",
    "rvar": "mad",
    "qsp": "qsp",
    "msd": "msd",
    "tlag": "tlag",
}


def main(argv):
    as_json = "--json" in argv
    argv = [a for a in argv if a != "--json"]
    tau = None
    if "--tau-us" in argv:
        i = argv.index("--tau-us")
        tau = int(float(argv[i + 1]))
        del argv[i : i + 2]
    logs = argv[1:]
    if not logs:
        print(__doc__.strip().splitlines()[0], file=sys.stderr)
        print(
            "usage: tlagb_rttdump.py [--json] [--tau-us <us>] <log> [<log> ...]",
            file=sys.stderr,
        )
        return 2

    merged = {}
    for lg in logs:
        for pid, s in parse_dump(lg).items():
            if pid not in merged:
                merged[pid] = s
            else:
                merged[pid]["series"].extend(s["series"])
                merged[pid]["emitted"] += s["emitted"]
                merged[pid]["batches"] += s["batches"]
                merged[pid]["malformed"] += s["malformed"]
                merged[pid]["capped"] = merged[pid]["capped"] or s["capped"]
    for s in merged.values():
        s["series"].sort(key=lambda p: p[0])

    rows = {}
    for pid in sorted(merged):
        s = merged[pid]
        f = population_functionals(s["series"], tau)
        f.update(
            {
                "path": pid,
                "capped": s["capped"],
                "emitted": s["emitted"],
                "seen": s["seen"],
                "batches": s["batches"],
                "malformed": s["malformed"],
            }
        )
        rows[pid] = f

    if as_json:
        print(json.dumps(rows, indent=2, sort_keys=True))
        return 0
    for pid in sorted(rows):
        r = rows[pid]

        def fmt(k):
            v = r[k]
            return "-" if v is None else "%.1f" % v

        print(
            "RTTDUMP-POP p=%d n=%d rate=%s span=%s tau=%s "
            "sd=%s mad=%s qsp=%s msd=%s tlag=%s/np%d "
            "capped=%d batches=%d malformed=%d"
            % (
                pid,
                r["n"],
                "-" if r["rate_hz"] is None else "%.1f" % r["rate_hz"],
                "-" if r["span_s"] is None else "%.2f" % r["span_s"],
                "-" if r["tau_us"] is None else str(r["tau_us"]),
                fmt("sd"),
                fmt("mad"),
                fmt("qsp"),
                fmt("msd"),
                fmt("tlag"),
                r["tlag_pairs"],
                1 if r["capped"] else 0,
                r["batches"],
                r["malformed"],
            )
        )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
