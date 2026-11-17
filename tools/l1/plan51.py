#!/usr/bin/env python3
"""Status 51, block 51: the off-Bulk baseline and the delta sweep (plan 51). Tables, per-arm run spec, row witnesses, the replacement rule and the slot check.
docs/derivations/s51-design.md sections 2 (cells, arms, order), 4 (VOID and witnesses), 6 (harness items 1-7) with the three corrections of s51-evict-design.md section 9.2: the per-row hint rides the armspec
(`ARM_SPEC[arm][2]`), the emission-batching echo is a NEW witness field (the 4th ARM_SPEC field is the estimator-cadence echo), and section 3.2's readouts are added. ONE source for the shell and the scorer
(plan60.py's pattern): the battery asks `armspec` for every row's bytes / timeout / hint / env, s51_score.py imports this module.

    plan51.py plan 51 [smoke]       the table of LD_CELL (c2-100 | c8-100, default c2-100), `cell store arm seed rep` per line (smoke = H-B, H-R, B+, N-A one each; the rehearsal runs the smoke)
    plan51.py count 51 [smoke]      the number of rows the table holds (the battery asserts it)
    plan51.py armspec <cell> <arm>  `BYTES TIMEOUT_S HINT ENV...` (ENV is `RWM_DELTA=<v>` on a numeric arm, empty on a named-hint arm)
    plan51.py replace <ledger> <cell> <arm> <seed> <rep>    prints the reason and returns 0 iff the slot's rows so far are exactly one and replaceable (once only)
    plan51.py check 51 <ledger>     rc 0 iff the battery finished and the ledger holds a row for every slot
    plan51.py blocks                the rows per block of the LD_CELL table (the battery's budget guard: a block starts only with rows x 215 s left)

ARM NAMES carry no `-`, `+` or the minus sign (the per-row log names are parsed on `-`): H-B = HB, H-A = HA, H-R = HR, N-A = NA, B- / B+ = BM / BP, A- / A+ = AM / AP, R- / R+ = RM / RP, I1..I4 as named.
Every arm runs --window-reliable (rho = 1), RWM_GEN=0 RWM_DIAG=1, 600 MB, RWM_PERF_TIMEOUT_S=200; the carrier of every NUMERIC arm is Auto (so the sweep is the shipped default machine moved along delta)."""
import math
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

