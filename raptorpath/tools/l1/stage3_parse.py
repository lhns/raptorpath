#!/usr/bin/env python3
"""Parser and scorer for THE STAGE-3 BASELINE (docs/status.md §5,
"Stage-3 baseline — pre-registration", and its amendments).

    stage3_parse.py row   <cell> <arm> <seed> <rep> <rc> <wall_s> <drv_out> <cli_log> <srv_log> [cotenant]
    stage3_parse.py check <ledger>
    stage3_parse.py smoke <ledger> [<tail_matrix smoke log>]
    stage3_parse.py cost  <ledger>            # smoke cost: sum of RUNTIME walls
    stage3_parse.py score <ledger>...
    stage3_parse.py crown <crown-s42.log> <crown-s7.log>

`row` turns ONE invocation into ONE `S3ROW {json}` ledger line. Its status is
exactly one of, in priority order:

  VOID-RC       the driver (perf_rwm_c.sh) exited non-zero (ABORT-RC): the
                row is void, the battery goes on.
  VOID-COTENANT a cargo/rustc process was on the box before or after it.
  NO_DATA       no `"summary"` line on the client (ABORT-BRINGUP after the
                retries): a skipped datum, not a zero.
  CONTAMINATED  a `[PIPE]` echo (either endpoint) or the `pipeline=` header
                names another pipeline/backend/hint than the arm's.
  WITNESS-FAIL  any other routing witness missing: `[GATES]` absent on an
                endpoint; the RLC auto-select line absent on an endpoint
                (every arm is a window arm); a generation guard line; the
                estimator-cadence echo not two-sided (present on BOTH
                endpoints of CAD, absent on BOTH endpoints of every other
                arm); `[GATES] RWM_POOL_ANCHOR` not 0 on both endpoints.
  LIVE          every routing witness holds; the run completed or DNF'd.

Gauges (never change a row's status; an absent gauge is None):
  mbps, seconds, dnf       the client's per-run JSON (perf.rs)
  cpu_cli, cpu_srv         the driver's `CPU:` line (whole invocation)
  util                     cpu_cli / seconds (completed rows only)
  wall_s                   the battery's RUNTIME for the invocation
  busy_med, busy_last      client `[DIAG]` `wait[... busy=P%]`, median over
                           the run's lines / the last line
  plc_p<i>                 client LAST `[DIAG]` per-path `plc=` (cumulative)
  plu_med_p<i>, plu_last_p<i>  per-path `plu=`, median over the run's lines /
                           last line
  src, cod                 client LAST `[DIAG]` `cum=src/cod/ack`
  coded_share              cod / (src + cod)
  truth_*                  l1common.truth_columns (the `[TRUTH]` lines)
  feed_ratio_p<i>          plc_p<i> / truth_loss_p<i> (truth > 0)

`score` applies the pre-registered rules LITERALLY (constants below, one
definition each, restated in docs/status.md §5). `crown` scores the
same-session crown no-regression spot: `CROWN REPAIRS-INERT-ON-CROWN`,
`CROWN CROWN-MOVED(...)` or `CROWN SPOT-UNSCOREABLE(...)`.

The (b) block-vs-window verdict (WINDOW-NOT-WORSE, docs/status.md §5) was
scored by this parser before ADR-0069 deleted the block pipeline; that
version (BLKb/BLKa arms, VERDICT-B, AUTO-BLOCK-C3) is in git history.
"""
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import l1common as lc  # noqa: E402

# ── THE DESIGN (docs/status.md §5) ───────────────────────────────────────
# cell name -> (scenA, scenB, mode, bytes). Every name carries geometry+size.
CELL_SPEC = {
    "c1s-400": ("c1", "c1", "single", 400000000),
    "c1d-400": ("c1", "c1", "dual", 400000000),
    "c2-100": ("c2", "c2", "single", 100000000),
    "c3-25": ("c3", "c3", "single", 25000000),
    "c7-100": ("c2", "c2", "dual", 100000000),
    "c8-100": ("c2", "c3", "dual", 100000000),
}
CELLS = list(CELL_SPEC)
N_LEGS = {c: (2 if s[2] == "dual" else 1) for c, s in CELL_SPEC.items()}
# arm -> (pipeline, backend, hint, cadence_on)
ARM_SPEC = {
    "A1": ("window", "Rlc", "bulk", False),
    "A2": ("window", "Rlc", "bulk", False),
    "CAD": ("window", "Rlc", "bulk", True),
    "WINa": ("window", "Rlc", "auto", False),
}
ARMS = list(ARM_SPEC)
# which arms run at which cell (the per-rep plan; stage3_battery.sh mirrors it)
PLAN = {
    "c1s-400": ["A1", "A2", "WINa"],
    "c1d-400": ["A1", "A2", "CAD"],
    "c2-100": ["A1", "A2", "CAD", "WINa"],
    "c3-25": ["A1", "A2", "CAD", "WINa"],
    "c7-100": ["A1", "A2", "CAD", "WINa"],
    "c8-100": ["A1", "A2", "CAD", "WINa"],
}
AA_CELLS = CELLS
D_CELLS = ["c1d-400", "c2-100", "c3-25", "c7-100", "c8-100"]
SEEDS = ["42", "7"]

