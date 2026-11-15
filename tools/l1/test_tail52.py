#!/usr/bin/env python3
"""Tests for the status 52 harness (plan 52, tools/l1/tail52_score.py + the place38_parse plan-52 parts): the readout parsers on synthetic and committed logs,
the DRY RUN that reproduces the committed §48b c8 numbers from docs/l1-raw/bbr48b/diag-ld-full.tgz, the arm witnesses (red first: a missing / foreign
RWM_PLACE_WDIV token voids the row), the plan table, the smoke / check / score commands, and the shell side.  Run: python3 test_tail52.py"""
import io
import json
import os
import re
import shutil
import sys
import tarfile
import tempfile
from contextlib import redirect_stdout

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
os.environ.pop("LD_PLAN", None)
_BASE_SHA_AT_START = os.environ.get("LD_BASE_SHA")      # the suite runs this file with and without LD_BASE_SHA; the tests below set it explicitly where it matters
import l1common as lc  # noqa: E402
import place38_parse as pp  # noqa: E402
import tail52_score as t52  # noqa: E402

fails = 0


def check(name, ok, detail=""):
    global fails
    print(("ok   " if ok else "FAIL ") + name + ((" -- " + str(detail)[:1200]) if not ok else ""))
    if not ok:
        fails += 1


def near(a, b, tol):
    return a is not None and abs(a - b) <= tol


# ── the parsers on synthetic lines ──
def diag(t, src, win, cum, retx, rep, seqs, on, rtt=250.0):
    s, c, a = cum
    return (f"[DIAG] t={t:.2f}s win={win}/4096 paused=0% good=100.0Mbit src={src}sym/s cod=0sym/s cum={s}/{c}/{a} cwnd=1 infl=1 np=2 rtt={rtt}ms bdp100=1sym sweeps=1 "
            f"retx={retx} mpr[rep={rep} seqs={seqs} fired={retx} supp=0/0/0] fp/on p0:{on[0]}/{on[1]} p1:{on[2]}/{on[3]}] wnd2=0/0")


check("parse_cx_leg: `0:3,1:2` -> {0: 3, 1: 2}; `-` -> {} (nothing closed); absent -> None; garbage -> None",
      t52.parse_cx_leg("0:3,1:2") == {0: 3, 1: 2} and t52.parse_cx_leg("-") == {} and t52.parse_cx_leg(None) is None and t52.parse_cx_leg("x") is None)
HOLD0 = ("[HOLD] site=sender path=0 gen=0 hn_n=10 fed=99 age_p50_us=3670016 age_p90_us=4718592 cx_n=7 cx_p50_us=229376 cx_p90_us=589824 "
         "cx_leg=0:3,1:4 fa_class=0.0625")
HOLD1 = "[HOLD] site=sender path=1 gen=0 hn_n=5 fed=50 age_p50_us=262144 age_p90_us=4718592 cx_n=2 cx_p50_us=163840 cx_p90_us=245760 cx_leg=- fa_class=0.0625"
HOLDP = "[HOLD] site=sender path=- gen=0 hn_n=0 fed=0 age_p50_us=- cx_p50_us=- cx_leg=-"
h = t52.hold_by_path([HOLD0, HOLD1, HOLDP, "[HOLD] site=receiver path=0 age_p50_us=1"])
check("hold_by_path: per original path, the pooled `path=-` line and the receiver's line are skipped; cx_leg parsed, `-` is {}",
      sorted(h) == [0, 1] and h[0]["age_p50_us"] == 3670016 and h[0]["cx_p90_us"] == 589824 and h[0]["cx_leg"] == {0: 3, 1: 4} and h[1]["cx_leg"] == {}, h)
h_old = t52.hold_by_path([HOLD0.replace(" cx_leg=0:3,1:4", "")])
check("hold_by_path: a binary without `cx_leg=` reads None (not in this binary), never an error", h_old[0]["cx_leg"] is None and h_old[0]["cx_p50_us"] == 229376)

