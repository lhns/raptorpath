#!/usr/bin/env python3
"""Status 51, block E: the evicting-seat m-bar / W1 / K4 block with the spare-clamp gauge (plan 51e). Tables, per-arm run spec, row witnesses, the replacement rule and the slot check.
docs/derivations/s51-evict-design.md section 4 (rows, load points), 5.2 (pacing witness), 6.2 (gauge witnesses), 10 (go/no-go), 11 (budget). ONE source for the shell and the scorer: the battery asks
`armspec` for every row's bytes / timeout / hint / env (plan60.py's pattern), and s51e_score.py imports this module.

    plan51e.py plan 51e [smoke|rehearsal]    the table, `cell store arm seed rep` per line (smoke = ONE PM row; rehearsal = PL, PM, U, one each)
    plan51e.py count 51e [smoke|rehearsal]   the number of rows the table holds (the battery asserts it)
    plan51e.py armspec <cell> <arm>          `BYTES TIMEOUT_S HINT ENV...` (ENV tokens are KEY=VALUE, RWM_SLAW first)
    plan51e.py replace <ledger> <cell> <arm> <seed> <rep>    prints the reason and returns 0 iff the slot's rows so far are exactly one and replaceable (once only)
    plan51e.py check 51e <ledger>            rc 0 iff the battery finished and the ledger holds a row for every slot
    plan51e.py dropped                       1 iff PL was dropped by amendment (the battery's table-size assert follows it)
    plan51e.py calibrate <median A*>         the PL calibration rule (section 4): prints x-hat, the new PL rate (sym/s), lambda, bytes

LD_ONLY=PL (with LD_REHEARSE=1) is the PL-only recheck row of the calibration rule: the rehearsal table is filtered to the named arms.

Arms (section 4): PL / PM are paced (RWM_PERF_RATE_MBIT, hint-independent offered load), U is the unpaced positive control. Every arm is the evicting seat: Realtime hint, RWM_C_RELIABLE=0, the gauge on
(RWM_SLAW=1), RWM_GEN=0 RWM_DIAG=1 RWM_RDIAG=1 from the battery. THE PL RATE IS ONE NUMBER: PL_RATE_SYM (symbols/s). lambda, the bytes and the RWM_PERF_RATE_MBIT token derive from it, so the registered
calibration amendment is a one-line edit of that constant (PL_DROPPED = True drops PL by amendment). PM's one number is PM_LAMBDA_MBIT."""
import json
import math
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

C2 = "c2-100"
CELL = C2
ARMS = ("PL", "PM", "U")
PACED = ("PL", "PM")
# ── the registered load points (section 4); the ONLY numbers an amendment may change ──
PL_RATE_SYM = 700                  # symbols/s; the calibration rule (section 4) re-sets it, rounded to 10 sym/s
PL_DROPPED = False                 # set True by amendment when the PL recheck fails (a one-line edit)
PM_LAMBDA_MBIT = 35.0              # payload Mbit/s
CHUNK_PAYLOAD = 494                # B: the Realtime chunk payload (perf.rs chunk_payload_len: 508 - 14)
T_E = 60                           # s of emission per paced row
TIMEOUT_PACED = T_E + 15           # RWM_PERF_TIMEOUT_S: the DNF lands 15 s after emission ends
BYTES_U, TIMEOUT_U = 600_000_000, 30
SEEDS = ("42", "7", "42", "7", "42")
N_REP = {"PL": 5, "PM": 5, "U": 2}
REPS_CAP51E = None                 # test_stubrun's fast mode caps the replicates (None = the registered design)
BRINGUP_S = 15                     # per-row bring-up (section 11)
# Budget (section 11): the battery's worst case is every slot twice (990 s of rows once) + build + smoke + the tools tests. The launch sets LD_HARD = launch + 1.5 h (battery) / + 1 h (rehearsal).
ROW_S = {"PL": TIMEOUT_PACED + BRINGUP_S, "PM": TIMEOUT_PACED + BRINGUP_S, "U": TIMEOUT_U + BRINGUP_S}
ROWS_ONCE_S = sum(N_REP[a] * ROW_S[a] for a in ARMS)                                   # 990
BUDGET_S = {"51e": 2 * ROWS_ONCE_S + 360 + 90 + 300, "51er": 1500}                      # battery worst case 2 730 s; rehearsal worst case 25 min (section 11)
LATE_T = 5.0                       # section 8.1: LATE is t >= 5 s
SMOKE = [(CELL, "def", "PM", "42", 1)]
REHEARSAL = [(CELL, "def", "PL", "42", 1), (CELL, "def", "PM", "42", 1), (CELL, "def", "U", "42", 1)]
PRED_RATE_BAND = 0.05              # section 5.2: median LATE src within +-5 % of rate_sym
PACE_ACH_TOL = 0.02                # section 5.2: achieved_mbit within +-2 % of lambda
PACE_STALL_MAX_MS = 250.0          # status 51 Amendment 4: pace-stall iff late_max_ms > 250 (one reading window, declared); replaces section 5.2's `late <= 1 %`, which is now only reported
OFFLOAD_BAND = PRED_RATE_BAND      # the per-window version of the median src check: a LATE tick whose src is off the offered rate by more than 5 % is OFF-LOAD (an engine reading, never a void)
LAST_TICK_SLACK_S = 2.0            # section 5.2: the last tick with src > 0 at t >= T_e - 2 s