# Scoring constants.
MDE_METRICS = ("gp", "ct", "cpu", "util", "busy")
MIN_AA = 3              # completed reps per A/A arm for an MDE to exist
MIN_LIVE = 3            # live rows per (cell, arm) for a comparison
WITNESS_FAIL_LIMIT = 2  # failed rows per (arm, cell) that void the cell
NOISE_BOUND_REL = 0.25  # MDE/median above this: that metric is NOISE-BOUND
DNF_FLOOR = 0.20        # the DNF-rate clause's minimum excess
FEED_BAND = 1.3         # CAD feed ratio within [1/1.3, 1.3] x CTL's
PLU_FLOOR = (0.0350, 0.0360)
LAST_MEASURED = {  # window bulk, the last same-cell readings (cross-era, not scored)
    "c1s-400": "v2 284.8-293.9", "c1d-400": "v3 B2 186.9-201.5",
    "c2-100": "v3 B2 88.9-89.3", "c3-25": "v3 B2 17.05-17.48",
    "c7-100": "v2 169.2-177.8", "c8-100": "v3 B2 99.5-103.0",
}
WIN_LINE = "auto-selecting RLC windowed backend"
GEN_GUARD = "GUARD OK: generation ACTIVE"
CAD_ECHO = "estimator heavy-math cadence ACTIVE"
ABORT_FIRST_FIVE = ("ABORT-LOCK", "ABORT-CRLF", "ABORT-SHA",
                    "ABORT-SENTINEL-UNWRITABLE", "ABORT-SMOKE")


# ── ROW ──────────────────────────────────────────────────────────────────
def _jsons(lines):
    out = []
    for ln in lines:
        i = ln.find("{")
        if i < 0:
            continue
        try:
            o = json.loads(ln[i:])
        except ValueError:
            continue
        if isinstance(o, dict):
            out.append(o)
    return out


def pipe_echo(lines):
    ln = lc.last_with(lines, "[PIPE]")
    if ln is None:
        return None
    seg = ln[ln.index("[PIPE]"):]
    return (lc.field(seg, "pipeline"), lc.field(seg, "backend"), lc.field(seg, "hint"))


def header(lines):
    for ln in lines:
        if "--- RWM-C perf" in ln:
            return lc.field(ln, "pipeline"), lc.field(ln, "hint")
    return None, None


_CPU = re.compile(r"CPUSRV=([0-9.]+)s CPUCLI=([0-9.]+)s")
_BUSY = re.compile(r"\bbusy=([0-9.]+)%")
_PATH = re.compile(r"(?:^|\s)p(\d+):infl=.*?plu=([0-9.]+) plc=([0-9.]+)")
_CUM = re.compile(r"(?:^|\s)cum=(\d+)/(\d+)/(\d+)")


def diag_gauges(cli):
    """The client `[DIAG] t=` lines -> the gauge columns (None when absent)."""
    d = [ln for ln in cli if "[DIAG] t=" in ln]
    g = {"diag_lines": len(d), "busy_med": None, "busy_last": None,
         "src": None, "cod": None, "coded_share": None}
    busy = []
    plu = {}
    for ln in d:
        m = _BUSY.search(ln)
        if m:
            busy.append(float(m.group(1)))
        for pm in _PATH.finditer(ln):
            plu.setdefault(int(pm.group(1)), []).append(float(pm.group(2)))
    if busy:
        g["busy_med"] = lc.med(busy)
        g["busy_last"] = busy[-1]
    for i, v in sorted(plu.items()):
        g["plu_med_p%d" % i] = lc.med(v)
        g["plu_last_p%d" % i] = v[-1]
    if d:
        last = d[-1]
        for pm in _PATH.finditer(last):
            g["plc_p%s" % pm.group(1)] = float(pm.group(3))
        m = _CUM.search(last)
        if m:
            src, cod = int(m.group(1)), int(m.group(2))
            g["src"], g["cod"] = src, cod
            g["coded_share"] = cod / (src + cod) if src + cod > 0 else None
    return g


