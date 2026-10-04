#!/usr/bin/env python3
"""Offline exercise of `verify4_parse.py` (docs/status.md §6) on SYNTHETIC
lines and rows.

    python3 test_verify4_parse.py

NO ENGINE, NO VM. Endpoint lines reuse `test_stage3_parse`'s transcriptions
of the engine's format strings; the emission-batching echo is net/mod.rs's.
Scoring is exercised on hand-built ledgers whose verdicts are known in
advance (absolute checks of every clause and constant, not ordinal ones).
"""
import io
import json
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import verify4_parse as vp  # noqa: E402
import stage3_parse as sp  # noqa: E402

CHECKS = 0
FAILS = []


def check(cond, msg):
    global CHECKS
    CHECKS += 1
    if not cond:
        FAILS.append(msg)
        print("FAIL", msg)


TS = "\x1b[2m2026-10-04T10:00:00.000000Z\x1b[0m \x1b[32m INFO\x1b[0m raptorpath::net: "
WINL = TS + "reliable window mode (retain until acked): auto-selecting RLC windowed backend"
CADL = TS + ("estimator heavy-math cadence ACTIVE (RWM_EST_CADENCE: BOCD update at 10 ms/"
             "loss-event cadence, accumulated counts)")
OFFL = TS + "estimator heavy-math cadence OFF (RWM_EST_CADENCE=0: per-call BOCD update)"
EMBL = TS + ("emission batching ACTIVE (RWM_EMIT_BATCH: pacer-quantum TUN intake + per-burst "
             "taper/span refresh; flow-control and pacing contracts enforced at symbol granularity) burst=64")
SHA_NEW = "a" * 64
SHA_OLD = "b" * 64


def gates(emb=0, pa=0):
    return TS + f"[GATES] RWM_UNIFIED=1 RWM_POOL_ANCHOR={pa} RWM_EMIT_BATCH={emb} RWM_EMIT_BURST=64 RWM_GEN=384"


def pipe(p, b, h):
    return TS + f"[PIPE] pipeline={p} backend={b} hint={h}"


def diag(busy, paths, cum=(1000, 10, 900)):
    pp = "".join(f" p{i}:infl=0/sinfl=0/bdp1(cap0) sout=1 est=Y pl=0.0000 qlp=1/2 | ANCHOR sent=0 "
                 f"gen=0 fill=1 plu={u:.4f} plc={c:.4f}" for i, (u, c) in enumerate(paths))
    return (f"[DIAG] t=1.0s win=1/2 paused=0% good=1.0Mbit cum={cum[0]}/{cum[1]}/{cum[2]} cwnd=1 "
            f"wait[tun=0% paused=0% n=10 us=100 busy={busy}% busy_us=5]{pp} rce=1")


def truth(i, loss, egress=1000):
    return (f"    [TRUTH] leg={i} dev=cli{i} egress_dgrams={egress} egress_skbs=250 gso=4.00 "
            f"netem_sent_dgrams=975 netem_dropped_skbs=6 backlog=0 lost=25 loss={loss} "
            f"rcvbuf_drops=0 rcvbuf_scope=netns")


def drv(p, h, legs, cpu=2.5):
    return ([f"--- RWM-C perf pipeline={p} mode=single hint={h} A=c2 B=c2 ooo=0 extra='' T=default (1 x 1) start=10:00:00",
             f"    CPU: CPUSRV=1.00s CPUCLI={cpu:.2f}s (srv=decoder cli=sender; whole-invocation incl warmup)"]
            + [truth(i, 0.025) for i in range(legs)])


def acked(mbps, secs):
    return json.dumps({"proto": "rp-native", "hint": "bulk", "bytes": 1, "run": 1,
                       "seconds": secs, "mbps": mbps})


SUMMARY = json.dumps({"summary": True, "dnf": 0})


