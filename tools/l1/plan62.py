#!/usr/bin/env python3
"""Status 62: c3 at jitter 0 (the knee need with the jitter removed), plans 62r0 (rehearsal) / 62a (the cell). Tables, per-arm run spec, row witnesses, the replacement rule and the ledger check.
docs/derivations/s62-registration.md sections 2 (the cell), 3.2 (the pilot), 3.3 (the rehearsal), 8 (this module's brief). ONE source for the shell and the scorer: the battery asks `armspec` for every row's
bytes / timeout / hint / env (s62_score.py imports this module). Built on plan61.py's pattern; plan 61's tables are untouched.

    plan62.py plan <62r0|62a> [smoke]       the table, `cell store arm seed rep` per line (62a needs LD_KNEE62, else exit 3 = ABORT-PILOT)
    plan62.py count <62r0|62a> [smoke]      the number of rows the table holds (the battery asserts it)
    plan62.py armspec <cell> <arm>          `BYTES TIMEOUT_S HINT ENV...` (ENV tokens are KEY=VALUE, RWM_SLAW first)
    plan62.py replace <ledger> <cell> <arm> <seed> <rep>    prints the reason and returns 0 iff the slot's rows so far are exactly one and replaceable (once only)
    plan62.py check <62a|62r0> <ledger>     rc 0 iff the battery finished and the ledger holds a row for every slot
    plan62.py pilot <u_lo> <u_hi> [p_lo p_hi]   the pilot's W-hat (section 3.2) and the arms it places

Environment: LD_KNEE62 = the pilot's W-hat (verbatim from the rehearsal's `PILOT62 W=`); LD_CTL62 = 512 is required (the control is W 512, section 3.2; run_all refuses a launch without it: ABORT-CONTROL62).
Arms: `G<W>` = the gauge on (RWM_SLAW=1), W = the realised store window (one live path: RWM_STORE_PATH_POOL = W); `D<W>` = the same arm with RWM_RTT_DUMP=1 (kind D, section 2: excluded from every scored median).
Every row is Bulk."""
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import plan61 as p61  # noqa: E402

C3J0 = "c3j0-25"
CELLS = (C3J0,)
NLIVE = {C3J0: 1}
FLOOR_W = (12, 24)                                       # section 2: the two floor arms (W 12 and W 24)
CTL_W = 512                                              # section 3.2: the control
PILOT62 = (80, 128)                                      # section 3.2: the probes
MULT_K, MULT_P1 = (0.92, 1.06, 1.20), 1.5                # section 3.2: K = r4(0.92 / 1.06 / 1.20 W-hat), P1 = r8(1.5 W-hat)
W_HAT_MIN, W_HAT_MAX = 48, 192                           # section 3.2: the clamp
REPROBE_LOW, REPROBE_HIGH = (112, 160), (64, 96)         # section 3.3: both probes below 0.95 / both at or above it
N_REP = 6
REPS_CAP62 = None                                        # test_stubrun's fast mode caps the replicates (None = the registered design)
SHIPPED_POOL = 2048
LATE_T = {C3J0: 15.0}                                    # LATE windows t >= 15 s
import plan60 as p60  # noqa: E402
p60.LATE_T.setdefault(C3J0, 15.0)                        # s60_score.row_metrics and s61_score.row_extras read LATE_T.get(cell, 15.0): a silent default for an unregistered cell, so the cell is registered in both
p61.LATE_T.setdefault(C3J0, 15.0)
BYTES_FLOOR_MB = {12: 25, 24: 30}                        # section 2: W 12 at 25 MB, W 24 at 30 MB, both 150 s
TIMEOUT_FLOOR, TIMEOUT_REST = 150, 120                   # the rest as 61a
BYTES_KNEE_MB = p61.BYTES_KNEE_MB                        # 120 MB at the K, P1 and control arms
LD_HARD_S = {"62r0": 3600, "62a": 9000}                  # section 8: the registered LD_HARD offsets (launch + 1.0 h / + 2.5 h)
BUDGET_S = {"62r0": 2400, "62a": 7800}                   # run_all's guard (ABORT-BUDGET): the WORST CASE, strictly under the offset (the guard reads the time left after the lock wait). 62a: 44 rows x 150 s + 45 bring-ups (~15 s) = 7 275 s,
                                                         # the smoke row inside it, rounded up; 62r0: 9 rows (R1-R6, D1, the re-probe) x 150 s + 9 bring-ups + one floor re-run = 1 815 s, rounded up. A guard AT the offset would refuse every registered launch
