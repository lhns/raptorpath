#!/usr/bin/env python3
"""Parser and scorer for THE LOOP-TIME DIAGNOSTIC (docs/status.md §23 follow-up;
paper docs/fec-arq-model.md §6.1 "The law is fed by its own output").

One binary. Arms: MAIN (shipped) and S<n> (RWM_STORE=<n>). Every row runs
RWM_DIAG=1 RWM_ACKDIAG=1 RWM_LOOPDIAG=1: per path and per 250 ms [DIAG] window
the echo-RTT loop-time distribution, Copa's RTprop / jitter / srtt, the
delivered rate and quinn's own BBR pacing rate, side by side.

    loopdiag_parse.py row   <cell> <arm> <seed> <rep> <rc> <wall_s> <drv> <cli> <srv> [cotenant] [sha]
    loopdiag_parse.py smoke <ledger>
    loopdiag_parse.py check <ledger>
    loopdiag_parse.py score <ledger>...

`row` prints one `LDROW {json}` line. Raw sample lists are never stored, only
summaries. Nothing here is a ship verdict.
"""
import json
import math
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import l1common as lc  # noqa: E402
import stage3_parse as sp  # noqa: E402
import storelaw_parse as sl  # noqa: E402  (regexes only)
import anchordiag_parse as ad  # noqa: E402  (shared extractors)
from storelaw_parse import col, f, live, mean  # noqa: E402
from anchordiag_parse import _r, arm_n  # noqa: E402

GRID_C1S = [192, 256, 384, 512, 768, 1024, 1536, 2048]
GRID_C2 = [64, 96, 128, 192, 256, 384, 512, 1024]
GRID = {"c1s-400": GRID_C1S, "c2-100": GRID_C2}
CELLS = ["c1s-400", "c2-100"]
EXPECT_ROWS = 46
WARM_S = 1.0                  # windows before this are warm-up
MIN_LD_WINDOWS = 4            # witness: post-warm-up p0 [LOOPDIAG] windows with n>0
LINK = {                      # L = link datagram rate (sym/s); LINK_B = link bytes/s
    "c1s-400": {"L": 97700.0, "B": 125e6},
    "c2-100": {"L": 9770.0, "B": 12.5e6},
}
W_WIRE = 1279.0               # wire bytes per datagram
Q_GAIN = 1.1                  # (1 + q), q = 0.1 at Bulk
KNEE = 0.95
O1_FLAT, O1_INFL = 1.25, 1.5
O3_LO, O3_HI, O3_SPEAR = 0.8, 1.25, 0.8
REPRO = 1.5                   # a candidate REPRODUCES if in [n*/1.5, 1.5 n*]

LD_KEYS = ["rtt_min_us", "rtt_p10_us", "rtt_p50_us", "rtprop_us", "jit_us", "srtt_us",
           "dlv_rate", "q_pace_bps", "q_cwnd", "q_srtt_us"]

for _a in ["MAIN"] + [f"S{n}" for n in sorted(set(GRID_C1S + GRID_C2))]:
    sp.ARM_SPEC[_a] = ("window", "Rlc", "bulk", "ACTIVE")

_LD = re.compile(
    r"\[LOOPDIAG\] t=([\d.]+)s p(\d+) n=(\d+) ov=(\d+) rtt_min_us=(\S+) rtt_p10_us=(\S+) "
    r"rtt_p50_us=(\S+) rtprop_us=(\S+) jit_us=(\S+) srtt_us=(\S+) dlv=(\d+) dlv_rate=(\S+) "
    r"q_pace_bps=(\S+) q_cwnd=(\S+) q_srtt_us=(\S+)")


