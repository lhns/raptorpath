#!/usr/bin/env python3
"""Status 60: the store law's gauge session (B-1), plans 60a / 60b. Tables, per-arm run spec, row witnesses, the replacement rule and the ledger check.
docs/derivations/s60-registration.md section 7 (rows and launches), section 5 (witnesses), section 9 (harness requirements). ONE source for the shell and the scorer:
the battery asks `armspec` for every row's bytes / timeout / hint / env, so there is no second copy to drift (the scorer s60_score.py imports this module).

    plan60.py plan <60a|60b|60r> [smoke]     the table, `cell store arm seed rep` per line (60r = the 10 rehearsal rows: both smokes + 6 floor-pool rows)
    plan60.py count <60a|60b|60r> [smoke]    the number of rows the table holds (the battery asserts it)
    plan60.py armspec <cell> <arm>           `BYTES TIMEOUT_S HINT ENV...` (ENV tokens are KEY=VALUE, RWM_SLAW first)
    plan60.py replace <ledger> <cell> <arm> <seed> <rep>    prints the reason and returns 0 iff the slot's rows so far are exactly one and replaceable (once only)
    plan60.py check <60a|60b> <ledger>       rc 0 iff the battery finished and the ledger holds a row for every slot

Arms: `G<W>` = the gauge on (RWM_SLAW=1), `C<W>` = the gauge off (RWM_SLAW=0), W = the realised store window (c8: 2 live paths, so RWM_STORE_PATH_POOL = W / 2; nothing is set at
the shipped 2048); `EVICT` = plan 59's evicting seat (Realtime hint, RWM_C_RELIABLE=0) with the gauge on (code kept; NOT scheduled in 60a / 60b since status §60 Amendment 2). Every arm runs RWM_DIAG=1 (the battery's base env)."""
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

C2, C8, C1S = "c2-100", "c8-100", "c1s-400"
CELLS = (C2, C8, C1S)
NLIVE = {C2: 1, C8: 2, C1S: 1}
FLOOR_W = {C2: (16, 32, 48), C8: (32, 64, 96), C1S: (64, 128, 192)}                      # s60 section 2: the floor pools (W)
GRID = {"60a": {C2: (16, 32, 48, 160, 176, 192, 256, 2048), C8: (32, 64, 96, 256, 320, 384, 512, 1280)},      # section 7's launch table
        "60b": {C1S: (64, 128, 192)}}
CONTROL_W = {C2: 2048, C8: 1280, C1S: 2048}
KNEE_W = {C2: (160, 176, 192), C8: (256, 320, 384)}                                     # the knee bracket arms (sections 3.1 / 3.2)
P1_W = {C2: 256, C8: 512, C1S: 2048}                                                     # section 1.2: the P1 / P3 arms
SHIPPED_POOL = 2048
N_REP = 6                                                                                # replicates per grid arm (section 7)
N_PAIRS, N_EXTRA_G = 10, 2                                                               # c1s control: 10 interleaved G/C pairs + 2 G rows (12 G, 10 C)
EVICT_ARM, EVICT_CELL, EVICT_SEEDS = "EVICT", C2, ("42", "7", "42", "7", "42")
REPS_CAP60 = None                                                                        # test_stubrun's fast mode caps the replicates (None = the registered design)
# bytes (section 2 "Pools and bytes"): the floor pools carry their own, everything else 600 MB (c2, c8) or 5 GB (c1s control).
BYTES_FLOOR = {(C2, 16): 100_000_000, (C2, 32): 200_000_000, (C2, 48): 300_000_000,
               (C8, 32): 200_000_000, (C8, 64): 400_000_000, (C8, 96): 600_000_000,
               (C1S, 64): 1_000_000_000, (C1S, 128): 1_500_000_000, (C1S, 192): 2_000_000_000}
