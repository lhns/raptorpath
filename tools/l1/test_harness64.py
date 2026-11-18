#!/usr/bin/env python3
"""Tests for status 64's harness (plan 64: the F2 merge guard; plan64.py, rt64_score.py, framing64.py and the l1common / perf_rwm_c.sh / place38_parse / battery / run_all wiring).
The registration is docs/derivations/f2-guard-registration.md (f2-guard-design 39269746), section 5; the launch paths run in test_stubrun.py (case64).

  1. the perf script's F2 lines run under the real script's shell options (`set -euo pipefail`, from lib.sh);
  2. the F2 row primitives and the denominator readout;
  3. stage G's F2 witnesses and the F2-FAIL status precedence;
  4. stage R's table, labels, replacement rule and score;
  5. framing64 on the two committed pre-read files;
  6. the shell side (suites runner, reuse, plan wiring).
Run: python3 test_harness64.py"""
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
for _k in ("LD_PLAN", "LD_WITNESS54", "LD_WITNESS64", "LD_SHA", "LD_SHA_BASE", "LD_BASE_SHA", "LD_ROW_BYTES", "LD_REHEARSE", "LD_BIN_SHA_BASE", "LD_BIN_SHA_CAND"):
    os.environ.pop(_k, None)
import l1common as lc  # noqa: E402

fails = 0
CHECKS = 0


def check(name, ok, detail=""):
    global fails, CHECKS
    CHECKS += 1
    print(("ok   " if ok else "FAIL ") + name + ((" -- " + str(detail)[:1500]) if not ok else ""))
    if not ok:
        fails += 1


def rd(name):
    return open(os.path.join(HERE, name), encoding="utf-8", errors="replace", newline="").read()


# ═══ 1. the perf script's F2 lines under `set -euo pipefail` ═══
perf = rd("perf_rwm_c.sh")
check("perf_rwm_c.sh prints `--- F2 undeliv=<max over both logs> unframable=<count>` after the client exits",
      'echo "--- F2 undeliv=${_F2_UND:-0} unframable=${_F2_UNF:-0}"' in perf and perf.index('echo "--- F2') > perf.index('[ "$CLI_ST" -ne 0 ] && echo "{\\"dnf\\":true'))
check("the battery keeps the `--- F2` line in the ledger (KEEP_PAT)", "--- F2" in rd("loopdiag_battery.sh").split("KEEP_PAT='")[1].split("'")[0])
f2_lines = [ln for ln in perf.splitlines() if ln.startswith(("_F2_TXT=", "_F2_UND=", "_F2_UNF=", 'echo "--- F2'))]
check("the perf script has exactly the four F2 lines, and every substitution ends in `|| true` (lib.sh turns `set -e` on: a bare `grep -c` with no match killed the script, rc 1, VOID-RC on every §63 row)",
      len(f2_lines) == 4 and all(ln.rstrip().endswith('|| true)"') for ln in f2_lines[:3]), f2_lines)
bash = shutil.which("bash")
if os.name == "nt":
    bash = next((c for c in (bash, r"C:\Program Files\Git\bin\bash.exe") if c and "system32" not in c.lower() and os.path.exists(c)), None)
if bash:
    snippet = "\n".join(f2_lines)
    d2 = tempfile.mkdtemp()
    try:
        def run_f2(s_text, c_text):
            with open(os.path.join(d2, "s.log"), "w", newline="\n") as fh:
                fh.write(s_text)
            with open(os.path.join(d2, "c.log"), "w", newline="\n") as fh:
                fh.write(c_text)
            sn = snippet.replace("/tmp/rwm-s.log", "'%s'" % os.path.join(d2, "s.log").replace("\\", "/")).replace("/tmp/rwm-c.log", "'%s'" % os.path.join(d2, "c.log").replace("\\", "/"))
            return subprocess.run([bash, "-c", "set -euo pipefail; " + sn + "; echo after-f2"], capture_output=True, text=True, timeout=60)
        cp = run_f2("\x1b[2m[CTLD]\x1b[0m t=1 undeliv=1\n[CTLD] t=2 undeliv=3\n[UNFRAMABLE] a\nplain\n", "[CTLD] t=3 undeliv=2\n x [UNFRAMABLE] b\n")
        check("the F2 lines under `set -euo pipefail` on logs with [CTLD] undeliv 1 / 3 / 2 and two [UNFRAMABLE] lines print `--- F2 undeliv=3 unframable=2` and l1common.f2_void reads it",
              cp.stdout.splitlines()[:1] == ["--- F2 undeliv=3 unframable=2"] and lc.f2_void(cp.stdout.splitlines()) is not None, (cp.returncode, cp.stdout, cp.stderr[-200:]))
        cp = run_f2("plain\n", "also plain\n")
        check("the F2 lines under `set -euo pipefail` on logs with NO [CTLD] and NO [UNFRAMABLE] line (grep exits 1 on no match) still print `--- F2 undeliv=0 unframable=0` and let the script go on (rc 0, `after-f2`)",
              cp.returncode == 0 and cp.stdout.splitlines()[:2] == ["--- F2 undeliv=0 unframable=0", "after-f2"], (cp.returncode, cp.stdout, cp.stderr[-200:]))
    finally:
        shutil.rmtree(d2, ignore_errors=True)
else:
    check("bash is available for the perf script's F2 lines", False, "no MSYS/Linux bash found")


# ═══ 2. the F2 row reading and the denominator ═══
import json  # noqa: E402
import plan64  # noqa: E402

