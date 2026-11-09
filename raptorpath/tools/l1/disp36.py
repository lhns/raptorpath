#!/usr/bin/env python3
"""Capture scorer for docs/status.md §36 (+ Amendments 1 and 2): receiver-side dispersion
as the store law's link-rate source. Pure stdlib, no scapy.

Per row (called from loopdiag_parse.make_row under LD_PLAN=36):
  * a minimal pcap reader (nanosecond magic 0xa1b23c4d, little or big endian, Ethernet);
  * per leg the skb series (t_ns, b): b = orig_len + (segs-1)*42, segs = ceil(udp_payload / m),
    m = the row's full-segment payload (most frequent payload among skbs <= 1472 B);
  * the estimator of §36 (1 s warm-up, train rate r_k(j) = sum b[j+1..j+k] / (t[j+k]-t[j]),
    250 ms windows, per-window max, per-row median and cv; the window-p90 variant), k in 2/4/8/16;
  * D3 (in-run share, busy-run lengths), the witnesses (bytes vs netem Sent bytes, skbs vs
    [TRUTH] egress_skbs - netem_dropped_skbs, kernel drops, the idle leg), D6 (quinn window,
    backlog B) and D7 (egress share per leg);
  * the kept per-skb tables (gzipped "dt_ns b" lines, first line the absolute t).
Per ledger: score_paths() -> D1, D2, D4, D5, D6, D7 and the verdict, ending in `RESULT {json}`.

Interpretations the spec leaves open (also listed in the harness report):
  window w = floor((t_j - t0)/250 ms), t0 = first skb of the leg + 1 s; a window with no train is
  skipped and the last window is dropped when it ends after the last skb; trains with
  t[j+k] == t[j] are skipped and counted; sd is the population sd; m ties go to the larger
  payload; per-cap cv_4 is the mean over the cap's live rows (max reported too).
"""
import gzip
import json
import math
import os
import re
import struct
import sys
from collections import Counter

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import l1common as lc  # noqa: E402

NS = 10 ** 9
WIN_NS = 250_000_000
WARM_NS = NS
K_LIST = (2, 4, 8, 16)
K_SCORED = 4
EXTRA_HDR = 42                # eth 14 + ip 20 + udp 8: what qdisc_pkt_len adds per extra GSO segment
M_MAX_PAYLOAD = 1472
IDLE_SKBS = 200
WITNESS_TOL = 0.01
D1_LO, D1_HI, D1_SPEAR = 0.85, 1.15, 0.8
D2_CV = 0.10
D4_DROP = 0.05
MAGIC_LE = b"\x4d\x3c\xb2\xa1"
MAGIC_BE = b"\xa1\xb2\x3c\x4d"
RUN_CAP = 64                  # busy-run lengths above this share one bucket

LINKS36 = {                   # per leg link bytes/s: c1 125e6, c2 12.5e6, c3 2.5e6
    "c1s-400": [125e6], "c2-100": [12.5e6], "c8-100": [12.5e6, 2.5e6], "c1d-400": [125e6, 125e6],
}
GRID36 = {
    "c1s-400": [16, 32, 64, 128, 256, 512, 1024, 2048],
    "c2-100": [16, 32, 64, 128, 256, 512],
    "c8-100": [16, 32, 64, 128, 256, 384, 512, 1024],
    "c1d-400": [128, 512, 2048],
}
NOCAP_C1S = [256, 1024, 2048]
SCORED36 = ["c1s-400", "c2-100", "c8-100"]            # decision cells (c1d is reported)
CELLS36 = ["c1s-400", "c2-100", "c8-100", "c1d-400"]  # block-0 order; block 1 reverses it
BLOCKS36 = [("42", 1), ("7", 1)]
REQUIRED36 = [("c1s-400", 0), ("c2-100", 0), ("c8-100", 0), ("c8-100", 1)]


class PcapError(Exception):
    pass


