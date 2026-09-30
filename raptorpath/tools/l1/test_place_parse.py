#!/usr/bin/env python3
"""Offline exercise of `place_parse.py` on synthetic endpoint logs.

    python3 test_place_parse.py

No engine, no VM, no namespace. Every gauge line below is transcribed from the
engine's own format string and nothing else:

  * `[ETA] site=sender`    `net/eta.rs` (head and per-path slots)
  * `[ETA] site=receiver`  `net/eta.rs`
  * `[LAT] site=receiver`  `net/lat.rs`
  * `[SUCC]`               `net/succ.rs`
  * `[DIAG]`               `net/diag.rs` (`rtt={:.1}ms` on the head,
                           per-path `rtt={:.0}/wrtt=...`)
  * the perf summaries     `perf.rs` (acked carries `mbps`; a DNF carries
                           `dnf: true` and no `mbps`)

A parser is a mechanism too (docs/measurement-discipline.md rule 1): a
battery whose parser has never been run against a line it will actually meet
discovers its own scrape bugs hours into a run. The `final=1` cases exercise
the exit-flush rule: both formats must parse, and the counts a flush
completes must not be double-counted by a scraper that counts lines.
"""
import json
import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import place_parse as pp  # noqa: E402

CHECKS = 0
FAILS = []


def check(cond, msg):
    global CHECKS
    CHECKS += 1
    if not cond:
        FAILS.append(msg)
        print("FAIL", msg)


def approx(a, b, tol=1e-6):
    return a is not None and b is not None and abs(a - b) <= tol


# ── The synthetic lines ──────────────────────────────────────────────────
ACKED = ('{"proto":"rp-native","hint":"bulk","bytes":100000000,"run":1,'
         '"seconds":8.6,"mbps":93.023}')
DNF = ('{"proto":"rp-native","hint":"bulk","bytes":100000000,"run":1,'
       '"dnf":true,"timeout_s":600}')

# net/diag.rs -- `rtt=10.4ms` on the head, `rtt=10/wrtt=11/rtp9ms`
# per path. `\brtt=` must not match `wrtt=`.
DIAG = ("[DIAG] t=8.0s win=100/200 paused=0% good=93.0Mbit ackrate_ewma=8000sym/s "
        "eff_pace=9000sym/s src=9000sym/s cod=0sym/s cum=90000/0/90000 "
        "sidle=0ms/0/mx0ms cwnd=100 infl=50 np=2 rtt=10.4ms bdp100=90sym sweeps=0 "
        "retx=12 gapdrop=0 nbud=0 xattr=0/0 loan=0/0"
        " p1:infl=30/sinfl=30/bdp80(cap100) sout=1/2/b3 ln=0/0 khr=1.00/kraw=1 "
        "btlbw=100 sr=1/g0d0 dr=1/a0s0g0d0 est=1 pl=0.0130 cmp=0 "
        "rtt=10/wrtt=11/rtp9ms sig_us=100/n20"
        " p2:infl=20/sinfl=20/bdp80(cap100) sout=1/2/b3 ln=0/0 khr=1.00/kraw=1 "
        "btlbw=20 sr=1/g0d0 dr=1/a0s0g0d0 est=1 pl=0.0200 cmp=0 "
        "rtt=40/wrtt=41/rtp39ms sig_us=300/n20")

# net/eta.rs -- the TSIG+HOL (HOLTSIG) sender line, every gauge live.
ETA_S_ARMED = (
    "[ETA] site=sender fhat_us=1950 n=4000 zero=0.2500 place_n=1200 cold_r=0.0100 "
    "cold_ge=0.0000 t_eff=0.150000 t_cold=0.0000 t_n=50 hol_sh=0.1200 hol_n=1200 "
    "hol_mv=0.0300 hol_calls=600 hol_w=0.010000"
    " p1:n=3000/4000 drop=0 tau_us=10000 e_p50=100 e_p90=200 e_p99=300 e_mx=400 "
    "late=0.3333 sig_us=1668/n27"
    " p2:n=2000/4000 drop=1 tau_us=40000 e_p50=150 e_p90=250 e_p99=350 e_mx=450 "
    "late=0.5000 sig_us=2633/n31")