D_REPS = ((42, 7), (7, 8))                               # section 2: the two dump rows' (seed, rep): D at W 12, D at K2
REHEARSAL = [(C3J0, "def", "G512", "42", 1),             # R1: control
             (C3J0, "def", "G12", "42", 1), (C3J0, "def", "G12", "7", 2),        # R2, R3: the floor pair
             (C3J0, "def", "G80", "42", 1), (C3J0, "def", "G128", "42", 1),      # R4, R5: the probes
             (C3J0, "def", "G512", "7", 2),              # R6: control
             (C3J0, "def", "D12", "42", 3)]              # D1: the dump row at W 12
UNSET_KNEE = None
r_to = p61.r_to


def arm_parse(arm):
    """`G192` -> ('G', 192), `D12` -> ('D', 12); anything else -> (None, None)."""
    m = re.fullmatch(r"([GD])(\d+)", arm or "")
    return (m.group(1), int(m.group(2))) if m else (None, None)


def clamp_knee(w):
    return min(max(float(w), float(W_HAT_MIN)), float(W_HAT_MAX))


def pilot_detail(u_lo, u_hi, p_lo=PILOT62[0], p_hi=PILOT62[1]):
    """Section 3.2's pilot rule: -> (W-hat rounded to 0.1 after the clamp to [48, 192], case, clamped: bool) or None when a utilisation is <= 0 (no division). The cap is p_hi, the rule's own bound.
    Cases: proportional (u_lo < 0.95 <= u_hi: min(p_lo * 0.95 / u_lo, p_hi)), secant (both below 0.95, u_hi > u_lo: p_hi + (0.95 - u_hi) * (p_hi - p_lo) / (u_hi - u_lo)), degenerate (both below, u_hi <= u_lo:
    p_hi * 0.95 / u_hi), both-above (both at or above 0.95: p_lo * 0.95 / u_lo), non-monotone (u_lo >= 0.95 > u_hi: p_lo * 0.95 / u_lo). Only `proportional` is GO (section 3.3). W-hat is registered at 0.1-slot
    resolution: the arms derive from the PRINTED value (it is what LD_KNEE62 receives verbatim)."""
    if not (u_lo > 0 and u_hi > 0):
        return None
    if u_lo < 0.95 <= u_hi:
        case, raw = "proportional", min(p_lo * 0.95 / u_lo, float(p_hi))
    elif u_lo < 0.95 and u_hi < 0.95:
        case, raw = ("secant", p_hi + (0.95 - u_hi) * (p_hi - p_lo) / (u_hi - u_lo)) if u_hi > u_lo else ("degenerate", p_hi * 0.95 / u_hi)
    elif u_hi >= 0.95:
        case, raw = "both-above", p_lo * 0.95 / u_lo
    else:
        case, raw = "non-monotone", p_lo * 0.95 / u_lo
    return round(clamp_knee(raw), 1), case, not (W_HAT_MIN <= raw <= W_HAT_MAX)


def reprobe_pair(u_lo, u_hi):
    """Section 3.3's one registered re-probe: both probes below 0.95 -> W 112 / 160; both at or above it -> W 64 / 96; a mixed pair (u_lo >= 0.95 > u_hi, non-monotone) has none: NO-GO-PILOT at once."""
    if u_lo < 0.95 and u_hi < 0.95:
        return REPROBE_LOW
    if u_lo >= 0.95 and u_hi >= 0.95:
        return REPROBE_HIGH
    return None


