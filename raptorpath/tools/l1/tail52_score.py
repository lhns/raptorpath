#!/usr/bin/env python3
"""Status 52: the c8 recovery levers, REPORTED ONLY. Per-arm readouts at c8-100, 600 MB, from the sender's [DIAG] / [HOLD] lines and the receiver's [C8CONV-R].

    tail52_score.py score <ledger> [...]   per-arm report from a plan-52 ledger (rows carry `t52`, computed at row time by `row_metrics`)
    tail52_score.py dry <diag dir> [ARM..]  the same readouts from raw logs (`<cell>@def-<ARM>-s<seed>-r<rep>-a<n>-{c.log,s.log,drv.out}`), e.g. the unpacked
                                            docs/l1-raw/bbr48b/diag-ld-full.tgz: reproduces the committed §48b c8 numbers (test_tail52.py)
    tail52_score.py plan [smoke]            plan 52's reported-block table (`cell store arm seed rep` per line)

Nothing here is a verdict: no clause, no threshold, no pass / fail. The only gate is the row's own witness (the arm's env echoed in [GATES]).

Definitions are ports of the committed post-hoc scripts (docs/l1-raw/bbr48b/tail-c8/{rows,price,build}.py, rtxleg/{a1_counts,a2_little,a4_hold}.py), same regexes:
  * ise = the last [DIAG] line with src > 0 at t >= 1 s (the last line that still emits new source); tail_s = run seconds ([median_s] of the driver summary) - t(ise);
  * H0 = `win=` (store outstanding) at the first [DIAG] line after ise with t <= run seconds (rows.py; None if there is none);
  * lag_end = src - ack of `cum=src/coded/acked` at ise (frontier lag, symbols);
  * send_leg1_share = on1 / (on0 + on1) of the LAST line's `fp/on p0:fp/on p1:fp/on` (retransmits SENT on leg 1, whole run; the `fp` side is the live-flight leg);
  * K, per 4 s bin of lines with src > 0 and at least two lines (price.py): K = mu * C, mu = d(retx)/dt, C = median pooled clock `rtt=` (seconds); the bin's
    Nh = d(seqs) / d(rep) is the gap sequences visited per processed report (a2_little.py, lever B's occupancy) and is reported beside it;
  * [HOLD] (sender, last line per ORIGINAL path): first-report age p50 / p90 (`age_p50_us`, `age_p90_us`: age of the sequence's live flight at its first report),
    closed-by-retransmit p50 / p90 (`cx_p50_us`, `cx_p90_us`, timed from the first report) and `cx_leg=<leg>:<n>,...` (send leg of the retransmit that closed the hole;
    `-` = none). A binary without `cx_leg=` (every binary before the diagnostic branch) reads None, never an error;
  * receiver duplicates = the sum of `dup=` over the paths of the last [C8CONV-R] line (None without one);
  * CPU/GB from `CPU: CPUSRV=<s> CPUCLI=<s>` of the driver output (whole invocation, warm-up included) over the transfer size.
A field absent from a log reads None; the aggregates skip Nones and print `-` when no row has the field."""
import json
import os
import re
import statistics as st
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import l1common as lc  # noqa: E402

BIN_S = 4.0
MAX_BINS = 16
WARM_S = 1.0
GB = 1e9
DEFAULT_BYTES = 600_000_000
ARMS52 = ("BASE", "CAND", "W0", "W05", "WDER")

_T = re.compile(r"t=([0-9.]+)s")
_SRC = re.compile(r"src=(\d+)sym")
_WIN = re.compile(r"win=(\d+)/")
_CUM = re.compile(r"cum=(\d+)/(\d+)/(\d+)")
_RETX = re.compile(r"retx=(\d+)")
_REP = re.compile(r"mpr\[rep=(\d+)")
_SEQS = re.compile(r" seqs=(\d+)")
_RTT = re.compile(r" rtt=([0-9.]+)ms")
_FPON = re.compile(r"fp/on p0:(\d+)/(\d+)(?: p1:(\d+)/(\d+))?")
_CPU = re.compile(r"CPUSRV=([0-9.]+)s CPUCLI=([0-9.]+)s")
_HOLD_PATH = re.compile(r"site=sender path=(\d+)")
_CXLEG = re.compile(r" cx_leg=(\S+)")
_DUP = re.compile(r"\bdup=(\d+)")
HOLD_FIELDS = ("age_p50_us", "age_p90_us", "cx_p50_us", "cx_p90_us", "cx_n", "hn_n", "fed")


def _i(pat, ln):
    m = pat.search(ln)
    return int(m.group(1)) if m else None


def _f(pat, ln):
    m = pat.search(ln)
    return float(m.group(1)) if m else None


