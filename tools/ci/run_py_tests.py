#!/usr/bin/env python3
"""Run the tools/l1 test files (test_*.py, test_*.sh) except the full stub run.

Each file is a script whose exit code is the number of failures. Files named in
tools/l1/quarantine.txt are skipped (or run and reported, not counted, with
--include-quarantined). Quarantine format, one per line:
    <file> | <cause> | <date>
Exit 1 if any non-quarantined file fails.
"""
import argparse
import pathlib
import subprocess
import sys
import time

L1 = pathlib.Path(__file__).resolve().parents[1] / "l1"
SKIP = {"test_stubrun.py"}  # nightly: slow


def quarantined():
    out = {}
    q = L1 / "quarantine.txt"
    if q.exists():
        for line in q.read_text().splitlines():
            if line.strip() and not line.startswith("#"):
                parts = [p.strip() for p in line.split("|")]
                out[parts[0]] = parts[1:]
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--include-quarantined", action="store_true")
    ap.add_argument("--timeout", type=int, default=900)
    a = ap.parse_args()
    q = quarantined()
    files = sorted(p for p in L1.glob("test_*") if p.suffix in (".py", ".sh") and p.name not in SKIP)
    bad = []
    for p in files:
        isq = p.name in q
        if isq and not a.include_quarantined:
            print(f"SKIP  {p.name} (quarantined: {q[p.name][0] if q[p.name] else ''})")
            continue
        cmd = [sys.executable, p.name] if p.suffix == ".py" else ["bash", p.name]
        t0 = time.time()
        try:
            r = subprocess.run(cmd, cwd=L1, capture_output=True, text=True, timeout=a.timeout)
            rc, out = r.returncode, r.stdout + r.stderr
        except subprocess.TimeoutExpired:
            rc, out = 124, "TIMEOUT"
        dt = time.time() - t0
        tag = "ok   " if rc == 0 else ("QFAIL" if isq else "FAIL ")
        print(f"{tag} {p.name} rc={rc} {dt:.1f}s", flush=True)
        if rc != 0:
            print("\n".join(out.splitlines()[-25:]), flush=True)
            if not isq:
                bad.append(p.name)
    print(f"failed: {bad}" if bad else "all non-quarantined tools/l1 tests passed")
    return 1 if bad else 0


sys.exit(main())
