#!/usr/bin/env python3
"""Parser and scorer for THE V-P1 BATTERY (docs/status.md, "Threading P1 —
pre-registration"): the threading-P1 binary (P1) against `main` (MAIN), one
session, interleaved.

    threadp1_parse.py row   <cell> <arm> <seed> <rep> <rc> <wall_s> <drv_out> <cli_log> <srv_log> [cotenant] [bin_sha]
    threadp1_parse.py check <ledger>
    threadp1_parse.py smoke <ledger>
    threadp1_parse.py cost  <ledger>
    threadp1_parse.py score <ledger>

`row` reuses `stage3_parse.make_row` (statuses, goodput/completion/CPU,
`[TRUTH]`, `plc`, `busy`, the two-sided `[PIPE]`/`[GATES]`/RLC/cadence/
`RWM_POOL_ANCHOR=0` witnesses) and adds:

  emit     `[GATES] RWM_EMIT_BATCH=1` on both endpoints and the "emission
           batching ACTIVE" echo on the client: both arms run the shipped
           default (emission batching ON since bf3a636);
  wake     the client's LAST `[DIAG]` `wake[..]` token (cumulative counts;
           threading P1, D1). Present on every P1 row and on no MAIN row —
           the P1 binary's execution witness besides its sha256;
  rtp      per leg, the RTprop floor: the minimum non-zero `p<i>: … rtp_us=`
           over the client's `[DIAG]` lines;
  bin      the row's `bin_sha` is its arm's binary (ledger header).

Scoring constants are below, one definition each, restated in the
pre-registration.
"""
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import l1common as lc  # noqa: E402
import stage3_parse as sp  # noqa: E402

# ── THE DESIGN ───────────────────────────────────────────────────────────
ARMS = {
    "MAIN": ("window", "Rlc", "bulk", "ACTIVE"),
    "P1": ("window", "Rlc", "bulk", "ACTIVE"),
}
sp.ARM_SPEC.update(ARMS)
ARM_BIN = {"MAIN": "main", "P1": "p1"}
CELLS = ["c1s-400", "c1d-400", "c2-100", "c8-100"]
SEEDS = ["42", "7"]
EMB_ECHO = "emission batching ACTIVE"

# §5's committed relative MDE (status.md §5 (a)): cell -> {gp, cpu}. `cpu` is
# the CPUCLI MDE; it is applied to CPUSRV as a declared transfer (§5 measured
# no server-CPU MDE).
S3_REL = {
    "c1s-400": {"gp": 0.049, "cpu": 0.024},
    "c1d-400": {"gp": 0.056, "cpu": 0.065},
    "c2-100": {"gp": 0.014, "cpu": 0.063},
    "c8-100": {"gp": 0.040, "cpu": 0.116},
}
RTP_REL_FLOOR = 0.05      # RTprop tolerance: max(this, MAIN's half-range / median)
DNF_THRESHOLD = 0.20      # §5
MIN_LIVE = 3              # live rows per (cell, arm)
WITNESS_FAIL_LIMIT = 2    # failed rows per (arm, cell) that void the cell
FEED_BAND = 1.3           # P1's plc/truth within [1/1.3, 1.3] x MAIN's, per leg
WAKE_MIN = 100            # paused-type wakes (paused + ack) for a row to read D1
WAKE_RATIO = 0.05         # D1 holds on a row iff wake[timer_acked] <= 0.05 x wake[ack]
ABORT_FIRST_FIVE = sp.ABORT_FIRST_FIVE

_WAKE = re.compile(r"\bwake\[([^\]]*)\]")
_RTP = re.compile(r"(?:^|\s)p(\d+):infl=.*?\brtp_us=(\d+)")


def wake_of(cli):
    """The last client `[DIAG]` wake[..] token as a dict, or None."""
    d = [ln for ln in cli if "[DIAG] t=" in ln]
    for ln in reversed(d):
        m = _WAKE.search(ln)
        if m:
            out = {}
            for kv in m.group(1).split():
                k, _, v = kv.partition("=")
                try:
                    out[k] = int(v)
                except ValueError:
                    pass
            return out
    return None