# a synthetic row: source until t = 10, tail 2.5 s, 600 MB
T = []
retx = rep = seqs = 0
cum_s = 0
for k in range(1, 51):
    t = 0.25 * k
    src = 1000 if t <= 10.0 else 0
    cum_s += 250 if src else 0
    retx += 100
    rep += 10
    seqs += 500                                    # Nh = 50 per report
    on = [60 * k, 40 * k, 20 * k, 40 * k]            # leg 1 sends 40 / 80 of the retransmits
    win = 500 if t <= 10.0 else max(0, 3000 - int(1000 * (t - 10.0)))
    T.append(diag(t, src, win, (cum_s, 5, cum_s - 700), retx, rep, seqs, on))
DRV = ['{"bytes":600000000,"dnf":0,"mean_mbps":98.5,"median_s":12.5,"summary":true}', "    CPU: CPUSRV=30.00s CPUCLI=21.00s (srv=decoder cli=sender; whole-invocation incl warmup)"]
SRV = ["[C8CONV-R] p0:fst=1 dup=10 lead=0 unb=1/5ms p1:fst=1 dup=3 lead=0 unb=1/5ms", "[C8CONV-R] p0:fst=2 dup=700 lead=0 unb=1/5ms p1:fst=2 dup=300 lead=0 unb=1/5ms"]
m = t52.row_metrics(T + [HOLD0, HOLD1], SRV, DRV)
check("row_metrics: goodput, tail = run seconds - t(last line with src > 0) (12.5 - 10.0), H0 = win at the first tail line, frontier lag = src - ack at that line",
      m["goodput_mbps"] == 98.5 and near(m["tail_s"], 2.5, 1e-9) and m["H0"] == 3000 - 250 and m["lag_end"] == 700 and near(m["t_src_end"], 10.0, 1e-9), m)
check("row_metrics: send-leg share = on1/(on0+on1) of the last fp/on pair (2000:2000 -> 0.5), retransmits per GB, receiver duplicates, CPU per GB",
      near(m["send_leg1_share"], 0.5, 1e-9) and m["send_on"] == [2000, 2000] and near(m["retx_per_gb"], 5000 / 0.6, 1e-6) and m["rx_dup"] == 1000
      and near(m["cpu_cli_s_per_gb"], 35.0, 1e-9) and near(m["cpu_srv_s_per_gb"], 50.0, 1e-9), m)
check("row_metrics: K per 4 s bin = mu * C (mu = d(retx)/dt, C = pooled clock) and Nh = d(seqs)/d(rep); only main-phase (src > 0) bins",
      sorted(m["kbins"]) == [0, 1, 2] and near(m["kbins"][1]["Nh"], 50.0, 1e-9) and near(m["kbins"][1]["mu"], 400.0, 1.0) and near(m["kbins"][1]["K"], 400.0 * 0.25, 0.5), m["kbins"])
check("row_metrics: [HOLD] per path rides along", m["hold"][0]["cx_leg"] == {0: 3, 1: 4})
m0 = t52.row_metrics([], [], [])
check("row_metrics on empty logs: every readout None, no exception", m0["tail_s"] is None and m0["H0"] is None and m0["rx_dup"] is None and m0["hold"] == {} and m0["kbins"] == {})

# JSON round trip (the ledger): int keys become strings; the report must still read them
rt = json.loads(json.dumps(dict(m, arm="W0")))
buf = io.StringIO()
with redirect_stdout(buf):
    t52.report([rt, dict(rt)], arms=("W0",))
txt = buf.getvalue()
check("report after a JSON round trip (string keys): [HOLD] medians, cx_leg pooled counts and the K bins print",
      "first-report age p50/p90 3670/4719" in txt and "closed-by-retransmit p50/p90 229/590" in txt and "pooled {0: 6, 1: 8}" in txt and "1-4s" not in txt and "4-8s:" in txt, txt)