def make_row(cell, arm, seed, rep, rc, wall_s, drv, cli, srv, cotenant=0):
    want_pipe, want_backend, want_hint, cad = ARM_SPEC[arm]
    objs = _jsons(cli)
    summ = [o for o in objs if o.get("summary") is True]
    runs = [o for o in objs if "run" in o and not o.get("summary")]
    pc, ps = pipe_echo(cli), pipe_echo(srv)
    hdr_pipe, hdr_hint = header(drv)
    row = {
        "cell": cell, "arm": arm, "seed": str(seed), "rep": int(rep), "rc": int(rc),
        "wall_s": lc.fnum(wall_s), "header": [hdr_pipe, hdr_hint],
        "pipe_cli": list(pc) if pc else None, "pipe_srv": list(ps) if ps else None,
        "gates_cli": any("[GATES]" in ln for ln in cli),
        "gates_srv": any("[GATES]" in ln for ln in srv),
        "winline_cli": any(WIN_LINE in ln for ln in cli),
        "winline_srv": any(WIN_LINE in ln for ln in srv),
        "cad_cli": any(CAD_ECHO in ln for ln in cli),
        "cad_srv": any(CAD_ECHO in ln for ln in srv),
        "pa_cli": lc.gate(cli, "RWM_POOL_ANCHOR"),
        "pa_srv": lc.gate(srv, "RWM_POOL_ANCHOR"),
        "gen_guard": any(GEN_GUARD in ln for ln in drv),
        "summary": bool(summ), "seconds": None, "mbps": None, "dnf": None,
        "cpu_cli": None, "cpu_srv": None, "util": None, "problems": [],
    }
    if runs:
        r = runs[-1]
        if r.get("dnf") is True:
            row["dnf"] = True
        else:
            row["dnf"] = False
            row["seconds"] = lc.fnum(r.get("seconds"))
            row["mbps"] = lc.fnum(r.get("mbps"))
    elif summ:
        row["dnf"] = bool(summ[-1].get("dnf"))
        if not row["dnf"]:
            row["seconds"] = lc.fnum(summ[-1].get("median_s"))
            row["mbps"] = lc.fnum(summ[-1].get("mean_mbps"))
    for ln in drv:
        m = _CPU.search(ln)
        if m:
            row["cpu_srv"], row["cpu_cli"] = float(m.group(1)), float(m.group(2))
    if row["cpu_cli"] is not None and row["seconds"]:
        row["util"] = row["cpu_cli"] / row["seconds"]
    row.update(diag_gauges(cli))
    row.update(lc.truth_columns(drv, N_LEGS[cell]))
    for i in range(N_LEGS[cell]):
        plc, tr = row.get("plc_p%d" % i), row.get("truth_loss_p%d" % i)
        row["feed_ratio_p%d" % i] = (plc / tr) if (plc is not None and tr) else None
    p = row["problems"]
    contaminated = False
    want = (want_pipe, want_backend, want_hint)
    for side, echo in (("cli", pc), ("srv", ps)):
        if echo is None:
            p.append(f"no-pipe-echo-{side}")
            contaminated = True
        elif tuple(echo) != want:
            p.append(f"pipe-echo-{side}={'/'.join(str(x) for x in echo)}")
            contaminated = True
    if (hdr_pipe, hdr_hint) != (want_pipe, want_hint):
        p.append(f"header={hdr_pipe}/{hdr_hint}")
        contaminated = True
    other = []
    if not row["gates_cli"]:
        other.append("no-gates-cli")
    if not row["gates_srv"]:
        other.append("no-gates-srv")
    if want_pipe == "window" and not (row["winline_cli"] and row["winline_srv"]):
        other.append("window-line-missing-on-window")
    if row["gen_guard"]:
        other.append("generation-guard-present")
    if cad and not (row["cad_cli"] and row["cad_srv"]):
        other.append("cadence-echo-missing-on-CAD")
    if not cad and (row["cad_cli"] or row["cad_srv"]):
        other.append("cadence-echo-on-non-CAD")
    if row["pa_cli"] != 0 or row["pa_srv"] != 0:
        other.append(f"pool-anchor={row['pa_cli']}/{row['pa_srv']}")
    p.extend(other)
    row["cotenant"] = int(cotenant)
    if int(rc) != 0:
        row["status"] = "VOID-RC"
    elif int(cotenant):
        row["status"] = "VOID-COTENANT"
    elif not summ:
        row["status"] = "NO_DATA"
    elif contaminated:
        row["status"] = "CONTAMINATED"
    elif other:
        row["status"] = "WITNESS-FAIL"
    else:
        row["status"] = "LIVE"
    return row


