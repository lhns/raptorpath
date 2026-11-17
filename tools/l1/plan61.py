#!/usr/bin/env python3
"""Status 61: the c3 sweep (is S_F the knee's term at a second leg shape?), plans 61r (rehearsal) / 61a (c3) / 61b (c2 add-on). Tables, per-arm run spec, row witnesses, the replacement rule and the ledger check.
docs/derivations/s61-registration.md section 1 (bytes and timeouts), 2 (arms, the pilot), 6 (the rehearsal), 8 (budget), 9 (this module's brief). ONE source for the shell and the scorer: the battery asks `armspec`
for every row's bytes / timeout / hint / env (the scorer s61_score.py imports this module). Built on plan60.py's pattern; plan 60's tables are untouched.

    plan61.py plan <61r|61a|61b> [smoke]     the table, `cell store arm seed rep` per line (61r = the 6 rehearsal rows; 61a needs LD_KNEE61, else exit 3 = ABORT-PILOT)
    plan61.py count <61r|61a|61b> [smoke]    the number of rows the table holds (the battery asserts it)
    plan61.py armspec <cell> <arm>           `BYTES TIMEOUT_S HINT ENV...` (ENV tokens are KEY=VALUE, RWM_SLAW first)
    plan61.py replace <ledger> <cell> <arm> <seed> <rep>    prints the reason and returns 0 iff the slot's rows so far are exactly one and replaceable (once only)
    plan61.py check <61a|61b> <ledger>       rc 0 iff the battery finished and the ledger holds a row for every slot
    plan61.py pilot <u88> <u128>             the pilot's knee W-hat (section 2.2) and the arms it places

Environment: LD_KNEE61 = the pilot's W-hat (verbatim from the rehearsal's `PILOT61 W=`); LD_CTL61 = 512 selects the W 512 control, which §61 Amendment 2 makes the 61a control outright (run_all refuses a 61a launch without it: ABORT-CONTROL61); unset, the global default stays W 2 048 (61b's control).
Arms: `G<W>` = the gauge on (RWM_SLAW=1), W = the realised store window (one live path: RWM_STORE_PATH_POOL = W; nothing is set at the shipped 2048). Every row is Bulk."""
import math
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

C3, C2 = "c3-25", "c2-100"
CELLS = (C3, C2)
NLIVE = {C3: 1, C2: 1}                                                       # single leg at both cells (section 9 item 1)
FLOOR_W = {C3: (12, 16, 24), C2: ()}                                         # section 2.1: the c3 floor pools; 61b carries no floor arm
CTL_W, CTL_FALLBACK_W = 2048, 512                                            # section 2.2 / 6: the control and its registered fallback
PILOT_W = (88, 128)                                                          # section 2.2: the pilot probes
FRAC_K, FRAC_B, FRAC_P1 = (0.95, 1.05, 1.14), (0.57, 0.76, 0.86), 1.5        # section 2.2: the arm fractions of the pilot knee
W_HAT_MIN, W_HAT_MAX = 64, 192                                               # section 2.2: the clamp
C2_ARMS = (96, 128, 144, 160, 2048)                                          # section 2.4
N_REP = 6
REPS_CAP61 = None                                                            # test_stubrun's fast mode caps the replicates (None = the registered design)
SHIPPED_POOL = 2048
LATE_T = {C3: 15.0, C2: 15.0}                                                # section 1: LATE windows t >= 15 s
# bytes (section 1): bytes(W) = 53 s * min(0.19 W, 17.5) Mbit/s / 8, rounded to the nearest 5 MB, >= 15 MB; 120 MB at and above the knee bracket and at the control. c2 (61b): 600 MB as section 60.
BYTES_S, BYTES_RATE, BYTES_CAP_MBIT, BYTES_STEP_MB, BYTES_MIN_MB, BYTES_KNEE_MB = 53.0, 0.19, 17.5, 5, 15, 120
BYTES_C2 = 600_000_000
TIMEOUT_C3, TIMEOUT_C2 = 120, 150
FLOOR_RERUN_S = 160                                                          # section 8: a c3 floor-pool DNF is re-run once at 120 s + bring-up
BUDGET_S = {"61a": 14220, "61b": 6000}                                       # section 8: 3.95 h / 1.67 h worst case; LD_HARD is launch + 4.0 h / + 1.8 h
SMOKE_K = 1                                                                  # 61a's smoke is the middle knee arm K2 (index 1)
C2_SMOKE_ARM = "G160"                                                        # 61b's smoke: one c2 G 160 row
REHEARSAL = [(C3, "def", "G2048", "42", 1),                                  # R1: the smoke and the pilot's control
             (C3, "def", "G12", "42", 1), (C3, "def", "G12", "7", 2),        # R2, R3: the floor go/no-go
             (C3, "def", "G88", "42", 1), (C3, "def", "G128", "42", 1),      # R4, R5: the pilot
             (C3, "def", "G512", "42", 1)]                                   # R6: the fallback control