SRV = ["[RFA] gen=0 fires=1 src_n=111 rep_n=2", "[RFA] gen=0 fires=9 src_n=338440 rep_n=65 final=1", "[CTLD] p0 tx=1 rx=62089 dgrx0[fr=62000 rd=1 ev=0] rxdrop0=0 undeliv=0", "[CTLD] p0 tx=2 rx=329889 dgrx0[fr=329880 rd=1 ev=0] rxdrop0=0 undeliv=0",
       "[SYMSZ] local=1200 decoder=1200"]
CLI = ["[CTLD] p0 tx=5 rx=6 dgrx0[fr=6 rd=1 ev=0] rxdrop0=0 undeliv=0", "[SYMSZ] local=1200 decoder=1200"]
rdg = plan64.f2_reading(CLI, SRV, ["--- F2 undeliv=0 unframable=0"])
check("f2_reading: the denominator is the server's LAST [RFA] src_n and its LAST [CTLD] rx= and dgrx0[fr=]; presence counts the logs' [CTLD] lines that carry undeliv= (server and client apart)",
      (rdg["src_n"], rdg["srv_rx"], rdg["srv_fr"], rdg["ctld_srv"], rdg["ctld_cli"], rdg["undeliv"], rdg["unframable"]) == (338440, 329889, 329880, 2, 1, 0, 0), rdg)
BASE_SRV = ["[CTLD] p0 tx=2 rx=329889 dgrx0[fr=329889 rd=1 ev=0] rxdrop0=0"]
rdg = plan64.f2_reading(["[CTLD] p0 tx=2"], BASE_SRV, ["--- F2 undeliv=0 unframable=0"])
check("f2_reading: a BASE log has [CTLD] lines but no undeliv= (no counter): the presence check reads the LOGS, never the driver's `--- F2 undeliv=0` line, so it is NOT a witness (ctld_srv 0)", rdg["ctld_srv"] == 0 and rdg["ctld_cli"] == 0 and rdg["undeliv"] == 0, rdg)
check("f2_reading: the sum over both ends is the reported undeliv, an [UNFRAMABLE] line on either end counts, the warm-up token and window_reliable are read",
      (lambda r: r["undeliv"] == 5 and r["unframable"] == 2 and r["warm"] == "NEVER-ACKED" and r["wr_cli"] is True and r["wr_srv"] is False)(
          plan64.f2_reading(["[CTLD] undeliv=2", "[UNFRAMABLE] a", "warm-up object never acked", "perf configuration peer_config=P { window_reliable: true }"], ["[CTLD] undeliv=3", "x [UNFRAMABLE] b", "perf configuration x window_reliable: false"])))
check("symsz_state: a missing end and a wrong end are told apart (missing = no line, wrong = a line that is not local = decoder = the size)",
      plan64.symsz_state({"symsz_cli": [], "symsz_srv": [(1200, 1200)]}, (1200, 1200)) == (["cli"], []) and plan64.symsz_state({"symsz_cli": [(1200, 1200)], "symsz_srv": [(512, 512)]}, (1200, 1200)) == ([], ["srv"])
      and plan64.symsz_state({"symsz_cli": [(1200, 512)], "symsz_srv": [(1200, 1200)]}, (1200, 1200)) == ([], ["cli"]))
check("rt_items: items 1-4 of registration 2.2 (warm-up, the receiving server's [CTLD] undeliv= and undeliv = 0, [UNFRAMABLE], [SYMSZ] 512/512 on both ends)",
      plan64.rt_items({"warm": "ACKED", "ctld_srv": 3, "undeliv": 0, "unframable": 0, "symsz_cli": [(512, 512)], "symsz_srv": [(512, 512)]}) == []
      and plan64.rt_items({"warm": "NEVER-ACKED", "ctld_srv": 0, "undeliv": 4, "unframable": 1, "symsz_cli": [(512, 512)], "symsz_srv": [(1200, 1200)]}) == [1, 2, 3, 4]
      and plan64.rt_items({"warm": "ACKED", "ctld_srv": 0, "undeliv": 0, "unframable": 0, "symsz_cli": [(512, 512)], "symsz_srv": [(512, 512)]}) == [2])

