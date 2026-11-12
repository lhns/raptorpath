#!/usr/bin/env python3
"""Tests for disp40.py (status §40). Run: python3 test_disp40.py

Pcaps are synthesized from netem's release rule t_i = max(t_{i-1}, a_i + d_i) + b_i/R at a realistic epoch
(ns timestamps exceed 2**53). b/R is a whole number of ns at the link rates (8, 80, 400 ns per byte), so a busy
run is asserted with ==.
"""
import json
import os
import shutil
import struct
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
os.environ.pop("LD_PLAN", None)
import l1common as lc  # noqa: E402
import disp36 as d36  # noqa: E402
import disp40 as dp  # noqa: E402

fails = 0
TMPDIRS = []


def check(name, ok, detail=""):
    global fails
    print(("ok   " if ok else "FAIL ") + name + ((" -- " + str(detail)) if not ok else ""))
    if not ok:
        fails += 1


EPOCH = 1_700_000_000 * 10 ** 9
R = 12_500_000                      # 80 ns per byte
NSB = 80
M, HDR = 1252, 42


def frame_len(payload):
    return 14 + 20 + 8 + payload


# ── plan table ─────────────────────────────────────────────────────────────
pl = dp.plan40()
check("plan 40: 108 invocations = 27 caps x 4 rows", len(pl) == 108 == dp.EXPECT_ROWS40, len(pl))
check("plan 40: every (cell, cap) has exactly 4 rows: seeds 42 and 7, two reps each",
      all(sorted((s, r) for c, a, s, r in pl if c == cell and a == f"CAP{n}") == [("42", 1), ("42", 2), ("7", 1), ("7", 2)]
          for cell in dp.CELLS40 for n in dp.GRID40[cell]))
check("plan 40: block 0 ascending with c1s first; block 1 descending with c2tbf first (cells alternated)",
      pl[0] == ("c1s-400", "CAP16", "42", 1) and pl[27] == ("c2tbf-100", "CAP512", "7", 1) and pl[26] == ("c2tbf-100", "CAP512", "42", 1), (pl[0], pl[26], pl[27]))
check("plan 40: smoke = c2 CAP128 and c2tbf CAP128", dp.plan40(smoke=True) == [("c2-100", "CAP128", "42", 1), ("c2tbf-100", "CAP128", "42", 1)])
check("plan 40: grids as registered", dp.GRID40["c8-100"] == [16, 32, 64, 128, 256, 512, 1024] and dp.GRID40["c2slot-100"] == [32, 128, 512]
      and dp.GRID40["c1s-400"][-1] == 2048 and 768 not in dp.GRID40["c1s-400"])
check("plan 40: sustained rates (c2tbf is 6.25 MB/s) and crossovers (c8 slow 256)",
      dp.RATE40["c2tbf-100"] == [6.25e6] and dp.CROSS40["c8-100"] == [128, 256] and dp.CROSS40["c1s-400"] == [128])
cp = subprocess.run([sys.executable, os.path.join(HERE, "loopdiag_parse.py"), "plan40"], capture_output=True, text=True)
check("`plan40` subcommand prints the 108 lines of disp40.plan40", cp.stdout.splitlines() == [" ".join(map(str, r)) for r in pl], cp.stdout[:60])
import loopdiag_parse as ld  # noqa: E402
check("every plan-40 arm and cell is registered (ARM_SPEC, CELL_SPEC, N_LEGS)",
      all(a in ld.sp.ARM_SPEC and c in ld.sp.CELL_SPEC and ld.sp.N_LEGS[c] == len(dp.RATE40[c]) for c, a, _, _ in pl))

# ── scenarios (lib.sh): tc option text ─────────────────────────────────────
def lib(cmd):
    return subprocess.run(["bash", "-c", f"source ./lib.sh; {cmd}"], cwd=HERE, capture_output=True, text=True).stdout.strip()