def arms(w_hat):
    """Section 3.2's arms from the pilot knee: {'K': 3 knee-bracket arms r4(0.92 / 1.06 / 1.20 W-hat), 'P1': r8(1.5 W-hat)}."""
    w = clamp_knee(w_hat)
    return {"K": [r_to(m * w, 4) for m in MULT_K], "P1": r_to(MULT_P1 * w, 8)}


def knee_env():
    """LD_KNEE62 as a float in [48, 192], or None when unset / empty. A set but unparsable or out-of-range value is an error (ValueError): the launch must not run on a guess."""
    v = os.environ.get("LD_KNEE62", "").strip()
    if not v:
        return UNSET_KNEE
    w = float(v)
    if not (W_HAT_MIN <= w <= W_HAT_MAX):
        raise ValueError(f"LD_KNEE62={v} is outside the pilot clamp [{W_HAT_MIN}, {W_HAT_MAX}]")
    return w


def _all_w():
    """Every W an arm of a 62 plan can carry for any pilot knee (the scorer's valid_slot and place38_parse's ARM_SPEC)."""
    ws = set(FLOOR_W) | {CTL_W} | set(PILOT62) | set(REPROBE_LOW) | set(REPROBE_HIGH)
    for i in range(W_HAT_MIN * 10, W_HAT_MAX * 10 + 1):
        a = arms(i / 10.0)
        ws |= set(a["K"]) | {a["P1"]}
    return ws


ALL_W = _all_w()


def valid_slot(cell, arm):
    kind, w = arm_parse(arm)
    if cell != C3J0 or kind is None:
        return False
    return w in ALL_W if kind == "G" else w in ALL_W - {CTL_W}


def all_arms():
    """Every arm name any 62 plan can carry (place38_parse's ARM_SPEC registration)."""
    return sorted({"G%d" % w for w in ALL_W} | {"D%d" % w for w in ALL_W})


def is_floor(cell, arm):
    kind, w = arm_parse(arm)
    return kind == "G" and w in FLOOR_W


def _knee(knee):
    return knee_env() if knee == "env" else knee


def arm_bytes(cell, arm, knee="env"):
    """Section 2: W 12 25 MB, W 24 30 MB; 120 MB at the control and at and above K1 (K1 from LD_KNEE62); else 61a's formula (the probes: with no pilot knee yet no row is 'at the knee bracket')."""
    w = arm_parse(arm)[1]
    if w in BYTES_FLOOR_MB:
        return BYTES_FLOOR_MB[w] * 1_000_000
    if w == CTL_W:
        return BYTES_KNEE_MB * 1_000_000
    k = _knee(knee)
    if k is not None and w >= arms(k)["K"][0]:
        return BYTES_KNEE_MB * 1_000_000
    return p61.formula_mb(w) * 1_000_000


def arm_timeout(cell, arm):
    return TIMEOUT_FLOOR if arm_parse(arm)[1] in FLOOR_W else TIMEOUT_REST


def arm_hint(arm):
    return "bulk"


def arm_env(cell, arm):
    """[KEY=VALUE]: RWM_SLAW=1 first (the echo witnessed on both ends), RWM_RTOBS=1 (plan 60's instrument env), the pool knob (not at the shipped 2048), and RWM_RTT_DUMP=1 on a D arm."""
    kind, w = arm_parse(arm)
    if kind is None:
        raise ValueError(f"unknown arm {arm}")
    env = ["RWM_SLAW=1", "RWM_RTOBS=1"]
    if w != SHIPPED_POOL:
        env.append("RWM_STORE_PATH_POOL=%d" % w)
    if kind == "D":
        env.append("RWM_RTT_DUMP=1")
    return env


