#!/usr/bin/env python3
"""Plan 67: VM batch 1 (docs/derivations/vm-batch-1-registration.md, section 9). The seat table, the stage tables, the row specs, the pins and the budgets; the row parser, witnesses,
clauses and readouts are s67_score.py; the shell side (loopdiag_run_all.sh -> loopdiag_p67.sh, loopdiag_battery.sh, tun67_battery.sh) reads this file through the CLI below.

Sessions (LD_PLAN):   67pre session 0: the pre-flight with no battery (cargo check, the gates reference, the suites and the repeat runs on S1 and S2, the visualizer, binid; loopdiag_p67pre.sh; refuses LD_REHEARSE=1)
                      67  the rehearsal (needs LD_REHEARSE=1: builds, tools tests, one smoke per stage; no scored row)
                      67g session 2: G1 then G2 (plan 41d twice, the BIN / BASE seats rebound)
                      67x session 3: A (with A1), R1 (with A1-R), T, R2 (with the twins), the dump twins (LD_DUMP67=1 only), the pool block
Battery plans:        41d (stage G, LD_WITNESS67=g1|g2)  67a (stage A)  67r (stage R1)  67p (stage R2)  67o (the pool block)  67d (the dump twins); stage T is tun67_battery.sh

    plan67.py plan <plan> [smoke]            the stage's table, `cell store arm seed rep` per line
    plan67.py count <plan> [smoke]           the number of rows it holds
    plan67.py armspec <plan> <cell> <arm>    `BYTES TIMEOUT_S HINT ENV...` (ENV tokens are KEY=VALUE); rc 1 on an unknown slot
    plan67.py seat <arm>                     the seat whose binary the arm runs
    plan67.py strings <seat>                 `need <s>` / `forbid <s>` lines: the strings that identify the seat's binary (ABORT-SEAT)
    plan67.py pin <seat>                     the commit pin env name (LD_PIN_<SEAT>); every seat's pin is REQUIRED environment, none has a default (the second amendment names them: ABORT-SEAT when unset)
    plan67.py seams [<G_ROWS67>]             the test seams that differ from the registered design, space separated (env: LD_HARD_MIN_S R67_T1_S LD_ROW_BYTES LD_BLOCK_ROW_S R67_N R67_BUDGET_S R67_SIM; source: the constants of REGISTERED and G_ROWS67 as the
                                             shell passes it); prints nothing when none is active. A plan-67 session aborts ABORT-SEAM on any of them unless LD_TEST_SEAMS=1 (the explicit stub marker)
    plan67.py budget <67pre|67|67g|67x>      the registered worst-case seconds the session needs before LD_HARD
    plan67.py check <plan> <ledger>          rc 0 iff the battery finished and the ledger holds a row for every slot (and no other)
    plan67.py replace <plan> <ledger> <cell> <arm> <seed> <rep>   battery helper: prints the reason, rc 0 iff the slot's one row is a VOID-WITNESS (or another replaceable void) and the stage's cap is not used up
    plan67.py s2-smoke-fail <stage> <smoke-check.txt>   rehearsal: rc 0 iff the stage's smoke failure is the S2 family's alone (g2; A / R1 with only S2 items): guard 2 VOID, the other smokes still run
    plan67.py stages <67pre|67|67g|67x>      the session's stage names in run order (LD_DROP_S2=1: the S2 stages and arms dropped; LD_DUMP67=1: the dump twins)

Arms. A, R1, G: the seat names (B0, S1, S2). R2: the pacer seats and their test-seams twins (P1, P2, P1T, P2T). The pool block: S1P<pool> / S2P<pool>. The dump twins: S1D / S2D.
Replacement (registration 4.2): a row failing a PRECONDITION witness is VOID-WITNESS and is re-run once, at most MAX_REPLACED_A slots per stage; an OUTCOME witness failure on an S2 row (taper > 0 or
plost > 0 where the preconditions hold, or a DNF where every S1 row of the cell completes) is S06-FAIL, never VOID and never replaced (s67_score.py applies the labels; this file lists them).
"""
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))