# ── LEDGER ───────────────────────────────────────────────────────────────
def rows_of(paths):
    rows, tokens = [], []
    for path in paths:
        for ln in lc.read(path):
            if ln.startswith("S3ROW "):
                try:
                    rows.append(json.loads(ln[len("S3ROW "):]))
                except ValueError:
                    tokens.append("MALFORMED-S3ROW")
            for t in ABORT_FIRST_FIVE:
                if ln.startswith(t):
                    tokens.append(t)
            if ln.startswith("TRUNCATED-AT-REP-BOUNDARY"):
                tokens.append("TRUNCATED-AT-REP-BOUNDARY")
    return rows, tokens


def fmt(x, nd=1):
    return "-" if x is None else f"{x:.{nd}f}"


def pct(x):
    return "-" if x is None else f"{100 * x:.1f}%"


def values(rows, metric):
    """A metric's values over the COMPLETED rows (a DNF has no goodput,
    completion, CPU-per-transfer or busy reading of a whole transfer)."""
    key = {"gp": "mbps", "ct": "seconds", "cpu": "cpu_cli", "util": "util",
           "busy": "busy_med"}[metric]
    return [r[key] for r in rows if r["dnf"] is False and r.get(key) is not None]


def dnf_rate(rows):
    return (sum(1 for r in rows if r["dnf"]) / len(rows)) if rows else None


def sel(live, cell, arm, seed=None):
    return [r for r in live if r["cell"] == cell and r["arm"] == arm
            and (seed is None or r["seed"] == seed)]


def mde_table(live):
    """(cell, metric) -> dict(mde, rel, med, a1, a2, state). state is
    MDE-COMMITTED, NOISE-BOUND or MDE-UNDEFINED."""
    out = {}
    for cell in AA_CELLS:
        a1r, a2r = sel(live, cell, "A1"), sel(live, cell, "A2")
        for m in MDE_METRICS:
            a1, a2 = values(a1r, m), values(a2r, m)
            e = {"n1": len(a1), "n2": len(a2), "med": None, "mde": None, "rel": None,
                 "d_med": None, "half_range": None, "state": "MDE-UNDEFINED",
                 "dnf1": dnf_rate(a1r), "dnf2": dnf_rate(a2r)}
            if len(a1) >= MIN_AA and len(a2) >= MIN_AA:
                allv = a1 + a2
                e["med"] = lc.med(allv)
                e["d_med"] = abs(lc.med(a1) - lc.med(a2))
                e["half_range"] = (max(allv) - min(allv)) / 2
                e["mde"] = max(2 * e["d_med"], e["half_range"])
                e["rel"] = e["mde"] / e["med"] if e["med"] else None
                e["state"] = ("NOISE-BOUND" if (e["rel"] is None or e["rel"] > NOISE_BOUND_REL)
                              else "MDE-COMMITTED")
            out[(cell, m)] = e
    return out


def dnf_threshold(mde, cell):
    e = mde.get((cell, "gp"))
    d1, d2 = (e or {}).get("dnf1"), (e or {}).get("dnf2")
    aa = 2 * abs(d1 - d2) if (d1 is not None and d2 is not None) else 0.0
    return max(DNF_FLOOR, aa)


def rel_of(mde, cell, metric):
    e = mde.get((cell, metric))
    if not e or e["state"] != "MDE-COMMITTED":
        return None
    return e["rel"]


def compare(x_rows, ref_rows, mde, cell, metric, higher_is_better):
    """X against the reference on one metric, the relative MDE of (cell,
    metric) applied to the reference median. Returns (reading, detail):
    reading in WORSE / BETTER / WITHIN / UNSCOREABLE / VACUOUS."""
    rel = rel_of(mde, cell, metric)
    xv, rv = values(x_rows, metric), values(ref_rows, metric)
    if not rv:
        return "VACUOUS", "reference completed none"
    if not xv:
        return "WORSE", "X completed none"
    if rel is None:
        return "UNSCOREABLE", f"MDE {mde.get((cell, metric), {}).get('state', 'MDE-UNDEFINED')}"
    xm, rm = lc.med(xv), lc.med(rv)
    lo, hi = rm * (1 - rel), rm * (1 + rel)
    det = f"X={xm:.3f} ref={rm:.3f} band=[{lo:.3f},{hi:.3f}] rel={rel:.3f}"
    if higher_is_better:
        return ("WORSE" if xm < lo else "BETTER" if xm > hi else "WITHIN"), det
    return ("WORSE" if xm > hi else "BETTER" if xm < lo else "WITHIN"), det