BYTES_DEFAULT = {C2: 600_000_000, C8: 600_000_000, C1S: 5_000_000_000}
# RWM_PERF_TIMEOUT_S: 150 s on every floor-pool row (section 2), and on the c2 / c8 / evicting rows (their typical 75-90 s, section 7); 210 s on the c1s 5 GB control (section 7: "210 s worst, 57a's convention").
TIMEOUT_FLOOR, TIMEOUT_DEFAULT, TIMEOUT_C1S_CONTROL = 150, 150, 210
LATE_T = {C2: 15.0, C8: 15.0, C1S: 5.0}                                                  # section 1.1: LATE windows (t >= 15 s at c2 / c8, >= 5 s at c1s)
# The budget guard (section 7: 3.2 h / 2.9 h left at launch) PLUS the worst-case floor-pool DNF re-runs (section 2 allows each floor row one re-run): every floor row DNFs at its 150 s timeout and is re-run once at
# 150 s + ~40 s bring-up = 190 s. 60a: 36 floor rows (c2 + c8, 18 each) -> +6 840 s; 60b: 18 c1s floor rows -> +3 420 s. NEW WORST CASES: 60a 11 520 + 6 840 = 18 360 s (5.1 h); 60b 10 440 + 3 420 = 13 860 s (3.85 h).
# The launch must therefore set LD_HARD at least that far ahead (launch + 5.2 h / + 3.9 h), not the registered launch + 4 h.
FLOOR_RERUN_S = 190
BUDGET_S = {"60a": 11520 + 36 * FLOOR_RERUN_S, "60b": 10440 + 18 * FLOOR_RERUN_S}
SMOKE = {"60a": [(C2, "def", "C192", "42", 1), (C2, "def", "G192", "42", 1)],            # section 5: c2 W 192, one C row and one G row
         "60b": [(C1S, "def", "G64", "42", 1), (C1S, "def", "C2048", "42", 1)]}          # c1s W 64 G (1 GB) and W 2048 C
EVICT_REHEARSAL = (EVICT_CELL, "def", EVICT_ARM, "42", 1)                                # NOT scheduled since status §60 Amendment 2 (60b / 60br carry no evicting row); kept for the future evicting registration and the scorer's tests
REHEARSAL = SMOKE["60a"] + SMOKE["60b"] + [(C2, "def", "G16", "42", 2),                  # section 7: the 10 rehearsal rows (all G beyond the smokes)
                                            (C8, "def", "G32", "42", 1), (C8, "def", "G32", "7", 2), (C8, "def", "G96", "42", 1),
                                            (C1S, "def", "G64", "7", 2), (C1S, "def", "G64", "42", 3)]
PLAN_CELLS = {"60a": (C2, C8), "60b": (C1S,)}


def arm_parse(arm):
    """`G192` -> ('G', 192); `C2048` -> ('C', 2048); `EVICT` -> ('E', None); anything else -> (None, None)."""
    if arm == EVICT_ARM:
        return "E", None
    m = re.fullmatch(r"([GC])(\d+)", arm or "")
    return (m.group(1), int(m.group(2))) if m else (None, None)


def gauge_on(arm):
    return arm_parse(arm)[0] in ("G", "E")


def pool_per_path(cell, w):
    """RWM_STORE_PATH_POOL for a window W (None for the evicting seat / the shipped pool): c8 has 2 live paths (the realised cap is max(N * pool, floor), so W = 2 * pool)."""
    return w // NLIVE[cell] if w is not None else None


def is_floor(cell, arm):
    kind, w = arm_parse(arm)
    return kind in ("G", "C") and w in FLOOR_W.get(cell, ())


def arm_bytes(cell, arm):
    kind, w = arm_parse(arm)
    if kind == "E":
        return BYTES_DEFAULT[EVICT_CELL]
    return BYTES_FLOOR.get((cell, w), BYTES_DEFAULT[cell])


def arm_timeout(cell, arm):
    kind, w = arm_parse(arm)
    if kind == "E" or is_floor(cell, arm):
        return TIMEOUT_FLOOR
    return TIMEOUT_C1S_CONTROL if cell == C1S else TIMEOUT_DEFAULT