def _cap(n):
    return n if REPS_CAP62 is None else min(n, REPS_CAP62)


def grid_ws(w_hat):
    """The 7 scored arms in table order: floor 12 / 24, K x3, P1, the control."""
    a = arms(w_hat)
    ws = list(FLOOR_W) + a["K"] + [a["P1"], CTL_W]
    if len(set(ws)) != len(ws):
        raise ValueError(f"arm collision at W-hat {w_hat}: {ws}")
    return ws


def _grid_rows(ws):
    """A cyclic Latin square across replicates, seeds 42 / 7 alternating (plan61._grid_rows)."""
    out = []
    for k in range(_cap(N_REP)):
        j = k % len(ws)
        a = ["G%d" % w for w in ws]
        out += [(C3J0, "def", x, "42" if k % 2 == 0 else "7", k + 1) for x in a[j:] + a[:j]]
    return out


def plan(name, smoke=False, knee="env"):
    """[(cell, store, arm, seed, rep)] in run order. 62r0: the 7 rehearsal rows (R1-R6 and the dump row D1; a re-probe adds two slots, see reprobe_slots). 62a: 7 arms x 6 replicates plus the two dump rows,
    from LD_KNEE62 (ValueError otherwise); its smoke is the middle knee arm K2."""
    if name == "62r0":
        return list(REHEARSAL)
    if name == "62a":
        k = _knee(knee)
        if k is None:
            raise ValueError("LD_KNEE62 is unset: 62a needs the rehearsal's PILOT62 W-hat (ABORT-PILOT)")
        a = arms(k)
        if smoke:
            return [(C3J0, "def", "G%d" % a["K"][1], "42", 1)]
        rows = _grid_rows(grid_ws(k))
        (s1, r1), (s2, r2) = D_REPS
        return rows + [(C3J0, "def", "D12", str(s1), r1), (C3J0, "def", "D%d" % a["K"][1], str(s2), r2)]
    raise ValueError(name)


def reprobe_slots(pair):
    """The two re-probe rows (section 3.3): the same seed / rep as R4 / R5, the new probe arms."""
    return [(C3J0, "def", "G%d" % pair[0], "42", 1), (C3J0, "def", "G%d" % pair[1], "42", 1)]


def expect_rows(name):
    return len(plan(name))


# ── row witnesses and the replacement rule ──
def row_problems(cell, arm, cli, srv, sha=None, sha_cand=None):
    """Plan 61's echo witnesses at the c3j0 cell: RWM_SLAW=1 and RWM_DIAG=1 on BOTH ends' [GATES], RWM_THREE_TERM off, the pool knob echoed as requested, the dump gate (RWM_RTT_DUMP=1 on a D arm, off on a G arm),
    and the one seat's binary (LD_SHA). A mismatch voids the row (replaced once by the battery); the [NETEM] witness (netem_problems) is NOT here: it aborts the launch, never voids a row."""
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
        rd = lc.gate_tok(lines, "RWM_RTT_DUMP")
        if rd != ("1" if kind == "D" else "0"):
            p.append(f"gate-rttdump-{side}={rd} (want {'1' if kind == 'D' else '0'})")
    for side, lines in (("cli", cli), ("srv", srv)):
        tok, info = pp.pool_echo56(lines)
        if tok != str(w):
            p.append(f"gate-pool-{side}={tok} (want {w})")
        if side == "cli" and info != w:
            p.append(f"pool_per_path-{side}={info} (want {w})")
        if side == "srv" and info is not None and info != w:
            p.append(f"pool_per_path-{side}={info} (want {w})")
    return p + pool54_score.sha_problems("CAND", sha, sha_cand, None)


def is_echo_void(row):
    return any(x.startswith(p61.ECHO_PREFIX + ("gate-rttdump",)) for x in row.get("problems") or [])


