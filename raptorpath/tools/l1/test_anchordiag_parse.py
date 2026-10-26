#!/usr/bin/env python3
"""Tests for anchordiag_parse.py. Run: python3 test_anchordiag_parse.py

Real saved logs (docs/l1-raw/store-law-n1/diag-sl) give the base line shapes;
the arm-specific lines ([ACKDIAG], [RSTRACE-LEGACY], [RSTRACE], echoes, gate
values, [SUMCAP]) are synthetic and follow the Rust format strings
(src/net/ackdiag.rs, src/scheduler/copa.rs, src/gates.rs echo_line).
"""
import os
import re
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import l1common as lc  # noqa: E402
import anchordiag_parse as ad  # noqa: E402

RAW = os.path.join(HERE, "..", "..", "docs", "l1-raw", "store-law-n1", "diag-sl")
C1S = os.path.join(RAW, "c1s-400-CTL-s42-r1-a1-")
C1D = os.path.join(RAW, "c1d-400-LAW-s42-r1-a1-")
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
LEG = "[RSTRACE-LEGACY] path=0 rate={r} delta={d} elapsed_ms={e} max_bw=700000"
RS = "[RSTRACE] path=0 seq=5 rate=200000 delivered=40 interval_ms=2.50 send_ms=2.00 ack_ms=2.50 max_bw=100000"
SUMCAP = ("[SUMCAP] on=1 eng=100/100 chg=100/100 chg_frac=1.0000 pin=0.0000 floor=0.0000 "
          "cap=300.0 ask=300.0")


def base_cli():
    return lc.read(C1S + "c.log")


def sub_gate(lines, name, val):
    return [re.sub(r"\b" + name + r"=\S+", f"{name}={val}", ln) if "[GATES]" in ln else ln for ln in lines]


def set_den(lines, n):
    return [re.sub(r"\bwin=\d+/\d+", f"win={n}/{n}", ln) if "[DIAG] t=" in ln else ln for ln in lines]


def mk(arm, cell="c1s-400", plain=None, honest=None, store=None, eng=True, sampler=False, honest_echo=False,
       ackdiag=True, n=None, den=None):
    cli = sub_gate(base_cli(), "RWM_PLAIN_RS", plain if plain is not None else ("1" if arm == "PRS" else "0"))
    cli = sub_gate(cli, "RWM_HONEST_CAP", honest if honest is not None else "0")
    cli = sub_gate(cli, "RWM_STORE", store if store is not None else (str(n) if n else "unset"))
    if n is not None or den is not None:
        cli = set_den(cli, den if den is not None else n)
    if eng:
        cli.append(SUMCAP)
    else:
        cli = [ln for ln in cli if "[SUMCAP]" not in ln]
    if ackdiag:
        cli += [ACK, ACK]
    if sampler:
        cli.append("plain-mode send-interval SAMPLER ACTIVE (RWM_PLAIN_RS sampling-only)")
    if honest_echo:
        cli.append("honest floor-clock store caps ACTIVE (RWM_PLAIN_RS+RWM_HONEST_CAP)")
    cli += [LEG.format(r=300000, d=300, e="1.00"), LEG.format(r=300000, d=300, e="1.50"),
            LEG.format(r=250000, d=40, e="3.00"), RS]
    return ad.make_row(cell, arm, "42", "1", 0, 10, lc.read(C1S + "drv.out"), cli, lc.read(C1S + "s.log"), 0, "x")


# ── per-path btlbw on a dual log ─────────────────────────────────────────
D = lambda t, a, b: (f"[DIAG] t={t}s win=100/100 rtt=30.0ms bdp100=3sym fp/on p0:309/270 p1:276/315 "
                     f"p0:infl=1/sinfl=0/bdp1(cap0) btlbw={a} rtp_us={a // 10} | ANCHOR sent=0 "
                     f"p1:infl=1/sinfl=0/bdp1(cap0) btlbw={b} rtp_us={b // 10} | ANCHOR sent=0 rce=1")
cli = [D("0.5", 999999, 888888), D("1.0", 100, 10), D("2.0", 300, 30), D("3.0", 200, 20)]
b, r = ad.diag_paths(cli)
check("synthetic dual: two paths, warm-up excluded", sorted(b) == ["0", "1"] and len(b["0"]) == 3, b)
check("synthetic dual: per-path medians", lc.med(b["0"]) == 200 and lc.med(b["1"]) == 20, b)
check("synthetic dual: per-path rtp_us", lc.med(r["0"]) == 20 and lc.med(r["1"]) == 2, r)
rd = ad.make_row("c1d-400", "MAIN", "42", "1", 0, 10, lc.read(C1D + "drv.out"), lc.read(C1D + "c.log"),
                 lc.read(C1D + "s.log"), 0, "x")