def active_arms():
    return tuple(a for a in ARMS if not (a == "PL" and PL_DROPPED))


def only_arms():
    """LD_ONLY=PL[,PM]: the PL-only recheck (a rehearsal filter); empty = every active arm."""
    sel = tuple(x for x in os.environ.get("LD_ONLY", "").split(",") if x)
    return tuple(a for a in active_arms() if not sel or a in sel)


# ── the load points, derived from the one table number of each arm ──
def lam_mbit(arm):
    """lambda (payload Mbit/s): PL = rate_sym x 8 x 494 / 1e6 (2.7664 at 700), PM = the table's 35; U has none."""
    if arm == "PL":
        return round(PL_RATE_SYM * 8 * CHUNK_PAYLOAD / 1e6, 6)
    if arm == "PM":
        return PM_LAMBDA_MBIT
    return None


def rate_sym(arm):
    """rate_sym = lambda / (8 x 494 B), symbols (= chunks) per second at Realtime; PM rounds to the table's 8 856."""
    lam = lam_mbit(arm)
    return None if lam is None else (PL_RATE_SYM if arm == "PL" else int(lam * 1e6 / (8 * CHUNK_PAYLOAD)))


def rate_token(arm):
    """The RWM_PERF_RATE_MBIT value the battery sets and the [GATES] echo must read (numerically)."""
    lam = lam_mbit(arm)
    return None if lam is None else ("%.6f" % lam).rstrip("0").rstrip(".")


def arm_bytes(cell, arm):
    lam = lam_mbit(arm)
    return BYTES_U if lam is None else int(round(lam * 1e6 * T_E / 8))              # PL 20 748 000, PM 262 500 000


def arm_timeout(cell, arm):
    return TIMEOUT_U if arm == "U" else TIMEOUT_PACED


def arm_hint(arm):
    return "realtime"


def arm_env(cell, arm):
    """[KEY=VALUE]: RWM_SLAW first (the echo witnessed on both ends), RWM_C_RELIABLE=0 (the evicting seat), RWM_PERF_RATE_MBIT on the paced arms only. RWM_GEN=0 RWM_DIAG=1 RWM_RDIAG=1 come from the battery."""
    if arm not in ARMS:
        raise ValueError(f"unknown arm {arm}")
    env = ["RWM_SLAW=1", "RWM_C_RELIABLE=0"]
    tok = rate_token(arm)
    if tok is not None:
        env.append("RWM_PERF_RATE_MBIT=" + tok)
    return env


VALID = {(CELL, a) for a in ARMS}


def valid_slot(cell, arm):
    return (cell, arm) in VALID


def all_arms():
    return list(ARMS)


def _cap(n):
    return n if REPS_CAP51E is None else min(n, REPS_CAP51E)


def plan(name="51e", smoke=False, rehearsal=False):
    """[(cell, store, arm, seed, rep)] in run order. Blocked by replicate: PL, PM, U in the first two replicates, then PL and PM only; PL / PM alternate (odd replicates PL first). Seeds 42 / 7 / 42 / 7 / 42
    (U: 42 and 7). A replaced slot adds a ledger row, not a table row. LD_ONLY filters the smoke and rehearsal sets (the PL-only recheck); the full table honours PL_DROPPED."""
    if name != "51e":
        raise ValueError(name)
    keep = set(only_arms())
    if smoke:
        return [r for r in SMOKE if r[2] in keep]
    if rehearsal:
        return [r for r in REHEARSAL if r[2] in keep]
    out = []
    for k in range(1, _cap(max(N_REP.values())) + 1):
        sd = SEEDS[k - 1]
        pair = ["PL", "PM"] if k % 2 == 1 else ["PM", "PL"]
        for a in pair + (["U"] if k <= N_REP["U"] else []):
            if a in active_arms():
                out.append((CELL, "def", a, sd, k))
    return out


