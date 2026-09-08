#!/usr/bin/env python3
"""Offline exercise of `eta_s4.py` on SYNTHETIC ledger lines.

    python3 test_eta_s4.py

NO ENGINE, NO VM. The `[ETA]` lines are transcribed from `net/eta.rs:424-427`
(sender) and `:574-580` (receiver); the ledger prefix is `tail_matrix.sh`'s
own scrape shape (`  EVICT <arm> <size>B <endpoint>: [ETA] ...`) and the
context lines are `crownspot8.sh` / `tail_matrix.sh`'s.

WHAT IS UNDER TEST: the `final=1` exit-flush rule. `eta_s4.py` COUNTS
readings (every `[ETA]` line it meets is a point, and `pool()` takes
min/median/max over them), so a flush line that read as one more point would
inflate `n` and could turn a degenerate range into a spurious "spread". The
rule is that a `final=1` point SUPERSEDES the cadence points of the same key
and a ledger with no `final=` field parses exactly as before.
"""
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import eta_s4 as s4  # noqa: E402

CHECKS = 0
FAILS = []


def check(cond, msg):
    global CHECKS
    CHECKS += 1
    if not cond:
        FAILS.append(msg)
        print("FAIL", msg)


def sender(sig1, n1, sig2=None, n2=None, t_eff="-", t_n=0):
    s = ("[ETA] site=sender fhat_us=1950 n=4000 zero=0.2500 place_n=1200 cold_r=0.0100 "
         "cold_ge=0.0000 t_eff=%s t_cold=- t_n=%d hol_sh=- hol_n=0 hol_mv=- hol_calls=0 "
         "hol_w=- p1:n=3000/4000 drop=0 tau_us=10000 e_p50=100 e_p90=200 e_p99=300 "
         "e_mx=400 late=0.3333 sig_us=%s/n%d" % (t_eff, t_n, sig1, n1))
    if sig2 is not None:
        s += (" p2:n=2000/4000 drop=1 tau_us=40000 e_p50=150 e_p90=250 e_p99=350 "
              "e_mx=450 late=0.5000 sig_us=%s/n%d" % (sig2, n2))
    return s


def receiver(sig, n, total=2000, bind="0.0100"):
    return ("[ETA] site=receiver n=%d p1:n=%d bind=%s tau_us=10000 srtt_src=wire "
            "l_p50=900 l_p90=1500 l_p95=1800 l_p99=2200 l_mx=3000 sig_us=%s/n%d minrst=0"
            % (total, total, bind, sig, n))


def ledger(lines, cell="c2", seed="42", arm="ship", size="400"):
    out = ["=== crownspot8 CROWNSPOT stage seed=%s cell=%s\n" % (seed, cell),
           "--- %s %sB\n" % (arm, size)]
    for endpoint, ln in lines:
        out.append("  EVICT %s %sB %s: %s\n" % (arm, size, endpoint, ln))
    return "".join(out)


def parse_text(text):
    with tempfile.NamedTemporaryFile("w", suffix=".log", delete=False, encoding="utf-8") as f:
        f.write(text)
        p = f.name
    try:
        return s4.parse_ledger(p)
    finally:
        os.unlink(p)


# ── 1. NO `final=` ANYWHERE: the pre-flush format parses as before ────────
pts = parse_text(ledger([("tm-c.log", sender(1668, 27, 2633, 31)),
                         ("tm-s.log", receiver(3430, 13))]))
check(len(pts) == 3, "3 path-readings (2 sender paths + 1 receiver), got %d" % len(pts))
check(all(p.final is False for p in pts), "no point is final")
check(pts[0].cell == "c2" and pts[0].seed == "42" and pts[0].arm == "ship"
      and pts[0].size == "400" and pts[0].endpoint == "tm-c.log",
      "ledger context + scrape prefix carried")
check(pts[0].sigma_us == 1668.0 and pts[0].pairs == 27.0 and pts[0].tau_us == 10000.0,
      "sender p1 sig_us/pairs/tau_us")
check(pts[1].path == "2" and pts[1].sigma_us == 2633.0 and pts[1].tau_us == 40000.0, "sender p2")
check(pts[2].site == "receiver" and pts[2].bind == 0.01 and pts[2].sigma_us == 3430.0, "receiver")

