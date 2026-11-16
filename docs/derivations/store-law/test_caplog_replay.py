#!/usr/bin/env python3
"""Test caplog_replay.py against a synthetic log. Run: python3 test_caplog_replay.py"""
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import caplog_replay as cr  # noqa: E402

fails = 0


def check(name, ok, detail=""):
    global fails
    print(("ok   " if ok else "FAIL ") + name + ((" -- " + str(detail)) if not ok else ""))
    if not ok:
        fails += 1


def line(i, br, cap, **kw):
    d = dict(seat="plain", on=1, sum=1, dc=1, b=2.0, q=0.1, n=1, psum=1000.0, gain=2.0, floor=10,
             pool=2048, smax=1024, boot=128, rep=0)
    d.update(kw)
    return (f"[CAPLOG] i={i} t={i * 5000} seat={d['seat']} br={br} on={d['on']} sum={d['sum']} dc={d['dc']} "
            f"b={d['b']} q={d['q']} n={d['n']} psum={d['psum']} gain={d['gain']} floor={d['floor']} "
            f"pool={d['pool']} smax={d['smax']} boot={d['boot']} cap={cap} rep={d['rep']}")


# Hand-computed caps (Bulk: q = 0.1, value = 1 + q = 1.1).
# Operation order is part of the law: store_cap.rs evaluates (value * count) * psum.
# With sum=0, n=7, psum=140: (1.1 * 7.0) * 140.0 ceils to 1079, while 1.1 * (7.0 * 140.0)
# would ceil to 1078 -- a replay in the other order would fail this row.
check("float order: ((1+q)*N)*psum, not (1+q)*(N*psum)",
      cr.replay(cr.parse_line(line(1, "pooled", 1079, sum=0, n=7, psum=140.0))) == 1079)

LOG = [
    "2026-11-15T00:00:00Z INFO noise that is not a record",
    line(1, "pooled", 1100),                                         # 1.1 * 1000, N=1
    line(2, "pooled", 1100, rep=3),                                  # repeats counted, not re-logged
    line(3, "pooled", 2200, n=2, psum=2000.0),                       # 1.1 * 2000
    line(4, "pooled", 2048, psum=1900.0),                            # 2090 > knee 1*2048 -> 2048 (pin)
    line(5, "pooled", 10, psum=3.0),                                 # 3.3 -> 4 < floor -> 10 (floor)
    line(6, "pooled", 1079, sum=0, n=7, psum=140.0),                 # the float-order row above
    line(7, "pooled", 2000, dc=0, psum=1000.0),                      # fixed gain 2.0 * 1000
    line(8, "chain", 1024, on=0, psum=900.0),                        # clamp(ceil(1800), 10, 1024)
    line(9, "boot", 128, psum=0.0),
    line(10, "tt", 77),
]

o = cr.replay_log(LOG)
check("11 lines, 10 records parsed, the noise line skipped", o["records"] == 10, o)
check("9 replayable records reproduce every logged cap", o["replayed"] == 9 and not o["mismatches"], o["mismatches"])
check("the three-term record is reported unreplayable, not a mismatch", o["unreplayable"] == 1)
check("refreshes covered counts the rep= skips", o["refreshes"] == 10 + 3, o["refreshes"])
check("the logged q agrees with the shipped band at b=2.0", o["q_off"] == 0)

# A corrupted cap is caught, with the replayed value reported.
bad = LOG[:]
bad[1] = line(1, "pooled", 1101)
ob = cr.replay_log(bad)
check("a wrong logged cap is a mismatch", len(ob["mismatches"]) == 1 and ob["mismatches"][0][1] == 1100, ob["mismatches"])

# An empty log is a failure of the witness (rule 1), via main().
import tempfile  # noqa: E402
with tempfile.TemporaryDirectory() as d:
    p = os.path.join(d, "x.log")
    open(p, "w").write("nothing here\n")
    check("main() fails on a log with no [CAPLOG] records", cr.main([p]) == 1)
    open(p, "w").write("\n".join(LOG) + "\n")
    check("main() passes on the good synthetic log", cr.main([p]) == 0)

# q_of_b at the three presets (Realtime 0.05, Auto 1/15, Bulk 0.10).
check("q(b): Realtime/Bulk anchors", abs(cr.q_of_b(0.5) - 0.05) < 1e-12 and abs(cr.q_of_b(2.0) - 0.10) < 1e-12)

sys.exit(1 if fails else 0)