def diag_ticks(cli):
    """[(t, dict)] of the sender's [DIAG] lines; fields absent from a line are None."""
    out = []
    for ln in cli or []:
        if not ln.startswith("[DIAG]"):
            continue
        t = _f(_T, ln)
        if t is None:
            continue
        cum = _CUM.search(ln)
        fp = _FPON.search(ln)
        g = [int(x) if x else 0 for x in fp.groups()] if fp else None
        out.append((t, {"src": _i(_SRC, ln), "win": _i(_WIN, ln), "retx": _i(_RETX, ln), "rep": _i(_REP, ln), "seqs": _i(_SEQS, ln),
                        "rtt": _f(_RTT, ln), "cum": tuple(int(x) for x in cum.groups()) if cum else None, "fpon": g}))
    return out


def parse_cx_leg(tok):
    """`0:3,1:2` -> {0: 3, 1: 2}; `-` -> {} (no retransmit closed a hole); None (no such field in this binary) -> None."""
    if tok is None:
        return None
    if tok == "-":
        return {}
    out = {}
    for part in tok.split(","):
        a, _, b = part.partition(":")
        if not (a.isdigit() and b.isdigit()):
            return None
        out[int(a)] = int(b)
    return out


def hold_by_path(cli):
    """{original path: {HOLD_FIELDS..., cx_leg}} from the LAST sender [HOLD] line of each path (the end-of-run histograms); path `-` (the pooled line) is skipped."""
    out = {}
    for ln in cli or []:
        if not ln.startswith("[HOLD]"):
            continue
        m = _HOLD_PATH.search(ln)
        if not m:
            continue
        d = {}
        for k in HOLD_FIELDS:
            mm = re.search(r" " + k + r"=(\d+)", ln)
            d[k] = int(mm.group(1)) if mm else None
        mx = _CXLEG.search(ln)
        d["cx_leg"] = parse_cx_leg(mx.group(1) if mx else None)
        out[int(m.group(1))] = d
    return out


def summary_json(lines):
    for ln in reversed(lines or []):
        s = ln.strip()
        if s.startswith("{") and '"summary"' in s:
            try:
                return json.loads(s)
            except ValueError:
                continue
    return {}


def k_bins(ticks, tse):
    """{bin index: {K, Nh, mu, C_s}} over the main phase (price.py / a2_little.py): lines with src > 0 in each 4 s bin, at least two."""
    out = {}
    for b in range(MAX_BINS):
        idx = [i for i, (t, d) in enumerate(ticks) if BIN_S * b <= t < BIN_S * (b + 1) and (d["src"] or 0) > 0 and t <= tse]
        if len(idx) < 2:
            continue
        (t0, a), (t1, z) = ticks[idx[0]], ticks[idx[-1]]
        dt = t1 - t0
        if dt <= 0 or any(a[k] is None or z[k] is None for k in ("retx", "rep", "seqs")):
            continue
        rtts = [ticks[i][1]["rtt"] for i in idx if ticks[i][1]["rtt"] is not None]
        mu = (z["retx"] - a["retx"]) / dt
        c_s = st.median(rtts) / 1e3 if rtts else None
        drep = z["rep"] - a["rep"]
        out[b] = {"mu": mu, "C_s": c_s, "K": mu * c_s if c_s is not None else None,
                  "Nh": (z["seqs"] - a["seqs"]) / drep if drep > 0 else None}
    return out


def row_metrics(cli, srv, drv, nbytes=None):
    """One row's readouts (see the module doc); never raises on a missing field. `nbytes` is the transfer size (default 600 MB)."""
    nbytes = nbytes or DEFAULT_BYTES
    ticks = diag_ticks(cli)
    summ = summary_json(drv)
    secs = summ.get("median_s")
    m = {"bytes": nbytes, "secs": secs, "goodput_mbps": summ.get("mean_mbps"), "n_diag": len(ticks)}
    ise = None
    for i, (t, d) in enumerate(ticks):
        if (d["src"] or 0) > 0 and t >= WARM_S:
            ise = i
    m.update({"t_src_end": None, "tail_s": None, "H0": None, "lag_end": None, "retx_total": None, "retx_per_gb": None, "send_leg1_share": None,
              "send_on": None, "kbins": {}})
    if ticks:
        last = ticks[-1][1]
        m["retx_total"] = last["retx"]
        m["retx_per_gb"] = last["retx"] / (nbytes / GB) if last["retx"] is not None else None
        if last["fpon"]:
            on0, on1 = last["fpon"][1], last["fpon"][3]
            m["send_on"] = [on0, on1]
            m["send_leg1_share"] = on1 / (on0 + on1) if on0 + on1 else None
    if ise is not None:
        tse, d = ticks[ise]
        m["t_src_end"] = tse
        if secs is not None:
            m["tail_s"] = secs - tse
            tl = [i for i in range(ise + 1, len(ticks)) if ticks[i][0] <= secs]
            m["H0"] = ticks[tl[0]][1]["win"] if tl else None
        m["lag_end"] = d["cum"][0] - d["cum"][2] if d["cum"] else None
        m["kbins"] = k_bins(ticks, tse)
    m["hold"] = hold_by_path(cli)
    dups = [ln for ln in srv or [] if ln.startswith("[C8CONV-R]")]
    m["rx_dup"] = sum(int(x) for x in _DUP.findall(dups[-1])) if dups else None
    cpu = [mm for mm in (_CPU.search(ln) for ln in drv or []) if mm]
    if cpu:
        m["cpu_srv_s_per_gb"] = float(cpu[-1].group(1)) / (nbytes / GB)
        m["cpu_cli_s_per_gb"] = float(cpu[-1].group(2)) / (nbytes / GB)
    else:
        m["cpu_srv_s_per_gb"] = m["cpu_cli_s_per_gb"] = None
    return m