# ═══ 3. stage G: the row witnesses and the F2-FAIL precedence (real row parser on the committed smoke fixture) ═══
import place38_parse as pp  # noqa: E402
FIXD = os.path.join(HERE, "..", "..", "docs", "l1-raw", "thread-pd", "battery", "diag-smoke", "c1s-400-MAIN-s42-r1-a1-")
if os.path.exists(FIXD + "c.log"):
    FC, FS, FD = (lc.read(FIXD + x) for x in ("c.log", "s.log", "drv.out"))
    os.environ["LD_PLAN"], os.environ["LD_WITNESS64"] = "41d", "1"
    os.environ["LD_SHA"], os.environ["LD_SHA_BASE"] = "c" * 64, "b" * 64

    def grow(arm, cli, srv, drv=None, rc=0, cot=0, sha=None):
        sha = sha or ("c" * 64 if arm == "CAND" else "b" * 64)
        return pp.make_row("c1s-400", arm, "42", 1, rc, 40, drv if drv is not None else FD, cli, srv, cot, sha)
    SZ = "[SYMSZ] local=1200 decoder=1200"
    addu = lambda ls: [ln + " undeliv=0" if "[CTLD]" in ln else ln for ln in ls]      # noqa: E731
    good_c, good_s = addu(FC) + [SZ], addu(FS) + [SZ]
    r = grow("CAND", good_c, good_s)
    check("stage G: a CAND row with [SYMSZ] 1200/1200 on both ends and undeliv=0 on a present server [CTLD] is LIVE, with the executes witness and the seat comparison executed", r["status"] == "LIVE" and r["w64"]["executed"] and r["w64"]["sha_checked"] and not r["w64"]["hard"] and not r["w64"]["missing"], (r["status"], r["problems"], r["w64"]))
    r = grow("CAND", good_c + ["x [UNFRAMABLE] y"], good_s)
    check("stage G: an [UNFRAMABLE] line makes the row F2-FAIL (never VOID, never a DNF), with its reason and the label it would have had", r["status"] == "F2-FAIL" and r["status_41d"] == "LIVE" and "[UNFRAMABLE] x1" in r["f2_why"][0], (r["status"], r.get("f2_why")))
    r = grow("CAND", good_c, addu(FS) + ["[CTLD] p0 tx=1 rx=1 dgrx0[fr=1 rd=1 ev=0] rxdrop0=0 undeliv=2", SZ])
    check("stage G: undeliv > 0 on any [CTLD] line is F2-FAIL", r["status"] == "F2-FAIL" and "undeliv=2" in r["f2_why"][0], (r["status"], r.get("f2_why")))
    r = grow("CAND", good_c + ["x [UNFRAMABLE] y"], good_s, rc=1)
    check("stage G: the hard F2 failure wins over a crash (rc 1, VOID-RC): F2-FAIL, kept with its 41d label VOID-RC (a fail-closed tunnel looks like a DNF)", r["status"] == "F2-FAIL" and r["status_41d"] == "VOID-RC", (r["status"], r.get("status_41d")))
    r = grow("CAND", good_c, addu(FS))
    check("stage G: a missing witness line ([SYMSZ] on the server) is F2-FAIL on an otherwise LIVE row", r["status"] == "F2-FAIL" and "[SYMSZ] missing on srv" in r["f2_why"][0], (r["status"], r.get("f2_why")))
    r = grow("CAND", good_c, addu(FS) + [SZ], drv=FD)
    r2 = grow("CAND", addu(FC) + [SZ], [ln.replace("undeliv=0", "") for ln in addu(FS)] + [SZ])
    check("stage G: a [CTLD] only on the sending end (the receiving server printed none with undeliv=) is a missing witness: F2-FAIL, and the reason names the sending end", r2["status"] == "F2-FAIL" and "the sending end printed one" in "; ".join(r2["f2_why"]), (r2["status"], r2.get("f2_why")))
    r = grow("CAND", good_c, addu(FS), rc=1)
    check("stage G: a missing witness on a crash (VOID-RC) row is NOT F2-FAIL (only an otherwise LIVE or DNF row): it stays VOID-RC", r["status"] == "VOID-RC", (r["status"], r.get("f2_why")))
    r = grow("CAND", good_c + ["x [UNFRAMABLE] y"], good_s, cot=1)
    check("stage G: an infrastructure void comes first: a cotenant row with [UNFRAMABLE] stays VOID-COTENANT (replaced under 41d's rule)", r["status"] == "VOID-COTENANT", r["status"])
    r = grow("CAND", good_c + ["x [UNFRAMABLE] y"], good_s, sha="d" * 64)
    check("stage G: a seat / sha mismatch comes first too: WITNESS-FAIL on bin-sha, not F2-FAIL", r["status"] == "WITNESS-FAIL" and any(x.startswith("bin-sha") for x in r["problems"]), (r["status"], r["problems"]))
    r = grow("CAND", good_c + ["x [UNFRAMABLE] y"], good_s, drv=FD[:0] + [ln for ln in FD if '"summary"' not in ln] + ['{"bytes":400000000,"dnf":1,"hint":"bulk","proto":"rp-native","runs":1,"summary":true}'])
    check("stage G: the same on a DNF row (LIVE with dnf true, rc 0) is F2-FAIL labelled by its 41d status LIVE, never `DNF`", r["status"] in ("F2-FAIL", "NO_DATA") and (r["status"] != "F2-FAIL" or r["status_41d"] == "LIVE"), (r["status"], r.get("status_41d"), r.get("dnf")))
    r = grow("BASE", FC, FS)
    check("stage G: a BASE row records f2_void (expected None) and is never F2-FAIL; a BASE row with undeliv>0 would be reported (f2_void set), not failed", r["status"] == "LIVE" and r["w64"]["f2_void"] is None and r["w64"]["sha_checked"], (r["status"], r["w64"]))
    r = grow("BASE", FC, FS + ["[CTLD] p0 undeliv=1"])
    check("stage G: a BASE row with a fail-closed reading carries it in f2_void (reported in the witness table, never a verdict)", r["status"] == "LIVE" and r["w64"]["f2_void"] is not None, (r["status"], r["w64"]))
    check("view41d: F2-FAIL rows are scored under their 41d label (LIVE stays LIVE; a label that would have been replaceable reads as a crash, never a void slot)",
          plan64.view41d({"status": "F2-FAIL", "status_41d": "LIVE"})["status"] == "LIVE" and plan64.view41d({"status": "F2-FAIL", "status_41d": "VOID-SHORT"})["status"] == "VOID-RC"
          and plan64.view41d({"status": "F2-FAIL", "status_41d": "VOID-RC"})["status"] == "VOID-RC" and plan64.view41d({"status": "LIVE"})["status"] == "LIVE" and "F2-FAIL" not in pp.REPLACEABLE)
    for k in ("LD_PLAN", "LD_WITNESS64", "LD_SHA", "LD_SHA_BASE"):
        os.environ.pop(k, None)