# ── seats (registration 1.2). need / forbid are strings the release binary must (not) contain: ABORT-SEAT before any row. NO seat has a default commit pin (a main merge moves the seat heads, a default goes stale
#    silently): every pin is required environment (LD_PIN_<SEAT>, the second amendment names it), a launch without it is ABORT-SEAT, like plan 51e's LD_BIN_SHA_EXPECT. ──
GAUGES = ("rlaw=", "pfec=")                                   # the c8 gauges (c8-gauges + c8-gauges-pfec): S1 and S2 carry them, B0 not
GUARD_MSG = "is a perf/native-object mode"                      # the tunnel guard's refusal text (config.rs tunnel_config)
SEATS = {
    "B0": {"tree": "B0", "features": "", "pin_env": "LD_PIN_B0", "pin": None, "need": (), "forbid": GAUGES + (GUARD_MSG, "pslot=", "[ACCT-RX]")},
    "S1": {"tree": "S1", "features": "", "pin_env": "LD_PIN_S1", "pin": None, "need": GAUGES + (GUARD_MSG,), "forbid": ("pslot=", "[ACCT-RX]")},
    "S2": {"tree": "S2", "features": "", "pin_env": "LD_PIN_S2", "pin": None, "need": GAUGES + (GUARD_MSG, "pslot="), "forbid": ("[ACCT-RX]",)},
    "P1": {"tree": "P1", "features": "", "pin_env": "LD_PIN_P1", "pin": None, "need": GAUGES + (GUARD_MSG, "RWM_PERF_RATE_MBIT", "offered-load pacing ACTIVE"), "forbid": ("pslot=", "[ACCT-RX]")},
    "P2": {"tree": "P2", "features": "", "pin_env": "LD_PIN_P2", "pin": None, "need": GAUGES + (GUARD_MSG, "pslot=", "RWM_PERF_RATE_MBIT", "offered-load pacing ACTIVE"), "forbid": ("[ACCT-RX]",)},
    "P1T": {"tree": "P1", "features": "test-seams", "pin_env": "LD_PIN_P1", "pin": None, "need": GAUGES + (GUARD_MSG, "RWM_PERF_RATE_MBIT", "offered-load pacing ACTIVE", "[ACCT-RX]"), "forbid": ("pslot=",)},
    "P2T": {"tree": "P2", "features": "test-seams", "pin_env": "LD_PIN_P2", "pin": None, "need": GAUGES + (GUARD_MSG, "pslot=", "RWM_PERF_RATE_MBIT", "offered-load pacing ACTIVE", "[ACCT-RX]"), "forbid": ()},
}
SEAT_ORDER = ("B0", "S1", "S2", "P1", "P2", "P1T", "P2T")
TREES = ("B0", "S1", "S2", "P1", "P2")                          # the five source trees ($ROOT/seats/<tree>/src); the twins are builds of P1's and P2's trees
S2_SEATS = ("S2", "P2", "P2T")                                  # dropped by LD_DROP_S2=1 (a seat abort on S2 stops guard 2 only)
ACCT_MARKERS = ("[ACCT]", "[ACCT-RX]")                          # the twins' kept-line filter (the section 64 [SYMSZ] lesson): both must reach the battery's kept log
PACER_LINE = "RWM_PERF_RATE_MBIT"                               # the one lib.sh forward line the pacer seats' own tools tree carries and the harness's does not (registration 1.2)
# (informational, not read: the pacer commit is 4ee1df9b; every pin comes from LD_PIN_<SEAT> only)