# ── the DRY RUN on the committed §48b logs ──
TGZ = os.path.join(HERE, "..", "..", "docs", "l1-raw", "bbr48b", "diag-ld-full.tgz")
td = tempfile.mkdtemp(prefix="tail52-")
try:
    with tarfile.open(TGZ) as tf:
        n = 0
        for mem in tf.getmembers():
            base = os.path.basename(mem.name)
            if mem.isfile() and base.startswith("c8-100@def-") and base.endswith(("-c.log", "-s.log", "-drv.out")):
                with tf.extractfile(mem) as src, open(os.path.join(td, base), "wb") as dst:
                    shutil.copyfileobj(src, dst)
                n += 1
    rows = t52.rows_from_dir(td)
    by = {a: [r for r in rows if r["arm"] == a] for a in ("BASE", "CAND")}
    check("dry run: 72 c8-100 log files unpacked, 12 BASE and 12 CAND rows", n == 72 and len(by["BASE"]) == 12 and len(by["CAND"]) == 12, (n, {k: len(v) for k, v in by.items()}))
    mean = lambda rs, k: sum(r[k] for r in rs) / len(rs)  # noqa: E731
    B, C = by["BASE"], by["CAND"]
    check("dry run, BASE: H0 = 1890 (committed tail-c8/rows_out.txt), tail 3.91 s, frontier lag at source end 73179, retx send-leg-1 share 0.58 (rtxleg/a1_out.txt)",
          near(mean(B, "H0"), 1890, 0.5) and near(mean(B, "tail_s"), 3.91, 0.005) and near(mean(B, "lag_end"), 73179, 0.5) and round(mean(B, "send_leg1_share"), 2) == 0.58,
          (mean(B, "H0"), mean(B, "tail_s"), mean(B, "lag_end"), mean(B, "send_leg1_share")))
    check("dry run, CAND: H0 = 2246, tail 4.73 s, lag 83189, send-leg-1 share 0.65",
          near(mean(C, "H0"), 2246, 0.5) and near(mean(C, "tail_s"), 4.73, 0.005) and near(mean(C, "lag_end"), 83189, 0.5) and round(mean(C, "send_leg1_share"), 2) == 0.65,
          (mean(C, "H0"), mean(C, "tail_s"), mean(C, "lag_end"), mean(C, "send_leg1_share")))
    K = t52.kbin_stat(B, "K")
    committed = [84, 86, 66, 69, 87, 102, 102, 89, 82, 70, 88]      # tail-c8/price_out.txt, BASE, bins 0-4 .. 40-44 s
    check("dry run, BASE: K = mu * C per 4 s bin reproduces price_out.txt (84 86 66 69 87 102 102 89 82 70 88)", [round(K[b]) for b in range(11)] == committed, [round(K.get(b, -1)) for b in range(11)])
    check("dry run, BASE: [HOLD] first-report age p50 at original path 0 is 3.0 s and closed-by-retransmit p50 / p90 180 / 377 ms (rtxleg/a4_out.txt); cx_leg is None (not in these binaries)",
          near(t52.hold_stat(B, 0, "age_p50_us") / 1e3, 3015, 1) and round(t52.hold_stat(B, 0, "cx_p50_us") / 1e3) == 180 and round(t52.hold_stat(B, 0, "cx_p90_us") / 1e3) == 377
          and all(r["hold"][0]["cx_leg"] is None for r in B), (t52.hold_stat(B, 0, "age_p50_us"), t52.hold_stat(B, 0, "cx_p50_us")))
    check("dry run: goodput, retransmits per GB, receiver duplicates, CPU per GB all parse on every row",
          all(r[k] is not None for r in B + C for k in ("goodput_mbps", "retx_per_gb", "rx_dup", "cpu_cli_s_per_gb", "cpu_srv_s_per_gb")))
finally:
    shutil.rmtree(td, ignore_errors=True)

# ── the plan table ──
plan = pp.plan52()
check("plan52: 30 rows = 6 groups (seeds 42, 7 x reps 1..3, rep-major) x 5 arms, n = 6 per arm, every row at c8-100 default store",
      len(plan) == 30 and pp.EXPECT_ROWS52 == 30 and all(r[0] == "c8-100" and r[1] == "def" for r in plan)
      and all(sum(1 for r in plan if r[2] == a) == 6 for a in pp.ARMS52) and [(r[3], r[4]) for r in plan][::5] == [("42", 1), ("7", 1), ("42", 2), ("7", 2), ("42", 3), ("7", 3)], plan[:6])
check("plan52: each group holds the five arms once (interleaved per block) and every arm leads one of the first five groups' rotation",
      all(sorted(r[2] for r in plan[i:i + 5]) == sorted(pp.ARMS52) for i in range(0, 30, 5)) and len({plan[i][2] for i in range(0, 30, 5)}) == 5, [plan[i][2] for i in range(0, 30, 5)])
