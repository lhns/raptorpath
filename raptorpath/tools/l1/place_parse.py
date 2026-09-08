#!/usr/bin/env python3
"""Per-invocation parser for the PLACEMENT BATTERY (Track A, paper §16.81).

    place_parse.py <cell> <arm> <seed> <rep> <client.log> <server.log>

Emits one `PLACERESULT ` + JSON line. A SEPARATE parser from `lat_parse.py`
on purpose: that one is the latency-lever battery's instrument and its output
is what those verdicts were read off, so it is left byte-identical.

THE FIELDS, AND WHY EACH IS HERE — in the order the pre-registration reads
them, which is NOT the order of interest:

  1. `[LAT]` — THE DECOMPOSITION, READ FIRST. `sh_ax` / `sh_xp` / `sh_sp` /
     `sh_rep` are the shares of delivered latency by cause, and the CTL arm's
     values decide, BEFORE any challenger is looked at, whether this battery
     is measuring the right law at all:

         PLACEMENT-INDICTED   sh_xp  >= 0.5
         QUEUE-DOMINATED      sh_ax  >= 0.5  and  sh_xp <= 0.2
         REPAIR-DOMINATED     sh_rep >= 0.5
         MIXED                none of the above

     `INSTRUMENT-INDICTS-QUEUE` is a LEGAL OUTCOME of the whole track and may
     re-route it. `rw_xp == 0` at N = 1 is the control: the same field reads
     0.87–0.97 two paths over, so a zero at one path is a property of the wire
     and not an unreached emission site.

  2. `[ETA]` — THE S4 SCORE, computed OFFLINE and before any arm verdict:
     `sig_ref = σ̂/ref`, against the shipped constant's own claim of
     **0.19238** (`T = 0.15 ⇔ σ̂_e = 0.19238·ref`). Three outcomes: ≈0.192 at
     every cell (the law WAS a dispersion and 0.15 was a lucky guess), stable
     but elsewhere (the FORM is right, the value is wrong by a measurable
     amount), or varying across cells (a fixed `T` cannot be right anywhere).

  3. THE ARM BIND GAUGES — `t_eff`, `t_cold`, `hol_sh`, `hol_mv`, `hol_w`.
     `hol_mv` is the EXECUTION WITNESS: a frontier term that never changed an
     argmin was computed, not measured, and an arm scored on it would be
     scored on nothing. `hol_sh` is `κ`'s bind fraction — a declared upper
     bound that never binds cannot be wrong.

  4. `[SUCC] xp_n/det` — the cross-path resolution fraction D0 measured. It
     must FALL at c8/c9h under a working placement arm and be identically 0 at
     c1. **A MOVING c1 VOIDS THE RUN**, which is why `xp_n` is carried on every
     row and not only on the duals.

  5. GOODPUT — `mbps`, `seconds`, `dnf`. A GUARD, PRE-DECLARED UNDERPOWERED at
     n = 8: reported so a regression is visible, never scored.

Every field is `None` when its own denominator is zero — `-` iff `n = 0`, the
engine's own convention carried through the parser rather than papered over
with a default, because "the gauge read zero" and "the gauge never ran" are
different findings and this battery's whole point is telling them apart.
"""
import json
import re
import sys

SHIPPED_SIGMA_OVER_REF = 0.19238  # T = 0.15 ⇔ σ̂_e = 0.19238·ref (§16.81.1)


def read(path):
    try:
        with open(path, errors="replace") as f:
            return [re.sub(r"\x1b\[[0-9;]*m", "", ln) for ln in f]
    except OSError:
        return []


def last_with(lines, pat):
    for ln in reversed(lines):
        if pat in ln:
            return ln.strip()
    return None


def field(line, key):
    """The token following `key` on a gauge line, or None if absent."""
    if not line:
        return None
    for t in line.split():
        if t.startswith(key):
            return t[len(key):]
    return None


def num(line, key):
    """A numeric gauge field. `-` (the engine's `n = 0` rendering) is None,
    and so is an absent key -- which is what an OLD ENGINE looks like."""
    v = field(line, key)
    if v is None or v == "-":
        return None
    try:
        return float(v)
    except ValueError:
        return None


def slots(line):
    """The per-path slots of a gauge line, each rendered so `field()` reads
    it: the `p<id>:` prefix becomes its own token."""
    if not line:
        return []
    out = []
    for t in line.split():
        head = t.split(":", 1)
        is_head = (
            len(head) == 2
            and head[0].startswith("p")
            and len(head[0]) > 1
            and head[0][1:].isdigit()
        )
        if is_head:
            out.append(t.replace(":", " ", 1))
        elif out:
            out[-1] += " " + t
    return out


def sigma_pair(slot):
    """`sig_us=<v|->/n<pairs>` -> (value_us or None, pairs)."""
    raw = field(slot, "sig_us=")
    if raw is None or "/n" not in raw:
        return (None, 0)
    v, n = raw.split("/n", 1)
    try:
        pairs = int(n)
    except ValueError:
        pairs = 0
    val = None if v == "-" else float(v)
    return (val, pairs)


