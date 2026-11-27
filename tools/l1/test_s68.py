#!/usr/bin/env python3
"""Tests for status 68's [HOFF] parser, row readouts and report (tools/l1/s68_report.py).

  1. the parser on fixture lines in the engine's format (hoff.rs), including UNKNOWN extra fields at the top level and inside a path block (the `qdisc=` another worker is adding), `-` values, `inf`, triples and tags;
  2. the row readouts: the gate token, the refusal count (sum of hand_bind x n), the B-over-beta excess, the QCC counters;
  3. the report's stop rules, each fed a row that must fire it and a row that must not (the rule's EXECUTION, not only its text), and the no-label contract: no PASS / FAIL / VERDICT line is ever printed.
Run: python3 test_s68.py"""
import io
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
for _k in ("LD_PLAN", "LD_SHA", "LD_SHA_BASE"):
    os.environ.pop(_k, None)
import plan68  # noqa: E402
import s68_report as sr  # noqa: E402

fails = 0


def check(name, ok, detail=""):
    global fails
    print(("ok   " if ok else "FAIL ") + name + ((" -- " + str(detail)[:1500]) if not ok else ""))
    if not ok:
        fails += 1


P0 = ("p0[B=30/52/56 n=400 dgq=20 F=45 win=60 R=11500 tau_o=2.00ms(meas) tau_p=1.00ms l_o=0.30ms l_p=0.20ms L=3.50ms beta=53.8 hand_bind=0.620 spare_i=0 spare_bind=0.550 "
      "reopen=0.500/3.000ms/4 starve=0.010 app_limited=0.050 batch=8.00 g_batch=1.20ms phat=1.05ms fifo_wait_resend=2.50ms/3]")
LINE = f"[HOFF] t=12.5s gate=on owner_timer_wakes=41 probe_acks=900 tau_floor_bind=0.40 {P0} infl_vs_out=900/880/x1.02 store_pin=0.10"

# ── 1. the parser ──
d = sr.parse_hoff_line(LINE)
p = d["paths"][0]
check("a registered-format line parses: top-level fields", d is not None and d["t"] == 12.5 and d["gate"] == "on" and d["owner_timer_wakes"] == 41 and d["probe_acks"] == 900 and d["store_pin"] == 0.10 and d["infl_ratio"] == 1.02, d)
check("a path block parses: B triple, tagged tau_o, beta, hand_bind, spare_i, reopen and fifo lists",
      p["B"] == [30, 52, 56] and (p["B_p50"], p["B_p95"], p["B_max"]) == (30, 52, 56) and p["tau_o"] == 2.0 and p["tau_o_tag"] == "meas" and p["beta"] == 53.8 and p["hand_bind"] == 0.62 and p["spare_i"] == 0
      and p["reopen"] == [0.5, 3.0, 4] and p["fifo_wait_resend"] == [2.5, 3] and p["n"] == 400 and p["batch"] == 8.0, p)
EXTRA = LINE.replace("tau_floor_bind=0.40 ", "tau_floor_bind=0.40 qdisc=fq ").replace("fifo_wait_resend=2.50ms/3]", "fifo_wait_resend=2.50ms/3 qdisc=netem_100mbit_5ms]")
e = sr.parse_hoff_line(EXTRA)
check("an UNKNOWN extra field at the top level and inside a path block is tolerated: every known field reads as before, the extras are kept and ignored",
      e is not None and e["qdisc"] == "fq" and e["paths"][0]["qdisc"] == "netem_100mbit_5ms" and e["paths"][0]["beta"] == 53.8 and e["paths"][0]["fifo_wait_resend"] == [2.5, 3] and e["gate"] == "on" and e["probe_acks"] == 900
      and all(e["paths"][0][k] == p[k] for k in p), e)