check("real c1d log: two btlbw paths", sorted(rd["btlbw_p"]) == ["0", "1"], rd["btlbw_p"])
check("real c1d log: base fields parse", rd["mbps"] and rd["gates_cli"] and rd["sumcap_eng"] and rd["ax_p50_us"]
      and rd["cpu_cli_gb"] and rd["rtt_med"], {k: rd.get(k) for k in ("mbps", "sumcap_eng", "ax_p50_us", "cpu_cli_gb")})
check("real c1d log (no ACKDIAG): WITNESS-FAIL no-ackdiag", rd["status"] == "WITNESS-FAIL" and "no-ackdiag" in rd["problems"],
      (rd["status"], rd["problems"]))
r1 = ad.make_row("c1s-400", "MAIN", "42", "1", 0, 10, lc.read(C1S + "drv.out"), base_cli(), lc.read(C1S + "s.log"), 0, "x")
check("real c1s log: single path, base fields", list(r1["btlbw_p"]) == ["0"] and r1["mbps"], r1["btlbw_p"])
check("real c1s log: WITNESS-FAIL names no-ackdiag", "no-ackdiag" in r1["problems"] and r1["status"] == "WITNESS-FAIL",
      r1["problems"])

# ── ACKDIAG / RSTRACE parsing ────────────────────────────────────────────
ok = mk("MAIN")
a0 = ok["ackdiag"]["0"]
check("ackdiag fields", a0["gap_p50"] == 40 and a0["gap_n"] == 5000 and a0["drecv_max"] == 64 and a0["rd_acc"] == 4000
      and a0["rate_lr"] == 95000 and a0["x_p99"] == 9.5 and a0["xanchor"] == 7.4 and a0["anchor"] == 700000, a0)
check("ackdiag uses lines, last per path", ok["ackdiag_n"] == 2 and list(ok["ackdiag"]) == ["0"])
lg = ok["legacy"]
check("legacy summary", lg["n"] == 3 and abs(lg["frac_lt2"] - 2 / 3) < 1e-3 and lg["el_max"] == 3.0 and lg["delta_max"] == 300
      and lg["rate_max"] == 300000, lg)
check("rstrace summary", ok["rstrace"]["n"] == 1 and ok["rstrace"]["interval_ms_p50"] == 2.5 and ok["rstrace"]["send_ms_p50"] == 2.0, ok["rstrace"])
check("[RSTRACE-LEGACY] is not counted as [RSTRACE]", ok["rstrace"]["n"] == 1)
check("ackdiag with '-' fields -> None",
      ad.ackdiag_last([ACK.replace("x[p50=1.20 p90=3.00 p99=9.50]", "x[p50=- p90=- p99=-]").replace("xanchor=7.40", "xanchor=-")])[0]["0"]["x_p50"] is None)

# ── witnesses, each arm: one pass, one fail ──────────────────────────────
m = mk("MAIN")
check("MAIN pass", m["status"] == "LIVE", (m["status"], m["problems"]))
m2 = mk("MAIN", eng=False)
check("MAIN fail: SUMCAP eng absent", m2["status"] == "WITNESS-FAIL" and "main-sumcap-eng-zero" in m2["problems"], m2["problems"])
m3 = mk("MAIN", sampler=True)
check("MAIN fail: SAMPLER echo present", "main-sampler-echo-present" in m3["problems"] and m3["status"] == "WITNESS-FAIL", m3["problems"])
m4 = mk("MAIN", ackdiag=False)
check("any arm fail: no [ACKDIAG]", "no-ackdiag" in m4["problems"], m4["problems"])
p = mk("PRS", sampler=True)
check("PRS pass", p["status"] == "LIVE", (p["status"], p["problems"]))
p2 = mk("PRS", sampler=False)
check("PRS fail: SAMPLER echo missing", "prs-sampler-echo-missing" in p2["problems"], p2["problems"])
p3 = mk("PRS", sampler=True, honest="1")
check("PRS fail: effective RWM_HONEST_CAP=1", "gate-honest-cap=1" in p3["problems"], p3["problems"])
p4 = mk("PRS", sampler=True, honest_echo=True)
check("PRS fail: honest-cap echo present", "prs-honest-cap-echo-present" in p4["problems"], p4["problems"])
p5 = mk("PRS", sampler=True, eng=False)
check("PRS fail: SUMCAP eng absent", "prs-sumcap-eng-zero" in p5["problems"], p5["problems"])
s = mk("S256", n=256, eng=False)
check("S<n> pass (eng absent, every denominator == n)", s["status"] == "LIVE", (s["status"], s["problems"]))
s0 = base_cli()
s0 = [re.sub(r"(RWM_STORE=)\S+", r"\g<1>256", ln) if "[GATES]" in ln else ln for ln in s0]
s0 = [ln for ln in s0 if "[SUMCAP]" not in ln] + [SUMCAP.replace("eng=100/100", "eng=0/0")] + [ACK]
s_zero = ad.make_row("c1s-400", "S256", "42", "1", 0, 10, lc.read(C1S + "drv.out"), set_den(s0, 256),
                     lc.read(C1S + "s.log"), 0, "x")
