#!/usr/bin/env python3
"""LOCAL GATE for clause B's new reference. No VM, no engine, no network.

Clause B is only exact if THREE things are true, and each is asserted here
rather than described:

  1. the dump's wire format round-trips to the EXACT original timeline (if it
     did not, every successive-difference functional would be computed at the
     wrong lag and B would score a mis-timed series);
  2. `_tlag_pairs` implements the SAME band the engine implements, so that any
     beta away from 1 is the online gauge's window and decimation and never a
     difference of definition;
  3. each population functional is the one its gauge actually claims, checked
     against a closed form on a series whose answer is known.

Run: python3 test_tlagb_rttdump.py
"""

import sys

import tlagb_rttdump as R

FAILS = []


def check(name, cond, detail=""):
    if cond:
        print("  ok   %s" % name)
    else:
        print("  FAIL %s %s" % (name, detail))
        FAILS.append(name)


def approx(a, b, tol=1e-9):
    if a is None or b is None:
        return a is b
    return abs(a - b) <= tol * max(1.0, abs(b))


def emit_batches(pid, series, batch=256):
    """Re-encode a timeline the way the engine does, so the test exercises the
    real format and not a convenient one."""
    lines = []
    for s in range(0, len(series), batch):
        chunk = series[s : s + batch]
        t0 = chunk[0][0]
        prev = t0
        parts = []
        for t, v in chunk:
            parts.append("%d,%d;" % (t - prev, v))
            prev = t
        lines.append(
            "[RTTDUMP] p=%d t0=%d n=%d d=%s" % (pid, t0, len(chunk), "".join(parts))
        )
    return lines


print("1. THE FORMAT ROUND-TRIPS EXACTLY")
# A deliberately irregular timeline crossing several batch boundaries: constant
# spacing would hide an off-by-one in the delta encoding.
orig = []
t = 1_000_000
for i in range(1000):
    t += 37 + (i % 11) * 13
    orig.append((t, 500 + (i * 7) % 300))
got = R.parse_dump(emit_batches(3, orig))
check("path present", 3 in got)
check("sample count", got[3]["emitted"] == 1000, got[3]["emitted"])
check("timeline identical", got[3]["series"] == orig)
check("batch count", got[3]["batches"] == 4, got[3]["batches"])
check("nothing malformed", got[3]["malformed"] == 0)

print("2. INTERLEAVED PATHS AND UNSORTED ARRIVAL DO NOT CORRUPT A TIMELINE")
a = [(1000 + 100 * i, 10 + i) for i in range(300)]
b = [(1500 + 250 * i, 900 + i) for i in range(300)]
lines = emit_batches(0, a) + emit_batches(1, b)
lines = lines[::-1]  # arrival order is not emission order
got = R.parse_dump(lines)
check("path 0 recovered", got[0]["series"] == a)
check("path 1 recovered", got[1]["series"] == b)

print("3. A MALFORMED ENTRY IS COUNTED, NEVER SILENTLY DROPPED")
got = R.parse_dump(["[RTTDUMP] p=9 t0=5 n=3 d=0,100;garbage;20,300;"])
check("good entries kept", got[9]["emitted"] == 2, got[9]["emitted"])
check("bad entry counted", got[9]["malformed"] == 1, got[9]["malformed"])

print("4. THE CAP IS READ AS A TRUNCATION WITNESS")
got = R.parse_dump(
    ["[RTTDUMP] p=2 t0=1 n=1 d=0,50;", "[RTTDUMP-CAP] p=2 emitted=1 seen=99 — cap"]
)
check("capped latched", got[2]["capped"] is True)
check("seen recovered", got[2]["seen"] == 99, got[2]["seen"])

print("5. THE FUNCTIONALS ARE THE ONES THE GAUGES CLAIM")
# A series with a closed form: rtt alternates 100 / 300 at a fixed 1 ms
# spacing. mean = 200, every |x-mean| = 100, so sd (n-1) -> 100 as n grows and
# mad is exactly 100. Every successive difference is 200. P90 and P50 are 300
# and 100 on an even split under nearest-rank, so qsp = 200.
ser = [(1_000_000 + 1000 * i, 100 if i % 2 == 0 else 300) for i in range(1000)]
f = R.population_functionals(ser, tau_us=None)
check("n", f["n"] == 1000)
check("mad is the MEAN deviation", approx(f["mad"], 100.0), f["mad"])
check("sd is the sample stdev", approx(f["sd"], 100.0, tol=2e-3), f["sd"])
check("msd = median successive |diff|", approx(f["msd"], 200.0), f["msd"])
# qsp is checked on a RAMP rather than on the two-point series above, because
# nearest-rank on an even split is genuinely ambiguous and would be asserting
# the rounding rule rather than the functional. On 0..999 the ranks are
# unambiguous: P50 = sorted[round(999*0.50)] = sorted[500] = 500 and
# P90 = sorted[round(999*0.90)] = sorted[899] = 899, so qsp = 399.
ramp = [(1_000_000 + 1000 * i, i) for i in range(1000)]
fr = R.population_functionals(ramp)
check("qsp = P90 - P50 (nearest-rank, the engine's convention)",
      approx(fr["qsp"], 399.0), fr["qsp"])