# ── aggregation ──
def _vals(rows, key):
    return [r[key] for r in rows if r.get(key) is not None]


def _fmt(x, nd=2):
    return "-" if x is None else f"{x:.{nd}f}"


def stat(rows, key):
    v = _vals(rows, key)
    if not v:
        return None
    return {"n": len(v), "mean": st.mean(v), "median": st.median(v), "min": min(v), "max": max(v)}


def _hold(r, path):
    """The row's [HOLD] dict of original `path` (the keys are ints in memory and strings after a JSON round trip through the ledger)."""
    h = r.get("hold") or {}
    return h.get(path) or h.get(str(path)) or {}


def hold_stat(rows, path, field):
    v = [_hold(r, path)[field] for r in rows if _hold(r, path).get(field) is not None]
    return st.median(v) if v else None


def cx_leg_stat(rows, path):
    """-> (pooled {leg: n}, median per-row share of the closers sent on leg 1, rows with the field) for original path `path`."""
    pooled, shares, n = {}, [], 0
    for r in rows:
        d = _hold(r, path).get("cx_leg")
        if d is None:
            continue
        d = {int(k): v for k, v in d.items()}
        n += 1
        for leg, c in d.items():
            pooled[leg] = pooled.get(leg, 0) + c
        tot = sum(d.values())
        if tot:
            shares.append(d.get(1, 0) / tot)
    return pooled, (st.median(shares) if shares else None), n


def kbin_stat(rows, key):
    """{bin: mean over rows} of the K-bin field `key`; K itself is mean(mu) * mean(C) over the rows of the bin, as price.py prints it."""
    if key == "K":
        mu, c = kbin_stat(rows, "mu"), kbin_stat(rows, "C_s")
        return {b: mu[b] * c[b] for b in mu if b in c}
    acc = {}
    for r in rows:
        for b, d in (r.get("kbins") or {}).items():
            v = d.get(key)
            if v is not None:
                acc.setdefault(int(b), []).append(v)
    return {b: st.mean(v) for b, v in sorted(acc.items())}


def report_arm(arm, rows, out):
    n = len(rows)
    out(f"== {arm}: n = {n} rows")
    if not n:
        return
    for label, key, nd in (("goodput Mbit/s", "goodput_mbps", 1), ("completion tail s", "tail_s", 2), ("H0 holes at source end", "H0", 0),
                           ("frontier lag at source end (symbols)", "lag_end", 0), ("retx send-leg-1 share (fp/on, whole run)", "send_leg1_share", 3),
                           ("retransmits per GB", "retx_per_gb", 0), ("receiver duplicates", "rx_dup", 0),
                           ("client CPU s/GB", "cpu_cli_s_per_gb", 2), ("server CPU s/GB", "cpu_srv_s_per_gb", 2)):
        s = stat(rows, key)
        out(f"  {label:<42} " + ("-" if s is None else f"mean {_fmt(s['mean'], nd)}  median {_fmt(s['median'], nd)}  [{_fmt(s['min'], nd)} - {_fmt(s['max'], nd)}]  (n {s['n']})"))
    for p in (0, 1):
        ms = lambda f: _fmt(None if hold_stat(rows, p, f) is None else hold_stat(rows, p, f) / 1e3, 0)  # noqa: E731
        out(f"  [HOLD] original path {p} (median over rows, ms): first-report age p50/p90 {ms('age_p50_us')}/{ms('age_p90_us')}; "
            f"closed-by-retransmit p50/p90 {ms('cx_p50_us')}/{ms('cx_p90_us')}")
        pooled, share, nrow = cx_leg_stat(rows, p)
        out(f"    cx_leg (send leg of the closing retransmit): " + (f"pooled {dict(sorted(pooled.items()))}, median row share on leg 1 {_fmt(share, 3)} ({nrow} rows)"
                                                               if nrow else "- (not in this binary's [HOLD])"))
    kk, nh = kbin_stat(rows, "K"), kbin_stat(rows, "Nh")
    out("  K = mu * C per 4 s main-phase bin (mean over rows): " + (" ".join(f"{int(BIN_S * b)}-{int(BIN_S * (b + 1))}s:{kk[b]:.0f}" for b in kk) or "-"))
    out("  Nh = gap sequences visited per processed report, same bins: " + (" ".join(f"{int(BIN_S * b)}-{int(BIN_S * (b + 1))}s:{nh[b]:.1f}" for b in nh) or "-"))


