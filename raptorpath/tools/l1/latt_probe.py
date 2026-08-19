#!/usr/bin/env python3
"""THE PER-LEG DELIVERED-LATENCY PROBE READER — goal-gate "Latency Truth".

  usage: latt_probe.py <ping-0.txt> [<ping-1.txt> ...]      (one line per leg)

WHY THIS FILE EXISTS. The era battery (goal-gate "Era Battery — THE SCORED
RESULT" §4) reported a SIGN DISAGREEMENT it could not resolve: the engine's own
`q_p50` standing-queue estimate fell by 198–342 ms at the lossy duals while the
independent ICMP probe read 13–45 ms SLOWER. Before that disagreement can be
adjudicated the PROBE ITSELF has to be trustworthy, and it was not. Three
defects, all read off `perf_rwm_c.sh` at `7f2b009` and all repaired in the same
commit as this file:

  1. ONE LEG OF TWO. The probe pinged `10.77.0.2` — path A — on EVERY topology,
     including the ASYMMETRIC duals `c8`/`c8L` (`c2` on leg A, `c3` on leg B).
     A two-leg system was scored on one leg, and the arms load the legs
     DIFFERENTLY: leg A is the fast leg at `c8`, so a scheduler that moves work
     onto leg B empties the queue the probe watches while filling the one it
     does not. The repair is one probe PER LEG, with the leg count derived from
     `CLI_LEGS` so a quad gets four.

  2. THE SUMMARY WAS NEVER WRITTEN, SO LOSS WAS NEVER COUNTED. The reaper sent
     `kill` — SIGTERM. `iputils` `ping` installs its `sigexit` handler on
     SIGINT and SIGALRM ONLY; SIGTERM takes the default action and the process
     dies WITHOUT printing `N packets transmitted, M received`. `era_parse.py`
     parses exactly that line for `ping_tx`/`ping_rx`/`ping_loss` — so those
     three columns were `None` on every one of the era battery's 204
     invocations, and indeed no era artifact reports them. The repair is
     `kill -INT` plus a bounded wait for the summary to land.

  3. AND THE CENSORING RUNS THE WRONG WAY. This is the one that can invert a
     verdict. A probe packet that is LOST NEVER PRODUCES A `time=` LINE. Loss
     on these cells is not incidental: `topo_dual.sh` shapes the DATA direction
     with `netem loss gemodel`, so on `c8` leg A drops p/(p+q) = 1.3/51.3 =
     **2.53 %** of probes and leg B drops 2/42 = **4.76 %**, in BURSTS, plus
     whatever the loaded qdisc tail-drops when the bulk transfer fills it. Every
     one of those censored samples is drawn from EXACTLY the worst states — the
     bad GE state, the full queue. **The delivered-latency tail was measured on
     the subset of probes that survived the conditions the tail is about.** A
     percentile computed over the survivors is a percentile of a truncated
     distribution and it is BIASED LOW, in the direction that makes a latency
     claim look better than it is.

WHAT THIS MODULE THEREFORE REPORTS, and the rule it pre-commits. For each leg:
`sent`, `recv`, `censor_frac = (sent - recv) / sent`, and BESIDE EVERY
PERCENTILE a censoring verdict, because a percentile without one is a number
whose error bar points in a known direction and is not written down.

  STRUCTURAL RULE (the honest one). If a fraction `c` of probes is missing, and
  the worst case is that ALL of them would have landed in the tail, then the top
  `c` of the true distribution is UNOBSERVABLE. A percentile `qq` is therefore
  structurally unscoreable when `qq > 1 - c`: no amount of arithmetic on the
  survivors can place it. At c8 leg B's 4.76 % floor that already kills `p99`
  (0.99 > 0.952) while leaving `p95` (0.95 < 0.952) alive by a hair — before the
  loaded qdisc adds anything.

  CONTRACT BAR (the coarse one, pre-registered for the lat-truth battery).
  `censor_frac > 0.20` on a leg makes EVERY tail percentile on that leg
  unscoreable, `p50` included, because a fifth of the sample being drawn from
  the bad states makes the MEDIAN a median of the good ones.

Both flags are emitted; neither is derived from the other; the report scores the
contract bar and DISCLOSES the structural one.

WHAT THE PROBE IS AND IS NOT, stated here because the whole adjudication turns
on it and the era battery's §4 had to name it as an open instrument question:

  `q_p50`  is `median(max(0, rtt - rtp))` computed BY THE CODE UNDER TEST, from
           the sender's OWN estimate of its OWN path. It is the engine's
           self-reported standing queue. It is not delivered latency and it was
           never a measurement of one.
  `ping_*` is DELIVERED round-trip time for an unrelated flow, measured by the
           kernel, through the WHOLE shaped path — netem's fixed delay, its
           jitter, its rate serialization, ITS queue, and our own bytes sitting
           in front of the probe. It is what a different flow experiences.

These are DIFFERENT QUANTITIES and they may legitimately move in opposite
directions (the engine drains its own queue while pushing more bytes into the
shaped one). Nothing here promotes either. The adjudication is about which
quantity a LATENCY CLAIM is entitled to be stated in, and that is a question the
lat-truth battery answers with both instruments beside each other, never
averaged.
"""
import json
import re
import sys

#: `ping -D` per-reply line. The `-D` timestamp prefix is optional in this
#: regex on purpose: a reader must not silently return zero samples because a
#: future driver dropped `-D`.
REPLY = re.compile(r"icmp_seq=(\d+).*?\btime=([0-9.]+)\s*ms")
#: `ping`'s own summary, written ONLY on SIGINT/SIGALRM (see defect 2 above).
#: `+N errors` may appear between the two counts.
SUMMARY = re.compile(r"(\d+) packets transmitted, (\d+) (?:packets )?received")

#: The percentile estimator, TRANSCRIBED from `era_parse.py:q` and NOT
#: reinvented, so a per-leg percentile pools with the era ledger's `ping_p50`
#: without a second dialect. Nearest-rank on the sorted survivors, clamped.
def q(v, p):
    if not v:
        return None
    s = sorted(v)
    return round(s[min(len(s) - 1, int(p * len(s)))], 4)


#: The percentiles every leg reports. `p50` is here so the censoring verdict is
#: printed beside the MEDIAN too — the contract bar can kill it.
PCTS = (("p50", 0.50), ("p95", 0.95), ("p99", 0.99))

#: The coarse pre-registered bar. Above this, every percentile on the leg dies.
CONTRACT_BAR = 0.20


def read(path):
    try:
        with open(path, "r", errors="replace") as f:
            return [re.sub(r"\x1b\[[0-9;]*m", "", l) for l in f]
    except OSError:
        return []


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
    # measures the samples the PERCENTILES were actually computed over. The
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
        # STRUCTURAL: the top `censor` of the true distribution never produced a
        # sample, so any percentile inside it cannot be placed at all.
        out[name + "_censored"] = (censor is not None and qq > 1.0 - censor)
        # CONTRACT: the coarse pre-registered bar kills the whole leg.
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
