#!/usr/bin/env python3
"""Tests for loopdiag_parse.py. Run: python3 test_loopdiag_parse.py

Real saved logs (docs/l1-raw/store-law-n1/diag-sl, docs/l1-raw/anchor-diag/diag-ad)
give the base line shapes; the [LOOPDIAG] and [ACKDIAG] lines are synthetic and
follow the Rust format strings (src/net/loopdiag.rs `format_line`,
src/net/ackdiag.rs, src/gates.rs echo_line).
"""
import json
import os
import re
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import l1common as lc  # noqa: E402
import loopdiag_parse as ld  # noqa: E402

RAW = os.path.join(HERE, "..", "..", "docs", "l1-raw", "store-law-n1", "diag-sl")
C1S = os.path.join(RAW, "c1s-400-CTL-s42-r1-a1-")
ADRAW = os.path.join(HERE, "..", "..", "docs", "l1-raw", "anchor-diag", "diag-ad")
AD_MAIN = os.path.join(ADRAW, "c1s-400-MAIN-s42-r1-a1-")
fails = 0


def check(name, ok, detail=""):
    global fails
    print(("ok   " if ok else "FAIL ") + name + ((" -- " + str(detail)) if not ok else ""))
    if not ok:
        fails += 1


ACK = ("[ACKDIAG] p0 win=1.00s acks=5000/z=10(0.2%) gap_us[p50=40 p90=900 p99=2000 n=5000] "
       "drecv[p50=1 p90=4 max=64 n=5000 sum=9000] rd[acc=4000 rej=100 cnt=4100] "
       "rate_lr=95000sym/s x[p50=1.20 p90=3.00 p99=9.50] xanchor=7.40 anchor=700000sym "
       "rtprop=2.50ms recon[sent=1 crecv=1 cexp=1 srcack=1 cr/s=1.000 ce/cr=1.000 cr/sa=1.000] ov=0")
SUMCAP = ("[SUMCAP] on=1 eng=100/100 chg=100/100 chg_frac=1.0000 pin=0.0000 floor=0.0000 "
          "cap=300.0 ask=300.0")


def LDL(t, p=0, n=100, ov=0, mn="4000", p10="4100", p50="4300", rtp="4000", jit="150", srtt="4400",
        dlv=24000, rate=97000, pace="1000000000", cwnd="900000", qs="4200"):
    """A [LOOPDIAG] line in the exact shape of loopdiag::format_line."""
    return (f"[LOOPDIAG] t={t}s p{p} n={n} ov={ov} rtt_min_us={mn} rtt_p10_us={p10} rtt_p50_us={p50} "
            f"rtprop_us={rtp} jit_us={jit} srtt_us={srtt} dlv={dlv} dlv_rate={rate} "
            f"q_pace_bps={pace} q_cwnd={cwnd} q_srtt_us={qs}")


def base_cli():
    return lc.read(C1S + "c.log")


def sub_gate(lines, name, val):
    return [re.sub(r"\b" + name + r"=\S+", f"{name}={val}", ln) if "[GATES]" in ln else ln for ln in lines]


def set_den(lines, n):
    return [re.sub(r"\bwin=\d+/\d+", f"win={n}/{n}", ln) if "[DIAG] t=" in ln else ln for ln in lines]


def mk(arm, cell="c1s-400", store=None, eng=True, ackdiag=True, n=None, den=None, ld_lines=None):
    cli = sub_gate(base_cli(), "RWM_STORE", store if store is not None else (str(n) if n else "unset"))
    if n is not None or den is not None:
        cli = set_den(cli, den if den is not None else n)
    cli = [ln for ln in cli if "[SUMCAP]" not in ln]
    if eng:
        cli.append(SUMCAP)
    if ackdiag:
        cli += [ACK, ACK]
    if ld_lines is None:
        ld_lines = [LDL(0.5, mn="1"), LDL(1.0), LDL(1.5), LDL(2.0), LDL(2.5, mn="4200", p50="4500"), LDL(3.0)]
    cli += ld_lines
    return ld.make_row(cell, arm, "42", "1", 0, 10, lc.read(C1S + "drv.out"), cli, lc.read(C1S + "s.log"), 0, "x")