def loopdiag_windows(cli, path="0"):
    """Post-warm-up [LOOPDIAG] windows of one path -> list of dicts (None for `-`)."""
    out = []
    for ln in cli:
        m = _LD.search(ln)
        if not m or float(m.group(1)) < WARM_S or m.group(2) != path:
            continue
        g = m.groups()
        d = {"t": float(g[0]), "n": int(g[2]), "ov": int(g[3]), "dlv": int(g[10])}
        for key, v in zip(("rtt_min_us", "rtt_p10_us", "rtt_p50_us", "rtprop_us", "jit_us", "srtt_us"), g[4:10]):
            d[key] = lc.fnum(v)
        d["dlv_rate"] = lc.fnum(g[11])
        d["q_pace_bps"], d["q_cwnd"], d["q_srtt_us"] = lc.fnum(g[12]), lc.fnum(g[13]), lc.fnum(g[14])
        out.append(d)
    return out


def loopdiag_summary(cli):
    """(windows with n>0, {key: median over those windows}, numeric q_pace window count)."""
    ws = [w for w in loopdiag_windows(cli) if w["n"] > 0]
    med = {}
    for k in LD_KEYS:
        v = [w[k] for w in ws if w.get(k) is not None]
        med[k] = _r(lc.med(v), 3) if v else None
    npace = sum(1 for w in ws if w.get("q_pace_bps") is not None)
    return len(ws), med, npace


IO_HIGH_FULL10 = 20.0   # io full avg10 at or above this flags a row (a covariate, never a void)


def make_row(cell, arm, seed, rep, rc, wall_s, drv, cli, srv, cotenant=0, sha=None):
    row = sp.make_row(cell, arm, seed, rep, rc, wall_s, drv, cli, srv, cotenant)
    row["bin_sha"] = sha
    row["gate_store"] = lc.gate_tok(cli, "RWM_STORE")
    cap = [m for m in (sl._SUMCAP.search(ln) for ln in cli) if m]
    if cap:
        m = cap[-1]
        row["sumcap_eng"], row["sumcap_refresh"] = int(m.group(2)), int(m.group(3))
        row["sumcap_pin"], row["sumcap_floor"] = float(m.group(7)), float(m.group(8))
        row["sumcap_cap"], row["sumcap_ask"] = float(m.group(9)), float(m.group(10))
    else:
        row["sumcap_eng"] = row["sumcap_refresh"] = None
    wins, rtt = [], []
    for ln in cli:
        mt = sl._T.search(ln)
        if not mt or float(mt.group(1)) < WARM_S:
            continue
        mw = sl._WIN.search(ln)
        if mw:
            wins.append(int(mw.group(2)))
        mr = sl._RTT.search(ln)
        if mr:
            rtt.append(float(mr.group(1)))
    row["win_n"] = len(wins)
    row["win_den_med"] = lc.med(wins) if wins else None
    row["win_den_min"] = min(wins) if wins else None
    row["win_den_max"] = max(wins) if wins else None
    row["rtt_med"] = lc.med(rtt) if rtt else None
    btl, rtp = ad.diag_paths(cli)
    row["btlbw_p"] = {p: lc.med(v) for p, v in sorted(btl.items())}
    row["rtp_us_p"] = {p: lc.med(v) for p, v in sorted(rtp.items())}
    row["rtp_us_med"] = row["rtp_us_p"].get("0")
    ax = [m for m in (sl._AX.search(ln) for ln in srv if "[LAT]" in ln) if m]
    row["ax_p50_us"] = int(ax[-1].group(1)) if ax else None
    row["ax_p95_us"] = int(ax[-1].group(3)) if ax else None
    row["ackdiag"], row["ackdiag_n"] = ad.ackdiag_last(cli)
    row["ld_n"], row["ld_med"], row["ld_pace_n"] = loopdiag_summary(cli)
    nbytes = sp.CELL_SPEC[cell][3]
    row["cpu_cli_gb"] = _r(row["cpu_cli"] / (nbytes / 1e9)) if row.get("cpu_cli") is not None else None

    p = []
    if row["ackdiag_n"] < 1:
        p.append("no-ackdiag")
    if row["ld_n"] < MIN_LD_WINDOWS:
        p.append(f"loopdiag-p0-windows={row['ld_n']}")
    if row["ld_pace_n"] < 1:
        p.append("no-q-pace-bps")
    n = arm_n(arm)
    eng = row["sumcap_eng"]
    if arm == "MAIN":
        if not eng:
            p.append("main-sumcap-eng-zero")
        if row["gate_store"] != "unset":
            p.append(f"gate-store={row['gate_store']}")
    elif n is not None:
        if eng:
            p.append(f"s-sumcap-eng={eng}")
        if row["gate_store"] != str(n):
            p.append(f"gate-store={row['gate_store']}")
        if row["win_n"] < 2:
            p.append(f"s-win-windows={row['win_n']}")
        elif row["win_den_min"] != n or row["win_den_max"] != n:
            p.append(f"s-win-den-not-n={row['win_den_min']}..{row['win_den_max']}")
    row["problems"].extend(p)
    if row["problems"] and row["status"] == "LIVE":
        row["status"] = "WITNESS-FAIL"
    # I/O pressure covariate (section 24 amendment 3): the max io `full avg10`
    # read before/after the row. Recorded and flagged, never a void.
    io = os.environ.get("LD_IO_FULL10")
    try:
        row["io_full10"] = float(io) if io not in (None, "") else None
    except ValueError:
        row["io_full10"] = None
    row["io_high"] = row["io_full10"] is not None and row["io_full10"] >= IO_HIGH_FULL10
    # A row whose binary or run dir is not on tmpfs is VOID-ONDISK (shared rules).
    if os.environ.get("LD_ONDISK", "0") == "1":
        row["status"] = "VOID-ONDISK"
    return row


