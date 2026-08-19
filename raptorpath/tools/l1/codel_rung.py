#!/usr/bin/env python3
"""TIER-1 RE-SCORE 2a — THE CoDel RUNG, scored against data we ALREADY OWN.

Literature item: `docs/research/literature-crosscheck.md` Tier 1.1 — *"Score the
ladder's existing curves at the CoDel rung: `BDP*1.05` per cell."* RFC 8289 (CoDel)
§3.2 derives the standing-queue setpoint as **5-10 % of the RTT** from Kleinrock
power maximisation; the derived quantity is the RATIO 0.05, not the shipped 5 ms
(folklore correction 8 in the same document).

WHAT THIS SCRIPT IS AND IS NOT.

  IS      an arithmetic re-score of EXISTING ledgers against a rung derived from
          the cells' own measured anchors. No VM, no new arm, no new binary.
  IS NOT  a verdict on the unbuilt delta-cap. **No arm in this tree was ever run
          AT the derived setpoint**, so every cell below is scored as
          SUPPORT / CONTRADICT / INSUFFICIENT for the setpoint's NEIGHBOURHOOD,
          with the arm's own confounds printed beside it. A cell whose nearest
          arm is far from the rung reads INSUFFICIENT however good its numbers.

THE ARITHMETIC, written once.

  CoDel's target queue  Q*   = f * RTT * rate,  f in {0.05, 0.10}
  a sender-side ceiling that permits exactly that standing queue is
       cap*(f)          = BDP + Q*  =  rate*RTT + f*rate*RTT  =  BDP * (1 + f)
  so the rung needs ONE measured input per cell: the BDP anchor in symbols.
  Both anchor sources below are printed; neither is invented here.

THE CONFOUND RULE (MEASUREMENT DISCIPLINE 1: prove the mechanism under test
executes). A cap contrast is only readable against an arm that differs in the
CAP and as little else as possible. Every contrast below names its matched
control explicitly and every unmatched contrast is DOWNGRADED, never quietly
reported as if it were matched.

  usage: codel_rung.py <ledger.log> [<ledger.log> ...]
         (ladder-*.log, latlever-*.log, ccap-*.log — the format is autodetected
          from the JSON row's own `arm` vocabulary)
"""
import json
import math
import os
import sys
from collections import defaultdict

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")

# ── THE MEASURED ANCHORS. Both are TRANSCRIBED from the tree, with their source
#    on the line, and both are printed so the rung is never read off one number.
#
#  (1) COMPOSED-BATTERY ERA — ADR-0071 `docs/adr/0071-successor-candidates.md`
#      inputs table, `BDP = W/K` column, itself computed from the 833 `[3T]`
#      evaluations of the composed battery. This is the source the literature
#      cross-check's own c1/sc2/c7/c8/c8L numbers came from.
BDP_COMPOSED = {"c1": 174.8, "sc2": 328.1, "c7": 1106.1, "c8": 1604.8, "c8L": 4976.1}
#  (2) LADDER ERA — read off the ladder's OWN `[SUMCAP] ask=` on arm N, where
#      `ask = gain * Sigma` with `gain = 2` and the count multiplier deleted
#      (`net::pooled_store_cap_unclamped`, src/net/mod.rs). Sigma IS the pooled
#      BDP anchor `Sigma_i(max_bw_i * min_rtt_i)`. Duals only: the pooled law
#      returns None at `n_live < 2`, so the singles have no `[SUMCAP]` anchor
#      and fall back to source (1). Computed live below, not transcribed.
GAIN = 2.0

#: `sc3` and `c2r100`/`c2r200` appear only in the latency-lever ledger and have
#: no published BDP anchor in either source. They are REPORTED (their cap moves
#: are large and one-directional) and scored on NOTHING.
CELLS = ["c1", "sc2", "c7", "c8", "c8L"]

#: Headroom permissions, transcribed from the batteries' own discipline-16
#: tables (goal-gate "Ladder Battery — RESULTS" and "Latency Lever — BATTERY").
#: A goodput claim is only admissible where the cell had headroom to lose.
HEADROOM = {"c1": 77.6, "c7": 5.6, "c8": 18.5, "c8L": 36.0, "sc2": 1.7}

