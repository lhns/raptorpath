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
def mk(t, gate, hb, n, b95=52.0, beta=53.8, batch=8.0, wakes=4, acks=50):
    return (f"[HOFF] t={t:.1f}s gate={gate} owner_timer_wakes={wakes} probe_acks={acks} tau_floor_bind=0.40 p0[B=30/{b95}/56 n={n} dgq=20 F=45 win=60 R=11500 tau_o=2.00ms(meas) tau_p=1.00ms l_o=0.30ms "
            f"l_p=0.20ms L=3.50ms beta={beta} hand_bind={hb:.3f} spare_i=0 spare_bind=0.550 reopen=0.500/3.000ms/4 starve=0.010 app_limited=0.050 batch={batch} g_batch=1.20ms phat=1.05ms fifo_wait_resend=2.50ms/3] infl_vs_out=900/880/x1.02 store_pin=0.10")


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
def row(arm, rep, gate, hb, n=400, b95=52.0, gp=95.0, cpu_srv_gb=None, coded=None, cod_src=None, rr=None, acks=900, gate_echo="1", status="LIVE", batch=8.0):
    ls = [mk(2.0 + 0.25 * k, gate, hb, n, b95=b95, batch=batch, acks=acks) for k in range(8)]
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
check("rule 'gate executes': B p95 above beta by more than one batch (70 - 53.8 = 16.2 > 8) fires STOP68 B-over-beta; within one batch (b95 = 60) does not",
      any(s.startswith("B-over-beta") for s in stops) and not any(s.startswith("B-over-beta") for s in run_rules([row("GK2", 1, "on", 0.62, b95=60.0)])[0]), stops)
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
    check("the report runs on a ledger, prints ROW68 / MED68 / RATIO68 lines and the three UNBUILT68 cells, and never claims them cleared",
          rc == 0 and txt.count("ROW68 ") == 9 and "RATIO68 K2 goodput: R4/BASE=0.989 GATE/BASE=0.983 GATE/R4=0.994" in txt and txt.count("UNBUILT68 ") == 3 and "no row ran, nothing read, nothing cleared" in txt, txt[-1500:])
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
print("FAILED %d" % fails if fails else "ALL OK")
sys.exit(1 if fails else 0)