else:
    print("SKIP stage G row checks: docs/l1-raw/thread-pd fixtures missing")

# ═══ 4. stage R: the table, the labels, the replacement rule and the score ═══
tb = plan64.plan()
check("stage R's table: 24 rows (12 RB + 12 RC) at c2-100, seeds 42 / 7 six each per arm, plan 41c's interleave (the pair RB,RC on even groups, RC,RB on odd), rep-major with seed 42 before 7",
      len(tb) == 24 and sum(1 for x in tb if x[2] == "RC") == 12 and [x[2] for x in tb[:4]] == ["RB", "RC", "RC", "RB"] and [(x[3], x[4]) for x in tb[::2][:3]] == [("42", 1), ("7", 1), ("42", 2)]
      and all(x[0] == "c2-100" and x[1] == "def" for x in tb) and {x[3] for x in tb} == {"42", "7"} and plan64.plan(smoke=True) == [("c2-100", "def", "RB", "42", 1), ("c2-100", "def", "RC", "42", 1)], tb[:4])
check("stage R's registered constants: 100 MB, timeout 150 s, at most 2 replaced slots, the Realtime echo equals place38_parse.REALTIME_ECHO59, the sizes 1200 (Bulk) / 512 (Realtime)",
      plan64.BYTES_R == 100000000 and plan64.TIMEOUT_R == 150 and plan64.MAX_REPLACEMENTS_R == 2 and plan64.REALTIME_ECHO == pp.REALTIME_ECHO59 and plan64.SYMSZ_G == (1200, 1200) and plan64.SYMSZ_R == (512, 512))
bat = rd_sh = rd("loopdiag_battery.sh")
runall = rd("loopdiag_run_all.sh")
check("the battery's stage R wiring: plan 64 accepted, HINT realtime, RB on BASE's binary (RBIN=$BIN_BASE) and RC on the CAND binary with no arm env, 24 rows asserted, the plan64.py table, the replacement helper, WITNESS64 only on the 41d stage",
      "P64=0; [ \"$PLAN\" = \"64\" ] && { P64=1; P38=1; P41=1; PCAND=1; }" in bat and "[ \"$P64\" = 1 ] && HINT=realtime" in bat and "RB|RC) [ \"$P64\" = 1 ]" in bat and '[ "$arm" = "RB" ] && { RBIN="$BIN_BASE"; RWANT="$WANT_BASE"; }' in bat
      and '-eq 24 ] || { echo "ABORT-PLAN64 table' in bat and "python3 ./plan64.py plan" in bat and "python3 ./plan64.py replace" in bat and "LD_WITNESS64=1 is defined for the plan-41d stage only" in bat)
check("RWM_C_RELIABLE is scrubbed from every row's env, so perf_rwm_c.sh keeps its default (--window-reliable to BOTH ends: rho = 1 at the Realtime hint)", "-u RWM_C_RELIABLE" in bat and "RWM_C_RELIABLE:-1" in rd("perf_rwm_c.sh") and "--window-reliable" in rd("perf_rwm_c.sh"))


def ledger(rows):
    d = tempfile.mkdtemp()
    p = os.path.join(d, "ld.log")
    with open(p, "w", newline="\n") as fh:
        for r in rows:
            fh.write("P38ROW " + json.dumps(r, sort_keys=True) + "\n")
        fh.write("LD-BATTERY-DONE tag=ld blocks=1\n")
    return d, p


def mkrow(arm, seed, rep, status=None, dnf=False, items=None, rc=0, warm="ACKED", mbps=70.0, cpu=10.0, extra=None):
    st = status or ("RT-OK" if arm == "RC" else "LIVE")
    r = {"cell": "c2-100", "store": "def", "arm": arm, "seed": seed, "rep": rep, "status": st, "rc": rc, "dnf": dnf, "mbps": None if dnf else mbps, "cpu_cli": None if dnf else cpu, "cpu_srv": None if dnf else cpu * 1.1,
         "bytes": 100000000, "cli_lag_E": 1.0, "srv_lag_E": 1.0, "inorder_mean": 5.0, "tick_span_s": 12.0, "rtp_floor_p0": 5000, "bin_sha": "a" * 64, "problems": [],
         "t64": {"warm": warm, "ctld_srv": 3, "ctld_cli": 1, "undeliv_srv": 0, "undeliv_cli": 0, "undeliv": 0, "unframable": 0, "symsz_cli": [[512, 512]], "symsz_srv": [[512, 512]], "src_n": 40000, "srv_rx": 41000, "srv_fr": 40900,
                 "wr_cli": True, "wr_srv": True, "echo_cli": True, "echo_srv": True, "items": items or [], "sha_checked": True}}
    if items:
        r["rt_items"] = items
    r.update(extra or {})
    return r


def table_rows(mod=None):
    """The 24 slots of the registered table, all passing; `mod(arm, seed, rep) -> row or None` replaces slots."""
    rows = []
    for _, _, a, sd, rp in tb:
        r = (mod(a, sd, rp) if mod else None) or mkrow(a, sd, rp)
        rows.append(r)
    return rows


import io  # noqa: E402
from contextlib import redirect_stdout  # noqa: E402
import rt64_score  # noqa: E402


def rt_score(rows):
    d, p = ledger(rows)
    buf = []
    res = rt64_score.score([p], out=buf.append)
    shutil.rmtree(d, ignore_errors=True)
    return res, "\n".join(buf)