check("plan52 smoke: one row per env arm (W0, W05, WDER), unscored", [r[2] for r in pp.plan52(smoke=True)] == ["W0", "W05", "WDER"])
bat = open(os.path.join(HERE, "loopdiag_battery.sh"), encoding="utf-8").read()
run = open(os.path.join(HERE, "loopdiag_run_all.sh"), encoding="utf-8").read()
check("the shell side agrees with the scorer: 600 MB for c8-100 only, 30-row and 3-row tables, W-arms on BASE's binary with their env, env -u of the diagnostics, the BASE preflight",
      f"c8-100) echo {pp.BYTES52} ;;" in bat and '-eq 30 ] || { echo "ABORT-PLAN52' in bat and '-eq 3 ] || { echo "ABORT-PLAN52 smoke' in bat
      and 'W0) arm_env=(RWM_PLACE_WDIV=0)' in bat and 'W05) arm_env=(RWM_PLACE_WDIV=0.5)' in bat and 'WDER) arm_env=(RWM_PLACE_WDIV_DERIVED=1)' in bat
      and 'RBIN="$BIN_BASE"; RWANT="$WANT_BASE"' in bat and "-u RWM_PLACE_WDIV -u RWM_PLACE_WDIV_DERIVED" in bat and 'grep -aqF -- " RWM_PLACE_WDIV="' in bat
      and 'P52=0; [ "$PLAN" = "52" ] && { P52=1; P38=1; P41=1; PCAND=1; }' in bat, None)
check("run_all: plan 52 = suites, guard (41c, scored), rep52 (LD_PLAN=52) in its own dir; LD_BASE_SHA required; rehearsal ends after both smokes; reported block outside the guard's ledger",
      re.search(r"^  52\) P_SMOKE=smoke52", run, re.M) and 'P52=0; [ "$PLAN" = "52" ] && { P52=1; P46=1; P41=1; }' in run and "stage guard 41c; grc=$?" in run and "stage rep52 52" in run
      and '[ "$P52" = 1 ] || [ "$P53" = 1 ] || [ "$P54B" = 1 ]; then\n  : "${LD_BASE_SHA:?' in run and '52) smk=smoke52; scr=score52; chk=check52' in run and 'rep52_ok=$ROK' in run
      and '[ "$PLAN" = "47" ] || [ "$PLAN" = "48" ] || [ "$PLAN" = "52" ] || [ "$PLAN" = "53" ] || [ "$PLAN" = "54b" ]; then\n  SD="$RUN/suites"' in run and '[ "$PLAN" = "54b" ] || {' in run, None)
check("run_all: the python gates for plan 52 include test_tail52.py", "test_bbr48_parse.py test_place38_parse.py test_tail52.py" in run)

# ── the arm witnesses (red first) ──
GATES = "[GATES] RWM_UNIFIED=1 RWM_PLACE_T_DERIVED=0 RWM_PLACE_HOL=0 RWM_PLACE_WDIV_DERIVED={d} RWM_GEN=0 RWM_DIAG=1 RWM_FDIAG=0{w}"


def gates(d="0", w=None):
    return [GATES.format(d=d, w=(f" RWM_PLACE_WDIV={w}" if w is not None else ""))]


def wp(arm, cli, srv=None):
    return pp.wdiv_problems(arm, cli, srv if srv is not None else cli)


check("witness: BASE / CAND with no wdiv token and DERIVED=0 hold", wp("BASE", gates()) == [] and wp("CAND", gates()) == [])
check("witness: a binary that predates the gate (no DERIVED token at all) holds for BASE", wp("BASE", ["[GATES] RWM_UNIFIED=1"]) == [])
check("witness: W0 reads RWM_PLACE_WDIV=0, W05 reads 0.5 (numerically), WDER reads DERIVED=1 with no wdiv token",
      wp("W0", gates(w="0")) == [] and wp("W05", gates(w="0.5")) == [] and wp("W05", gates(w="0.50")) == [] and wp("WDER", gates(d="1")) == [])