# ── stage tables ──
C2, C8 = "c2-100", "c8-100"
N_BLOCKS = 6               # stages A and R1: 6 blocks per cell = S1 12, S2 12, B0 6 per cell  (test_stubrun's fast mode shrinks it)
N_TWIN_REPS = 3            # stage R2: 3 reps x seeds 42 / 7 = 6 rows per arm                    (shrunk by test_stubrun)
N_POOL_REPS = 3            # the pool block: n = 3 per (seat, pool), seed 42                       (shrunk by test_stubrun)
MIN_LIVE_A = 10            # registration 4.2: fewer live S1 or S2 rows than this in a cell after replacement: UNSCOREABLE for AS and A1 (shrunk by test_stubrun)
MIN_LIVE_B0 = 4            # not registered (an ambiguity, reported): the 6 B0 rows of a cell need at least 4 live for A1's exact null to be read    (shrunk by test_stubrun)
MIN_TWIN = 5               # registration 4.4: at least 5 live twin rows per seat, else R2-HOLD is UNSCOREABLE and guard 2 is not eligible          (shrunk by test_stubrun)
MAX_REPLACED_A = 4         # registration 4.2: at most 4 replaced slots in stage A (the same cap is used for every stage but R2 and R1's 2)
MAX_REPLACED_R1 = 2        # plan 64's stage R rule (registration 4.3 "plan 64's stage R mechanism")
POOLS = (170, 256, 640)
POOL_SEED = "42"
SEEDS = ("42", "7")
BYTES_A, TIMEOUT_A = 600_000_000, 200
BYTES_R1, TIMEOUT_R1 = 100_000_000, 150
PACE_MBIT = "35"
SMOKE_ARMS_A = ("S1", "S2", "B0")
# the registered design: the constants above as the registration states them. test_stubrun's fast mode shrinks them (source edits) and so does any hand edit: `plan67.py seams` reports a constant that differs.
REGISTERED = {"N_BLOCKS": 6, "N_TWIN_REPS": 3, "N_POOL_REPS": 3, "MIN_LIVE_A": 10, "MIN_LIVE_B0": 4, "MIN_TWIN": 5, "G_ROWS67": 96}
SEAM_ENV = ("LD_HARD_MIN_S", "R67_T1_S", "LD_ROW_BYTES", "LD_BLOCK_ROW_S", "R67_N", "R67_BUDGET_S", "R67_SIM", "LD_DEVSHM", "LD_PROCFS", "LD_LOCK_VM", "LD_LOCK_RP", "LD_TMPD")
# R67_N / R67_BUDGET_S / R67_SIM shrink session 0's repeat and binid. LD_DEVSHM / LD_PROCFS / LD_LOCK_VM / LD_LOCK_RP / LD_TMPD redirect /dev/shm, /proc, the two operator locks and the per-row /tmp logs into a stub sandbox:
# a real session never sets them (run_all / the batteries default to /dev/shm, /proc, /tmp/rwm-vm.lock, $HOME/rp.lock, /tmp), so on a plan-67 session they are seams too.
BUDGET_S = {"67pre": 17040, "67": 7500, "67g": 10000, "67x": 15600}               # section 6 worst cases under LD_HARD = launch + 4.9 h (17 640 s, less the 600 s slack the others keep) / 2.25 h / 3 h / 4.5 h (the registration's 15.5 h worst total is the sum of re-runs); LD_HARD_MIN_S is a test seam

PLANS = ("41d", "67a", "67r", "67p", "67o", "67d")
STAGE_PLAN = {"g1": "41d", "g2": "41d", "a": "67a", "r1": "67r", "r2": "67p", "pool": "67o", "dump": "67d"}
STAGE_DIR = {"g1": "g1", "g2": "g2", "a": "sa", "r1": "r1", "r2": "r2", "pool": "pool", "dump": "dump", "t": "tun"}
SESSION = {"67pre": (), "67": ("g1", "g2", "a", "r1", "t", "r2"), "67g": ("g1", "g2"), "67x": ("a", "r1", "t", "r2", "dump", "pool")}
G_SEATS = {"g1": ("S1", "B0"), "g2": ("S2", "S1")}                  # stage -> (CAND, BASE)


_DROP_S2 = [None]      # set by drop_s2_as(): a scorer reading a pulled ledger takes the drop state from the ledger's LD-PLAN67 line, not from the host's environment


def drop_s2():
    return _DROP_S2[0] if _DROP_S2[0] is not None else os.environ.get("LD_DROP_S2") == "1"


class drop_s2_as:
    """`with drop_s2_as(True | False | None):` the tables below read that drop state (None = the environment)."""
    def __init__(self, v):
        self.v = v

    def __enter__(self):
        self.old, _DROP_S2[0] = _DROP_S2[0], self.v

    def __exit__(self, *a):
        _DROP_S2[0] = self.old


def dump_on():
    return os.environ.get("LD_DUMP67") == "1"


def stages(session):
    """The stages of a session in run order, as the coordinator's switches leave them: LD_DROP_S2=1 (guard 2 and every S2 arm gone), LD_G2_FAIL67=1 (session 3 after a G2 FAIL or an S06-FAIL in G2: R2, the pool
    block and the dump twins dropped as well, A / R1 / T kept), LD_DUMP67=1 (the dump twins run: the rehearsal found the [SUCC] line without the close times), LD_CUT67=dump (the one registered cap cut)."""
    out = [s for s in SESSION[session] if not (drop_s2() and s == "g2")]
    if session != "67x":
        return out
    cut = [x for x in os.environ.get("LD_CUT67", "").split(",") if x]
    out = [s for s in out if s != "dump" or (dump_on() and "dump" not in cut)]
    if os.environ.get("LD_G2_FAIL67") == "1":
        out = [s for s in out if s not in ("r2", "pool", "dump")]
    return out