# The CTL sender line: the arm gauges are `-` iff their denominator is 0.
ETA_S_CTL = (
    "[ETA] site=sender fhat_us=1950 n=4000 zero=0.2500 place_n=1200 cold_r=0.0100 "
    "cold_ge=0.0000 t_eff=- t_cold=- t_n=0 hol_sh=- hol_n=0 hol_mv=- hol_calls=0 hol_w=-"
    " p1:n=3000/4000 drop=0 tau_us=10000 e_p50=100 e_p90=200 e_p99=300 e_mx=400 "
    "late=0.3333 sig_us=1668/n27"
    " p2:n=2000/4000 drop=1 tau_us=40000 e_p50=150 e_p90=250 e_p99=350 e_mx=450 "
    "late=0.5000 sig_us=2633/n31")
# net/eta.rs, receiver line
ETA_R = ("[ETA] site=receiver n=2000 p1:n=2000 bind=0.0100 tau_us=10000 srtt_src=wire "
         "l_p50=900 l_p90=1500 l_p95=1800 l_p99=2200 l_mx=3000 sig_us=3430/n13 minrst=0")


def lat_line(n, p1, p2=None):
    """net/lat.rs:239-268, per-path body verbatim; sums chosen so the pooled
    shares are exact decimals."""
    def body(pid, ax, xp, sp, rrep, rep, xpn):
        tot = ax + xp + sp + rrep + rep
        sh = lambda v: "%.4f" % (v / tot)
        return (f" p{pid}:n=1000 nowait=10 minrst=0 ax_n=900 ax_p50=100 ax_p90=200 "
                f"ax_p95=250 ax_p99=300 ax_sum={ax} "
                f"rwxp_n={xpn} rwxp_p50=1000 rwxp_p95=1500 rwxp_sum={xp} "
                f"rwsp_n=1 rwsp_p50=50 rwsp_p95=50 rwsp_sum={sp} "
                f"rwrep_n=0 rwrep_p50=- rwrep_p95=- rwrep_sum={rrep} "
                f"rep_n=1 rep_p50=350 rep_p95=350 rep_sum={rep} "
                f"tot_p50=800 tot_p95=1900 tot_p99=21{pid}0 "
                f"sh_ax={sh(ax)} sh_rwxp={sh(xp)} sh_rwsp={sh(sp)} "
                f"sh_rwrep={sh(rrep)} sh_rep={sh(rep)}")
    s = f"[LAT] site=receiver n={n} over=0" + body(1, *p1)
    if p2:
        s += body(2, *p2)
    return s


# path 1: ax 600, xp 3000, sp 50, rwrep 0, rep 350 (tot 4000); path 2: ax
# 3000, xp 800, sp 100, rwrep 0, rep 100 (tot 4000). Pooled over 8000:
# sh_ax 0.45, sh_xp 0.475, sh_sp 0.01875, sh_rep 0.05625 -> MIXED.
LAT_DUAL = lat_line(2000, (600, 3000, 50, 0, 350, 200), (3000, 800, 100, 0, 100, 60))
# single path, queue-dominated: ax 3500 xp 100 sp 100 rrep 0 rep 300
LAT_QUEUE = lat_line(1000, (3500, 100, 100, 0, 300, 5))
LAT_C1 = lat_line(1000, (3500, 0, 200, 0, 300, 0))
LAT_EMPTY = "[LAT] site=receiver n=0 over=0 -"


def succ_line(det=7, sp_n=1, xp_n=2, gen=0):
    slot = lambda name: (f"{name}_n=2 {name}_p50_us=960 {name}_p90_us=1920 "
                         f"{name}_p99_us=1920 {name}_mx_us=1920 {name}_mean_us=1440")
    xf = "-" if (sp_n + xp_n) == 0 else "%.4f" % (xp_n / (sp_n + xp_n))
    return (f"[SUCC] gen={gen} det={det} res=3 {slot('orig')} {slot('rep')} "
            f"{slot('aban')} open=0 over=0 orig_frac=0.2857 cross_us=- dump=0/0 "
            f"sp_n={sp_n} xp_n={xp_n} xp_frac={xf} sp_p50_us=500 sp_p90_us=500 "
            f"xp_p50_us=900 xp_p90_us=1200")


def cli_log(eta=ETA_S_ARMED, summaries=(ACKED,), diag=DIAG):
    return [
        "[GATES] RWM_PLACE_T_DERIVED=1 RWM_PLACE_HOL=1\n",
        "[DIAG] t=0.3s np=0 rtt=0.0ms\n",
        "[ETA] site=sender fhat_us=10 n=1 zero=1.0000 place_n=1 cold_r=1.0000 "
        "cold_ge=- t_eff=- t_cold=- t_n=0 hol_sh=- hol_n=0 hol_mv=- hol_calls=0 "
        "hol_w=- p1:n=0/1 drop=0 tau_us=10000 e_p50=- e_p90=- e_p99=- e_mx=- "
        "late=- sig_us=-/n0\n",
        diag + "\n",
        eta + "\n",
    ] + [s + "\n" for s in summaries]