check("witness RED: W05 without the RWM_PLACE_WDIV token (the env did not reach the engine) is a problem on both endpoints", len(wp("W05", gates())) == 2, wp("W05", gates()))
check("witness RED: W0 echoing 0.5, W05 echoing 0, W0 with an unparseable value, W0 on the server only: all problems",
      wp("W0", gates(w="0.5")) != [] and wp("W05", gates(w="0")) != [] and wp("W0", gates(w="abc")) != [] and len(wp("W0", gates(w="0"), gates())) == 1)
check("witness RED: BASE / CAND / WDER carrying RWM_PLACE_WDIV (a leaked env) is a problem", wp("BASE", gates(w="0")) != [] and wp("CAND", gates(w="0.5")) != [] and wp("WDER", gates(d="1", w="0")) != [])
check("witness RED: DERIVED=1 on BASE / W0 / W05, or DERIVED=0 on WDER, is a problem",
      wp("BASE", gates(d="1")) != [] and wp("W0", gates(d="1", w="0")) != [] and wp("W05", gates(d="1", w="0.5")) != [] and wp("WDER", gates(d="0")) != [] and wp("WDER", gates()) != [])
check("witness: RWM_PLACE_WDIV_DERIVED=... is not read as RWM_PLACE_WDIV (the token match is exact)", lc.gate_tok(gates(d="1"), "RWM_PLACE_WDIV") is None)

# ── make_row under LD_PLAN=52: the readouts land in the row, the witnesses void it ──
RAW = os.path.join(HERE, "..", "..", "docs", "l1-raw")
PD = os.path.join(RAW, "thread-pd", "battery")
FIXC8 = "diag-tpd/c1d-400-MAIN-s42-r1-a1-"      # a two-leg fixture, reused as the c8 shape (as test_bbr48_parse.py does)


def mk(arm, w=None, d=None, with_diag=True, base_sha="b3000701f440d11b1b5ad946c8b36c3623c770cd"):
    drv, cli, srv = (lc.read(os.path.join(PD, FIXC8 + x)) for x in ("drv.out", "c.log", "s.log"))
    cli = [ln for ln in cli if "[DIAG] t=" not in ln and "[ETA] site=sender" not in ln]
    cli = [re.sub(r"\sRWM_PLACE_WIRE=\S+", "", ln) if "[GATES]" in ln else ln for ln in cli]
    srv = [re.sub(r"\sRWM_PLACE_WIRE=\S+", "", ln) if "[GATES]" in ln else ln for ln in srv]

    def tag(lines):
        out = []
        for ln in lines:
            if "[GATES]" in ln:
                ln = re.sub(r"\sRWM_PLACE_WDIV(_DERIVED)?=\S+", "", ln)
                ln += (f" RWM_PLACE_WDIV_DERIVED={d}" if d is not None else "") + (f" RWM_PLACE_WDIV={w}" if w is not None else "")
            out.append(ln)
        return out
    cli, srv = tag(cli), tag(srv)
    drv = [x for x in drv if "[TRUTH]" not in x and not x.lstrip().startswith("{")] + [
        '{"bytes":600000000,"dnf":0,"mean_mbps":98.5,"median_s":12.5,"summary":true}',
        '{"seconds":12.5,"mbps":98.5,"run":1}'] + [
        f"    [TRUTH] leg={i} dev=cli{i} egress_dgrams=500 egress_skbs=10 gso=1.00 netem_sent_dgrams=500 netem_dropped_skbs=0 backlog=0 lost=5 loss=0.010000 rcvbuf_drops=0 rcvbuf_scope=netns" for i in (0, 1)]
    if with_diag:
        cli = cli + T
    old = {k: os.environ.get(k) for k in ("LD_PLAN", "LD_ROW_BYTES", "LD_BASE_SHA")}
    os.environ.update(LD_PLAN="52", LD_ROW_BYTES="600000000")
    if base_sha is None:
        os.environ.pop("LD_BASE_SHA", None)
    else:
        os.environ["LD_BASE_SHA"] = base_sha
    try:
        return pp.make_row("c8-100", arm, "42", 1, 0, 13.0, drv, cli + [HOLD0, HOLD1], srv + SRV, 0, "x", store="def")
    finally:
        for k, v in old.items():
            if v is None:
                os.environ.pop(k, None)
            else:
                os.environ[k] = v