def witness_fails(rows, arm, cell):
    return sum(1 for r in rows if r["arm"] == arm and r["cell"] == cell
               and r["status"] in ("CONTAMINATED", "WITNESS-FAIL"))


def cell_blockers(rows, live, cell, arms):
    """Reasons a comparison at `cell` among `arms` cannot be scored."""
    why = []
    for a in arms:
        nl = len(sel(live, cell, a))
        if nl < MIN_LIVE:
            why.append(f"{a} live={nl}<{MIN_LIVE}")
        wf = witness_fails(rows, a, cell)
        if wf >= WITNESS_FAIL_LIMIT:
            why.append(f"{a} witness-failed={wf}")
    return why


def score(paths, out=print):
    rows, tokens = rows_of(paths)
    live = [r for r in rows if r["status"] == "LIVE"]
    aborted = [t for t in ABORT_FIRST_FIVE if t in tokens]
    out("STATUS-TABLE (every invocation row)")
    for st in ("LIVE", "NO_DATA", "VOID-RC", "VOID-COTENANT", "CONTAMINATED", "WITNESS-FAIL"):
        out(f"  {st:<13} {sum(1 for r in rows if r['status'] == st)}")
    for r in rows:
        if r["status"] != "LIVE":
            out(f"  NONLIVE {r['cell']} {r['arm']} s{r['seed']} rep{r['rep']} {r['status']} "
                f"rc={r['rc']} {' '.join(r['problems'])}")
    out(f"  ABORT-TOKENS {' '.join(aborted) or 'none'}; truncated="
        f"{int('TRUNCATED-AT-REP-BOUNDARY' in tokens)}")
    out("")
    out("ARM-N (live rows per cell/arm, and per seed)")
    for cell in CELLS:
        out("  " + cell + " " + " ".join(
            f"{a}={len(sel(live, cell, a))}({'/'.join(str(len(sel(live, cell, a, s))) for s in SEEDS)})"
            for a in PLAN[cell]))
    out("")
    # ── (a) the A/A noise floor ──
    mde = mde_table(live)
    out("(a) MDE TABLE  MDE = max(2*|med A1 - med A2|, (max-min)/2 of A1uA2); rel = MDE/med(A1uA2)")
    out("  cell metric n1/n2 med(A1uA2) |dmed| half_range MDE rel state")
    for cell in AA_CELLS:
        for m in MDE_METRICS:
            e = mde[(cell, m)]
            out(f"  MDE {cell} {m} {e['n1']}/{e['n2']} {fmt(e['med'], 3)} {fmt(e['d_med'], 3)} "
                f"{fmt(e['half_range'], 3)} {fmt(e['mde'], 3)} {pct(e['rel'])} {e['state']}")
        e = mde[(cell, "gp")]
        out(f"  AA-DNF {cell} A1={fmt(e['dnf1'], 2)} A2={fmt(e['dnf2'], 2)} "
            f"dnf-threshold={dnf_threshold(mde, cell):.2f}; last measured gp {LAST_MEASURED[cell]} (cross-era, not scored)")
    out("")
    # ── (d) EST_CADENCE ──
    out("(d) EST_CADENCE  CAD vs CTL = A1uA2")
    d_worse, d_feed, d_unsc, d_better = {}, {}, {}, {}
    for cell in D_CELLS:
        ctl = [r for a in ("A1", "A2") for r in sel(live, cell, a)]
        cad = sel(live, cell, "CAD")
        hard = cell_blockers(rows, live, cell, ["A1", "A2", "CAD"]) + (["abort"] if aborted else [])
        why = list(hard)
        res = {}
        for m, hib in (("gp", True), ("ct", False), ("cpu", False)):
            res[m] = compare(cad, ctl, mde, cell, m, hib)
            if res[m][0] == "UNSCOREABLE":
                why.append(f"{m} MDE")
        thr = dnf_threshold(mde, cell)
        dc, dd = dnf_rate(ctl), dnf_rate(cad)
        dnf_bad = (dc is not None and dd is not None and dd - dc > thr)
        worse = [m for m in ("gp", "ct", "cpu") if res[m][0] == "WORSE"] + (["dnf"] if dnf_bad else [])
        better = [m for m in ("gp", "cpu") if res[m][0] == "BETTER"]
        feed = []
        for i in range(N_LEGS[cell]):
            k = "feed_ratio_p%d" % i
            rc_ = lc.med([r[k] for r in ctl if r["dnf"] is False and r.get(k) is not None])
            rd_ = lc.med([r[k] for r in cad if r["dnf"] is False and r.get(k) is not None])
            tc = lc.med([r["truth_loss_p%d" % i] for r in ctl if r.get("truth_loss_p%d" % i) is not None])
            td = lc.med([r["truth_loss_p%d" % i] for r in cad if r.get("truth_loss_p%d" % i) is not None])
            if rc_ is None or rd_ is None:
                why.append(f"feed p{i} unread")
                st = "UNREAD"
            elif rc_ / FEED_BAND <= rd_ <= rc_ * FEED_BAND:
                st = "UNCHANGED"
            else:
                st = "MOVED"
                feed.append(f"p{i}")
            out(f"  FEED {cell} p{i} truth CTL={fmt(tc, 5)} CAD={fmt(td, 5)} | plc/truth CTL={fmt(rc_, 3)} "
                f"CAD={fmt(rd_, 3)} band=[{fmt(rc_ / FEED_BAND if rc_ else None, 3)},"
                f"{fmt(rc_ * FEED_BAND if rc_ else None, 3)}] {st}")
            for arm, rs in (("CTL", ctl), ("CAD", cad)):
                pm = [r.get("plu_med_p%d" % i) for r in rs if r.get("plu_med_p%d" % i) is not None]
                fl = sum(1 for v in pm if PLU_FLOOR[0] <= v <= PLU_FLOOR[1])
                out(f"  PLU {cell} p{i} {arm} plu_med(run medians)={fmt(lc.med(pm), 4)} "
                    f"[{fmt(min(pm, default=None), 4)}-{fmt(max(pm, default=None), 4)}] on-floor {fl}/{len(pm)}")
        for arm, rs in (("CTL", ctl), ("CAD", cad)):
            cs = [r["coded_share"] for r in rs if r.get("coded_share") is not None]
            out(f"  CODED {cell} {arm} coded_share med={fmt(lc.med(cs), 5)} "
                f"[{fmt(min(cs, default=None), 5)}-{fmt(max(cs, default=None), 5)}] "
                f"busy_med={fmt(lc.med(values(rs, 'busy')), 1)}% util={fmt(lc.med(values(rs, 'util')), 3)}")
        out(f"  D {cell} CTL n={len(ctl)} CAD n={len(cad)} | "
            + " | ".join(f"{m}:{res[m][0]} ({res[m][1]})" for m in ("gp", "ct", "cpu"))
            + f" | dnf CTL={fmt(dc, 2)} CAD={fmt(dd, 2)} excess>{thr:.2f}:{int(dnf_bad)}"
            + f" | worse={'+'.join(worse) or '-'} better={'+'.join(better) or '-'} feed-moved={'+'.join(feed) or '-'}"
            + (f" | UNSCOREABLE({'; '.join(why)})" if why else ""))
        if worse and not hard:
            d_worse[cell] = worse
        if feed and not hard:
            d_feed[cell] = feed
        if why:
            d_unsc[cell] = why
        if better and not worse and not hard:
            d_better[cell] = better
    if aborted:
        dv = "UNSCOREABLE (" + ", ".join(aborted) + ")"
    elif d_worse:
        dv = "WORSE-AT-" + ",".join(d_worse)
    elif d_feed:
        dv = "FEED-MOVED-AT-" + ",".join(f"{c}:{'+'.join(v)}" for c, v in d_feed.items())
    elif d_unsc:
        dv = "UNSCOREABLE (" + "; ".join(f"{c}: {', '.join(v)}" for c, v in d_unsc.items()) + ")"
    elif d_better:
        dv = "FLIP-RECOMMENDED (better at " + ", ".join(f"{c}:{'+'.join(v)}" for c, v in d_better.items()) + ")"
    else:
        dv = "INERT-AS-DERIVED"
    out(f"VERDICT-D {dv}")
    out("")
    out("PER-REP (live; value per rep in rep order; DNF marked) cell arm seed: mbps / cpu_cli / busy_med")
    for cell in CELLS:
        for arm in PLAN[cell]:
            for s in SEEDS:
                rs = sorted(sel(live, cell, arm, s), key=lambda r: r["rep"])
                if rs:
                    out(f"  REPS {cell} {arm} s{s}: "
                        + " ".join("DNF" if r["dnf"] else fmt(r["mbps"]) for r in rs) + " / "
                        + " ".join(fmt(r["cpu_cli"], 2) for r in rs) + " / "
                        + " ".join(fmt(r.get("busy_med"), 0) for r in rs)
                        + f" | seed-median gp={fmt(lc.med(values(rs, 'gp')))}")
    out("")
    out("TRUTH (per leg: median truth loss, gso; median plc; rcvbuf_drops max) per cell/arm")
    for cell in CELLS:
        for arm in PLAN[cell]:
            rs = sel(live, cell, arm)
            parts = []
            for i in range(N_LEGS[cell]):
                tl = [r["truth_loss_p%d" % i] for r in rs if r.get("truth_loss_p%d" % i) is not None]
                gs = [r["truth_gso_p%d" % i] for r in rs if r.get("truth_gso_p%d" % i) is not None]
                pc = [r["plc_p%d" % i] for r in rs if r.get("plc_p%d" % i) is not None]
                parts.append(f"p{i} truth={fmt(lc.med(tl), 5)} gso={fmt(lc.med(gs), 2)} plc={fmt(lc.med(pc), 4)}")
            rb = [r["truth_rcvbuf_drops"] for r in rs if r.get("truth_rcvbuf_drops") is not None]
            out(f"  TRUTH {cell} {arm} n={len(rs)} " + " ".join(parts)
                + f" rcvbuf_max={max(rb) if rb else '-'}")
    return dv


