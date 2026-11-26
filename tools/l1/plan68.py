#!/usr/bin/env python3
"""Status 68: the hand-off gate's step 1 (docs/derivations/reg-handoff-gate.md sections 1 and 3): a 54-row diagnostic, three arms, REPORTED ONLY (no verdict label). Tables, the per-arm run spec, the row witnesses and
the slot check. ONE source for the shell and the scorer: the battery asks `armspec` for every row's bytes / timeout / hint / env (plan63.py's pattern), and s68_report.py imports this module.

    plan68.py plan 68 [smoke|rehearsal]   the EXECUTED table (built cells only), `cell store arm seed rep` per line (smoke = rehearsal = one row per arm)
    plan68.py registered                  the registered 54-slot table, UNBUILT cells included
    plan68.py count 68 [smoke|rehearsal]  the number of rows the executed table holds (the battery asserts it)
    plan68.py armspec <cell> <arm>        `BYTES TIMEOUT_S HINT ENV...` (ENV tokens are KEY=VALUE)
    plan68.py unbuilt                     the UNBUILT cell codes, one line
    plan68.py needs-pacer                 1 iff a built cell uses RWM_PERF_RATE_MBIT (the battery then requires the pacing echo in both binaries)
    plan68.py check 68 <ledger>           rc 0 iff the battery finished and the ledger holds a row for every executed slot

ARMS (registration section 1). Two binaries serve three arms: BASE = main plus the gauge commit (the BASE seat, LD_BIN_BASE), R4 and GATE = ONE binary, the hand-off-gate tree (the CAND seat, LD_BIN):
    arm letter  tree               env
    B           BASE seat          RWM_DIAG=1
    R           CAND seat          RWM_QUIC_CC=bbr_rs RWM_DIAG=1                       (the gate's arm OFF: RWM_HOFF_GATE unset, scrubbed)
    G           CAND seat          RWM_QUIC_CC=bbr_rs RWM_HOFF_GATE=1 RWM_DIAG=1
R4 is the handoff-gate binary with the gate unset, as section 1 says ("R4 and GATE share one binary"): the gate's effect is one environment arm wide, and no third build exists.

ARM NAMES encode the seat and the registration cell: <letter><cell code>, e.g. `GK2` = GATE at c2 Bulk. The ledger's `cell` column stays the topology (c2-100 / c8-100); the arm carries the hint, size and pacing.

CELLS (section 3). BUILT: K2 (c2-100 Bulk 600 MB), K8 (c8-100 Bulk 600 MB), A2 (c2-100 Auto 600 MB). UNBUILT (UNBUILT below; the registration allows it when the harness has no source pacing):
  * LR (low rate, Realtime rho = 1, 36 datagrams/s) and PC (paced headroom, Auto, 70 % of the link) need the app-side pacer RWM_PERF_RATE_MBIT (commit 4ee1df9b, "offered-load pacing ACTIVE"). Neither the hand-off-gate tree
    nor main carries it: `git grep PERF_RATE` is empty on both, so the env var would give silently UNPACED rows. Both are fully specified here (rate, bytes, timeout) and flip to built by deleting them from
    UNBUILT once both binaries carry the pacer (the battery then checks the echo in both binaries and in every row).
  * BU (bursty headroom, Realtime rho = 1, frames of ~10 datagrams every 33 ms) has no mechanism: the pacer is a constant-rate schedule. It stays UNBUILT.
An UNBUILT cell is printed UNBUILT by the report and is never counted as cleared.

ROWS: 6 cells x 3 arms x 3 rows (seed 42, rep 1..3) = 54, interleaved: rep-major, then cell, with the arm order rotated by (rep - 1 + cell index) so no arm owns a position."""
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

