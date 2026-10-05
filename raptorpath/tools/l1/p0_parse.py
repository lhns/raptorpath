#!/usr/bin/env python3
"""Parser and scorer for THE THREADING-REDESIGN P0 BATTERY (docs/status.md
§9, "Threading redesign P0 -- per-thread core budget -- pre-registration").

    p0_parse.py row   <cell> <arm> <seed> <rep> <rc> <wall_s> <drv_out> <cli_log> <srv_log> [cotenant] [bin_sha]
    p0_parse.py check <ledger>
    p0_parse.py smoke <ledger>
    p0_parse.py cost  <ledger>
    p0_parse.py score <ledger>

Arms (two binaries, interleaved in one session):
  P0    this branch: the named runtime + the [THR]/[LAG] instrument;
  MAIN  main 8d7d8c1 (the shipped stack): the control for the
        no-behaviour-change check.
Both run `RWM_RDIAG=1` (the engine-receiver saturation probe, both ends).

`row` reuses `stage3_parse.make_row` (statuses, goodput/CPU, `[PIPE]`/
`[GATES]`/RLC/cadence/pool-anchor witnesses, `[TRUTH]`) and adds:

  witnesses (two-sided; a miss makes the row WITNESS-FAIL):
    both arms  `[GATES] RWM_EMIT_BATCH=1` and `RWM_RDIAG=1` on both ends;
               >= 1 in-transfer `[RDIAG]` line on the server;
    P0         the transfer-window `[THR] rt` + `[THR] sum` + `[LAG]` lines on
               both ends (client `run=1`, server `obj=1`); on Linux `[THR] os`
               lines with a `comm=rp-w-*` thread on both ends;
    MAIN       no `[THR]` / `[LAG]` line on either end.
  gauges: `thr_columns` for the client (`cli_*`) and server (`srv_*`)
          transfer windows; the `[RDIAG]` busy fraction per side over the
          IN-TRANSFER lines (msgs/s >= RDIAG_MIN_MSGS): median, max, and the
          median q_avg; the server's `phase=run` lines present (reported).

Scoring constants (one definition each, restated in status.md §9).
"""
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import l1common as lc  # noqa: E402
import stage3_parse as sp  # noqa: E402

# ── THE DESIGN (docs/status.md §9) ───────────────────────────────────────
ARMS = ["P0", "MAIN"]
sp.ARM_SPEC.update({a: ("window", "Rlc", "bulk", "ACTIVE") for a in ARMS})
CELLS = ["c1s-400", "c1d-400"]
SEEDS = ["42", "7"]
# §5's committed relative MDE (status.md §5 (a)): goodput / CPUCLI.
S5_REL = {
    "c1s-400": {"gp": 0.049, "cpu": 0.024},
    "c1d-400": {"gp": 0.056, "cpu": 0.065},
}
# §10 NEW (feat/quic-feeder-v2, n = 16): the reference numbers the brief
# names, and the control-identity band = median +- 2 x rel_gp x median.
S10_NEW = {
    "c1s-400": {"gp": 510.4, "cpu": 5.73, "band": (467.1, 568.5)},
    "c1d-400": {"gp": 424.4, "cpu": 11.19, "band": (377.8, 473.0)},
}
D3_CELL = "c1s-400"
D3_CORE = 0.90          # one thread / task at >= 0.9 core
RDIAG_BUSY = 90.0       # [RDIAG] busy % equivalent of D3_CORE
RDIAG_MIN_MSGS = 1000   # msgs/s: an [RDIAG] line inside the transfer
MIN_LIVE = 3
ABORT_FIRST = ("ABORT-LOCK", "ABORT-CRLF", "ABORT-BUILD", "ABORT-TESTS", "ABORT-SHA",
               "ABORT-SENTINEL-UNWRITABLE", "ABORT-SMOKE")

_RDIAG = re.compile(r"\[RDIAG\] busy=(-?[0-9.]+)% msgs=(\d+)/s q_avg=([0-9.]+) q_max=(\d+)")