C2, C8 = "c2-100", "c8-100"
CELLS = (C2, C8)
BYTES = 600_000_000
TIMEOUT_S = 200
DELTA_AUTO, DELTA_BULK = 0.5, 0.005
# arm -> (carrier hint, RWM_DELTA or None, role, rows per cell)
ARM_TABLE = {
    "HB": ("bulk", None, "named Bulk (the L1 corner; same-session control)", 6),
    "HA": ("auto", None, "named Auto (the shipped default)", 6),
    "HR": ("realtime", None, "named Realtime (at rho = 1)", 6),
    "NA": ("auto", 0.5, "the same delta as HA, as a number: the A/A and the RWM_DELTA wiring witness", 6),
    "BM": ("auto", 0.0049, "-2 % nudge at Bulk", 6), "BP": ("auto", 0.0051, "+2 % nudge at Bulk", 6),
    "AM": ("auto", 0.49, "-2 % nudge at Auto", 6), "AP": ("auto", 0.51, "+2 % nudge at Auto", 6),
    "RM": ("auto", 49.0, "-2 % nudge at Realtime", 6), "RP": ("auto", 51.0, "+2 % nudge at Realtime", 6),
    "I1": ("auto", 0.0075, "below every predicted crossing delta*", 4), "I2": ("auto", 0.0158, "between the c2 and c8 delta*", 4),
    "I3": ("auto", 0.05, "the r battery's MID point, above every delta*", 4), "I4": ("auto", 5.0, "between Auto and Realtime", 4),
}
ARMS = tuple(ARM_TABLE)
SCORED = ("HB", "HA", "HR", "NA", "BM", "BP", "AM", "AP", "RM", "RP")
INTERIOR = ("I1", "I2", "I3", "I4")
PAIRS = {"B": ("BM", "BP"), "A": ("AM", "AP"), "R": ("RM", "RP")}      # nudge pair per preset: (-2 %, +2 %)
PRESET_DELTA = {"B": DELTA_BULK, "A": DELTA_AUTO, "R": 50.0}
N_BLOCKS = 6
SMOKE_ARMS = ("HB", "HR", "BP", "NA")
REPS_CAP51 = None                  # test_stubrun's fast mode caps the blocks (None = the registered design)
ROW_BUDGET_S = 215                 # a block starts only if rows x 215 s remain before LD_HARD (s51-design section 6 item 2)
BUDGET_S = {"51": 17400, "51r": 1200}      # worst case 76 x 215 s + 16 min = 4 h 50 (section 7); the rehearsal ~20 min. The launch sets LD_HARD = launch + 5 h (a rehearsal: + 1 h)
SYMBOL_PAYLOAD = {"bulk": 1182, "auto": 1182, "realtime": 494}        # chunk payload B per carrier: perf.rs chunk_payload_len = 1196 - 14 (bulk / auto), 508 - 14 (realtime), the header being 14 B
SRC_SIZE_BAND = (0.8, 2.0)         # source symbols x payload / bytes: a wrong carrier symbol size reads 0.42 or 2.4 (see row_problems). Checked on a Bulk row: 507 616 symbols x 1 182 B / 600 MB = 1.000 (s53 c8 BASE)
S08_DUP = 2.2                      # c8 + realtime: the upper bound widens by this (S08's duplicate source may be counted)
BATCH_ECHO = {"bulk": "emission batching ACTIVE", "auto": "emission batching ACTIVE", "realtime": "emission batching out of scope"}
MIN_ROW_S = 30.0                   # a c2 / c8 row with its last tick before 30 s is VOID-SHORT (section 2.1)


def cell_from_env():
    c = os.environ.get("LD_CELL") or C2
    if c not in CELLS:
        raise ValueError(f"LD_CELL={c} (want {C2} or {C8})")
    return c


def arm_hint(arm):
    return ARM_TABLE[arm][0]


def arm_delta(arm):
    return ARM_TABLE[arm][1]


def delta_token(arm):
    d = arm_delta(arm)
    return None if d is None else ("%.6g" % d)


def beta(d):
    """beta(delta) = clamp(log10(delta_Auto / delta) / log10(delta_Auto / delta_Bulk), 0, 1) (the rate-mix weight; math lib.rs:196)."""
    return min(1.0, max(0.0, math.log10(DELTA_AUTO / d) / math.log10(DELTA_AUTO / DELTA_BULK)))


def arm_beta(arm):
    """The arm's beta (H-B = 1, H-A / H-R = 0, a numeric arm by the law)."""
    d = arm_delta(arm)
    return {"HB": 1.0, "HA": 0.0, "HR": 0.0}.get(arm) if d is None else beta(d)


def arm_bytes(cell, arm):
    return BYTES


def arm_timeout(cell, arm):
    return TIMEOUT_S


def arm_env(cell, arm):
    if arm not in ARM_TABLE:
        raise ValueError(f"unknown arm {arm}")
    tok = delta_token(arm)
    return [] if tok is None else ["RWM_DELTA=" + tok]


VALID = {(c, a) for c in CELLS for a in ARMS}


def valid_slot(cell, arm):
    return (cell, arm) in VALID


def all_arms():
    return list(ARMS)


def _cap(n):
    return n if REPS_CAP51 is None else min(n, REPS_CAP51)