def srv_log(lat=LAT_DUAL, succ=None, eta_r=ETA_R, extra=()):
    if succ is None:
        succ = succ_line()
    return [
        "[GATES] RWM_PLACE_T_DERIVED=1 RWM_PLACE_HOL=1\n",
        "[SUCC] gen=0 det=0 res=0 orig_n=0 open=0 over=0 sp_n=0 xp_n=0 xp_frac=-\n",
        "[LAT] site=receiver n=0 over=0 -\n",
        succ + "\n",
        eta_r + "\n",
        lat + "\n",
    ] + [x + "\n" for x in extra]


# ── 1. The full row, no `final=` anywhere ────────────────────────────────
row = pp.parse("c8L", "HOLTSIG", "42", "1", cli_log(), srv_log())
check(row["cell"] == "c8L" and row["arm"] == "HOLTSIG", "cell/arm carried")
check(list(row)[:2] == ["cell", "arm"], "cell then arm FIRST: ARMCOUNT greps on that shape")
check(row["mbps"] == 93.023 and row["seconds"] == 8.6 and row["dnf"] is False,
      "goodput off the acked summary")
check(row["runs_n"] == 1 and row["acked_n"] == 1, "runs/acked counts")
# [LAT]
check(row["lat_present"] and row["lat_n"] == 2000.0 and row["lat_over"] == 0.0,
      "[LAT] head n=/over= (lat.rs:239)")
check(approx(row["sh_ax"], 0.45) and approx(row["sh_xp"], 0.475)
      and approx(row["sh_sp"], 0.01875, 1e-4) and approx(row["sh_rep"], 0.05625, 1e-4),
      "pooled shares weighted by per-path sums, got %s" % {k: row.get(k) for k in ("sh_ax", "sh_xp", "sh_sp", "sh_rep")})
check(row["lat_reading"] == "MIXED", "MIXED at 0.45/0.475")
check(row["rwxp_p95_worst"] == 1500.0 and row["ax_p95_worst"] == 250.0
      and row["tot_p99_worst"] == 2120.0, "worst-leg quantiles")
check(row["lat_rwxp_n"] == 260.0, "rwxp_n summed across paths")
check(len(row["lat_paths"]) == 2 and row["lat_paths"][1]["path"] == "p2"
      and row["lat_paths"][0]["rwrep_p95"] is None, "per-path slots incl. `-` -> None")
check(row["lat_final"] is False and row["recv_final"] is False, "no flush -> recv_final False")
check(row["lat_lines"] == 2 and row["succ_lines"] == 2 and row["eta_recv_lines"] == 1
      and row["eta_sender_lines"] == 2, "cadence counts without any flush")
# [ETA] sender head, every gauge by its net/eta.rs name
for k, v in (("fhat_us", 1950.0), ("eta_stamped", 4000.0), ("eta_zero", 0.25),
             ("place_n", 1200.0), ("cold_r", 0.01), ("cold_ge", 0.0),
             ("t_eff", 0.15), ("t_cold", 0.0), ("t_n", 50.0), ("hol_sh", 0.12),
             ("hol_n", 1200.0), ("hol_mv", 0.03), ("hol_calls", 600.0), ("hol_w", 0.01)):
    check(approx(row.get(k), v), "sender gauge %s = %s (got %s)" % (k, v, row.get(k)))
check(row["hol_executed"] is True, "hol_mv > 0 -> the execution witness fires")
check(row["eta_paths"][0]["matched"] == 3000.0 and row["eta_paths"][0]["stamped"] == 4000.0
      and row["eta_paths"][1]["drop"] == 1.0 and row["eta_paths"][1]["tau_us"] == 40000.0
      and row["eta_paths"][1]["late"] == 0.5 and row["eta_paths"][1]["pairs"] == 31,
      "sender per-path slots (eta.rs:441)")
check(row["sig_sender_us"] == 2633.0, "sigma max = worst leg")
rms = ((1668.0 ** 2 + 2633.0 ** 2) / 2) ** 0.5
check(approx(row["sig_sender_rms_us"], round(rms, 1), 1e-9), "sigma RMS over the candidate set")
check(approx(row["s4_from_teff"], 0.19238, 1e-5), "t_eff=0.15 inverts to 0.19238 (16.81.1)")
check(row["s4_confirms_0p15"] is True and approx(row["s4_ratio"], 1.0, 1e-3),
      "S4 verdict off the engine's own t_eff")