res, txt = rt_score(table_rows())
check("rt64_score: 12 passing RC rows and 12 live completed RB rows read RT-PASS, item 5 SCORED, BASE-F2-DEAD 0/12 (and the k = 0 weakness is reported, not voided), the owner-flag line, the denominator per RC row",
      res["verdict"] == "RT-PASS" and res["item5"] == "SCORED" and "BASE-F2-DEAD 0/12" in txt and "nothing reproduced" in txt and "OWNER-FLAG64 none" in txt and txt.count("denominator (server received): src_n=40000 ctld_rx=41000 ctld_fr=40900") == 12, txt[-900:])
res, txt = rt_score(table_rows(lambda a, sd, rp: mkrow(a, sd, rp, dnf=True) if (a, sd, rp) == ("RC", "42", 1) else None))
check("rt64_score: item 5 (RC completion): an RC DNF while every live RB row completes is `RT-FAIL 5`", res["verdict"] == "RT-FAIL" and "s42 r1 item 5" in "; ".join(res["fails"]) and "label=RT-FAIL 5" in txt, (res, txt[-500:]))
res, txt = rt_score(table_rows(lambda a, sd, rp: mkrow(a, sd, rp, dnf=True) if (a, sd, rp) in (("RC", "42", 1), ("RB", "7", 3)) else None))
check("rt64_score: an RB DNF makes item 5 REPORTED: the same RC DNF is not a failure and the verdict is RT-PASS", res["verdict"] == "RT-PASS" and res["item5"].startswith("REPORTED"), (res, txt[-400:]))
res, txt = rt_score(table_rows(lambda a, sd, rp: mkrow(a, sd, rp, status="BASE-F2-DEAD", dnf=True, warm="NEVER-ACKED", rc=1) if a == "RB" else (mkrow(a, sd, rp, dnf=True) if (a, sd, rp) == ("RC", "42", 1) else None)))
check("rt64_score: with NO live RB row (all 12 dead) item 5 is NOT-SCORED and printed as such; the RC DNF is not an item-5 failure; BASE-F2-DEAD 12/12", res["item5"].startswith("NOT-SCORED") and res["verdict"] == "RT-PASS" and "BASE-F2-DEAD 12/12" in txt, (res, txt[-400:]))
res, txt = rt_score(table_rows(lambda a, sd, rp: mkrow(a, sd, rp, status="RT-FAIL", items=[2]) if (a, sd, rp) == ("RC", "7", 2) else None))
check("rt64_score: an RC row failing item 2 is RT-FAIL (the row and the item named) and never replaced", res["verdict"] == "RT-FAIL" and res["fails"] == ["s7 r2 item 2"], res)
res, txt = rt_score(table_rows(lambda a, sd, rp: mkrow(a, sd, rp, status="VOID-RHO", rc=0) if (a, sd, rp) == ("RC", "42", 1) else None))
check("rt64_score: an RC slot that is still a non-F2 void in the ledger (the replacement also voided, or the cap was spent) is RC-VOID, counts as not passed and reads RT-INCOMPLETE (not a pass, not an RT-FAIL)",
      res["verdict"] == "RT-INCOMPLETE" and res["fails"] == [] and len(res["rc_void"]) == 1 and "label=RC-VOID" in txt, (res, txt[-500:]))
rows = table_rows()
rows.insert(1, mkrow("RB", "42", 1, status="VOID-RC", rc=1))      # the first row of the RB slot voided ...
res, txt = rt_score(rows)
check("rt64_score: a replaced slot (a void first row, a later row of the same slot) takes the replacement; REPLACED SLOTS counts it (1 of at most 2)", res["verdict"] == "RT-PASS" and "REPLACED SLOTS: 1 of at most 2" in txt, txt[:400])
res, txt = rt_score(table_rows(lambda a, sd, rp: mkrow(a, sd, rp, status="VOID-RC", rc=1) if (a, sd, rp) == ("RB", "42", 1) else None))
check("rt64_score: an RB slot left void is RB-VOID: not a live RB row, not BASE-F2-DEAD, the RC reading is unaffected (RT-PASS)", res["verdict"] == "RT-PASS" and "RB-VOID 1" in txt and "BASE-F2-DEAD 0/12" in txt, txt[-500:])
res, txt = rt_score(table_rows(lambda a, sd, rp: mkrow(a, sd, rp, mbps=60.0, cpu=12.0) if a == "RC" else None))
check("rt64_score: the owner flag fires for a reported readout with RC worse than c2's d80 at rank p <= 0.05 (goodput -14 %, cpu +20 %), and the verdict is still RT-PASS (the readouts decide nothing)",
      res["verdict"] == "RT-PASS" and txt.count("OWNER-FLAG64 goodput") == 1 and "OWNER-FLAG64 cpu_cli/GB" in txt and "OWNER-FLAG64 cpu_srv/GB" in txt, txt[-900:])
check("rank_p: one-sided (RC worse), seeded and reproducible; the extreme split reads 1 / (N + 1) at best, an identical split reads 1", rt64_score.rank_p([9.0] * 3, [1.0] * 3, 1) == rt64_score.rank_p([9.0] * 3, [1.0] * 3, 1) and rt64_score.rank_p([9.0] * 3, [1.0] * 3, 1) < 0.2
      and rt64_score.rank_p([1.0] * 3, [9.0] * 3, 1) > 0.9 and rt64_score.rank_p([2.0] * 3, [2.0] * 3, 1) == 1.0 and rt64_score.rank_p([], [1.0], 1) is None)
# the replacement helper (the battery's `plan64.py replace`)


def qreplace(*a):
    with redirect_stdout(io.StringIO()):
        return plan64.replace(*a)