E2 = LINE.replace(P0, P0.replace("]", " mystery=1/2/3 note=a-b]")).replace("store_pin=0.10", "store_pin=0.10 newfield=7")
e2 = sr.parse_hoff_line(E2)
check("other unknown tokens (a triple, a word, a trailing top-level field) do not disturb the known ones", e2 is not None and e2["paths"][0]["mystery"] == [1, 2, 3] and e2["newfield"] == 7 and e2["store_pin"] == 0.10 and e2["paths"][0]["hand_bind"] == 0.62)
NAN = LINE.replace("beta=53.8", "beta=-").replace("tau_o=2.00ms(meas)", "tau_o=2.00ms(assumed)").replace("spare_i=0", "spare_i=inf")
n = sr.parse_hoff_line(NAN)["paths"][0]
check("`-` reads None, `inf` reads infinity, the (assumed) tag is kept", n["beta"] is None and n["spare_i"] == float("inf") and n["tau_o_tag"] == "assumed")
two = f"[HOFF] t=3.0s gate=off owner_timer_wakes=0 probe_acks=5 tau_floor_bind=1.00 {P0} {P0.replace('p0[', 'p1[')} infl_vs_out=-/0/x- store_pin=0.00"
t2 = sr.parse_hoff_line(two)
check("two path blocks, gate=off, a `-` ratio", sorted(t2["paths"]) == [0, 1] and t2["gate"] == "off" and t2["infl_ratio"] is None, t2)
check("a line without the tag, a truncated line and garbage are not lines (None), never an exception", sr.parse_hoff_line("[DIAG] t=1s") is None and sr.parse_hoff_line("[HOFF] t=") is None and sr.parse_hoff_line("[HOFF] p0[B=1/2") is None
      and sr.parse_hoff(["x", LINE, "[HOFF] junk"]) == [sr.parse_hoff_line(LINE)])
ANSI = "\x1b[2m2026-11-26T10:00:00Z\x1b[0m INFO " + LINE
check("a line carrying the tracing prefix and colour codes parses", sr.parse_hoff_line(ANSI)["t"] == 12.5)

# ── 2. the row readouts ──
def mk(t, gate, hb, n, b95=52.0, beta=53.8, batch=8.0, wakes=4, acks=50, wit=""):
    return (f"[HOFF] t={t:.1f}s gate={gate} owner_timer_wakes={wakes} probe_acks={acks} tau_floor_bind=0.40 p0[B=30/{b95}/56 n={n} dgq=20 F=45 win=60 R=11500 tau_o=2.00ms(meas) tau_p=1.00ms l_o=0.30ms "
            f"l_p=0.20ms L=3.50ms beta={beta} hand_bind={hb:.3f} spare_i=0 spare_bind=0.550 reopen=0.500/3.000ms/4 starve=0.010 app_limited=0.050 batch={batch} g_batch=1.20ms phat=1.05ms fifo_wait_resend=2.50ms/3{wit}] infl_vs_out=900/880/x1.02 store_pin=0.10")


WIT = " fs_over={o} fs_bypass=0/1/0/2 ex_max=46/37/9 ex_n=4/2/3 fs_n=9 beta_q=40.1/53.8/60.2 own_gap_max=7000us pend_max=12"
lines = [mk(0.5, "on", 0.9, 400)] + [mk(2.0 + 0.25 * k, "on", 0.62, 400) for k in range(8)]
h = sr.hoff_summary(lines)
check("the summary skips the warm-up windows (t < 2 s), reads the last gate token and the end-of-run counters", h["n_lines"] == 9 and h["n_post"] == 8 and h["gate"] == "on" and h["probe_acks"] == 50 and h["owner_timer_wakes"] == 4, h)
check("first-send refusals = sum of hand_bind x n over the windows and paths: 8 x round(0.62 x 400) = 1984", h["refusals"] == 1984 and h["paths"][0]["refusals"] == 1984 and h["paths"][0]["hand_bind_med"] == 0.62, h["paths"][0])
check("the B bound: max over windows of (B p95 - beta) = 52 - 53.8 < 0; a window with B p95 above beta shows its excess", h["paths"][0]["b_excess_max"] < 0
      and sr.hoff_summary([mk(2.0, "on", 0.6, 400, b95=70.0)])["paths"][0]["b_excess_max"] == 70.0 - 53.8)
check("a gate=off window's hand_bind is the shadow model's reading and refused nothing: it counts for no refusal", sr.hoff_summary([mk(2.0, "off", 0.9, 400), mk(2.25, "off", 0.9, 400)])["refusals"] == 0
      and sr.hoff_summary([mk(2.0, "off", 0.9, 400), mk(2.25, "on", 0.5, 400)])["refusals"] == 200)