def rtp_floor(cli):
    """Per leg, the minimum non-zero `rtp_us` over the client's `[DIAG]` lines."""
    best = {}
    for ln in cli:
        if "[DIAG] t=" not in ln:
            continue
        for m in _RTP.finditer(ln):
            i, v = int(m.group(1)), int(m.group(2))
            if v > 0:
                best[i] = min(v, best.get(i, v))
    return best


# ── ROW ──────────────────────────────────────────────────────────────────
def make_row(cell, arm, seed, rep, rc, wall_s, drv, cli, srv, cotenant=0, bin_sha=None):
    row = sp.make_row(cell, arm, seed, rep, rc, wall_s, drv, cli, srv, cotenant)
    row["bin_sha"] = bin_sha
    row["bin"] = ARM_BIN[arm]
    row["emb_gate_cli"] = lc.gate(cli, "RWM_EMIT_BATCH")
    row["emb_gate_srv"] = lc.gate(srv, "RWM_EMIT_BATCH")
    row["emb_echo_cli"] = any(EMB_ECHO in ln for ln in cli)
    w = wake_of(cli)
    row["wake"] = w
    for i, v in rtp_floor(cli).items():
        row["rtp_floor_p%d" % i] = v
    p = []
    if row["emb_gate_cli"] != 1 or row["emb_gate_srv"] != 1:
        p.append(f"emit-gate={row['emb_gate_cli']}/{row['emb_gate_srv']}")
    if not row["emb_echo_cli"]:
        p.append("emit-echo-missing")
    if arm == "P1" and w is None:
        p.append("wake-token-missing-on-P1")
    if arm == "MAIN" and w is not None:
        p.append("wake-token-on-MAIN")
    row["problems"].extend(p)
    if p and row["status"] == "LIVE":
        row["status"] = "WITNESS-FAIL"
    return row


def bin_check(rows, shas):
    for r in rows:
        want = shas.get(r.get("bin"))
        if want and r.get("bin_sha") and r["bin_sha"] != want and r["status"] == "LIVE":
            r["status"] = "CONTAMINATED"
            r["problems"].append("bin-sha-mismatch")
    return rows


# ── LEDGER ───────────────────────────────────────────────────────────────
_HDR_SHA = re.compile(r"^=== binary (MAIN|P1) \S+ sha256 ([0-9a-f]{64})")


def rows_of(paths):
    rows, tokens, shas = [], [], {}
    for path in paths:
        for ln in lc.read(path):
            m = _HDR_SHA.match(ln)
            if m:
                shas[m.group(1).lower()] = m.group(2)
            if ln.startswith("TP1ROW "):
                try:
                    rows.append(json.loads(ln[len("TP1ROW "):]))
                except ValueError:
                    tokens.append("MALFORMED-TP1ROW")
            for t in ABORT_FIRST_FIVE:
                if ln.startswith(t):
                    tokens.append(t)
            if ln.startswith("TRUNCATED-AT-REP-BOUNDARY"):
                tokens.append("TRUNCATED-AT-REP-BOUNDARY")
    return bin_check(rows, shas), tokens, shas


fmt = sp.fmt


def sel(live, cell, arm, seed=None):
    return [r for r in live if r["cell"] == cell and r["arm"] == arm
            and (seed is None or r["seed"] == seed)]


def vals(rows, key):
    return [r[key] for r in rows if r["dnf"] is False and r.get(key) is not None]


def per_byte(rows, key):
    """CPU seconds per GB moved (the cell's bytes are fixed, so this is the
    CPU clause; printed in a per-byte unit)."""
    out = []
    for r in rows:
        if r["dnf"] is False and r.get(key) is not None:
            b = sp.CELL_SPEC[r["cell"]][3] if len(sp.CELL_SPEC[r["cell"]]) > 3 else None
            out.append(r[key] / (b / 1e9) if b else r[key])
    return out