check("S<n> pass with eng=0/0", s_zero["status"] == "LIVE", (s_zero["status"], s_zero["problems"]))
s2 = mk("S256", n=256, eng=True)
check("S<n> fail: SUMCAP eng>0", any(x.startswith("s-sumcap-eng") for x in s2["problems"]), s2["problems"])
s3 = mk("S256", n=256, eng=False, den=1024)
check("S<n> fail: denominator != n", any(x.startswith("s-win-den-not-n") for x in s3["problems"]), s3["problems"])
s4 = mk("S256", eng=False, store="512", den=256)
check("S<n> fail: gate RWM_STORE != n", "gate-store=512" in s4["problems"], s4["problems"])
s5 = ad.make_row("c1s-400", "S256", "42", "1", 0, 10, lc.read(C1S + "drv.out"),
                 [ln for ln in set_den(s0, 256) if "[DIAG] t=" not in ln], lc.read(C1S + "s.log"), 0, "x")
check("S<n> fail: fewer than 2 windows", any(x.startswith("s-win-windows") for x in s5["problems"]), s5["problems"])
check("MAIN fail: wrong gate RWM_PLAIN_RS", "gate-plain-rs=1" in mk("MAIN", plain="1")["problems"])
check("void rows keep their status", ad.make_row("c1s-400", "MAIN", "42", "1", 3, 10, [], [], [], 0, "x")["status"] == "VOID-RC")


# ── synthetic ledger for Q1 / Q2 / Q3 ────────────────────────────────────
def R(cell, arm, seed=42, rep=1, mbps=500.0, btl=None, rtp=None, legacy=0.95, xa=None, status="LIVE", **kw):
    row = {"cell": cell, "arm": arm, "seed": str(seed), "rep": rep, "status": status, "problems": [], "mbps": mbps,
           "ax_p50_us": 1000, "ax_p95_us": 2000, "rtt_med": 10.0, "cpu_cli_gb": 20.0, "sumcap_cap": 300.0,
           "sumcap_pin": 0.0, "btlbw_p": btl or {}, "rtp_us_med": rtp,
           "ackdiag": {"0": {"xanchor": xa}} if xa is not None else {},
           "legacy": {"n": 10, "frac_lt2": legacy, "el_p50": 1.0, "el_p90": 1.5, "el_max": 2.5}}
    row.update(kw)
    return row


def run(fn, *a):
    buf = []
    v = fn(*a, buf.append)
    return v, "\n".join(buf)


# Q1
good = [R("c1s-400", "MAIN", s, 1, btl={"0": 4e5}) for s in (42, 7)]
v, _ = run(ad.q1, good, "c1s-400")
check("Q1 MECHANISM-CONFIRMED", v == "MECHANISM-CONFIRMED", v)
v, _ = run(ad.q1, good + [R("c1s-400", "MAIN", 42, 2, btl={"0": 4e5}, legacy=0.5)], "c1s-400")
check("Q1 MECHANISM-OPEN when one row has < 90 % floor-length samples", v == "MECHANISM-OPEN", v)
v, _ = run(ad.q1, good + [R("c1s-400", "MAIN", 42, 2, btl={"0": 2.0e5})], "c1s-400")   # 2.05 L < 3 L
check("Q1 MECHANISM-OPEN when one row's btlbw < 3 L", v == "MECHANISM-OPEN", v)
v, _ = run(ad.q1, good + [R("c1s-400", "MAIN", 42, 2, btl={"0": 4e5}, legacy=None)], "c1s-400")
check("Q1 no RSTRACE-LEGACY samples is not CONFIRMED", v == "MECHANISM-OPEN", v)
v, _ = run(ad.q1, [R("c2-100", "MAIN", btl={"0": 29310})], "c2-100")                    # exactly 3 L (L=9770)
check("Q1 boundary 3 L at c2 confirmed", v == "MECHANISM-CONFIRMED", v)
v, _ = run(ad.q1, [], "c2-100")
check("Q1 no live rows is UNSCOREABLE, not a verdict", v == "UNSCOREABLE", v)
v, _ = run(ad.q1, [R("c1s-400", "MAIN", btl={"0": 4e5}, status="WITNESS-FAIL")], "c1s-400")
check("Q1 ignores non-live rows", v == "UNSCOREABLE", v)