q = sr.qcc_last(["[QCC] t=3.0s p0:qm=B qrt_us=2000 qsr_us=4000 qbw=104000000 qmin_us=1900 qcns=900/10/880 qrn=2 qrus=30000 qrb=1 qab=0 qage_us=100"])
check("the last [QCC] line's ProbeRtt counters and its qcns read count are read per path; BASE (no [QCC]) reads {}", q == {0: {"qcns_reads": 10, "qrn": 2, "qrus": 30000, "qrb": 1, "qab": 0}} and sr.qcc_last(["x"]) == {})
tm = sr.row_metrics(["[GATES] RWM_HOFF_GATE=1"] + lines, ["[GATES] RWM_HOFF_GATE=1"], [], 600_000_000, "c2-100", "GK2", 90.0, row={"cpu_cli": 12.0, "cpu_srv": 6.0, "rtt_wrtt_med": [20.0]})
check("row_metrics: the gate echo on both ends, the cpu per GB from the row, never raising", tm["gate_echo_cli"] == "1" and tm["gate_echo_srv"] == "1" and abs(tm["cpu_cli_gb"] - 20.0) < 1e-9 and abs(tm["cpu_srv_gb"] - 10.0) < 1e-9 and tm["rtt_wrtt_med"] == [20.0], tm)
check("row_metrics on empty logs does not raise and reports no [HOFF] lines", sr.row_metrics([], [], [])["hoff"]["n_lines"] == 0)

# ── 3. the report's rules ──
def row(arm, rep, gate, hb, n=400, b95=52.0, gp=95.0, cpu_srv_gb=None, coded=None, cod_src=None, rr=None, acks=900, gate_echo="1", status="LIVE", batch=8.0, fs_over=None):
    ls = [mk(2.0 + 0.25 * k, gate, hb, n, b95=b95, batch=batch, acks=acks, wit="" if fs_over is None else WIT.format(o=fs_over)) for k in range(8)]
    t = sr.row_metrics(["[GATES] RWM_HOFF_GATE=" + gate_echo] + ls, [], [], 600_000_000, "c2-100", arm, gp)
    t["cpu_srv_gb"], t["coded_share"], t["cod_src"], t["rr"] = cpu_srv_gb, coded, cod_src, rr
    return {"cell": "c2-100", "arm": arm, "seed": "42", "rep": rep, "status": status, "mbps": gp, "t68": t, "bin_sha": "ab" * 32}


def run_rules(rows):
    groups = sr._by(rows)
    out, stops = [], []
    sr._stop_rules(groups, lambda m: (stops.append(m), out.append("STOP68 " + m)), out.append)
    sr._witnesses(groups, lambda m: (stops.append(m), out.append("STOP68 " + m)), out.append)
    return stops, out


good = [row("BK2", 1, "off", 0.1), row("RK2", 1, "off", 0.1), row("GK2", 1, "on", 0.62)]
stops, out = run_rules(good)
check("a healthy trio (GATE binds, gate=on, probe acks, B within beta + one batch, refusals > 0): no STOP68, the witness line reads refusals > 0",
      stops == [] and any(ln.startswith("WITNESS68") and "total=" in ln and "total=0 " not in ln for ln in out) and any(ln.startswith("UNWITNESSED68 congestion_state()") for ln in out), (stops, out[-3:]))
stops, _ = run_rules([row("GK2", 1, "off", 0.0)])
check("rule 'gate executes': a GATE row printing gate=off fires STOP68 gate-not-on (and, with no refusal at all, first-send-refusals-zero and hand_bind-c2-bulk)",
      any(s.startswith("gate-not-on GK2") for s in stops) and any(s.startswith("first-send-refusals-zero") for s in stops) and any(s.startswith("hand_bind-c2-bulk") for s in stops), stops)
stops, _ = run_rules([row("GK2", 1, "on", 0.62, gate_echo="0")])
check("rule 'gate executes': a GATE row whose [GATES] echo says RWM_HOFF_GATE=0 fires STOP68 gate-echo", any(s.startswith("gate-echo GK2") for s in stops), stops)
stops, _ = run_rules([row("GK2", 1, "on", 0.62, acks=0)])
check("rule 'gate executes': probe_acks = 0 fires STOP68 routeprobe-reads-zero", any(s.startswith("routeprobe-reads-zero") for s in stops), stops)
stops, _ = run_rules([row("GK2", 1, "on", 0.4)])
check("rule 'gate executes': hand_bind <= 0.5 at c2 Bulk fires STOP68 hand_bind-c2-bulk (0.62 does not, above)", any(s.startswith("hand_bind-c2-bulk") for s in stops), stops)
stops, _ = run_rules([row("GK2", 1, "on", 0.62, b95=70.0)])
stops, out = run_rules([row("GK2", 1, "on", 0.62, b95=70.0)])
check("B p95 above beta by more than one batch (70 - 53.8 = 16.2 > 8) is REPORTED ONLY now: READ68 b-over-beta, no STOP68; within one batch (b95 = 60) nothing is printed",
      any(ln.startswith("READ68 b-over-beta GK2") for ln in out) and not any("B-over-beta" in s or "b-over-beta" in s for s in stops)
      and not any("b-over-beta" in ln for ln in run_rules([row("GK2", 1, "on", 0.62, b95=60.0)])[1]), (stops, out))
