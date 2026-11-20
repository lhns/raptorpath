#!/usr/bin/env python3
"""The flake rule (ADR-0080): a failure that passes on a re-run is still a failure,
unless the test is on raptorpath/tests/quarantine.txt.

Quarantine format, one test per line (a quarantined test is also #[ignore]d with that reason):
    <test name> | <owner> | <reason, naming the diagnosis or ledger entry>
A cargo name matches an entry when it is equal or ends with "::<entry>".

  flake_rule.py --quarantine FILE --check NAME          exit 0 if NAME is quarantined, else 1
  flake_rule.py --quarantine FILE --failures FILE       lines "<test> [<cmd>]"; exit 1 if any
                [--passes FILE] [--runs N]              is outside quarantine. --passes: lines
                                                        "<test> [<cmd>] ok|FAILED", one per run
"""
import argparse
import collections
import pathlib
import sys

DEFAULT_Q = pathlib.Path(__file__).resolve().parents[2] / "raptorpath/tests/quarantine.txt"


def parse_quarantine(text):
    """-> {name: (owner, reason)}. A malformed line is an error, not a skip."""
    out = {}
    for n, line in enumerate(text.splitlines(), 1):
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        parts = [p.strip() for p in line.split("|")]
        if len(parts) < 3 or not all(parts[:3]):
            raise ValueError(f"quarantine line {n}: need '<test> | <owner> | <reason>': {line!r}")
        out[parts[0]] = (parts[1], parts[2])
    return out


def is_quarantined(name, q):
    return any(name == e or name.endswith("::" + e) for e in q)


def split_failure(line):
    name, _, rest = line.strip().partition(" [")
    return name, rest.rstrip("]")


def classify(failure_lines, q):
    """-> (outside, inside): lists of (name, cmd, count)."""
    cnt = collections.Counter(split_failure(l) for l in failure_lines if l.strip())
    outside = [(n, c, k) for (n, c), k in sorted(cnt.items()) if not is_quarantined(n, q)]
    inside = [(n, c, k) for (n, c), k in sorted(cnt.items()) if is_quarantined(n, q)]
    return outside, inside


def pass_table(pass_lines, runs):
    """Per-test pass counts from lines '<test> [<cmd>] ok|FAILED' -> sorted rows (name, cmd, ok, failed)."""
    ok, bad = collections.Counter(), collections.Counter()
    for l in pass_lines:
        if not l.strip():
            continue
        head, _, st = l.rstrip().rpartition(" ")
        key = split_failure(head)
        (ok if st == "ok" else bad)[key] += 1
    keys = sorted(set(ok) | set(bad))
    return [(n, c, ok[(n, c)], bad[(n, c)]) for n, c in keys]


def report(failure_lines, q, runs=None, pass_lines=None):
    outside, inside = classify(failure_lines, q)
    o = []
    if pass_lines is not None:
        rows = pass_table(pass_lines, runs)
        o.append(f"{len(rows)} distinct tests seen; per-test pass counts (tests that never failed are summarised):")
        flaky = [r for r in rows if r[3]]
        for n, c, ok, bad in flaky:
            o.append(f"  {n} [{c}]: passed {ok}, failed {bad}" + (f" of {runs}" if runs else ""))
        o.append(f"  {len(rows) - len(flaky)} tests passed every time they ran")
    for n, c, k in inside:
        o.append(f"QUARANTINED  {n} [{c}] failed {k}x (owner {q[next(e for e in q if n == e or n.endswith('::' + e))][0]})")
    for n, c, k in outside:
        o.append(f"FAIL  {n} [{c}] failed {k}x and is not on the quarantine list: a flaky test is a failing test (ADR-0080)")
    return "\n".join(o), bool(outside)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--quarantine", default=str(DEFAULT_Q))
    ap.add_argument("--check")
    ap.add_argument("--failures")
    ap.add_argument("--passes")
    ap.add_argument("--runs", type=int)
    a = ap.parse_args()
    qp = pathlib.Path(a.quarantine)
    try:
        q = parse_quarantine(qp.read_text(encoding="utf-8")) if qp.exists() else {}
    except ValueError as e:
        print(e, file=sys.stderr)
        return 2
    if a.check:
        return 0 if is_quarantined(a.check, q) else 1
    fl = pathlib.Path(a.failures).read_text(encoding="utf-8", errors="replace").splitlines() if a.failures else []
    pl = pathlib.Path(a.passes).read_text(encoding="utf-8", errors="replace").splitlines() if a.passes else None
    text, bad = report(fl, q, a.runs, pl)
    print(text)
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