SEEDS = ("42",)
N_REP = 3                          # rows per arm per cell (seed 42, rep 1..3); test_stubrun shrinks it
SEED = "42"
ARM_LETTERS = ("B", "R", "G")
SEAT = {"B": "BASE", "R": "CAND", "G": "CAND"}
ARM_NAME = {"B": "BASE", "R": "R4", "G": "GATE"}
CHUNK_PAYLOAD = 494                # perf.rs chunk_payload_len: Realtime 508 - 14 (Bulk / Auto: 1196 - 14 = 1182)
T_E = 60                           # s of emission of a paced row
TIMEOUT_PACED = T_E + 15           # RWM_PERF_TIMEOUT_S: a paced row's DNF lands 15 s after emission ends
BYTES_BIG, TIMEOUT_BIG = 600_000_000, 200
LINK_MBIT = 100.0                  # c2-100's netem rate
LOW_DGRAM_S = 36.0                 # section 3: a call rate of ~36 datagrams/s
PACED_FRAC = 0.70                  # section 3: ~70 % of the link
BURST_DGRAMS, BURST_PERIOD_MS = 10, 33

# code -> (topology cell, label, hint, bytes, timeout, rate Mbit/s payload or None, rho-one witnessed (Realtime)); order = the registration's table
CELLS = {
    "K2": ("c2-100", "c2-100 Bulk", "bulk", BYTES_BIG, TIMEOUT_BIG, None, False),
    "K8": ("c8-100", "c8-100 Bulk", "bulk", BYTES_BIG, TIMEOUT_BIG, None, False),
    "A2": ("c2-100", "c2-100 Auto", "auto", BYTES_BIG, TIMEOUT_BIG, None, False),
    "LR": ("c2-100", "c2-100 low rate (Realtime rho=1, 36 dg/s)", "realtime", int(LOW_DGRAM_S * T_E * CHUNK_PAYLOAD), TIMEOUT_PACED, round(LOW_DGRAM_S * CHUNK_PAYLOAD * 8 / 1e6, 6), True),
    "PC": ("c2-100", "c2-100 paced (Auto, 70 % of the link)", "auto", int(PACED_FRAC * LINK_MBIT * 1e6 * T_E / 8), TIMEOUT_PACED, PACED_FRAC * LINK_MBIT, False),
    "BU": ("c2-100", "c2-100 bursty (Realtime rho=1, 10-dgram frames / 33 ms)", "realtime", None, TIMEOUT_PACED, None, True),
}
CELL_ORDER = tuple(CELLS)
# ── the one switch an amendment edits ──
UNBUILT = ("LR", "PC", "BU")
UNBUILT_WHY = {"LR": "needs RWM_PERF_RATE_MBIT: the pacer is in neither tree", "PC": "needs RWM_PERF_RATE_MBIT: the pacer is in neither tree", "BU": "no bursty-frame source exists (the pacer is constant-rate)"}
BUILT = tuple(c for c in CELL_ORDER if c not in UNBUILT)
BUDGET_S = {"68": 11400, "68r": 3000}      # worst case: 27 big rows x 215 s + 27 paced rows x 90 s + two builds (~17 min) + tools tests; the rehearsal: builds + 3 rows + tests
SMOKE_CELL = "K2"
LATE_T = 5.0


def arm_name(letter, code):
    return letter + code


def arm_parse(arm):
    """`GK2` -> ('G', 'K2'); anything else -> (None, None)."""
    if arm and len(arm) == 3 and arm[0] in ARM_LETTERS and arm[1:] in CELLS:
        return arm[0], arm[1:]
    return None, None


def seat_of(arm):
    """`BASE` or `CAND` (the binary the row runs)."""
    letter, _ = arm_parse(arm)
    return SEAT[letter] if letter else None


def all_arms():
    return [arm_name(a, c) for c in CELL_ORDER for a in ARM_LETTERS]


def valid_slot(cell, arm):
    letter, code = arm_parse(arm)
    return letter is not None and CELLS[code][0] == cell


def _cell(arm):
    return CELLS[arm_parse(arm)[1]]


def arm_bytes(cell, arm):
    b = _cell(arm)[3]
    if b is None:
        raise ValueError(f"{arm}: no source (UNBUILT)")
    return b


def arm_timeout(cell, arm):
    return _cell(arm)[4]


def arm_hint(arm):
    return _cell(arm)[2]


def rate_token(arm):
    r = _cell(arm)[5]
    return None if r is None else ("%.6f" % r).rstrip("0").rstrip(".")