stops, _ = run_rules([row("RK2", 1, "on", 0.1)])
check("an R4 row printing gate=on fires STOP68 r4-gate-on (the arm must be off)", any(s.startswith("r4-gate-on") for s in stops), stops)
a2 = [row("RA2", 1, "off", 0.1, coded=0.20), row("GA2", 1, "on", 0.62, coded=0.25)]
a2 = [dict(r, cell="c2-100") for r in a2]
stops, out = run_rules(a2)
check("rule 'clamp at Auto': a coded share on GATE >= R4's while hand_bind > 0.5 fires STOP68 clamp-inert (0.25 >= 0.20)", any(s.startswith("clamp-inert A2") for s in stops), (stops, out))
stops, out = run_rules([row("RA2", 1, "off", 0.1, coded=0.20), row("GA2", 1, "on", 0.62, coded=0.05)])
check("rule 'clamp at Auto': a coded share on GATE under R4's (the clamp works) does not fire", not any(s.startswith("clamp-inert") for s in stops), stops)
refused = [row("GK2", 1, "on", 0.0)]
stops, out = run_rules(refused)
check("the execution witness: no refusal on any GATE row is STOP68 first-send-refusals-zero, and the WITNESS68 line prints total=0", any(s.startswith("first-send-refusals-zero") for s in stops) and any("WITNESS68" in ln and "total=0" in ln for ln in out), (stops, out))

# the pacer-dependent rules run only when their cell is built: flip LR and PC to built for these tests
built0, unbuilt0 = plan68.BUILT, plan68.UNBUILT
plan68.UNBUILT = ("BU",)
plan68.BUILT = tuple(c for c in plan68.CELL_ORDER if c not in plan68.UNBUILT)


def prow(arm, rep, gate, hb, **kw):
    r = row(arm, rep, gate, hb, **kw)
    r["cell"] = "c2-100"
    return r


stops, out = run_rules([prow("GPC", 1, "on", 0.62), prow("RPC", 1, "off", 0.1)])
spare_hi = [prow("GPC", 1, "on", 0.62)]
spare_hi[0]["t68"]["hoff"]["paths"][0]["spare_bind_mean"] = 0.2
stops_hi, _ = run_rules(spare_hi)
check("rule 'clamp with headroom' (when PC is built): spare_bind > 0.05 fires STOP68 clamp-with-headroom; the coded-share comparison prints A/A-UNRESOLVED and is not evaluated",
      any(s.startswith("clamp-with-headroom") for s in stops_hi) and any("A/A-UNRESOLVED" in ln for ln in out), (stops_hi, out))
lr = [prow("GLR", 1, "on", 0.62, cod_src=0.30, rr=0.25)]
lr_ok = [prow("GLR", 1, "on", 0.62, cod_src=0.251, rr=0.25)]
check("rule 'low rate' (when LR is built): repairs per source outside r +- 5 % fires STOP68 low-rate (0.30 vs 0.25); inside it does not; B max > 2 fires it too (B max 56 here)",
      any("repairs per source" in s for s in run_rules(lr)[0]) and not any("repairs per source" in s for s in run_rules(lr_ok)[0]) and any("B max" in s for s in run_rules(lr_ok)[0]))
plan68.BUILT, plan68.UNBUILT = built0, unbuilt0

# the whole report on a synthetic ledger, with UNBUILT cells and the no-label contract
import tempfile  # noqa: E402
import json  # noqa: E402

rows = [row(a, r, g, hb, gp=gp) for r in (1, 2, 3) for a, g, hb, gp in (("BK2", "off", 0.1, 90.0), ("RK2", "off", 0.1, 89.0), ("GK2", "on", 0.62, 88.5))]
for r in rows:
    r["bytes"] = 600_000_000