# ── plan table ────────────────────────────────────────────────────────────
def plan36(smoke=False):
    """[(cell, arm, seed, rep)] in run order. Blocks seed 42 then 7; block 0 ascending, block 1 descending;
    the cell order alternates (reversed on block 1); at c1s each NOCAP<n> row sits next to the CAP<n> row
    (CAP first on block 0, NOCAP first on block 1) so the capture-cost control shares its drift."""
    if smoke:
        return [("c1s-400", "CAP256", "42", 1), ("c8-100", "CAP128", "42", 1)]
    out = []
    for b, (seed, rep) in enumerate(BLOCKS36):
        for cell in (CELLS36 if b % 2 == 0 else CELLS36[::-1]):
            g = GRID36[cell] if b % 2 == 0 else GRID36[cell][::-1]
            for n in g:
                pair = [f"CAP{n}"]
                if cell == "c1s-400" and n in NOCAP_C1S:
                    pair = pair + [f"NOCAP{n}"] if b % 2 == 0 else [f"NOCAP{n}"] + pair
                out += [(cell, a, seed, rep) for a in pair]
    return out


EXPECT_ROWS36 = len(plan36())


def arm36(arm):
    """('CAP'|'NOCAP', n) or (None, None)."""
    m = re.fullmatch(r"(CAP|NOCAP)(\d+)", arm)
    return (m.group(1), int(m.group(2))) if m else (None, None)


# ── pcap ──────────────────────────────────────────────────────────────────
def read_pcap(path):
    """-> (records [(t_ns, orig_len, udp_payload)], stats). Only the nanosecond magic is accepted: a
    microsecond file (tcpdump falling back silently) would mis-scale every spacing, so it is refused."""
    with open(path, "rb") as fh:
        data = fh.read()
    if len(data) < 24:
        raise PcapError("short file")
    mg = data[:4]
    if mg == MAGIC_LE:
        end = "<"
    elif mg == MAGIC_BE:
        end = ">"
    else:
        raise PcapError(f"not a nanosecond pcap (magic {mg.hex()})")
    link = struct.unpack_from(end + "I", data, 20)[0]
    if link != 1:
        raise PcapError(f"link type {link} (want 1, Ethernet)")
    rec = struct.Struct(end + "IIII")
    off, recs, other, trunc = 24, [], 0, 0
    while off + 16 <= len(data):
        sec, nsec, incl, orig = rec.unpack_from(data, off)
        off += 16
        if nsec >= NS:
            raise PcapError("ts_nsec >= 1e9 (not a nanosecond file)")
        if off + incl > len(data):
            trunc += 1
            break
        pkt = data[off:off + incl]
        off += incl
        if incl < 42 or pkt[12:14] != b"\x08\x00":
            other += 1
            continue
        ihl = (pkt[14] & 0x0F) * 4
        if pkt[23] != 17 or ihl < 20 or incl < 14 + ihl + 8:
            other += 1
            continue
        ulen = struct.unpack_from(">H", pkt, 14 + ihl + 4)[0]
        recs.append((sec * NS + nsec, orig, ulen - 8))
    return recs, {"other": other, "truncated": trunc}


def full_segment_payload(payloads):
    """The most frequent payload among skbs <= 1472 B (a tie goes to the larger); None without any."""
    c = Counter(p for p in payloads if 0 < p <= M_MAX_PAYLOAD)
    if not c:
        return None
    top = max(c.values())
    return max(p for p, n in c.items() if n == top)


