#!/usr/bin/env python3
"""Per-invocation parser for THE r > 0 BATTERY (goal-gate "THE r > 0 BATTERY —
PRE-REGISTRATION"; paper §16.82).

    r_parse.py <cell> <size> <arm> <seed> <rep> <client.log> <server.log>

Emits ONE line of JSON on stdout. A SEPARATE parser from `lat_parse.py` and
`ccand_parse.py` on purpose: those are other batteries' instruments and their
output is what those verdicts were read off, so they are left byte-identical.

WHAT IT READS, AND WHY EACH FIELD IS HERE
-----------------------------------------
`completion_p50` — THE SCORED DIMENSION. The median of the per-run `seconds`
    within THIS invocation. §16.82.7: *"The scored dimension is completion p50
    at the two sizes, which is where the two hypotheses disagree."* The unit is
    the INVOCATION's p50 over its own objects (40 at 1.8 MB, 4 at 25 MB),
    because `H_object` is a PER-OBJECT effect at the stream tail.

`mbps` / `dnf`  — THE GUARD, not the score. §16.82.7 declares the goodput leg
    `GUARD-UNDERPOWERED` at n = 8 in advance. `dnf` is counted from the summary
    object; ABORT (no summary at all) is DISTINCT from DNF and is reported as
    such — conflating them once reported `dnf = 111`.

`cum_src/cum_cod/cod_frac` — MECHANISM LIVENESS (`W5`). The last `[DIAG] cum=`
    triple (`net/diag.rs:934,945` — "the end-of-run accounting reads the LAST
    line"). `cod = 0` on a funded arm is `R-INERT`. `cod_frac` is also
    `H_price`'s OWN predicted goodput cost, `−cod/(src+cod)`, so the hypothesis
    is scored against a number the same run produced.

`chi_*`         — `W6`. `[CHI] n / max / frac_gt_half` (`net/mod.rs:2340`),
    read on BOTH arms: the control's `n=0 max=0.0000` is the two-sided half of
    the reachability claim.

`fdiag_*`       — THE PRE-STATED FALSIFIER (§16.82.6). `DECODE avg` is
    decode-resolved wall time and `SOURCE avg` is ARQ-resolved wall time, per
    hole (`receiver.rs:1899`). `present_at_stall`, `probe_holes` and
    `probe_buffered` ride BESIDE them because the record's "19–32 ms decodes"
    were RESOLUTION WAITING and not compute (goal-gate ~6983 vs ~7078-7091):
    a `DECODE avg` quoted without `present_at_stall` is not a reading of this
    battery.

`rfa_*`         — `W9`, including `preempt_src` by name (`net/mod.rs:5789-5795`:
    `false = dup_src + preempt_src`), the reactive plane's own view of a repair
    the retransmit beat.

`eps_hat`       — the sender's own loss estimate, which the pre-registration's
    R-INERT ATTRIBUTION RULE needs: below 0.05 the glide's fully-exposed target
    `BULK_TAIL_BUDGET = 0.05` leaves `r* = 0` BY ARITHMETIC, which is a finding
    about the CONSTANT and not about `r`.

NO ENGINE IS NEEDED TO EXERCISE THIS. `test_r_parse.py` beside it runs the
whole parser over a synthetic ledger.
"""
import json
import re
import sys


def strip(line):
    return re.sub(r"\x1b\[[0-9;]*m", "", line).replace("\r", "")


def read(path):
    try:
        with open(path, errors="replace") as f:
            return [strip(ln) for ln in f]
    except OSError:
        return []


def quant(vals, p):
    if not vals:
        return None
    v = sorted(vals)
    return round(v[min(len(v) - 1, int(round(p * (len(v) - 1))))], 6)


def last(lines, needle):
    """Last line containing `needle`. Every gauge this battery reads is
    CUMULATIVE (the `[RFA]` convention, net/mod.rs:2402), so the last line is
    the run's accounting and an earlier one is a snapshot of a partial run."""
    hit = None
    for ln in lines:
        if needle in ln:
            hit = ln
    return hit