def reading(xv, rv, rel, higher_is_better):
    """The median clause with the min-max rule. Returns (reading, detail).
    WORSE/BETTER: median beyond ref*(1 -/+ rel) in that direction AND the
    two arms' [min, max] ranges disjoint in that direction; TREND-WORSE /
    TREND-BETTER: median beyond the band, ranges overlapping (reported, not
    a fail); WITHIN otherwise."""
    if not rv:
        return "VACUOUS", "reference has no value"
    if not xv:
        return "WORSE", "P1 has no value"
    xm, rm = lc.med(xv), lc.med(rv)
    lo, hi = rm * (1 - rel), rm * (1 + rel)
    det = (f"P1={xm:.4g} [{min(xv):.4g}-{max(xv):.4g}] MAIN={rm:.4g} [{min(rv):.4g}-{max(rv):.4g}] "
           f"({100 * (xm / rm - 1):+.1f}%, rel={rel:.3f})")
    if higher_is_better:
        worse_med, better_med = xm < lo, xm > hi
        disjoint_worse, disjoint_better = max(xv) < min(rv), min(xv) > max(rv)
    else:
        worse_med, better_med = xm > hi, xm < lo
        disjoint_worse, disjoint_better = min(xv) > max(rv), max(xv) < min(rv)
    if worse_med:
        return ("WORSE" if disjoint_worse else "TREND-WORSE"), det
    if better_med:
        return ("BETTER" if disjoint_better else "TREND-BETTER"), det
    return "WITHIN", det


def witness_fails(rows, arm, cell):
    return sum(1 for r in rows if r["arm"] == arm and r["cell"] == cell
               and r["status"] in ("CONTAMINATED", "WITNESS-FAIL"))


def blockers(rows, live, cell, aborted):
    why = []
    for a in ARMS:
        nl = len(sel(live, cell, a))
        if nl < MIN_LIVE:
            why.append(f"{a} live={nl}<{MIN_LIVE}")
        wf = witness_fails(rows, a, cell)
        if wf >= WITNESS_FAIL_LIMIT:
            why.append(f"{a} witness-failed={wf}")
    if aborted:
        why.append("abort")
    return why


def cell_verdict(rows, live, cell, aborted, out):
    xs, rs = sel(live, cell, "P1"), sel(live, cell, "MAIN")
    why = blockers(rows, live, cell, aborted)
    rel = S3_REL[cell]
    clauses = {}
    clauses["goodput"] = reading(vals(xs, "mbps"), vals(rs, "mbps"), rel["gp"], True)
    clauses["cpu_cli/GB"] = reading(per_byte(xs, "cpu_cli"), per_byte(rs, "cpu_cli"), rel["cpu"], False)
    clauses["cpu_srv/GB"] = reading(per_byte(xs, "cpu_srv"), per_byte(rs, "cpu_srv"), rel["cpu"], False)
    feed_bad, feed_unread = [], []
    for i in range(sp.N_LEGS[cell]):
        k = "rtp_floor_p%d" % i
        rv = vals(rs, k)
        rrel = RTP_REL_FLOOR
        if rv:
            m = lc.med(rv)
            rrel = max(RTP_REL_FLOOR, 0.5 * (max(rv) - min(rv)) / m if m else 0.0)
        clauses[f"rtp_floor_p{i}"] = reading(vals(xs, k), rv, rrel, False)
        fk = "feed_ratio_p%d" % i
        fx, fr = lc.med(vals(xs, fk)), lc.med(vals(rs, fk))
        if fx is None or fr is None:
            feed_unread.append(i)
            out(f"  FEED {cell} p{i} P1={fmt(fx, 3)} MAIN={fmt(fr, 3)} -> UNREAD")
        else:
            ok = fr / FEED_BAND <= fx <= fr * FEED_BAND
            if not ok:
                feed_bad.append(i)
            out(f"  FEED {cell} p{i} plc/truth P1={fx:.3f} MAIN={fr:.3f} band=[{fr / FEED_BAND:.3f},"
                f"{fr * FEED_BAND:.3f}] -> {'SAME' if ok else 'FEED-MOVED'}")
    dr = (sum(1 for r in rs if r["dnf"]) / len(rs)) if rs else None
    dx = (sum(1 for r in xs if r["dnf"]) / len(xs)) if xs else None
    dnf_bad = dr is not None and dx is not None and dx - dr > DNF_THRESHOLD
    for k, (rd, det) in clauses.items():
        out(f"  CLAUSE {cell} {k}: {rd} {det}")
    worse = [k for k, (rd, _) in clauses.items() if rd == "WORSE"]
    worse += [f"feed_p{i}" for i in feed_bad] + (["dnf"] if dnf_bad else [])
    better = [k for k, (rd, _) in clauses.items() if rd == "BETTER" and not k.startswith("rtp")]
    if why or feed_unread:
        v = "UNSCOREABLE"
    elif worse:
        v = "WORSE"
    elif better:
        v = "BETTER"
    else:
        v = "SAME"
    trend = [f"{k}:{rd}" for k, (rd, _) in clauses.items() if rd.startswith("TREND")]
    out(f"  CELL {cell} P1 n={len(xs)} MAIN n={len(rs)} dnf MAIN={fmt(dr, 2)} P1={fmt(dx, 2)} "
        f"worse={'+'.join(worse) or '-'} better={'+'.join(better) or '-'} trend={','.join(trend) or '-'}"
        + (f" UNSCOREABLE({'; '.join(why + [f'feed-unread-p{i}' for i in feed_unread])})"
           if (why or feed_unread) else "")
        + f" => {v}")
    return v