def replace_reason(ledger, cell, arm, seed, rep):
    """-> the reason the slot is re-run once, or None: its rows so far are exactly one and that row is VOID by an echo witness (RWM_SLAW, RWM_DIAG, the pool or dump echo), or is a floor-arm DNF (a DNF stays in
    the ledger as a datum), or (the rehearsal only) a probe-row DNF. A WRONG BINARY (bin-sha-*) is never re-run: the harness rules abort on it."""
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
    probes = set(PILOT62) | set(REPROBE_LOW) | set(REPROBE_HIGH)
    if os.environ.get("LD_PLAN") == "62r0" and arm_parse(arm)[0] == "G" and arm_parse(arm)[1] in probes and r.get("dnf") and r.get("status") != "VOID-ONDISK":
        return "pilot-dnf"
    return None


def check(name, path):
    """rc 0 iff the battery finished and the ledger holds a row for every slot of the table (a replaced slot has two rows; a re-probe's two slots are expected iff the ledger carries them)."""
    import place38_parse as pp
    rows, _ = pp.rows_of([path])
    t = open(path, errors="replace").read()
    slots = {(r["cell"], r["arm"], str(r["seed"]), str(r["rep"])) for r in rows}
    want = {(c, a, sd, str(rp)) for c, _, a, sd, rp in plan(name)}
    extra = slots - want
    if name == "62r0":
        extra -= {(c, a, sd, str(rp)) for pr in (REPROBE_LOW, REPROBE_HIGH) for c, _, a, sd, rp in reprobe_slots(pr)}
    ok = "LD-BATTERY-DONE" in t and want <= slots and not extra
    print(f"CHECK{name.upper()} done={'LD-BATTERY-DONE' in t} slots={len(slots & want)}/{len(want)} rows={len(rows)} extra={len(extra)}")
    return 0 if ok else 1


def netem_check(path):
    """ABORT-NETEM's witness (section 3.3): rc 0 iff a row's driver output carries the live `tc qdisc show` of BOTH legs before (pre) and after (post) the transfer, each reading delay 20 ms with no jitter term.
    A WRONG read (a jitter term, another delay, no netem root) is rc 1 always; a MISSING read is rc 1 only when the row completed (its driver printed a summary, so it reached the post read): a bring-up failure that never
    reached the qdisc is a void row, not a harness fault. The battery runs it after every row (the first row's pre and the last row's post are two of those reads) and stops on rc 1."""
    import l1common as lc
    text = open(path, errors="replace").read()
    got, prob = lc.netem_witness(text.splitlines())
    missing = [x for x in prob if x.endswith("no [NETEM] witness")]
    wrong = [x for x in prob if x not in missing]
    bad = bool(wrong) or (bool(missing) and '"summary"' in text)
    print("NETEM62 " + " ".join(f"{t}/{leg}={'OK' if ok else 'BAD'}" for (t, leg), (ok, _) in sorted(got.items())) + (f" PROBLEMS {'; '.join(prob)}" if prob else " OK")
          + (" (the row did not complete: a missing read is not an abort)" if prob and not bad else ""))
    return 1 if bad else 0


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
        if cmd == "netem":
            return netem_check(a[0])
        if cmd == "pilot":
            u_lo, u_hi = float(a[0]), float(a[1])
            p_lo, p_hi = (int(a[2]), int(a[3])) if len(a) >= 4 else PILOT62
            d = pilot_detail(u_lo, u_hi, p_lo, p_hi)
            if d is None:
                print("PILOT62 none: a pilot utilisation is <= 0")
                return 1
            print(f"PILOT62 W={d[0]:.1f} u_lo={u_lo:.3f} u_hi={u_hi:.3f} arms={arms(d[0])} case={d[1]} clamped={'yes' if d[2] else 'no'} probes={p_lo}/{p_hi}")
            return 0
    except ValueError as e:
        print(f"ABORT-PILOT {e}")
        return 3
    print(f"unknown command {cmd}")
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