def seat_of(arm):
    """The seat whose binary an arm runs: B0 / S1 / S2 / P1 / P2 / P1T / P2T are seats, S1P170 and S1D are S1 (pool block, dump twin), and so on."""
    for s in ("S1", "S2", "P1T", "P2T", "P1", "P2", "B0"):
        if arm == s or (arm.startswith(s) and arm[len(s):][:1] in ("P", "D") and (arm[len(s) + 1:].isdigit() or arm[len(s):] == "D")):
            return s
    return None


def pool_of(arm):
    """S1P170 -> 170; None for any other arm."""
    s = seat_of(arm)
    return int(arm[len(s) + 1:]) if s in ("S1", "S2") and arm[len(s):][:1] == "P" and arm[len(s) + 1:].isdigit() else None


def arms_a(plan_id="67a"):
    return tuple(a for a in ("S1", "S2", "B0") if not (drop_s2() and a == "S2"))


# ── tables ──
def _blocks(cells):
    """Stages A / R1: per block index b (cells alternate inside a block index, so a cell is not read at one time of the session), the registered block (S1, S2, S1, S2, B0); b even is seed 42, odd seed 7;
    S1 reps 2k+1 and 2k+2 and B0 rep k+1 with k = b // 2, S2 as S1."""
    out = []
    for b in range(N_BLOCKS):
        seed, k = SEEDS[b % 2], b // 2
        for cell in cells:
            for a, rep in (("S1", 2 * k + 1), ("S2", 2 * k + 1), ("S1", 2 * k + 2), ("S2", 2 * k + 2), ("B0", k + 1)):
                if not (drop_s2() and a == "S2"):
                    out.append((cell, "def", a, seed, rep))
    return out


def table_r2():
    """Stage R2 and its twins: group g = (seed SEEDS[g % 2], rep g // 2 + 1); the pair (P1, P2) then the twin pair (P1T, P2T), the order reversed on odd groups. P2 / P2T gone under LD_DROP_S2."""
    out = []
    for g in range(2 * N_TWIN_REPS):
        seed, rep = SEEDS[g % 2], g // 2 + 1
        order = ("P1", "P2", "P1T", "P2T") if g % 2 == 0 else ("P2", "P1", "P2T", "P1T")
        out += [(C2, "def", a, seed, rep) for a in order if not (drop_s2() and a in ("P2", "P2T"))]
    return out


def table_pool():
    """The pool block: c8-100, S1 and S2 at pools 170 / 256 / 640, n = N_POOL_REPS, seed 42, order rotated per rep (S1 first on odd reps)."""
    out = []
    for rep in range(1, N_POOL_REPS + 1):
        for pool in POOLS:
            for a in (("S1", "S2") if rep % 2 else ("S2", "S1")):
                if not (drop_s2() and a == "S2"):
                    out.append((C8, "def", "%sP%d" % (a, pool), POOL_SEED, rep))
    return out


def table_dump():
    """The dump twins (registration 4.2): one per seat, seed and cell for S1 and S2: 8 rows."""
    return [(cell, "def", a + "D", sd, 1) for cell in (C2, C8) for sd in SEEDS for a in ("S1", "S2") if not (drop_s2() and a == "S2")]


def plan(plan_id, smoke=False):
    """[(cell, store, arm, seed, rep)] in run order. The smokes (the rehearsal): A one c8 row per seat, R1 one row per seat, R2 one row per pacer seat and twin."""
    if plan_id == "67a":
        return [(C8, "def", a, "42", 1) for a in arms_a()] if smoke else _blocks((C2, C8))
    if plan_id == "67r":
        return [(C2, "def", a, "42", 1) for a in arms_a()] if smoke else _blocks((C2,))
    if plan_id == "67p":
        return [(C2, "def", a, "42", 1) for a in ("P1", "P2", "P1T", "P2T") if not (drop_s2() and a in ("P2", "P2T"))] if smoke else table_r2()
    if plan_id == "67o":
        return table_pool()
    if plan_id == "67d":
        return table_dump()
    raise ValueError("unknown plan " + str(plan_id))


def valid_slot(plan_id, cell, arm):
    return any(c == cell and a == arm for c, _, a, _, _ in plan(plan_id) + plan(plan_id, True)) or (plan_id == "67o" and cell == C8 and pool_of(arm) in POOLS) \
        or (plan_id == "67d" and cell in (C2, C8) and arm in ("S1D", "S2D"))