d, p = ledger([mkrow("RC", "42", 1, status="VOID-RHO")])
check("plan64 replace: a one-row slot whose row is a non-F2 void is replaced (reason printed, rc 0); an F2 failure, a dead BASE warm-up or a LIVE row never is",
      qreplace(p, "c2-100", "RC", "42", 1) == 0 and qreplace(p, "c2-100", "RC", "7", 1) == 1)
shutil.rmtree(d, ignore_errors=True)
for st in ("RT-FAIL", "BASE-F2-DEAD", "LIVE", "RT-OK", "F2-FAIL"):
    d, p = ledger([mkrow("RB" if st in ("BASE-F2-DEAD", "LIVE") else "RC", "42", 1, status=st)])
    check(f"plan64 replace: status {st} is never replaced", qreplace(p, "c2-100", "RB" if st in ("BASE-F2-DEAD", "LIVE") else "RC", "42", 1) == 1)
    shutil.rmtree(d, ignore_errors=True)
d, p = ledger([mkrow("RC", "42", 1, status="VOID-RHO"), mkrow("RC", "42", 1, status="VOID-RHO"), mkrow("RB", "7", 1, status="VOID-RC", rc=1), mkrow("RB", "7", 1, status="LIVE"), mkrow("RC", "7", 2, status="VOID-COTENANT")])
check("plan64 replace: a slot that already holds two rows is not replaced again, and the third voided slot is NOT replaced (at most 2 slots)", qreplace(p, "c2-100", "RC", "42", 1) == 1 and qreplace(p, "c2-100", "RC", "7", 2) == 1)
shutil.rmtree(d, ignore_errors=True)
check("plan64 check: the ledger must hold every slot and LD-BATTERY-DONE", True)

# ═══ 5. framing64 on the two committed pre-read files ═══
import framing64  # noqa: E402
t8 = rd("testdata/framing_ns-ab9feec4.txt")
t7 = rd("testdata/framing_ns-626fdf9a.txt")
runs8, runs7 = framing64.parse(t8), framing64.parse(t7)
check("framing64: the bound is the registered formula f * q95 * C / N_GB: frame <= +22.3 and read <= +36.5 ns/call (N_GB = 10^9 / 1182)",
      abs(framing64.BOUND["frame"] - 22.3) < 0.05 and abs(framing64.BOUND["read"] - 36.5) < 0.05 and abs(framing64.N_GB - 1e9 / 1182) < 1, framing64.BOUND)
diffs8 = {k: [round(r[k][1] - r[k][0], 1) for r in runs8] for k in runs8[0]}
check("framing64: commit 8's three runs parse to the registration's table (frame 64: +20.7, +8.7, +12.8; read 600: -1.7, +0.7, +5.4; packed: -32.4, -31.1, -33.9)",
      len(runs8) == 3 and diffs8["frame S=1200 len=64"] == [20.7, 8.7, 12.8] and diffs8["read S=1200 len=600"] == [-1.7, 0.7, 5.4] and diffs8["read S=512 3x160 packed"] == [-32.4, -31.1, -33.9], diffs8)
rows8 = {k: (m, lo, hi) for k, m, lo, hi, n, b in framing64.reading(runs8)}
rows7 = {k: m for k, m, lo, hi, n, b in framing64.reading(runs7)}
check("framing64: the per-point median of CAND - BASE with its min-max (commit 8: frame 64 +12.8 [+8.7..+20.7], frame 600 +9.8, frame 1196 +5.1; read 64 +2.0, read 600 +0.7, read 1196 +4.6)",
      all(abs(a - b) < 0.05 for a, b in zip(rows8["frame S=1200 len=64"], (12.8, 8.7, 20.7))) and abs(rows8["frame S=1200 len=600"][0] - 9.8) < 0.05 and abs(rows8["frame S=1200 len=1196"][0] - 5.1) < 0.05 and abs(rows8["read S=1200 len=64"][0] - 2.0) < 0.05
      and abs(rows8["read S=1200 len=600"][0] - 0.7) < 0.05 and abs(rows8["read S=1200 len=1196"][0] - 4.6) < 0.05, rows8)
lines8, lines7 = [], []
lab8, lab7 = framing64.report(t8, out=lines8.append), framing64.report(t7, out=lines7.append)
check("framing64: commit 8 reads BENCH PASS (every S = 1200 point within the bound; the packed point reported only)", lab8 == "BENCH PASS" and lines8[-1] == "BENCH PASS" and "reported only" in "\n".join(lines8), lines8[-3:])
check("framing64: commit 7 reads BENCH OVER at the three read points (+50.7 / +50.1 / +55.4 against +36.5) with the cost each implies in ms/GB and as a % of C_srv, the frame points within",
      lab7.startswith("BENCH OVER read S=1200 len=64") and lab7.count("read S=1200") == 3 and "frame S" not in lab7 and "+42.9 ms/GB, 0.24% of C" in lab7 and abs(rows7["read S=1200 len=64"] - 50.7) < 0.05, lab7)
check("framing64: a file with no bench point reads NO-DATA (rc 1), never a silent PASS", framing64.report("nothing here", out=lambda s: None) is None)
# the median, not the worst: one outlier run does not move it
t_out = t8 + "\n".join(f"{x}" for x in ["--- bench 3"]) + "\n" + "\n".join(ln.replace("CAND   124.7", "CAND   900.0") for ln in t8.splitlines()[:7]) + "\n"
check("framing64: the statistic is the MEDIAN over the runs (a fourth run with a +800 ns outlier moves frame 64 from +12.8 to +16.75, not to the maximum)", abs(dict((k, m) for k, m, *_ in framing64.reading(framing64.parse(t_out)))["frame S=1200 len=64"] - 16.75) < 0.01, framing64.reading(framing64.parse(t_out))[0])