# And the same rank convention on the two-point series is recorded rather than
# hidden: an even split puts BOTH P50 and P90 in the upper half, so qsp reads
# 0 there. A parser that expected 200 would be reading its own rounding rule
# into the gauge.
check("nearest-rank on an even split puts P50 in the upper half",
      approx(f["qsp"], 0.0), f["qsp"])
check("tlag is None with no tau (NO fallback constant)", f["tlag"] is None)
check("rate_hz reads the real spacing", approx(f["rate_hz"], 1000.0, tol=2e-3), f["rate_hz"])

print("6. THE TAU BAND IS THE ENGINE'S BAND")
# Spacing 1 ms. With tau = 4 ms the band is [4, 8] ms, so each anchor's partner
# is the sample exactly 4 ms back (the MOST RECENT at lag >= tau) -- i.e. 4
# steps, an even number, so on the alternating series every pair difference is
# 0 and the median is 0. That is a sharp, sign-carrying check: a band that
# selected an ODD lag would read 200 instead.
f = R.population_functionals(ser, tau_us=4000)
check("even lag -> zero dispersion", approx(f["tlag"], 0.0), f["tlag"])
check("one pair per eligible anchor", f["tlag_pairs"] == 996, f["tlag_pairs"])
# tau = 5 ms -> partner 5 steps back, an ODD lag, so every difference is 200.
f = R.population_functionals(ser, tau_us=5000)
check("odd lag -> full swing", approx(f["tlag"], 200.0), f["tlag"])

print("7. THE BAND'S UPPER EDGE ACTUALLY EXCLUDES")
# One sample per 10 ms against tau = 1 ms: the band is [1, 2] ms and the
# nearest partner is 10 ms back, outside it. NO pair is admissible, and the
# functional must be None rather than 0 -- the distinction between "measured
# zero dispersion" and "could not measure" is the whole point of the n column.
sparse = [(1_000_000 + 10_000 * i, 100 + i) for i in range(100)]
f = R.population_functionals(sparse, tau_us=1000)
check("no pair inside the band", f["tlag_pairs"] == 0, f["tlag_pairs"])
check("unmeasurable is None, not 0", f["tlag"] is None, f["tlag"])
# Widen tau so the 10 ms spacing lands inside [tau, 2 tau]: at tau = 6 ms the
# band is [6, 12] ms and every consecutive pair qualifies.
f = R.population_functionals(sparse, tau_us=6000)
check("band admits at the right tau", f["tlag_pairs"] == 99, f["tlag_pairs"])
check("and reads the real step", approx(f["tlag"], 1.0), f["tlag"])

print("8. RATE INVARIANCE OF THE ESTIMAND, THE PROPERTY THE WHOLE PASS CLAIMS")
# THE point of section 16.75, asserted on a series where the answer is known.
# One underlying process -- rtt rises 1 µs per ms of elapsed time -- sampled at
# two rates 20x apart. `msd` (a fixed SAMPLE lag) must read 20x apart because
# its lag IS the spacing. `tlag` at a fixed tau must read the SAME.
dense = [(1_000_000 + 500 * i, 1000 + (500 * i) // 1000) for i in range(4000)]
sparse2 = [(1_000_000 + 10_000 * i, 1000 + (10_000 * i) // 1000) for i in range(200)]
fd = R.population_functionals(dense, tau_us=40_000)
fs = R.population_functionals(sparse2, tau_us=40_000)
check(
    "the two legs really do differ 20x in rate",
    approx(fd["rate_hz"] / fs["rate_hz"], 20.0, tol=1e-2),
    "%s vs %s" % (fd["rate_hz"], fs["rate_hz"]),
)
check(
    "msd (fixed SAMPLE lag) is rate-DEPENDENT -- the defect",
    fs["msd"] > 5 * fd["msd"],
    "dense %s vs sparse %s" % (fd["msd"], fs["msd"]),
)
check(
    "tlag (fixed TIME lag) is rate-INVARIANT -- the claim",
    approx(fd["tlag"], fs["tlag"], tol=0.10),
    "dense %s vs sparse %s" % (fd["tlag"], fs["tlag"]),
)

print("9. AN EMPTY OR SINGLETON STREAM IS UNDEFINED EVERYWHERE, NOT ZERO")
f = R.population_functionals([], tau_us=1000)
check("empty n", f["n"] == 0)
check("empty sd", f["sd"] is None)
check("empty msd", f["msd"] is None)
check("empty tlag", f["tlag"] is None)
f = R.population_functionals([(1, 100)], tau_us=1000)
check("singleton sd undefined (n-1 = 0)", f["sd"] is None)
check("singleton msd undefined", f["msd"] is None)

print("10. THE POP_FUNC MAP COVERS EVERY GAUGE THE REPORT SCORES")
check(
    "five gauges mapped",
    sorted(R.POP_FUNC) == ["msd", "qsp", "rvar", "sig", "tlag"],
    sorted(R.POP_FUNC),
)
keys = set(R.population_functionals([(1, 2), (2, 3)], 1).keys())
check(
    "every mapped functional is produced",
    all(v in keys for v in R.POP_FUNC.values()),
    R.POP_FUNC,
)
check("the engine's band width is transcribed", R.TLAG_BAND_C == 2)
check("the UNSCOREABLE-THIN floor is transcribed", R.K_THIN == 32)

print()
if FAILS:
    print("FAILED: %d" % len(FAILS))
    for f in FAILS:
        print("  - %s" % f)
    sys.exit(1)
print("ALL GREEN")