check(row["ref_us"] == 10000.0, "[DIAG] ref = min rtt, wrtt not matched (got %s)" % row["ref_us"])
check(row["tau_ref_us"] == 10000.0, "tau_us ref = min over sender paths")
check(approx(row["sig_ref_sender"], round(rms / 10000.0, 5), 1e-9)
      and approx(row["sig_ref_sender_tau"], round(rms / 10000.0, 5), 1e-9),
      "sigma/ref by both surrogate routes")
# [ETA] receiver
check(row["eta_recv_present"] and row["eta_recv_n"] == 2000.0 and row["sig_recv_us"] == 3430.0
      and row["eta_bind_max"] == 0.01 and row["lat_p95_recv"] == 1800.0,
      "receiver line (eta.rs:574)")
check(row["witness_sender_ge_recv"] is False, "16.81.6 witness REPORTED (2633 < 3430)")
# [SUCC]
check(row["succ_det"] == 7.0 and row["succ_xp_n"] == 2.0 and row["succ_sp_n"] == 1.0
      and approx(row["succ_xp_frac"], 0.6667) and approx(row["xp_over_det"], 0.2857, 1e-4),
      "[SUCC] fields (succ.rs:775)")
check("control_violated" not in row, "a dual is not a control")

# ── 2. CTL: the arm gauges are `-` and the S4 score falls back to tau_us ─
row = pp.parse("c7", "CTL", "42", "2", cli_log(eta=ETA_S_CTL), srv_log())
check(row["t_eff"] is None and row["t_n"] == 0.0 and row["hol_mv"] is None
      and row["hol_calls"] == 0.0 and row["hol_w"] is None, "`-` iff n = 0 -> None")
check("hol_executed" not in row and "s4_from_teff" not in row, "no witness, no engine inversion")
check(approx(row["s4_sigma_over_ref"], row["sig_ref_sender_tau"], 1e-9),
      "S4 falls back to the tau_us route before [DIAG]")

# ── 3. The reading table ─────────────────────────────────────────────────
row = pp.parse("c8L", "CTL", "7", "1", cli_log(), srv_log(lat=LAT_QUEUE))
check(row["lat_reading"] == "QUEUE-DOMINATED", "sh_ax 0.875, sh_xp 0.025 -> QUEUE-DOMINATED")
row = pp.parse("c8L", "CTL", "7", "1", cli_log(),
               srv_log(lat=lat_line(1000, (100, 3000, 100, 0, 800, 50))))
check(row["lat_reading"] == "PLACEMENT-INDICTED", "sh_xp 0.75 -> PLACEMENT-INDICTED")
row = pp.parse("c8L", "CTL", "7", "1", cli_log(),
               srv_log(lat=lat_line(1000, (500, 100, 100, 1500, 1800, 5))))
check(row["lat_reading"] == "REPAIR-DOMINATED", "rwrep+rep 0.825 -> REPAIR-DOMINATED")
row = pp.parse("c8L", "CTL", "7", "1", cli_log(), srv_log(lat=LAT_EMPTY))
check(row["lat_present"] and row["lat_paths"] == [] and "lat_reading" not in row
      and row["rwxp_p95_worst"] is None, "the empty `-` body parses to nothing")

# ── 4. The c1 control, both sides ────────────────────────────────────────
row = pp.parse("c1", "TSIG", "42", "1", cli_log(), srv_log(lat=LAT_C1, succ=succ_line(xp_n=0)))
check("control_violated" not in row, "c1 with xp_n=0 and rwxp_n=0 is clean")
row = pp.parse("c1", "TSIG", "42", "1", cli_log(), srv_log(lat=LAT_C1, succ=succ_line(xp_n=1)))
check(row.get("control_violated") is True, "c1 with [SUCC] xp_n>0 VOIDS")
row = pp.parse("sc2", "SINGLE", "42", "1", cli_log(), srv_log(lat=LAT_QUEUE, succ=succ_line(xp_n=0)))
check(row.get("control_violated") is True, "single with [LAT] rwxp_n>0 VOIDS")