# ── [LOOPDIAG] extraction ────────────────────────────────────────────────
ws = ld.loopdiag_windows([LDL(0.5, mn="1"), LDL(1.0), LDL(2.0, p=1, mn="9"), LDL(1.25, n=0, mn="-", p10="-", p50="-")])
check("warm-up (t < 1.0 s) and other paths excluded", [w["t"] for w in ws] == [1.0, 1.25], ws)
check("`-` fields parse to None, numbers to float", ws[1]["rtt_min_us"] is None and ws[0]["rtt_min_us"] == 4000.0
      and ws[0]["q_pace_bps"] == 1e9 and ws[0]["dlv_rate"] == 97000.0, ws)
n, med, npace = ld.loopdiag_summary([LDL(1.0), LDL(2.0, mn="4200"), LDL(3.0, mn="4600"), LDL(1.5, n=0, mn="-", p10="-", p50="-")])
check("windows with n=0 do not count and do not enter the medians", n == 3 and med["rtt_min_us"] == 4200.0, (n, med))
check("per-row medians over all ten fields", med["rtprop_us"] == 4000.0 and med["jit_us"] == 150.0
      and med["q_pace_bps"] == 1e9 and med["q_cwnd"] == 900000.0 and med["q_srtt_us"] == 4200.0
      and med["dlv_rate"] == 97000.0 and med["srtt_us"] == 4400.0 and med["rtt_p10_us"] == 4100.0, med)
n, med, npace = ld.loopdiag_summary([LDL(1.0, pace="-"), LDL(2.0, pace="-")])
check("q_pace_bps all `-`: median None, numeric count 0", med["q_pace_bps"] is None and npace == 0, (med, npace))

# ── witnesses ────────────────────────────────────────────────────────────
m = mk("MAIN")
check("MAIN pass", m["status"] == "LIVE", (m["status"], m["problems"]))
check("MAIN row carries the medians (rtt_min_us over 5 post-warm windows)", m["ld_n"] == 5
      and m["ld_med"]["rtt_min_us"] == 4000.0 and m["ld_pace_n"] == 5, (m["ld_n"], m["ld_med"]))
m2 = mk("MAIN", eng=False)
check("MAIN fail: SUMCAP eng absent", "main-sumcap-eng-zero" in m2["problems"] and m2["status"] == "WITNESS-FAIL", m2["problems"])
m3 = mk("MAIN", ackdiag=False)
check("fail: no [ACKDIAG]", "no-ackdiag" in m3["problems"], m3["problems"])
m4 = mk("MAIN", ld_lines=[LDL(1.0), LDL(2.0), LDL(3.0)])
check("fail: 3 post-warm-up p0 windows < 4", "loopdiag-p0-windows=3" in m4["problems"], m4["problems"])
m5 = mk("MAIN", ld_lines=[LDL(1.0), LDL(2.0), LDL(3.0), LDL(4.0, n=0, mn="-", p10="-", p50="-")])
check("fail: an n=0 window is not a witness window", "loopdiag-p0-windows=3" in m5["problems"], m5["problems"])
m6 = mk("MAIN", ld_lines=[LDL(0.1), LDL(0.2), LDL(0.3), LDL(0.4), LDL(0.5), LDL(1.0)])
check("fail: warm-up windows are not witnesses", "loopdiag-p0-windows=1" in m6["problems"], m6["problems"])
m7 = mk("MAIN", ld_lines=[LDL(t, pace="-") for t in (1.0, 2.0, 3.0, 4.0)])
check("fail: q_pace_bps never numeric", "no-q-pace-bps" in m7["problems"] and "loopdiag-p0-windows=4" not in m7["problems"], m7["problems"])
m8 = mk("MAIN", ld_lines=[LDL(t, p=1) for t in (1.0, 2.0, 3.0, 4.0)])
check("fail: p1 windows do not witness p0", "loopdiag-p0-windows=0" in m8["problems"], m8["problems"])
m9 = mk("MAIN", store="512")
check("MAIN fail: RWM_STORE set", "gate-store=512" in m9["problems"], m9["problems"])
s = mk("S256", n=256, eng=False)
check("S<n> pass (eng absent, every denominator == n)", s["status"] == "LIVE", (s["status"], s["problems"]))
s2 = mk("S256", n=256, eng=True)
check("S<n> fail: SUMCAP eng>0", any(x.startswith("s-sumcap-eng") for x in s2["problems"]), s2["problems"])
s3 = mk("S256", n=256, eng=False, den=1024)
check("S<n> fail: denominator != n", any(x.startswith("s-win-den-not-n") for x in s3["problems"]), s3["problems"])
s4 = mk("S256", eng=False, store="512", den=256)
check("S<n> fail: gate RWM_STORE != n", "gate-store=512" in s4["problems"], s4["problems"])
check("void rows keep their status", ld.make_row("c1s-400", "MAIN", "42", "1", 3, 10, [], [], [], 0, "x")["status"] == "VOID-RC")