def expect_rows(name="51e"):
    return len(plan(name))


# ── row witnesses and the replacement rule ──
ECHO_PREFIX = ("gate-slaw", "gate-diag", "gate-three-term", "evict-", "gate-unified", "no-realtime-echo", "pace-", "spc-")      # the VOIDs the battery replaces once; "bin-sha-*" is deliberately NOT here


def row_problems(cell, arm, cli, srv, sha=None, sha_cand=None, drv=None):
    """The row's witnesses: plan 59's liveness (the shed echo, shed= on [SPAN], RWM_UNIFIED / RWM_UNIFIED_SHED on both ends, no retain-until-acked echo, the Realtime echo) with RWM_SLAW=1 / RWM_DIAG=1 on both ends
    (plan60.row_problems for an evicting arm), the one seat's binary, the pacing witness of section 5.2 (pace-*) and the gauge witnesses of section 6.2 (spc-*)."""
    import plan60
    import s51e_score as sc
    if not valid_slot(cell, arm):
        return [f"unknown-arm-{arm}-at-{cell}"]
    p = plan60.row_problems(cell, plan60.EVICT_ARM, cli, srv, sha, sha_cand)
    t = sc.row_metrics(cli, srv, drv or [], arm_bytes(cell, arm), cell, arm, None)
    return p + sc.pace_problems(arm, t) + sc.gauge_problems(t)


def is_echo_void(row):
    return any(x.startswith(ECHO_PREFIX) for x in row.get("problems") or [])


def replace_reason(ledger, cell, arm, seed, rep):
    """-> the reason the slot is re-run once, or None: its rows so far are exactly one and that row is VOID by an echo / liveness witness (RWM_SLAW, RWM_DIAG, the evicting seat's liveness incl. the Realtime echo,
    the pacing witness pace-*, the gauge witness spc-*). A WRONG BINARY (bin-sha-*) is never re-run, and a DNF is a datum, never re-run (section 10)."""
    import place38_parse as pp
    rows, _ = pp.rows_of([ledger])
    mine = [r for r in rows if (r.get("cell"), r.get("arm"), str(r.get("seed")), str(r.get("rep"))) == (cell, arm, str(seed), str(rep))]
    if len(mine) != 1:
        return None
    r = mine[0]
    if not is_echo_void(r):
        return None
    pr = r["problems"]
    for pref, why in (("pace-", "pace"), ("spc-", "spc"), ("gate-slaw", "slaw-echo"), ("gate-diag", "slaw-echo")):
        if any(x.startswith(pref) for x in pr):
            return why
    return "evict-liveness"


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


def calibrate(median_astar):
    """Section 4's PL calibration rule: x-hat = A*~ - 0.5; the new PL rate = PL_RATE_SYM x 4.5 / x-hat, rounded to 10 sym/s (the registered 700 x 4.5 / x-hat on the first pass), so that rate x b x rtprop sits mid-integer
    at A* = 5. -> dict(x_hat, rate_sym, lam_mbit, bytes) or None when A* is undefined or x-hat <= 0."""
    if median_astar is None or median_astar - 0.5 <= 0:
        return None
    x_hat = median_astar - 0.5
    new = int(math.floor(PL_RATE_SYM * 4.5 / x_hat / 10.0 + 0.5)) * 10      # round half up to 10 sym/s
    if new <= 0:
        return None
    lam = round(new * 8 * CHUNK_PAYLOAD / 1e6, 6)
    return {"x_hat": x_hat, "rate_sym": new, "lam_mbit": lam, "bytes": int(round(lam * 1e6 * T_E / 8))}


def main(argv):
    if not argv:
        print(__doc__)
        return 2
    cmd, a = argv[0], argv[1:]
    if cmd in ("plan", "count"):
        mode = a[1] if len(a) > 1 else ""
        rows = plan(a[0], smoke=(mode == "smoke"), rehearsal=(mode == "rehearsal"))
        if cmd == "count":
            print(len(rows))
        else:
            for c, s, ar, sd, rp in rows:
                print(f"{c} {s} {ar} {sd} {rp}")
        return 0
    if cmd == "dropped":
        print(1 if PL_DROPPED else 0)
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
    if cmd == "calibrate":
        c = calibrate(float(a[0]))
        print(json.dumps(c) if c else "UNCOMPUTABLE")
        return 0 if c else 1
    print(f"unknown command {cmd}")
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