def rdiag(lines):
    """The in-transfer `[RDIAG]` lines -> (busy_med, busy_max, q_avg_med, n)."""
    busy, qs = [], []
    for ln in lines or []:
        m = _RDIAG.search(ln)
        if m and int(m.group(2)) >= RDIAG_MIN_MSGS:
            busy.append(float(m.group(1)))
            qs.append(float(m.group(3)))
    if not busy:
        return None, None, None, 0
    return lc.med(busy), max(busy), lc.med(qs), len(busy)


def has_thr(lines):
    return any("[THR] " in ln or "[LAG] " in ln for ln in lines or [])


# ── ROW ──────────────────────────────────────────────────────────────────
def make_row(cell, arm, seed, rep, rc, wall_s, drv, cli, srv, cotenant=0, bin_sha=None):
    row = sp.make_row(cell, arm, seed, rep, rc, wall_s, drv, cli, srv, cotenant)
    row["bin_sha"] = bin_sha
    for side, lines in (("cli", cli), ("srv", srv)):
        row[f"emb_gate_{side}"] = lc.gate_tok(lines, "RWM_EMIT_BATCH")
        row[f"rdiag_gate_{side}"] = lc.gate_tok(lines, "RWM_RDIAG")
        b, bmax, q, n = rdiag(lines)
        row[f"rdiag_{side}_busy_med"], row[f"rdiag_{side}_busy_max"] = b, bmax
        row[f"rdiag_{side}_q_med"], row[f"rdiag_{side}_n"] = q, n
        row[f"thr_any_{side}"] = has_thr(lines)
    row.update(lc.thr_columns(cli, "cli", "xfer", "run=1"))
    row.update(lc.thr_columns(srv, "srv", "xfer", "obj=1"))
    tc, ts = lc.thr(cli, "xfer", "run=1"), lc.thr(srv, "xfer", "obj=1")
    row["cli_os_rpw"] = bool(tc and tc["threads"] and any((x["comm"] or "").startswith("rp-w-") for x in tc["threads"]))
    row["srv_os_rpw"] = bool(ts and ts["threads"] and any((x["comm"] or "").startswith("rp-w-") for x in ts["threads"]))
    row["srv_run_lines"] = lc.thr(srv, "run") is not None
    # Per-thread table of this rep (for the budget table and rule 4).
    row["cli_threads"] = [(x["comm"], x["cores"]) for x in ((tc or {}).get("threads") or [])]
    row["srv_threads"] = [(x["comm"], x["cores"]) for x in ((ts or {}).get("threads") or [])]
    eg = [row.get("truth_egress_p%d" % i) for i in range(sp.N_LEGS[cell])]
    row["egress_dgrams"] = sum(eg) if eg and all(e is not None for e in eg) else None
    row["cpu_us_per_dgram"] = (1e6 * row["cpu_cli"] / row["egress_dgrams"]
                               if row["cpu_cli"] is not None and row["egress_dgrams"] else None)
    p = []
    if row["emb_gate_cli"] != "1" or row["emb_gate_srv"] != "1":
        p.append(f"emit-batch-gate={row['emb_gate_cli']}/{row['emb_gate_srv']}")
    if row["rdiag_gate_cli"] != "1" or row["rdiag_gate_srv"] != "1":
        p.append(f"rdiag-gate={row['rdiag_gate_cli']}/{row['rdiag_gate_srv']}")
    if row["rdiag_srv_n"] == 0:
        p.append("no-in-transfer-rdiag-srv")
    if arm == "P0":
        for side in ("cli", "srv"):
            if not row[f"{side}_n_workers"]:
                p.append(f"no-thr-rt-{side}")
            if row[f"{side}_cores"] is None and row[f"{side}_wall_s"] is None:
                p.append(f"no-thr-sum-{side}")
            if not row[f"{side}_lag_n"]:
                p.append(f"no-lag-{side}")
            if not row[f"{side}_os_rpw"]:
                p.append(f"no-thr-os-rpw-{side}")
    else:
        if row["thr_any_cli"] or row["thr_any_srv"]:
            p.append(f"thr-on-{arm}={int(row['thr_any_cli'])}/{int(row['thr_any_srv'])}")
    row["problems"].extend(p)
    if row["status"] == "LIVE" and p:
        row["status"] = "WITNESS-FAIL"
    return row