# ── scoring helpers ──────────────────────────────────────────────────────
def rows_of(paths):
    rows = []
    for path in paths:
        for ln in lc.read(path):
            if ln.startswith("LDROW "):
                try:
                    rows.append(json.loads(ln[len("LDROW "):]))
                except ValueError:
                    pass
    return rows


def median(v):
    v = sorted(v)
    if not v:
        return None
    k = len(v)
    return v[k // 2] if k % 2 else (v[k // 2 - 1] + v[k // 2]) / 2


def ldcol(rs, key):
    return [(r.get("ld_med") or {}).get(key) for r in rs if (r.get("ld_med") or {}).get(key) is not None]


def spearman(x, y):
    return ad.spearman(x, y)


def per_n(rows, cell):
    """{n: {live, G, Gmin, Gmax, ax, rtt, <LD_KEYS>...}} -- per-n means of the row values, live S<n> rows."""
    out = {}
    for n in GRID[cell]:
        rs = live(rows, cell, f"S{n}")
        d = {"live": len(rs)}
        g = col(rs, "mbps")
        d["G"], d["Gmin"], d["Gmax"] = mean(g), (min(g) if g else None), (max(g) if g else None)
        d["ax"] = mean(col(rs, "ax_p50_us"))
        d["rtt"] = mean(col(rs, "rtt_med"))
        for k in LD_KEYS:
            d[k] = mean(ldcol(rs, k))
        out[n] = d
    return out


def in_band(x, nstar):
    return x is not None and nstar / REPRO <= x <= REPRO * nstar


# ── the score for one cell ───────────────────────────────────────────────
def score_cell(rows, cell, out):
    """Prints every intermediate number; returns {'O1','O3','o1_repro','o3_repro',...}."""
    L, B = LINK[cell]["L"], LINK[cell]["B"]
    pn = per_n(rows, cell)
    out(f"CELL {cell}  (L={L:g} sym/s, link {B:g} B/s, W={W_WIRE:g} B/datagram, q={Q_GAIN - 1:.1f})")
    out("  n     live  goodput mean [min-max]   A_x p50   rtt ms  Tmin us  Tp10 us  Tp50 us  RTprop us  jit us"
        "       D sym/s    Q=quinn/link  q_cwnd B")
    for n in GRID[cell]:
        d = pn[n]
        d["Q"] = d["q_pace_bps"] / 8.0 / B if d["q_pace_bps"] is not None else None
        out(f"  {n:<5} {d['live']:<5} {f(d['G'],1):>7} [{f(d['Gmin'],1)}-{f(d['Gmax'],1)}]"
            f"{'':<3}{f(d['ax'],0):>8}  {f(d['rtt'],1):>7}  {f(d['rtt_min_us'],0):>7}  {f(d['rtt_p10_us'],0):>7}"
            f"  {f(d['rtt_p50_us'],0):>7}  {f(d['rtprop_us'],0):>9}  {f(d['jit_us'],0):>6}"
            f"  {f(d['dlv_rate'],0):>10}  {f(d['Q'],3):>12}  {f(d['q_cwnd'],0):>9}")
    res = {"O1": "-", "O3": "-", "o1_repro": None, "o3_repro": None, "nstar": None}
    G = [(n, pn[n]["G"]) for n in GRID[cell] if pn[n]["G"] is not None]
    if not G:
        out("  n*: - (no goodput readings)")
        out(f"  OPTION1 - MISSES n_fp1=-")
        out(f"  OPTION3 - MISSES C3med=- C3b_med=-")
        res["o1_repro"] = res["o3_repro"] = False
        return res
    gmax = max(g for _, g in G)
    nstar = min(n for n, g in G if g >= KNEE * gmax)
    res["nstar"] = nstar
    lo, hi = nstar / REPRO, REPRO * nstar
    out(f"  Gmax {f(gmax,1)}  n* = {nstar} (smallest n with G >= {KNEE:g} Gmax = {f(KNEE * gmax,1)});"
        f" reproduction band [n*/1.5, 1.5 n*] = [{f(lo,1)}, {f(hi,1)}]")

    # option 1: the minimum loop time
    tm = [(n, pn[n]["rtt_min_us"]) for n in GRID[cell] if pn[n]["rtt_min_us"] is not None]
    if len(tm) >= 2:
        n_lo, t_lo = tm[0]
        n_hi, t_hi = tm[-1]
        r1 = t_hi / t_lo
        flat = all(t <= O1_FLAT * t_lo for _, t in tm)
        o1 = "FLAT" if flat else ("INFLATES" if r1 >= O1_INFL else "AMBIGUOUS")
        out(f"  O1: Tmin(n={n_lo})={f(t_lo,0)} us, Tmin(n={n_hi})={f(t_hi,0)} us, R1 = {f(r1,3)}; "
            f"FLAT iff every Tmin <= {O1_FLAT:g} x {f(t_lo,0)} = {f(O1_FLAT * t_lo,0)} (max {f(max(t for _, t in tm),0)}); "
            f"INFLATES iff R1 >= {O1_INFL:g}  -> {o1}")
    else:
        o1 = "-"
        out("  O1: - (Tmin computable at fewer than two grid points)")
    # C1(n) = 1.1 D(n) Tmin(n); fixed point
    c1 = {}
    for n in GRID[cell]:
        d = pn[n]
        c1[n] = Q_GAIN * d["dlv_rate"] * d["rtt_min_us"] / 1e6 if d["dlv_rate"] is not None and d["rtt_min_us"] is not None else None
    out("  C1(n) = 1.1 D(n) Tmin(n) symbols;  C1/n:  " +
        "  ".join(f"{n}:{f(c1[n],1)}/{f(c1[n] / n if c1[n] is not None else None,3)}" for n in GRID[cell]))
    n_fp1 = next((n for n in GRID[cell] if c1[n] is not None and c1[n] < n), None)
    out(f"  n_fp1 = smallest grid n with C1(n) < n = {n_fp1 if n_fp1 is not None else '-'}")
    o1_repro = in_band(n_fp1, nstar)

    # option 3: quinn's bandwidth
    qs = [(n, pn[n]["Q"]) for n in GRID[cell] if pn[n]["Q"] is not None]
    if qs:
        qv = [q for _, q in qs]
        qmed = median(qv)
        rho = spearman([n for n, _ in qs], qv)
        q_nmin = qs[0][1]
        if all(O3_LO <= q <= O3_HI for q in qv):
            o3 = "FLAT-AT-LINK"
        elif qmed > O3_HI:
            o3 = "OVER-READ"
        elif rho is not None and rho >= O3_SPEAR and q_nmin < O3_LO:
            o3 = "CIRCULAR"
        else:
            o3 = "NEITHER"
        out(f"  O3: Q(n) = " + " ".join(f"{n}:{f(q,3)}" for n, q in qs) +
            f"; median Q {f(qmed,3)}; Spearman(Q,n) {f(rho,3)}; Q(n_min={qs[0][0]}) {f(q_nmin,3)}; "
            f"FLAT-AT-LINK iff all in [{O3_LO:g},{O3_HI:g}]; OVER-READ iff median > {O3_HI:g}; "
            f"CIRCULAR iff Spearman >= {O3_SPEAR:g} and Q(n_min) < {O3_LO:g}  -> {o3}")
    else:
        o3 = "-"
        out("  O3: - (no quinn pacing readings)")
    c3, c3b, cj = {}, {}, {}
    for n in GRID[cell]:
        d = pn[n]
        rate = d["Q"] * B / W_WIRE if d["Q"] is not None else None
        rp = d["rtprop_us"] / 1e6 if d["rtprop_us"] is not None else None
        tm_ = d["rtt_min_us"] / 1e6 if d["rtt_min_us"] is not None else None
        c3[n] = Q_GAIN * rate * rp if rate is not None and rp is not None else None
        c3b[n] = Q_GAIN * rate * tm_ if rate is not None and tm_ is not None else None
        cj[n] = Q_GAIN * L * (rp + d["jit_us"] / 1e6) if rp is not None and d["jit_us"] is not None else None
    out("  C3(n)  = 1.1 (Q linkB / W) RTprop : " + "  ".join(f"{n}:{f(c3[n],1)}" for n in GRID[cell]))
    out("  C3b(n) = 1.1 (Q linkB / W) Tmin   : " + "  ".join(f"{n}:{f(c3b[n],1)}" for n in GRID[cell]))
    out("  CJ(n)  = 1.1 L (RTprop + jit)     : " + "  ".join(f"{n}:{f(cj[n],1)}" for n in GRID[cell]) + "  (report only)")
    c3med = median([v for v in c3.values() if v is not None])
    c3bmed = median([v for v in c3b.values() if v is not None])
    cjmed = median([v for v in cj.values() if v is not None])
    o3_repro = in_band(c3med, nstar)
    out(f"  medians over n: C3 {f(c3med,1)} ({'REPRODUCES' if o3_repro else 'MISSES'}), "
        f"C3b {f(c3bmed,1)} ({'REPRODUCES' if in_band(c3bmed, nstar) else 'MISSES'}), "
        f"CJ {f(cjmed,1)} ({'REPRODUCES' if in_band(cjmed, nstar) else 'MISSES'}, report only); "
        f"band [{f(lo,1)}, {f(hi,1)}]")
    out(f"  OPTION1 {o1} {'REPRODUCES' if o1_repro else 'MISSES'} n_fp1={n_fp1 if n_fp1 is not None else '-'}")
    out(f"  OPTION3 {o3} {'REPRODUCES' if o3_repro else 'MISSES'} C3med={f(c3med,1)} C3b_med={f(c3bmed,1)}")
    res.update(O1=o1, O3=o3, o1_repro=o1_repro, o3_repro=o3_repro, n_fp1=n_fp1, c3med=c3med, c3bmed=c3bmed)
    return res


def main_reference(rows, cell, out):
    rs = live(rows, cell, "MAIN")
    out(f"  MAIN (live n={len(rs)}) reference: goodput {f(mean(col(rs, 'mbps')),1)} Mbit/s, "
        f"A_x p50 {f(mean(col(rs, 'ax_p50_us')),0)} us, SUMCAP cap {f(mean(col(rs, 'sumcap_cap')),1)}")
    out("    LOOPDIAG medians: " + " ".join(f"{k}={f(mean(ldcol(rs, k)),0)}" for k in LD_KEYS))


def score(paths, out=print):
    rows = rows_of(paths)
    out("LOOP-DIAG SCORE (diagnostic, nothing here ships)")
    out("")
    out("ROWS by (cell, arm): live / total / statuses")
    arms = ["MAIN"] + [f"S{n}" for n in sorted(set(GRID_C1S + GRID_C2))]
    for c in CELLS:
        for a in arms:
            rs = [r for r in rows if r["cell"] == c and r["arm"] == a]
            if not rs:
                continue
            st = {}
            for r in rs:
                st[r["status"]] = st.get(r["status"], 0) + 1
            out(f"  {c} {a}: live {len([r for r in rs if r['status'] == 'LIVE'])}/{len(rs)} {st}")
    out("")
    out("VOID TABLE (rows not LIVE)")
    void = [r for r in rows if r["status"] != "LIVE"]
    for r in sorted(void, key=lambda r: (r["cell"], r["arm"], r["seed"], r["rep"])):
        out(f"  {r['cell']} {r['arm']} s{r['seed']} r{r['rep']}: {r['status']} {','.join(r['problems'])}")
    if not void:
        out("  none")
    out("")
    out(f"IO PRESSURE (covariate; flagged at full avg10 >= {IO_HIGH_FULL10:g}, never voided)")
    for r in sorted(rows, key=lambda r: (r["cell"], r["arm"], r["seed"], r["rep"])):
        if r.get("io_high"):
            out(f"  FLAG {r['cell']} {r['arm']} s{r['seed']} r{r['rep']}: io_full10={r['io_full10']} "
                f"goodput={r.get('mbps')} status={r['status']}")
    if not any(r.get("io_high") for r in rows):
        out("  no row flagged")
    out("")
    for cell in CELLS:
        score_cell(rows, cell, out)
        main_reference(rows, cell, out)
        out("")
    return 0


def check(path):
    t = open(path, errors="replace").read()
    ok = "LD-BATTERY-DONE" in t
    n = len([ln for ln in t.splitlines() if ln.startswith("LDROW ")])
    good = ok and n == EXPECT_ROWS
    print(f"CHECK done={ok} rows={n}/{EXPECT_ROWS}")
    return 0 if good else 1


def smoke(path):
    got = {(r["cell"], r["arm"]): r for r in rows_of([path])}
    need = [("c1s-400", "MAIN"), ("c2-100", "S192")]
    bad = []
    for k in need:
        r = got.get(k)
        if r is None:
            bad.append(f"{k}: no row")
            continue
        if r["status"] != "LIVE":
            bad.append(f"{k}: {r['status']} {r['problems']}")
        if not r.get("gates_cli"):
            bad.append(f"{k}: no [GATES] echo")
        if not r.get("ackdiag_n"):
            bad.append(f"{k}: no [ACKDIAG]")
        print(f"SMOKE {k}: mbps={r.get('mbps')} eng={r.get('sumcap_eng')} ld_n={r.get('ld_n')} "
              f"ld_med={r.get('ld_med')} win_den={r.get('win_den_min')}..{r.get('win_den_max')} status={r['status']}")
    if bad:
        print("SMOKE-FAIL " + "; ".join(bad))
        return 1
    print("SMOKE-PASS")
    return 0


def main(argv):
    if len(argv) < 2:
        print(__doc__)
        return 2
    cmd = argv[1]
    if cmd == "row":
        cell, arm, seed, rep, rc, wall, drv, cli, srv = argv[2:11]
        cot = int(argv[11]) if len(argv) > 11 else 0
        sha = argv[12] if len(argv) > 12 else None
        row = make_row(cell, arm, seed, rep, rc, wall, lc.read(drv), lc.read(cli), lc.read(srv), cot, sha)
        print("LDROW " + json.dumps(row, sort_keys=True))
        return 0
    if cmd == "smoke":
        return smoke(argv[2])
    if cmd == "check":
        return check(argv[2])
    if cmd == "score":
        return score(argv[2:])
    print(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