def arm_env(cell, arm):
    """[KEY=VALUE]: the arm's tree-independent env. RWM_DIAG=1 on every row (so [HOFF] prints); RWM_QUIC_CC and RWM_HOFF_GATE are scrubbed by the battery before it, so R4 never inherits the gate. A Realtime cell sets
    RWM_C_RELIABLE=1 (rho = 1; the witness is `window_reliable: true` on both ends); a paced cell sets RWM_PERF_RATE_MBIT."""
    letter, code = arm_parse(arm)
    if letter is None:
        raise ValueError(f"unknown arm {arm}")
    env = []
    if letter in ("R", "G"):
        env.append("RWM_QUIC_CC=bbr_rs")
    if letter == "G":
        env.append("RWM_HOFF_GATE=1")
    env.append("RWM_DIAG=1")
    if CELLS[code][6]:
        env.append("RWM_C_RELIABLE=1")
    tok = rate_token(arm)
    if tok is not None:
        env.append("RWM_PERF_RATE_MBIT=" + tok)
    return env


def needs_pacer():
    return any(CELLS[c][5] is not None for c in BUILT)


def registered():
    """All 54 slots: [(cell, store, arm, seed, rep)], rep-major, then cell, with the arm order rotated by (rep - 1 + cell index)."""
    out = []
    for rep in range(1, N_REP + 1):
        for ci, code in enumerate(CELL_ORDER):
            k = (rep - 1 + ci) % len(ARM_LETTERS)
            for letter in ARM_LETTERS[k:] + ARM_LETTERS[:k]:
                out.append((CELLS[code][0], "def", arm_name(letter, code), SEED, rep))
    return out


def plan(name="68", smoke=False, rehearsal=False):
    """The EXECUTED table: the registered table without its UNBUILT cells. The smoke and the rehearsal are one row per arm on K2 (seed 42, rep 1)."""
    if name != "68":
        raise ValueError(name)
    if smoke or rehearsal:
        return [(CELLS[SMOKE_CELL][0], "def", arm_name(a, SMOKE_CELL), SEED, 1) for a in ARM_LETTERS]
    return [r for r in registered() if arm_parse(r[2])[1] in BUILT]


def expect_rows(name="68"):
    return len(plan(name))


def row_problems(cell, arm, cli, srv, sha=None, sha_cand=None, sha_base=None, paced_echo=None):
    """Only the binary (the row's bin_sha against its seat; a comparison that cannot run is a problem) and, on a paced cell, the pacing echo. The gate's execution, the cc echo and every readout are READINGS:
    a failing GATE row is a finding printed as STOP68, never a void."""
    import pool54_score
    if not valid_slot(cell, arm):
        return [f"unknown-arm-{arm}-at-{cell}"]
    p = pool54_score.sha_problems(seat_of(arm), sha, sha_cand, sha_base)
    if _cell(arm)[5] is not None and not any("offered-load pacing ACTIVE" in ln for ln in list(cli or []) + list(srv or [])):
        p.append("pace-echo-missing")
    return p


def check(name, path):
    """rc 0 iff the battery finished and the ledger holds a row for every executed slot."""
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
        if cmd in ("plan", "count"):
            mode = a[1] if len(a) > 1 else ""
            rows = plan(a[0], smoke=(mode == "smoke"), rehearsal=(mode == "rehearsal"))
            if cmd == "count":
                print(len(rows))
            else:
                for c, s, ar, sd, rp in rows:
                    print(f"{c} {s} {ar} {sd} {rp}")
            return 0
        if cmd == "registered":
            for c, s, ar, sd, rp in registered():
                print(f"{c} {s} {ar} {sd} {rp}")
            return 0
        if cmd == "unbuilt":
            print(" ".join(UNBUILT))
            return 0
        if cmd == "needs-pacer":
            print(1 if needs_pacer() else 0)
            return 0
        if cmd == "armspec":
            cell, arm = a[0], a[1]
            if not valid_slot(cell, arm) or arm_parse(arm)[1] not in BUILT:
                print(f"UNKNOWN-ARM {arm} at {cell}")
                return 1
            print(arm_bytes(cell, arm), arm_timeout(cell, arm), arm_hint(arm), *arm_env(cell, arm))
            return 0
        if cmd == "check":
            return check(a[0], a[1])
    except ValueError as e:
        print(f"ABORT-PLAN68 {e}")
        return 3
    print(f"unknown command {cmd}")
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