def check(path):
    rows, tokens = rows_of([path])
    if not rows or "MALFORMED-S3ROW" in tokens:
        print(f"CHECK-FAIL {path} rows={len(rows)} tokens={tokens}")
        return 1
    print(f"CHECK-OK {path} rows={len(rows)} live={sum(1 for r in rows if r['status'] == 'LIVE')}")
    return 0


def smoke(path, tm_log=None):
    """rc 0 iff every smoke row is LIVE AND carries every gauge the scorer
    reads (CPU line; [TRUTH] per data leg; on window arms a [DIAG] with
    plc/plu/busy per leg), every arm of the plan appears, and (when given)
    the tail_matrix smoke produced a `ship` rep line. Nothing is a result."""
    rows, _ = rows_of([path])
    bad = []
    for r in rows:
        miss = []
        if r["status"] != "LIVE":
            miss.append(r["status"])
        if r["cpu_cli"] is None:
            miss.append("no-CPU-line")
        for i in range(N_LEGS[r["cell"]]):
            if r.get("truth_loss_p%d" % i) is None:
                miss.append(f"no-TRUTH-p{i}")
            if ARM_SPEC[r["arm"]][0] == "window":
                for k in ("plc_p%d" % i, "plu_med_p%d" % i):
                    if r.get(k) is None:
                        miss.append(f"no-{k}")
        if ARM_SPEC[r["arm"]][0] == "window" and r.get("busy_med") is None:
            miss.append("no-busy")
        if ARM_SPEC[r["arm"]][0] == "window" and r.get("coded_share") is None:
            miss.append("no-cum")
        print(f"SMOKE-ROW {r['cell']} {r['arm']} rc={r['rc']} status={r['status']} wall={r['wall_s']} "
              f"header={r['header']} pipe={r['pipe_cli']}/{r['pipe_srv']} "
              f"gates={int(r['gates_cli'])}/{int(r['gates_srv'])} winline={int(r['winline_cli'])}/{int(r['winline_srv'])} "
              f"cad={int(r['cad_cli'])}/{int(r['cad_srv'])} pa={r['pa_cli']}/{r['pa_srv']} "
              f"mbps={r['mbps']} dnf={r['dnf']} cpu={r['cpu_cli']} busy={r.get('busy_med')} "
              f"diag_lines={r.get('diag_lines')} {' '.join(r['problems'] + miss)}")
        if miss:
            bad.append(r)
    arms = {r["arm"] for r in rows}
    ok = bool(rows) and not bad and arms == set(ARMS)
    if tm_log is not None:
        reps = [ln for ln in lc.read(tm_log) if re.match(r"^\s*ship \d+B rep\d+: p50=", ln)]
        print(f"SMOKE-CROWN tail_matrix ship rep lines={len(reps)}")
        ok = ok and len(reps) >= 1
    if not ok:
        print("ABORT-SMOKE " + (f"{len(bad)} rows missing witnesses/gauges; arms={sorted(arms)}"
                                if rows else "no rows"))
        return 1
    print(f"SMOKE-PASS rows={len(rows)}")
    return 0


