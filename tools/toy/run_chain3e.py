"""Stage 3e chain (registration 14.6), run by a Windows Scheduled Task with no Claude parent:

    AA, AA2 -> freeze the A/A floors (PASS-aa) -> TIME (its 24 h gate) -> DECIDE (read; a FAIL stops) -> EARLY (read) -> MAIN (waits for PASS-simread)
    -> REST cell by cell (read and double each) -> the stage verdict (VERDICT-PASSES).  ROBUST and SMOOTH are not part of the chain: they wait for a PASSES verdict.

Each part stops on batch3e's STOP markers.  After each part the reader's output is appended to results-stage3e.md and a verdict file out3e/verdict-<PART>.txt is
written.  Logs: out3e/chain.log (this driver) and out3e/logs/<step>.log (every batch3e and summarize3e call).

    python run_chain3e.py --reuse-dirs 3d=PATH,ps1=PATH [--light-workers 4] [--heavy-workers 2]
"""
import argparse
import json
import os
import re
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
OUT = os.path.join(HERE, "out3e")
LOGS = os.path.join(OUT, "logs")
RESULTS = os.path.join(HERE, "results-stage3e.md")
DOUBLE_ONCE = {}


def log(msg):
    os.makedirs(LOGS, exist_ok=True)
    line = "[%s] %s" % (time.strftime("%Y-%m-%d %H:%M:%S"), msg)
    print(line, flush=True)
    with open(os.path.join(OUT, "chain.log"), "a") as f:
        f.write(line + "\n")


def stopped():
    return os.path.exists(os.path.join(OUT, "STOP"))


def die(msg):
    log("CHAIN STOPPED: " + msg)
    if not stopped():
        with open(os.path.join(OUT, "STOP"), "w") as f:
            f.write("run_chain3e: %s\n" % msg)
    sys.exit(1)


def call(script, args, tag, capture=False):
    os.makedirs(LOGS, exist_ok=True)
    cmd = [sys.executable, os.path.join(HERE, script)] + args
    log("run %s %s" % (script, " ".join(args)))
    with open(os.path.join(LOGS, tag + ".log"), "a") as lf:
        r = subprocess.run(cmd, cwd=HERE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
        lf.write(r.stdout)
    return r.returncode, r.stdout


def batch(part, a, extra=(), tag=None):
    args = ["--part", part, "--light-workers", str(a.light_workers), "--heavy-workers", str(a.heavy_workers), "--reuse-dirs", a.reuse_dirs] + list(extra)
    rc, out = call("batch3e.py", args, tag or part.replace(":", "-"))
    if rc != 0 or stopped():
        die("batch3e %s returned %d (see out3e/STOP and out3e/logs)" % (part, rc))
    return out


def verdict_file(part, text):
    with open(os.path.join(OUT, "verdict-%s.txt" % part), "a") as f:
        f.write(text + "\n")


def read_cell(cell, modes, part, a, partial=False):
    """Reader, one doubling for the comparisons that read undetermined, the reader again; returns the cell's outcome."""
    und = os.path.join(OUT, "double-%s.json" % part)
    if os.path.exists(und):
        os.remove(und)
    args = ["read", cell, "--modes", ",".join(modes), "--write-undet", und]
    rc, out = call("summarize3e.py", args, "read-%s-%s" % (part, cell))
    if rc != 0:
        die("reader failed on %s" % cell)
    if os.path.exists(und) and json.load(open(und)):
        extra = ["--cells", cell, "--double-partial"] if partial else ["--cells", cell]
        batch("DOUBLE:" + part, a, extra, "double-%s-%s" % (part, cell))
        rc, out = call("summarize3e.py", ["read", cell, "--modes", ",".join(modes), "--out", RESULTS], "reread-%s-%s" % (part, cell))
    else:
        call("summarize3e.py", ["read", cell, "--modes", ",".join(modes), "--out", RESULTS], "reread-%s-%s" % (part, cell))
    m = re.search(r"\*\*CELL (\S+): (\w[\w ]*)\*\*", out)
    outcome = m.group(2) if m else "no data"
    verdict_file(part, "%s: %s" % (cell, outcome))
    log("reader %s %s: %s" % (part, cell, outcome))
    return outcome


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--reuse-dirs", required=True)
    ap.add_argument("--light-workers", type=int, default=4)
    ap.add_argument("--heavy-workers", type=int, default=2)
    a = ap.parse_args()
    import batch3e as B
    B.build()
    log("chain starts; reuse %s; workers %d light + %d heavy" % (a.reuse_dirs, a.light_workers, a.heavy_workers))
    batch("AA", a)
    batch("AA2", a)
    rc, _ = call("summarize3e.py", ["freeze"], "freeze")
    if rc != 0 or not os.path.exists(os.path.join(OUT, "PASS-aa")):
        die("the A/A freeze failed")
    batch("TIME", a, ["--results", RESULTS])
    with open(os.path.join(OUT, "time.json")) as f:
        tj = json.load(f)
    with open(os.path.join(OUT, "TIME_TOTAL.txt"), "w") as f:
        f.write("TIME total by 14.7's one-pool formula: %.2f h (gate 24 h)\n" % tj["total_hours"])
    log("TIME total %.2f h" % tj["total_hours"])
    batch("DECIDE", a)
    o = read_cell("CL-long", ["OFF", "RL"], "DECIDE", a)
    if o == "FAILS":
        die("DECIDE reads FAILS on CL-long: the plan's 3.5 decision is needed (CROSS runs only if its condition holds)")
    verdict_file("DECIDE", "DECIDE outcome %s" % o)
    batch("EARLY", a)
    for cell in ("RT", "OV", "CL-long-v"):
        read_cell(cell, ["RL"], "EARLY", a)
    verdict_file("EARLY", "Ec: identity cell, gated by the runner's T = inf pair checks")
    log("waiting for PASS-simread before MAIN")
    while not os.path.exists(os.path.join(OUT, "PASS-simread")):
        if stopped():
            die("STOP while waiting for PASS-simread")
        time.sleep(60)
    batch("RB", a)
    for cell in B.RB_CELLS:
        read_cell(cell, ["RB", "RB025"], "RB", a)
    batch("MAIN", a)
    verdict_file("MAIN", "MAIN done: every run finished, every gate held (the runner stops on a failure); predictions are read from the stored runs")
    cells = []
    for j in B.PARTS["REST"]:
        if j.cell not in cells:
            cells.append(j.cell)
    outs = {}
    for cell in cells:
        batch("REST", a, ["--cells", cell], "REST-" + cell)
        if cell in B.FILE_CELLS:
            continue
        modes = ["OFF", "RL"] + (["RE"] if cell in B.RE_CELLS else [])
        outs[cell] = read_cell(cell, modes, "REST", a, partial=True)
    batch("REST", a)                                                         # the unfiltered pass writes DONE-REST
    allcells = ["CL-long", "RT", "OV", "CL-long-v"] + [c for c in outs if c not in ("RT", "OV", "CL-long-v")]
    rc, out = call("summarize3e.py", ["verdict", ",".join(allcells)], "verdict")
    m = re.search(r"STAGE VERDICT: (\w+)", out)
    v = m.group(1) if m else "unknown"
    verdict_file("REST", "STAGE VERDICT: %s" % v)
    log("stage verdict: %s" % v)
    log("chain done")


if __name__ == "__main__":
    main()