# Q2 (c8: p1 is the c3 leg, L = 1955)
v, _ = run(ad.q2, [R("c8-100", "PRS", btl={"0": 10000, "1": 2000})], "c8-100")
check("Q2 HONEST (o <= 1.2 on both paths)", v == "HONEST", v)
v, txt = run(ad.q2, [R("c8-100", "PRS", btl={"0": 10000, "1": 2400})], "c8-100")        # o1 = 1.23
check("Q2 OVER-READ on the slow path (c3 leg L=1955)", v == "OVER-READ" and "p1=1.23" in txt, (v, txt))
v, _ = run(ad.q2, [R("c1s-400", "PRS", btl={"0": 1.2 * 97700}), R("c1s-400", "PRS", 7, 1, btl={"0": 97700})], "c1s-400")
check("Q2 boundary o = 1.2 is HONEST", v == "HONEST", v)
v, _ = run(ad.q2, [R("c8-100", "PRS", btl={"0": 1000})], "c8-100")
check("Q2 missing path reading is UNSCOREABLE", v == "UNSCOREABLE", v)
v, _ = run(ad.q2, [R("c1s-400", "PRS", btl={"0": 1.0e5}), R("c1s-400", "PRS", 7, 1, btl={"0": 5e5})], "c1s-400")
check("Q2 one over-reading row makes the cell OVER-READ", v == "OVER-READ", v)

# Q2 compare: PRS - MAIN with 2 sigma_pooled
rows = [R("c1s-400", "PRS", s, r, mbps=g) for (s, r, g) in ((42, 1, 100.0), (7, 1, 102.0), (42, 2, 104.0))]
rows += [R("c1s-400", "MAIN", s, r, mbps=g) for (s, r, g) in ((42, 1, 200.0), (7, 1, 202.0), (42, 2, 204.0))]
buf = []
ad.q2_compare(rows, "c1s-400", buf.append)
check("Q2 compare: diff -100, 2s = 4", any("goodput" in ln and "diff -100.0" in ln and "2s 4.0" in ln for ln in buf), buf)
d, bnd = ad.band([1.0, 3.0], [2.0])
check("band: needs n>=2 in both arms for sigma", d == 0.0 and bnd is None)

# Q3: ask_h and n* arithmetic
GM = {192: 100, 256: 300, 384: 600, 512: 700, 768: 720, 1024: 725, 1536: 725, 2048: 720}


def sweep(cell, gm, per=2, thin=()):
    out = []
    for n, g in gm.items():
        k = 1 if n in thin else per
        out += [R(cell, f"S{n}", 42 + i, 1, mbps=float(g), xa=n / 100.0) for i in range(k)]
    return out


prs = lambda bw, rtp: [R("c1s-400", "PRS", 42, 1, btl={"0": bw}, rtp=rtp), R("c1s-400", "PRS", 7, 1, btl={"0": bw}, rtp=rtp),
                       R("c1s-400", "PRS", 42, 2, btl={"0": bw}, rtp=rtp)]
