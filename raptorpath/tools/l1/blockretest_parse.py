#!/usr/bin/env python3
"""Parser and scorer for the BLOCK DEFAULT RE-TEST (docs/status.md §4,
"Block default re-test — pre-registration", and its amendments).

    blockretest_parse.py row   <cell> <arm> <hint> <seed> <rep> <rc> <drv_out> <cli_log> <srv_log> [cotenant]
    blockretest_parse.py check <ledger>
    blockretest_parse.py smoke <ledger>
    blockretest_parse.py score <ledger>...
    blockretest_parse.py crown <crown-s42.log> <crown-s7.log>

`row` turns ONE invocation into ONE `BRROW {json}` ledger line. Its status is
exactly one of, in priority order:

  VOID-RC       the driver (perf_rwm_c.sh) exited non-zero: ABORT-RC, the
                invocation's rows are void, the battery goes on.
  VOID-COTENANT a cargo/rustc process was on the box before or after the
                invocation (amendment 3): void, excluded from every
                denominator, not a witness failure.
  NO_DATA       no `"summary"` line on the client: a skipped datum, not a zero
                and not an abort (ABORT-BRINGUP when the retries ran out).
  CONTAMINATED  a `[PIPE]` echo (either endpoint) or the `pipeline=` header
                names a pipeline/backend other than the arm's.
  WITNESS-FAIL  any other pre-registered witness missing: `[GATES]` absent on
                an endpoint, the window/RLC auto-select line present on BLK
                or absent on WIN, a generation guard line on either arm.
  LIVE          every witness holds; the run either completed (`seconds`,
                `mbps`) or is a DNF (`dnf: true`, the RWM_PERF_TIMEOUT_S cut).

`score` applies the pre-registered outcome set LITERALLY and prints one
verdict line: `VERDICT WINDOW-NOT-WORSE`, `VERDICT BLOCK-BETTER-AT-<cell>[,..]`
or `VERDICT UNSCOREABLE (<reasons>)`. `crown` scores the same-session crown
no-regression spot: `CROWN REPAIRS-INERT-ON-CROWN`, `CROWN CROWN-MOVED(...)`
or `CROWN SPOT-UNSCOREABLE(...)`.

Reading rules come from l1common.py (one definition per rule): colour/CR are
stripped by `read`, the quantile is `q` (linear interpolation), `field` reads
`key=` tokens.

Scoring choices the pre-registration leaves implicit, fixed HERE before any
number exists (and restated in the scored section):
  * goodput and completion are read from COMPLETED reps only; a DNF has no
    goodput and enters the DNF count. Where one arm completed no rep at a
    (cell, hint[, seed]) the goodput and completion clauses read: BLK none ->
    vacuous (no bottom/top of BLK's spread exists; the DNF clause decides);
    WIN none while BLK has some -> the clause FAILS (WIN has no median).
  * "a failed witness on >= 2 reps of an arm-cell" counts CONTAMINATED and
    WITNESS-FAIL rows over both hints and both seeds of that (arm, cell).
  * "fewer than 2 live reps per seed" counts LIVE rows (completed + DNF).
"""
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import l1common as lc  # noqa: E402

CELLS = ["c1", "c2", "c3", "c7", "c8"]
HINTS = ["bulk", "auto"]
SEEDS = ["42", "7"]
ARMS = ["BLK", "WIN"]
ARM_PIPE = {"BLK": ("block", "RaptorQ"), "WIN": ("window", "Rlc")}
WIN_LINE = "auto-selecting RLC windowed backend"
GEN_GUARD = "GUARD OK: generation ACTIVE"
ABORT_FIRST_FIVE = ("ABORT-LOCK", "ABORT-CRLF", "ABORT-SHA",
                    "ABORT-SENTINEL-UNWRITABLE", "ABORT-SMOKE")
MIN_LIVE_PER_SEED = 2
WITNESS_FAIL_LIMIT = 2