# ── LEDGER ───────────────────────────────────────────────────────────────
def rows_of(path):
    rows, tokens = [], []
    for ln in lc.read(path):
        if ln.startswith("P0ROW "):
            try:
                rows.append(json.loads(ln[len("P0ROW "):]))
            except ValueError:
                tokens.append("MALFORMED-P0ROW")
        for t in ABORT_FIRST:
            if ln.startswith(t):
                tokens.append(t)
        if ln.startswith("TRUNCATED-AT-REP-BOUNDARY"):
            tokens.append("TRUNCATED-AT-REP-BOUNDARY")
    return rows, tokens


def fmt(x, nd=2):
    return "-" if x is None else f"{x:.{nd}f}"


def check(path):
    rows, tokens = rows_of(path)
    if not rows or "MALFORMED-P0ROW" in tokens:
        print(f"CHECK-FAIL {path} rows={len(rows)} tokens={tokens}")
        return 1
    print(f"CHECK-OK {path} rows={len(rows)} live={sum(1 for r in rows if r['status'] == 'LIVE')}")
    return 0


def smoke(path):
    """rc 0 iff every smoke row is LIVE (the witnesses above), carries its
    CPU line and a [TRUTH] per leg, and both arms appear. Nothing is a result."""
    rows, _ = rows_of(path)
    bad = []
    for r in rows:
        miss = []
        if r["status"] != "LIVE":
            miss.append(r["status"])
        if r["cpu_cli"] is None or r["cpu_srv"] is None:
            miss.append("no-CPU-line")
        for i in range(sp.N_LEGS[r["cell"]]):
            if r.get("truth_loss_p%d" % i) is None:
                miss.append(f"no-TRUTH-p{i}")
        print(f"SMOKE-ROW {r['cell']} {r['arm']} rc={r['rc']} status={r['status']} wall={r['wall_s']} "
              f"mbps={r['mbps']} cpu={r['cpu_cli']}/{r['cpu_srv']} rdiag_srv={r['rdiag_srv_busy_med']}"
              f"(n={r['rdiag_srv_n']}) cli_thr_r1={r.get('cli_thr_r1')} srv_thr_r1={r.get('srv_thr_r1')} "
              f"cli_lag_p99={r.get('cli_lag_p99_us')} srv_lag_p99={r.get('srv_lag_p99_us')} "
              f"srv_run_lines={int(r.get('srv_run_lines', False))} {' '.join(r['problems'] + miss)}")
        if miss:
            bad.append(r)
    arms = {r["arm"] for r in rows}
    if not rows or bad or arms != set(ARMS):
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


# ── SCORE ────────────────────────────────────────────────────────────────
def live(rows, cell, arm):
    return [r for r in rows if r["status"] == "LIVE" and r["cell"] == cell and r["arm"] == arm]


def vals(rs, key):
    return [r[key] for r in rs if r.get(key) is not None]


def mmm(v, nd=2):
    """'median [min-max] (n=k)' of a sample."""
    if not v:
        return "- (n=0)"
    return f"{fmt(lc.med(v), nd)} [{fmt(min(v), nd)}-{fmt(max(v), nd)}] (n={len(v)})"


def nochange_clause(p0, mn, rel, higher_is_better=True):
    """One no-behaviour-change clause: 'within' if |med(P0)/med(MAIN) - 1| <=
    rel; else 'MOVED' when the [min, max] ranges are disjoint, else
    'UNDERPOWERED'. None when either side is empty."""
    if not p0 or not mn:
        return None, None
    d = lc.med(p0) / lc.med(mn) - 1.0
    if abs(d) <= rel:
        return "within", d
    if max(p0) < min(mn) or min(p0) > max(mn):
        return "MOVED", d
    return "UNDERPOWERED", d


