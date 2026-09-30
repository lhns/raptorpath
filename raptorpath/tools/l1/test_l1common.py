#!/usr/bin/env python3
"""Offline tests for `l1common.py`, the helpers every L1 parser imports.

    python3 test_l1common.py        # exit 0 iff every check passes
"""
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import l1common as c  # noqa: E402

CHECKS = 0
FAILS = []


def check(cond, msg):
    global CHECKS
    CHECKS += 1
    print(("ok    " if cond else "FAIL  ") + msg)
    if not cond:
        FAILS.append(msg)


# ── strip ───────────────────────────────────────────────────────────────
check(c.strip("\x1b[2m[LAT]\x1b[0m n=5\r\n") == "[LAT] n=5", "strip removes ANSI, CR and the newline")
check(c.strip("plain") == "plain", "strip leaves a clean line alone")

# ── split_interleaved: THE INTERLEAVE CASE ──────────────────────────────
glued = ("[LAT] site=receiver n=5 final=1\x1b[2m2026-09-08T18:54:50.123Z\x1b[0m "
         "\x1b[32m INFO\x1b[0m raptorpath: cleaning up TUN interface\n")
pieces = c.split_interleaved(glued)
check(len(pieces) == 2, "a glued tracing record splits into two readouts (got %d)" % len(pieces))
check(pieces[0] == "[LAT] site=receiver n=5 final=1", "the readout piece ends at the marker")
check(c.is_final(pieces[0]), "final=1 survives the split")
check(not c.is_final(glued), "without the split the glued flush is NOT recognised")
check("cleaning up TUN" in pieces[1] and "\x1b" not in pieces[1], "the tracing piece is colour-stripped")
plain = "[SUCC] det=7 final=1 2026-09-08T18:54:50Z  INFO raptorpath: bye\n"
check(c.is_final(c.split_interleaved(plain)[0]), "plain (no ANSI) interleave also splits")
check(c.split_interleaved("[LAT] n=5 final=1\n") == ["[LAT] n=5 final=1"], "an unglued line is one piece")
lead = "2026-09-08T18:54:50Z  INFO raptorpath: starting\n"
check(c.split_interleaved(lead) == [lead.rstrip("\n")], "a line that IS a tracing record is not split at 0")
check(c.split_interleaved("") == [""], "an empty line stays one empty piece")

# ── read ────────────────────────────────────────────────────────────────
with tempfile.NamedTemporaryFile("w", suffix=".log", delete=False, encoding="utf-8", newline="") as f:
    f.write("[GATES] RWM_GEN=0\r\n" + glued + "\x1b[1m[RFA]\x1b[0m gen=0 fires=3\n")
    path = f.name
try:
    lines = c.read(path)
finally:
    os.unlink(path)
check(lines[0] == "[GATES] RWM_GEN=0", "read strips CR")
check(len(lines) == 4, "read splits the glued line (4 readouts from 3 physical lines, got %d)" % len(lines))
check(lines[-1] == "[RFA] gen=0 fires=3", "read strips ANSI")
check(c.read(os.path.join(HERE, "no-such-file.log")) == [], "a missing file reads as []")
check(c.read("") == [] and c.read(None) == [], "no path reads as []")

# ── is_final / FINAL_RE ─────────────────────────────────────────────────
check(c.is_final("[ETA] n=3 final=1"), "final=1 at the end")
check(c.is_final("[ETA] final=1 n=3"), "final=1 in the head")
check(not c.is_final("[ETA] final=10"), "final=10 is not the flag")
check(not c.is_final("[ETA] xfinal=1"), "xfinal=1 is not the flag")
check(not c.is_final(None) and not c.is_final(""), "None / empty are not final")

# ── last_with ───────────────────────────────────────────────────────────
ls = ["[RFA] fires=1", "[RFA] fires=9 final=1", "[RFA] fires=2", "[DIAG] t=1"]
check(c.last_with(ls, "[RFA]") == "[RFA] fires=9 final=1", "a final=1 line wins wherever it sits")
check(c.last_with(ls[:1] + ls[2:], "[RFA]") == "[RFA] fires=2", "without a flush the last line wins")
check(c.last_with(ls, "[CHI]") is None, "an absent gauge is None")

