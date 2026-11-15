#!/usr/bin/env python3
"""The per-leg delivered-latency probe reader.

  usage: latt_probe.py <ping-0.txt> [<ping-1.txt> ...]      (one line per leg)

(`main` prints the usage line above, line 2 of this docstring.)

The probe must be trustworthy before it can adjudicate a latency claim, and
three properties make it so:

  1. One probe per leg. The arms load the legs differently: on an asymmetric
     dual a scheduler that moves work onto leg B empties the queue a leg-A
     probe watches while filling the one it does not. `perf_rwm_c.sh` runs one
     probe per leg, with the leg count derived from `CLI_LEGS`.

  2. The summary is written, so loss is counted. `iputils` `ping` installs its
     `sigexit` handler on SIGINT and SIGALRM only; SIGTERM takes the default
     action and the process dies without printing `N packets transmitted, M
     received`. The reaper sends `kill -INT` and waits, bounded, for the
     summary to land.

  3. The censoring runs the wrong way. A lost probe never produces a `time=`
     line. `topo_dual.sh` shapes the data direction with `netem loss gemodel`,
     so on `c8` leg A drops p/(p+q) = 1.3/51.3 = 2.53 % of probes and leg B
     2/42 = 4.76 %, in bursts, plus whatever the loaded qdisc tail-drops. Every
     censored sample is drawn from exactly the worst states — the bad GE state,
     the full queue — so a percentile over the survivors is a percentile of a
     truncated distribution and is biased low, in the direction that makes a
     latency claim look better than it is.

What this module reports, for each leg: `sent`, `recv`,
`censor_frac = (sent - recv) / sent`, and beside every percentile a
censoring verdict:

  Structural rule. If a fraction `c` of probes is missing, and the worst case
  is that all of them would have landed in the tail, then the top `c` of the
  true distribution is unobservable. A percentile `qq` is therefore
  structurally unscoreable when `qq > 1 - c`. At c8 leg B's 4.76 % floor that
  already kills `p99` (0.99 > 0.952) while leaving `p95` (0.95 < 0.952) alive
  by a hair — before the loaded qdisc adds anything.

  Contract bar. `censor_frac > 0.20` on a leg makes every tail percentile on
  that leg unscoreable, `p50` included, because a fifth of the sample being
  drawn from the bad states makes the median a median of the good ones.

Both flags are emitted; neither is derived from the other; the report scores
the contract bar and discloses the structural one.

What the probe is and is not:

  `q_p50`  is `median(max(0, rtt - rtp))` computed by the code under test, from
           the sender's own estimate of its own path: the engine's
           self-reported standing queue, not delivered latency.
  `ping_*` is delivered round-trip time for an unrelated flow, measured by the
           kernel, through the whole shaped path — netem's fixed delay, its
           jitter, its rate serialization, its queue, and our own bytes sitting
           in front of the probe. It is what a different flow experiences.

These are different quantities and may legitimately move in opposite
directions (the engine can drain its own queue while pushing more bytes into
the shaped one). Both are reported beside each other, never averaged.
"""
import json
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from l1common import q as _q, read  # noqa: E402

#: `ping -D` per-reply line. The `-D` timestamp prefix is optional in this
#: regex on purpose: a reader must not silently return zero samples because a
#: future driver dropped `-D`.
REPLY = re.compile(r"icmp_seq=(\d+).*?\btime=([0-9.]+)\s*ms")
#: `ping`'s own summary, written only on SIGINT/SIGALRM (see item 2 above).
#: `+N errors` may appear between the two counts.
SUMMARY = re.compile(r"(\d+) packets transmitted, (\d+) (?:packets )?received")

#: The percentile estimator is `l1common.q` (the one quantile rule: linear
#: interpolation between closest ranks), rounded to 4 places.
def q(v, p):
    return _q(v, p, 4)


#: The percentiles every leg reports. `p50` is here so the censoring verdict is
#: printed beside the median too — the contract bar can kill it.
PCTS = (("p50", 0.50), ("p95", 0.95), ("p99", 0.99))

#: The coarse contract bar. Above this, every percentile on the leg dies.
CONTRACT_BAR = 0.20