UNSET_KNEE = None


def r_to(x, m):
    """x rounded to the nearest multiple of m (half up: Python's round() is banker's)."""
    return int(math.floor(x / m + 0.5)) * m


def arm_parse(arm):
    """`G192` -> ('G', 192); anything else -> (None, None)."""
    m = re.fullmatch(r"(G)(\d+)", arm or "")
    return (m.group(1), int(m.group(2))) if m else (None, None)


def gauge_on(arm):
    return arm_parse(arm)[0] == "G"


def pool_per_path(cell, w):
    return w // NLIVE[cell]


def clamp_knee(w):
    return min(max(float(w), float(W_HAT_MIN)), float(W_HAT_MAX))


def pilot_knee(u88, u128):
    """The pilot knee W-hat as a float: a thin wrapper of pilot_detail (one rule, no second copy). Raises ValueError for a utilisation <= 0."""
    d = pilot_detail(u88, u128)
    if d is None:
        raise ValueError(f"pilot utilisation <= 0 (u88={u88}, u128={u128})")
    return d[0]


def pilot_detail(u88, u128):
    """Section 2.2's pilot rule, as amended by status 61 Amendment 2: -> (W-hat rounded to 0.1 after the clamp to [64, 192], case, clamped: bool) or None when a utilisation is <= 0 (no division).
    Cases: proportional (u88 < 0.95 <= u128: W-hat = min(88 * 0.95 / u88, 128); the cap is the straddle's own bound, reached only for u88 < 0.65), secant (both below 0.95, u128 > u88: 128 + (0.95 - u128) * 40 / (u128 - u88)),
    degenerate (both below, u128 <= u88: 128 * 0.95 / u128), both-above (88 * 0.95 / u88), non-monotone (u88 >= 0.95 > u128: 88 * 0.95 / u88). W-hat is registered at 0.1-slot resolution: the arms derive from the PRINTED value
    (it is what LD_KNEE61 receives verbatim)."""
    if not (u88 > 0 and u128 > 0):
        return None
    lo, hi = PILOT_W
    if u88 < 0.95 <= u128:
        case, raw = "proportional", min(lo * 0.95 / u88, float(hi))
    elif u88 < 0.95 and u128 < 0.95:
        case, raw = ("secant", hi + (0.95 - u128) * (hi - lo) / (u128 - u88)) if u128 > u88 else ("degenerate", hi * 0.95 / u128)
    elif u128 >= 0.95:
        case, raw = "both-above", lo * 0.95 / u88
    else:
        case, raw = "non-monotone", lo * 0.95 / u88
    return round(clamp_knee(raw), 1), case, not (W_HAT_MIN <= raw <= W_HAT_MAX)


def arms(w_hat):
    """Section 2.2's arms from the pilot knee: {'K': 3 knee-bracket, 'B': 3 below-knee, 'P1': r8(1.5 W-hat)} (r4 = a multiple of 4)."""
    w = clamp_knee(w_hat)
    return {"K": [r_to(f * w, 4) for f in FRAC_K], "B": [r_to(f * w, 4) for f in FRAC_B], "P1": r_to(FRAC_P1 * w, 8)}


def knee_env():
    """LD_KNEE61 as a float in [64, 192], or None when unset / empty. A set but unparsable or out-of-range value is an error (ValueError): the launch must not run on a guess."""
    v = os.environ.get("LD_KNEE61", "").strip()
    if not v:
        return UNSET_KNEE
    w = float(v)
    if not (W_HAT_MIN <= w <= W_HAT_MAX):
        raise ValueError(f"LD_KNEE61={v} is outside the pilot clamp [{W_HAT_MIN}, {W_HAT_MAX}]")
    return w


def control_w(ctl="env"):
    v = os.environ.get("LD_CTL61", "") if ctl == "env" else (ctl or "")
    return CTL_FALLBACK_W if str(v).strip() == str(CTL_FALLBACK_W) else CTL_W


def _all_c3_w():
    """Every W an arm of a c3 plan can carry for any pilot knee (the scorer's ARM_SPEC registration and valid_slot)."""
    ws = set(FLOOR_W[C3]) | {CTL_W, CTL_FALLBACK_W} | set(PILOT_W)
    for i in range(W_HAT_MIN * 4, W_HAT_MAX * 4 + 1):
        a = arms(i / 4.0)
        ws |= set(a["K"]) | set(a["B"]) | {a["P1"]}
    return ws