# ── 2. `final=1` SUPERSEDES the cadence lines of the same key ─────────────
raw = "".join(ln + "\n" for ln in (
    receiver(1000, 5, total=500),
    receiver(2000, 9, total=1000),
    receiver(3430, 13, total=2000) + " final=1",
))
pts = parse_text(raw)
check(len(pts) == 1, "3 receiver lines, one of them final -> ONE reading, got %d" % len(pts))
check(pts[0].final is True and pts[0].sigma_us == 3430.0 and pts[0].pairs == 13.0,
      "the reading is the flush's complete count")
# and the flush wins wherever it sits: a stale cadence line AFTER it is dropped
raw2 = raw + receiver(2500, 11, total=1500) + "\n"
pts = parse_text(raw2)
check(len(pts) == 1 and pts[0].sigma_us == 3430.0, "a cadence line after the flush is dropped")
# `final=1` in head position (before the paths) is the same flag
pts = parse_text("[ETA] site=receiver n=2000 final=1 " + receiver(3430, 13).split(" ", 3)[3] + "\n")
check(len(pts) == 1 and pts[0].final and pts[0].sigma_us == 3430.0, "final=1 in the head")
# NOT the flag
pts = parse_text(receiver(3430, 13) + " final=10\n" + receiver(3430, 13) + " xfinal=1\n")
check(len(pts) == 2 and not any(p.final for p in pts), "final=10 / xfinal=1 are not the flag")

# ── 3. THE RULE IS PER KEY: another arm's cadence points are untouched ────
text = (ledger([("tm-s.log", receiver(1000, 5)),
                ("tm-s.log", receiver(3430, 13) + " final=1")], arm="ship")
        + ledger([("tm-s.log", receiver(2000, 9))], arm="rlc"))
pts = parse_text(text)
check(len(pts) == 2, "ship collapses to its flush, rlc keeps its cadence point (got %d)" % len(pts))
check(sorted((p.arm, p.sigma_us) for p in pts) == [("rlc", 2000.0), ("ship", 3430.0)],
      "per-key supersession")

# ── 4. SCORING AND THE READING run end-to-end on both formats ─────────────
def run_main(text):
    with tempfile.NamedTemporaryFile("w", suffix=".log", delete=False, encoding="utf-8") as f:
        f.write(text)
        p = f.name
    import io
    import contextlib
    buf = io.StringIO()
    try:
        with contextlib.redirect_stdout(buf):
            rc = s4.main([p])
    finally:
        os.unlink(p)
    return rc, buf.getvalue()


two_cells = (ledger([("tm-c.log", sender(1900, 40)), ("tm-s.log", receiver(1700, 35))], cell="c2", arm="a")
             + ledger([("tm-c.log", sender(2100, 41)), ("tm-s.log", receiver(1800, 36))], cell="c2", arm="b")
             + ledger([("tm-c.log", sender(7600, 42)), ("tm-s.log", receiver(7200, 37))], cell="c3", arm="a")
             + ledger([("tm-c.log", sender(8000, 43)), ("tm-s.log", receiver(7700, 38))], cell="c3", arm="b"))
rc, out = run_main(two_cells)
check(rc == 0 and "READING: " in out, "main() runs on the pre-flush format")
check("of which exit-flushed (final=1): 0" in out, "header reports 0 flushed points")
# the same ledger with every scraped line an exit flush reads IDENTICALLY
flushed = two_cells.replace("/n35 minrst=0", "/n35 minrst=0 final=1") \
                   .replace("/n36 minrst=0", "/n36 minrst=0 final=1") \
                   .replace("/n37 minrst=0", "/n37 minrst=0 final=1") \
                   .replace("/n38 minrst=0", "/n38 minrst=0 final=1")
rc2, out2 = run_main(flushed)
check(rc2 == 0 and "of which exit-flushed (final=1): 4" in out2, "header reports 4 flushed points")
verdict = [l for l in out.splitlines() if l.startswith("READING:")]
verdict2 = [l for l in out2.splitlines() if l.startswith("READING:")]
check(verdict == verdict2 and verdict, "the reading is the same with and without the flush field: %s" % verdict)
import re
POOLED_ROW = re.compile(r"^  c[23]\s+(sender|receiver)\s")
pooled = [l for l in out.splitlines() if POOLED_ROW.match(l)]
pooled2 = [l for l in out2.splitlines() if POOLED_ROW.match(l)]
check(pooled == pooled2 and len(pooled) == 4, "pooled table identical (n counts not inflated)")
check(all("(n=2)" in l for l in pooled), "n = 2 reps per cell x site, not 3")

print("test_eta_s4: %d checks, %d failed" % (CHECKS, len(FAILS)))
sys.exit(1 if FAILS else 0)