def armspec(plan_id, cell, arm):
    """`BYTES TIMEOUT HINT ENV...`. Stage A / pool / dump: Auto at rho = 1 (RWM_C_RELIABLE is scrubbed, so perf_rwm_c.sh passes --window-reliable), 600 MB, 200 s. R1: Realtime, 100 MB, 150 s, rho = 1.
    R2 and its twins: block E's PM shape (plan51e.py: Realtime, RWM_C_RELIABLE=0 = evicting, RWM_PERF_RATE_MBIT=35, 262.5 MB, 75 s). The pool block adds RWM_STORE_PATH_POOL, the dump twins RWM_SUCC_DUMP=1."""
    if not valid_slot(plan_id, cell, arm):
        return None
    if plan_id == "67a":
        return (BYTES_A, TIMEOUT_A, "auto", [])
    if plan_id == "67r":
        return (BYTES_R1, TIMEOUT_R1, "realtime", [])
    if plan_id == "67p":
        import plan51e
        return (plan51e.arm_bytes(cell, "PM"), plan51e.arm_timeout(cell, "PM"), "realtime", plan51e.arm_env(cell, "PM"))
    if plan_id == "67o":
        return (BYTES_A, TIMEOUT_A, "auto", ["RWM_STORE_PATH_POOL=%d" % pool_of(arm)])
    if plan_id == "67d":
        return (BYTES_A, TIMEOUT_A, "auto", ["RWM_SUCC_DUMP=1"])
    return None


def expect_slots(plan_id):
    return {(c, a, sd, str(rp)) for c, _, a, sd, rp in plan(plan_id)}