with tempfile.TemporaryDirectory() as td:
    led = os.path.join(td, "ld.log")
    with open(led, "w", newline="\n") as fh:
        for r in rows:
            fh.write("P38ROW " + json.dumps(r) + "\n")
    buf = io.StringIO()
    rc = sr.report(led, out=lambda s: buf.write(s + "\n"))
    txt = buf.getvalue()
    check("the report runs on a ledger, prints ROW68 / MED68 / RATIO68 lines and the UNBUILT68 cell (BU), and never claims it cleared",
          rc == 0 and txt.count("ROW68 ") == 9 and "RATIO68 K2 goodput: R4/BASE=0.989 GATE/BASE=0.983 GATE/R4=0.994 Q/BASE=-" in txt and txt.count("UNBUILT68 ") == 1 and "no row ran, nothing read, nothing cleared" in txt, txt[-1500:])
    check("DIAG68 (not STOP68) is the token of the 'diagnose / attribute' rules: c2 goodput GATE/BASE 0.983 < 0.995 prints DIAG68 c2-goodput", "DIAG68 c2-goodput" in txt and "STOP68 c2-goodput" not in txt, txt[-1200:])
    check("no verdict label: no PASS / FAIL / VERDICT token in the report, and its last line is REPORT68 REPORTED-ONLY", not any(tok in txt for tok in ("PASS", "FAIL", "VERDICT")) and txt.strip().splitlines()[-1].startswith("REPORT68 REPORTED-ONLY"), txt[-600:])
    stub = os.path.join(td, "smoke.log")
    with open(stub, "w", newline="\n") as fh:
        for r in (dict(rows[0], rep=1), dict(rows[1], rep=1), dict(rows[2], rep=1)):
            fh.write("P38ROW " + json.dumps(r) + "\n")
    sb = io.StringIO()
    src = sr.smoke("smoke", stub, out=lambda s: sb.write(s + "\n"))
    check("the smoke check passes on one row per arm (each with its t68, seat binary and [HOFF] lines) and prints SMOKE-PASS", src == 0 and "SMOKE-PASS" in sb.getvalue(), sb.getvalue())
    sb2 = io.StringIO()
    src2 = sr.smoke("smoke", stub + ".missing", out=lambda s: sb2.write(s + "\n"))
    check("the smoke check fails (SMOKE-FAIL) when a row is missing", src2 == 1 and "SMOKE-FAIL" in sb2.getvalue())

# ── 4. the witness counters (the gate's step-1 rerun), the Q arm, the in-order tail ──
W = sr.parse_hoff_line(LINE.replace(P0, P0.replace("]", WIT.format(o=0) + "]")))["paths"][0]
check("the witness fields parse: fs_over, the four-way bypass list, ex_max and ex_n triples, beta_q, own_gap_max (the `us` unit is dropped) and pend_max",
      W["fs_over"] == 0 and W["fs_bypass"] == [0, 1, 0, 2] and W["ex_max"] == [46, 37, 9] and W["ex_n"] == [4, 2, 3] and W["fs_n"] == 9 and W["beta_q"] == [40.1, 53.8, 60.2] and W["own_gap_max"] == 7000 and W["pend_max"] == 12, W)
ps = sr.path_summary([dict(W, _gate="on"), dict(W, _gate="on", fs_over=3, ex_max=[60, 50, 10])])
check("path_summary sums the counters over the windows, keeps the worst ex_max, the beta_q spread and the maxima",
      ps["fs_over"] == 3 and ps["fs_bypass"] == [0, 2, 0, 4] and ps["ex_max"] == [60, 50, 10] and ps["ex_n"] == [8, 4, 6] and ps["fs_n"] == 18 and ps["beta_p5_min"] == 40.1 and ps["beta_p95_max"] == 60.2 and ps["own_gap_max"] == 7000 and ps["pend_max"] == 12, ps)
old = sr.path_summary([dict(sr.parse_hoff_line(LINE)["paths"][0], _gate="on")])
check("an old ledger (no witness fields) summarizes with None and does not raise", old["fs_over"] is None and old["fs_bypass"] is None and old["ex_max"] is None and old["own_gap_max"] is None and old["pend_max"] is None, old)
stops, out = run_rules([row("GK2", 1, "on", 0.62, fs_over=2)])
check("STOP68 fs-over fires on a GATE row with fs_over > 0 (2 per window x 8 windows = 16)", any(s.startswith("fs-over GK2 r1: 16 ") for s in stops), stops)
stops, out = run_rules([row("QK2", 1, "on", 0.62, fs_over=1)])
check("STOP68 fs-over fires on a Q (gate alone) row too", any(s.startswith("fs-over QK2 r1") for s in stops), stops)
check("fs_over = 0 fires nothing; a ledger without the field fires nothing", not any(s.startswith("fs-over") for s in run_rules([row("GK2", 1, "on", 0.62, fs_over=0), row("QK2", 1, "on", 0.62, fs_over=0)])[0])
      and not any(s.startswith("fs-over") for s in run_rules([row("GK2", 1, "on", 0.62), row("QK2", 1, "on", 0.62)])[0]))