# a real saved log (pre-LOOPDIAG): the specific witness problems, and the rest still parse
real = ld.make_row("c1s-400", "MAIN", "42", "1", 0, 10, lc.read(AD_MAIN + "drv.out"), lc.read(AD_MAIN + "c.log"),
                   lc.read(AD_MAIN + "s.log"), 0, "x")
check("real anchor-diag MAIN log: no LOOPDIAG -> loopdiag-p0-windows=0 and no-q-pace-bps",
      "loopdiag-p0-windows=0" in real["problems"] and "no-q-pace-bps" in real["problems"]
      and real["status"] == "WITNESS-FAIL", (real["status"], real["problems"]))
check("real log: everything else still extracts (goodput, SUMCAP, ACKDIAG, A_x, rtt, rtp_us, CPU/GB)",
      real["mbps"] and real["sumcap_eng"] and real["ackdiag_n"] >= 1 and real["ax_p50_us"] and real["rtt_med"]
      and real["rtp_us_med"] and real["cpu_cli_gb"] and "no-ackdiag" not in real["problems"]
      and "main-sumcap-eng-zero" not in real["problems"],
      {k: real.get(k) for k in ("mbps", "sumcap_eng", "ackdiag_n", "ax_p50_us", "rtt_med", "rtp_us_med", "cpu_cli_gb")})
check("real log: LDROW round-trips through JSON", json.loads(json.dumps(real, sort_keys=True))["cell"] == "c1s-400")


# ── synthetic ledger for the score ───────────────────────────────────────
GM = {192: 100, 256: 300, 384: 600, 512: 700, 768: 720, 1024: 725, 1536: 725, 2048: 720}   # n* = 512 (>= 688.75)
CELL = "c1s-400"
LINKB = 125e6


def R(cell, arm, mbps, ldm, seed=42, rep=1, status="LIVE"):
    return {"cell": cell, "arm": arm, "seed": str(seed), "rep": rep, "status": status, "problems": [], "mbps": mbps,
            "ax_p50_us": 1000, "ax_p95_us": 2000, "rtt_med": 10.0, "sumcap_cap": 300.0, "ld_med": ldm}


def LM(tmin=4000.0, q=1.0, d=97700.0, rtp=4000.0, jit=100.0, linkb=LINKB):
    return {"rtt_min_us": tmin, "rtt_p10_us": tmin * 1.05, "rtt_p50_us": tmin * 1.1, "rtprop_us": rtp, "jit_us": jit,
            "srtt_us": tmin * 1.1, "dlv_rate": d, "q_pace_bps": q * linkb * 8.0, "q_cwnd": 900000.0, "q_srtt_us": tmin}


def sweep(gm=GM, tmin=lambda n: 4000.0, q=lambda n: 1.0, d=lambda n: 97700.0, rtp=4000.0, per=2, cell=CELL, linkb=LINKB):
    rows = []
    for n, g in gm.items():
        for i in range(per):
            rows.append(R(cell, f"S{n}", float(g), LM(tmin(n), q(n), d(n), rtp, linkb=linkb), seed=42 + i))
    return rows


