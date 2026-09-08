#!/usr/bin/env python3
"""Offline exercise of `r_parse.py` + `r_report.py` on a SYNTHETIC ledger.

    python3 test_r_parse.py

NO ENGINE, NO VM, NO NAMESPACE. The gauge lines below are transcribed from the
engine's own format strings and nothing else:

  * `[DIAG]`  — `net/diag.rs:934` (`cum=<src>/<cod>/<ack>`, the last line is
                the run's accounting)
  * `[CHI]`   — `net/mod.rs:2340`
  * `[FDIAG]` — `net/receiver.rs:1899`
  * `[RFA]`   — `net/mod.rs:5906-5908` (`preempt_src` by name)
  * `[GATES]` — `gates.rs:1296-1322`

THE POINT OF THIS FILE. MEASUREMENT DISCIPLINE 1 says prove the mechanism under
test executes. A parser is a mechanism, and a battery whose parser has never
been run against a line it will actually meet is a battery that discovers its
own scrape bugs at hour six of an overnight. Every assertion below is a claim
about a NUMBER the pre-registration will read.
"""
import json
import os
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))

GATES = (
    "[GATES] RWM_UNIFIED=0 RWM_THREE_TERM={three_term} RWM_DELTA_CAP=1 "
    "RWM_MIN_R=unset RWM_DELTA={delta} RWM_COMPLETION_EXPOSURE={chi} "
    "RWM_DIAG=1 RWM_FDIAG=1\n"
)