# ── ROW ──────────────────────────────────────────────────────────────────
def _jsons(lines):
    """Every JSON object a log carries (a tracing prefix may precede it)."""
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
    """(pipeline, backend, hint) off the LAST `[PIPE]` line, or None."""
    ln = lc.last_with(lines, "[PIPE]")
    if ln is None:
        return None
    seg = ln[ln.index("[PIPE]"):]
    return (lc.field(seg, "pipeline"), lc.field(seg, "backend"), lc.field(seg, "hint"))


def header_pipeline(lines):
    for ln in lines:
        if "--- RWM-C perf" in ln:
            return lc.field(ln, "pipeline")
    return None


def make_row(cell, arm, hint, seed, rep, rc, drv, cli, srv, cotenant=0):
    want_pipe, want_backend = ARM_PIPE[arm]
    objs = _jsons(cli)
    summ = [o for o in objs if o.get("summary") is True]
    runs = [o for o in objs if "run" in o and not o.get("summary")]
    pc, ps = pipe_echo(cli), pipe_echo(srv)
    hdr = header_pipeline(drv)
    gates_c = any("[GATES]" in ln for ln in cli)
    gates_s = any("[GATES]" in ln for ln in srv)
    winl_c = any(WIN_LINE in ln for ln in cli)
    winl_s = any(WIN_LINE in ln for ln in srv)
    genguard = any(GEN_GUARD in ln for ln in drv)
    gen_false = any("unified global decoder" in ln and "generation=false" in ln for ln in srv)
    row = {
        "cell": cell, "arm": arm, "hint": hint, "seed": str(seed), "rep": int(rep),
        "rc": int(rc), "header": hdr,
        "pipe_cli": list(pc) if pc else None, "pipe_srv": list(ps) if ps else None,
        "gates_cli": gates_c, "gates_srv": gates_s,
        "winline_cli": winl_c, "winline_srv": winl_s,
        "gen_guard": genguard, "srv_generation_false": gen_false,
        "summary": bool(summ), "seconds": None, "mbps": None, "dnf": None,
        "problems": [],
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
        # A summary with no per-run line: read the summary's own dnf count.
        row["dnf"] = bool(summ[-1].get("dnf"))
        if not row["dnf"]:
            row["seconds"] = lc.fnum(summ[-1].get("median_s"))
            row["mbps"] = lc.fnum(summ[-1].get("mean_mbps"))
    p = row["problems"]
    want = (want_pipe, want_backend, hint)
    contaminated = False
    for side, echo in (("cli", pc), ("srv", ps)):
        if echo is None:
            p.append(f"no-pipe-echo-{side}")
            contaminated = True
        elif tuple(echo) != want:
            p.append(f"pipe-echo-{side}={'/'.join(str(x) for x in echo)}")
            contaminated = True
    if hdr != want_pipe:
        p.append(f"header={hdr}")
        contaminated = True
    other = []
    if not gates_c:
        other.append("no-gates-cli")
    if not gates_s:
        other.append("no-gates-srv")
    if arm == "BLK" and (winl_c or winl_s):
        other.append("window-line-on-BLK")
    if arm == "WIN" and not (winl_c and winl_s):
        other.append("window-line-missing-on-WIN")
    if genguard:
        other.append("generation-guard-present")
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


# ── LEDGER READING ───────────────────────────────────────────────────────
def rows_of(paths):
    rows, tokens = [], []
    for path in paths:
        for ln in lc.read(path):
            if ln.startswith("BRROW "):
                try:
                    rows.append(json.loads(ln[len("BRROW "):]))
                except ValueError:
                    tokens.append("MALFORMED-BRROW")
            for t in ABORT_FIRST_FIVE:
                if ln.startswith(t):
                    tokens.append(t)
    return rows, tokens


def fmt(x, nd=1):
    return "-" if x is None else f"{x:.{nd}f}"


def stats(rows):
    """The scored quantities over a set of LIVE rows of one arm."""
    done = [r for r in rows if r["dnf"] is False and r["mbps"] is not None]
    gp = [r["mbps"] for r in done]
    ct = [r["seconds"] for r in done]
    return {
        "n": len(rows), "done": len(done), "dnf": sum(1 for r in rows if r["dnf"]),
        "gp_med": lc.med(gp), "gp_min": min(gp) if gp else None,
        "gp_max": max(gp) if gp else None,
        "ct_p50": lc.med(ct), "ct_max": max(ct) if ct else None,
        "gp": gp, "ct": ct,
    }


def clauses(blk, win):
    """The three pre-registered clauses: WIN not worse than BLK. Returns the
    list of failed clause names (empty = not worse)."""
    failed = []
    if blk["done"] > 0:
        if win["done"] == 0 or win["gp_med"] < blk["gp_min"]:
            failed.append("goodput")
        if win["done"] == 0 or win["ct_p50"] > blk["ct_max"]:
            failed.append("completion")
    if win["dnf"] > blk["dnf"]:
        failed.append("dnf")
    return failed


def score(paths, out=print):
    rows, tokens = rows_of(paths)
    live = [r for r in rows if r["status"] == "LIVE"]
    reasons = []
    for t in ABORT_FIRST_FIVE:
        if t in tokens:
            reasons.append(f"{t} fired")
    # Void / status table.
    out("STATUS-TABLE (every invocation row)")
    for st in ("LIVE", "NO_DATA", "VOID-RC", "VOID-COTENANT", "CONTAMINATED", "WITNESS-FAIL"):
        out(f"  {st:<13} {sum(1 for r in rows if r['status'] == st)}")
    for r in rows:
        if r["status"] != "LIVE":
            out(f"  NONLIVE {r['cell']} {r['hint']} {r['arm']} s{r['seed']} rep{r['rep']} "
                f"{r['status']} rc={r['rc']} {' '.join(r['problems'])}")
    seeds = [s for s in SEEDS if any(r["seed"] == s for r in rows)]
    # Witness failures per arm-cell.
    for arm in ARMS:
        for cell in CELLS:
            nf = sum(1 for r in rows if r["arm"] == arm and r["cell"] == cell
                     and r["status"] in ("CONTAMINATED", "WITNESS-FAIL"))
            if nf >= WITNESS_FAIL_LIMIT:
                reasons.append(f"witness failed on {nf} reps of {arm}-{cell}")
    # Live reps per seed.
    for cell in CELLS:
        for hint in HINTS:
            for arm in ARMS:
                for s in SEEDS:
                    n = sum(1 for r in live if (r["cell"], r["hint"], r["arm"], r["seed"])
                            == (cell, hint, arm, s))
                    if n < MIN_LIVE_PER_SEED:
                        reasons.append(f"{cell}/{hint}/{arm}/s{s} live={n}<{MIN_LIVE_PER_SEED}")
    out("")
    out("TABLE cell hint scope | BLK n done dnf gp_med [min-max] ct_p50 ct_max "
        "| WIN n done dnf gp_med [min-max] ct_p50 ct_max | failed-clauses")
    fails = {}
    for cell in CELLS:
        for hint in HINTS:
            for scope in ["pooled"] + [f"s{s}" for s in SEEDS]:
                sel = [r for r in live if r["cell"] == cell and r["hint"] == hint
                       and (scope == "pooled" or "s" + r["seed"] == scope)]
                b = stats([r for r in sel if r["arm"] == "BLK"])
                w = stats([r for r in sel if r["arm"] == "WIN"])
                if b["n"] == 0 and w["n"] == 0:
                    out(f"ROW {cell} {hint} {scope} | no live rows")
                    continue
                f = clauses(b, w)
                if f:
                    fails.setdefault(cell, []).append(f"{hint}/{scope}:{'+'.join(f)}")

                def part(x):
                    return (f"n={x['n']} done={x['done']} dnf={x['dnf']} "
                            f"gp={fmt(x['gp_med'])} [{fmt(x['gp_min'])}-{fmt(x['gp_max'])}] "
                            f"ct_p50={fmt(x['ct_p50'], 2)} ct_max={fmt(x['ct_max'], 2)}")
                out(f"ROW {cell} {hint} {scope} | BLK {part(b)} | WIN {part(w)} | "
                    f"{'+'.join(f) if f else 'ok'}")
    out("")
    out("PER-REP (live) cell hint arm seed: mbps list / seconds list")
    for cell in CELLS:
        for hint in HINTS:
            for arm in ARMS:
                for s in SEEDS:
                    sel = sorted((r for r in live if (r["cell"], r["hint"], r["arm"], r["seed"])
                                  == (cell, hint, arm, s)), key=lambda r: r["rep"])
                    if sel:
                        out(f"  REPS {cell} {hint} {arm} s{s}: "
                            + " ".join("DNF" if r["dnf"] else fmt(r["mbps"]) for r in sel)
                            + " / " + " ".join("-" if r["dnf"] else fmt(r["seconds"], 2) for r in sel))
    out("")
    out(f"SEEDS-PRESENT {' '.join(seeds) or '-'}")
    if reasons:
        out("VERDICT UNSCOREABLE (" + "; ".join(reasons) + ")")
        if fails:
            out("UNSCORED-CLAUSE-FAILS " + " ".join(f"{c}={','.join(v)}" for c, v in fails.items()))
        return "UNSCOREABLE"
    if fails:
        cells = [c for c in CELLS if c in fails]
        out("VERDICT BLOCK-BETTER-AT-" + ",".join(cells))
        for c in cells:
            out(f"  FAILS {c}: {' '.join(fails[c])}")
        return "BLOCK-BETTER-AT-" + ",".join(cells)
    out("VERDICT WINDOW-NOT-WORSE")
    return "WINDOW-NOT-WORSE"


def check(path):
    """rc 0 iff the ledger parses, carries >= 1 BRROW, and every BRROW is
    well-formed. Used to EARN the per-seed DONE sentinel."""
    rows, tokens = rows_of([path])
    if not rows or "MALFORMED-BRROW" in tokens:
        print(f"CHECK-FAIL {path} rows={len(rows)} tokens={tokens}")
        return 1
    print(f"CHECK-OK {path} rows={len(rows)} live={sum(1 for r in rows if r['status'] == 'LIVE')}")
    return 0


def smoke(path):
    """rc 0 iff every smoke row is LIVE (every witness shown). Nothing in it
    is a result."""
    rows, _ = rows_of([path])
    bad = [r for r in rows if r["status"] != "LIVE"]
    for r in rows:
        print(f"SMOKE-ROW {r['cell']} {r['hint']} {r['arm']} rc={r['rc']} status={r['status']} "
              f"header={r['header']} pipe_cli={r['pipe_cli']} pipe_srv={r['pipe_srv']} "
              f"gates={int(r['gates_cli'])}/{int(r['gates_srv'])} "
              f"winline={int(r['winline_cli'])}/{int(r['winline_srv'])} "
              f"{' '.join(r['problems'])}")
    arms = {r["arm"] for r in rows}
    if not rows or bad or arms != set(ARMS):
        print("ABORT-SMOKE " + (f"{len(bad)} non-live rows" if rows else "no rows"))
        return 1
    print(f"SMOKE-PASS rows={len(rows)}")
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
        cell, arm, hint, seed, rep, rc, drv, cli, srv = a[:9]
        cot = a[9] if len(a) > 9 else 0
        row = make_row(cell, arm, hint, seed, rep, rc, lc.read(drv), lc.read(cli), lc.read(srv), cot)
        print("BRROW " + json.dumps(row, sort_keys=True))
        return 0
    if cmd == "check":
        return check(a[0])
    if cmd == "smoke":
        return smoke(a[0])
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