av = sys.argv[1:]
cell, arm, seed, rep, clog, slog = av[:6]
cli = read(clog)
srv = read(slog)

row = {
    "cell": cell,
    "arm": arm,
    "seed": int(seed),
    "rep": int(rep),
}

# ── 5. GOODPUT: THE GUARD (read first only because it decides ABORT) ─────
# Same abort rule every parser in this tree uses: NO summary at all means the
# invocation died before the engine started, which is an ABORT and not a DNF.
# Conflating them reports a dnf count that is really a harness failure.
runs, dnf = [], False
for ln in cli:
    i = ln.find("{")
    if i < 0:
        continue
    try:
        o = json.loads(ln[i:])
    except ValueError:
        continue
    if isinstance(o, dict) and "mbps" in o:
        runs.append(o)
        if o.get("dnf"):
            dnf = True
if not runs:
    row["abort"] = True
    print("PLACERESULT " + json.dumps(row))
    sys.exit(0)
best = max(runs, key=lambda o: o.get("mbps", 0.0))
row["mbps"] = round(float(best.get("mbps", 0.0)), 3)
row["seconds"] = round(float(best.get("seconds", 0.0)), 3)
row["dnf"] = dnf
row["runs_n"] = len(runs)

# ── 1. [LAT]: THE DECOMPOSITION, AND THE PRE-REGISTERED FIRST READING ────
lat = last_with(srv, "[LAT] site=receiver")
row["lat_present"] = lat is not None
if lat:
    row["lat_n"] = num(lat, "n=")
    row["lat_over"] = num(lat, "over=")
    # Shares are per-path; the battery reads the POOLED share, weighted by
    # each path's own total wait, because a per-path mean of shares would let
    # a path that delivered ten symbols outvote one that delivered a million.
    sums = {k: 0.0 for k in ("ax", "rwxp", "rwsp", "rwrep", "rep")}
    tot = 0.0
    per_path = []
    for sl in slots(lat):
        s = {k: num(sl, f"{k}_sum=") or 0.0 for k in sums}
        w = sum(s.values())
        tot += w
        for k in sums:
            sums[k] += s[k]
        per_path.append(
            {
                "n": num(sl, "n="),
                "nowait": num(sl, "nowait="),
                "ax_p95": num(sl, "ax_p95="),
                "rwxp_p95": num(sl, "rwxp_p95="),
                "rwxp_n": num(sl, "rwxp_n="),
                "tot_p50": num(sl, "tot_p50="),
                "tot_p95": num(sl, "tot_p95="),
                "sh_ax": num(sl, "sh_ax="),
                "sh_rwxp": num(sl, "sh_rwxp="),
            }
        )
    row["lat_paths"] = per_path
    if tot > 0:
        row["sh_ax"] = round(sums["ax"] / tot, 4)
        row["sh_xp"] = round(sums["rwxp"] / tot, 4)
        row["sh_sp"] = round(sums["rwsp"] / tot, 4)
        row["sh_rep"] = round((sums["rwrep"] + sums["rep"]) / tot, 4)
        # THE PRE-REGISTERED READING. Emitted on every row; it is only BINDING
        # on the CTL arm, and the report reads it there first.
        if row["sh_xp"] >= 0.5:
            row["lat_reading"] = "PLACEMENT-INDICTED"
        elif row["sh_ax"] >= 0.5 and row["sh_xp"] <= 0.2:
            row["lat_reading"] = "QUEUE-DOMINATED"
        elif row["sh_rep"] >= 0.5:
            row["lat_reading"] = "REPAIR-DOMINATED"
        else:
            row["lat_reading"] = "MIXED"
    # The p95 the arms are scored on, worst leg -- a multipath latency claim
    # is about the leg that hurts, not about the average of the legs.
    p95s = [p["rwxp_p95"] for p in per_path if p["rwxp_p95"] is not None]
    ax95 = [p["ax_p95"] for p in per_path if p["ax_p95"] is not None]
    row["rwxp_p95_worst"] = max(p95s) if p95s else None
    row["ax_p95_worst"] = max(ax95) if ax95 else None