stops, out = run_rules([row("QK2", 1, "off", 0.0)])
check("a Q row printing gate=off fires STOP68 gate-not-on (the arm is the gate alone: it must bind); a healthy Q row fires nothing", any(s.startswith("gate-not-on QK2") for s in stops)
      and run_rules([row("BK2", 1, "off", 0.1), row("GK2", 1, "on", 0.62), row("QK2", 1, "on", 0.62)])[0] == [])
check("the Q arm exists on the K cells only, runs the CAND seat with the gate and quinn's own BBR",
      plan68.arm_parse("QK2") == ("Q", "K2") and plan68.arm_parse("QLR") == (None, None) and plan68.seat_of("QA2") == "CAND"
      and plan68.arm_env("c2-100", "QK2") == ["RWM_HOFF_GATE=1", "RWM_DIAG=1"] and "QK8" in plan68.all_arms() and "QPC" not in plan68.all_arms())
ls = [f"[DIAG] t={t}.0s src=10 cum={c}/0/{a}" for t, c, a in ((1, 100, 100), (2, 200, 200), (3, 300, 300), (4, 400, 300), (5, 500, 300))]
tm = sr.row_metrics(ls, [], [])
check("row_metrics carries the in-order delay tail beside the mean: delays 0 0 0 1 2 s -> mean 600, p95 / p99 / max 2000 ms",
      abs(tm["inorder_mean_ms"] - 600.0) < 1e-6 and abs(tm["inorder_p95_ms"] - 2000.0) < 1e-6 and abs(tm["inorder_p99_ms"] - 2000.0) < 1e-6 and abs(tm["inorder_max_ms"] - 2000.0) < 1e-6, tm)
qrows = [row(a, r, g, hb, gp=gp, fs_over=0) for r in (1, 2, 3) for a, g, hb, gp in (("BK2", "off", 0.1, 90.0), ("RK2", "off", 0.1, 89.0), ("GK2", "on", 0.62, 88.5), ("QK2", "on", 0.62, 45.0))]
for r in qrows:
    r["bytes"] = 600_000_000
with tempfile.TemporaryDirectory() as td:
    led = os.path.join(td, "ld.log")
    with open(led, "w", newline="\n") as fh:
        for r in qrows:
            fh.write("P38ROW " + json.dumps(r) + "\n")
    buf = io.StringIO()
    sr.report(led, out=lambda s: buf.write(s + "\n"))
    txt = buf.getvalue()
    check("the report prints Q/BASE beside GATE/R4, the witness fields per row, the per-cell WIT68 summary and the in-order tail",
          "RATIO68 K2 goodput: R4/BASE=0.989 GATE/BASE=0.983 GATE/R4=0.994 Q/BASE=0.500" in txt and "fs_over=0 fs_n=72 fs_bypass=0/8/0/16 ex_max=46/37/9 ex_n=32/16/24" in txt and "WIT68 K2 GATE" in txt and "WIT68 K2 GATE-ALONE" in txt
          and "inorder_ms mean/p95/p99/max=" in txt and "RATIO68 K2 inorder_p99:" in txt and "own_gap_max_us=7000" in txt and "pend_max=12" in txt, txt[-2500:])
    # the rerun's K cells have no BASE / R4 rows: a reference ledger lends them
    led2 = os.path.join(td, "ref.log")
    with open(led2, "w", newline="\n") as fh:
        for r in qrows:
            if r["arm"][0] in "BR":
                fh.write("P38ROW " + json.dumps(r) + "\n")
    led3 = os.path.join(td, "main.log")
    with open(led3, "w", newline="\n") as fh:
        for r in qrows:
            if r["arm"][0] in "GQ":
                fh.write("P38ROW " + json.dumps(r) + "\n")
    b2, b3 = io.StringIO(), io.StringIO()
    sr.report(led3, out=lambda s: b2.write(s + "\n"))
    sr.report(led3, out=lambda s: b3.write(s + "\n"), ref=led2)
    check("without a reference ledger the K-cell ratios against BASE / R4 read `-`; with one they are computed (and no stop rule reads the reference rows)",
          "RATIO68 K2 goodput: R4/BASE=- GATE/BASE=- GATE/R4=- Q/BASE=-" in b2.getvalue() and "REF68 6 " in b3.getvalue() and "RATIO68 K2 goodput: R4/BASE=0.989 GATE/BASE=0.983 GATE/R4=0.994 Q/BASE=0.500" in b3.getvalue(), (b2.getvalue()[-800:], b3.getvalue()[:600]))