def logs(arm, cell, cad=None, emb_gate=None, emb_echo=None):
    p, b, h, want = sp.ARM_SPEC[arm]
    legs = sp.N_LEGS[cell]
    cad = want if cad is None else cad
    is_emb = arm == "EMB"
    emb_gate = (1 if is_emb else 0) if emb_gate is None else emb_gate
    emb_echo = is_emb if emb_echo is None else emb_echo
    echo = {"ACTIVE": [CADL], "OFF": [OFFL], "NONE": []}[cad]
    base = [gates(emb_gate), pipe(p, b, h), WINL] + echo
    cli = base + ([EMBL] if emb_echo else []) + [
        diag(20, [(0.0354, 0.0250)] * legs), diag(40, [(0.0354, 0.0260)] * legs),
        acked(88.0, 9.09), SUMMARY]
    srv = list(base)
    return drv(p, h, legs), cli, srv


def row(arm, cell, **kw):
    d, c, s = logs(arm, cell, **kw)
    sha = SHA_OLD if vp.ARM_BIN[arm] == "old" else SHA_NEW
    return vp.make_row(cell, arm, 42, 1, 0, 12, d, c, s, 0, sha)


# ── row witnesses ────────────────────────────────────────────────────────
for arm, cell in (("NEW", "c2-100"), ("OLD", "c8-100"), ("NEWa", "c3-25"), ("OLDa", "c2-100"),
                  ("EMB", "c1s-400")):
    r = row(arm, cell)
    check(r["status"] == "LIVE", f"{arm} live with its own witnesses: {r['status']} {r['problems']}")
check(vp.V4_ARMS["OLD"][3] == "NONE" and vp.V4_ARMS["NEW"][3] == "ACTIVE",
      "OLD (Stage-3 binary) prints no cadence line; NEW prints ACTIVE")
check(row("OLD", "c2-100", cad="ACTIVE")["status"] == "WITNESS-FAIL", "a cadence echo on OLD = not the Stage-3 binary")
check(row("NEW", "c2-100", cad="NONE")["status"] == "WITNESS-FAIL", "no cadence echo on NEW fails")
check(row("NEW", "c2-100", cad="OFF")["status"] == "WITNESS-FAIL", "OFF on NEW fails")
check(row("EMB", "c2-100", emb_gate=0)["status"] == "WITNESS-FAIL", "EMB with gate 0 fails")
check(row("EMB", "c2-100", emb_echo=False)["status"] == "WITNESS-FAIL", "EMB without client echo fails")
check(row("NEW", "c2-100", emb_gate=1)["status"] == "WITNESS-FAIL", "emit gate 1 on NEW fails")
check(row("NEW", "c2-100", emb_echo=True)["status"] == "WITNESS-FAIL", "emit echo on NEW fails")
r = row("NEW", "c8-100")
check(r["egress_dgrams"] == 2000 and abs(r["cpu_us_per_dgram"] - 2.5e6 / 2000) < 1e-9,
      f"cpu per datagram = CPUCLI / sum of egress datagrams over legs: {r['egress_dgrams']} {r['cpu_us_per_dgram']}")
d, c, s = logs("NEW", "c2-100")
check(vp.make_row("c2-100", "NEW", 42, 1, 3, 12, d, c, s, 0, SHA_NEW)["status"] == "VOID-RC", "rc void kept")
check(vp.make_row("c2-100", "NEW", 42, 1, 0, 12, d, c, s, 1, SHA_NEW)["status"] == "VOID-COTENANT", "cotenant kept")
check(vp.make_row("c2-100", "OLDa", 42, 1, 0, 12, d, c, s, 0, SHA_OLD)["status"] == "CONTAMINATED",
      "bulk logs under an auto arm contaminate")


# ── score: hand-built ledgers ────────────────────────────────────────────
def mk(cell, arm, seed, rep, mbps, cpu=5.0, dnf=False, plc=0.025, tr=0.025, gso=4.0):
    legs = sp.N_LEGS[cell]
    r = {"cell": cell, "arm": arm, "seed": str(seed), "rep": rep, "rc": 0, "status": "LIVE",
         "problems": [], "dnf": dnf, "mbps": None if dnf else mbps,
         "seconds": None if dnf else 100.0 / mbps, "cpu_cli": cpu, "busy_med": 30.0,
         "cpu_srv": 1.0, "util": None, "wall_s": 10, "bin": vp.ARM_BIN[arm],
         "bin_sha": SHA_OLD if vp.ARM_BIN[arm] == "old" else SHA_NEW,
         "egress_dgrams": 1000 * legs, "cpu_us_per_dgram": 1e6 * cpu / (1000 * legs)}
    for i in range(legs):
        r["plc_p%d" % i] = plc
        r["truth_loss_p%d" % i] = tr
        r["feed_ratio_p%d" % i] = plc / tr
        r["plu_med_p%d" % i] = 0.0354
        r["truth_gso_p%d" % i] = gso
    r["coded_share"] = 0.001
    return r