def d3_verdict(rows):
    """(outcome, detail) for D3 at c1s-400 on the P0 rows."""
    rs = live(rows, D3_CELL, "P0")
    busy = vals(rs, "rdiag_srv_busy_med")
    top = vals(rs, "srv_thr_r1")
    if len(rs) < MIN_LIVE:
        return "UNSCOREABLE", f"{len(rs)} live P0 rows at {D3_CELL} < {MIN_LIVE}"
    if len(busy) < MIN_LIVE and len(top) < MIN_LIVE:
        return "NEEDS-MORE-rdiag-and-thr", f"rdiag n={len(busy)}, thr n={len(top)}"
    b = lc.med(busy) if len(busy) >= MIN_LIVE else None
    t = lc.med(top) if len(top) >= MIN_LIVE else None
    hit = []
    if b is not None and b >= RDIAG_BUSY:
        hit.append(f"receiver task busy {b:.1f} % >= {RDIAG_BUSY:.0f} %")
    if t is not None and t >= D3_CORE:
        hit.append(f"hottest server thread {t:.3f} core >= {D3_CORE}")
    det = f"server [RDIAG] busy med={fmt(b, 1)} %, hottest server thread med={fmt(t, 3)} core"
    if hit:
        return "D3-CONFIRMED", det + " -- " + "; ".join(hit)
    if b is None or t is None:
        return "NEEDS-MORE-" + ("rdiag" if b is None else "thr"), det
    return "D3-REFUTED-WITH-RECORD", det