def wake_reading(live, cell, out):
    """D1's mechanism reading at a cell (P1 rows): per row wake[paused] vs
    wake[ack]; rows with fewer than WAKE_MIN paused-type wakes are not read."""
    rows = sel(live, cell, "P1")
    read, held = 0, 0
    for r in rows:
        w = r.get("wake") or {}
        pa, ak, ta = w.get("paused", 0), w.get("ack", 0), w.get("timer_acked")
        ok = None
        if pa + ak >= WAKE_MIN and ta is not None:
            read += 1
            ok = ta <= WAKE_RATIO * ak
            held += int(ok)
        out(f"  WAKE {cell} s{r['seed']} rep{r['rep']} timer_acked={ta} paused={pa} ack={ak} "
            f"tun={w.get('tun')} pace={w.get('pace')} tail={w.get('tail')} nack={w.get('nack')} "
            f"-> {'-' if ok is None else ('HOLDS' if ok else 'FAILS')}")
    if read == 0:
        return "D1-INERT-NEVER-PAUSED"
    return "D1-WAKE-HOLDS" if held == read else f"D1-WAKE-FAILS({read - held}/{read})"


def score(paths, out=print):
    rows, tokens, shas = rows_of(paths)
    live = [r for r in rows if r["status"] == "LIVE"]
    aborted = [t for t in ABORT_FIRST_FIVE if t in tokens]
    out(f"BINARIES main={shas.get('main', '-')} p1={shas.get('p1', '-')}")
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
    out("PER-REP (rule 4): cell arm seed rep mbps cpu_cli cpu_srv busy rtp_floor feed wake[paused/ack]")
    for r in sorted(rows, key=lambda r: (CELLS.index(r["cell"]) if r["cell"] in CELLS else 9,
                                         r["arm"], r["seed"], r["rep"])):
        legs = range(sp.N_LEGS.get(r["cell"], 1))
        w = r.get("wake") or {}
        out(f"  {r['cell']} {r['arm']} s{r['seed']} r{r['rep']} {r['status']} mbps={fmt(r.get('mbps'), 1)} "
            f"cli={fmt(r.get('cpu_cli'), 2)} srv={fmt(r.get('cpu_srv'), 2)} busy={fmt(r.get('busy_med'), 1)} "
            f"rtp={'/'.join(str(r.get('rtp_floor_p%d' % i, '-')) for i in legs)} "
            f"feed={'/'.join(fmt(r.get('feed_ratio_p%d' % i), 3) for i in legs)} "
            f"wake={w.get('paused', '-')}/{w.get('ack', '-')}")
    out("")
    out("PER-SEED MEDIANS (goodput, cpu_cli)")
    for cell in CELLS:
        for a in ARMS:
            out(f"  {cell} {a} " + " ".join(
                f"s{s}: gp={fmt(lc.med(vals(sel(live, cell, a, s), 'mbps')), 1)} "
                f"cli={fmt(lc.med(vals(sel(live, cell, a, s), 'cpu_cli')), 2)}" for s in SEEDS))
    out("")
    verdicts, wake = {}, {}
    for cell in CELLS:
        out(f"[{cell}]")
        verdicts[cell] = cell_verdict(rows, live, cell, aborted, out)
        wake[cell] = wake_reading(live, cell, out)
        bx = lc.med(vals(sel(live, cell, "P1"), "busy_med"))
        br = lc.med(vals(sel(live, cell, "MAIN"), "busy_med"))
        out(f"  BUSY {cell} sender busy median MAIN={fmt(br, 1)}% P1={fmt(bx, 1)}% (reported)")
        out(f"  D1 {cell} {wake[cell]}")
    out("")
    out("VERDICT-CELLS " + " ".join(f"{c}:{verdicts[c]}" for c in CELLS))
    worse = [c for c in CELLS if verdicts[c] == "WORSE"]
    unsc = [c for c in CELLS if verdicts[c] == "UNSCOREABLE"]
    if aborted:
        v = "UNSCOREABLE (abort: " + ",".join(aborted) + ")"
    elif worse:
        v = "REFUTED-WITH-RECORD (WORSE-AT-" + ",".join(worse) + ")"
    elif unsc:
        v = "UNSCOREABLE-AT-" + ",".join(unsc)
    else:
        better = [c for c in CELLS if verdicts[c] == "BETTER"]
        v = "DELIVERED" + (" (BETTER-AT-" + ",".join(better) + ")" if better else " (SAME everywhere)")
    out("VERDICT " + v)
    out("D1-MECHANISM " + " ".join(f"{c}:{wake[c]}" for c in CELLS))
    bx = lc.med(vals(sel(live, "c1s-400", "P1"), "busy_med"))
    br = lc.med(vals(sel(live, "c1s-400", "MAIN"), "busy_med"))
    if bx is None or br is None:
        out("PREDICTION c1s busy falls: UNREAD")
    else:
        out(f"PREDICTION c1s busy falls: {'MET' if bx < br else 'MISSED'} (MAIN {br:.1f}% -> P1 {bx:.1f}%)")
    return v