S3MED = {c: v[0] for c, v in vp.S3_CTL.items()}


def base(gp=None, overrides=None):
    """Every planned (cell, arm) at 3 reps x 2 seeds; OLD at §5's CTL median,
    OLDa at §5's WINa median, every other arm = its reference unless `gp`."""
    rows = []
    gp = gp or {}
    for cell, arms in vp.PLAN.items():
        for arm in arms:
            ref = vp.S3_WINA[cell][0] if arm in ("NEWa", "OLDa") else S3MED[cell]
            for seed in (42, 7):
                for rep in (1, 2, 3):
                    rows.append(mk(cell, arm, seed, rep, gp.get((cell, arm), ref)))
    for fn in (overrides or []):
        rows = [fn(r) for r in rows]
    return rows


HDR = f"=== binary NEW /x/new/raptorpath sha256 {SHA_NEW}\n=== binary OLD /x/old/raptorpath sha256 {SHA_OLD}\n"


def ledger(rows, extra=""):
    f = tempfile.NamedTemporaryFile("w", suffix=".log", delete=False, encoding="utf-8")
    f.write(HDR + extra)
    for r in rows:
        f.write("V4ROW " + json.dumps(r, sort_keys=True) + "\n")
    f.close()
    return f.name


def run(rows, extra=""):
    buf = io.StringIO()
    v, cv = vp.score([ledger(rows, extra)], out=lambda s: buf.write(s + "\n"))
    return v, cv, buf.getvalue()


v, cv, txt = run(base())
check(all(v[("A", c)] == "SAME" for c in vp.A_CELLS), f"all-equal -> SAME everywhere: {v}")
check(cv == "INERT-AS-DERIVED", f"all-equal EMB -> INERT: {cv}")
check(all(v[("B", c)] == "SAME" and v[("Bh", c)] == "HIST-SAME" for c in vp.B_CELLS), f"B same: {v}")
check("PREDICTION-A MISSED-AT-c1s-400" in txt, "prediction check names the c1s miss")
check(txt.count("-> IN-BAND") == 8, "OLD/OLDa at §5's medians are IN-BAND")

# (A) the absolute clause edges: c1s rel_gp 4.9 % of 303.2 -> band [288.34, 318.06]
v, _, _ = run(base({("c1s-400", "NEW"): 303.2 * 1.050}))
check(v[("A", "c1s-400")] == "BETTER", f"+5.0 % > 4.9 % -> BETTER {v[('A', 'c1s-400')]}")
v, _, _ = run(base({("c1s-400", "NEW"): 303.2 * 1.048}))
check(v[("A", "c1s-400")] == "SAME", f"+4.8 % inside 4.9 % -> SAME {v[('A', 'c1s-400')]}")
v, _, _ = run(base({("c2-100", "NEW"): 89.1 * 0.985}))
check(v[("A", "c2-100")] == "WORSE", f"-1.5 % past c2's 1.4 % -> WORSE {v[('A', 'c2-100')]}")


# CPU: NEW 6.0 % lower CPUCLI at c1d (rel 6.5 %) -> SAME; 7 % lower -> BETTER; 7 % higher -> WORSE
def cpu_at(cell, arm, f):
    def fn(r):
        if r["cell"] == cell and r["arm"] == arm:
            r["cpu_cli"] = 5.0 * f
            r["cpu_us_per_dgram"] = 1e6 * r["cpu_cli"] / r["egress_dgrams"]
        return r
    return fn


