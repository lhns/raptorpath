#!/usr/bin/env python3
"""Tests for disp36.py (status §36 + amendments 1-2). Run: python3 test_disp36.py

Pcaps are synthesized from netem's release rule t_i = max(t_{i-1}, a_i + d_i) + b_i/R, at a
realistic epoch (ns timestamps exceed 2**53, so a float path goes red). b/R is a whole number of
ns at all three links (8, 80, 400 ns per byte), so a busy run is asserted with ==.
"""
import gzip
import json
import math
import os
import random
import struct
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
os.environ.pop("LD_PLAN", None)
import l1common as lc  # noqa: E402
import disp36 as dp  # noqa: E402

fails = 0


def check(name, ok, detail=""):
    global fails
    print(("ok   " if ok else "FAIL ") + name + ((" -- " + str(detail)) if not ok else ""))
    if not ok:
        fails += 1


EPOCH = 1_700_000_000 * 10 ** 9
NSB = {125_000_000: 8, 12_500_000: 80, 2_500_000: 400}      # ns per byte at each link
M = 1200                                                    # full-segment payload of the synthetic engine
HDR = 42


def release(arrivals, sizes, R, delay=0):
    """netem's rule: t_i = max(t_{i-1}, a_i + d) + b_i/R, integer ns."""
    t, out = 0, []
    for a, b in zip(arrivals, sizes):
        t = max(t, a + delay) + b * NSB[R]
        out.append(t)
    return out


def pkt_of(t_ns, orig, payload):
    """(t, orig_len, udp_payload) -> an Ethernet/IPv4/UDP frame truncated to 64 B, as tcpdump -s 64 writes it."""
    eth = b"\x02" * 6 + b"\x04" * 6 + b"\x08\x00"
    ip = struct.pack(">BBHHHBBH4s4s", 0x45, 0, min(payload + 28, 65535), 0, 0, 64, 17, 0, bytes([10, 77, 0, 1]), bytes([10, 77, 0, 2]))
    udp = struct.pack(">HHHH", 40000, 7000, payload + 8, 0)
    return (eth + ip + udp + b"\x00" * 64)[:64], orig