# ── 6. the engine's own line: printed verbatim by hoff_gate_tests::hoff_line_prints_the_witness_fields_only_when_armed (raptorpath, handoff-gate) ──
ENGINE = ("[HOFF] t=0.3s gate=on owner_timer_wakes=0 probe_acks=0 tau_floor_bind=1.00 p0[B=10/10/10 n=1 dgq=0 F=0 win=280000 R=15909091 tau_o=2.00ms(meas) tau_p=2.06ms l_o=0.02ms l_p=0.10ms L=4.17ms beta=53.83 "
          "hand_bind=0.000 spare_i=inf spare_bind=0.000 reopen=0.000/0.000ms/0 starve=0.000 qdisc=0 app_limited=- batch=- g_batch=-ms phat=-ms fifo_wait_resend=0.000ms/0 fs_over=0 fs_bypass=0/1/0/0 ex_max=46/0/0 "
          "ex_n=0/0/0 fs_n=0 beta_q=53.83/53.83/53.83 own_gap_max=7000us pend_max=1010] infl_vs_out=40/10/x4.000 store_pin=0.00")
ep = (sr.parse_hoff_line(ENGINE) or {"paths": {}})["paths"].get(0, {})
check("the engine's verbatim [HOFF] line parses with every witness field (the format is not hand-matched)",
      ep.get("fs_over") == 0 and ep.get("fs_bypass") == [0, 1, 0, 0] and ep.get("ex_max") == [46, 0, 0] and ep.get("ex_n") == [0, 0, 0] and ep.get("fs_n") == 0 and ep.get("beta_q") == [53.83, 53.83, 53.83]
      and ep.get("own_gap_max") == 7000 and ep.get("pend_max") == 1010 and ep.get("beta") == 53.83, ep)

# ── 5. review fixes: warm-up windows, reference rows, every path, the env var ──
warm = [mk(0.5, "on", 0.62, 400, wit=WIT.format(o=4))] + [mk(2.0 + 0.25 * k, "on", 0.62, 400, wit=WIT.format(o=0)) for k in range(4)]
hs = sr.hoff_summary(warm)
check("the witness counters sum over ALL windows, warm-up included (a leak or a cold-start bypass at t = 0.5 s counts), while the other readouts skip the warm-up",
      hs["paths"][0]["fs_over"] == 4 and hs["paths"][0]["fs_bypass"] == [0, 5, 0, 10] and hs["n_post"] == 4 and hs["paths"][0]["n_win"] == 4, hs["paths"][0])
leak = row("GK2", 1, "on", 0.62)
leak["t68"]["hoff"] = sr.hoff_summary(warm)
check("a leak in the warm-up window alone fires STOP68 fs-over", any(s_.startswith("fs-over GK2") for s_ in run_rules([leak])[0]))
two = {"t68": {"hoff": {"paths": {"0": {"fs_over": 0, "fs_n": 5}, "1": {"fs_over": 7, "fs_n": 9}}}}}
check("the per-row witness fields print for EVERY path (the slow leg of K8 is the one to read)", "w0[fs_over=0 fs_n=5" in sr._wit_paths(two) and "w1[fs_over=7 fs_n=9" in sr._wit_paths(two), sr._wit_paths(two))
with tempfile.TemporaryDirectory() as td:
    main_l, ref_l = os.path.join(td, "main.log"), os.path.join(td, "ref.log")
    bad_ref = [dict(row("RK2", r, "on", 0.62, fs_over=0), bytes=600_000_000) for r in (1, 2, 3)]      # an R4 reference row that prints gate=on: the r4-gate-on rule must NOT read it
    good = [dict(row(a, r, g, hb, fs_over=0), bytes=600_000_000) for r in (1, 2, 3) for a, g, hb in (("GK2", "on", 0.62), ("QK2", "on", 0.62))]
    for pth, rs_ in ((main_l, good), (ref_l, bad_ref)):
        with open(pth, "w", newline="\n") as fh:
            for r in rs_:
                fh.write("P38ROW " + json.dumps(r) + "\n")
    b4 = io.StringIO()
    sr.report(main_l, out=lambda s_: b4.write(s_ + "\n"), ref=ref_l)
    check("reference rows feed the ratios only: an R4 reference row printing gate=on does not fire r4-gate-on, and the ratios use it", "STOP68 r4-gate-on" not in b4.getvalue() and "REF68 3 " in b4.getvalue(), b4.getvalue()[:700])
    os.environ["LD_REF68"] = ref_l
    b5 = io.StringIO()
    sr.report(main_l, out=lambda s_: b5.write(s_ + "\n"))
    del os.environ["LD_REF68"]
    check("LD_REF68 names the reference ledger when the report is called without one (run_all's call)", "REF68 3 " in b5.getvalue())