check("c2slot = c2's five fields + netem `slot 1ms 3ms packets 16`",
      lib("scenario_params c2slot") == lib("scenario_params c2") and lib("scenario_netem_extra c2slot") == "slot 1ms 3ms packets 16")
check("c2tbf = c2's five fields + child `tbf rate 50mbit burst 125000 latency 50ms`",
      lib("scenario_params c2tbf") == lib("scenario_params c2") and lib("scenario_child_qdisc c2tbf") == "tbf rate 50mbit burst 125000 latency 50ms")
check("c2 and the other scenarios have neither an extra nor a child qdisc",
      lib("scenario_netem_extra c2; scenario_child_qdisc c2; scenario_netem_extra c3; scenario_child_qdisc c1") == "")
topo = open(os.path.join(HERE, "topo_dual.sh"), encoding="utf-8").read()
check("topo_dual.sh only names the root `handle 1:` when a child qdisc exists (other scenarios' tc is unchanged)",
      'h="handle 1:"' in topo and "[[ -n \"$child\" ]] && h=" in topo and "parent 1: handle 10: $child" in topo)


# ── helpers: netem release rule, pcap writer ───────────────────────────────
def release(arrivals, sizes, rate_nsb=NSB):
    t, out = 0, []
    for a, b in zip(arrivals, sizes):
        t = max(t, a) + b * rate_nsb
        out.append(t)
    return out


def stream(gaps_ns, b, start=EPOCH):
    """skbs of b bytes with the given inter-arrival gaps (ns): [(t, b)]"""
    t, out = start, []
    for g in gaps_ns:
        t += g
        out.append((t, b))
    return out


def pkt_of(orig, payload):
    eth = b"\x02" * 6 + b"\x04" * 6 + b"\x08\x00"
    ip = struct.pack(">BBHHHBBH4s4s", 0x45, 0, min(payload + 28, 65535), 0, 0, 64, 17, 0, bytes([10, 77, 0, 1]), bytes([10, 77, 0, 2]))
    udp = struct.pack(">HHHH", 40000, 7000, payload + 8, 0)
    return (eth + ip + udp + b"\x00" * 64)[:64], orig