def run(rows, cell=CELL):
    buf = []
    res = ld.score_cell(rows, cell, buf.append)
    return res, "\n".join(buf)


# n*, the table and the verdict lines
res, txt = run(sweep())
check("n* = 512 (smallest n with G >= 0.95 Gmax = 688.75)", res["nstar"] == 512, res)
check("band printed: [341.3, 768.0]", "[341.3, 768.0]" in txt, txt)
check("table has every column header", all(h in txt for h in ("Tmin us", "Tp10 us", "Tp50 us", "RTprop us", "jit us", "D sym/s", "Q=quinn/link", "q_cwnd")))
check("the two verdict lines are present", re.search(r"^  OPTION1 \S+ (REPRODUCES|MISSES) n_fp1=\S+$", txt, re.M)
      and re.search(r"^  OPTION3 \S+ (REPRODUCES|MISSES) C3med=\S+ C3b_med=\S+$", txt, re.M), txt)
res_knee, _ = run(sweep(gm={**GM, 512: 0.95 * 725}))
check("knee: G == 0.95 Gmax qualifies", res_knee["nstar"] == 512, res_knee)

# O1: FLAT / INFLATES / AMBIGUOUS
res, txt = run(sweep(tmin=lambda n: 4000.0))
check("O1 FLAT (constant Tmin, R1 = 1)", res["O1"] == "FLAT", txt)
res, txt = run(sweep(tmin=lambda n: 4000.0 * 1.25 if n == 2048 else 4000.0))
check("O1 FLAT at the 1.25 boundary (inclusive)", res["O1"] == "FLAT", txt)
res, txt = run(sweep(tmin=lambda n: 4000.0 * (1 + 0.5 * (n - 192) / (2048 - 192))))
check("O1 INFLATES (R1 = 1.5 exactly)", res["O1"] == "INFLATES" and "R1 = 1.500" in txt, txt)
res, txt = run(sweep(tmin=lambda n: 4000.0 * (1 + 0.3 * (n - 192) / (2048 - 192))))
check("O1 AMBIGUOUS (R1 = 1.3)", res["O1"] == "AMBIGUOUS", txt)
res, txt = run(sweep(tmin=lambda n: 4000.0 * 1.3 if n == 512 else 4000.0))
check("O1 AMBIGUOUS when only a mid-grid bump breaks FLAT (R1 = 1)", res["O1"] == "AMBIGUOUS" and "R1 = 1.000" in txt, txt)

# C1 fixed point: C1 = 1.1 * 97700 * T.  T = 2 ms -> 214.9 (< 256), T = 4 ms -> 429.9 (< 512 only)
res, txt = run(sweep(tmin=lambda n: 2000.0))
check("C1 fixed point: T=2 ms -> n_fp1 = 256 (C1 = 214.9 >= 192, < 256), outside the band -> MISSES",
      res["n_fp1"] == 256 and "OPTION1 FLAT MISSES n_fp1=256" in txt, txt)
check("C1(n)/n printed per n", "192:214.9/1.119" in txt and "256:214.9/0.840" in txt, txt)
res, txt = run(sweep(tmin=lambda n: 4000.0))
check("C1 fixed point: T=4 ms -> n_fp1 = 512 (C1 = 429.9) in [341.3, 768] -> REPRODUCES",
      res["n_fp1"] == 512 and "OPTION1 FLAT REPRODUCES n_fp1=512" in txt, txt)
res, txt = run(sweep(tmin=lambda n: 20000.0))
check("C1 never < n on the grid -> n_fp1 `-`, MISSES", res["n_fp1"] is None and "OPTION1 INFLATES MISSES n_fp1=-" in txt or
      ("n_fp1=-" in txt and "MISSES n_fp1=-" in txt), txt)