def check(path):
    rows, tokens, _ = rows_of([path])
    if not rows or "MALFORMED-TP1ROW" in tokens:
        print(f"CHECK-FAIL {path} rows={len(rows)} tokens={tokens}")
        return 1
    print(f"CHECK-OK {path} rows={len(rows)} live={sum(1 for r in rows if r['status'] == 'LIVE')}")
    return 0


def smoke(path):
    """rc 0 iff every smoke row is LIVE with every gauge the scorer reads and
    both arms appear; the P1 rows carry `wake[`, the MAIN rows do not."""
    rows, _, _ = rows_of([path])
    bad = []
    for r in rows:
        miss = []
        if r["status"] != "LIVE":
            miss.append(r["status"])
        for k in ("cpu_cli", "cpu_srv", "busy_med", "mbps"):
            if r.get(k) is None:
                miss.append(f"no-{k}")
        for i in range(sp.N_LEGS[r["cell"]]):
            for k in ("truth_loss_p%d" % i, "plc_p%d" % i, "rtp_floor_p%d" % i):
                if r.get(k) is None:
                    miss.append(f"no-{k}")
        print(f"SMOKE-ROW {r['cell']} {r['arm']} bin={r['bin']} rc={r['rc']} status={r['status']} "
              f"mbps={r.get('mbps')} cli={r.get('cpu_cli')} srv={r.get('cpu_srv')} busy={r.get('busy_med')} "
              f"wake={r.get('wake')} {' '.join(r['problems'] + miss)}")
        if miss:
            bad.append(r)
    arms = {r["arm"] for r in rows}
    if not rows or bad or arms != set(ARMS):
        print(f"ABORT-SMOKE rows={len(rows)} bad={len(bad)} arms={sorted(arms)}")
        return 1
    print(f"SMOKE-PASS rows={len(rows)}")
    return 0


def main(argv):
    if not argv:
        print(__doc__)
        return 2
    cmd, a = argv[0], argv[1:]
    if cmd == "row":
        cell, arm, seed, rep, rc, wall, drv, cli, srv = a[:9]
        cot = a[9] if len(a) > 9 else 0
        bsha = a[10] if len(a) > 10 else None
        row = make_row(cell, arm, seed, rep, rc, wall, lc.read(drv), lc.read(cli), lc.read(srv), cot, bsha)
        print("TP1ROW " + json.dumps(row, sort_keys=True))
        return 0
    if cmd == "check":
        return check(a[0])
    if cmd == "smoke":
        return smoke(a[0])
    if cmd == "cost":
        return sp.cost(a[0])
    if cmd == "score":
        score(a)
        return 0
    print(f"unknown command {cmd}")
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