C3_W = _all_c3_w()
C2_W = set(C2_ARMS)


def valid_slot(cell, arm):
    kind, w = arm_parse(arm)
    return kind == "G" and w in (C3_W if cell == C3 else C2_W if cell == C2 else ())


def all_arms():
    """Every arm name any plan can carry (for place38_parse's ARM_SPEC registration)."""
    return sorted({"G%d" % w for w in C3_W | C2_W})


def is_floor(cell, arm):
    return arm_parse(arm)[1] in FLOOR_W.get(cell, ())


def formula_mb(w):
    """Section 1's bytes formula in MB (nearest 5, >= 15)."""
    return max(BYTES_MIN_MB, r_to(BYTES_S * min(BYTES_RATE * w, BYTES_CAP_MBIT) / 8.0, BYTES_STEP_MB))


def _knee(knee):
    return knee_env() if knee == "env" else knee


def arm_bytes(cell, arm, knee="env"):
    """c2: 600 MB. c3: the formula, except 120 MB at the control and at and above the knee bracket (K1 from LD_KNEE61). With no pilot knee yet (the rehearsal) no row is 'at the knee bracket': the pilot rows
    take the formula (DECISION 2 of s61_score; the registration prints no threshold for them)."""
    w = arm_parse(arm)[1]
    if cell == C2:
        return BYTES_C2
    if w in (CTL_W, CTL_FALLBACK_W):
        return BYTES_KNEE_MB * 1_000_000
    k = _knee(knee)
    if k is not None and w >= arms(k)["K"][0]:
        return BYTES_KNEE_MB * 1_000_000
    return formula_mb(w) * 1_000_000


def arm_timeout(cell, arm):
    return TIMEOUT_C2 if cell == C2 else TIMEOUT_C3


def arm_hint(arm):
    return "bulk"


def arm_env(cell, arm):
    """[KEY=VALUE]: RWM_SLAW=1 first (the echo witnessed on both ends), RWM_RTOBS=1 (section 60's instrument env, section 2.3), then the pool knob (not at the shipped 2048)."""
    kind, w = arm_parse(arm)
    if kind is None:
        raise ValueError(f"unknown arm {arm}")
    env = ["RWM_SLAW=1", "RWM_RTOBS=1"]
    pool = pool_per_path(cell, w)
    if pool != SHIPPED_POOL:
        env.append("RWM_STORE_PATH_POOL=%d" % pool)
    return env


def _cap(n):
    return n if REPS_CAP61 is None else min(n, REPS_CAP61)


def _grid_rows(cell, ws):
    """A cyclic Latin square across replicates, seeds 42 / 7 alternating (plan60._grid_rows)."""
    out = []
    for k in range(_cap(N_REP)):
        j = k % len(ws)
        a = ["G%d" % w for w in ws]
        out += [(cell, "def", x, "42" if k % 2 == 0 else "7", k + 1) for x in a[j:] + a[:j]]
    return out


def c3_grid(w_hat, ctl="env"):
    """The 11 c3 arms in table order: floor 12 / 16 / 24, B x3, K x3, P1, the control."""
    a = arms(w_hat)
    ws = list(FLOOR_W[C3]) + a["B"] + a["K"] + [a["P1"], control_w(ctl)]
    if len(set(ws)) != len(ws):
        raise ValueError(f"arm collision at W-hat {w_hat}: {ws}")
    return ws


def plan(name, smoke=False, rehearsal=False, knee="env", ctl="env"):
    """[(cell, store, arm, seed, rep)] in run order. 61r: the 6 rehearsal rows. 61a: 11 arms x 6 replicates at c3 (needs LD_KNEE61: ValueError otherwise). 61b: 5 arms x 6 at c2. The smokes are fixed lists
    (61a's is the middle knee arm). A replaced slot adds a ledger row, not a table row."""
    if name == "61r" or (rehearsal and name == "61a"):
        return list(REHEARSAL)
    if name == "61a":
        k = _knee(knee)
        if k is None:
            raise ValueError("LD_KNEE61 is unset: 61a needs the rehearsal's PILOT61 W-hat (ABORT-PILOT)")
        ws = c3_grid(k, ctl)
        if smoke:
            return [(C3, "def", "G%d" % arms(k)["K"][SMOKE_K], "42", 1)]
        return _grid_rows(C3, ws)
    if name == "61b":
        return [(C2, "def", C2_SMOKE_ARM, "42", 1)] if smoke else _grid_rows(C2, C2_ARMS)
    raise ValueError(name)