# O3: FLAT-AT-LINK / OVER-READ / CIRCULAR / NEITHER
res, txt = run(sweep(q=lambda n: 1.0))
check("O3 FLAT-AT-LINK (Q = 1)", res["O3"] == "FLAT-AT-LINK", txt)
check("O3 FLAT-AT-LINK at both boundaries (0.8 and 1.25 inclusive)",
      run(sweep(q=lambda n: 0.8 if n < 512 else 1.25))[0]["O3"] == "FLAT-AT-LINK")
res, txt = run(sweep(q=lambda n: 2.0))
check("O3 OVER-READ (median Q = 2)", res["O3"] == "OVER-READ", txt)
res, txt = run(sweep(q=lambda n: 0.2 + 0.0005 * n))
check("O3 CIRCULAR (Q rises with n, Q(192) = 0.296 < 0.8, Spearman 1)", res["O3"] == "CIRCULAR" and "Spearman(Q,n) 1.000" in txt, txt)
res, txt = run(sweep(q=lambda n: 0.5))
check("O3 NEITHER (Q = 0.5 flat below the link: no rise)", res["O3"] == "NEITHER", txt)
res, txt = run(sweep(q=lambda n: 0.5 - 0.0001 * n))
check("O3 NEITHER (Q falls with n)", res["O3"] == "NEITHER", txt)
res, txt = run(sweep(q=lambda n: 0.9 + 0.0003 * n if n < 1536 else 0.5))
check("O3 NEITHER (not flat, median in band, no rank rise to a low start)", res["O3"] == "NEITHER", (res["O3"], txt))

# C3 / C3b / CJ: C3 = 1.1 (Q linkB / W) RTprop; Q = 1, RTprop = 4 ms -> 1.1 * 125e6/1279 * 0.004 = 429.99
res, txt = run(sweep(q=lambda n: 1.0, tmin=lambda n: 4000.0, rtp=4000.0))
want = 1.1 * 125e6 / 1279.0 * 0.004
check("C3med = 1.1 (Q linkB / W) RTprop = %.1f" % want, abs(res["c3med"] - want) < 1e-6 and "C3med=430.0" in txt, (res["c3med"], txt))
check("C3 in the band -> OPTION3 REPRODUCES", "OPTION3 FLAT-AT-LINK REPRODUCES C3med=430.0 C3b_med=430.0" in txt, txt)
res, txt = run(sweep(q=lambda n: 1.0, tmin=lambda n: 1000.0, rtp=4000.0))
check("C3b uses Tmin, not RTprop (Tmin = 1 ms -> 107.5)", "C3b_med=107.5" in txt and "C3med=430.0" in txt, txt)
res, txt = run(sweep(q=lambda n: 0.25, tmin=lambda n: 4000.0, rtp=4000.0))
check("C3 outside the band -> MISSES (Q = 0.25 -> C3 = 107.5 < 341.3)", "MISSES C3med=107.5" in txt, txt)
check("CJ reported, with its own value: 1.1 L (RTprop + jit) = 1.1*97700*0.0041 = 440.6", "CJ(n)" in txt
      and any("440.6" in ln for ln in txt.splitlines() if "CJ" in ln), txt)

# unscoreable pieces print `-`, never crash
thin = [r for r in sweep() if r["arm"] in ("S192", "S256")]
res, txt = run(thin)
check("sparse ledger: n*, O1, O3 still computed on what exists", res["nstar"] == 256, (res, txt))
res, txt = run([])
check("empty ledger: dashes, MISSES, no exception", "OPTION1 - MISSES n_fp1=-" in txt and "OPTION3 - MISSES C3med=- C3b_med=-" in txt, txt)
rows = sweep()
for r in rows:
    r["ld_med"]["q_pace_bps"] = None
res, txt = run(rows)
check("no quinn readings: O3 `-`, C3 `-`", res["O3"] == "-" and "C3med=-" in txt, txt)
live_only = sweep() + [R(CELL, "S512", 9999.0, LM(), seed=7, status="WITNESS-FAIL")]
res, txt = run(live_only)
check("non-live rows are ignored in the per-n means", "9999" not in txt, txt)

