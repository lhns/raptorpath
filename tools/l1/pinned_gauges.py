"""Seats, archives and per-ledger-row gauge specs for the pinned-law check (S3 part 1).

A spec names a log field and what counts as "at its bound" for it, taken from the evidence column of
docs/derivations/liveness-audit.md section 1. It never names a class: the class is computed from the
logs (pinned_summary.py counts, pinned_law_check.py classifies). A row with no spec has no gauge (NG).

Gauge kinds (counts kept per seat, pooled over the seat's rows):
  mode   tick field: n ticks, ticks at the modal value.            PIN when the modal share >= threshold
  bind   tick predicate `pred`: ticks where it holds. `pre` is a predicate that must hold on at least
         one tick or the law's precondition never held (NR).      PIN when the bind share >= threshold
  zero   tick counter `field`: ticks reading 0, and `w`, ticks where the witness predicate holds; `pre` as for bind.
         All zero with a witness is PIN; all zero without one is UZ (rule 1, no execution witness).
  end    last line of `tag` per row, one number per row: how=share (value is a bind fraction),
         latched (min == max within the row), off (value equals `off`).
  gate   [GATES] key=value per row; every row at `off` is NR (the gate echo is the witness).
`absent_nr` says a seat without the field or line reads NR (the law is not live there); else NG.
"""
SEATS = ["c2 B", "c2 A", "c2 R", "c8 B", "c8 A", "c8 R", "E"]
ARCHIVES = {  # seat -> (archive path relative to the repo root, member directory marker, arm)
    "c2 B": ("docs/l1-raw/s51b/full/b51c2-full.tgz", "/diag-ld/", "HB"),
    "c2 A": ("docs/l1-raw/s51b/full/b51c2-full.tgz", "/diag-ld/", "HA"),
    "c2 R": ("docs/l1-raw/s51b/full/b51c2-full.tgz", "/diag-ld/", "HR"),
    "c8 B": ("docs/l1-raw/s51b/full/b51c8-full.tgz", "/diag-ld/", "HB"),
    "c8 A": ("docs/l1-raw/s51b/full/b51c8-full.tgz", "/diag-ld/", "HA"),
    "c8 R": ("docs/l1-raw/s51b/full/b51c8-full.tgz", "/diag-ld/", "HR"),
    "E": ("docs/l1-raw/s51e/s51e-session.tgz", "/rep51e/diag-ld/", "PM"),
}
# ledger scope word -> seats it covers (a PINNED/DEGENERATE tag with an unlisted scope string fails the check)
SCOPES = {
    "c8": ["c8 B", "c8 A", "c8 R"],
    "c1s": [], "c1d": [],  # single-path cells outside these seats
    "every measured rho < 1 point": ["E"],
    "all one-way cells": SEATS,
    "tunnel": SEATS,
    "L1 cells: Bulk with chi = 0": ["c2 B", "c8 B"],
    "L1: never fires": ["c2 B", "c8 B"],  # S06, read on Bulk only
    "r = 0": ["c2 B", "c8 B"],
    "below r = eps/(1-eps)": ["c2 B", "c8 B"],
    "c8: the law sees only old holes": ["c8 B", "c8 A", "c8 R"],
    "rho inert at Auto and Realtime under the shipped R": [],  # M04, model only
}

# tick predicates: parsed line (plib.parse) -> bool
PRED = {
    "wsize_clip": lambda p: p.get("wsize") == 201,
    "astar_ge_w": lambda p: p.get("a_star") is not None and p.get("wsize") is not None and p["a_star"] >= p["wsize"],
    "rr_cap": lambda p: p.get("rr") == 0.5,
    "rr_pos": lambda p: p.get("rr", 0) > 0,
    "cwnd_max": lambda p: p.get("cwnd") == 10000 * p.get("np", 1),
    "rtt_gt20": lambda p: p.get("rtt", 0) > 20,
    "nbud10": lambda p: p.get("nbud") == 10,
    "debt_zero": lambda p: p.get("debt") == 0,
    "debt_pos": lambda p: p.get("debt", 0) > 0,
    "np1": lambda p: p.get("np", 1) == 1,
    "np_multi": lambda p: p.get("np", 1) > 1,
    "fired": lambda p: p.get("mpr.fired", 0) > 0,
    "reports": lambda p: p.get("mpr.rep", 0) > 0,
    "fired_or_rep": lambda p: p.get("mpr.fired", 0) > 0 or p.get("mpr.rep", 0) > 0,
}

G = {}
def mode(row, tag, field, absent_nr=False, w=None): G[row] = dict(kind="mode", tag=tag, field=field, absent_nr=absent_nr, w=w)
def bind(row, tag, pred, pre=None, absent_nr=False): G[row] = dict(kind="bind", tag=tag, pred=pred, pre=pre, absent_nr=absent_nr)
def zero(row, tag, field, w, pre=None): G[row] = dict(kind="zero", tag=tag, field=field, w=w, pre=pre)
def end(row, side, tag, field, how, site=None, off=None, absent_nr=False):
    G[row] = dict(kind="end", side=side, tag=tag, site=site, field=field, how=how, off=off, absent_nr=absent_nr)
def gate(row, key, off): G[row] = dict(kind="gate", key=key, off=off)

mode("E01", "DIAG", "rtt"); mode("E03", "DIAG", "rtp_us"); mode("E04", "DIAG", "btlbw")
mode("E05", "DIAG", "wpl_us"); mode("E06", "DIAG", "pl"); mode("E07", "DIAG", "plu")
end("E10", "s", "QCLK", "w_us", "latched", site="receiver", absent_nr=True)   # w_us_min == w_us_max in the row
mode("F02", "DIAG", "win.1")                                                  # store cap = denominator of win=a/b
end("F01", "c", "SUMCAP", "pin", "share", absent_nr=True)
gate("F04", "RWM_THREE_TERM", "0"); gate("K08", "RWM_XPATH_REPAIR", "0"); gate("C12", "RWM_RECV_REQUEST_LAW", "0")
end("R05", "c", "CHI", "n", "off", off=0)
end("F03", "c", "DIAG", "eb_bursts", "off", off=0)
mode("R01", "SPAN", "rr")
bind("R02", "SPAN", "rr_cap", pre="rr_pos")
bind("R07", "SPAN", "wsize_clip")
bind("R08", "DIAG", "rtt_gt20")
bind("S02", "SPAN", "astar_ge_w")
mode("S03", "SPAN", "delta")
mode("S10", "SPAN", "owed", w="rr_pos")
end("S05", "s", "SHEDH", "floor_frac", "share")
bind("C09", "DIAG", "nbud10")
bind("C11", "SPAN", "debt_zero", pre="debt_pos")
zero("C04", "DIAG", "mpr.supp.1", "fired", pre="np1")      # the srtt/2 age gate runs at N = 1 only
zero("C01", "DIAG", "mpr.supp.2", "fired", pre="np_multi")  # the multipath law term, N >= 2
zero("C02", "DIAG", "mpr.fast", "reports")
end("C06", "c", "QCLK", "w_us_p", "latched", site="sender")                   # w_us_p05 == w_us_p95
end("C07", "s", "QCLK", "w_us_p", "latched", site="receiver", absent_nr=True)
end("C08", "s", "LATE", "sampler_bind", "share")
bind("K01", "DIAG", "cwnd_max")
mode("F06", "DIAG", "srel.1", absent_nr=True)