r = mk("W05", w="0.5")
check("make_row (LD_PLAN=52): a W05 row echoing RWM_PLACE_WDIV=0.5 on both endpoints is LIVE, carries the echo and its t52 readouts, and its own byte count",
      r["status"] == "LIVE" and r["gate_wdiv_cli"] == "0.5" and r["gate_wdiv_srv"] == "0.5" and r["t52"]["tail_s"] is not None and r["bytes"] == 600000000
      and r["t52"]["hold"][0]["cx_leg"] == {0: 3, 1: 4}, (r["status"], r["problems"], r.get("t52_error")))
for arm, kw, label in (("W0", dict(w="0"), "W0"), ("WDER", dict(d="1"), "WDER"), ("BASE", {}, "BASE"), ("CAND", {}, "CAND")):
    rr = mk(arm, **kw)
    check(f"make_row (LD_PLAN=52): a {label} row with exactly its registered env is LIVE", rr["status"] == "LIVE", (rr["status"], rr["problems"]))
rr = mk("W05")
check("make_row RED: a W05 row WITHOUT RWM_PLACE_WDIV in its [GATES] echo is WITNESS-FAIL", rr["status"] == "WITNESS-FAIL" and any("gate-wdiv" in x for x in rr["problems"]), (rr["status"], rr["problems"]))
rr = mk("BASE", w="0")
check("make_row RED: a BASE row carrying RWM_PLACE_WDIV is WITNESS-FAIL", rr["status"] == "WITNESS-FAIL" and any("gate-wdiv" in x for x in rr["problems"]), (rr["status"], rr["problems"]))
rr = mk("CAND", d="1")
check("make_row RED: a CAND row with DERIVED=1 is WITNESS-FAIL", rr["status"] == "WITNESS-FAIL", (rr["status"], rr["problems"]))
for sha in (None, "b3000701f440d11b1b5ad946c8b36c3623c770cd", "4926822" + "0" * 33):
    rr = mk("W0", w="0", base_sha=sha)
    check(f"make_row: the W-arm witnesses do not depend on LD_BASE_SHA ({'unset' if sha is None else sha[:8]}): W0 with its echo is LIVE or void only for the wire gauges, never for a wdiv witness",
          not any("gate-wdiv" in x for x in rr["problems"]), rr["problems"])
rr = mk("W05", w="0.5", with_diag=False)
check("make_row: a row with no [DIAG] lines still gets a t52 (all None), no t52_error, and is not voided by the readouts", rr["t52"] is not None and rr["t52"]["tail_s"] is None and "t52_error" not in rr)
real = t52.row_metrics
try:
    t52.row_metrics = lambda *a, **k: (_ for _ in ()).throw(ValueError("boom"))
    rr = mk("W05", w="0.5")
    check("make_row: a readout exception is recorded in t52_error and never voids the row", rr["status"] == "LIVE" and rr["t52"] is None and "boom" in rr["t52_error"], (rr["status"], rr.get("t52_error")))
finally:
    t52.row_metrics = real

# ── smoke / check / score on ledgers ──
def ledger(rows, extra=()):
    fh = tempfile.NamedTemporaryFile("w", suffix=".log", delete=False, encoding="utf-8", newline="\n")
    for x in rows:
        fh.write("P38ROW " + json.dumps(x, sort_keys=True) + "\n")
    fh.write("\n".join(extra) + "\n")
    fh.close()
    return fh.name


def slot(arm, seed, rep, base_sha="b3000701f440d11b1b5ad946c8b36c3623c770cd"):
    w = {"W0": "0", "W05": "0.5"}.get(arm)
    return mk(arm, w=w, d="1" if arm == "WDER" else None, base_sha=base_sha)


smoke_rows = [json.loads(json.dumps(slot(a, "42", 1))) for a in ("W0", "W05", "WDER")]
p = ledger(smoke_rows)
buf = io.StringIO()
with redirect_stdout(buf):
    rc = pp.smoke52(p)