# the c2 cell uses its own link: L = 9770, link 12.5e6 B/s
c2 = sweep(gm={64: 10, 96: 40, 128: 80, 192: 90, 256: 90, 384: 90, 512: 90, 1024: 90}, d=lambda n: 9770.0,
           tmin=lambda n: 20000.0, rtp=20000.0, cell="c2-100", linkb=12.5e6)
res, txt = run(c2, "c2-100")
check("c2: n* = 192; C1 = 1.1*9770*0.02 = 214.9 -> n_fp1 = 256", res["nstar"] == 192 and res["n_fp1"] == 256
      and abs(res["c3med"] - 1.1 * 12.5e6 / 1279.0 * 0.02) < 1e-6, (res, txt))

# score() end to end: tables first, MAIN reference lines
allrows = sweep() + [R(CELL, "MAIN", 700.0, LM(), seed=s) for s in (42, 7)] + \
    [R("c2-100", "MAIN", 90.0, LM(linkb=12.5e6), status="WITNESS-FAIL")]
allrows[-1]["problems"] = ["x-problem"]
buf = []
with tempfile.TemporaryDirectory() as td:
    path = os.path.join(td, "ld.log")
    with open(path, "w") as fh:
        for r in allrows:
            fh.write("LDROW " + json.dumps(r) + "\n")
    ld.score([path], buf.append)
txt = "\n".join(buf)
check("score: ROWS and VOID tables precede the cell blocks",
      txt.index("ROWS by") < txt.index("VOID TABLE") < txt.index("CELL c1s-400") < txt.index("CELL c2-100"))
check("score: void row listed with its problem", "c2-100 MAIN s42 r1: WITNESS-FAIL x-problem" in txt, txt)
check("score: MAIN reference per cell (goodput 700.0, SUMCAP cap, LOOPDIAG medians)",
      "MAIN (live n=2) reference: goodput 700.0" in txt and "LOOPDIAG medians:" in txt and "q_pace_bps=1000000000" in txt, txt)
check("score: verdict lines for both cells", txt.count("OPTION1 ") == 2 and txt.count("OPTION3 ") == 2, txt)
with tempfile.TemporaryDirectory() as td:
    path = os.path.join(td, "empty.log")
    open(path, "w").close()
    ld.score([path], lambda *_: None)
check("score: empty ledger does not raise", True)

# check / smoke
with tempfile.TemporaryDirectory() as td:
    path = os.path.join(td, "ld.log")
    with open(path, "w") as fh:
        fh.write("LDROW {}\n" * 46 + "LD-BATTERY-DONE tag=ld blocks=3 x\n")
    check("check: 46 rows + done line passes", ld.check(path) == 0)
    with open(path, "w") as fh:
        fh.write("LDROW {}\n" * 45 + "LD-BATTERY-DONE tag=ld blocks=3 x\n")
    check("check: 45 rows fails", ld.check(path) == 1)
    with open(path, "w") as fh:
        fh.write("LDROW {}\n" * 46)
    check("check: no completion line fails", ld.check(path) == 1)
    path2 = os.path.join(td, "smoke.log")
    sm = [mk("MAIN"), mk("S192", cell="c2-100", n=192, eng=False)]
    with open(path2, "w") as fh:
        for r in sm:
            fh.write("LDROW " + json.dumps(r, sort_keys=True) + "\n")
    check("smoke: two live rows pass", ld.smoke(path2) == 0)
    sm[1] = mk("S192", cell="c2-100", n=192, eng=False, ld_lines=[LDL(1.0)])
    with open(path2, "w") as fh:
        for r in sm:
            fh.write("LDROW " + json.dumps(r, sort_keys=True) + "\n")
    check("smoke: a witness-failed row fails", ld.smoke(path2) == 1)

# the plan: 3 blocks x (2 MAIN + 8) + 2 x 8 = 46
check("plan arithmetic", 3 * (2 + len(ld.GRID_C1S)) + 2 * len(ld.GRID_C2) == ld.EXPECT_ROWS == 46)

print("FAILED %d" % fails if fails else "ALL OK")
sys.exit(1 if fails else 0)