#: The matched-control map: for each (ledger, arm) the arm that differs from it
#: in the CAP and in as little else as possible. `None` = no matched control
#: exists in that ledger, which downgrades every contrast taken from it.
#:
#:  ladder   A    shipped default (cap pinned at N*knee)
#:           N    RWM_SUM_CAP only            -> matched control for A
#:           T    the ledger/loss trio        -> NOT a cap arm
#:           NT   N+T                         -> matched control is T
#:           FULL NT+UNIFIED+LATE_BRAKE       -> matched control is NT
#:  latlever A    shipped default
#:           B    RWM_THREE_TERM=1 RWM_PLAIN_RS=1  -> matched control is D
#:           D    RWM_PLAIN_RS=1 (the ANCHOR alone, cap untouched)
#:  ccap     A    shipped default
#:           C    RWM_COMPOSED_CAP=1          -> matched control is A
MATCHED = {
    "ladder": {"N": "A", "NT": "T", "FULL": "NT"},
    "latlever": {"B": "D"},
    "ccap": {"C": "A"},
}
CONFOUNDS = {
    ("ladder", "N", "A"): "clean: RWM_SUM_CAP is the ONLY gate that differs",
    ("ladder", "NT", "T"): "clean for the cap: both arms carry the T trio; NT adds RWM_SUM_CAP only",
    ("ladder", "FULL", "NT"): "NOT clean: FULL adds RWM_STORE_CAP_UNIFIED + RWM_LATE_BRAKE beside the cap",
    ("latlever", "B", "D"): "clean for the cap: both arms carry RWM_PLAIN_RS; B adds RWM_THREE_TERM only",
    ("ccap", "C", "A"): "NOT clean: RWM_COMPOSED_CAP changes the LAW's shape, not only its magnitude",
}

ARM_VOCAB = {
    frozenset(["A", "N", "T", "NT", "FULL"]): "ladder",
    frozenset(["A", "B", "D"]): "latlever",
    frozenset(["A", "C"]): "ccap",
}