def block_units(b):
    """Block b (1-based): the scheduling units in run order. Units are singletons (H-B, H-A, H-R, N-A and, in blocks 1-4, the four interior arms) and the three nudge pairs, each pair back to back with the first
    member alternating by block (the minus side first in odd blocks); the unit list is rotated by b - 1 positions, so pairs and singletons rotate."""
    singles = [(a,) for a in ("HB", "HA", "HR", "NA")] + ([(a,) for a in INTERIOR] if b <= 4 else [])
    pairs = [PAIRS[k] if b % 2 == 1 else PAIRS[k][::-1] for k in ("B", "A", "R")]
    units = singles + pairs
    k = (b - 1) % len(units)
    return units[k:] + units[:k]


def plan(name="51", smoke=False, cell=None):
    """[(cell, store, arm, seed, rep)] in run order for one cell: six blocks (rep = block), seeds 42 / 7 alternating (odd blocks 42). A replaced slot adds a ledger row, not a table row.
    Smoke / rehearsal: H-B, H-R, B+, N-A, one row each (seed 42, rep 1)."""
    if name != "51":
        raise ValueError(name)
    cell = cell or cell_from_env()
    if smoke:
        return [(cell, "def", a, "42", 1) for a in SMOKE_ARMS]
    out = []
    for b in range(1, _cap(N_BLOCKS) + 1):
        sd = "42" if b % 2 == 1 else "7"
        for u in block_units(b):
            out += [(cell, "def", a, sd, b) for a in u]
    return out


def expect_rows(name="51", cell=None):
    return len(plan(name, cell=cell))


def block_rows(cell=None):
    """{block: rows} of the cell's table."""
    out = {}
    for _, _, _, _, rp in plan(cell=cell):
        out[rp] = out.get(rp, 0) + 1
    return out


# ── row witnesses and the replacement rule ──
ECHO_PREFIX = ("gate-delta", "gate-diag", "batch-echo", "chi-", "srcsize-", "pipe-echo", "no-pipe-echo", "header=")      # the VOIDs the battery replaces once; "bin-sha-*" is deliberately NOT here


def _delta_ok(tok, want):
    if want is None:
        return tok == "unset"
    try:
        return abs(float(tok) - want) <= 1e-9 * abs(want)
    except (TypeError, ValueError):
        return False


def row_problems(cell, arm, cli, srv, sha=None, sha_cand=None, nbytes=None, dnf=False):
    """The row's two-sided witnesses (s51-design section 4), before any number is read: [GATES] RWM_DELTA = the arm's value (or `unset`) on BOTH ends; RWM_DIAG=1 on both ends; the emission-batching echo on the
    sender (ACTIVE at bulk / auto, `out of scope` at realtime, and not the other one); source symbols per byte matching the carrier's symbol size; no [CHI] activity (chi n = 0); and the one seat's binary. The
    pipe echo `[window, Rlc, <arm hint>]` and the generation guard are make_row's (ARM_SPEC)."""
    import l1common as lc
    import pool54_score
    if not valid_slot(cell, arm):
        return [f"unknown-arm-{arm}-at-{cell}"]
    hint, want = arm_hint(arm), arm_delta(arm)
    p = []
    for side, lines in (("cli", cli), ("srv", srv)):
        tok = lc.gate_tok(lines, "RWM_DELTA")
        if not _delta_ok(tok, want):
            p.append(f"gate-delta-{side}={tok} (want {'unset' if want is None else delta_token(arm)})")
        d = lc.gate_tok(lines, "RWM_DIAG")
        if d != "1":
            p.append(f"gate-diag-{side}={d} (want 1)")
    on, off = BATCH_ECHO[hint], BATCH_ECHO["realtime" if hint != "realtime" else "auto"]
    if not any(on in ln for ln in cli):
        p.append(f"batch-echo-missing ('{on}' on the sender at {hint})")
    if any(off in ln for ln in cli):
        p.append(f"batch-echo-wrong ('{off}' on the sender at {hint})")
    for ln in cli:
        if ln.startswith("[CHI]"):
            import re
            m = re.search(r"\bn=(\d+)", ln)
            if m and int(m.group(1)) > 0:
                p.append("chi-active (the completion-exposure estimator ran: " + ln[:60] + ")")
                break
    srcs = [m for m in (_CUM.search(ln) for ln in cli if ln.startswith("[DIAG]")) if m]
    if srcs and nbytes:
        ratio = int(srcs[-1].group(1)) * SYMBOL_PAYLOAD[hint] / nbytes
        lo, hi = SRC_SIZE_BAND
        if cell == C8 and hint == "realtime":
            hi *= S08_DUP      # S08's duplicate source (on the second path, Realtime) may be counted in cum= (unverified): the band cannot then tell a wrong carrier from a doubled count
        if ratio > hi or (ratio < lo and not dnf):      # a DNF is a datum, kept and ranked worst: it may have sent under 80 % of the object, so only the upper bound applies to it
            p.append(f"srcsize-ratio={ratio:.2f} (source symbols x {SYMBOL_PAYLOAD[hint]} B / bytes, want {lo}-{hi:g} at {hint}{' (a DNF: upper bound only)' if dnf else ''})")
    return p + pool54_score.sha_problems("CAND", sha, sha_cand, None)