def cost(path):
    tot, n = 0, 0
    for ln in lc.read(path):
        m = re.match(r"RUNTIME \S+ .*? (\d+)s rc=", ln)
        if m:
            tot += int(m.group(1))
            n += 1
    print(f"COST total={tot} n={n}")
    return 0


# ── CROWN SPOT ───────────────────────────────────────────────────────────
CROWN_P99 = {  # (cell, size, seed) -> union of the committed spreads (ms)
    ("c2", 400, "42"): (34, 199), ("c2", 400, "7"): (34, 56),
    ("c2", 1200, "42"): (35, 57), ("c2", 1200, "7"): (35, 169),
    ("c3", 400, "42"): (87, 154), ("c3", 400, "7"): (88.5, 297),
    ("c3", 1200, "42"): (84.3, 175), ("c3", 1200, "7"): (90.8, 139.1),
}
CROWN_P50 = {"c2": (7.0, 9.0), "c3": (22.0, 27.0)}
_STAGE = re.compile(r"=== CROWNSPOT stage seed=(\d+) cell=(\w+) start=")
_REP = re.compile(r"^\s*ship (\d+)B rep(\d+): p50=([0-9.?]+)ms p99=([0-9.]+)ms .* n=(\S+)")


def crown_reps(paths):
    reps = {}
    for path in paths:
        seed = cell = None
        for ln in lc.read(path):
            m = _STAGE.search(ln)
            if m:
                seed, cell = m.group(1), m.group(2)
                continue
            m = _REP.match(ln)
            if m and seed:
                key = (cell, int(m.group(1)), seed)
                reps.setdefault(key, []).append({
                    "p50": lc.fnum(m.group(3)), "p99": lc.fnum(m.group(4)),
                    "count": lc.inum(m.group(5))})
    return reps