def write_pcap(path, recs):
    with open(path, "wb") as fh:
        fh.write(struct.pack("<I", 0xA1B23C4D) + struct.pack("<HHiIII", 2, 4, 0, 0, 64, 1))
        for t, orig, pay in recs:
            data, o = pkt_of(orig, pay)
            fh.write(struct.pack("<IIII", t // 10 ** 9, t % 10 ** 9, len(data), o) + data)


# ── the estimator: p90 of r_4 per window, median over windows ──────────────
B = 10_000
S = B * NSB                                         # 800 us per skb at 12.5 MB/s
busy = stream([S] * 6000, B)
e = dp.estimate40(busy)
check("a busy run reads the link exactly (p90 of identical trains, == )", e["lhat"] == R and e["wmed"] == R and e["cv"] == 0.0, e)
check("D of a busy run is the link (delivered bytes / time, within one skb)", abs(e["D"] / R - 1) < 2e-3, e["D"])


def cycle_stream(slow, fast, nc, b=B):
    """nc cycles of `slow` skbs at spacing S (rate R) then `fast` skbs at spacing S/2 (rate 2R)."""
    gaps = ([S] * slow + [S // 2] * fast) * nc
    return stream(gaps, b)


c1 = cycle_stream(58, 2, 60)            # 2 fast gaps per 60: 5 trains of 60 (8.3 %) read above R, none fully fast
est_max = d36.estimate(c1, (4,))[4]
check("8.3 % of the trains read above R (4/3 R at most): the window max reads 4/3 R, the p90 reads R",
      abs(est_max["lhat"] / R - 4 / 3) < 1e-9 and abs(dp.estimate40(c1)["lhat"] / R - 1.0) < 1e-9, (est_max["lhat"] / R, dp.estimate40(c1)["lhat"] / R))
c2 = cycle_stream(40, 12, 80)           # 8 fully-fast trains of 52: 15.4 % > 10 %
check("15 % of the trains read 2R: the p90 reads 2R (it is a quantile, not a cap)", abs(dp.estimate40(c2)["lhat"] / R - 2.0) < 1e-9, dp.estimate40(c2)["lhat"])
check("the window median ignores the fast trains (reported beside)", abs(dp.estimate40(c2)["wmed"] / R - 1.0) < 1e-9, dp.estimate40(c2)["wmed"])
first = busy[0][0]
comp = [(first + 100 * i if t < first + 10 ** 9 else t, b) for i, (t, b) in enumerate(busy)]
check("the first 1 s is warm-up: a compressed first second does not move L-hat", dp.estimate40(comp)["lhat"] == R)
gappy = stream(([S] * 7 + [8 * S]) * 400, B)           # bursts of 8 skbs at the line rate, 15 S per cycle
eg = dp.estimate40(gappy)
# a 4-skb group, then a 3 S idle gap, repeated
check("a bursty leg: D = 8/15 R (8 skbs per 15 S) while the p90 sees the in-burst rate R",
      abs(eg["D"] / R - 8 / 15) < 5e-3 and abs(eg["lhat"] / R - 1.0) < 1e-9, (eg["D"] / R, eg["lhat"] / R))
check("E3 ratio of the bursty leg: (1 + q) L-hat / D = 1.1 x 15/8 = 2.06 >= 1.2", abs(dp.Q_GAIN * eg["lhat"] / eg["D"] - 1.1 * 15 / 8) < 0.01, dp.Q_GAIN * eg["lhat"] / eg["D"])
check("a leg with fewer than 5 skbs has no estimate", dp.estimate40(busy[:4])["lhat"] is None)

# ── E6 fields from [DIAG] ──────────────────────────────────────────────────
DIAGL = ("[DIAG] t={t}s win=768/1024 cwnd=1 infl=2 p0:infl=437/sinfl=0/bdp1(cap0) sout=0 rtt=7/wrtt=3/rtp3ms rtp_us=2842 "
         "sig_us={a}/n249339 rvar_us={b}/n249339 qsp_us={c}/n256 msd_us=-/n255 tlag_us=2671/n180 gapd=0/0 qcwnd=1 | ANCHOR")
cl = [DIAGL.format(t=0.5, a=9999, b=9999, c=9999), DIAGL.format(t=1.0, a=100, b=10, c=1), DIAGL.format(t=1.25, a=300, b=30, c=3),
      DIAGL.format(t=1.5, a=200, b=20, c=2)]
e6 = dp.e6_medians(cl, 1)[0]
check("E6: post-warm-up medians of sig/rvar/qsp/tlag (µs); a `-` field is skipped", e6["sig_us"] == 200 and e6["rvar_us"] == 20 and e6["qsp_us"] == 2
      and e6["tlag_us"] == 2671 and e6["msd_us"] is None, e6)


# ── a row through the capture path (child qdisc witnesses) ─────────────────
def snap_tree(tag, i, root, child=None, hit=0, backlog=0):
    """[SNAP] text of a netem root (+ optional tbf child): root = (sent_b, sent_p, dropped)"""
    q = (f"qdisc netem 1: root refcnt 11 limit 1000 delay 5ms  3ms rate 100Mbit seed 42  Sent {root[0]} bytes {root[1]} pkt (dropped {root[2]}, overlimits 0 requeues 0)"
         f"   backlog {backlog * 7000}b {backlog}p requeues 0  ")
    if child:
        q += (f"qdisc tbf 10: parent 1: rate 50Mbit burst 125000b lat 50ms  Sent {child[0]} bytes {child[1]} pkt "
              f"(dropped {child[2]}, overlimits 99 requeues 0)   backlog 0b 0p requeues 0  ")
    return (f"[SNAP] tag={tag} leg={i} dev=cli{i} qdisc: {q}filter: filter protocol all pref 49152 matchall chain 0 handle 0x1 "
            f"not_in_hw (rule hit {hit}) \taction order 1: gact action pass \tAction statistics: \tSent 1 bytes 1 pkt (dropped 0, overlimits 0 requeues 0)  ")


recs = []
t = EPOCH
for i in range(4000):
    t += 5 * 1294 * NSB if i % 5 else 1294 * NSB
    recs.append((t, frame_len(5 * M) + 4 * HDR if i % 5 else frame_len(M), 5 * M if i % 5 else M))
ser = d36.skb_series([(a, b, c) for a, b, c in recs], M)
# a capture where the tbf child dropped 40 skbs the root's counters include in `dropped`
tdir = tempfile.mkdtemp()
TMPDIRS.append(tdir)
write_pcap(os.path.join(tdir, "p0.pcap"), recs)
sb = sum(b for _, b in ser)
drv = [snap_tree("start", 0, (678, 9, 0), (678, 9, 0), hit=9),
       snap_tree("stop", 0, (678 + sb, 9 + 4000 * 3, 61 + 40), (678 + sb, 9 + 4000 * 3, 40), hit=9 + 4000 + 61 + 40),
       "[CAPTURE] leg=0 dev=srv0 captured=4000 kdrop=0"]
d, prob, notes = dp.row_fields("c2tbf-100", "CAP128", drv, [], tdir, None, "r")
check("child qdisc: the root's Sent/dropped already include the child's drops; hits - drops matches the capture (no problems)", prob == [], prob)
leg = d["legs"][0]
check("the row carries the p90 estimate, D and E6 fields for the live leg", leg["state"] == "live" and "p90" in leg and "e6" in leg and leg["p90"]["D"] is not None, leg.keys())
drv_bad = list(drv)
drv_bad[1] = snap_tree("stop", 0, (678 + sb, 9 + 4000 * 3, 61 + 40), (678 + sb, 9 + 4000 * 3, 40), hit=9 + 4000 + 61 + 40 + 400)
d, prob, _ = dp.row_fields("c2tbf-100", "CAP128", drv_bad, [], tdir, None, "r")
check("child qdisc: a 10 % shortfall of captured skbs against the windowed hits trips the witness", any(p.startswith("p36-skbs-p0=") for p in prob), prob)
write_pcap(os.path.join(tdir, "p0.pcap"), recs[:-400])
d, prob, _ = dp.row_fields("c2tbf-100", "CAP128", drv, [], tdir, None, "r")
check("child qdisc: a tail-truncated capture trips both the skb and the byte witness", any(p.startswith("p36-skbs") for p in prob) and any(p.startswith("p36-bytes") for p in prob), prob)

# ── clauses and the verdict on constructed ledgers ─────────────────────────
def mkrow(cell, n, qs, d=1.0, cv=0.05, share=0.7, status="LIVE", seed="42", rep=1, wm=None, e6=None, state="live"):
    legs = []
    for i, rate in enumerate(dp.RATE40[cell]):
        q = qs[i]
        lh = q * rate
        legs.append({"i": i, "state": state, "d3": {"share": share, "runs": {}},
                     "e6": e6 or {f: 100.0 for f in dp.FIELDS40},
                     "p90": {"lhat": lh, "wmed": (wm if wm is not None else q) * rate, "cv": cv, "D": rate * (1.0 if d is None else d),
                             "nwin": 20, "zero_dt": 0}})
    return {"cell": cell, "arm": f"CAP{n}", "seed": seed, "rep": rep, "status": status, "problems": [], "notes": [], "d36": {"legs": legs, "m": M}}


def ledger(qfun, dfun=lambda c, i, n: 0.5, skip=(), jitter=lambda c, i, n, k: 1.0):
    rows = []
    for cell in dp.CELLS40:
        for n in dp.GRID40[cell]:
            for k, (seed, rep) in enumerate(dp.BLOCKS40):
                if (cell, n) in skip:
                    continue
                qs = [qfun(cell, i, n) * jitter(cell, i, n, k) for i in range(len(dp.RATE40[cell]))]
                rows.append(mkrow(cell, n, qs, d=None, seed=seed, rep=rep))
                for i, lg in enumerate(rows[-1]["d36"]["legs"]):
                    lg["p90"]["D"] = dfun(cell, i, n) * dp.RATE40[cell][i]
    return rows


def run(rows):
    lines = []
    res = dp.score_rows(rows, out=lines.append)
    return res, "\n".join(lines)


def q_ok(cell, i, n):
    cross = dp.CROSS40[cell][i if len(dp.CROSS40[cell]) > 1 else 0] if cell in dp.CROSS40 else 128
    return 1.0 if n >= cross else 0.8        # below the crossover the p90 reads the delivered rate's multiple (not over the line)


def dstd(c, i, n):
    """delivered rate / sustained: 1.0 at and above the leg's crossover, 0.5 below it (so (1+q) x 0.8 / 0.5 = 1.76)"""
    cross = dp.CROSS40[c][i if len(dp.CROSS40[c]) > 1 else 0]
    return 1.0 if n >= cross else 0.5


res, txt = run(ledger(q_ok, dfun=dstd))
check("verdict: Q = 1.00 above the crossover, E3 ratio 1.1 x 0.8 / 0.5 = 1.76 below, rows agree -> P90-READS-LINK and AGG-SAFE",
      res["verdict"] == "P90-READS-LINK" and res["agg"] == "AGG-SAFE" and res["failed"] == [], (res["failed"], res["agg"]))
check("the score ends in a RESULT json line", txt.splitlines()[-1].startswith("RESULT {") and json.loads(txt.splitlines()[-1][7:])["verdict"] == "P90-READS-LINK")


def with_(over):
    def f(c, i, n):
        v = over(c, i, n)
        return q_ok(c, i, n) if v is None else v
    return f


r_, _ = run(ledger(with_(lambda c, i, n: 1.1001 if (c, n) == ("c2-100", 256) else None), dfun=dstd))
check("E1 band is closed at 1.10 and Q = 1.1001 above the crossover fails E1 (and E2), naming the cell",
      {(f["cell"], f["clause"]) for f in r_["failed"]} == {("c2-100", "E1"), ("c2-100", "E2")} and r_["verdict"] == "NOT-YET", r_["failed"])
r_, _ = run(ledger(with_(lambda c, i, n: 1.10 if (c, n) == ("c2-100", 256) else None), dfun=dstd))
check("Q = 1.10 exactly passes E1 and E2", r_["verdict"] == "P90-READS-LINK", r_["failed"])
r_, _ = run(ledger(with_(lambda c, i, n: 0.8999 if (c, n) == ("c1s-400", 512) else None), dfun=dstd))
check("Q = 0.8999 above the crossover fails E1 (and not E2)", {(f["cell"], f["clause"]) for f in r_["failed"]} == {("c1s-400", "E1")}, r_["failed"])
r_, _ = run(ledger(with_(lambda c, i, n: 1.12 if (c, n) == ("c1s-400", 16) else None), dfun=dstd))
check("E2 holds at every cap: Q = 1.12 below the crossover fails E2 only", {(f["cell"], f["clause"]) for f in r_["failed"]} == {("c1s-400", "E2")}, r_["failed"])
r_, _ = run(ledger(q_ok, dfun=lambda c, i, n: dstd(c, i, n) if n >= dp.CROSS40[c][i if len(dp.CROSS40[c]) > 1 else 0] else 0.8 * 1.1 / 1.19))
check("E3 boundary: (1+q) L-hat/D = 1.19 < 1.2 fails at every scored cell", {f["clause"] for f in r_["failed"]} == {"E3"} and len(r_["failed"]) == 4, r_["failed"])
r_, _ = run(ledger(q_ok, dfun=lambda c, i, n: dstd(c, i, n) if n >= dp.CROSS40[c][i if len(dp.CROSS40[c]) > 1 else 0] else 0.8 * 1.1 / 1.21))
check("E3 boundary: 1.21 >= 1.2 passes", r_["verdict"] == "P90-READS-LINK", r_["failed"])
r_, _ = run(ledger(q_ok, dfun=lambda c, i, n: 1.0))
check("E3: L-hat/D = 0.8 below the crossover gives (1+q) x 0.8 = 0.88 < 1.2: E3 fails at every scored cell",
      {f["clause"] for f in r_["failed"]} == {"E3"} and {f["cell"] for f in r_["failed"]} == {"c1s-400", "c2-100", "c8-100"}, r_["failed"])
r_, _ = run(ledger(q_ok, dfun=dstd, jitter=lambda c, i, n, k: 1.06 if (k == 3 and n >= 128 and c == "c2-100") else 1.0))
check("E4: one of the 4 rows 6 % above the others at a cap >= the crossover fails E4 (max/min 1.06 > 1.05)",
      {(f["cell"], f["clause"]) for f in r_["failed"]} == {("c2-100", "E4")} and r_["cells"]["c2-100"]["legs"]["0"]["E4"]["256"] > 1.05, r_["failed"])
r_, _ = run(ledger(q_ok, dfun=dstd, jitter=lambda c, i, n, k: 1.04 if (k == 3 and n >= 128 and c == "c2-100") else 1.0))
check("E4: 4 % spread passes", r_["verdict"] == "P90-READS-LINK", r_["failed"])
r_, _ = run(ledger(q_ok, dfun=dstd, skip={("c8-100", 1024)}))
check("a missing cap is UNSCOREABLE (E1, E2), never a vacuous pass -> NOT-YET", r_["verdict"] == "NOT-YET" and {f["got"] for f in r_["failed"]} == {"UNSCOREABLE"}, r_["failed"])
rows_ = [r for r in ledger(q_ok, dfun=dstd) if not (r["cell"] == "c1s-400" and r["arm"] == "CAP256" and r["rep"] == 2 and r["seed"] == "7")]
r_, _ = run(rows_)
check("E4 needs the 4 rows: with 3 live rows at one cap E4 is UNSCOREABLE", {(f["cell"], f["clause"], f["got"]) for f in r_["failed"]} == {("c1s-400", "E4", "UNSCOREABLE")}, r_["failed"])
r_, _ = run([])
check("an empty ledger is NOT-YET (every scored clause unscoreable)", r_["verdict"] == "NOT-YET" and len(r_["failed"]) == 16, len(r_["failed"]))
# the c8 slow leg crosses over at 256
def q_c8(c, i, n):
    if c == "c8-100" and i == 1:
        return 1.0 if n >= 256 else 0.8
    return q_ok(c, i, n)


def d_c8(c, i, n):
    if c == "c8-100" and i == 1:
        return 1.0 if n >= 256 else 0.5
    return 1.0 if n >= 128 else 0.5


r_, _ = run(ledger(q_c8, dfun=d_c8))
check("c8's slow leg crosses over at 256: Q(128) = 0.8 there is below the crossover (E3, not E1) and the verdict holds", r_["verdict"] == "P90-READS-LINK", r_["failed"])
r_, _ = run(ledger(lambda c, i, n: 0.8 if (c == "c8-100" and i == 1) else q_ok(c, i, n), dfun=d_c8))
check("... the same slow leg at Q = 0.8 for all caps fails E1 at 256 and above", {(f["cell"], f["clause"]) for f in r_["failed"]} == {("c8-100", "E1")}, r_["failed"])

# aggregation and policer
base = lambda c, i, n: 1.0 if c in dp.SCORED40 else 1.0                     # noqa: E731
r_, txt_ = run(ledger(lambda c, i, n: (2.0 if c == "c2slot-100" else 1.0), dfun=lambda c, i, n: 1.0))
check("AGG-OVER-READ when c2slot reads 2x its sustained rate, with the factor; scored verdict unaffected by the agg cells",
      r_["agg"] == "AGG-OVER-READ" and abs(r_["agg_factor"] - 2.0) < 1e-9 and "x2.00" in txt_ and not any(f["cell"] in dp.AGG40 for f in r_["failed"]), (r_["agg"], r_["agg_factor"]))
r_, txt_ = run(ledger(lambda c, i, n: (2.0 if c == "c2tbf-100" else q_ok(c, i, n)), dfun=dstd))
check("AGG-OVER-READ for the tbf policer too; P90-READS-LINK + AGG-OVER-READ prints the aggregation-bound line",
      r_["agg"] == "AGG-OVER-READ" and r_["verdict"] == "P90-READS-LINK" and "aggregation bound" in txt_, txt_[-400:])
r_, _ = run(ledger(lambda c, i, n: (1.1 if c in dp.AGG40 else q_ok(c, i, n)), dfun=dstd))
check("AGG-SAFE needs E2 on both cells: Q = 1.10 passes", r_["agg"] == "AGG-SAFE", r_["agg"])
r_, _ = run(ledger(lambda c, i, n: (1.1001 if c == "c2slot-100" else (1.0 if c in dp.AGG40 else q_ok(c, i, n))), dfun=dstd))
check("... and Q = 1.1001 on one of them is AGG-OVER-READ", r_["agg"] == "AGG-OVER-READ", r_["agg"])
r_, _ = run([r for r in ledger(q_ok, dfun=dstd) if r["cell"] not in dp.AGG40])
check("absent agg cells are AGG-UNSCOREABLE, not AGG-SAFE", r_["agg"] == "AGG-UNSCOREABLE", r_["agg"])
# E5/E6 are reported
rr = ledger(q_ok, dfun=dstd)
for r in rr:
    if r["cell"] == "c2-100" and r["arm"] == "CAP128":
        r["d36"]["legs"][0]["e6"] = {"sig_us": 11.0, "rvar_us": 22.0, "qsp_us": 33.0, "msd_us": 44.0, "tlag_us": 55.0}
r_, txt_ = run(rr)
check("E6: the per-cap medians of the µs fields are reported", r_["cells"]["c2-100"]["legs"]["0"]["E6"]["128"]["tlag_us"] == 55.0 and "55" in txt_)
check("E5: the window-median Q and the in-run share are printed beside", "Q(wmed)" in txt_ and "in-run" in txt_)

# ── the battery keeps ABORT-BIN, and plan 40 shares plan 36's tmpfs rules ──
bat = open(os.path.join(HERE, "loopdiag_battery.sh"), encoding="utf-8").read()
runall = open(os.path.join(HERE, "loopdiag_run_all.sh"), encoding="utf-8").read()
check("battery: plan 40 accepted, 108-row table, cells c2slot-100/c2tbf-100 specified, ABORT-BIN kept",
      "36|38|40" in bat and '-eq 108' in bat and "c2slot-100)" in bat and "c2tbf-100)" in bat and "ABORT-BIN" in bat)
check("run_all: plan 40 takes the §36 precondition, LD_HARD, root-owned tmpfs tree and root-installed binary, and gates on test_disp40.py",
      'P_SMOKE=smoke40' in runall and runall.count('"$PLAN" = "40"') >= 6 and "test_disp40.py" in runall)

for d_ in TMPDIRS:
    shutil.rmtree(d_, ignore_errors=True)
print("FAILED %d" % fails if fails else "ALL OK")
sys.exit(1 if fails else 0)