def med(v):
    v = sorted(x for x in v if x is not None)
    if not v:
        return None
    n = len(v)
    return v[n // 2] if n % 2 else (v[n // 2 - 1] + v[n // 2]) / 2.0


def mean(v):
    v = [x for x in v if x is not None]
    return sum(v) / len(v) if v else None


def sd(v):
    v = [x for x in v if x is not None]
    if len(v) < 2:
        return None
    m = sum(v) / len(v)
    return math.sqrt(sum((x - m) ** 2 for x in v) / (len(v) - 1))


def two_sigma_pooled(a, b):
    """2*sqrt(s_a^2/n_a + s_b^2/n_b) — the batteries' own 'is it real' bar."""
    sa, sb = sd(a), sd(b)
    if sa is None or sb is None:
        return None
    return 2.0 * math.sqrt(sa * sa / len(a) + sb * sb / len(b))


def f(x, p=1):
    return "-" if x is None else f"{x:.{p}f}"


# ── LOAD ────────────────────────────────────────────────────────────────────
rows = []
for path in sys.argv[1:]:
    base = os.path.basename(path.replace("\\", "/"))
    if "calib" in base or "smoke" in base:
        continue          # n = 1 passes carry no sigma and are not results
    with open(path, errors="replace") as fh:
        for ln in fh:
            i = ln.find('{"cell"')
            if i < 0:
                continue
            try:
                r = json.loads(ln[i:])
            except ValueError:
                continue
            r["_base"] = base
            rows.append(r)

# Group by ledger family, inferred from the arm vocabulary present in the file.
fam_arms = defaultdict(set)
for r in rows:
    fam_arms[r["_base"]].add(r.get("arm"))
fam_of = {}
for base, arms in fam_arms.items():
    fam_of[base] = ARM_VOCAB.get(frozenset(a for a in arms if a), "?")
    if fam_of[base] == "?":
        for key, name in ARM_VOCAB.items():
            if arms <= key:
                fam_of[base] = name
                break

by = defaultdict(list)
for r in rows:
    # LIVE only: an aborted invocation has no `[GATES]` on either endpoint and
    # is in no denominator (every predecessor battery's own convention).
    if not (r.get("gates_lines_cli") or r.get("gates_lines_srv")):
        continue
    by[(fam_of[r["_base"]], r["cell"], r.get("arm"))].append(r)

print("=" * 100)
print("TIER-1 RE-SCORE 2a — THE CoDel RUNG (RFC 8289 §3.2, the derived 5-10 % ratio)")
print("scored against EXISTING ledgers only. No VM, no new arm, no new binary.")
print("=" * 100)

# ── 1. THE RUNG, WITH ITS ARITHMETIC ────────────────────────────────────────
print("\n### 1. THE RUNG — cap*(f) = BDP * (1 + f), f in {0.05, 0.10}\n")

# Ladder-era Sigma, backed out of arm N's own [SUMCAP] ask (duals only).
bdp_ladder = {}
for c in CELLS:
    asks = [r.get("sumcap_ask") for r in by.get(("ladder", c, "N"), [])]
    a = med(asks)
    if a:
        bdp_ladder[c] = a / GAIN

print(f"{'cell':<6}{'BDP(composed)':>15}{'5% rung':>10}{'10% rung':>10}"
      f"{'BDP(ladder)':>13}{'5% rung':>10}{'10% rung':>10}{'headroom%':>11}")
for c in CELLS:
    b1 = BDP_COMPOSED[c]
    b2 = bdp_ladder.get(c)
    print(f"{c:<6}{b1:>15.1f}{b1 * 1.05:>10.0f}{b1 * 1.10:>10.0f}"
          f"{f(b2):>13}{f(b2 * 1.05, 0) if b2 else '-':>10}"
          f"{f(b2 * 1.10, 0) if b2 else '-':>10}{HEADROOM[c]:>11.1f}")
print("\n  BDP(composed): ADR-0071 inputs table `BDP = W/K`, 833 [3T] evaluations.")
print("  BDP(ladder)  : arm N's own `[SUMCAP] ask=` / gain, gain = 2")
print("                 (`net::pooled_store_cap_unclamped`). Duals only —")
print("                 `pooled_store_cap` returns None at n_live < 2.")
print("  The two eras DISAGREE and the disagreement is a finding, not noise:")
print("  the ladder's own Ladder-Battery RESULTS block records c8's Sigma at")
print("  1154.3, 23.5-28.1 % below both published anchors. Both rungs are")
print("  therefore carried through every verdict below.")

# ── 2. THE CAP LADDER ACTUALLY WALKED, per cell ─────────────────────────────
print("\n\n### 2. THE CAP POINTS THE EXISTING DATA ACTUALLY WALKED\n")
print("`cap` = `occcap_p50` median (the occupancy ceiling IN FORCE, reported on")
print("every arm of every ledger — the only cap measurand common to all three).")
print("`x5%`/`x10%` = cap / rung, on the COMPOSED anchor. A value near 1.0 is an")
print("arm at the CoDel class; the whole question is whether any arm is near 1.\n")
print(f"{'cell':<6}{'ledger':<9}{'arm':<6}{'n':>4}{'cap':>8}{'x5%':>7}{'x10%':>7}"
      f"{'mbps':>9}{'2sig':>8}{'ping50':>8}{'q_p50':>7}{'q_p99':>7}{'retx':>8}{'wall':>8}")
CAPROWS = {}
for c in CELLS:
    for fam in ("ladder", "latlever", "ccap"):
        for a in ("A", "N", "T", "NT", "FULL", "B", "D", "C"):
            rs = by.get((fam, c, a), [])
            if not rs:
                continue
            cap = med([r.get("occcap_p50") for r in rs])
            mb = [r["mbps"] for r in rs if r.get("mbps") is not None]
            CAPROWS[(fam, c, a)] = {"n": len(rs), "cap": cap, "mbps": mb,
                                    "ping": [r.get("ping_p50") for r in rs],
                                    "q50": [r.get("q_p50") for r in rs],
                                    "q99": [r.get("q_p99") for r in rs]}
            r5 = cap / (BDP_COMPOSED[c] * 1.05) if cap else None
            r10 = cap / (BDP_COMPOSED[c] * 1.10) if cap else None
            print(f"{c:<6}{fam:<9}{a:<6}{len(rs):>4}{f(cap, 0):>8}"
                  f"{f(r5, 2):>7}{f(r10, 2):>7}"
                  f"{f(mean(mb), 2):>9}{f(2 * sd(mb) if sd(mb) else None, 2):>8}"
                  f"{f(med([r.get('ping_p50') for r in rs]), 1):>8}"
                  f"{f(med([r.get('q_p50') for r in rs]), 0):>7}"
                  f"{f(med([r.get('q_p99') for r in rs]), 0):>7}"
                  f"{f(med([r.get('retx') for r in rs]), 0):>8}"
                  f"{f(med([r.get('wall_dur_ms') for r in rs]), 0):>8}")

# ── 3. THE MATCHED CONTRASTS ────────────────────────────────────────────────
#
# Every contrast is ORIENTED LOW-CAP vs HIGH-CAP regardless of which side is the
# nominal "arm", because the question is always what the LOWER cap cost. A
# contrast whose two caps agree within 10 % is not a cap contrast at all and is
# printed as `flat` and excluded from every verdict — that exclusion is what
# stops an arm that moved the LAW without moving the CAP from being read as one.
CAP_MOVE = 0.90

print("\n\n### 3. THE MATCHED CAP CONTRASTS — every one names its control\n")
print("oriented LOW-cap vs HIGH-cap. `d_*` = (low-cap arm) - (high-cap arm), so a")
print("negative `d_mbps` outside 2sigma_pooled is a REAL GOODPUT COST OF CAPPING")
print("LOWER, and a negative `d_ping`/`d_q50` is a delivered-latency GAIN.\n")
print(f"{'cell':<6}{'ledger':<9}{'low vs high':<14}{'cap lo':>8}{'cap hi':>8}"
      f"{'x5% lo':>8}{'ratio':>7}{'d_mbps':>9}{'2sig_p':>8}{'real?':>7}"
      f"{'d_ping':>9}{'d_q50':>8}")
CONTRASTS = []
for c in CELLS:
    for fam, mm in MATCHED.items():
        for arm, ctrl in mm.items():
            k1, k2 = (fam, c, arm), (fam, c, ctrl)
            if k1 not in CAPROWS or k2 not in CAPROWS:
                continue
            if CAPROWS[k1]["cap"] is None or CAPROWS[k2]["cap"] is None:
                continue
            (lo_a, LO), (hi_a, HI) = sorted(
                ((arm, CAPROWS[k1]), (ctrl, CAPROWS[k2])), key=lambda t: t[1]["cap"])
            ratio = LO["cap"] / HI["cap"] if HI["cap"] else 1.0
            d = mean(LO["mbps"]) - mean(HI["mbps"])
            s2 = two_sigma_pooled(LO["mbps"], HI["mbps"])
            if ratio > CAP_MOVE:
                real = "flat"
            else:
                real = ("LOSS" if (s2 and d < -s2) else
                        "GAIN" if (s2 and d > s2) else "within")
            dp = (med(LO["ping"]) - med(HI["ping"])
                  if med(LO["ping"]) is not None and med(HI["ping"]) is not None else None)
            dq = (med(LO["q50"]) - med(HI["q50"])
                  if med(LO["q50"]) is not None and med(HI["q50"]) is not None else None)
            x5 = LO["cap"] / (BDP_COMPOSED[c] * 1.05)
            CONTRASTS.append({"cell": c, "fam": fam, "lo": lo_a, "hi": hi_a,
                              "pair": f"{arm} vs {ctrl}", "cap": LO["cap"],
                              "cap_hi": HI["cap"], "ratio": ratio, "x5": x5,
                              "d": d, "s2": s2, "real": real, "dp": dp, "dq": dq,
                              "n": LO["n"], "n_hi": HI["n"]})
            print(f"{c:<6}{fam:<9}{lo_a + ' vs ' + hi_a:<14}{LO['cap']:>8.0f}{HI['cap']:>8.0f}"
                  f"{x5:>8.2f}{ratio:>7.2f}{d:>9.2f}{f(s2, 2):>8}{real:>7}"
                  f"{f(dp, 1):>9}{f(dq, 0):>8}")
print("\n  Confounds, per contrast pair (as defined by the ledger, not by orientation):")
for (fam, arm, ctrl), why in sorted(CONFOUNDS.items()):
    print(f"    {fam:<9}{arm} vs {ctrl:<5} — {why}")

# ── 4. THE PER-CELL VERDICT ─────────────────────────────────────────────────
#
# The rule, fixed here and applied uniformly rather than per cell:
#
#   the CANDIDATE at a cell is the matched contrast that (a) actually moved the
#   cap (ratio <= 0.90) and (b) whose LOW cap is closest, in log distance, to
#   the 5 % rung — with ties broken toward the CLEAN contrasts.
#
#   SUPPORT      a candidate exists inside [0.5, 2.0] x the rung and its goodput
#                delta is NOT a real loss.
#   CONTRADICT   a candidate exists inside that window and its delta IS a loss.
#   INSUFFICIENT no candidate lands inside the window — the data never visited
#                the setpoint's neighbourhood at this cell.
#
NEAR_LO, NEAR_HI = 0.5, 2.0
#: Cells VOIDED before their numbers are read, on arithmetic rather than on
#: outcome. c8L's memory bound (`N*knee` = 4096) is BELOW its own BDP anchor
#: (4976), so the 5 % rung 5225 is unreachable by construction on that cell and
#: no arm could have visited it. ADR-0071 finding 1 and the Ladder Battery's
#: pre-declared void say the same thing about the same cell.
PREVOID = {"c8L": "memory-starved: N*knee = 4096 < BDP 4976, so the 5 % rung "
                  "5225 is unreachable by construction (ADR-0071 finding 1)"}
CLEAN = {("ladder", "N", "A"), ("ladder", "NT", "T"), ("latlever", "B", "D")}
print("\n\n### 4. THE PER-CELL VERDICT\n")
print(f"rule: the candidate is the matched contrast that MOVED the cap (ratio <= "
      f"{CAP_MOVE})\nand whose low cap is closest in log distance to the 5 % rung. "
      f"SUPPORT if it lands\ninside [{NEAR_LO}, {NEAR_HI}] x the rung with no real goodput "
      f"loss; CONTRADICT if with one;\nINSUFFICIENT otherwise. Written once, applied to all five.\n")
for c in CELLS:
    if c in PREVOID:
        print(f"  {c:<5} INSUFFICIENT-VOID   ({PREVOID[c]})")
        continue
    moved = [x for x in CONTRASTS if x["cell"] == c and x["ratio"] <= CAP_MOVE]
    if not moved:
        print(f"  {c:<5} INSUFFICIENT-DATA   (no matched contrast moved the cap at all)")
        continue
    cand = min(moved, key=lambda x: (abs(math.log(x["x5"])),
                                     0 if (x["fam"], x["lo"], x["hi"]) in CLEAN
                                     or (x["fam"], x["hi"], x["lo"]) in CLEAN else 1))
    if not (NEAR_LO <= cand["x5"] <= NEAR_HI):
        print(f"  {c:<5} INSUFFICIENT-DATA   (nearest cap-moving contrast is "
              f"{cand['fam']}/{cand['lo']} vs {cand['hi']} at {cand['x5']:.2f}x the rung)")
        continue
    verdict = "CONTRADICT" if cand["real"] == "LOSS" else "SUPPORT"
    print(f"  {c:<5} {verdict:<19} {cand['fam']}/{cand['lo']} vs {cand['hi']}: cap "
          f"{cand['cap']:.0f} = {cand['x5']:.2f}x the 5 % rung ({cand['ratio']:.2f}x the "
          f"control's), n={cand['n']}/{cand['n_hi']}, goodput {cand['d']:+.2f} "
          f"(2sig_p {f(cand['s2'], 2)}) {cand['real']}, ping {f(cand['dp'], 1)} ms, "
          f"q_p50 {f(cand['dq'], 0)} ms")
print("\n  NO ARM IN THIS TREE WAS EVER RUN AT THE SETPOINT. Every line above is")
print("  a statement about the setpoint's NEIGHBOURHOOD from arms that were run")
print("  for other reasons, with the confounds printed in section 3.")