check("smoke52: PASS on three LIVE env rows with every readout, cx_leg included (ledger round trip)", rc == 0 and "SMOKE-PASS" in buf.getvalue(), buf.getvalue()[-600:])
bad = json.loads(json.dumps(slot("W05", "42", 1)))
bad["t52"]["hold"]["0"]["cx_leg"] = None
p2 = ledger([smoke_rows[0], bad, smoke_rows[2]])
buf = io.StringIO()
with redirect_stdout(buf):
    rc2 = pp.smoke52(p2)
check("smoke52: ABORT-SMOKE (rc 1) when BASE's binary has no cx_leg=, when a row is missing, or when a row is void",
      rc2 == 1 and "no-cx_leg-p0" in buf.getvalue(), buf.getvalue()[-400:])
buf = io.StringIO()
with redirect_stdout(buf):
    rc3 = pp.smoke52(ledger(smoke_rows[:2]))
    voided = json.loads(json.dumps(slot("W05", "42", 1)))
    voided["status"] = "WITNESS-FAIL"
    rc4 = pp.smoke52(ledger([smoke_rows[0], voided, smoke_rows[2]]))
check("smoke52: rc 1 with a missing row and with a WITNESS-FAIL row", rc3 == 1 and rc4 == 1)
full = []
for arm_row in plan:
    c, s_, arm, seed, rep = arm_row
    x = json.loads(json.dumps(slot(arm, seed, rep)))
    x["seed"], x["rep"] = seed, rep
    full.append(x)
pf = ledger(full, extra=["LD-BATTERY-DONE tag=ld blocks=6"])
buf = io.StringIO()
with redirect_stdout(buf):
    rcc = pp.check52(pf)
check("check52: 30 rows and LD-BATTERY-DONE -> rc 0; 29 rows -> rc 1", rcc == 0 and pp.check52(ledger(full[:29], extra=["LD-BATTERY-DONE"])) == 1)
buf = io.StringIO()
with redirect_stdout(buf):
    t52.score([pf])
sc = buf.getvalue()
check("score52: reported only (REPORTED ONLY header, last line VERDICT52 REPORTED-ONLY rows=30/30 live=30), one block per arm with n = 6, no clause / verdict word",
      "REPORTED ONLY" in sc and sc.rstrip().splitlines()[-1] == "VERDICT52 REPORTED-ONLY rows=30/30 live=30" and all(f"== {a}: n = 6 rows" in sc for a in pp.ARMS52)
      and "NO VERDICT" not in sc and "PASS" not in sc and "FAIL" not in sc, sc[-900:])
check("score52: the witness line names each arm's echoed env (W05 wdiv 0.5/0.5, WDER derived 1/1, BASE no wdiv)",
      "W05 wdiv=['0.5/0.5']" in sc and "WDER wdiv=['None/None'] derived=['1/1']" in sc and "BASE wdiv=['None/None']" in sc, sc[:1800])
os.remove(pf)
# the scorer through the loopdiag_parse dispatcher the shell calls
import subprocess  # noqa: E402
pl = ledger(full, extra=["LD-BATTERY-DONE tag=ld blocks=6"])
outs = {c: subprocess.run([sys.executable, os.path.join(HERE, "loopdiag_parse.py"), c, pl], capture_output=True, text=True, errors="replace") for c in ("check52", "score52")}
check("loopdiag_parse.py check52 / score52 / smoke52 dispatch (the commands run_all calls)", outs["check52"].returncode == 0 and outs["score52"].returncode == 0
      and "VERDICT52 REPORTED-ONLY" in outs["score52"].stdout, (outs["check52"].stdout[-200:], outs["score52"].stderr[-300:]))
ps = subprocess.run([sys.executable, os.path.join(HERE, "loopdiag_parse.py"), "smoke52", p], capture_output=True, text=True, errors="replace")
check("loopdiag_parse.py smoke52 dispatch passes the three-row smoke ledger", ps.returncode == 0 and "SMOKE-PASS" in ps.stdout, ps.stdout[-300:] + ps.stderr[-300:])
os.remove(pl)

if _BASE_SHA_AT_START is not None:
    os.environ["LD_BASE_SHA"] = _BASE_SHA_AT_START
print("FAILED %d" % fails if fails else "ALL OK")
sys.exit(1 if fails else 0)