def skb_series(recs, m):
    """[(t_ns, b)] with b = orig_len + (segs-1)*42, segs = ceil(payload/m)."""
    out = []
    for t, orig, pay in recs:
        segs = max(1, -(-pay // m)) if m else 1
        out.append((t, orig + (segs - 1) * EXTRA_HDR))
    return out


# ── the estimator ─────────────────────────────────────────────────────────
def _pct(xs, p):
    xs = sorted(xs)
    h = (len(xs) - 1) * p
    lo = math.floor(h)
    hi = min(lo + 1, len(xs) - 1)
    return xs[lo] + (h - lo) * (xs[hi] - xs[lo])


def _median(xs):
    return _pct(xs, 0.5) if xs else None


def estimate(skbs, ks=K_LIST):
    """{k: {lhat, cv, p90, nwin, zero_dt}} (bytes/s). Post-warm-up = after the first 1 s of the leg."""
    res = {k: {"lhat": None, "cv": None, "p90": None, "nwin": 0, "zero_dt": 0} for k in ks}
    n = len(skbs)
    if n < 2:
        return res
    ts = [t for t, _ in skbs]
    pre = [0]
    for _, b in skbs:
        pre.append(pre[-1] + b)
    t0, tlast = ts[0] + WARM_NS, ts[-1]
    for k in ks:
        wins, zero = {}, 0
        for j in range(n - k):
            if ts[j] < t0:
                continue
            dt = ts[j + k] - ts[j]
            if dt <= 0:
                zero += 1
                continue
            wins.setdefault((ts[j] - t0) // WIN_NS, []).append((pre[j + k + 1] - pre[j + 1]) * NS / dt)
        keep = [w for w in sorted(wins) if t0 + (w + 1) * WIN_NS <= tlast]
        mx = [max(wins[w]) for w in keep]
        p9 = [_pct(wins[w], 0.9) for w in keep]
        d = res[k]
        d["zero_dt"], d["nwin"] = zero, len(keep)
        if mx:
            d["lhat"], d["p90"] = _median(mx), _median(p9)
            mean = sum(mx) / len(mx)
            sd = math.sqrt(sum((x - mean) ** 2 for x in mx) / len(mx))
            d["cv"] = sd / mean if mean > 0 else None
    return res


def d3(skbs, link):
    """In-run share = fraction of consecutive post-warm-up pairs whose spacing is within +-10 % of b/R
    (b of the later skb); runs = {length in skbs: count} over maximal chains of in-run pairs."""
    if len(skbs) < 2:
        return {"pairs": 0, "share": None, "runs": {}}
    t0 = skbs[0][0] + WARM_NS
    pairs = inrun = 0
    runs, cur = Counter(), 0
    for (ta, _), (tb, b) in zip(skbs, skbs[1:]):
        if tb < t0:
            continue
        exp = b * NS / link
        ok = abs((tb - ta) - exp) <= 0.1 * exp
        pairs += 1
        if ok:
            inrun += 1
            cur += 1
        elif cur:
            runs[min(cur + 1, RUN_CAP + 1)] += 1
            cur = 0
    if cur:
        runs[min(cur + 1, RUN_CAP + 1)] += 1
    return {"pairs": pairs, "share": inrun / pairs if pairs else None, "runs": {str(k): v for k, v in sorted(runs.items())}}


def run_stats(runs):
    """{length: count} -> (n runs, mean, median, p90) in skbs."""
    xs = [int(k) for k, v in runs.items() for _ in range(v)]
    if not xs:
        return 0, None, None, None
    return len(xs), sum(xs) / len(xs), _median(xs), _pct(xs, 0.9)


# ── driver-log readers ────────────────────────────────────────────────────
_QDISC = re.compile(r"QDISC cli(\d+): .*?Sent (\d+) bytes (\d+) pkt \(dropped (\d+)")
_CAPTURE = re.compile(r"\[CAPTURE\] leg=(\d+) dev=(\S+) captured=(\S+) kdrop=(\S+)")
_DIAG_T = re.compile(r"^\[DIAG\] t=([\d.]+)s ")
_GCWND = re.compile(r" cwnd=(\d+) infl=")
_QCW = re.compile(r"\bp(\d+):infl=.*?qcwnd=(\d+)")
_DGQ = re.compile(r"dgq(\d+)\[hand=(\d+) tx=(\d+)")


def parse_qdisc(drv):
    out = {}
    for ln in drv:
        m = _QDISC.search(ln)
        if m:
            out[int(m.group(1))] = {"sent_bytes": int(m.group(2)), "sent_pkts": int(m.group(3)), "dropped": int(m.group(4))}
    return out


def parse_capture(drv):
    out = {}
    for ln in drv:
        m = _CAPTURE.search(ln)
        if m:
            out[int(m.group(1))] = {"dev": m.group(2), "captured": lc.inum(m.group(3)), "kdrop": lc.inum(m.group(4))}
    return out


def diag_d6(cli, nleg, warm_s=1.0):
    """D6 per leg from post-warm-up [DIAG] lines: median quinn window qcwnd (bytes), median backlog
    B = hand - tx (datagrams); plus the median of the line's own `cwnd=` (store window, symbols)."""
    qc = {i: [] for i in range(nleg)}
    bb = {i: [] for i in range(nleg)}
    gc = []
    for ln in cli:
        m = _DIAG_T.match(ln)
        if not m or float(m.group(1)) < warm_s:
            continue
        g = _GCWND.search(ln)
        if g:
            gc.append(int(g.group(1)))
        for pid, v in _QCW.findall(ln):
            if int(pid) in qc:
                qc[int(pid)].append(int(v))
        for pid, h, x in _DGQ.findall(ln):
            if int(pid) in bb:
                bb[int(pid)].append(int(h) - int(x))
    return ([{"qcwnd_med": _median(qc[i]), "B_med": _median(bb[i]), "diag_n": len(qc[i]), "dgq_n": len(bb[i])}
             for i in range(nleg)], _median(gc))


def write_table(path, skbs):
    """gzipped lines `dt_ns b`; the first line is `t_abs_ns b`. mtime 0: the bytes depend on the data only."""
    os.makedirs(os.path.dirname(path), exist_ok=True)
    prev = None
    lines = []
    for t, b in skbs:
        lines.append(f"{t if prev is None else t - prev} {b}")
        prev = t
    with open(path, "wb") as raw, gzip.GzipFile(filename="", mode="wb", fileobj=raw, mtime=0) as gz:
        gz.write(("\n".join(lines) + ("\n" if lines else "")).encode())


def read_table(path):
    """Inverse of write_table -> [(t_ns, b)]."""
    out, t = [], None
    with gzip.open(path, "rt") as fh:
        for ln in fh:
            a, b = ln.split()
            t = int(a) if t is None else t + int(a)
            out.append((t, int(b)))
    return out


# ── one row ───────────────────────────────────────────────────────────────
def row_fields(cell, arm, drv, cli, pcap_dir=None, table_dir=None, row_name=None):
    """-> (d36 dict, problems [str], notes [str]). `drv`/`cli` are line lists."""
    kind, _ = arm36(arm)
    nleg = len(LINKS36[cell])
    problems, notes = [], []
    cap = parse_capture(drv)
    d6, gcwnd = diag_d6(cli, nleg)
    truth = lc.truth(drv)
    d = {"kind": kind, "m": None, "gcwnd_med": gcwnd, "legs": []}
    for i in range(nleg):
        t = truth.get(i, {})
        d["legs"].append({"i": i, "state": "none", **d6[i], "egress_dgrams": t.get("egress_dgrams")})
    if kind == "NOCAP":
        if cap or (pcap_dir and os.path.isdir(pcap_dir)):
            problems.append("p36-nocap-has-capture")
        return d, problems, notes
    if kind != "CAP":
        problems.append(f"p36-unknown-arm={arm}")
        return d, problems, notes
    qd = parse_qdisc(drv)
    recs, bad = {}, set()
    for i in range(nleg):
        path = os.path.join(pcap_dir or "", f"p{i}.pcap")
        try:
            recs[i], _ = read_pcap(path)
        except (OSError, PcapError) as e:
            bad.add(i)
            problems.append(f"p36-pcap-unreadable-p{i}")
            d["legs"][i]["error"] = str(e)
    m = full_segment_payload(p for i in recs for _, _, p in recs[i])
    d["m"] = m
    live = 0
    for i in range(nleg):
        leg = d["legs"][i]
        c = cap.get(i)
        if c is None:
            problems.append(f"p36-capture-line-missing-p{i}")
        elif c["kdrop"] != 0:
            problems.append(f"p36-kdrop-p{i}={c['kdrop']}")
        leg["captured"], leg["kdrop"] = (c or {}).get("captured"), (c or {}).get("kdrop")
        if i in bad:
            continue
        skbs = skb_series(recs[i], m)
        leg["skbs"], leg["bytes"] = len(skbs), sum(b for _, b in skbs)
        if len(skbs) < IDLE_SKBS:
            leg["state"] = "idle"
            notes.append(f"p{i}-idle")
            continue
        leg["state"] = "live"
        live += 1
        q = qd.get(i)
        if q is None:
            problems.append(f"p36-no-qdisc-p{i}")
        else:
            leg["sent_bytes"] = q["sent_bytes"]
            if q["sent_bytes"] <= 0 or abs(leg["bytes"] - q["sent_bytes"]) / q["sent_bytes"] > WITNESS_TOL:
                problems.append(f"p36-bytes-p{i}={leg['bytes']}/{q['sent_bytes']}")
        t = truth.get(i, {})
        if t.get("egress_skbs") is None or t.get("netem_dropped_skbs") is None:
            problems.append(f"p36-no-truth-p{i}")
        else:
            exp = t["egress_skbs"] - t["netem_dropped_skbs"]
            leg["exp_skbs"] = exp
            if exp <= 0 or abs(leg["skbs"] - exp) / exp > WITNESS_TOL:
                problems.append(f"p36-skbs-p{i}={leg['skbs']}/{exp}")
        est = estimate(skbs)
        leg["k"] = {str(k): v for k, v in est.items()}
        leg["d3"] = d3(skbs, LINKS36[cell][i])
        leg["t_first_ns"], leg["t_last_ns"] = skbs[0][0], skbs[-1][0]
        if table_dir and row_name:
            write_table(os.path.join(table_dir, f"{row_name}.p{i}.gz"), skbs)
    if live == 0:
        problems.append("p36-every-leg-idle")
    return d, problems, notes


# ── scoring a ledger ──────────────────────────────────────────────────────
def _mean(v):
    v = [x for x in v if x is not None]
    return sum(v) / len(v) if v else None


def fmt(x, nd=2):
    return "-" if x is None else f"{x:.{nd}f}"


def live_leg(r, i):
    legs = (r.get("d36") or {}).get("legs") or []
    return legs[i] if i < len(legs) and legs[i].get("state") == "live" else None


def cap_rows(rows, cell, n):
    return [r for r in rows if r["cell"] == cell and r["arm"] == f"CAP{n}" and r["status"] == "LIVE"]


def q_by_cap(rows, cell, leg, k, variant="lhat"):
    """{n: mean over live rows of L_hat_k / link}; caps without a reading are absent."""
    out = {}
    link = LINKS36[cell][leg]
    for n in GRID36[cell]:
        v = []
        for r in cap_rows(rows, cell, n):
            lg = live_leg(r, leg)
            x = ((lg or {}).get("k") or {}).get(str(k), {}).get(variant)
            if x is not None:
                v.append(x / link)
        if v:
            out[n] = sum(v) / len(v)
    return out


def cv_by_cap(rows, cell, leg, k):
    out = {}
    for n in GRID36[cell]:
        v = []
        for r in cap_rows(rows, cell, n):
            lg = live_leg(r, leg)
            x = ((lg or {}).get("k") or {}).get(str(k), {}).get("cv")
            if x is not None:
                v.append(x)
        if v:
            out[n] = (sum(v) / len(v), max(v))
    return out


def d1_class(qd, cell):
    """§36 D1 on {n: Q}. A leg missing a cap of its grid is UNSCOREABLE (never a vacuous FLAT)."""
    import loopdiag_parse as ld
    if not qd or any(n not in qd for n in GRID36[cell]):
        return "UNSCOREABLE", None
    cls, med, _ = ld.q1_class({0: qd}, lo=D1_LO, hi=D1_HI, spear=D1_SPEAR)
    return cls, med


def d2_class(cvd, cell):
    if not cvd or any(n not in cvd for n in GRID36[cell]):
        return "UNSCOREABLE", []
    noisy = [n for n in GRID36[cell] if cvd[n][0] > D2_CV]
    return ("NOISY" if noisy else "QUIET"), noisy


def interp(pts, x):
    """Linear interpolation on {n: v}; None outside the covered range (no extrapolation)."""
    xs = sorted(n for n, v in pts.items() if v is not None)
    if not xs or x < xs[0] or x > xs[-1]:
        return None
    for lo, hi in zip(xs, xs[1:] + [None]):
        if x == lo:
            return pts[lo]
        if hi is not None and lo < x < hi:
            return pts[lo] + (pts[hi] - pts[lo]) * (x - lo) / (hi - lo)
    return None


LEDGERS = {"c2-100": ("p27", "p27/ld.log"), "c8-100": ("p27", "p27/ld.log"),
           "c1s-400": ("p34", "p34/run/ld.log"), "c1d-400": ("p34", "p34/run/ld.log")}


def ledger_rtp_s(cell, base=None):
    """({leg: {n: (RTprop_us, S_us)}}, source, missing reason). §27's DEF family (c2, c8) and §34 (c1s, c1d)
    through loopdiag_parse's own extractors (rows_of, per_n27)."""
    import loopdiag_parse as ld
    base = base or os.environ.get("D36_LEDGERS") or os.path.join(HERE, "..", "..", "docs", "l1-raw", "loop-diag")
    tag, rel = LEDGERS[cell]
    path = os.path.join(base, rel)
    if not os.path.exists(path):
        return None, tag, f"ledger {rel} absent"
    pn = ld.per_n27(ld.rows_of([path]), cell, "DEF")
    out = {}
    for p in range(len(LINKS36[cell])):
        out[p] = {n: (pn[n]["paths"][p]["rtprop_us"], pn[n]["paths"][p]["S"]) for n in ld.GRID27[cell]
                  if pn[n]["paths"][p]["rtprop_us"] is not None and pn[n]["paths"][p]["S"] is not None}
    return out, tag, None


def ask_series(cell, rts, qmap):
    """ask(n) = 1.1 sum_p L_p (RTprop_p + S_p) over the session grid, RTprop and S interpolated in n from the
    ledger; L_p = link/W (oracle, qmap None) or Q(p,n) link/W. None where a input is missing/out of range."""
    import loopdiag_parse as ld
    ask = {}
    for n in GRID36[cell]:
        tot, ok = 0.0, True
        for p, link in enumerate(LINKS36[cell]):
            rt = interp({k: v[0] for k, v in rts[p].items()}, n)
            s = interp({k: v[1] for k, v in rts[p].items()}, n)
            q = 1.0 if qmap is None else qmap[p].get(n)
            if rt is None or s is None or q is None:
                ok = False
                break
            tot += q * link / ld.W_WIRE * (rt + s) / 1e6
        ask[n] = ld.Q_GAIN * tot if ok else None
    return ask


def goodput_curve(rows, cell):
    return {n: _mean([r["mbps"] for r in cap_rows(rows, cell, n) if r.get("mbps") is not None]) for n in GRID36[cell]}


def score_rows(rows, out=print, ledgers=None):
    import loopdiag_parse as ld
    out("DISPERSION DIAGNOSTIC PLAN 36 SCORE (receiver-side dispersion as the store law's link-rate source; nothing here ships)")
    st = Counter((r["cell"], r["status"]) for r in rows)
    out(f"ROWS {len(rows)}/{EXPECT_ROWS36} status counts {sorted(st.items())}")
    for r in rows:
        if r["status"] != "LIVE" or r.get("notes") or r.get("io_high"):
            out(f"  {r['cell']} {r['arm']} s{r['seed']} r{r['rep']}: {r['status']} problems={r.get('problems')} "
                f"notes={r.get('notes')} io_full10={r.get('io_full10')}")
    res = {"cells": {}}
    d1k, d2k = {}, {}      # (cell, leg) -> {k: class}
    for cell in CELLS36:
        nleg = len(LINKS36[cell])
        role = "scored" if cell in SCORED36 else "reported"
        out(f"CELL {cell} ({role}); links {LINKS36[cell]} B/s; caps {GRID36[cell]}")
        cres = {"legs": {}}
        for leg in range(nleg):
            out(f"  LEG p{leg}")
            out("    n      rows  Q(k=2)  Q(k=4)  Q(k=8)  Q(k=16) Q(k=4,p90)  cv4 mean/max   in-run share  runs n/mean/med/p90")
            qs = {k: q_by_cap(rows, cell, leg, k) for k in K_LIST}
            qp = q_by_cap(rows, cell, leg, K_SCORED, "p90")
            cv = cv_by_cap(rows, cell, leg, K_SCORED)
            for n in GRID36[cell]:
                lv = [live_leg(r, leg) for r in cap_rows(rows, cell, n)]
                lv = [x for x in lv if x]
                shr = _mean([x["d3"]["share"] for x in lv])
                runs = Counter()
                for x in lv:
                    for kk, vv in x["d3"]["runs"].items():
                        runs[kk] += vv
                rn, rm, rmed, rp9 = run_stats(runs)
                c = cv.get(n)
                out(f"    {n:<6} {len(lv):<4} " + " ".join(f"{fmt(qs[k].get(n), 3):>7}" for k in K_LIST)
                    + f" {fmt(qp.get(n), 3):>9}  {fmt(c[0] if c else None, 3)}/{fmt(c[1] if c else None, 3)}"
                    + f"   {fmt(shr, 3):>8}   {rn}/{fmt(rm, 1)}/{fmt(rmed, 1)}/{fmt(rp9, 1)}")
            lres = {"D1": {}, "D2": {}}
            for k in K_LIST:
                cls, med = d1_class(qs[k], cell)
                c2, noisy = d2_class(cv_by_cap(rows, cell, leg, k), cell)
                lres["D1"][str(k)], lres["D2"][str(k)] = cls, c2
                d1k.setdefault((cell, leg), {})[k], d2k.setdefault((cell, leg), {})[k] = cls, c2
                if k == K_SCORED:
                    lres["D1med"] = med
                    lres["D2noisy"] = noisy
                    lres["Q"] = {str(n): v for n, v in sorted(qs[k].items())}
            clsp, _ = d1_class(qp, cell)
            lres["D1p90"] = clsp
            out(f"    D1 (k=4) {lres['D1']['4']} (median Q {fmt(lres.get('D1med'), 3)}); D2 (k=4) {lres['D2']['4']}"
                + (f" NOISY at caps {lres['D2noisy']}" if lres["D2noisy"] else "")
                + f";  other k: D1 {[(k, lres['D1'][str(k)]) for k in K_LIST if k != K_SCORED]}"
                f" D2 {[(k, lres['D2'][str(k)]) for k in K_LIST if k != K_SCORED]};  D1 p90-variant (k=4) {clsp}")
            cres["legs"][str(leg)] = lres
        # D6 / D7
        out("  D6 authority and D7 placement per cap (means over live CAP rows)")
        out("    n      rows  " + "  ".join(f"p{p}: qcwnd B/dgram(W)   B     state" for p in range(nleg)) + "   store cwnd=  egress share p0..")
        d67 = {}
        for n in GRID36[cell]:
            rs = cap_rows(rows, cell, n)
            cells_ = []
            sh = []
            for p in range(nleg):
                lg = [((r.get("d36") or {}).get("legs") or [{}] * nleg)[p] for r in rs]
                qc = _mean([x.get("qcwnd_med") for x in lg])
                bb = _mean([x.get("B_med") for x in lg])
                idle = sum(1 for x in lg if x.get("state") == "idle")
                cells_.append(f"{fmt(qc / ld.W_WIRE if qc is not None else None, 0):>6} {fmt(bb, 0):>6} idle={idle}/{len(lg)}")
                eg = [x.get("egress_dgrams") for x in lg]
                sh.append(_mean([e for e in eg]))
                d67.setdefault(str(n), {})[f"p{p}"] = {"qcwnd_dgrams": None if qc is None else qc / ld.W_WIRE, "B": bb, "idle_rows": idle}
            tot = sum(s for s in sh if s)
            shares = [None if not tot or s is None else s / tot for s in sh]
            gc = _mean([(r.get("d36") or {}).get("gcwnd_med") for r in rs])
            out(f"    {n:<6} {len(rs):<4}  " + "  ".join(cells_) + f"   {fmt(gc, 0):>8}   " + " ".join(fmt(s, 3) for s in shares))
            d67[str(n)]["share"] = shares
        cres["D6D7"] = d67
        # D5
        rts, tag, why = ledger_rtp_s(cell, ledgers)
        if rts is None:
            out(f"  D5 unavailable: {why}")
        else:
            qmap = {p: q_by_cap(rows, cell, p, K_SCORED) for p in range(nleg)}
            g = goodput_curve(rows, cell)
            gmax = max([v for v in g.values() if v is not None], default=None)
            d5 = {}
            for name, qm in (("oracle-L", None), ("L-hat", qmap)):
                ask = ask_series(cell, rts, qm)
                fp = ld.fixed_point27(ask)
                gfp = interp(g, fp["n_fp"]) if fp["n_fp"] is not None else None
                d5[name] = {"kind": fp["kind"], "n_fp": fp["n_fp"], "slope": fp["slope"], "G_fp": gfp,
                            "G_over_Gmax": gfp / gmax if gfp is not None and gmax else None}
                out(f"  D5 {name}: fixed point {fp['kind']} n_fp={fmt(fp['n_fp'], 1)} slope={fmt(fp['slope'], 3)} "
                    f"G(n_fp)={fmt(gfp, 1)} Mbit/s = {fmt(d5[name]['G_over_Gmax'], 3)} of Gmax {fmt(gmax, 1)};  "
                    f"ask(n) " + " ".join(f"{n}:{fmt(a, 0)}" for n, a in ask.items()))
            out(f"  D5 note: RTprop and S from the {tag} ledger (cross-session), interpolated in n over "
                f"{min(min(v) for v in rts.values() if v) if any(rts.values()) else '-'}.. with no extrapolation; "
                "caps outside it have no ask (FLOOR/CEILING then refers to the covered range)")
            cres["D5"] = d5
        res["cells"][cell] = cres
    # D4
    out("D4 capture cost, c1s goodput NOCAP - CAP (same session; a drop > 5 % is a covariate of c1s D1)")
    d4 = {}
    for n in NOCAP_C1S:
        gc = _mean([r["mbps"] for r in cap_rows(rows, "c1s-400", n) if r.get("mbps") is not None])
        gn = _mean([r["mbps"] for r in rows if r["cell"] == "c1s-400" and r["arm"] == f"NOCAP{n}"
                    and r["status"] == "LIVE" and r.get("mbps") is not None])
        drop = (gn - gc) / gn if gc is not None and gn else None
        cov = drop is not None and drop > D4_DROP
        d4[str(n)] = {"CAP": gc, "NOCAP": gn, "drop": drop, "covariate": cov}
        out(f"  n={n}: CAP {fmt(gc, 1)} NOCAP {fmt(gn, 1)} drop {fmt(None if drop is None else 100 * drop, 1)} %"
            + (" -> COVARIATE of c1s D1" if cov else ""))
    res["D4"] = d4
    # verdict
    failed = []
    for cell, leg in REQUIRED36:
        c1, c2 = d1k.get((cell, leg), {}), d2k.get((cell, leg), {})
        for clause, got, want in (("D1", c1.get(K_SCORED), "FLAT-AT-LINK"), ("D2", c2.get(K_SCORED), "QUIET")):
            if got != want:
                tab = c1 if clause == "D1" else c2
                ok_k = [k for k in K_LIST if k != K_SCORED and tab.get(k) == want]
                failed.append({"cell": cell, "leg": leg, "clause": clause, "got": got, "pass_at_k": ok_k})
    verdict = "NOT-YET" if failed else "DISPERSION-READS-LINK"
    out(f"VERDICT {verdict} (D1 FLAT-AT-LINK and D2 QUIET at k=4 on c1s, c2 and both legs of c8)")
    for f_ in failed:
        out(f"  failed {f_['cell']} p{f_['leg']} {f_['clause']}={f_['got']}; would pass at k={f_['pass_at_k'] or 'none of 2/8/16'} "
            "(any re-scoring at another k is a new registration)")
    res["verdict"], res["failed"] = verdict, failed
    out("RESULT " + json.dumps(res, sort_keys=True, default=str))
    return res


def score_paths(paths, out=print, ledgers=None):
    import loopdiag_parse as ld
    return score_rows(ld.rows_of(paths), out=out, ledgers=ledgers)


def main(argv):
    if len(argv) >= 2 and argv[1] == "plan36":
        for cell, arm, seed, rep in plan36(smoke=len(argv) > 2 and argv[2] == "smoke"):
            print(f"{cell} {arm} {seed} {rep}")
        return 0
    print(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