def arm_hint(arm):
    return "realtime" if arm == EVICT_ARM else "bulk"


def arm_env(cell, arm):
    """[KEY=VALUE]: RWM_SLAW first (the echo witnessed on both ends), then the pool knob (not at the shipped 2048), then the evicting seat's switches (plan 59's RWM_C_RELIABLE=0 and RWM_RDIAG=1)."""
    kind, w = arm_parse(arm)
    if kind is None:
        raise ValueError(f"unknown arm {arm}")
    env = ["RWM_SLAW=%d" % (1 if kind in ("G", "E") else 0)]
    if kind == "E":
        return env + ["RWM_C_RELIABLE=0"]      # plan 59's arm; RWM_RDIAG=1 comes from the battery's instrument env (every row), RWM_RTOBS is NOT set (plan 59 did not)
    env.append("RWM_RTOBS=1")      # §56's instrument env (RWM_RTOBS=1 RWM_RDIAG=1) on every G / C row, so the knee and the collapse thresholds derived on §56 apply; the registration is silent; G and C both carry it
    pool = pool_per_path(cell, w)
    if pool != SHIPPED_POOL:
        env.append("RWM_STORE_PATH_POOL=%d" % pool)
    return env


VALID = ({(c, "G%d" % w) for g in GRID.values() for c, ws in g.items() for w in ws}
         | {(C1S, "G2048"), (C1S, "C2048"), (C2, "G192"), (C2, "C192"), (EVICT_CELL, EVICT_ARM)})      # every (cell, arm) a plan, a smoke or the rehearsal can carry; EVICT stays valid (harmless: kept for the future evicting registration, and a ledger row carrying it is still recognised)


def all_arms():
    """Every arm name any plan can carry (for place38_parse's ARM_SPEC registration)."""
    return sorted({a for _, a in VALID})


def valid_slot(cell, arm):
    return (cell, arm) in VALID


def _cap(n):
    return n if REPS_CAP60 is None else min(n, REPS_CAP60)


def _grid_rows(cell, ws):
    out = []
    for k in range(_cap(N_REP)):
        j = k % len(ws)
        arms = ["G%d" % w for w in ws]
        out += [(cell, "def", a, "42" if k % 2 == 0 else "7", k + 1) for a in arms[j:] + arms[:j]]      # a cyclic Latin square across replicates, seed 42 / 7 alternating
    return out


def plan(name, smoke=False, rehearsal=False):
    """[(cell, store, arm, seed, rep)] in run order. 60a: c2 block then c8 block (8 arms x 6, all G). 60b: the c1s floor block (3 pools x 6), the control (10 interleaved G/C pairs,
    the order alternating, then 2 G rows), and nothing else: the evicting rows left 60b (status §60 Amendment 2; EVICT_SEEDS / EVICT_REHEARSAL are kept for their own registration). The smoke and the rehearsal are fixed lists. A replaced slot adds a ledger row, not a table row."""
    if name == "60br":
        return list(SMOKE["60b"])      # no evicting row since status §60 Amendment 2
    if rehearsal:
        return list(REHEARSAL)
    if smoke:
        return list(SMOKE[name])
    out = []
    nogo = {x for x in os.environ.get("LD_NOGO", "").split(",") if x}      # the rehearsal's NO-GO decisions (section 5): c1s drops 60b's floor block ("60b then runs only the G/C pairs")
    if name == "60a":
        for cell in (C2, C8):
            out += _grid_rows(cell, GRID["60a"][cell])
    elif name == "60b":
        if "c1s" not in nogo:
            out += _grid_rows(C1S, GRID["60b"][C1S])
        npair = N_PAIRS if REPS_CAP60 is None else min(N_PAIRS, REPS_CAP60)
        for j in range(1, npair + 1):
            sd = "42" if j % 2 == 1 else "7"
            pair = [("G2048", "G"), ("C2048", "C")] if j % 2 == 1 else [("C2048", "C"), ("G2048", "G")]      # interleaved: the order alternates
            out += [(C1S, "def", a, sd, j) for a, _ in pair]
        if REPS_CAP60 is None:
            out += [(C1S, "def", "G2048", "42" if j % 2 == 1 else "7", j) for j in range(N_PAIRS + 1, N_PAIRS + N_EXTRA_G + 1)]
    else:
        raise ValueError(name)
    return out