def client_log(*, mbps, seconds, runs, delta, chi, src, cod, ack, chimax,
               three_term=0, pl=0.018, rtt=41.2):
    out = [GATES.format(delta=delta, chi=chi, three_term=three_term)]
    if chi == "1":
        out.append("completion-exposure feed ACTIVE (T_rem from the perf client)\n")
    # A MID-RUN [DIAG] with SMALLER cumulative totals, deliberately: the parser
    # must read the LAST line and not the first, and a scrape that took the
    # first would silently under-report `cod` by a factor of ten.
    out.append(
        "[DIAG] t=0.5s win=10/4096 paused=0%% good=1.0Mbit ackrate_ewma=100sym/s "
        "eff_pace=100sym/s src=100sym/s cod=1sym/s cum=%d/%d/%d sidle=0ms/0/mx0ms "
        "cwnd=64 infl=10 np=1 rtt=%.1fms bdp100=100sym sweeps=0 retx=0 gapdrop=0 "
        "nbud=0 xattr=0/0 loan=0/0 pl=%.4f\n" % (src // 10, cod // 10, ack // 10, rtt, pl)
    )
    for i in range(runs):
        out.append(json.dumps({"run": i, "mbps": mbps + 0.01 * i,
                               "seconds": seconds + 0.001 * i}) + "\n")
    out.append(
        "[DIAG] t=9.9s win=10/4096 paused=0%% good=1.0Mbit ackrate_ewma=100sym/s "
        "eff_pace=100sym/s src=100sym/s cod=1sym/s cum=%d/%d/%d sidle=0ms/0/mx0ms "
        "cwnd=64 infl=10 np=1 rtt=%.1fms bdp100=100sym sweeps=0 retx=7 gapdrop=0 "
        "nbud=0 xattr=0/0 loan=0/0 pl=%.4f\n" % (src, cod, ack, rtt, pl)
    )
    out.append("[CHI] n=120 max=%.4f frac_gt_half=%.4f mean=0.2100 "
               "rttvar_src=srtt_eighth\n" % (chimax, 0.31 if chimax > 0.5 else 0.0))
    out.append(json.dumps({"summary": True, "dnf": 0, "runs": runs}) + "\n")
    return "".join(out)


def server_log(*, delta, chi, dec_n, dec_avg, src_n, src_avg, preempt,
               three_term=0):
    return (
        GATES.format(delta=delta, chi=chi, three_term=three_term)
        + "[FDIAG] frontier=9001 seen=9000 gap=1 probe_holes=19 probe_buffered=4 "
          "| DECODE n=%d avg=%.1fus present_at_stall=0 | SOURCE n=%d avg=%.1fus "
          "| COMPUTE calls=812 avg=8us max=41us total=6ms | rf=12 ru=9\n"
          % (dec_n, dec_avg, src_n, src_avg)
        + "[RFA] gen=0 fires=1200 false=140 false_frac=0.1167 fill_coded=300 "
          "fill_src=760 dup_src=60 preempt_src=%d src_n=9000 rep_n=1200 "
          "nu_recv=0.13000 fa_class=0.1100 rep_redundant=41 late_after_aban=0\n"
          % preempt
    )


def parse(tmp, cell, size, arm, seed, rep, **kw):
    c = os.path.join(tmp, "c-%s-%s-%s-%d-%d.log" % (cell, size, arm, seed, rep))
    s = os.path.join(tmp, "s-%s-%s-%s-%d-%d.log" % (cell, size, arm, seed, rep))
    open(c, "w").write(kw.pop("cli"))
    open(s, "w").write(kw.pop("srv"))
    out = subprocess.run(
        [sys.executable, os.path.join(HERE, "r_parse.py"),
         cell, size, arm, str(seed), str(rep), c, s],
        capture_output=True, text=True, check=True).stdout.strip()
    return out, json.loads(out)


def main():
    tmp = tempfile.mkdtemp(prefix="rparse-")
    fails = []
    total = [0]

    def check(name, cond, detail=""):
        total[0] += 1
        print("  %-58s %s%s" % (name, "OK" if cond else "FAIL",
                                "" if cond else "  <-- " + detail))
        if not cond:
            fails.append(name)

    print("=== 1 -- CTL at sc2/1.8 MB: the corner, measured")
    line, r = parse(
        tmp, "sc2", "s18", "CTL", 42, 1,
        cli=client_log(mbps=84.0, seconds=0.171, runs=40, delta="unset", chi="0",
                       src=60000, cod=0, ack=59900, chimax=0.0),
        srv=server_log(delta="unset", chi="0", dec_n=2, dec_avg=900.0,
                       src_n=812, src_avg=11400.0, preempt=60))
    check("RRESULT line is one JSON object", line.startswith("{") and line.endswith("}"))
    check("ARMCOUNT's grep key is present verbatim",
          '"cell": "sc2", "size": "s18", "arm": "CTL"' in line, line[:80])
    check("completion_p50 is the MEDIAN of the run seconds",
          abs(r["completion_p50"] - 0.191) < 1e-9, str(r["completion_p50"]))
    check("runs_n counts the per-run objects, not the summary", r["runs_n"] == 40)
    check("abort is False when a summary was seen", r["abort"] is False)
    check("W5: cum= is read off the LAST [DIAG], not the first",
          r["cum_src"] == 60000 and r["cum_ack"] == 59900,
          "%s/%s" % (r["cum_src"], r["cum_ack"]))
    check("W5: CTL carries cod = 0 (the corner)", r["cum_cod"] == 0)
    check("cod_frac is 0 at the corner", r["cod_frac"] == 0.0)
    check("H_price's predicted goodput delta is 0 when cod = 0 (unfalsifiable "
          "on this row)", r["h_price_predicted_goodput_delta"] == 0.0)
    check("W6: [CHI] max = 0 on the unarmed arm (the two-sided half)",
          r["chi_max"] == 0.0 and r["chi_feed_echo"] == 0)
    check("eps_hat scraped from pl=", abs(r["eps_hat"] - 0.018) < 1e-9)
    check("eps_hat_above_budget is False at 1.8 % < 5 %",
          r["eps_hat_above_budget"] is False)
    check("falsifier UNREADABLE at DECODE n = 2 (< 30) -> None, never False",
          r["entangled"] is None)
    check("W9: preempt_src scraped by name", r["rfa_preempt_src"] == 60)
    check("W4: [RFA] gen = 0 (RWM_GEN=0 took)", r["rfa_gen"] == "0")
    check("gate echoes captured two-sided",
          r["g_cli_RWM_DELTA"] == "unset" and r["g_srv_RWM_DELTA"] == "unset")

    print("\n=== 2 -- MID at c3hg/1.8 MB: r reaches the wire, and the pin echoes")
    _, r = parse(
        tmp, "c3hg", "s18", "MID", 42, 1,
        cli=client_log(mbps=14.1, seconds=1.02, runs=40, delta="0.05", chi="0",
                       src=60000, cod=3000, ack=59000, chimax=0.0, pl=0.061),
        srv=server_log(delta="0.05", chi="0", dec_n=140, dec_avg=8200.0,
                       src_n=300, src_avg=21000.0, preempt=90))
    check("W2: RWM_DELTA echoes the arm's NUMBER on both endpoints",
          r["g_cli_RWM_DELTA"] == "0.05" and r["g_srv_RWM_DELTA"] == "0.05")
    check("W4: RWM_THREE_TERM=0 two-sided (the named confound, pinned off)",
          r["g_cli_RWM_THREE_TERM"] == "0" and r["g_srv_RWM_THREE_TERM"] == "0")
    check("W5: cod > 0 -- r reached the wire", r["cum_cod"] == 3000)
    check("cod_frac = cod/(cod+src)", abs(r["cod_frac"] - 3000 / 63000.0) < 1e-6)
    check("H_price's own prediction is -cod_frac",
          abs(r["h_price_predicted_goodput_delta"] + 3000 / 63000.0) < 1e-6)
    check("eps_hat 6.1 % IS above the 5 % budget line",
          r["eps_hat_above_budget"] is True)
    check("falsifier readable at n >= 30 and DECODE < SOURCE -> not entangled",
          r["entangled"] is False, str(r["entangled"]))
    check("present_at_stall carried beside the DECODE avg (the 19-32 ms lesson)",
          r["fdiag_present_at_stall"] == 0.0)
    check("probe_holes/probe_buffered carried", r["fdiag_probe_holes"] == 19.0
          and r["fdiag_probe_buffered"] == 4.0)

    print("\n=== 3 -- GLIDE: chi reaches the glide, and the falsifier FIRES")
    _, r = parse(
        tmp, "c3hg", "s18", "GLIDE", 7, 3,
        cli=client_log(mbps=13.2, seconds=1.09, runs=40, delta="unset", chi="1",
                       src=60000, cod=1500, ack=59000, chimax=0.9312, pl=0.061),
        srv=server_log(delta="unset", chi="1", dec_n=140, dec_avg=26000.0,
                       src_n=300, src_avg=11400.0, preempt=310))
    check("W3: RWM_COMPLETION_EXPOSURE=1 two-sided",
          r["g_cli_RWM_COMPLETION_EXPOSURE"] == "1"
          and r["g_srv_RWM_COMPLETION_EXPOSURE"] == "1")
    check("W6: [CHI] max > 0.5 and the feed ACTIVE echo is present",
          r["chi_max"] > 0.5 and r["chi_feed_echo"] == 1)
    check("W6: frac_gt_half carried", r["chi_frac_gt_half"] == 0.31)
    check("falsifier FIRES: DECODE avg 26000us > SOURCE avg 11400us",
          r["entangled"] is True)

    print("\n=== 4 -- GLIDE-INERT: chi lives, r does not follow")
    _, r = parse(
        tmp, "sc2", "s25", "GLIDE", 42, 2,
        cli=client_log(mbps=85.5, seconds=2.34, runs=4, delta="unset", chi="1",
                       src=90000, cod=0, ack=89000, chimax=0.8100, pl=0.011),
        srv=server_log(delta="unset", chi="1", dec_n=0, dec_avg=0.0,
                       src_n=44, src_avg=9200.0, preempt=12))
    check("chi reached the glide", r["chi_max"] > 0.5)
    check("and r did NOT follow (cod = 0) -- the GLIDE-INERT reading",
          r["cum_cod"] == 0)
    check("eps_hat 1.1 % < 5 % -> the BUDGET-BOUND attribution's own input",
          r["eps_hat_above_budget"] is False)

    print("\n=== 5 -- ABORT is DISTINCT from DNF")
    _, r = parse(
        tmp, "c8", "s18", "CTL", 42, 4,
        cli="[GATES] RWM_DELTA=unset RWM_COMPLETION_EXPOSURE=0\n",
        srv="[GATES] RWM_DELTA=unset RWM_COMPLETION_EXPOSURE=0\n")
    check("no summary and no runs => abort = True", r["abort"] is True)
    check("and dnf stays 0 (they are not the same class)", r["dnf"] == 0)
    check("completion_p50 is None, so the row cannot enter a score",
          r["completion_p50"] is None)
    check("missing gauges are None, not 0 (a missing gauge must not score)",
          r["fdiag_decode_n"] is None and r["rfa_fires"] is None)

    print("\n=== 6 -- CRLF-tainted logs parse identically")
    cli = client_log(mbps=84.0, seconds=0.171, runs=40, delta="unset", chi="0",
                     src=60000, cod=0, ack=59900, chimax=0.0)
    _, a = parse(tmp, "sc2", "s18", "CTL", 42, 5, cli=cli,
                 srv=server_log(delta="unset", chi="0", dec_n=2, dec_avg=900.0,
                                src_n=812, src_avg=11400.0, preempt=60))
    _, b = parse(tmp, "sc2", "s18", "CTL", 42, 6,
                 cli=cli.replace("\n", "\r\n"),
                 srv=server_log(delta="unset", chi="0", dec_n=2, dec_avg=900.0,
                                src_n=812, src_avg=11400.0,
                                preempt=60).replace("\n", "\r\n"))
    a.pop("rep"); b.pop("rep")
    check("a CRLF ledger yields a byte-identical row", a == b)

    print("\n=== 7 -- r_report.py end to end on a synthetic ledger")
    # A ledger where MID wins at 1.8 MB and moves less at 25 MB: the shape
    # `R-FUNDED-POSITIVE-SMALL-ONLY` requires, so the outcome branch and the
    # §6 two-part clause both execute.
    outdir = os.path.join(tmp, "led")
    os.makedirs(outdir)
    rows = []
    for seed in (42, 7):
        for rep in range(1, 9):
            j = 0.002 * ((rep * 7 + seed) % 5)
            rows.append(dict(cell="sc2", size="s18", arm="CTL", seed=seed, rep=rep,
                             completion_p50=0.200 + j, mbps=84.0, dnf=0, abort=False,
                             cum_src=60000, cum_cod=0, cod_frac=0.0,
                             h_price_predicted_goodput_delta=0.0,
                             chi_max=0.0, chi_feed_echo=0, eps_hat=0.018,
                             diag_rtt_ms=41.0 + j, entangled=None,
                             fdiag_present=True, rfa_present=True,
                             gates_cli=True, gates_srv=True))
            rows.append(dict(cell="sc2", size="s18", arm="MID", seed=seed, rep=rep,
                             completion_p50=0.170 + j, mbps=83.0, dnf=0, abort=False,
                             cum_src=60000, cum_cod=2400, cod_frac=0.0385,
                             h_price_predicted_goodput_delta=-0.0385,
                             chi_max=0.0, chi_feed_echo=0, eps_hat=0.018,
                             diag_rtt_ms=41.2 + j, entangled=False,
                             fdiag_present=True, rfa_present=True,
                             gates_cli=True, gates_srv=True))
            rows.append(dict(cell="sc2", size="s25", arm="CTL", seed=seed, rep=rep,
                             completion_p50=2.400 + j, mbps=84.0, dnf=0, abort=False,
                             cum_src=90000, cum_cod=0, cod_frac=0.0,
                             h_price_predicted_goodput_delta=0.0,
                             chi_max=0.0, chi_feed_echo=0, eps_hat=0.018,
                             diag_rtt_ms=41.0 + j, entangled=None,
                             fdiag_present=True, rfa_present=True,
                             gates_cli=True, gates_srv=True))
            rows.append(dict(cell="sc2", size="s25", arm="MID", seed=seed, rep=rep,
                             completion_p50=2.398 + j, mbps=83.0, dnf=0, abort=False,
                             cum_src=90000, cum_cod=3400, cod_frac=0.0364,
                             h_price_predicted_goodput_delta=-0.0364,
                             chi_max=0.0, chi_feed_echo=0, eps_hat=0.018,
                             diag_rtt_ms=41.1 + j, entangled=False,
                             fdiag_present=True, rfa_present=True,
                             gates_cli=True, gates_srv=True))
    with open(os.path.join(outdir, "r-s42.log"), "w") as f:
        for r0 in rows:
            f.write("RRESULT " + json.dumps(r0, separators=(", ", ": ")) + "\n")
    p = subprocess.run([sys.executable, os.path.join(HERE, "r_report.py"),
                        "--outdir", outdir], capture_output=True, text=True)
    txt = p.stdout
    check("report exits 0", p.returncode == 0, p.stderr[-400:])
    check("the abort-cause table comes FIRST",
          txt.index("ABORT-CAUSE TABLE") < txt.index("MECHANISM LIVENESS"))
    check("liveness is read BEFORE the score",
          txt.index("MECHANISM LIVENESS") < txt.index("THE SCORE"))
    check("the falsifier is read before the score",
          txt.index("PRE-STATED FALSIFIER") < txt.index("THE SCORE"))
    check("MID is scored a WIN at 1.8 MB", "s18  MID" in txt and "WIN" in txt)
    check("the outcome is R-FUNDED-POSITIVE-SMALL-ONLY",
          "R-FUNDED-POSITIVE-SMALL-ONLY" in txt, txt[-900:])
    check("the ruling clause is printed", "does NOT bless" in txt)
    check("the goodput leg is reported as a GUARD",
          "GUARD-UNDERPOWERED" in txt)

    print("\n=== 8 -- r_report.py --calib: ABORT-SMOKE on a dead glide")
    outdir2 = os.path.join(tmp, "led2")
    os.makedirs(outdir2)
    with open(os.path.join(outdir2, "r-s42.log"), "w") as f:
        for cell in ("c3hg", "sc2"):
            for arm, cmax in (("CTL", 0.0), ("GLIDE", 0.0)):
                f.write("RRESULT " + json.dumps(dict(
                    cell=cell, size="s18", arm=arm, seed=42, rep=1,
                    completion_p50=1.0, mbps=14.0 if cell == "c3hg" else 84.0,
                    dnf=0, abort=False, cum_src=1000, cum_cod=0, cod_frac=0.0,
                    chi_max=cmax, chi_feed_echo=0, eps_hat=0.02,
                    diag_rtt_ms=40.0, entangled=None, fdiag_present=True,
                    rfa_present=True, gates_cli=True, gates_srv=True),
                    separators=(", ", ": ")) + "\n")
    p2 = subprocess.run([sys.executable, os.path.join(HERE, "r_report.py"),
                         "--outdir", outdir2, "--calib"],
                        capture_output=True, text=True)
    check("--calib fires ABORT-SMOKE when [CHI] is dead",
          "ABORT-SMOKE" in p2.stdout, p2.stdout[-400:] + p2.stderr[-300:])
    check("--calib exits nonzero so the launcher can refuse", p2.returncode == 6,
          str(p2.returncode))
    check("--calib says NOTHING IS A RESULT",
          "NOTHING IN THE CALIBRATION IS A RESULT" in p2.stdout)
    check("--calib reports the headroom check", "utilisation" in p2.stdout)
    check("--calib records cod = 0 as NOT an abort",
          "RECORDED, NOT AN ABORT" in p2.stdout)

    shutil.rmtree(tmp, ignore_errors=True)
    print("\n%s  (%d checks, %d failed)"
          % ("ALL CHECKS PASS" if not fails else "FAILURES: " + ", ".join(fails),
             total[0], len(fails)))
    return 1 if fails else 0


if __name__ == "__main__":
    sys.exit(main())