# ── fnum / inum / numeric_prefix ────────────────────────────────────────
check(c.fnum("0.25") == 0.25 and c.fnum("-") is None and c.fnum(None) is None
      and c.fnum("12ms") is None, "fnum is strict")
check(c.inum("7") == 7 and c.inum("3.0") == 3 and c.inum("3.5") is None
      and c.inum("-") is None, "inum")
check(c.numeric_prefix("41.0ms") == 41.0, "numeric_prefix 41.0ms")
check(c.numeric_prefix("3/5") == 3.0, "numeric_prefix 3/5")
check(c.numeric_prefix("-2.5e-3x") == -0.0025, "numeric_prefix signed exponent")
check(c.numeric_prefix("-") is None and c.numeric_prefix("none") is None
      and c.numeric_prefix(None) is None and c.numeric_prefix("") is None,
      "numeric_prefix on a non-number is None")

# ── field ───────────────────────────────────────────────────────────────
lat = "[LAT] site=receiver n=100 over=0 p0:n=60 tot_p99=30 p1:n=40 tot_p99=50"
check(c.field(lat, "n=") == "100", "field: the head wins over the p<id>: slots")
check(c.field(lat, "n") == "100", "field: the key may omit '='")
check(c.field(lat, "tot_p99=") == "30", "field: first occurrence")
check(c.field("[REQ] gen=7 on=1", "n=") is None, "field: n= does not match inside gen=")
check(c.field("[DIAG] rtt=41.0ms p0:rtt=40/wrtt=42", "rtt") == "41.0ms", "field: wrtt= is not rtt=")
check(c.field("[CHI] n=- max=0", "n") is None, "field: '-' is None")
check(c.field("[CHI] max=0", "n") is None, "field: absent key is None")
check(c.field(None, "n") is None and c.field("", "n") is None, "field: no line is None")
check(c.field("p0 sig_us=120/n7", "sig_us=") == "120/n7", "field: raw token returned")

# ── gate / gate_tok ─────────────────────────────────────────────────────
g = ["[GATES] RWM_GEN=384 RWM_X=1", "noise", "[GATES] RWM_GEN=0 RWM_X=0 RWM_XY=1 RWM_V=10"]
check(c.gate_tok(g, "RWM_GEN") == "0", "gate_tok reads the LAST [GATES] line")
check(c.gate(g, "RWM_X") == 0, "gate: RWM_X is not confused with RWM_XY")
check(c.gate(g, "RWM_XY") == 1, "gate: 1")
check(c.gate(g, "RWM_V") is None, "gate: a non-boolean value is None, not its first digit")
check(c.gate(g, "RWM_NOPE") is None and c.gate([], "RWM_X") is None, "gate: absent is None")

# ── q / med: THE ONE QUANTILE RULE ──────────────────────────────────────
check(c.med([3, 1, 2]) == 2, "odd median is the middle value")
check(c.med([4, 1, 3, 2]) == 2.5, "even n=4 median is the mean of the middles")
check(c.med([1, 2, 3, 4, 5, 6]) == 3.5, "even n=6 median is the mean of the middles (no half-even flip)")
check(c.med([1, 2]) == 1.5, "n=2 median")
check(c.med([7]) == 7 and c.q([7], 0.99) == 7, "n=1: every quantile is the value")
check(c.med([]) is None and c.q([], 0.5) is None, "empty is None")
check(c.med([None, 1, 3]) == 2, "None values are ignored")
check(c.q(list(range(1, 101)), 0.5) == 50.5, "q(1..100, .5) = 50.5")
check(abs(c.q(list(range(1, 101)), 0.99) - 99.01) < 1e-9, "q(1..100, .99) = 99.01 (linear)")
check(c.q([10, 20, 30], 0.0) == 10 and c.q([10, 20, 30], 1.0) == 30, "p=0 is min, p=1 is max")
check(c.q([0.1234567, 0.2], 0.0, 4) == 0.1235, "ndigits rounds")