def write_pcap(path, recs, end="<", magic=None, link=1):
    """recs [(t_ns, orig_len, payload)]"""
    mg = magic if magic is not None else (dp.MAGIC_LE if end == "<" else dp.MAGIC_BE)
    with open(path, "wb") as fh:
        fh.write(mg + struct.pack(end + "HHiIII", 2, 4, 0, 0, 64, link))
        for t, orig, pay in recs:
            data, o = pkt_of(t, orig, pay)
            fh.write(struct.pack(end + "IIII", t // dp.NS, t % dp.NS, len(data), o) + data)


def frame_len(payload):
    return 14 + 20 + 8 + payload


def skbs_of(sizes_segs, R, start=EPOCH, gap_after=None, arrivals=None, delay=0):
    """GSO-aware skbs. sizes_segs: segment counts per skb (payload M each, last full). Returns (recs, [(t,b)])."""
    recs, bs = [], []
    for s in sizes_segs:
        pay = s * M
        recs.append([0, frame_len(pay), pay])
        bs.append(frame_len(pay) + (s - 1) * HDR)
    return recs, bs


def busy_stream(R, span_ns=1_800_000_000, segs_cycle=(1, 6, 2, 11, 3), start=EPOCH):
    """A permanently backlogged netem for span_ns (so the post-warm-up part holds whole 250 ms windows):
    arrivals all at start. -> (recs, skbs)"""
    mean_b = sum(frame_len(s * M) + (s - 1) * HDR for s in segs_cycle) / len(segs_cycle)
    nsk = int(span_ns / (mean_b * NSB[R])) + 1
    segs = [segs_cycle[i % len(segs_cycle)] for i in range(nsk)]
    recs, bs = skbs_of(segs, R)
    ts = release([start] * nsk, bs, R)
    for r, t in zip(recs, ts):
        r[0] = t
    return [tuple(r) for r in recs], list(zip(ts, bs))


# ── the reader ─────────────────────────────────────────────────────────────
with tempfile.TemporaryDirectory() as td:
    recs = [(EPOCH + 5, frame_len(M), M), (EPOCH + 999_999_999 + 1, frame_len(3 * M), 3 * M)]
    for end in ("<", ">"):
        pth = os.path.join(td, "a.pcap")
        write_pcap(pth, recs, end)
        got, st = dp.read_pcap(pth)
        check(f"reader ({'LE' if end == '<' else 'BE'}): ns timestamps (epoch > 2**53), orig_len, UDP payload from uh->len",
              got == recs and EPOCH > 2 ** 53, got)
    write_pcap(os.path.join(td, "us.pcap"), recs, "<", magic=b"\xd4\xc3\xb2\xa1")
    try:
        dp.read_pcap(os.path.join(td, "us.pcap"))
        check("reader refuses a microsecond pcap (a silent tcpdump fallback would mis-scale every spacing)", False)
    except dp.PcapError:
        check("reader refuses a microsecond pcap (a silent tcpdump fallback would mis-scale every spacing)", True)
    write_pcap(os.path.join(td, "l.pcap"), recs, "<", link=113)
    try:
        dp.read_pcap(os.path.join(td, "l.pcap"))
        check("reader refuses a non-Ethernet link type", False)
    except dp.PcapError:
        check("reader refuses a non-Ethernet link type", True)

# ── segment / byte accounting ─────────────────────────────────────────────
check("m = the most frequent payload among skbs <= 1472 B (GSO totals and the odd tail ignored)",
      dp.full_segment_payload([1200] * 5 + [100] * 3 + [3600] * 50 + [2400] * 9) == 1200)
check("m tie goes to the larger payload", dp.full_segment_payload([1200, 1200, 800, 800]) == 1200)
check("m of nothing is None", dp.full_segment_payload([]) is None)
sk = dp.skb_series([(1, 1242, 1200), (2, frame_len(3600), 3600), (3, frame_len(100), 100), (4, frame_len(2500), 2500)], 1200)
check("b = orig_len + (segs-1)*42: 1 seg, 3 segs, a short single, a 3-seg skb with a short tail",
      [b for _, b in sk] == [1242, frame_len(3600) + 84, frame_len(100), frame_len(2500) + 84], sk)

# ── the estimator: absolute values ────────────────────────────────────────
for R in (125_000_000, 12_500_000, 2_500_000):
    recs_, skb = busy_stream(R)
    # mixed GSO sizes: an index shift (b_j..b_{j+k-1} instead of b_{j+1}..b_{j+k}) would read off R
    est = dp.estimate(skb)
    ok = all(est[k]["lhat"] == R and est[k]["cv"] == 0.0 and est[k]["p90"] == R for k in dp.K_LIST)
    check(f"busy run at R={R:,} B/s with mixed GSO sizes reads R exactly for k=2/4/8/16 (==)", ok,
          {k: est[k]["lhat"] for k in dp.K_LIST})
    check(f"... and its in-run share is 1.0 with one busy run of the whole post-warm-up length (R={R:,})",
          dp.d3(skb, R)["share"] == 1.0 and len(dp.d3(skb, R)["runs"]) == 1, dp.d3(skb, R))

# warm-up: the first second is not read
R = 125_000_000
recs_, skb = busy_stream(R)
first = skb[0][0]
warm = {i for i, (t, _) in enumerate(skb) if t < first + 10 ** 9}
comp = [(first + 100 * i if i in warm else t, b) for i, (t, b) in enumerate(skb)]    # the whole first second 100 ns apart
check("the first 1 s of the leg is excluded (a compressed warm-up does not move L-hat; the median would follow it if it counted)",
      dp.estimate(comp)[4]["lhat"] == R and dp.estimate(comp[len(warm):])[4]["lhat"] != R, dp.estimate(comp)[4]["lhat"])

# lateness J on the anchor: over-read bounded by x/(1-x), x = J/(k b/R)
b1, step = 10_000, 80_000                                   # 10 kB at 125 MB/s = 80 us
late = []
for i in range(40_000):
    t = EPOCH + i * step
    if i % 500 == 3:                                        # one late-stamped skb per ~40 ms window
        t += 32_000                                         # J = 32 us: x = J/(4*80us) = 0.1
    late.append((t, b1))
est = dp.estimate(late)
x = 32_000 / (4 * step)
check("timer lateness J on one skb per window: k=4 over-read = x/(1-x) = 11.1 % exactly, within the J/(k b/R) bound's 1/(1-x)",
      abs(est[4]["lhat"] / R - 1 / (1 - x)) < 1e-9 and x <= est[4]["lhat"] / R - 1 <= x / (1 - x) + 1e-12, est[4]["lhat"] / R)
x16 = 32_000 / (16 * step)
check("... and the over-read shrinks with k (k=16: x = 2.5 %)", abs(est[16]["lhat"] / R - 1 / (1 - x16)) < 1e-9, est[16]["lhat"] / R)
check("... lateness once only (a single late skb) leaves the per-row median at R",
      dp.estimate([(t + (32_000 if i == 20_000 else 0), b) for i, (t, b) in enumerate(late[:0] or [(EPOCH + i * step, b1) for i in range(40_000)])])[4]["lhat"] == R)

# a gap reads low; an all-sparse leg reads b / spacing
sparse = [(EPOCH + i * 1_000_000, 10_000) for i in range(6000)]          # 10 kB per ms = 10 MB/s = 0.08 R
est = dp.estimate(sparse)
check("a leg that is never busy reads its own spacing: 10 kB per 1 ms = 1e7 B/s = 0.08 of the link", est[4]["lhat"] == 1e7, est[4]["lhat"])
gapped = [(EPOCH + i * step, b1) for i in range(40_000)]
gapped = [(t + (7_000_000 if i >= 20_000 else 0), b) for i, (t, b) in enumerate(gapped)]
est = dp.estimate(gapped)
check("a 7 ms gap inside a busy leg: only the trains that straddle it read low; the window max stays R",
      est[4]["lhat"] == R and est[4]["cv"] < 0.02, (est[4]["lhat"], est[4]["cv"]))
check("windows with no train are skipped, not zero (a 1 s silence does not pull the median)",
      dp.estimate([(t + (10 ** 9 if i >= 20_000 else 0), b) for i, (t, b) in enumerate(gapped)])[4]["lhat"] == R)
dup = [(EPOCH + i * step, b1) for i in range(40_000)]
dup[20_001] = dup[20_002] = dup[20_000]
est2 = dp.estimate(dup)
check("trains with t[j+k] == t[j] are skipped and counted, not read as infinite", est2[2]["zero_dt"] >= 1 and math.isfinite(est2[2]["lhat"]), est2[2])

# the window p90 variant sits at or below the max, and differs when a window is mixed
rng = random.Random(7)
mixed, t = [], EPOCH
for i in range(30_000):
    t += step if rng.random() < 0.5 else 3 * step
    mixed.append((t, b1))
em = dp.estimate(mixed)
check("window-p90 variant <= the window max, strictly below on a stuttering stream", em[4]["p90"] < em[4]["lhat"], (em[4]["p90"], em[4]["lhat"]))

# a netem simulation with arrival gaps (idle then backlog): the busy stretches read R
rng = random.Random(3)
segs = [rng.choice((1, 2, 5, 9, 12)) for _ in range(20_000)]
_, bs = skbs_of(segs, 125_000_000)
arr, a = [], EPOCH
for i, b in enumerate(bs):
    a += rng.choice((0, 0, 0, 5_000, 400_000)) if i % 997 else 3_000_000       # mostly backlogged, occasional idle gaps
    arr.append(a)
ts = release(arr, bs, 125_000_000, delay=1_000_000)
est = dp.estimate(list(zip(ts, bs)))
check("simulated netem (GSO skbs, bursts, idle gaps): per-window max reads the link exactly in the backlogged windows",
      est[4]["lhat"] == 125_000_000, est[4]["lhat"])

# D3: in-run share and busy-run lengths, constructed
t, seq = EPOCH, []
for blk in range(1500):
    t += 2_000_000                                          # an idle gap, then 5 back-to-back 10 kB skbs
    for j in range(5):
        t += step if j else step + 500_000
        seq.append((t, b1))
d = dp.d3(seq, 125_000_000)
check("D3: runs of 5 skbs (4 in-run pairs) after every gap: share 4/5 of post-warm-up pairs, run length 5",
      abs(d["share"] - 0.8) < 0.01 and set(d["runs"]) == {"5"} and d["runs"]["5"] >= 100, d)
check("D3 run statistics: n, mean, median, p90 of the run lengths", dp.run_stats({"5": 10, "3": 10})[:3] == (20, 4.0, 4.0), dp.run_stats({"5": 10, "3": 10}))
check("D3: ±10 % edge (spacing 1.1 b/R in, 1.12 b/R out)",
      dp.d3([(EPOCH, b1), (EPOCH + 2 * 10 ** 9, b1), (EPOCH + 2 * 10 ** 9 + 88_000, b1), (EPOCH + 2 * 10 ** 9 + 88_000 + 89_600, b1)], R)["share"] == 1 / 3)

# ── tables ─────────────────────────────────────────────────────────────────
with tempfile.TemporaryDirectory() as td:
    pth = os.path.join(td, "x", "t.gz")
    ser = [(EPOCH + 17, 1242), (EPOCH + 17 + 9936, 8000), (EPOCH + 17 + 9936 + 1, 42)]
    dp.write_table(pth, ser)
    first = gzip.open(pth, "rt").read().split("\n")
    check("table: first line absolute t, then `dt_ns b`", first[:3] == [f"{EPOCH + 17} 1242", "9936 8000", "1 42"], first)
    check("table round-trips", dp.read_table(pth) == ser)
    a = open(pth, "rb").read()
    dp.write_table(pth, ser)
    check("table bytes are deterministic (mtime 0)", open(pth, "rb").read() == a)

# ── the row: witnesses ─────────────────────────────────────────────────────
def drv_lines(cell, legs, sent=None, skbs=None, kdrop=0, egress=None, drop=0):
    """legs: {i: [skb (t,b)]} -> a driver-output fragment in the shapes of perf_rwm_c.sh / lib.sh truth_compute."""
    out = []
    for i, ser in legs.items():
        sb = sum(b for _, b in ser) if sent is None else sent
        out.append(f"    QDISC cli{i}: qdisc netem 8039: root refcnt 11 limit 1000 delay 1ms rate 1Gbit seed 42  Sent {sb} bytes {len(ser) * 3} pkt "
                   f"(dropped {drop}, overlimits 0 requeues 0)   backlog 0b 0p requeues 0 ")
        eg = (len(ser) + drop) if egress is None else egress
        out.append(f"    [TRUTH] leg={i} dev=cli{i} egress_dgrams={eg * 3} egress_skbs={eg} gso=3.00 netem_sent_dgrams={len(ser) * 3} "
                   f"netem_dropped_skbs={drop} backlog=0 lost=0 loss=0.000000 rcvbuf_drops=0 rcvbuf_scope=netns")
        out.append(f"[CAPTURE] leg={i} dev=srv{i} captured={len(ser)} kdrop={kdrop}")
    return out


def row(cell, arm, legs, R, **kw):
    """Write the pcaps of `legs` ({i: skbs as (t, orig, payload)}) and run row_fields."""
    td = tempfile.mkdtemp()
    ser = {}
    for i, rc in legs.items():
        write_pcap(os.path.join(td, f"p{i}.pcap"), rc)
        ser[i] = dp.skb_series(rc, M)
    drv = drv_lines(cell, ser, **kw.pop("drvkw", {}))
    return dp.row_fields(cell, arm, drv, kw.pop("cli", []), td, kw.pop("tables", None), "row"), td


recs_, ser = busy_stream(12_500_000)
recs3, ser3 = busy_stream(2_500_000)
# busy_stream builds frame_len(s*M) with M payload per seg: same m the reader will infer
(d36, prob, notes), td = row("c2-100", "CAP256", {0: recs_}, 12_500_000)
check("clean row: no problems, leg live, m recovered from the capture", prob == [] and d36["legs"][0]["state"] == "live" and d36["m"] == M, (prob, d36["m"]))
check("clean row: k=4 L-hat == the link, exactly", d36["legs"][0]["k"]["4"]["lhat"] == 12_500_000, d36["legs"][0]["k"]["4"])
(d36, prob, _), _ = row("c2-100", "CAP256", {0: recs_}, 12_500_000, drvkw=dict(sent=int(sum(b for _, b in ser) * 1.02)))
check("witness trips: capture bytes 2 % below netem Sent bytes", any(p.startswith("p36-bytes-p0=") for p in prob), prob)
(d36, prob, _), _ = row("c2-100", "CAP256", {0: recs_}, 12_500_000, drvkw=dict(sent=int(sum(b for _, b in ser) / 1.0099)))
check("witness holds inside 1 % (0.99 % off)", prob == [], prob)
(d36, prob, _), _ = row("c2-100", "CAP256", {0: recs_}, 12_500_000, drvkw=dict(egress=int(len(ser) * 1.02)))
check("witness trips: captured skbs 2 % below [TRUTH] egress_skbs - netem_dropped_skbs (a tap that merged skbs)",
      any(p.startswith("p36-skbs-p0=") for p in prob), prob)
(d36, prob, _), _ = row("c2-100", "CAP256", {0: recs_}, 12_500_000, drvkw=dict(egress=len(ser) + 7, drop=7))
check("release unit nets the netem drops: egress - dropped == captured passes", prob == [], prob)
(d36, prob, _), _ = row("c2-100", "CAP256", {0: recs_}, 12_500_000, drvkw=dict(kdrop=3))
check("witness trips: tcpdump kernel drops > 0", "p36-kdrop-p0=3" in prob, prob)
(d36, prob, _), _ = row("c2-100", "CAP256", {0: recs_}, 12_500_000, drvkw=dict(kdrop="-"))
check("witness trips: no tcpdump summary (kdrop `-` is never read as 0)", any(p.startswith("p36-kdrop-p0") for p in prob), prob)
d_ = drv_lines("c2-100", {0: ser})
(d36, prob, _) = dp.row_fields("c2-100", "CAP256", [x for x in d_ if "[CAPTURE]" not in x], [], td, None, "r")
check("witness trips: [CAPTURE] line missing", "p36-capture-line-missing-p0" in prob, prob)
(d36, prob, _) = dp.row_fields("c2-100", "CAP256", [x for x in d_ if "QDISC" not in x], [], td, None, "r")
check("witness trips: netem QDISC line missing", "p36-no-qdisc-p0" in prob, prob)
(d36, prob, _) = dp.row_fields("c2-100", "CAP256", d_, [], os.path.join(td, "nope"), None, "r")
check("witness trips: pcap missing/unreadable; with every leg unreadable nothing is scored", "p36-pcap-unreadable-p0" in prob and d36["legs"][0]["state"] == "none", prob)
# idle leg: c1d with a 50-skb second leg
idle_recs = recs3[:50]
(d36, prob, notes), _ = row("c8-100", "CAP128", {0: recs_, 1: idle_recs}, 0)
check("a leg with < 200 skbs is noted p1-idle, not scored, and its witnesses are not applied (live c8 leg 0 has the full record)", notes == ["p1-idle"] and d36["legs"][1]["state"] == "idle"
      and "k" not in d36["legs"][1] and prob == [], (notes, prob))
(d36, prob, notes), _ = row("c8-100", "CAP128", {0: recs_[:50], 1: idle_recs}, 0)
check("the row is void only if every leg is idle", "p36-every-leg-idle" in prob and notes == ["p0-idle", "p1-idle"], (prob, notes))
(d36, prob, _), _ = row("c8-100", "CAP128", {0: recs_, 1: idle_recs}, 0, drvkw=dict(kdrop=1))
check("a kernel drop on an idle leg still voids the row", any(p.startswith("p36-kdrop") for p in prob), prob)
with tempfile.TemporaryDirectory() as td2:
    (d36, prob, _), _ = row("c2-100", "CAP256", {0: recs_}, 12_500_000, tables=td2)
    tb = dp.read_table(os.path.join(td2, "row.p0.gz"))
    check("kept table equals the skb series of the leg", tb == ser and len(tb) == len(ser), len(tb))
(d36, prob, _) = dp.row_fields("c2-100", "NOCAP256", [], [], None, None, "r")
check("NOCAP: no capture expected, no capture problems", prob == [] and d36["legs"][0]["state"] == "none", prob)
(d36, prob, _) = dp.row_fields("c2-100", "NOCAP256", d_, [], td, None, "r")
check("NOCAP with a capture line or pcap dir is a witness failure", "p36-nocap-has-capture" in prob, prob)

# ── D6 / D7 extraction ─────────────────────────────────────────────────────
def DG(t, h0, x0, h1, x1, q0=468342, q1=441144, g=1853):
    return (f"[DIAG] t={t}s win=768/1024 paused=19% good=842.6Mbit sidle=1080ms/211/mx15ms cwnd={g} infl=437 np=2 rtt=104.1ms "
            f"dgq0[hand={h0} tx={x0} full=0 err=0 sp=1] dgq1[hand={h1} tx={x1} full=0 err=0 sp=1] "
            f"p0:infl=437/sinfl=0/bdp1977(cap0) rtp_us=2842 qcwnd={q0} qce=16 qlp=360/251405 | ANCHOR sent=0 "
            f"p1:infl=0/sinfl=0/bdp106(cap0) rtp_us=2597 qcwnd={q1} qce=0 qlp=0/1186 | ANCHOR sent=0")
cl = [DG(0.5, 9999, 0, 9999, 0, q0=1), DG(1.0, 1000, 990, 50, 50), DG(1.25, 2000, 1970, 60, 55, q0=500000), DG(1.5, 3000, 2950, 70, 60, q0=520000)]
d6, gc = dp.diag_d6(cl, 2)
check("D6: post-warm-up median quinn window per leg (qcwnd, bytes)", d6[0]["qcwnd_med"] == 500000 and d6[1]["qcwnd_med"] == 441144, d6)
check("D6: backlog B = median of hand - tx per leg over post-warm-up lines (the 0.5 s line excluded)", d6[0]["B_med"] == 30 and d6[1]["B_med"] == 5, d6)
check("D6: the line's own cwnd= is read beside", gc == 1853, gc)
real = lc.read(os.path.join(HERE, "..", "..", "docs", "l1-raw", "loop-diag", "p34", "run", "diag-ld", "c1d-400-DEF1024-s42-r1-a1-c.log"))
rd6, _ = dp.diag_d6(real, 2)
check("D6 on a real saved main [DIAG] log: both legs read", rd6[0]["qcwnd_med"] and rd6[0]["dgq_n"] > 5 and rd6[1]["diag_n"] > 5, rd6)
rdrv = lc.read(os.path.join(HERE, "..", "..", "docs", "l1-raw", "loop-diag", "p34", "run", "diag-ld", "c1d-400-DEF1024-s42-r1-a1-drv.out"))
qd = dp.parse_qdisc(rdrv)
check("QDISC lines of a real saved driver output parse (netem `Sent … bytes … pkt` on the client-side devices)",
      qd[0]["sent_bytes"] == 437567394 and qd[0]["sent_pkts"] == 338803 and qd[0]["dropped"] == 39 and qd[1]["sent_bytes"] == 1469100, qd)

# ── scoring a ledger ───────────────────────────────────────────────────────
def lgd(n, q, link, cv=0.05, k_q=None, state="live", p90=None):
    ks = {}
    for k in dp.K_LIST:
        qq = (k_q or {}).get(k, q)
        ks[str(k)] = {"lhat": qq * link, "cv": cv, "p90": (p90 if p90 is not None else qq) * link, "nwin": 20, "zero_dt": 0}
    return {"i": 0, "state": state, "k": ks, "d3": {"pairs": 10, "share": 0.7, "runs": {"3": 4, "6": 2}},
            "qcwnd_med": 639_000.0, "B_med": 12.0, "egress_dgrams": 1000}


def mkrow(cell, n, qs, mbps=100.0, arm=None, cv=0.05, k_q=None, seed="42", status="LIVE", egress=None):
    legs = []
    for i, link in enumerate(dp.LINKS36[cell]):
        lg = lgd(n, qs[i], link, cv=cv, k_q=k_q)
        lg["i"] = i
        lg["egress_dgrams"] = (egress or [1000] * 4)[i]
        legs.append(lg)
    return {"cell": cell, "arm": arm or f"CAP{n}", "seed": seed, "rep": 1, "status": status, "problems": [], "notes": [],
            "mbps": mbps, "d36": {"legs": legs, "m": M, "kind": "CAP", "gcwnd_med": 1800.0}}


def ledger_rows(qfun, cvfun=lambda cell, leg, n: 0.05, k_q=None, skip=(), mbps=lambda cell, n: 100.0):
    rows = []
    for cell in dp.CELLS36:
        for n in dp.GRID36[cell]:
            for seed in ("42", "7"):
                if (cell, n) in skip:
                    continue
                r = mkrow(cell, n, [qfun(cell, i, n) for i in range(len(dp.LINKS36[cell]))], mbps=mbps(cell, n), seed=seed, k_q=k_q)
                for i, lg in enumerate(r["d36"]["legs"]):
                    for kk in lg["k"].values():
                        kk["cv"] = cvfun(cell, i, n)
                rows.append(r)
    return rows


def run(rows, **kw):
    lines = []
    res = dp.score_rows(rows, out=lines.append, **kw)
    return res, "\n".join(lines)


with tempfile.TemporaryDirectory() as led:
    os.makedirs(os.path.join(led, "p27")), os.makedirs(os.path.join(led, "p34", "run"))
    for tag, rel, cells in (("p27", "p27/ld.log", ("c2-100", "c8-100")), ("p34", "p34/run/ld.log", ("c1s-400", "c1d-400"))):
        with open(os.path.join(led, rel), "w") as fh:
            for cell in cells:
                for n in (64, 128, 256, 512, 1024, 2048):
                    r = {"cell": cell, "arm": f"DEF{n}", "status": "LIVE", "seed": "42", "rep": 1, "mbps": 1.0}
                    for p in range(len(dp.LINKS36[cell])):
                        r[f"ld_p{p}"] = {"rtprop_us": 5000.0, "rtt_min_us": 5000.0, "rtt_p50_us": 10000.0, "q_pace_bps": 1e9}
                    fh.write("LDROW " + json.dumps(r) + "\n")
    res, txt = run(ledger_rows(lambda c, i, n: 1.0), ledgers=led)
    check("verdict: Q = 1.00 at every cap/leg, cv 0.05 -> DISPERSION-READS-LINK", res["verdict"] == "DISPERSION-READS-LINK" and res["failed"] == [], res["failed"])
    check("score ends in a RESULT json line", txt.splitlines()[-1].startswith("RESULT {") and json.loads(txt.splitlines()[-1][7:])["verdict"] == "DISPERSION-READS-LINK")
    edge = ledger_rows(lambda c, i, n: 0.85 if n % 2 == 0 and i == 0 else 1.15)
    check("D1 band is closed: Q = 0.85 and 1.15 exactly are FLAT-AT-LINK", run(edge, ledgers=led)[0]["verdict"] == "DISPERSION-READS-LINK")
    check("D1 band: 1.1501 is OVER-READ -> NOT-YET naming the cell", run(ledger_rows(lambda c, i, n: 1.1501 if c == "c2-100" else 1.0), ledgers=led)[0]["failed"][0]["cell"] == "c2-100")
    res, txt = run(ledger_rows(lambda c, i, n: 1.3 if c == "c1s-400" else 1.0, k_q={8: 1.0, 16: 1.0}), ledgers=led)
    f0 = res["failed"][0]
    check("c1s OVER-READ at k=4 (1.3) but flat at k=8 and 16 -> NOT-YET, D1 OVER-READ, would pass at k=[8, 16]",
          res["verdict"] == "NOT-YET" and f0["cell"] == "c1s-400" and f0["clause"] == "D1" and f0["got"] == "OVER-READ" and f0["pass_at_k"] == [8, 16], res["failed"])
    check("... the score text names it", "failed c1s-400 p0 D1=OVER-READ; would pass at k=[8, 16]" in txt, txt[-600:])
    res, _ = run(ledger_rows(lambda c, i, n: 0.5 if c == "c2-100" else 1.0), ledgers=led)
    check("flat Q = 0.5 -> APP-LIMITED", res["cells"]["c2-100"]["legs"]["0"]["D1"]["4"] == "APP-LIMITED")
    g = dp.GRID36["c1s-400"]
    res, _ = run(ledger_rows(lambda c, i, n: (0.5 + 0.5 * g.index(n) / (len(g) - 1)) if c == "c1s-400" else 1.0), ledgers=led)
    check("Q rising 0.5 -> 1.0 with n (Spearman 1, Q(n_min) < 0.85) -> CIRCULAR", res["cells"]["c1s-400"]["legs"]["0"]["D1"]["4"] == "CIRCULAR", res["cells"]["c1s-400"]["legs"]["0"]["D1"])
    res, _ = run(ledger_rows(lambda c, i, n: (0.9 - 0.4 * g.index(n) / (len(g) - 1)) if c == "c1s-400" else 1.0), ledgers=led)
    check("Q falling 0.9 -> 0.5 -> not circular, median < 0.85 -> APP-LIMITED", res["cells"]["c1s-400"]["legs"]["0"]["D1"]["4"] == "APP-LIMITED")
    res, _ = run(ledger_rows(lambda c, i, n: (1.3 if g.index(n) % 2 else 0.7) if c == "c1s-400" else 1.0), ledgers=led)
    check("Q alternating 0.7 / 1.3 (median 1.0, no trend) -> NEITHER", res["cells"]["c1s-400"]["legs"]["0"]["D1"]["4"] == "NEITHER")
    res, _ = run(ledger_rows(lambda c, i, n: 1.0, cvfun=lambda c, i, n: 0.11 if (c, i, n) == ("c8-100", 1, 64) else 0.05), ledgers=led)
    fl = res["failed"]
    check("D2: cv_4 0.11 at one cap of c8 p1 -> NOISY naming the cap; the others stay QUIET",
          len(fl) == 1 and fl[0]["cell"] == "c8-100" and fl[0]["leg"] == 1 and fl[0]["clause"] == "D2" and res["cells"]["c8-100"]["legs"]["1"]["D2noisy"] == [64]
          and res["cells"]["c8-100"]["legs"]["0"]["D2"]["4"] == "QUIET", fl)
    check("D2 boundary: cv = 0.10 exactly is QUIET", run(ledger_rows(lambda c, i, n: 1.0, cvfun=lambda c, i, n: 0.10), ledgers=led)[0]["verdict"] == "DISPERSION-READS-LINK")
    res, _ = run(ledger_rows(lambda c, i, n: 1.0, skip={("c2-100", 16)}), ledgers=led)
    check("a missing cap is UNSCOREABLE, never a vacuous FLAT -> NOT-YET", res["verdict"] == "NOT-YET" and res["failed"][0]["got"] == "UNSCOREABLE", res["failed"])
    res, _ = run([], ledgers=led)
    check("an empty ledger is NOT-YET (every required clause unscoreable)", res["verdict"] == "NOT-YET" and len(res["failed"]) == 8, len(res["failed"]))
    res, _ = run([r for r in ledger_rows(lambda c, i, n: 1.0) if r["cell"] != "c1d-400"], ledgers=led)
    check("c1d is reported only: its absence does not touch the verdict", res["verdict"] == "DISPERSION-READS-LINK")
    res, _ = run([dict(r, status="WITNESS-FAIL") if r["cell"] == "c2-100" else r for r in ledger_rows(lambda c, i, n: 1.0)], ledgers=led)
    check("non-LIVE rows are not scored", res["verdict"] == "NOT-YET" and res["failed"][0]["cell"] == "c2-100")
    # an idle c8 leg (state idle) is not scored: that leg has no reading -> NOT-YET naming it
    rows_ = ledger_rows(lambda c, i, n: 1.0)
    for r in rows_:
        if r["cell"] == "c8-100":
            r["d36"]["legs"][1]["state"] = "idle"
    res, _ = run(rows_, ledgers=led)
    check("idle legs have no Q: c8 p1 UNSCOREABLE -> NOT-YET", res["verdict"] == "NOT-YET" and res["failed"][0]["leg"] == 1 and res["failed"][0]["got"] == "UNSCOREABLE")
    # D4
    rows_ = ledger_rows(lambda c, i, n: 1.0, mbps=lambda c, n: 900.0)
    rows_ += [mkrow("c1s-400", n, [1.0], mbps=1000.0 if n != 1024 else 1000.0, arm=f"NOCAP{n}") for n in dp.NOCAP_C1S]
    res, txt = run(rows_, ledgers=led)
    check("D4: NOCAP 1000 vs CAP 900 = 10 % drop > 5 % -> named as a covariate at every n",
          all(res["D4"][str(n)]["covariate"] and abs(res["D4"][str(n)]["drop"] - 0.1) < 1e-9 for n in dp.NOCAP_C1S) and "COVARIATE of c1s D1" in txt, res["D4"])
    rows_ = ledger_rows(lambda c, i, n: 1.0, mbps=lambda c, n: 960.0) + [mkrow("c1s-400", n, [1.0], mbps=1000.0, arm=f"NOCAP{n}") for n in dp.NOCAP_C1S]
    check("D4: a 4 % drop is not a covariate", not any(v["covariate"] for v in run(rows_, ledgers=led)[0]["D4"].values()))
    # D5: constant RTprop 5 ms + S 5 ms -> ask = 1.1 * link/1279 * 10 ms, a constant
    res, txt = run(ledger_rows(lambda c, i, n: 1.0, mbps=lambda c, n: float(min(n, 100))), ledgers=led)
    want = 1.1 * 12.5e6 / 1279.0 * 0.01
    o = res["cells"]["c2-100"]["D5"]["oracle-L"]
    check("D5 oracle-L at c2: constant ask = 1.1 * (12.5e6/1279) * 10 ms = 107.5 -> interior fixed point n_fp = 107.5, STABLE (slope 0)",
          o["kind"] == "STABLE" and abs(o["n_fp"] - want) < 1e-6 and o["slope"] == 0.0, o)
    gw = 64 + 36 * (want - 64) / 64
    check("D5: goodput at the fixed point is interpolated from this session's grid (G(64)=64, G(128)=100) against Gmax = 100",
          abs(o["G_fp"] - gw) < 1e-9 and abs(o["G_over_Gmax"] - gw / 100.0) < 1e-9, o)
    res, _ = run(ledger_rows(lambda c, i, n: 0.8), ledgers=led)
    h = res["cells"]["c2-100"]["D5"]["L-hat"]
    check("D5 L-hat at Q = 0.8 scales the ask: n_fp = 0.8 * 107.5 = 86.0", abs(h["n_fp"] - 0.8 * want) < 1e-6, h)
    c8 = res["cells"]["c8-100"]["D5"]["oracle-L"]
    wc8 = 1.1 * (12.5e6 + 2.5e6) / 1279.0 * 0.01
    res8, _ = run(ledger_rows(lambda c, i, n: 1.0), ledgers=led)
    check("D5 at c8 sums both legs: n_fp = 1.1 * (12.5e6 + 2.5e6)/1279 * 10 ms = 129.0",
          abs(res8["cells"]["c8-100"]["D5"]["oracle-L"]["n_fp"] - wc8) < 1e-6, res8["cells"]["c8-100"]["D5"]["oracle-L"])
    check("D5 has no ask below the ledger's lowest cap (no extrapolation): caps 16 and 32 print `-`", "ask(n) 16:- 32:-" in txt, [l for l in txt.splitlines() if "ask(n)" in l][:1])
    res, txt = run(ledger_rows(lambda c, i, n: 1.0), ledgers=os.path.join(led, "nowhere"))
    check("D5 says so when a ledger is absent and the rest of the score still runs", "D5 unavailable" in txt and res["verdict"] == "DISPERSION-READS-LINK")
    # D6 / D7 print
    rows_ = ledger_rows(lambda c, i, n: 1.0, )
    for r in rows_:
        if r["cell"] == "c8-100" and r["arm"] == "CAP64":
            r["d36"]["legs"][0]["egress_dgrams"], r["d36"]["legs"][1]["egress_dgrams"] = 900, 100
            r["d36"]["legs"][1]["state"] = "idle"
    res, txt = run(rows_, ledgers=led)
    d67 = res["cells"]["c8-100"]["D6D7"]["64"]
    check("D7: each leg's share of egress_dgrams per cap (900/100 -> 0.9, 0.1) and the idle-leg count",
          d67["share"] == [0.9, 0.1] and d67["p1"]["idle_rows"] == 2 and d67["p0"]["idle_rows"] == 0, d67)
    check("D6: quinn window (639000 B / 1279 = 500 datagrams) and backlog B per cap/leg", abs(d67["p0"]["qcwnd_dgrams"] - 639000 / 1279.0) < 1e-9 and d67["p0"]["B"] == 12.0, d67)

# ── the plan table ─────────────────────────────────────────────────────────
pl = dp.plan36()
check("plan 36: 56 invocations = 2 x (c1s 8 + c2 6 + c8 8 + c1d 3 + 3 NOCAP) (amendment 1's `62` does not add up)", len(pl) == 56 == dp.EXPECT_ROWS36, len(pl))
check("plan 36: block 0 (seed 42) first, ascending, c1s first; block 1 (seed 7) descending, c1d first",
      pl[0] == ("c1s-400", "CAP16", "42", 1) and pl[28][0] == "c1d-400" and pl[28][2:] == ("7", 1) and pl[28][1] == "CAP2048", (pl[0], pl[28]))
check("plan 36: per-cell grids", {c: sorted({int(a[3:]) for cc, a, s, _ in pl if cc == c and a.startswith("CAP") and s == "42"}) for c in dp.CELLS36} == dp.GRID36)
check("plan 36: NOCAP only at c1s {256,1024,2048}, once per block, beside its CAP row",
      sorted((a, s) for c, a, s, _ in pl if a.startswith("NOCAP")) == sorted((f"NOCAP{n}", s) for n in dp.NOCAP_C1S for s in ("42", "7"))
      and pl[pl.index(("c1s-400", "NOCAP256", "42", 1)) - 1][1] == "CAP256", "")
check("plan 36: smoke = c1s CAP256 and c8 CAP128", dp.plan36(smoke=True) == [("c1s-400", "CAP256", "42", 1), ("c8-100", "CAP128", "42", 1)])
check("plan 36: no duplicate rows", len(set(pl)) == len(pl))

print("FAILED %d" % fails if fails else "ALL OK")
sys.exit(1 if fails else 0)