def report(rows, out=print, arms=ARMS52):
    for arm in arms:
        report_arm(arm, [r for r in rows if r["arm"] == arm], out)
    out("")


# ── the ledger side ──
def score(paths, out=print):
    import place38_parse as pp
    rows_all, tokens = pp.rows_of(paths)
    aborted = sorted({t for t in tokens if t in pp.ABORT_TOKENS})
    rows = [r for r in rows_all if r["cell"] == pp.REPORT_CELL52 and r["arm"] in ARMS52]
    out("C8 RECOVERY LEVERS (status 52), REPORTED ONLY: c8-100 at 600 MB, arms BASE (BASE binary), CAND (CAND binary), W0 (BASE + RWM_PLACE_WDIV=0), "
        "W05 (BASE + RWM_PLACE_WDIV=0.5), WDER (BASE + RWM_PLACE_WDIV_DERIVED=1); nothing here is scored")
    out(f"ROWS {len(rows)}/{pp.EXPECT_ROWS52}; ABORT-TOKENS {' '.join(aborted) or 'none'}")
    out("STATUS-TABLE per arm (LIVE rows enter the readouts; a witness failure means the arm's env was not echoed as registered)")
    for arm in ARMS52:
        c = {}
        for r in rows:
            if r["arm"] == arm:
                c[r["status"]] = c.get(r["status"], 0) + 1
        out(f"  {arm:<5} " + (" ".join(f"{k}={v}" for k, v in sorted(c.items())) or "no rows"))
    for r in rows:
        if r["status"] != "LIVE":
            out(f"  NONLIVE {r['arm']} s{r['seed']} rep{r['rep']} {r['status']} rc={r['rc']} {' '.join(r['problems'])}")
    out("WITNESS (the arm's env in the [GATES] echo): " + "; ".join(
        f"{a} wdiv={sorted({str(r.get('gate_wdiv_cli')) + '/' + str(r.get('gate_wdiv_srv')) for r in rows if r['arm'] == a})} "
        f"derived={sorted({str(r.get('gate_wdiv_derived_cli')) + '/' + str(r.get('gate_wdiv_derived_srv')) for r in rows if r['arm'] == a})}" for a in ARMS52))
    live = [dict(r["t52"], arm=r["arm"]) for r in rows if r["status"] == "LIVE" and r.get("t52")]
    errs = [(r["arm"], r["seed"], r["rep"], r["t52_error"]) for r in rows if r.get("t52_error")]
    for e in errs:
        out(f"  T52-ERROR {e}")
    out("")
    report(live, out)
    out(f"VERDICT52 REPORTED-ONLY rows={len(rows)}/{pp.EXPECT_ROWS52} live={len(live)}")


def rows_from_dir(d, cell="c8-100", arms=ARMS52):
    """Raw-log rows: every `<cell>@def-<ARM>-s<seed>-r<rep>-a<n>-c.log` of `d` with its -s.log / -drv.out -> metric dicts (arm, seed, rep set)."""
    out = []
    for f in sorted(os.listdir(d)):
        m = re.match(re.escape(cell) + r"@\w+-(\w+)-s(\d+)-r(\d+)-a\d+-c\.log$", f)
        if not m or m.group(1) not in arms:
            continue
        base = os.path.join(d, f[:-len("-c.log")])
        r = row_metrics(lc.read(os.path.join(d, f)), lc.read(base + "-s.log"), lc.read(base + "-drv.out"))
        r.update(arm=m.group(1), seed=m.group(2), rep=int(m.group(3)))
        out.append(r)
    return out


def main(argv):
    if not argv:
        print(__doc__)
        return 2
    cmd, a = argv[0], argv[1:]
    if cmd == "score":
        score(a)
        return 0
    if cmd == "dry":
        arms = tuple(a[1:]) or ARMS52
        report(rows_from_dir(a[0], arms=arms), arms=arms)
        return 0
    if cmd == "plan":
        import place38_parse as pp
        for c, s_, ar, seed, rep in pp.plan52(smoke=bool(a) and a[0] == "smoke"):
            print(f"{c} {s_} {ar} {seed} {rep}")
        return 0
    print(f"unknown command {cmd}")
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
