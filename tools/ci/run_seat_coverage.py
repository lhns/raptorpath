#!/usr/bin/env python3
"""Run cargo-llvm-cov once per seat group of tools/ci/seat_groups.json.

Writes <out>/<group>.lcov and <out>/runs.json (per-group wall time and exit codes).
Report only: a failing test does not stop the run (its profile still counts) and
the script exits 0. Needs cargo-llvm-cov and a Linux host (the spawned perf
server child must exit on a signal to write its profile).
Usage: run_seat_coverage.py OUT_DIR [group ...]
"""
import json, pathlib, subprocess, sys, time

ROOT = pathlib.Path(__file__).resolve().parents[2]
CFG = json.loads((pathlib.Path(__file__).with_name("seat_groups.json")).read_text())
IGNORE = r"(/tests/|/\.cargo/|/rustc/|/tests\.rs$|/tests/)"
BASE = ["cargo", "llvm-cov"]
PROF = ["--release", "-p", "raptorpath"]


def run(cmd):
    t = time.time()
    p = subprocess.run(cmd, cwd=ROOT)
    return p.returncode, round(time.time() - t, 1)


def main():
    out = pathlib.Path(sys.argv[1]).resolve()
    out.mkdir(parents=True, exist_ok=True)
    want = sys.argv[2:] or list(CFG["groups"])
    runs = {}
    for name in want:
        g = CFG["groups"][name]
        print(f"::group::seat group {name}: {g['seat']}", flush=True)
        run(BASE + ["clean", "--profraw-only"])
        steps, t0 = [], time.time()
        sel = [["--lib"]] if g.get("lib") else [["--test", e["test"]] + (["--"] + e["filters"] if e["filters"] else []) for e in g["tests"]]
        for s in sel:
            head = s[:2]
            tail = s[2:]
            rc, secs = run(BASE + ["--no-report", "--no-fail-fast"] + PROF + head + tail)
            steps.append({"sel": " ".join(s), "rc": rc, "secs": secs})
        rc, _ = run(BASE + ["report", "--release", "--lcov", "--ignore-filename-regex", IGNORE, "--output-path", str(out / f"{name}.lcov")])
        runs[name] = {"seat": g["seat"], "steps": steps, "report_rc": rc, "wall_s": round(time.time() - t0, 1)}
        print("::endgroup::", flush=True)
    (out / "runs.json").write_text(json.dumps(runs, indent=1))
    for n, r in runs.items():
        bad = [s["sel"] for s in r["steps"] if s["rc"]]
        print(f"{n}: {r['wall_s']} s, {len(r['steps'])} runs, failing: {bad or 'none'}")


main()