# ── 2. [ETA]: THE S4 SCORE, OFFLINE AND BEFORE ANY ARM VERDICT ───────────
es = last_with(cli, "[ETA] site=sender")
er = last_with(srv, "[ETA] site=receiver")
row["eta_present"] = es is not None
if es:
    row["fhat_us"] = num(es, "fhat_us=")
    row["eta_stamped"] = num(es, "n=")
    row["eta_zero"] = num(es, "zero=")
    row["cold_r"] = num(es, "cold_r=")
    row["cold_ge"] = num(es, "cold_ge=")
    # 3. THE ARM BIND GAUGES.
    row["t_eff"] = num(es, "t_eff=")
    row["t_cold"] = num(es, "t_cold=")
    row["t_n"] = num(es, "t_n=")
    row["hol_sh"] = num(es, "hol_sh=")
    row["hol_n"] = num(es, "hol_n=")
    row["hol_mv"] = num(es, "hol_mv=")
    row["hol_calls"] = num(es, "hol_calls=")
    row["hol_w"] = num(es, "hol_w=")
    # THE EXECUTION WITNESS, promoted to its own boolean so a report can
    # refuse to score an arm that never decided anything.
    if row["hol_calls"]:
        row["hol_executed"] = bool(row["hol_mv"] and row["hol_mv"] > 0.0)
    sig_s = [v for (v, n) in (sigma_pair(sl) for sl in slots(es)) if v is not None]
    # TWO poolings, because they answer two questions. The RMS over the
    # candidate set is the one §16.81.1's variance match uses and therefore
    # the one the S4 score is computed on; the MAX is the witness's
    # left-hand side (§16.81.6), where the claim is about the worst leg.
    row["sig_sender_us"] = max(sig_s) if sig_s else None
    row["sig_sender_rms_us"] = (
        round((sum(v * v for v in sig_s) / len(sig_s)) ** 0.5, 1) if sig_s else None
    )
    # THE S4 SCORE WITHOUT A SECOND INSTRUMENT. When the Tσ arm ran, the
    # engine already divided the pooled σ̂ by its OWN `ref` -- the exact
    # quantity the placement law de-dimensionalises by -- so inverting
    # `T = (√6/π)·σ̂/ref` recovers `σ̂/ref` with no [DIAG] parse and no
    # possibility of the two `ref`s disagreeing.
    if row.get("t_eff") is not None and row.get("t_n"):
        row["s4_from_teff"] = round(row["t_eff"] / (6.0 ** 0.5 / 3.141592653589793), 5)
if er:
    sig_r = [v for (v, n) in (sigma_pair(sl) for sl in slots(er)) if v is not None]
    row["sig_recv_us"] = max(sig_r) if sig_r else None
    row["lat_p95_recv"] = None
    ls = [num(sl, "l_p95=") for sl in slots(er)]
    ls = [x for x in ls if x is not None]
    if ls:
        row["lat_p95_recv"] = max(ls)

# `ref` for the S4 score is the FASTEST path's SRTT -- the same quantity the
# placement law de-dimensionalises by, read off the sender's own [DIAG].
dg = last_with(cli, "[DIAG]")
ref_us = None
if dg:
    rtts = [float(m) for m in re.findall(r"\brtt=(\d+(?:\.\d+)?)", dg)]
    if rtts:
        ref_us = min(rtts) * 1000.0  # [DIAG] prints ms
row["ref_us"] = ref_us
for who, key in (("sender", "sig_sender_rms_us"), ("recv", "sig_recv_us")):
    s = row.get(key)
    if s is not None and ref_us:
        row[f"sig_ref_{who}"] = round(s / ref_us, 5)
# THE S4 VERDICT, per row. The shipped 0.15 ASSERTS σ̂_e/ref = 0.19238 at every
# cell; a 20 % band is the pre-registered "confirms" window. The engine's own
# `t_eff` inversion is preferred over the [DIAG]-derived one when it exists,
# because the two cannot then disagree about `ref`.
s4 = row.get("s4_from_teff", row.get("sig_ref_sender"))
if s4 is not None:
    row["s4_shipped_claim"] = SHIPPED_SIGMA_OVER_REF
    row["s4_sigma_over_ref"] = s4
    row["s4_ratio"] = round(s4 / SHIPPED_SIGMA_OVER_REF, 4)
    row["s4_confirms_0p15"] = abs(s4 - SHIPPED_SIGMA_OVER_REF) <= 0.2 * SHIPPED_SIGMA_OVER_REF
# The pre-stated witness of §16.81.6, REPORTED and not asserted: the sender
# estimates over its whole candidate set, the receiver only over symbols the
# placement itself selected, and selection on a cost containing the prediction
# can only narrow the realized spread.
if row.get("sig_sender_us") is not None and row.get("sig_recv_us") is not None:
    row["witness_sender_ge_recv"] = row["sig_sender_us"] >= row["sig_recv_us"]

# ── 4. [SUCC]: THE CROSS-PATH RESOLUTION FRACTION ────────────────────────
sc = last_with(srv, "[SUCC]")
row["succ_present"] = sc is not None
if sc:
    row["succ_det"] = num(sc, "det=")
    row["succ_xp_n"] = num(sc, "xp_n=")
    row["succ_sp_n"] = num(sc, "sp_n=")
    row["succ_xp_frac"] = num(sc, "xp_frac=")
    if row["succ_det"]:
        row["xp_over_det"] = round((row["succ_xp_n"] or 0.0) / row["succ_det"], 4)
    # THE C1 CONTROL, ASSERTED IN THE ROW. N = 1 collapses the softmax to an
    # identity, so a nonzero cross-path resolution at a single-path cell is
    # not a small effect -- it means the run is not what it says it is.
    if cell in ("c1", "sc2", "sc3") and (row["succ_xp_n"] or 0) > 0:
        row["control_violated"] = True

print("PLACERESULT " + json.dumps(row))