v, _, _ = run(base(overrides=[cpu_at("c1d-400", "NEW", 0.94)]))
check(v[("A", "c1d-400")] == "SAME", f"cpu -6 % inside 6.5 % {v[('A', 'c1d-400')]}")
v, _, _ = run(base(overrides=[cpu_at("c1d-400", "NEW", 0.93)]))
check(v[("A", "c1d-400")] == "BETTER", f"cpu -7 % -> BETTER {v[('A', 'c1d-400')]}")
v, _, _ = run(base(overrides=[cpu_at("c1d-400", "NEW", 1.07)]))
check(v[("A", "c1d-400")] == "WORSE", f"cpu +7 % -> WORSE {v[('A', 'c1d-400')]}")
# better goodput but worse cpu -> WORSE (worse wins)
v, _, _ = run(base({("c1d-400", "NEW"): 250.0}, overrides=[cpu_at("c1d-400", "NEW", 1.10)]))
check(v[("A", "c1d-400")] == "WORSE", f"worse wins over better {v[('A', 'c1d-400')]}")


# DNF excess: 2 of 6 NEW DNF (0.33 > 0.20) -> WORSE; 1 of 6 (0.17) -> not by DNF
def dnf_n(cell, arm, n):
    def fn(r):
        if r["cell"] == cell and r["arm"] == arm and r["seed"] == "42" and r["rep"] <= n:
            r["dnf"], r["mbps"], r["seconds"] = True, None, None
        return r
    return fn


v, _, _ = run(base(overrides=[dnf_n("c7-100", "NEW", 2)]))
check(v[("A", "c7-100")] == "WORSE", f"DNF 0.33 > 0.20 -> WORSE {v[('A', 'c7-100')]}")
v, _, _ = run(base(overrides=[dnf_n("c7-100", "NEW", 1)]))
check(v[("A", "c7-100")] == "SAME", f"DNF 0.17 -> SAME {v[('A', 'c7-100')]}")

# min live / witness fails -> UNSCOREABLE at that cell only
rows = [r for r in base() if not (r["cell"] == "c3-25" and r["arm"] == "OLD" and r["rep"] > 1)]
v, _, _ = run(rows)
check(v[("A", "c3-25")] == "UNSCOREABLE" and v[("A", "c2-100")] == "SAME", f"min live: {v}")


def wfail(r):
    if r["cell"] == "c8-100" and r["arm"] == "NEW" and r["rep"] == 1:
        r["status"] = "WITNESS-FAIL"
    return r


v, _, _ = run(base(overrides=[wfail]))
check(v[("A", "c8-100")] == "UNSCOREABLE", f"2 witness fails at c8 -> UNSCOREABLE {v[('A', 'c8-100')]}")


# a row run on the wrong binary is CONTAMINATED at score time
def wrongbin(r):
    if r["cell"] == "c1d-400" and r["arm"] == "OLD":
        r["bin_sha"] = SHA_NEW
    return r


v, _, txt = run(base(overrides=[wrongbin]))
check(v[("A", "c1d-400")] == "UNSCOREABLE" and "bin-sha-mismatch" in txt, "wrong binary contaminates")

# an abort token -> UNSCOREABLE everywhere
v, cv, _ = run(base(), extra="ABORT-SHA x\n")
check(all(v[("A", c)] == "UNSCOREABLE" for c in vp.A_CELLS) and cv.startswith("UNSCOREABLE"), "abort")

# CONTROL-MOVED: OLD at c2 = 86.0 (< 86.9 - 1.2 = 85.7? no: 86.0 inside) ; 85.0 -> moved
v, _, txt = run(base({("c2-100", "OLD"): 85.0, ("c2-100", "NEW"): 85.0}))
check("DRIFT A c2-100 OLD med=85.00" in txt and "CONTROL-MOVED" in txt, "OLD drift out of band is recorded")
check(v[("A", "c2-100")] == "SAME", "drift does not block the in-session (A) comparison")
# (B) historical: OLDa out of band -> the historical reading is CONTROL-MOVED
v, _, _ = run(base({("c3-25", "OLDa"): 13.0, ("c3-25", "NEWa"): 13.0}))
check(v[("Bh", "c3-25")] == "CONTROL-MOVED" and v[("B", "c3-25")] == "SAME", f"B hist control moved {v}")
v, _, _ = run(base({("c2-100", "NEWa"): 73.1 * 1.02}))
check(v[("Bh", "c2-100")] == "HIST-BETTER" and v[("B", "c2-100")] == "BETTER", f"B better {v}")