def fnum(text, key, default=None):
    """`key=<number>` out of a gauge line. Returns `default` when the key or
    the line is absent, so a MISSING gauge is a null in the row and never a
    zero that scores."""
    if not text:
        return default
    m = None
    for m2 in re.finditer(re.escape(key) + r"=(-?[0-9]+(?:\.[0-9]+)?)", text):
        m = m2
    return float(m.group(1)) if m else default


def ftok(text, key, default=None):
    if not text:
        return default
    m = None
    for m2 in re.finditer(re.escape(key) + r"=([^\s|]+)", text):
        m = m2
    return m.group(1) if m else default


def main(argv):
    if len(argv) < 7:
        sys.stderr.write(__doc__ or "")
        return 2
    cell, size, arm, seed, rep, clog, slog = argv[:7]
    cli = read(clog)
    srv = read(slog)

    row = {"cell": cell, "size": size, "arm": arm,
           "seed": int(seed), "rep": int(rep)}

    # ── GOODPUT AND COMPLETION, off the per-run JSON the engine prints ──
    # ABORT != DNF: no summary AND no runs means the invocation died before the
    # engine started and contributes NO datum and NO denominator. A summary
    # with `dnf > 0` is a DNF, which is a result.
    runs, dnf_count, saw_summary = [], None, False
    for ln in cli:
        i = ln.find("{")
        if i < 0:
            continue
        try:
            o = json.loads(ln[i:])
        except Exception:
            continue
        if o.get("summary"):
            saw_summary = True
            dnf_count = o.get("dnf", o.get("dnf_count"))
        elif "mbps" in o:
            runs.append(o)
    secs = [r.get("seconds") for r in runs if isinstance(r.get("seconds"), (int, float))]
    mbps = [r["mbps"] for r in runs if isinstance(r.get("mbps"), (int, float))]

    row["runs_n"] = len(runs)
    row["abort"] = (not runs) and (not saw_summary)
    row["dnf"] = int(dnf_count or 0)
    row["mbps"] = quant(mbps, 0.5)
    row["mbps_mean"] = round(sum(mbps) / len(mbps), 6) if mbps else None
    # THE SCORED DIMENSION.
    row["completion_p50"] = quant(secs, 0.5)
    row["completion_p05"] = quant(secs, 0.05)
    row["completion_p95"] = quant(secs, 0.95)

    # ── W5: DOES `r` REACH THE WIRE? ───────────────────────────────────
    diag = last(cli, "[DIAG] t=")
    cum = ftok(diag, "cum", "") or ""
    parts = cum.split("/")
    def _i(x):
        try:
            return int(float(x))
        except Exception:
            return None
    row["cum_src"] = _i(parts[0]) if len(parts) > 0 else None
    row["cum_cod"] = _i(parts[1]) if len(parts) > 1 else None
    row["cum_ack"] = _i(parts[2]) if len(parts) > 2 else None
    s, c = row["cum_src"], row["cum_cod"]
    row["cod_frac"] = round(c / (c + s), 6) if (s is not None and c is not None and (c + s) > 0) else None
    # `H_price`'s OWN prediction for this row, self-calibrating: the arm's
    # goodput cost IS its measured wire overhead. `cod = 0` predicts exactly 0,
    # and `H_price` is then unfalsifiable on this row — which is why `R-INERT`
    # is a legal outcome and not a refutation of the hypothesis.
    row["h_price_predicted_goodput_delta"] = (
        -row["cod_frac"] if row["cod_frac"] is not None else None)
    row["diag_rtt_ms"] = fnum(diag, "rtt")
    row["diag_retx"] = fnum(diag, "retx")

    # ── W6: DOES χ REACH THE GLIDE? ────────────────────────────────────
    chi = last(cli, "[CHI]")
    row["chi_present"] = chi is not None
    row["chi_n"] = fnum(chi, "n", 0)
    row["chi_max"] = fnum(chi, "max", 0.0)
    row["chi_frac_gt_half"] = fnum(chi, "frac_gt_half", 0.0)
    row["chi_feed_echo"] = sum(1 for l in cli if "completion-exposure feed ACTIVE" in l)

    # ── THE PRE-STATED FALSIFIER (§16.82.6) ────────────────────────────
    fd = last(srv, "[FDIAG]") or last(cli, "[FDIAG]")
    row["fdiag_present"] = fd is not None
    dec = re.search(r"DECODE n=(\d+) avg=([0-9.]+)us", fd or "")
    src = re.search(r"SOURCE n=(\d+) avg=([0-9.]+)us", fd or "")
    row["fdiag_decode_n"] = int(dec.group(1)) if dec else None
    row["fdiag_decode_avg_us"] = float(dec.group(2)) if dec else None
    row["fdiag_source_n"] = int(src.group(1)) if src else None
    row["fdiag_source_avg_us"] = float(src.group(2)) if src else None
    row["fdiag_present_at_stall"] = fnum(fd, "present_at_stall")
    row["fdiag_probe_holes"] = fnum(fd, "probe_holes")
    row["fdiag_probe_buffered"] = fnum(fd, "probe_buffered")
    # THE CRITERION, evaluated per rep and adjudicated (majority, n >= 30 on
    # BOTH classes) in r_report.py. `None` means UNREADABLE, never `False`.
    dn, sn = row["fdiag_decode_n"], row["fdiag_source_n"]
    da, sa = row["fdiag_decode_avg_us"], row["fdiag_source_avg_us"]
    if dn is not None and sn is not None and dn >= 30 and sn >= 30 and da is not None and sa is not None:
        row["entangled"] = bool(da > sa)
    else:
        row["entangled"] = None

    # ── W9: [RFA], preempt_src by name ─────────────────────────────────
    rfa = last(srv, "[RFA]") or last(cli, "[RFA]")
    row["rfa_present"] = rfa is not None
    row["rfa_gen"] = ftok(rfa, "gen")
    for k in ("fires", "false", "false_frac", "fill_coded", "fill_src",
              "dup_src", "preempt_src", "rep_redundant", "late_after_aban"):
        row["rfa_" + k] = fnum(rfa, k)

    # ── THE ATTRIBUTION RULE'S OWN INPUT ───────────────────────────────
    # The sender's per-path loss estimate. Below 0.05 the glide's fully-exposed
    # target `BULK_TAIL_BUDGET = 0.05` leaves `r* = 0` BY ARITHMETIC, so an
    # `R-INERT` reading here is `BUDGET-BOUND` (a finding about the constant)
    # or `ESTIMATOR-BOUND` (a finding about goal-gate open item 3) and never an
    # unattributed null.
    eps = None
    for ln in cli:
        for m in re.finditer(r"\bpl=([0-9.]+)", ln):
            v = float(m.group(1))
            eps = v if eps is None else max(eps, v)
    row["eps_hat"] = eps
    row["eps_hat_above_budget"] = (eps > 0.05) if eps is not None else None

    # ── GATE ECHOES, two-sided, for the record (the shell asserts them) ──
    gc = last(cli, "[GATES]")
    gs = last(srv, "[GATES]")
    row["gates_cli"] = gc is not None
    row["gates_srv"] = gs is not None
    for k in ("RWM_DELTA", "RWM_COMPLETION_EXPOSURE", "RWM_THREE_TERM",
              "RWM_TAIL_BUDGET", "RWM_MIN_R", "RWM_DELTA_CAP"):
        row["g_cli_" + k] = ftok(gc, k)
        row["g_srv_" + k] = ftok(gs, k)

    sys.stdout.write(json.dumps(row, separators=(", ", ": ")) + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