Wc = sr.parse_hoff_line(LINE.replace(P0, P0.replace("]", WIT.format(o=3).replace("fs_over=3", "fs_over=3 fs_over_cs=2/1") + "]")))["paths"][0]
psc = sr.path_summary([dict(Wc, _gate="on"), dict(Wc, _gate="on")])
check("fs_over_cs (closed/spent) parses and sums over windows; an old ledger without it reads None", Wc.get("fs_over_cs") == [2, 1] and psc["fs_over_cs"] == [4, 2] and old["fs_over_cs"] is None, (Wc.get("fs_over_cs"), psc.get("fs_over_cs")))
# ── 6. plan 68s: the A/A twin T, the burst cell, PH ──
os.environ["LD_PLAN"] = "68s"
stops, out = run_rules([row("TPH", 1, "on", 0.62, fs_over=2)])
os.environ["LD_PLAN"] = "68"
check("STOP68 fs-over fires on a T (GATE twin) row too", any(s_.startswith("fs-over TPH r1") for s_ in stops), stops)
check("T exists on PH only, runs the CAND seat with the gate and quinn's bbr_rs override like GATE", plan68.arm_parse("TPH") == ("T", "PH") and plan68.arm_parse("TK2") == (None, None) and plan68.seat_of("TPH") == "CAND"
      and plan68.arm_env("c2-100", "TPH") == plan68.arm_env("c2-100", "GPH") and "TPH" in plan68.all_arms() and "TBU" not in plan68.all_arms())
os.environ["LD_PLAN"] = "68s"
ph_rows = [dict(row(a, r, g, hb, gp=gp, fs_over=0), bytes=637_500_000) for r in (1, 2, 3) for a, g, hb, gp in (("BPH", "off", 0.1, 85.0), ("GPH", "on", 0.1, 84.0), ("TPH", "on", 0.1, 84.5))]
with tempfile.TemporaryDirectory() as td:
    led = os.path.join(td, "ld.log")
    with open(led, "w", newline="\n") as fh:
        for r in ph_rows:
            fh.write("P38ROW " + json.dumps(r) + "\n")
    b6 = io.StringIO()
    sr.report(led, out=lambda s_: b6.write(s_ + "\n"))
    t6 = b6.getvalue()
    check("the 68s report lists its own cells (PH with the T arm, no UNBUILT68 line, no LR or PC cell), prints the A/A ratio GATE-AA/GATE and the PH witness line",
          "CELL68 PH " in t6 and "GATE-AA=3" in t6 and "AA68 PH goodput: GATE-AA/GATE=1.006" in t6 and "UNBUILT68" not in t6 and "READ68 PC" not in t6 and "WIT68 PH GATE-AA" in t6 and "REPORT68 REPORTED-ONLY rows=9/30" in t6, t6[:900])
os.environ["LD_PLAN"] = "68s"
bu_ok, bu_out = run_rules([row("GBU", 1, "on", 0.62, cod_src=0.95, rr=1.0), row("RBU", 1, "off", 0.1, cod_src=1.0, rr=1.0)])
bu_bad, _ = run_rules([row("GBU", 1, "on", 0.62, cod_src=0.80, rr=1.0)])
tw_off, _ = run_rules([row("TPH", 1, "off", 0.0)])
check("a T row printing gate=off fires gate-not-on (a twin without the gate would print a false A/A); a healthy twin does not",
      any(s_.startswith("gate-not-on TPH") for s_ in tw_off) and not any(s_.startswith("gate-not-on") for s_ in run_rules([row("TPH", 1, "on", 0.1)])[0]), tw_off)
check("BU: repairs per source < 0.9 r fires the registered 'limit binds' stop, 0.95 r does not; the report reads hand_bind and repairs per source and says the frame completion time has no readout",
      any(s_.startswith("bursts-with-headroom BU GBU r1") for s_ in bu_bad) and not any(s_.startswith("bursts-with-headroom") for s_ in bu_ok) and any(ln.startswith("READ68 BU hand_bind") for ln in bu_out)
      and any(ln.startswith("UNWITNESSED68 BU frame completion") for ln in bu_out), (bu_bad, bu_out[-4:]))
os.environ["LD_PLAN"] = "68"
print("FAILED %d" % fails if fails else "ALL OK")
sys.exit(1 if fails else 0)