def expect_rows(name):
    return len(plan(name))


# ── row witnesses and the replacement rule ──
ECHO_PREFIX = ("gate-slaw", "gate-diag", "gate-three-term", "gate-pool", "pool_per_path", "evict-", "gate-unified", "no-realtime-echo")      # the VOIDs the battery replaces once; "bin-sha-*" is deliberately NOT here


def row_problems(cell, arm, cli, srv, sha=None, sha_cand=None):
    """The row's witnesses: RWM_SLAW as requested and RWM_DIAG=1 on BOTH ends' [GATES] (section 5: a mismatch voids the row, which is re-run once), RWM_THREE_TERM off, the pool knob echoed
    as requested (a G / C row), the evicting seat armed (EVICT: plan 59's witnesses), and the one seat's binary (LD_SHA)."""
    import l1common as lc
    import place38_parse as pp
    import pool54_score
    if not valid_slot(cell, arm):
        return [f"unknown-arm-{arm}-at-{cell}"]
    kind, w = arm_parse(arm)
    want = "1" if gauge_on(arm) else "0"
    p = []
    for side, lines in (("cli", cli), ("srv", srv)):
        g = lc.gate_tok(lines, "RWM_SLAW")
        if g != want:
            p.append(f"gate-slaw-{side}={g} (want {want})")
        d = lc.gate_tok(lines, "RWM_DIAG")
        if d != "1":
            p.append(f"gate-diag-{side}={d} (want 1)")
        t = lc.gate_tok(lines, "RWM_THREE_TERM")
        if t not in (None, "0"):
            p.append(f"gate-three-term-{side}={t} (want 0)")
    if kind == "E":
        return p + pp.evict_problems59(cli, srv, sha, sha_cand)
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
    """-> the reason the slot is re-run once, or None: its rows so far are exactly one and that row is VOID by an echo / liveness witness (RWM_SLAW, RWM_DIAG, the pool echo, the evicting seat's
    liveness incl. the Realtime echo), or is a floor-pool DNF (s60 section 2: 'A DNF is a datum, and the row is re-run once'). A WRONG BINARY (bin-sha-*) is never re-run: it is not an echo VOID, the
    harness rules abort on it. The DNF row stays in the ledger as a datum (the scorer reports it beside the replacement); the budget guard carries the worst-case floor re-runs (FLOOR_RERUN_S)."""
    import place38_parse as pp
    rows, _ = pp.rows_of([ledger])
    mine = [r for r in rows if (r.get("cell"), r.get("arm"), str(r.get("seed")), str(r.get("rep"))) == (cell, arm, str(seed), str(rep))]
    if len(mine) != 1:
        return None
    r = mine[0]
    if is_echo_void(r):
        return "slaw-echo" if any(x.startswith("gate-slaw") for x in r["problems"]) else ("evict-liveness" if arm == EVICT_ARM else "pool-echo")
    if is_floor(cell, arm) and r.get("dnf") and r.get("status") != "VOID-ONDISK":
        return "floor-dnf"
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
    if cmd == "plan":
        name = a[0]
        for c, s, ar, sd, rp in (plan("60a", rehearsal=True) if name == "60r" else plan(name, smoke=len(a) > 1 and a[1] == "smoke")):
            print(f"{c} {s} {ar} {sd} {rp}")
        return 0
    if cmd == "count":
        print(len(plan(a[0], smoke=len(a) > 1 and a[1] == "smoke")) if a[0] != "60r" else len(plan("60a", rehearsal=True)))
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