def score(path, out=print):
    rows, tokens = rows_of(path)
    out("=== P0 SCORE (docs/status.md §9, literally)")
    first = [t for t in ABORT_FIRST if t in tokens]
    out(f"ABORT TOKENS: {first or 'none'}; truncated={'TRUNCATED-AT-REP-BOUNDARY' in tokens}")
    st = {}
    for r in rows:
        st[r["status"]] = st.get(r["status"], 0) + 1
    out(f"ROWS {len(rows)} statuses={st}")
    for r in rows:
        if r["status"] != "LIVE":
            out(f"  NON-LIVE {r['cell']} {r['arm']} s{r['seed']} r{r['rep']} {r['status']} {r['problems']}")
    out("")
    out("--- per-rep values (rule 4)")
    for r in rows:
        out(f"  {r['cell']} {r['arm']:4s} s{r['seed']:>2} r{r['rep']} {r['status']:12s} gp={fmt(r['mbps'], 1)} "
            f"cpucli={fmt(r['cpu_cli'])} cpusrv={fmt(r['cpu_srv'])} us/dgram={fmt(r['cpu_us_per_dgram'])} "
            f"rdiag_srv={fmt(r['rdiag_srv_busy_med'], 1)}%/max{fmt(r['rdiag_srv_busy_max'], 1)}"
            f"/q{fmt(r['rdiag_srv_q_med'], 0)} rdiag_cli={fmt(r['rdiag_cli_busy_med'], 1)}% "
            f"srv_top={fmt(r.get('srv_thr_r1'), 3)}({r.get('srv_top_comm')}) "
            f"cli_top={fmt(r.get('cli_thr_r1'), 3)}({r.get('cli_top_comm')})")
    out("")
    out("--- goodput / CPU per cell and arm: median [min-max] (n)")
    nc_all = []
    for cell in CELLS:
        p0, mn = live(rows, cell, "P0"), live(rows, cell, "MAIN")
        for arm, rs in (("P0", p0), ("MAIN", mn)):
            out(f"  {cell} {arm:4s} gp={mmm(vals(rs, 'mbps'), 1)} cpucli={mmm(vals(rs, 'cpu_cli'))} "
                f"cpusrv={mmm(vals(rs, 'cpu_srv'))} us/dgram={mmm(vals(rs, 'cpu_us_per_dgram'))}")
            for s in SEEDS:
                ss = [r for r in rs if r["seed"] == s]
                out(f"      seed {s}: gp={mmm(vals(ss, 'mbps'), 1)} cpucli={mmm(vals(ss, 'cpu_cli'))}")
        ref = S10_NEW[cell]
        mg = vals(mn, "mbps")
        ident = "-"
        if mg:
            lo, hi = ref["band"]
            ident = "IN-BAND" if lo <= lc.med(mg) <= hi else "CONTROL-MOVED"
        out(f"  {cell} MAIN identity vs §10 NEW {ref['gp']} band {ref['band']}: {ident}")
        if len(p0) < MIN_LIVE or len(mn) < MIN_LIVE:
            out(f"  {cell} NO-CHANGE UNSCOREABLE (live P0={len(p0)} MAIN={len(mn)} < {MIN_LIVE})")
            nc_all.append("UNSCOREABLE")
            continue
        for metric, key in (("gp", "mbps"), ("cpu", "cpu_cli")):
            c, d = nochange_clause(vals(p0, key), vals(mn, key), S5_REL[cell][metric])
            out(f"  {cell} NO-CHANGE {metric}: P0 vs MAIN {fmt(100 * d if d is not None else None, 1)} % "
                f"(MDE {100 * S5_REL[cell][metric]:.1f} %) -> {c}")
            # §10-reference reading (reported, not the clause)
            r10 = ref[metric]
            v = vals(p0, key)
            out(f"      vs §10 NEW {r10}: {fmt(100 * (lc.med(v) / r10 - 1), 1)} % (reported)")
            nc_all.append(c)
    if any(c == "UNSCOREABLE" or c is None for c in nc_all):
        nc = "UNSCOREABLE"
    elif any(c == "MOVED" for c in nc_all):
        nc = "REFUTED-WITH-RECORD"
    elif any(c == "UNDERPOWERED" for c in nc_all):
        nc = "GUARD-UNDERPOWERED"
    else:
        nc = "NO-CHANGE-HELD"
    out(f"NO-CHANGE VERDICT: {nc}")
    out("")
    out("--- per-thread core budget (P0 rows; transfer window; median [min-max] (n))")
    for cell in CELLS:
        rs = live(rows, cell, "P0")
        for side, nm in (("cli", "client"), ("srv", "server")):
            out(f"  {cell} {nm}: wall_s={mmm(vals(rs, side + '_wall_s'))}")
            for k in ("thr_r1", "thr_r2", "thr_r3", "main_cores", "workers_cores", "cores"):
                out(f"      {k:14s} {mmm(vals(rs, side + '_' + k), 3)}")
            for i in range(1, 7):
                out(f"      busy_r{i}        {mmm(vals(rs, f'{side}_busy_r{i}'), 3)}")
            for k in ("park_per_s", "unpark_per_s"):
                out(f"      {k:14s} {mmm(vals(rs, side + '_' + k), 0)}")
            for k in ("lag_p50_us", "lag_p99_us", "lag_max_us"):
                out(f"      {k:14s} {mmm(vals(rs, side + '_' + k), 0)}")
            rd = vals(rs, f"rdiag_{side}_busy_med")
            out(f"      rdiag_busy_%   {mmm(rd, 1)}  q_avg {mmm(vals(rs, f'rdiag_{side}_q_med'), 0)}")
            out(f"      top_comm per rep: {[r.get(side + '_top_comm') for r in rs]}")
            for r in rs:
                th = sorted(r.get(f"{side}_threads") or [], key=lambda x: -(x[1] or 0))
                out(f"      s{r['seed']} r{r['rep']} threads: " + " ".join(f"{c}={fmt(v, 3)}" for c, v in th))
    out("")
    v, det = d3_verdict(rows)
    out(f"D3 VERDICT ({D3_CELL}): {v} -- {det}")
    if v == "D3-CONFIRMED":
        out("STOP RULE FIRED: P2's server half (receiver-role split) becomes the primary c1s gate.")
    srv_run = [r for r in rows if r["arm"] == "P0" and r["status"] == "LIVE"]
    out(f"server phase=run [THR] lines present in {sum(1 for r in srv_run if r.get('srv_run_lines'))}"
        f"/{len(srv_run)} live P0 rows (reported)")
    return 0


def main(argv):
    if len(argv) < 2:
        print(__doc__)
        return 2
    cmd = argv[1]
    if cmd == "row":
        a = argv[2:]
        cell, arm, seed, rep, rc, wall = a[0:6]
        drv, cli, srv = lc.read(a[6]), lc.read(a[7]), lc.read(a[8])
        cot = int(a[9]) if len(a) > 9 else 0
        sha = a[10] if len(a) > 10 else None
        print("P0ROW " + json.dumps(make_row(cell, arm, seed, rep, rc, wall, drv, cli, srv, cot, sha),
                                    sort_keys=True))
        return 0
    if cmd == "check":
        return check(argv[2])
    if cmd == "smoke":
        return smoke(argv[2])
    if cmd == "cost":
        return cost(argv[2])
    if cmd == "score":
        return score(argv[2])
    print(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