# ── truth / truth_columns: the per-datagram loss truth ──────────────────
T0 = ("    [TRUTH] leg=0 dev=cli0 egress_dgrams=178743 egress_skbs=67444 gso=2.65 "
      "netem_sent_dgrams=178579 netem_dropped_skbs=72 backlog=0 lost=164 "
      "loss=0.000918 rcvbuf_drops=8 rcvbuf_scope=netns")
T1 = ("[TRUTH] leg=1 dev=cli1 egress_dgrams=- egress_skbs=- gso=- "
      "netem_sent_dgrams=100 netem_dropped_skbs=3 backlog=- lost=- loss=- "
      "rcvbuf_drops=8 rcvbuf_scope=netns")
QDISC = ("    QDISC cli0: qdisc netem 84c1: root ... Sent 218953955 bytes 178579 pkt "
         "(dropped 72, overlimits 0 requeues 0)")
t = c.truth(["noise", QDISC, T0, T1])
check(sorted(t) == [0, 1], "truth: one entry per leg, keyed by int leg")
check(t[0]["loss"] == 0.000918 and t[0]["lost"] == 164, "truth: loss and lost read as numbers")
check(t[0]["egress_dgrams"] == 178743 and t[0]["egress_skbs"] == 67444, "truth: egress datagrams and skbs")
check(t[0]["gso"] == 2.65 and t[0]["netem_dropped_skbs"] == 72, "truth: gso and the skb drop count")
check(t[0]["dev"] == "cli0" and t[0]["rcvbuf_drops"] == 8, "truth: dev and rcvbuf_drops")
check(t[1]["loss"] is None and t[1]["egress_dgrams"] is None and t[1]["backlog"] is None,
      "truth: '-' (unreadable counter) is None, never 0")
check(t[1]["netem_sent_dgrams"] == 100, "truth: the readable fields of a partial line survive")
# The absolute invariant the line encodes: lost = egress - netem_sent - backlog.
check(t[0]["lost"] == t[0]["egress_dgrams"] - t[0]["netem_sent_dgrams"] - t[0]["backlog"],
      "truth: lost = egress - netem_sent - backlog on the sample line")
check(abs(t[0]["loss"] - t[0]["lost"] / t[0]["egress_dgrams"]) < 1e-6, "truth: loss = lost / egress")
# netem's skb drop count is NOT the loss: here it reads 72/178579 = 0.0004,
# below the datagram truth 0.00092 by about the GSO factor.
check(t[0]["netem_dropped_skbs"] / t[0]["netem_sent_dgrams"] < t[0]["loss"] / 2,
      "truth: the netem skb counter understates the datagram truth on the sample")
check(c.truth([QDISC, "noise"]) == {} and c.truth([]) == {} and c.truth(None) == {},
      "truth: no [TRUTH] line is {} (a QDISC line is not truth)")
check(c.truth(["[TRUTH] dev=cli0 loss=0.1"]) == {}, "truth: a line with no leg is skipped")
later = T0.replace("loss=0.000918", "loss=0.5")
check(c.truth([T0, later])[0]["loss"] == 0.5, "truth: a repeated leg keeps its LAST line")
check(c.truth(["[TRUTH] leg=2 loss=0.01 lost=1"])[2]["loss"] == 0.01,
      "truth: leg index 2 (quad legs)")
cols = c.truth_columns([T0, T1])
check(cols["truth_loss_p0"] == 0.000918 and cols["truth_gso_p0"] == 2.65
      and cols["truth_lost_p0"] == 164 and cols["truth_egress_p0"] == 178743,
      "truth_columns: per-leg columns keyed p<i>")
check(cols["truth_loss_p1"] is None and cols["truth_rcvbuf_drops"] == 8,
      "truth_columns: an unread leg is None; rcvbuf is one per-netns column")
cols = c.truth_columns([], n_legs=2)
check(set(cols) == {"truth_loss_p0", "truth_lost_p0", "truth_egress_p0", "truth_gso_p0",
                    "truth_loss_p1", "truth_lost_p1", "truth_egress_p1", "truth_gso_p1",
                    "truth_rcvbuf_drops"} and all(v is None for v in cols.values()),
      "truth_columns: n_legs fixes the row shape with None when nothing was captured")

print("test_l1common: %d checks, %d failed" % (CHECKS, len(FAILS)))
sys.exit(1 if FAILS else 0)
