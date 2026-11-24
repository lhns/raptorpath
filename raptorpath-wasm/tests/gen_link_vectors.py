"""Generate tests/link_vectors.txt: the forward leg and the resend budget, driven through a real toy2.Sim.

The model's forward leg (src/link.rs) and resend budget (src/budget.rs) against the offline simulation
(tools/toy/toy2.py on toy-stage3-knobs), per addendum 1 A4 and A5 of
docs/derivations/visualizer-three-knobs-plan.md. Same toy extraction and invocation as gen_knob_vectors.py:
    python -I raptorpath-wasm/tests/gen_link_vectors.py --toy-dir DIR --toy-sha SHA [--check]

Sections (an f64 is its 16-hex big-endian bit pattern, `#` starts a comment):
- `leg_cfg <C> <Q> <owd>` starts a leg: a fresh toy2.Sim with link = dict(C, Q), J = 0, no sawtooth, the
  propagation delay as its owd. Each following `leg <t> -> <departure> <arrival>` or `leg <t> -> drop` is
  one Sim.fwd(t) in order; the model feeds src/link.rs the same seconds with sv = 1/C and q = Q/C.
- `brake <tick> -> <toy> <model> <toy|boundary>`: one resend attempt at now = tick * 0.001 s through
  Sim.send_copy with resend_brake = "engine" (1 = admitted). `model` is the integer-tick window form
  (window = tick / 5, at most 10 per window); `boundary` marks the attempts where the toy's
  int(now / 0.005) puts the attempt in another window, and every later attempt whose decision that moves.
"""
import argparse
import difflib
import math
import os
import struct
import sys

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "link_vectors.txt")


def hx(x):
    return struct.pack(">d", float(x)).hex()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--toy-dir", required=True)
    ap.add_argument("--toy-sha", required=True)
    ap.add_argument("--check", action="store_true", help="only print the diff against the committed file")
    a = ap.parse_args()
    sys.path.insert(0, os.path.abspath(a.toy_dir))
    import toy2  # noqa: E402

    rows = []
    # ---- the forward leg: bursts above the link rate, a steady stream below it, idle gaps, exact ties
    legs = [(4000.0, 400.0, 0.025), (1000.0, 50.0, 0.0), (4750.0, 475.0, 0.0125), (2000.0, 3.0, 0.04)]
    for C, Q, owd in legs:
        sim = toy2.Sim(toy2.Cfg(link=dict(C=C, Q=Q), J=0.0, q=False, owd=owd, dur=2.0))
        rows.append(f"leg_cfg {hx(C)} {hx(Q)} {hx(owd)}")
        times = []
        times += [0.1] * 60                                   # a burst at one instant: queue, then drops
        times += [0.1 + k * 0.0004 for k in range(200)]       # above the rate for C < 2500, below for C = 4000+
        times += [0.5 + k * 0.001 for k in range(300)]        # one per tick, from an emptied queue
        times += [0.9 + k * 0.001 for k in range(0, 300, 7)]  # sparse sends after an idle gap
        times += [1.3 + (k // 4) * 0.001 for k in range(400)]  # four per tick (the model's cap 4 slots)
        for t in times:
            arr, dep = sim.fwd(t)
            if arr is None:
                rows.append(f"leg {hx(t)} -> drop")
            else:
                rows.append(f"leg {hx(t)} -> {hx(dep)} {hx(arr)}")

    # ---- the resend budget: attempt bursts of 0..7 per tick over 400 ticks
    sim = toy2.Sim(toy2.Cfg(resend_brake="engine", J=0.0, dur=2.0))
    used = {}
    seq = 0
    rng = 0x9E3779B97F4A7C15
    n_boundary = 0
    for tick in range(400):
        rng = (rng * 6364136223846793005 + 1442695040888963407) % (1 << 64)
        burst = (rng >> 33) % 8
        for _ in range(burst):
            now = tick * 0.001
            toy_ok = sim.send_copy(now, seq)
            seq += 1
            w = tick // 5
            model_ok = used.get(w, 0) < 10
            if model_ok:
                used[w] = used.get(w, 0) + 1
            off_window = int(now / 0.005) != w
            tag = "boundary" if (off_window or toy_ok != model_ok) else "toy"
            if tag == "boundary":
                n_boundary += 1
            rows.append(f"brake {tick} -> {int(toy_ok)} {int(model_ok)} {tag}")
    rows.insert(0, f"# brake: {n_boundary} boundary attempts (the toy's float window differs from tick / 5)")

    text = "\n".join([
        "# Forward-leg and resend-budget vectors (raptorpath-wasm/tests/gen_link_vectors.py). Do not edit by hand.",
        f"# toy: toy-stage3-knobs {a.toy_sha} (tools/toy/toy2.py, Sim.fwd and Sim.send_copy)",
    ] + rows) + "\n"
    old = open(OUT, encoding="utf-8").read() if os.path.exists(OUT) else ""
    diff = list(difflib.unified_diff(old.splitlines(), text.splitlines(), "committed", "generated", lineterm=""))
    if diff:
        print("\n".join(diff[:40]))
        print(f"# {len(diff)} diff lines")
    else:
        print("# no change against the committed file")
    if not a.check:
        with open(OUT, "w", encoding="utf-8", newline="\n") as f:
            f.write(text)
        print(f"# wrote {len(rows)} rows to {OUT}; brake boundary attempts {n_boundary}")
    return 1 if (a.check and diff) else 0


if __name__ == "__main__":
    sys.exit(main())