def probe_stats(path, leg=None):
    """One leg's delivered-latency readout, with its censoring accounting.

    `sent` is taken from `ping`'s OWN summary when it is present, because that
    is the only count that includes probes lost AFTER the last reply — a tail
    of consecutive drops (precisely the bufferbloat event of interest) is
    invisible to every other estimator. When the summary is absent the maximum
    observed `icmp_seq` is used as a LOWER BOUND and `sent_source` says so, so
    a reader can never mistake a floor for a count. A censoring fraction
    computed from a lower-bound denominator UNDERSTATES the censoring, which is
    why the fallback is labelled rather than silently used.
    """
    lines = read(path)
    rtts, seqs = [], []
    for ln in lines:
        m = REPLY.search(ln)
        if m:
            seqs.append(int(m.group(1)))
            rtts.append(float(m.group(2)))
    tx = rx = None
    for ln in lines:                       # last summary wins
        m = SUMMARY.search(ln)
        if m:
            tx, rx = int(m.group(1)), int(m.group(2))

    recv = len(rtts)
    if tx is not None:
        sent, sent_source = tx, "summary"
    elif seqs:
        sent, sent_source = max(seqs), "max_icmp_seq(LOWER BOUND)"
    else:
        sent, sent_source = 0, "none"

    # `recv` is counted from the reply lines, not taken from the summary, so it
    # measures the samples the percentiles were actually computed over. The
    # summary's own `received` is carried separately as a cross-check; a
    # disagreement is an instrument fault and is surfaced, not reconciled.
    censor = ((sent - recv) / sent) if sent > 0 else None
    if censor is not None:
        censor = max(0.0, round(censor, 4))

    out = {
        "leg": leg,
        "file": path,
        "n": recv,
        "sent": sent,
        "recv": recv,
        "sent_source": sent_source,
        "summary_tx": tx,
        "summary_rx": rx,
        # The instrument's own consistency check. `ping` counts a reply the
        # regex missed, or vice versa, and the percentile denominator is wrong.
        "recv_mismatch": (rx is not None and rx != recv),
        "censor_frac": censor,
        "censor_pct": (round(100.0 * censor, 2) if censor is not None else None),
        "contract_bar": CONTRACT_BAR,
        "leg_unscoreable": (censor is not None and censor > CONTRACT_BAR),
        "min": (round(min(rtts), 3) if rtts else None),
        "max": (round(max(rtts), 3) if rtts else None),
    }
    for name, qq in PCTS:
        out[name] = q(rtts, qq)
        # Structural: the top `censor` of the true distribution never produced a
        # sample, so any percentile inside it cannot be placed at all.
        out[name + "_censored"] = (censor is not None and qq > 1.0 - censor)
        # Contract: the coarse bar kills the whole leg.
        out[name + "_scoreable"] = (
            censor is not None
            and censor <= CONTRACT_BAR
            and not (qq > 1.0 - censor)
        )
    return out


def fmt(s):
    """ONE line per leg, and EVERY percentile carries its censoring verdict.

    A percentile printed without its censoring state is the defect this whole
    file exists to close, so the formatter has no mode that omits it.
    """
    if s["sent"] == 0:
        return ("LATPROBE-LEG leg=%s file=%s NO-PROBE-DATA (no replies and no "
                "summary — the probe did not run, or produced nothing)"
                % (s["leg"], s["file"]))
    parts = []
    for name, _ in PCTS:
        v = s[name]
        tag = "ok"
        if s["leg_unscoreable"]:
            tag = "UNSCOREABLE(contract>%.0f%%)" % (100 * CONTRACT_BAR)
        elif s[name + "_censored"]:
            tag = "UNSCOREABLE(inside censored tail)"
        parts.append("%s=%s[censor=%.2f%% %s]"
                     % (name, ("-" if v is None else v),
                        (s["censor_pct"] or 0.0), tag))
    extra = ""
    if s["sent_source"] != "summary":
        extra += " sent_source=%s" % s["sent_source"]
    if s["recv_mismatch"]:
        extra += (" INSTRUMENT-FAIL-PROBE-COUNT(summary_rx=%s vs parsed=%s)"
                  % (s["summary_rx"], s["recv"]))
    return ("LATPROBE-LEG leg=%s file=%s sent=%d recv=%d censor=%.2f%% %s%s"
            % (s["leg"], s["file"], s["sent"], s["recv"],
               (s["censor_pct"] or 0.0), " ".join(parts), extra))


def main(argv):
    if not argv:
        print(__doc__.splitlines()[2].strip(), file=sys.stderr)
        return 2
    for s in (probe_stats(p, leg=i) for i, p in enumerate(argv)):
        print(fmt(s))
    return 0


if __name__ == "__main__":
    args = [a for a in sys.argv[1:] if a != "--json"]
    if "--json" in sys.argv[1:]:
        print(json.dumps([probe_stats(p, leg=i) for i, p in enumerate(args)]))
        sys.exit(0)
    sys.exit(main(args))