# ═══ 6. the shell side ═══
check("run_all: plan 64 is plan 41d with W64=1 (alias like plan 54), SENV carries LD_WITNESS64, stage R is `stage rt64 64` after stage G, DONE-ALL needs both stages, the rehearsal runs both smokes",
      'W64=0; [ "$PLAN" = "64" ] && { W64=1; PLAN=41d; }' in runall and 'LD_WITNESS64="$([ "$splan" = "41d" ] && echo "$W64" || echo 0)"' in runall and "stage rt64 64" in runall and "rehearsal: stage rt64 smoke did not pass" in runall and "ROK=0; stage rt64 64 && ROK=1" in runall
      and "64) smk=smoke64r; scr=score64r; chk=check64r ;;" in runall and '[ "$W64" = 1 ] && { smk=smoke64; scr=score64; }' in runall)
check("run_all: the reference manifest defaults to tools/l1/ref64-manifest.sha256, the comparison is src_manifest() (the wider set), plan 64 refuses LD_PRIOR_TESTS, a MISS is ABORT-SUITES before any row and src_manifest() itself is untouched",
      'LD_REF_MANIFEST="${LD_REF_MANIFEST:-$HERE/ref64-manifest.sha256}"' in runall and "suites_stage64() {" in runall and "src_manifest | sed -E" in runall and 'ABORT-SUITES plan 64 suite-reuse MISS' in runall and 'ABORT-TESTS plan 64 reuses the engine suites by the reference manifest' in runall
      and "find raptorpath/src raptorpath/tests raptorpath-math/src raptorpath-core/src raptorpath-wasm/src gf256/src vendor -type f 2>/dev/null; } | LC_ALL=C sort -u | xargs -r sha256sum )" in runall and "if [ \"$W64\" = 1 ]; then suites_stage64; return; fi" in runall)
man = rd("ref64-manifest.sha256")
mls = man.splitlines()
check("ref64-manifest.sha256: LF, 212 lines `<sha256>  <path>` (two spaces, no text/binary marker), unique paths in C-locale order, Cargo.toml / Cargo.lock / every crate's Cargo.toml and the engine crates' src, none from tools/ or docs/",
      "\r" not in man and man.endswith("\n") and len(mls) == 212 and all(re.fullmatch(r"[0-9a-f]{64}  \S+", x) for x in mls) and [x[66:] for x in mls] == sorted(x[66:] for x in mls) and len({x[66:] for x in mls}) == 212
      and {"Cargo.toml", "Cargo.lock", "raptorpath/Cargo.toml", "raptorpath-core/Cargo.toml", "raptorpath-core/src/lib.rs"} <= {x[66:] for x in mls} and not any(x[66:].startswith(("tools/", "docs/")) for x in mls), (len(mls), mls[:2]))
check("the prebuilt binaries and their pins: LD_BIN_PREBUILT_BASE / _CAND are installed instead of built and a prebuilt REQUIRES its LD_BIN_SHA_* pin; a pin that is not the installed binary is ABORT-BUILD before any row",
      'LD_BIN_PREBUILT_CAND' in runall and 'LD_BIN_PREBUILT_BASE' in runall and "LD_BIN_PREBUILT_CAND needs LD_BIN_SHA_CAND" in runall and "LD_BIN_PREBUILT_BASE needs LD_BIN_SHA_BASE" in runall and "ABORT-BUILD plan 64 CAND binary" in runall and "ABORT-BUILD plan 64 BASE binary" in runall)
check("the seat witness: [SYMSZ] and [UNFRAMABLE] in CAND's binary, neither in BASE's, grep -aF (the brackets are literal), else ABORT-SEAT before any row", "grep -aqF -- \"$_s\" \"$BIN\"" in runall and "ABORT-SEAT plan 64" in runall)
check("the tools-test reuse: TOOLS-MANIFEST.sha256 (every file under tools/l1 in C-locale order, __pycache__ / *.pyc excluded) is written beside TOOLS-TESTS-OK; LD_PRIOR_TOOLS reuses it on a byte-identical manifest and prints TOOLS-TESTS-REUSED; a rehearsal always runs the tests",
      "tools_manifest() {" in runall and "find tools/l1 -type f ! -name '*.pyc' ! -path '*/__pycache__/*'" in runall and 'tools_manifest > "$SD/TOOLS-MANIFEST.sha256"; touch "$SD/TOOLS-TESTS-OK"' in runall and 'TOOLS-TESTS-REUSED $d' in runall
      and '[ "$REHEARSE" != 1 ] && [ -n "${LD_PRIOR_TOOLS:-}" ] && reuse_prior_tools' in runall)
i_build, i_runs = runall.index("--no-run ) > \"$BD/build.log\""), runall.index("taskset -c 2 timeout 900")
check("framing_ns in the rehearsal: the test binary is built first (--no-run, its own target dir, NOT pinned), then 10 runs each under `taskset -c 2` into bench/framing_ns.txt, then framing64.py; only the runs are pinned",
      i_build < i_runs and "for i in 1 2 3 4 5 6 7 8 9 10; do" in runall and '"$CARGO" test -p raptorpath --lib --release framing_ns -- --ignored --nocapture' in runall
      and 'target-bench' in runall and "framing64.py" in runall and "ABORT-BENCH" in runall and "taskset" not in runall[i_build - 200:i_build + 40] and sum(1 for ln in runall.splitlines() if "taskset -c 2 timeout" in ln) == 1, runall.count("taskset -c 2"))