# ── 5. ABORT != DNF ──────────────────────────────────────────────────────
row = pp.parse("c8L", "CTL", "42", "1", cli_log(summaries=()), srv_log())
check(row.get("abort") is True and "mbps" not in row, "no summary at all -> ABORT")
row = pp.parse("c8L", "CTL", "42", "1", cli_log(summaries=(DNF,)), srv_log())
check("abort" not in row and row["dnf"] is True and row["mbps"] is None
      and row["runs_n"] == 1 and row["acked_n"] == 0,
      "a DNF-only invocation (perf.rs:296, no `mbps`) is a DNF, not an ABORT")
row = pp.parse("c8L", "CTL", "42", "1", cli_log(summaries=(DNF, ACKED)), srv_log())
check(row["dnf"] is True and row["mbps"] == 93.023 and row["runs_n"] == 2 and row["acked_n"] == 1,
      "mixed DNF + acked: dnf flagged, goodput off the acked object")

# ── 6. `final=1`: the exit-flush rule ────────────────────────────────────
check(pp.is_final("[LAT] site=receiver n=5 over=0 final=1"), "final=1 at end")
check(pp.is_final("[SUCC] gen=0 final=1 det=7"), "final=1 mid-line")
check(pp.is_final("[ETA] site=receiver final=1 n=5"), "final=1 after the site")
check(not pp.is_final("[LAT] site=receiver n=5 final=10"), "final=10 is not the flag")
check(not pp.is_final("[LAT] site=receiver n=5 xfinal=1"), "xfinal=1 is not the flag")

# Interleaved tracing record glued after the flush marker (seen in real logs).
_glued = ("[LAT] site=receiver n=5 over=0 final=1\x1b[2m2026-09-08T18:54:50.123456Z"
          "\x1b[0m \x1b[32m INFO\x1b[0m raptorpath::tun: cleaning up TUN interface\n")
_pieces = pp.split_interleaved(_glued)
check(len(_pieces) == 2, "glued line splits into readout + tracing record")
check(pp.is_final(_pieces[0]), "final=1 survives the split")
check(_pieces[0].rstrip().endswith("final=1"), "readout piece ends at the marker")
check("cleaning up TUN" in _pieces[1] and "\x1b" not in _pieces[1], "tracing piece colour-stripped")
_plain = "[SUCC] gen=0 det=7 final=1 2026-09-08T18:54:50Z  INFO raptorpath: bye\n"
check(pp.is_final(pp.split_interleaved(_plain)[0]), "plain (no ANSI) interleave also split")
check(pp.split_interleaved("[LAT] site=receiver n=5 final=1\n") == ["[LAT] site=receiver n=5 final=1"],
      "unglued line untouched")
check(pp.split_interleaved("2026-09-08T18:54:50Z INFO start\n")[0].startswith("2026"),
      "a line that IS a tracing record is not split at column 0")
check(not pp.is_final("[LAT] site=receiver n=5 final=0"), "final=0 is not the flag")
check(not pp.is_final(None) and not pp.is_final(""), "empty is not final")

partial_lat = lat_line(1500, (600, 3000, 50, 0, 350, 150), (3000, 800, 100, 0, 100, 40))
final_lat = LAT_DUAL + " final=1"
final_succ = succ_line(det=9, xp_n=3) + " final=1"
final_eta_r = "[ETA] site=receiver n=2500 final=1 " + ETA_R.split(" ", 3)[3]
srv = srv_log(lat=partial_lat, succ=succ_line(det=5, xp_n=1),
              extra=(final_succ, final_eta_r, final_lat, "[RFA] gen=0 dup_src=3"))
row = pp.parse("c8L", "HOL", "42", "3", cli_log(), srv)
check(row["recv_final"] is True and row["lat_final"] and row["succ_final"] and row["eta_recv_final"],
      "the flush is seen on all three receiver gauges")
check(row["lat_n"] == 2000.0 and row["lat_rwxp_n"] == 260.0, "last-line scraper takes the flush's counts")
check(row["succ_det"] == 9.0 and row["succ_xp_n"] == 3.0, "[SUCC] off the flush")
check(row["eta_recv_n"] == 2500.0 and row["sig_recv_us"] == 3430.0,
      "[ETA] receiver off the flush, `final=1` in head position tolerated")
check(row["lat_lines"] == 2 and row["succ_lines"] == 2 and row["eta_recv_lines"] == 1,
      "line COUNTERS skip the flush (got lat=%s succ=%s eta=%s)"
      % (row["lat_lines"], row["succ_lines"], row["eta_recv_lines"]))
check(approx(row["sh_xp"], 0.475), "shares computed on the flushed line")