# (C): EMB better at c1s by 12 % -> FLIP; worse at c2 -> WORSE wins; feed moved
_, cv, _ = run(base({("c1s-400", "EMB"): 303.2 * 1.12}))
check(cv.startswith("FLIP-RECOMMENDED") and "c1s-400:gp" in cv, f"C flip {cv}")
_, cv, _ = run(base({("c1s-400", "EMB"): 303.2 * 1.12, ("c2-100", "EMB"): 89.1 * 0.98}))
check(cv == "WORSE-AT-c2-100", f"C worse wins {cv}")


def feed(r):
    if r["arm"] == "EMB" and r["cell"] == "c3-25":
        r["plc_p0"] = 0.035
        r["feed_ratio_p0"] = 0.035 / 0.025
    return r


_, cv, _ = run(base(overrides=[feed]))
check(cv == "FEED-MOVED-AT-c3-25:p0", f"C feed moved {cv}")
rows = [r for r in base() if not (r["cell"] == "c2-100" and r["arm"] == "EMB" and r["rep"] > 1)]
_, cv, _ = run(rows)
check(cv.startswith("UNSCOREABLE") and "c2-100" in cv, f"C min live {cv}")
check(set(vp.C_CELLS) == {"c1s-400", "c2-100", "c3-25"}, "EMB runs single-path cells only")

# ── (D) tunnel parsing ───────────────────────────────────────────────────


def tun_block(label, cell, hint, seed, mtu, pipeline, cad, mbps, sha):
    b = label.split("-")[0]
    backend = "Rlc" if pipeline == "window" else "RaptorQ"
    lines = [f"TUNARM label={label} cell={cell} hint={hint} bytes=1000 reps={len(mbps)} seed={seed} bin=/x/{b}/raptorpath start=x",
             f"TUNSHA {sha}", f"TUNMTU cli={mtu} srv={mtu}",
             f"TUNPIPE cli={pipeline}/{backend}/{hint} srv={pipeline}/{backend}/{hint}",
             f"TUNCAD cli={cad} srv={cad}"]
    for i, m in enumerate(mbps, 1):
        lines.append(f"TUNREP label={label} rep={i} rc=0 " + (
            json.dumps({"proto": "tcp", "run": 1, "seconds": 1.0, "mbps": m}) if m else '{"nodata": true}'))
    lines.append(f"TUNARM-DONE label={label} reps={len(mbps)} x")
    return "\n".join(lines) + "\n"


tl = (f"=== TUNBIN new {SHA_NEW}\n=== TUNBIN old {SHA_OLD}\n"
      + tun_block("new-bulk", "c2", "bulk", "42", 1196, "window", "ACTIVE", [80.0, 82.0], SHA_NEW)
      + tun_block("old-bulk", "c2", "bulk", "42", 1500, "block", "NONE", [70.0, None], SHA_OLD)
      + tun_block("new-bulk", "c2", "bulk", "7", 1500, "window", "ACTIVE", [99.0], SHA_NEW))
f = tempfile.NamedTemporaryFile("w", suffix=".log", delete=False, encoding="utf-8")
f.write(tl)
f.close()
arms = vp.tun_arms([f.name], {"new": SHA_NEW, "old": SHA_OLD})
check([a["status"] for a in arms] == ["LIVE", "LIVE", "WITNESS-FAIL"],
      f"tun witnesses: new 1196/window LIVE, old 1500/block LIVE, new at 1500 fails: {[(a['status'], a['problems']) for a in arms]}")
check(arms[1]["nodata"] == 1 and arms[1]["reps"] == [70.0], "a rep without JSON is a skipped datum")
buf = io.StringIO()
res = vp.tun([f.name], {"new": SHA_NEW, "old": SHA_OLD}, out=lambda s: buf.write(s + "\n"))
check(res[("c2", "bulk", "new")] == [80.0, 82.0], f"the witness-failed bring-up is excluded {res}")
check("TUN-DELTA c2 bulk new/old median = 1.157" in buf.getvalue(), buf.getvalue())

print(f"{CHECKS - len(FAILS)}/{CHECKS} checks passed")
sys.exit(1 if FAILS else 0)