def expect_rows(name):
    return len(plan(name))


# ── row witnesses and the replacement rule ──
ECHO_PREFIX = ("gate-slaw", "gate-diag", "gate-three-term", "gate-pool", "pool_per_path")      # the VOIDs the battery replaces once; "bin-sha-*" is deliberately NOT here


def row_problems(cell, arm, cli, srv, sha=None, sha_cand=None):
    """Plan 60's echo witnesses at the s61 cells: RWM_SLAW=1 and RWM_DIAG=1 on BOTH ends' [GATES] (a mismatch voids the row, re-run once), RWM_THREE_TERM off, the pool knob echoed as requested, and the one
    seat's binary (LD_SHA). A [SLAW] q other than 0.100000 is checked by the smoke (every row is Bulk)."""
    import l1common as lc
    import place38_parse as pp
    import pool54_score
    if not valid_slot(cell, arm):
        return [f"unknown-arm-{arm}-at-{cell}"]
    kind, w = arm_parse(arm)
    p = []
    for side, lines in (("cli", cli), ("srv", srv)):
        g = lc.gate_tok(lines, "RWM_SLAW")
        if g != "1":
            p.append(f"gate-slaw-{side}={g} (want 1)")
        d = lc.gate_tok(lines, "RWM_DIAG")
        if d != "1":
            p.append(f"gate-diag-{side}={d} (want 1)")
        t = lc.gate_tok(lines, "RWM_THREE_TERM")
        if t not in (None, "0"):
            p.append(f"gate-three-term-{side}={t} (want 0)")
    pool = pool_per_path(cell, w)
    for side, lines in (("cli", cli), ("srv", srv)):
        tok, info = pp.pool_echo56(lines)
        if tok != str(pool):
            p.append(f"gate-pool-{side}={tok} (want {pool})")
        if side == "cli" and info != pool:
            p.append(f"pool_per_path-{side}={info} (want {pool})")
        if side == "srv" and info is not None and info != pool:
            p.append(f"pool_per_path-{side}={info} (want {pool})")
    return p + pool54_score.sha_problems("CAND", sha, sha_cand, None)


def is_echo_void(row):
    return any(x.startswith(ECHO_PREFIX) for x in row.get("problems") or [])


def replace_reason(ledger, cell, arm, seed, rep):
    """-> the reason the slot is re-run once, or None: its rows so far are exactly one and that row is VOID by an echo witness (RWM_SLAW, RWM_DIAG, the pool echo), or is a c3 floor-pool DNF (section 8: floor-pool
    DNFs are re-run once; a DNF stays in the ledger as a datum). A WRONG BINARY (bin-sha-*) is never re-run: the harness rules abort on it."""
    import place38_parse as pp
    rows, _ = pp.rows_of([ledger])
    mine = [r for r in rows if (r.get("cell"), r.get("arm"), str(r.get("seed")), str(r.get("rep"))) == (cell, arm, str(seed), str(rep))]
    if len(mine) != 1:
        return None
    r = mine[0]
    if is_echo_void(r):
        return "slaw-echo" if any(x.startswith("gate-slaw") for x in r["problems"]) else "pool-echo"
    if is_floor(cell, arm) and r.get("dnf") and r.get("status") != "VOID-ONDISK":
        return "floor-dnf"
    if os.environ.get("LD_REHEARSE") == "1" and cell == C3 and arm_parse(arm)[1] in PILOT_W and r.get("dnf") and r.get("status") != "VOID-ONDISK":
        return "pilot-dnf"      # section 6: a pilot row that DNFs is re-run once (the rehearsal only: in 61a a W 88 / 128 arm is an ordinary arm)
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
    try:
        if cmd == "plan":
            for c, s, ar, sd, rp in plan(a[0], smoke=len(a) > 1 and a[1] == "smoke"):
                print(f"{c} {s} {ar} {sd} {rp}")
            return 0
        if cmd == "count":
            print(len(plan(a[0], smoke=len(a) > 1 and a[1] == "smoke")))
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
        if cmd == "pilot":
            u88, u128 = float(a[0]), float(a[1])
            d = pilot_detail(u88, u128)
            if d is None:
                print("PILOT61 none: a pilot utilisation is <= 0")
                return 1
            print(f"PILOT61 W={d[0]:.1f} u88={u88:.3f} u128={u128:.3f} arms={arms(d[0])} case={d[1]} clamped={'yes' if d[2] else 'no'}")
            return 0
    except ValueError as e:
        print(f"ABORT-PILOT {e}")
        return 3
    print(f"unknown command {cmd}")
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