def crown(paths, out=print):
    reps = crown_reps(paths)
    moved, unscore = [], []
    counts = []
    for key in sorted(CROWN_P99):
        cell, size, seed = key
        rs = reps.get(key, [])
        counts.extend(r["count"] for r in rs)
        p99 = lc.med([r["p99"] for r in rs])
        p50 = lc.med([r["p50"] for r in rs])
        lo, hi = CROWN_P99[key]
        plo, phi = CROWN_P50[cell]
        out(f"CROWN-CELL {cell} {size}B s{seed} n={len(rs)} p99_med={fmt(p99)} band=[{lo}-{hi}] "
            f"p50_med={fmt(p50, 2)} band=[{plo}-{phi}] counts={[r['count'] for r in rs]} "
            f"p99s={[r['p99'] for r in rs]}")
        if len(rs) < 6:
            unscore.append(f"{cell}/{size}B/s{seed} n={len(rs)}<6")
            continue
        if p99 > hi:
            moved.append(f"CROWN-MOVED({cell}, {seed}, p99@{size}B, up)")
        elif p99 < lo:
            moved.append(f"CROWN-MOVED({cell}, {seed}, p99@{size}B, down)")
        if p50 is None or p50 > phi:
            moved.append(f"CROWN-MOVED({cell}, {seed}, p50@{size}B, up)")
        elif p50 < plo:
            moved.append(f"CROWN-MOVED({cell}, {seed}, p50@{size}B, down)")
    full = sum(1 for c in counts if c == 1000)
    low = [c for c in counts if c is None or c < 995]
    out(f"CROWN-COUNT count=1000 in {full}/{len(counts)} reps (need >=62 of 64); below-995={low}")
    if unscore:
        out("CROWN SPOT-UNSCOREABLE(" + "; ".join(unscore) + ")")
        return "SPOT-UNSCOREABLE"
    if full < 62 or low:
        moved.append("CROWN-MOVED(all, all, count, down)")
    if moved:
        out("CROWN " + " ".join(moved))
        return "CROWN-MOVED"
    out("CROWN REPAIRS-INERT-ON-CROWN")
    return "REPAIRS-INERT-ON-CROWN"


def main(argv):
    if not argv:
        print(__doc__)
        return 2
    cmd, a = argv[0], argv[1:]
    if cmd == "row":
        cell, arm, seed, rep, rc, wall, drv, cli, srv = a[:9]
        cot = a[9] if len(a) > 9 else 0
        row = make_row(cell, arm, seed, rep, rc, wall, lc.read(drv), lc.read(cli), lc.read(srv), cot)
        print("S3ROW " + json.dumps(row, sort_keys=True))
        return 0
    if cmd == "check":
        return check(a[0])
    if cmd == "smoke":
        return smoke(a[0], a[1] if len(a) > 1 else None)
    if cmd == "cost":
        return cost(a[0])
    if cmd == "score":
        score(a)
        return 0
    if cmd == "crown":
        crown(a)
        return 0
    print(f"unknown command {cmd}")
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