def block1_n(plan_id):
    """The rows of the stage's first block (the block-1 rule, registration 5.2): after that many rows the battery asks `s67_score.py block1`. 0 = no such rule (the dump twins)."""
    n = len(plan(plan_id))
    return {"67a": n // N_BLOCKS, "67r": n // N_BLOCKS, "67p": n // (2 * N_TWIN_REPS), "67o": n // N_POOL_REPS}.get(plan_id, 0)


def replace_cap(plan_id):
    return MAX_REPLACED_R1 if plan_id == "67r" else MAX_REPLACED_A


# a status that the battery re-runs once (stage-wide cap above). S06-FAIL, BASE rows' DNF and R2's twin rows without any [ACCT] line are handled by the scorer's own labels.
REPLACEABLE = ("VOID-WITNESS", "VOID-RHO", "CONTAMINATED", "VOID-COTENANT", "VOID-ONDISK", "WITNESS-FAIL", "VOID-SHORT", "NO_DATA")


def replaceable(status, arm):
    """REPLACEABLE for this arm: NO_DATA is replaceable on the reference arms only; a candidate arm's (S2 family) NO_DATA stays a crash (CANDIDATE-CRASH), as place38_parse's REPLACEABLE treats it."""
    return status in REPLACEABLE and not (status == "NO_DATA" and seat_of(arm) in S2_SEATS)


def slot_rows(rows):
    d = {}
    for r in rows:
        d.setdefault((r["cell"], r["arm"], str(r["seed"]), str(r["rep"])), []).append(r)
    return d


def replace(plan_id, ledger, cell, arm, seed, rep):
    """Battery helper: the slot's rows so far are exactly one, it is a replaceable void, and fewer than the stage's cap of slots hold two rows -> print the reason, rc 0; else rc 1."""
    import s67_score
    rows = s67_score.rows_of([ledger])[0]
    sl = slot_rows(rows)
    mine = sl.get((cell, arm, str(seed), str(rep)), [])
    done = sum(1 for v in sl.values() if len(v) > 1)
    if len(mine) == 1 and replaceable(mine[0]["status"], arm) and done < replace_cap(plan_id):
        print(mine[0]["status"].lower())
        return 0
    return 1


def seams(g_rows=None):
    """The active test seams: the env ones (non-empty) and every constant of REGISTERED that is not its registered value (G_ROWS67 is the shell's, passed in; None = not asked)."""
    out = [k for k in SEAM_ENV if os.environ.get(k)]
    cur = dict(globals(), G_ROWS67=g_rows)
    out += [f"{k}={cur[k]}(registered {v})" for k, v in REGISTERED.items() if cur[k] is not None and int(cur[k]) != v]
    return out


def check(plan_id, path):
    import s67_score
    rows = s67_score.rows_of([path])[0]
    t = open(path, errors="replace").read()
    got = {(r["cell"], r["arm"], str(r["seed"]), str(r["rep"])) for r in rows}
    want = expect_slots(plan_id)
    ok = "LD-BATTERY-DONE" in t and got == want
    print(f"CHECK{plan_id} done={'LD-BATTERY-DONE' in t} slots={len(got & want)}/{len(want)} rows={len(rows)} extra={len(got - want)}")
    return 0 if ok else 1


def kept_acct(path):
    """Rehearsal criterion (registration 6.1): every twin row (P1T / P2T) of a ledger shows an [ACCT] line on the client AND an [ACCT-RX] line on the server IN THE LEDGER (the battery's ACCT-KEPT lines), not only in the raw log.
    rc 0 iff so; prints one line per twin row."""
    import re
    rows, bad, head = {}, [], None
    for ln in open(path, errors="replace"):
        m = re.match(r"=== rep=\d+ seed=(\d+) cell=(\S+) arm=(\S+) attempt", ln)
        if m:
            head = (m.group(3), m.group(1))
            rows.setdefault(head, set())
        elif head and ln.startswith("ACCT-KEPT side=cli ") and "[ACCT]" in ln:
            rows[head].add("cli")
        elif head and ln.startswith("ACCT-KEPT side=srv ") and "[ACCT-RX]" in ln:
            rows[head].add("srv")
    for (arm, seed), got in sorted(rows.items()):
        if arm in ("P1T", "P2T"):
            miss = sorted({"cli", "srv"} - got)
            print(f"{'PASS' if not miss else 'FAIL'} kept-log {arm} s{seed}: [ACCT] on cli, [ACCT-RX] on srv" + (f"; missing {','.join(miss)}" if miss else ""))
            bad += miss
    twin = [k for k in rows if k[0] in ("P1T", "P2T")]
    if not twin:
        print("FAIL kept-log: no twin row in the ledger")
        return 1
    return 1 if bad else 0


def s2_smoke_fail(stage, path):
    """Rehearsal helper: rc 0 iff the stage's smoke-check.txt reads SMOKE-FAIL and the failure is the S2 family's alone: every g2 failure (guard 2's own stage), or in A / R1 only S2 arm items (`S2: ...`, no S1 or B0 item).
    The rehearsal then marks guard 2 VOID and runs the other smokes (as for R2); any other smoke failure stops it."""
    if stage not in ("g2", "a", "r1"):
        return 1
    try:
        lines = open(path, errors="replace").read().splitlines()
    except OSError:
        return 1
    fail = [ln for ln in lines if ln.startswith("SMOKE-FAIL")]
    if not fail:
        return 1
    if stage == "g2":
        return 0
    arms = {x.split(":", 1)[0].strip() for ln in fail for x in ln[len("SMOKE-FAIL"):].split(";") if ":" in x}
    return 0 if "S2" in arms and not (arms & {"S1", "B0"}) else 1


def main(argv):
    if not argv:
        print(__doc__)
        return 2
    cmd, a = argv[0], argv[1:]
    if cmd in ("plan", "count"):
        rows = plan(a[0], len(a) > 1 and a[1] == "smoke")
        if cmd == "count":
            print(len(rows))
        else:
            for c, s, ar, sd, rp in rows:
                print(f"{c} {s} {ar} {sd} {rp}")
        return 0
    if cmd == "armspec":
        spec = armspec(a[0], a[1], a[2])
        if spec is None:
            print(f"UNKNOWN-ARM {a[2]} at {a[1]} (plan {a[0]})")
            return 1
        print(spec[0], spec[1], spec[2], *spec[3])
        return 0
    if cmd == "seat":
        s = seat_of(a[0])
        print(s or "")
        return 0 if s else 1
    if cmd == "strings":
        s = SEATS[a[0]]
        for x in s["need"]:
            print("need", x)
        for x in s["forbid"]:
            print("forbid", x)
        return 0
    if cmd == "pin":
        s = SEATS[a[0]]
        print(s["pin_env"], s["pin"] or "")
        return 0
    if cmd == "seams":
        print(" ".join(seams(int(a[0]) if a and a[0].isdigit() else None)))
        return 0
    if cmd == "budget":
        print(BUDGET_S[a[0]])
        return 0
    if cmd == "block1n":
        print(block1_n(a[0]))
        return 0
    if cmd == "kept-acct":
        return kept_acct(a[0])
    if cmd == "s2-smoke-fail":
        return s2_smoke_fail(a[0], a[1])
    if cmd == "stages":
        print(*stages(a[0]))
        return 0
    if cmd == "check":
        return check(a[0], a[1])
    if cmd == "replace":
        return replace(a[0], a[1], a[2], a[3], a[4], a[5])
    print(f"unknown command {cmd}")
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