import re as _re  # noqa: E402
_CUM = _re.compile(r" cum=(\d+)/(\d+)/(\d+)")


def is_echo_void(row):
    return any(x.startswith(ECHO_PREFIX) for x in row.get("problems") or [])


def replace_reason(ledger, cell, arm, seed, rep):
    """-> the reason the slot is re-run once, or None: its rows so far are exactly one and that row is VOID by an echo witness ('echo'), or VOID-SHORT (a completed row with its last tick before 30 s: 'short', re-run
    once, the guard's rule). A DNF is a datum (kept, ranked worst), a crash is not replaced, and a WRONG BINARY (bin-sha-*) never is."""
    import place38_parse as pp
    rows, _ = pp.rows_of([ledger])
    mine = [r for r in rows if (r.get("cell"), r.get("arm"), str(r.get("seed")), str(r.get("rep"))) == (cell, arm, str(seed), str(rep))]
    if len(mine) != 1:
        return None
    r = mine[0]
    if is_echo_void(r):
        return "echo"
    if r.get("status") == "VOID-SHORT" and not r.get("dnf"):
        return "short"
    return None


def check(name, path):
    """rc 0 iff the battery finished and the ledger holds a row for every slot of the table (a replaced slot has two rows)."""
    import place38_parse as pp
    rows, _ = pp.rows_of([path])
    t = open(path, errors="replace").read()
    slots = {(r["cell"], r["arm"], str(r["seed"]), str(r["rep"])) for r in rows}
    want = {(c, a, sd, str(rp)) for c, _, a, sd, rp in plan(name)}
    ok = "LD-BATTERY-DONE" in t and slots == want
    print(f"CHECK{name.upper()} done={'LD-BATTERY-DONE' in t} slots={len(slots & want)}/{len(want)} rows={len(rows)} extra={len(slots - want)}")
    return 0 if ok else 1


def main(argv):
    if not argv:
        print(__doc__)
        return 2
    cmd, a = argv[0], argv[1:]
    if cmd in ("plan", "count"):
        rows = plan(a[0], smoke=len(a) > 1 and a[1] == "smoke")
        if cmd == "count":
            print(len(rows))
        else:
            for c, s, ar, sd, rp in rows:
                print(f"{c} {s} {ar} {sd} {rp}")
        return 0
    if cmd == "blocks":
        for b, n in sorted(block_rows().items()):
            print(b, n)
        return 0
    if cmd == "armspec":
        cell, arm = a[0], a[1]
        if not valid_slot(cell, arm):
            print(f"UNKNOWN-ARM {arm} at {cell}")
            return 1
        print(arm_bytes(cell, arm), arm_timeout(cell, arm), arm_hint(arm), *arm_env(cell, arm))
        return 0
    if cmd == "replace":
        why = replace_reason(*a[:5])
        if why:
            print(why)
            return 0
        return 1
    if cmd == "check":
        return check(a[0], a[1])
    print(f"unknown command {cmd}")
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
