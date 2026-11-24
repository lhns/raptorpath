"""Generate tests/knob_vectors.txt from the offline simulation's law functions.

The model's laws (src/knobs.rs) and the toy (tools/toy/toy2.py on toy-stage3-knobs) cannot share
code, so they share vectors (plan docs/derivations/visualizer-three-knobs-plan.md section 5.3).

Usage (outside the toy directory; the toy imports numpy):
    git show github/toy-stage3-knobs:tools/toy/toy.py  > DIR/toy.py
    git show github/toy-stage3-knobs:tools/toy/toy2.py > DIR/toy2.py
    python -I raptorpath-wasm/tests/gen_knob_vectors.py --toy-dir DIR --toy-sha SHA [--check]

Line format: `<rust_name> <arg>... -> <result> <source>`. An f64 is its 16-hex big-endian bit pattern,
a bool is 0 or 1. `toy` rows come from the toy's own function; `formula` rows from the derivation's
formula where the toy has no single function (L6 is event-structured there; Revision 3's d(s) and
one-way term are not in the toy at its recorded sha). `#` starts a comment.
"""
import argparse
import difflib
import math
import os
import struct
import sys
import types

KNOB_LAW_REVISION = "17 (Revision 3)"
INF = math.inf
OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "knob_vectors.txt")


def hx(x):
    return struct.pack(">d", float(x)).hex()


def b(v):
    return "1" if v else "0"


def around(x):
    """x and its two IEEE neighbours (the exact-equality corners)."""
    return [math.nextafter(x, -INF), x, math.nextafter(x, INF)]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--toy-dir", required=True)
    ap.add_argument("--toy-sha", required=True)
    ap.add_argument("--check", action="store_true", help="only print the diff against the committed file")
    a = ap.parse_args()
    sys.path.insert(0, os.path.abspath(a.toy_dir))
    import toy2  # noqa: E402

    T_GRID = [0.01] + [0.4 * (1 + s * k) for k in (0.0, 0.01, 0.02) for s in ((1,) if k == 0 else (-1, 1))] \
        + [2.0, 1e6, INF]
    rows = []

    def row(name, args, res, src):
        rows.append(f"{name} {' '.join(hx(x) for x in args)} -> {res} {src}")

    for T in T_GRID:
        ages = sorted(set([0.0, 0.5 * T if T < INF else 1e9, 1e9] + (around(T) if T < INF else [])))
        for age in ages:
            row("intake_expired", [T, age], b(toy2.intake_expired_fn(T, age)), "toy")
            row("within_lifetime", [T, age], b(toy2.within_lifetime(T, age)), "toy")
        for x in (0.0, 0.025, 0.05):
            base = T - x if T < INF else 1e9
            for age in sorted(set([0.0, 1e9] + around(base))):
                row("resend_admissible", [T, age, x], b(toy2.resend_admissible(T, age, x)), "toy")
        for m in (0.052, 0.1):
            base = T - m if T < INF else 1e9
            for age in sorted(set([0.0, 1e9] + around(base))):
                row("dequeue_expired", [T, age, m], b(toy2.dequeue_expired(T, age, m)), "toy")
        # protected: the toy's 3c function takes srtt_eff and halves it; Revision 3 takes the one-way
        # term o_hat. With srtt_eff = 2*o_hat the expressions are identical (x*2/2 is exact).
        for now in (0.6, 1.0):
            for ls in (now - 0.2, now - 0.05, now):
                for d in (0.05, 0.1):
                    for o in (0.025, 0.05):
                        h = 0.5
                        edge = max(now, ls + d) - h + o
                        for TT in sorted(set([T] + (around(edge) if T == T_GRID[0] else []))):
                            row("protected", [TT, now, ls, d, h, o], b(toy2.protected(TT, now, ls, d, h, 2.0 * o)), "toy")

    for H in T_GRID + [0.0, 0.02 * 0.98, 0.02, 0.02 * 1.02]:
        for td in (0.1, 1.0):
            for tf in (0.2, 1.01, INF):
                for hf in (0.0, 0.05):
                    row("release_at", [td, H, tf, hf], hx(min(td + H, tf + hf)), "formula")
                    row("released_by_hold", [td, H, tf, hf], b(td + H <= tf + hf), "formula")

    # The ack timeout is the toy's method Sim.R_now, evaluated on a stub carrying only its inputs.
    for srtt in (0.005, 0.05, 0.08, 0.333):
        for rv in (0.0, 0.0001, srtt / 8.0, srtt / 2.0):
            stub = types.SimpleNamespace(srtt=srtt, rttvar=rv, cfg=types.SimpleNamespace(tick=0.001))
            row("ack_timeout", [srtt, rv, 0.001, 0.001], hx(toy2.Sim.R_now(stub)), "toy")

    # Revision 3 (three-knobs.md 17.1): d(s) and the one-way term ô, not in the toy at its recorded sha.
    for resent in (False, True):
        for c in (0.01, 0.05):
            for sp in (0.05, 0.08):
                for pr in (0.025, 0.1, INF):
                    v = c if resent else min(sp, pr)
                    rows.append(f"trigger_delay {b(resent)} {hx(c)} {hx(sp)} {hx(pr)} -> {hx(v)} formula")
    for srtt in (0.05, 0.08, 0.142):
        for rp in (0.05, 0.08, 0.098):
            row("one_way_fwd", [srtt, rp], hx(srtt - rp / 2.0), "formula")

    text = "\n".join([
        "# Three-knob law vectors (raptorpath-wasm/tests/gen_knob_vectors.py). Do not edit by hand.",
        f"# toy: toy-stage3-knobs {a.toy_sha} (tools/toy/toy2.py)",
        f"# KNOB_LAW_REVISION: {KNOB_LAW_REVISION}",
        "# format: <rust_name> <args as 16-hex f64 bits, or 0/1 for a bool> -> <result> <toy|formula>",
    ] + rows) + "\n"
    old = open(OUT, encoding="utf-8").read() if os.path.exists(OUT) else ""
    diff = list(difflib.unified_diff(old.splitlines(), text.splitlines(), "committed", "generated", lineterm=""))
    if diff:
        print("\n".join(diff[:200]))
        print(f"# {len(diff)} diff lines")
    else:
        print("# no change against the committed file")
    if not a.check:
        with open(OUT, "w", encoding="utf-8", newline="\n") as f:
            f.write(text)
        print(f"# wrote {len(rows)} vectors to {OUT}")
    return 1 if (a.check and diff) else 0


if __name__ == "__main__":
    sys.exit(main())