# The flush wins wherever it sits: a stale cadence line scraped after it
# (a log captured mid-teardown, or two lines racing on stderr) must not
# displace the complete count.
srv = srv_log(lat=partial_lat, extra=(final_lat, partial_lat))
row = pp.parse("c8L", "HOL", "42", "3", cli_log(), srv)
check(row["lat_n"] == 2000.0 and row["lat_final"] is True, "final line beats a later cadence line")
check(row["lat_lines"] == 3, "the three cadence lines are counted, the flush is not")

# Sender-side flush, should the sender ever emit one: same rule, same code.
srv = srv_log()
cli = cli_log() + [ETA_S_CTL + " final=1\n"]
row = pp.parse("c8L", "HOLTSIG", "42", "3", cli, srv)
check(row["eta_final"] is True and row["t_n"] == 0.0, "a sender flush is the sender reading")
check(row["eta_sender_lines"] == 2, "sender cadence count skips the flush")

# ── 7. Swapped logs are a parse, not a silent zero ───────────────────────
row = pp.parse("c8L", "CTL", "42", "1", cli_log() + srv_log(), cli_log(summaries=()))
check(row["lat_present"] and row["succ_present"] and row["eta_recv_present"],
      "receiver lines found in the other log")
row = pp.parse("c8L", "CTL", "42", "1", [ACKED + "\n"], srv_log() + [ETA_S_CTL + "\n"])
check(row["eta_present"] and row["fhat_us"] == 1950.0, "sender line found in the other log")

# ── 8. The CLI contract the battery greps on ─────────────────────────────
with tempfile.TemporaryDirectory() as td:
    c = os.path.join(td, "c.log")
    s = os.path.join(td, "s.log")
    with open(c, "w") as f:
        f.writelines(cli_log())
    with open(s, "w") as f:
        f.writelines(srv_log())
    out = subprocess.run([sys.executable, os.path.join(HERE, "place_parse.py"),
                          "c7", "T0", "7", "4", c, s],
                         capture_output=True, text=True)
    check(out.returncode == 0, "CLI exit 0 (stderr: %s)" % out.stderr.strip()[:200])
    check(out.stdout.startswith("PLACERESULT "), "PLACERESULT prefix")
    check('"cell": "c7", "arm": "T0"' in out.stdout, "ARMCOUNT's grep shape")
    j = json.loads(out.stdout[len("PLACERESULT "):])
    check(j["seed"] == 7 and j["rep"] == 4, "seed/rep ints")
    out = subprocess.run([sys.executable, os.path.join(HERE, "place_parse.py"),
                          "c7", "T0", "7", "4", os.path.join(td, "nope"), s],
                         capture_output=True, text=True)
    check(out.returncode == 0 and '"abort": true' in out.stdout, "a missing log is an ABORT row")
    leg0 = ("    [TRUTH] leg=0 dev=cli0 egress_dgrams=100000 egress_skbs=20000 gso=5.00 "
          "netem_sent_dgrams=97400 netem_dropped_skbs=520 backlog=0 lost=2600 loss=0.026000 "
          "rcvbuf_drops=3 rcvbuf_scope=netns\n")
    leg1 = leg0.replace("leg=0 dev=cli0", "leg=1 dev=cli1").replace("loss=0.026000", "loss=0.048000")
    qp = os.path.join(td, "q.txt")
    with open(qp, "w") as f:
        f.write("== TRUTH (per-datagram loss per data leg; lib.sh truth_line)\n" + leg0 + leg1)
    out = subprocess.run([sys.executable, os.path.join(HERE, "place_parse.py"),
                          "c8", "CTL", "7", "4", c, s, qp],
                         capture_output=True, text=True)
    j = json.loads(out.stdout[len("PLACERESULT "):])
    check(out.returncode == 0 and j["truth_loss_p0"] == 0.026 and j["truth_loss_p1"] == 0.048,
          "q.txt argument: per-leg truth_loss_p<i> columns")
    check(j["truth_gso_p0"] == 5.0 and j["truth_rcvbuf_drops"] == 3,
          "q.txt argument: gso and rcvbuf columns")
    out = subprocess.run([sys.executable, os.path.join(HERE, "place_parse.py"),
                          "c7", "T0", "7", "4", c, s],
                         capture_output=True, text=True)
    j = json.loads(out.stdout[len("PLACERESULT "):])
    check(j.get("truth_rcvbuf_drops", "absent") is None,
          "no q.txt argument: the truth column is None, not absent")

print("test_place_parse: %d checks, %d failed" % (CHECKS, len(FAILS)))
sys.exit(1 if FAILS else 0)