check("the budget: BUDGET_S['64'] = 7650 s and ['64r'] = 3300 s are the guard's numbers in run_all (LD_HARD = launch + 2.5 h / rehearsal + 1.25 h), the launch's worst case with every RB row at its full timeout fits in 2.5 h",
      plan64.BUDGET_S == {"64": 7650, "64r": 3300} and "P64_NEED=7650; [ \"$REHEARSE\" = 1 ] && P64_NEED=3300" in runall and plan64.BUDGET_S["64"] < 2.5 * 3600 and plan64.BUDGET_S["64r"] < 1.25 * 3600
      and 300 + 2880 + 1000 + 200 + 12 * 30 + 12 * (plan64.TIMEOUT_R + 15) + 2 * 165 + 600 == plan64.BUDGET_S["64"])

# run_suites: the core and test-seams lines (registration section 5 item 5), extracted from the real script and run under bash against a stub cargo
m_rs = re.search(r"^run_suites\(\) \{\n.*?^\}\n", rd(os.environ.get("H64_RUNALL", "loopdiag_run_all.sh")), re.S | re.M)
if bash and m_rs:
    def suites_run(seams):
        td = tempfile.mkdtemp()
        try:
            repo = os.path.join(td, "repo")
            hd = os.path.join(repo, "tools", "l1")
            os.makedirs(hd)
            os.makedirs(os.path.join(repo, "raptorpath"))
            with open(os.path.join(repo, "raptorpath", "Cargo.toml"), "w", newline="\n") as fh:
                fh.write("[package]\nname = \"raptorpath\"\n[features]\n" + ("test-seams = []\n" if seams else ""))
            for f in ("test_ok.py", "test_lib_battery.sh", "test_abort_witness.sh"):
                with open(os.path.join(hd, f), "w", newline="\n") as fh:
                    fh.write("exit 0\n" if f.endswith(".sh") else "pass\n")
            bind = os.path.join(td, "bin")
            os.makedirs(bind)
            cargolog = os.path.join(td, "cargo.log")
            for name, body in (("cargo", '#!/bin/bash\necho "$*" >> "%s"\necho "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out"\n' % cargolog.replace("\\", "/")), ("python3", "#!/bin/bash\nexit 0\n")):
                with open(os.path.join(bind, name), "w", newline="\n") as fh:
                    fh.write(body)
                os.chmod(os.path.join(bind, name), 0o755)
            sd = os.path.join(td, "sd")
            os.makedirs(sd)
            script = ("HERE='%s'\nROOT='%s'\nSD='%s'\nCARGO='%s'\nPLAN=41d; P57=0; P46=0; W64=1; ENGINE_SUITES=1; ENGINE_WHY=''\nera() { echo \"$*\"; }\nsrc_manifest() { echo m; }\ntools_manifest() { echo tm; }\n"
                      % (hd.replace("\\", "/"), td.replace("\\", "/"), sd.replace("\\", "/"), os.path.join(bind, "cargo").replace("\\", "/")) + m_rs.group(0) + "run_suites; echo rc=$?\n")
            env = dict(os.environ, PATH=bind.replace("\\", "/") + os.pathsep + os.environ.get("PATH", ""))
            cp = subprocess.run([bash, "-c", script], capture_output=True, text=True, timeout=120, env=env)
            return cp, open(cargolog).read().splitlines() if os.path.exists(cargolog) else [], open(os.path.join(sd, "TESTS.txt"), errors="replace").read(), os.listdir(sd)
        finally:
            shutil.rmtree(td, ignore_errors=True)
    cp, cargo_l, tt, sdl = suites_run(True)
    seams_cmd = "test -p raptorpath --release --features test-seams --test f2_unframable_closes --test f2_mixed_symbol_size --test f2_realtime_packed -- --test-threads=1"
    check("run_suites on a tree that declares test-seams: the engine suites run main, doc, wasm, CORE (`test -p raptorpath-core`) and the debug witness, then the seams line exactly as f2-impl's suites_run.sh has it (release, --features test-seams, the three F2 tests, --test-threads=1)",
          "rc=0" in cp.stdout and "test -p raptorpath-core" in cargo_l and seams_cmd in cargo_l and "TEST core rc=0" in tt and "TEST seams rc=0" in tt and "SEAMS n/a" not in tt
          and [i for i, x in enumerate(cargo_l) if x == "test -p raptorpath-core"][0] < [i for i, x in enumerate(cargo_l) if x == seams_cmd][0], (cp.stdout[-300:], cp.stderr[-300:], cargo_l, tt[:400]))
    cp, cargo_l, tt, sdl = suites_run(False)
    check("run_suites on a tree without the feature: core runs, no seams line, `SEAMS n/a` is printed (and a plan 64 session writes no TESTS-OK without the engine record: tools suites bash tests ran)",
          "rc=0" in cp.stdout and "test -p raptorpath-core" in cargo_l and not any("test-seams" in x for x in cargo_l) and "SEAMS n/a" in tt and "TEST core rc=0" in tt and "TEST bashtests rc=0" in tt, (cp.stdout[-300:], cargo_l, tt[:400]))
else:
    check("run_suites is extractable from loopdiag_run_all.sh and bash is available", False, (bool(bash), bool(m_rs)))

if __name__ == "__main__":
    print("test_harness64: %d checks, %d failed" % (CHECKS, fails))
    sys.exit(1 if fails else 0)