v, info = run(ad.q3, sweep("c1s-400", GM) + prs(30000, 10000), "c1s-400")[0]
check("Q3 n* = 512 (G >= 0.95 Gmax = 688.75)", info["nstar"] == 512, info)
check("Q3 ask_h = 1.1 * 30000 * 0.01 = 330", abs(info["ask_h"] - 330.0) < 1e-6, info)
check("Q3 HEADROOM-MISSING (512 > 495)", v == "HEADROOM-MISSING", v)
check("Q3 H = 182 symbols", abs(info["H"] - 182.0) < 1e-6, info)
txt = run(ad.q3, sweep("c1s-400", GM) + prs(30000, 10000), "c1s-400")[1]
check("Q3 H / bw_h in ms printed (6.07)", "6.07 ms" in txt, txt)
check("Q3 Spearman reported (xanchor rises with n -> 1.00)", "Spearman(xanchor, n) over grid means (reported only): 1.00" in txt, txt)
v, info = run(ad.q3, sweep("c1s-400", GM) + prs(40000, 9000), "c1s-400")[0]
check("Q3 LAW-SUFFICIENT (ask_h 396, 1.5x = 594 >= 512)", v == "LAW-SUFFICIENT" and abs(info["ask_h"] - 396.0) < 1e-6, (v, info))
v = run(ad.q3, sweep("c1s-400", GM, thin=(192, 256, 384)) + prs(30000, 10000), "c1s-400")[0][0]
check("Q3 UNSCOREABLE: 3 grid points under 2 live rows", v == "UNSCOREABLE", v)
v = run(ad.q3, sweep("c1s-400", GM, thin=(192, 256)) + prs(30000, 10000), "c1s-400")[0][0]
check("Q3 two thin grid points still scoreable", v != "UNSCOREABLE", v)
v = run(ad.q3, sweep("c1s-400", GM), "c1s-400")[0][0]
check("Q3 no PRS rows: UNSCOREABLE (no honest input)", v == "UNSCOREABLE", v)
# knee at a tie with the threshold: G(n) == 0.95 Gmax counts
GT = dict(GM)
GT[512] = 0.95 * 725
v, info = run(ad.q3, sweep("c1s-400", GT) + prs(30000, 10000), "c1s-400")[0]
check("Q3 knee: G == 0.95 Gmax qualifies", info["nstar"] == 512, info)
# c2 grid
GC = {64: 10, 96: 40, 128: 80, 192: 90, 256: 90, 384: 90, 512: 90, 1024: 90}
c2prs = [R("c2-100", "PRS", 42, 1, btl={"0": 10000}, rtp=20000), R("c2-100", "PRS", 7, 1, btl={"0": 10000}, rtp=20000)]
v, info = run(ad.q3, sweep("c2-100", GC) + c2prs, "c2-100")[0]
check("Q3 c2: n* = 192, ask_h = 220 -> LAW-SUFFICIENT", info["nstar"] == 192 and abs(info["ask_h"] - 220.0) < 1e-6
      and v == "LAW-SUFFICIENT", (v, info))
check("spearman: anti-monotone = -1", abs(ad.spearman([1, 2, 3, 4], [4, 3, 2, 1]) + 1) < 1e-9)
check("spearman: ties use average ranks", ad.spearman([1, 2, 3], [5, 5, 5]) is None)

# score() end to end on the synthetic ledger, void table first
allrows = sweep("c1s-400", GM) + prs(30000, 10000) + good + [R("c2-100", "MAIN", status="WITNESS-FAIL", problems=["x-problem"])]
buf = []
with tempfile.TemporaryDirectory() as td:
    path = os.path.join(td, "ad.log")
    import json
    with open(path, "w") as fh:
        for r in allrows:
            fh.write("ADROW " + json.dumps(r) + "\n")
    ad.score([path], buf.append)
txt = "\n".join(buf)
check("score: ROWS and VOID tables precede the readouts",
      txt.index("ROWS by") < txt.index("VOID TABLE") < txt.index("== Q1") < txt.index("== Q2") < txt.index("== Q3"))
check("score: void row listed with its problem", "c2-100 MAIN s42 r1: WITNESS-FAIL x-problem" in txt)
check("score: verdict lines present", "MECHANISM-CONFIRMED" in txt and "HEADROOM-MISSING" in txt)
buf = []
with tempfile.TemporaryDirectory() as td:
    path = os.path.join(td, "empty.log")
    open(path, "w").close()
    ad.score([path], buf.append)
check("score: empty ledger does not raise", True)

# check / smoke
with tempfile.TemporaryDirectory() as td:
    path = os.path.join(td, "ad.log")
    with open(path, "w") as fh:
        fh.write("ADROW {}\n" * 64 + "AD-BATTERY-DONE tag=ad blocks=3 x\n")
    check("check: 64 rows + done line passes", ad.check(path) == 0)
    with open(path, "w") as fh:
        fh.write("ADROW {}\n" * 63 + "AD-BATTERY-DONE tag=ad blocks=3 x\n")
    check("check: 63 rows fails", ad.check(path) == 1)
    path2 = os.path.join(td, "smoke.log")
    sm = [mk("MAIN"), mk("PRS", sampler=True), mk("S256", cell="c2-100", n=256, eng=False)]
    with open(path2, "w") as fh:
        for r in sm:
            fh.write("ADROW " + json.dumps(r, sort_keys=True) + "\n")
    check("smoke: three live rows pass", ad.smoke(path2) == 0)
    sm[1] = mk("PRS", sampler=False)
    with open(path2, "w") as fh:
        for r in sm:
            fh.write("ADROW " + json.dumps(r, sort_keys=True) + "\n")
    check("smoke: a witness-failed row fails", ad.smoke(path2) == 1)

print("FAILED %d" % fails if fails else "ALL OK")
sys.exit(1 if fails else 0)
