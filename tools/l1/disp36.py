#!/usr/bin/env python3
"""Capture scorer for docs/status.md §36 (+ Amendments 1 and 2): receiver-side dispersion
as the store law's link-rate source. Pure stdlib, no scapy.

Per row (called from loopdiag_parse.make_row under LD_PLAN=36):
  * a minimal pcap reader (nanosecond magic 0xa1b23c4d, little or big endian, Ethernet);
  * per leg the skb series (t_ns, b) of EVERY skb the unfiltered inbound tap saw (any protocol):
    b = orig_len + (segs-1)*42, segs = ceil(udp_payload / m), m = the payload carrying the most bytes
    (count x payload) among UDP skbs <= 1472 B. The engine's gso_size varies per skb (1246..1266), so
    one m over-counts segments by ~4 %: netem's byte count differs by exactly 42 B per phantom segment
    (0.15 %), inside the 1 % witness;
  * the estimator of §36 (1 s warm-up, train rate r_k(j) = sum b[j+1..j+k] / (t[j+k]-t[j]),
    250 ms windows, per-window max, per-row median and cv; the window-p90 variant), k in 2/4/8/16;
  * D3 (in-run share, busy-run lengths), the witnesses over the capture window's counter DELTAS
    ([SNAP] start/stop: bytes vs netem Sent bytes; skbs vs qdisc hits - drops - queued; kernel drops; the
    idle leg), D6 (quinn window,
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
    "c1s-400": [16, 32, 64, 128, 256, 512, 768, 1024, 2048],
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
    """-> (records [(t_ns, orig_len, udp_payload | None)], stats). EVERY captured skb is a record, of any
    protocol (netem's rate server spaces them all); the UDP payload is None for anything that is not IPv4/UDP.
    Only the nanosecond magic is accepted: a microsecond file (tcpdump falling back silently) would mis-scale
    every spacing, so it is refused."""
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
    off, recs, other, trunc, other_bytes = 24, [], 0, 0, 0
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
        pay = None
        if incl >= 42 and pkt[12:14] == b"\x08\x00":
            ihl = (pkt[14] & 0x0F) * 4
            if pkt[23] == 17 and ihl >= 20 and incl >= 14 + ihl + 8:
                pay = struct.unpack_from(">H", pkt, 14 + ihl + 4)[0] - 8
        if pay is None:
            other += 1
            other_bytes += orig
        recs.append((sec * NS + nsec, orig, pay))
    return recs, {"other": other, "other_bytes": other_bytes, "truncated": trunc}


def full_segment_payload(payloads):
    """The payload that carries the most bytes (count x payload) among UDP skbs <= 1472 B (a tie goes to the
    larger); None without any. Not the most frequent one: small client datagrams (36-41 B) outnumber the
    single-segment data skbs (status §36 amendment 5)."""
    c = Counter(p for p in payloads if p is not None and 0 < p <= M_MAX_PAYLOAD)
    if not c:
        return None
    return max(c, key=lambda p: (p * c[p], p))


def skb_series(recs, m):
    """[(t_ns, b)] with b = orig_len + (segs-1)*42, segs = ceil(payload/m)."""
    out = []
    for t, orig, pay in recs:
        segs = max(1, -(-pay // m)) if (m and pay) else 1
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
    """{k: {lhat, cv, p90, wmed, nwin, zero_dt}} (bytes/s). Post-warm-up = after the first 1 s of the leg.
    lhat (the per-window max) is the scored statistic; p90 and wmed (the window median, Amendment 3; its mode was
    dropped by the erratum) are reported beside it. Each is the per-row median over windows."""
    res = {k: {"lhat": None, "cv": None, "p90": None, "wmed": None, "nwin": 0, "zero_dt": 0} for k in ks}
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
        wm = [_median(wins[w]) for w in keep]
        d = res[k]
        d["zero_dt"], d["nwin"] = zero, len(keep)
        if mx:
            d["lhat"], d["p90"] = _median(mx), _median(p9)
            d["wmed"] = _median(wm)
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
_WIN = re.compile(r"^\[DIAG\] t=[\d.]+s win=(\d+)/(\d+) ")
_PATH = re.compile(r"\bp(\d+):infl=(\d+)/.*?rtt=(\d+)/wrtt=(\d+)/rtp\d+ms rtp_us=(\d+).*?qcwnd=(\d+)")
_DGQ = re.compile(r"dgq(\d+)\[hand=(\d+) tx=(\d+)")
_RDIAG = re.compile(r"\[RDIAG\] busy=(\d+)% msgs=(\d+)/s")
W_WIRE36 = 1279.0             # wire bytes per datagram (loopdiag_parse.W_WIRE; a test pins the equality)
RDIAG_BUSY = 20.0
RDIAG_PERIOD_NS = 500_000_000


def parse_qdisc(drv):
    out = {}
    for ln in drv:
        m = _QDISC.search(ln)
        if m:
            out[int(m.group(1))] = {"sent_bytes": int(m.group(2)), "sent_pkts": int(m.group(3)), "dropped": int(m.group(4))}
    return out


_SNAP = re.compile(r"\[SNAP\] tag=(\w+) leg=(\d+) dev=(\S+) qdisc: (.*?) filter: (.*)$")
_QSENT = re.compile(r"Sent (\d+) bytes (\d+) pkt \(dropped (\d+)")
_QBACK = re.compile(r"backlog (\d+)b (\d+)p")
_HIT = re.compile(r"rule hit (\d+)")


def parse_snap(drv):
    """{leg: {"start"|"stop": {sent_bytes, sent_pkts, dropped, backlog_pkts, hit}}} from the [SNAP] lines that
    perf_rwm_c.sh prints when the taps start and (after netem's queue drained) stop. The first `Sent`/`backlog`
    of the qdisc text is netem's; `rule hit` is the clsact egress counter in front of it (skbs)."""
    out = {}
    for ln in drv:
        m = _SNAP.search(ln)
        if not m:
            continue
        q, bl, hit = _QSENT.search(m.group(4)), _QBACK.search(m.group(4)), _HIT.search(m.group(5))
        if not (q and bl and hit):
            continue
        out.setdefault(int(m.group(2)), {})[m.group(1)] = {
            "sent_bytes": int(q.group(1)), "sent_pkts": int(q.group(2)), "dropped": int(q.group(3)),
            "backlog_pkts": int(bl.group(2)), "hit": int(hit.group(1))}
    return out


def window_expect(snap):
    """(bytes, skbs) netem released over the capture window: delta Sent bytes, and delta (skbs entering the
    qdisc - skbs dropped - skbs still queued). None when either snapshot is missing."""
    a, b = (snap or {}).get("start"), (snap or {}).get("stop")
    if not a or not b:
        return None
    return (b["sent_bytes"] - a["sent_bytes"],
            (b["hit"] - a["hit"]) - (b["dropped"] - a["dropped"]) - (b["backlog_pkts"] - a["backlog_pkts"]))


def parse_capture(drv):
    out = {}
    for ln in drv:
        m = _CAPTURE.search(ln)
        if m:
            out[int(m.group(1))] = {"dev": m.group(2), "captured": lc.inum(m.group(3)), "kdrop": lc.inum(m.group(4))}
    return out


def diag_d6(cli, nleg, warm_s=1.0):
    """D6/D8 inputs from the [DIAG] lines of main's binary (src/net/diag.rs). Per leg, post-warm-up medians of:
    qcwnd (quinn's window, bytes), B = hand - tx (datagrams), dwell = rtt - wrtt (ms; both whole-ms in the line:
    the app-echo RTT minus quinn's); and the LAST line's rtt, wrtt, rtp_us, infl (D8). Row level: the median of
    the line's own `cwnd=` (the scheduler's cwnd summed over ALL registered paths -- not quinn's window; kept in
    the row only) and R-hat = win numerator - sum qcwnd - B over the lines with total B > 0 (Amendment 3 + erratum).
    All per-leg fields come from the `p<i>:` block of src/net/diag.rs."""
    keys = ("qcwnd", "B", "dwell")
    ser = {i: {k: [] for k in keys} for i in range(nleg)}
    last = {i: None for i in range(nleg)}
    ndiag = {i: 0 for i in range(nleg)}
    gc, rh = [], []
    for ln in cli:
        m = _DIAG_T.match(ln)
        if not m or float(m.group(1)) < warm_s:
            continue
        g = _GCWND.search(ln)
        cwnd = int(g.group(1)) if g else None
        if cwnd is not None:
            gc.append(cwnd)
        btot = 0
        for pid, h, x in _DGQ.findall(ln):
            b = int(h) - int(x)
            btot += b
            if int(pid) in ser:
                ser[int(pid)]["B"].append(b)
        paths = _PATH.findall(ln)
        w = _WIN.match(ln)
        if w and paths and btot > 0:
            # R-hat = win numerator - sum_p qcwnd_p - B (qcwnd in bytes -> datagrams of W bytes, win is in symbols)
            rh.append(int(w.group(1)) - sum(int(x[5]) for x in paths) / W_WIRE36 - btot)
        for pid, infl, rtt, wrtt, rtp, qc in paths:
            p = int(pid)
            if p not in ser:
                continue
            ndiag[p] += 1
            ser[p]["qcwnd"].append(int(qc))
            ser[p]["dwell"].append(int(rtt) - int(wrtt))
            last[p] = {"t": float(m.group(1)), "rtt": int(rtt), "wrtt": int(wrtt), "rtp_us": int(rtp), "infl": int(infl)}
    legs = [{"qcwnd_med": _median(ser[i]["qcwnd"]), "B_med": _median(ser[i]["B"]), "dwell_med": _median(ser[i]["dwell"]),
             "last": last[i], "diag_n": ndiag[i], "dgq_n": len(ser[i]["B"])} for i in range(nleg)]
    return legs, {"cwnd_med": _median(gc), "rhat_med": _median(rh), "rhat_n": len(rh)}


def rdiag_mu(srv, skbs_by_leg):
    """mu_R = msgs/s / (busy/100) per server [RDIAG] line with busy >= 20 % (plan 34's rule), tagged with the
    number of active legs in that line's 500 ms period. The lines carry no stamps: period k is taken to start
    at the first captured packet of any leg (approximate: the receiver loop starts earlier), and a leg is active
    in a period when it carries >= 5 % of the period's skbs. -> [{k, mu, active}]"""
    lines = []
    for ln in srv:
        m = _RDIAG.search(ln)
        if m and float(m.group(1)) >= RDIAG_BUSY and int(m.group(2)) > 0:
            lines.append(int(m.group(2)) / (float(m.group(1)) / 100.0))
    allts = [t for sk in skbs_by_leg.values() for t, _ in sk]
    if not lines or not allts:
        return []
    t0 = min(allts)
    cnt = {}
    for i, sk in skbs_by_leg.items():
        for t, _ in sk:
            cnt.setdefault((t - t0) // RDIAG_PERIOD_NS, {}).setdefault(i, 0)
            cnt[(t - t0) // RDIAG_PERIOD_NS][i] += 1
    out = []
    # every [RDIAG] line (printed or not) is one period; busy < 20 % lines are dropped after indexing
    idx = 0
    for ln in srv:
        m = _RDIAG.search(ln)
        if not m:
            continue
        k = idx
        idx += 1
        if float(m.group(1)) < RDIAG_BUSY or int(m.group(2)) <= 0:
            continue
        c = cnt.get(k, {})
        tot = sum(c.values())
        act = sum(1 for v in c.values() if tot and v / tot >= 0.05)
        out.append({"k": k, "mu": int(m.group(2)) / (float(m.group(1)) / 100.0), "active": act})
    return out


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
def row_fields(cell, arm, drv, cli, pcap_dir=None, table_dir=None, row_name=None, srv=None):
    """-> (d36 dict, problems [str], notes [str]). `drv`/`cli`/`srv` are line lists."""
    kind, _ = arm36(arm)
    nleg = len(LINKS36[cell])
    problems, notes = [], []
    cap = parse_capture(drv)
    d6, dx = diag_d6(cli, nleg)
    truth = lc.truth(drv)
    d = {"kind": kind, "m": None, "gcwnd_med": dx["cwnd_med"], "rhat_med": dx["rhat_med"], "rhat_n": dx["rhat_n"], "legs": []}
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
    snaps = parse_snap(drv)
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
    sk_by_leg = {}
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
        sk_by_leg[i] = skbs
        leg["skbs"], leg["bytes"] = len(skbs), sum(b for _, b in skbs)
        if len(skbs) < IDLE_SKBS:
            leg["state"] = "idle"
            notes.append(f"p{i}-idle")
            continue
        leg["state"] = "live"
        live += 1
        # The witnesses compare the capture window's DELTAS (the qdisc's lifetime counters also cover the
        # topology bring-up and warm-up), against the unfiltered inbound tap.
        w = window_expect(snaps.get(i))
        if w is None:
            problems.append(f"p36-no-snap-p{i}")
        else:
            exp_b, exp_s = w
            leg["window_bytes"], leg["window_skbs"] = exp_b, exp_s
            if exp_b <= 0 or abs(leg["bytes"] - exp_b) / exp_b > WITNESS_TOL:
                problems.append(f"p36-bytes-p{i}={leg['bytes']}/{exp_b}")
            if exp_s <= 0 or abs(leg["skbs"] - exp_s) / exp_s > WITNESS_TOL:
                problems.append(f"p36-skbs-p{i}={leg['skbs']}/{exp_s}")
        est = estimate(skbs)
        leg["k"] = {str(k): v for k, v in est.items()}
        leg["d3"] = d3(skbs, LINKS36[cell][i])
        leg["t_first_ns"], leg["t_last_ns"] = skbs[0][0], skbs[-1][0]
        if table_dir and row_name:
            write_table(os.path.join(table_dir, f"{row_name}.p{i}.gz"), skbs)
    if live == 0:
        problems.append("p36-every-leg-idle")
    if srv and cell == "c1d-400":
        d["mur"] = rdiag_mu(srv, sk_by_leg)
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


def d8_rows(rows, cell):
    """{n: [{leg, ref, rows, ratios}]}: for a leg noted idle or with egress share < 5 % in a live CAP row, its last
    [DIAG] rtt, wrtt, rtp_us and infl as ratios to the busiest leg's; averaged over the cap's such rows."""
    out = {}
    for n in GRID36[cell]:
        acc = {}
        for r in cap_rows(rows, cell, n):
            legs = (r.get("d36") or {}).get("legs") or []
            eg = [x.get("egress_dgrams") or 0 for x in legs]
            tot = sum(eg)
            if len(legs) < 2 or not tot:
                continue
            ref = max(range(len(legs)), key=lambda i: eg[i])
            for i, x in enumerate(legs):
                if i == ref or not (x.get("state") == "idle" or eg[i] / tot < 0.05):
                    continue
                a, b = x.get("last"), legs[ref].get("last")
                if not a or not b:
                    continue
                e = acc.setdefault((i, ref), {"rows": 0, "own": [], "rtt": [], "wrtt": [], "rtp_us": [], "infl": []})
                e["rows"] += 1
                e["own"].append(a["rtt"] / a["wrtt"] if a["wrtt"] else None)
                for k in ("rtt", "wrtt", "rtp_us", "infl"):
                    e[k].append(a[k] / b[k] if b[k] else None)
        out[str(n)] = [{"leg": i, "ref": ref, "rows": e["rows"], "own_rtt_over_wrtt": _mean(e["own"]),
                        **{k: _mean(e[k]) for k in ("rtt", "wrtt", "rtp_us", "infl")}} for (i, ref), e in sorted(acc.items())]
    return {k: v for k, v in out.items() if v}


def mu_by_active(rows, cell):
    """{active legs: {median, n}} of mu_R over the live CAP rows' RDIAG periods (c1d rows only)."""
    pool = {}
    for r in rows:
        if r["cell"] == cell and r["status"] == "LIVE" and r["arm"].startswith("CAP"):
            for e in (r.get("d36") or {}).get("mur") or []:
                pool.setdefault(str(e["active"]), []).append(e["mu"])
    return {k: {"median": _median(v), "n": len(v)} for k, v in pool.items()}


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
            out("    n      rows  Q(k=2)  Q(k=4)  Q(k=8)  Q(k=16) Q(4,p90) Q(4,wmed)  cv4 mean/max   in-run share  runs n/mean/med/p90")
            qwm = q_by_cap(rows, cell, leg, K_SCORED, "wmed")
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
                    + f" {fmt(qp.get(n), 3):>8} {fmt(qwm.get(n), 3):>9}  {fmt(c[0] if c else None, 3)}/{fmt(c[1] if c else None, 3)}"
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
                    lres["cv4"] = {str(n): list(v) for n, v in sorted(cv_by_cap(rows, cell, leg, k).items())}
                    lres["Q_wmed"] = {str(n): v for n, v in sorted(qwm.items())}
            clsp, _ = d1_class(qp, cell)
            lres["D1p90"] = clsp
            out(f"    D1 (k=4) {lres['D1']['4']} (median Q {fmt(lres.get('D1med'), 3)}); D2 (k=4) {lres['D2']['4']}"
                + (f" NOISY at caps {lres['D2noisy']}" if lres["D2noisy"] else "")
                + f";  other k: D1 {[(k, lres['D1'][str(k)]) for k in K_LIST if k != K_SCORED]}"
                f" D2 {[(k, lres['D2'][str(k)]) for k in K_LIST if k != K_SCORED]};  D1 p90-variant (k=4) {clsp}")
            cres["legs"][str(leg)] = lres
        # D6 (authority, dwell, R-hat), D7 (placement share), D8 (idle-leg prices), mu_R by active legs
        out("  D6/D7 per cap (means over live CAP rows). qcwnd in datagrams (W); B = quinn backlog (dgrams); dwell = rtt - wrtt (ms, whole-ms fields);")
        out("     R-hat = win numerator - sum qcwnd (as datagrams) - B over lines with B > 0; all per-leg fields from the p<i>: block of [DIAG]")
        out("    n      rows  " + "  ".join(f"p{p}: qcwnd     B  dwell idle" for p in range(nleg)) + "   R-hat   egress share p0..")
        d67 = {}
        for n in GRID36[cell]:
            rs = cap_rows(rows, cell, n)
            cells_ = []
            sh = []
            for p in range(nleg):
                lg = [((r.get("d36") or {}).get("legs") or [{}] * nleg)[p] for r in rs]
                qc = _mean([x.get("qcwnd_med") for x in lg])
                bb = _mean([x.get("B_med") for x in lg])
                dw = _mean([x.get("dwell_med") for x in lg])
                idle = sum(1 for x in lg if x.get("state") == "idle")
                cells_.append(f"{fmt(qc / ld.W_WIRE if qc is not None else None, 0):>6} {fmt(bb, 0):>6} {fmt(dw, 1):>5} {idle}/{len(lg)}")
                sh.append(_mean([x.get("egress_dgrams") for x in lg]))
                d67.setdefault(str(n), {})[f"p{p}"] = {"qcwnd_dgrams": None if qc is None else qc / ld.W_WIRE, "qcwnd_bytes": qc, "B": bb,
                                                        "dwell_ms": dw, "idle_rows": idle}
            tot = sum(x for x in sh if x)
            shares = [None if not tot or x is None else x / tot for x in sh]
            rh = _mean([(r.get("d36") or {}).get("rhat_med") for r in rs])
            out(f"    {n:<6} {len(rs):<4}  " + "  ".join(cells_) + f"   {fmt(rh, 0):>6}   " + " ".join(fmt(x, 3) for x in shares))
            d67[str(n)]["share"] = shares
            d67[str(n)]["rhat"] = rh
        cres["D6D7"] = d67
        if nleg > 1:
            d8 = d8_rows(rows, cell)
            out("  D8 idle-leg prices: legs noted idle or with egress share < 5 %, last [DIAG] window, ratio to the active leg"
                " (rtt >> wrtt on the leg itself = frozen dwell-inclusive srtt = lock-out; fresh prices with a low share = the placement law)")
            for n in GRID36[cell]:
                for e in d8.get(str(n), []):
                    out(f"    n={n} p{e['leg']} (rows {e['rows']}): own rtt/wrtt {fmt(e['own_rtt_over_wrtt'], 2)}; ratios to p{e['ref']}: "
                        f"rtt {fmt(e['rtt'], 2)} wrtt {fmt(e['wrtt'], 2)} rtp_us {fmt(e['rtp_us'], 2)} infl {fmt(e['infl'], 2)}")
            cres["D8"] = d8
            mu = mu_by_active(rows, cell)
            if mu:
                out("  mu_R (server msgs/s / busy) split by active legs (RDIAG, busy >= 20 %): "
                    + "; ".join(f"{k} active: median {fmt(v['median'], 0)} (n={v['n']})" for k, v in sorted(mu.items())))
                cres["muR_by_active"] = mu
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
