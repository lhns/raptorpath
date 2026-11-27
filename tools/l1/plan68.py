#!/usr/bin/env python3
"""Status 68: the hand-off gate's step 1 (docs/derivations/reg-handoff-gate.md sections 1 and 3): a 54-slot diagnostic (45 executed), three arms, REPORTED ONLY (no verdict label). Tables, the per-arm run spec, the row witnesses and
the slot check. ONE source for the shell and the scorer: the battery asks `armspec` for every row's bytes / timeout / hint / env (plan63.py's pattern), and s68_report.py imports this module.

    plan68.py plan 68 [smoke|rehearsal]   the EXECUTED table (built cells only), `cell store arm seed rep` per line (smoke = rehearsal = one row per arm)
    plan68.py plan 68r [smoke|rehearsal]  the RERUN table (36 rows, see RERUN below); smoke = rehearsal = 6 rows
    plan68.py plan 68s [smoke|rehearsal]  the SECOND RERUN table (30 rows, see RERUN-S below); smoke = rehearsal = 6 rows
    plan68.py plan 68t [smoke|rehearsal]  the THIRD RERUN table (54 rows, see RERUN-T below); smoke = rehearsal = 6 rows
    plan68.py registered                  the registered 54-slot table, UNBUILT cells included
    plan68.py count 68 [smoke|rehearsal]  the number of rows the executed table holds (the battery asserts it)
    plan68.py armspec <cell> <arm>        `BYTES TIMEOUT_S HINT ENV...` (ENV tokens are KEY=VALUE)
    plan68.py unbuilt [<plan>]            the UNBUILT cell codes of the plan, one line
    plan68.py needs-pacer                 1 iff a built cell uses RWM_PERF_RATE_MBIT or RWM_PERF_BURST (the battery then requires the pacing echo in both binaries)
    plan68.py needs-burst <plan>          1 iff a row of the table uses RWM_PERF_BURST (the battery then requires the knob in both binaries)
    plan68.py check 68 <ledger>           rc 0 iff the battery finished and the ledger holds a row for every executed slot

ARMS (registration section 1). Two binaries serve three arms: BASE = main plus the gauge commit (the BASE seat, LD_BIN_BASE), R4 and GATE = ONE binary, the hand-off-gate tree (the CAND seat, LD_BIN):
    arm letter  tree               env
    B           BASE seat          RWM_DIAG=1
    R           CAND seat          RWM_QUIC_CC=bbr_rs RWM_DIAG=1                       (the gate's arm OFF: RWM_HOFF_GATE unset, scrubbed)
    G           CAND seat          RWM_QUIC_CC=bbr_rs RWM_HOFF_GATE=1 RWM_DIAG=1
R4 is the handoff-gate binary with the gate unset, as section 1 says ("R4 and GATE share one binary"): the gate's effect is one environment arm wide, and no third build exists.

ARM NAMES encode the seat and the registration cell: <letter><cell code>, e.g. `GK2` = GATE at c2 Bulk. The ledger's `cell` column stays the topology (c2-100 / c8-100); the arm carries the hint, size and pacing.

CELLS (section 3). BUILT: K2 (c2-100 Bulk 600 MB), K8 (c8-100 Bulk 600 MB), A2 (c2-100 Auto 600 MB), LR and PC (paced, since the pacer is restored). UNBUILT (UNBUILT below):
  * LR (low rate, Realtime rho = 1, 36 datagrams/s) and PC (paced headroom, Auto, 70 % of the link) use the app-side pacer RWM_PERF_RATE_MBIT (commit 4ee1df9b, "offered-load pacing ACTIVE", restored on both trees).
    The battery checks the echo in both binaries and in every row; a binary without it would run the rows silently unpaced.
  * BU (bursty headroom, Realtime rho = 1, frames of ~10 datagrams every 33 ms) has no mechanism: the pacer is a constant-rate schedule. It stays UNBUILT.
The pacer (4ee1df9b, restored) is in both seats' binaries now, so LR and PC are built; BU stays UNBUILT.
An UNBUILT cell is printed UNBUILT by the report and is never counted as cleared.

RERUN (`68r`, after the gate's witness counters): 36 rows = the GATE arm of K2 / K8 / A2 (9), the Q arm of the same three cells (9) and LR / PC with all three arms (B, R, G: 18). Q = "gate alone": the CAND binary with
RWM_QUIC_CC unset (quinn's own BBR) and RWM_HOFF_GATE=1 (arm letter Q, cells QK2 / QK8 / QA2; it exists only on the three K cells). Same interleave and seed rules as 68: rep-major, then cell, the arm order rotated by (rep - 1 + cell index).
The BASE seat of the LR / PC rows is the pacer-carrying BASE binary (handoff-gauges-pacer).

RERUN-S (`68s`, after the token witness and the bursty source): 30 rows = the GATE arm of K2 / K8 / A2 (9), BU at B / R4 / GATE (9) and PH (paced headroom at 85 % of the link, Auto) at B / R4 / GATE plus T, the GATE twin (12).
T = the A/A twin: the same binary, tree and env as GATE under its own arm letter, so GATE-vs-T is the run-to-run spread of one arm (AA68 in the report). BU is built: the perf client's RWM_PERF_BURST=10/33 (10 datagrams every 33 ms, Realtime, rho = 1, the echo
"offered-load pacing ACTIVE (RWM_PERF_BURST=10/33)"); both seats' binaries must carry it (the BASE seat is rebuilt from main plus the pacer, the gauge commit and the burst knob). PH is a 68s cell only (PC of 68 / 68r stays at 70 %). Same interleave rules.

RERUN-T (`68t`, after the window gauges: beta_max, the U witness, ack_bind; reported only): 54 rows = the GATE arm of K2 / K8 / A2 (9; read fs_over = 0, fs_over_cs, u_over, beta_max), the BW cell of
reg-handoff-gate.md section 11.6 at three frame sizes W1 / W2 / W4 = k of 10 / 20 / 40 datagrams every 33 ms, each at B / R4 / GATE (27), and PC at 75 and 80 Mbit/s (P7 / P8) at B / R4 / GATE (18).
BW (Realtime, rho = 1): an unpaced saturating lead-in of BW_LEAD datagrams (about 5 s at the link), then BW_FRAMES frames (30 s): the perf client's RWM_PERF_BURST=<k>/33+<lead>. Its witnesses (ProbeBw share of the [QCC]
lines after the lead-in, refusals per frame) are READ68 lines, never a stop. BUDGET: 9 big rows x 215 s + 27 BW rows x 75 s + 18 paced rows x 90 s = 5580 s, plus two builds, the tools tests and slack: 7200 s (2 h);
n = 3 at every frame size fits, so no frame size is dropped. The BASE seat must carry the burst knob WITH the lead-in (a `+<lead>` token): the handoff-gauges-pacer binary of this table is rebuilt from the branch that has it.

ROWS: 6 cells x 3 arms x 3 rows (seed 42, rep 1..3) = 54, interleaved: rep-major, then cell, with the arm order rotated by (rep - 1 + cell index) so no arm owns a position."""
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

SEEDS = ("42",)
N_REP = 3                          # rows per arm per cell (seed 42, rep 1..3); test_stubrun shrinks it
SEED = "42"
ARM_LETTERS = ("B", "R", "G", "Q", "T")
ARMS_68 = ("B", "R", "G")          # the arms of the main table
K_CELLS = ("K2", "K8", "A2")
RERUN_CELLS = ("K2", "K8", "A2", "LR", "PC")
RERUN_S_CELLS = ("K2", "K8", "A2", "BU", "PH")
RERUN_T_CELLS = ("K2", "K8", "A2", "W1", "W2", "W4", "P7", "P8")
REGISTERED_CELLS = ("K2", "K8", "A2", "LR", "PC", "BU")      # plan 68's registered 54 slots; PH exists in 68s only
SEAT = {"B": "BASE", "R": "CAND", "G": "CAND", "Q": "CAND", "T": "CAND"}
ARM_NAME = {"B": "BASE", "R": "R4", "G": "GATE", "Q": "GATE-ALONE", "T": "GATE-AA"}
CHUNK_PAYLOAD = 494                # perf.rs chunk_payload_len: Realtime 508 - 14 (Bulk / Auto: 1196 - 14 = 1182)
T_E = 60                           # s of emission of a paced row
TIMEOUT_PACED = T_E + 15           # RWM_PERF_TIMEOUT_S: a paced row's DNF lands 15 s after emission ends
BYTES_BIG, TIMEOUT_BIG = 600_000_000, 200
LINK_MBIT = 100.0                  # c2-100's netem rate
LOW_DGRAM_S = 36.0                 # section 3: a call rate of ~36 datagrams/s
PACED_FRAC = 0.70                  # section 3: ~70 % of the link
BURST_DGRAMS, BURST_PERIOD_MS = 10, 33
PH_FRAC = 0.85                     # 68s: paced headroom at 85 % of the link
BW_LEAD = 126_000                  # 68t: the saturating lead-in of BW, in datagrams (494 B each: about 5 s at the 100 Mbit/s link)
BW_FRAMES = 909                    # 68t: BW's frames after the lead-in (909 x 33 ms = 30 s)
BW_SIZES = (10, 20, 40)            # 68t: datagrams per frame

# code -> (topology cell, label, hint, bytes, timeout, rate Mbit/s payload or None, rho-one witnessed (Realtime), burst token "<datagrams>/<ms>" or None); order = the registration's table
CELLS = {
    "K2": ("c2-100", "c2-100 Bulk", "bulk", BYTES_BIG, TIMEOUT_BIG, None, False, None),
    "K8": ("c8-100", "c8-100 Bulk", "bulk", BYTES_BIG, TIMEOUT_BIG, None, False, None),
    "A2": ("c2-100", "c2-100 Auto", "auto", BYTES_BIG, TIMEOUT_BIG, None, False, None),
    "LR": ("c2-100", "c2-100 low rate (Realtime rho=1, 36 dg/s)", "realtime", int(LOW_DGRAM_S * T_E * CHUNK_PAYLOAD), TIMEOUT_PACED, round(LOW_DGRAM_S * CHUNK_PAYLOAD * 8 / 1e6, 6), True, None),
    "PC": ("c2-100", "c2-100 paced (Auto, 70 % of the link)", "auto", int(PACED_FRAC * LINK_MBIT * 1e6 * T_E / 8), TIMEOUT_PACED, PACED_FRAC * LINK_MBIT, False, None),
    "BU": ("c2-100", "c2-100 bursty (Realtime rho=1, 10-dgram frames / 33 ms)", "realtime", int(T_E * 1000 / BURST_PERIOD_MS) * BURST_DGRAMS * CHUNK_PAYLOAD, TIMEOUT_PACED, None, True, "%d/%d" % (BURST_DGRAMS, BURST_PERIOD_MS)),
    "PH": ("c2-100", "c2-100 paced (Auto, 85 % of the link)", "auto", int(PH_FRAC * LINK_MBIT * 1e6 * T_E / 8), TIMEOUT_PACED, PH_FRAC * LINK_MBIT, False, None),
    "P7": ("c2-100", "c2-100 paced (Auto, 75 % of the link)", "auto", int(0.75 * LINK_MBIT * 1e6 * T_E / 8), TIMEOUT_PACED, 0.75 * LINK_MBIT, False, None),
    "P8": ("c2-100", "c2-100 paced (Auto, 80 % of the link)", "auto", int(0.80 * LINK_MBIT * 1e6 * T_E / 8), TIMEOUT_PACED, 0.80 * LINK_MBIT, False, None),
}
for _k in BW_SIZES:      # BW: frames above beta after a saturating lead-in (section 11.6): W1 / W2 / W4 = 10 / 20 / 40 datagrams per 33 ms frame
    CELLS["W%d" % (_k // 10)] = ("c2-100", "c2-100 bursty after a lead-in (Realtime rho=1, %d-dgram frames / 33 ms)" % _k, "realtime", (BW_LEAD + BW_FRAMES * _k) * CHUNK_PAYLOAD, 70, None, True,
                                 "%d/%d+%d" % (_k, BURST_PERIOD_MS, BW_LEAD))
CELL_ORDER = tuple(CELLS)
# ── the one switch an amendment edits ──
UNBUILT = ("BU",)      # of plans 68 and 68r (the first never ran BU, the second does not hold it); 68s builds it (the burst knob): see unbuilt_in
UNBUILT_WHY = {"BU": "plans 68 / 68r have no bursty-frame source (the pacer is constant-rate); the burst knob belongs to 68s"}
BUILT = tuple(c for c in REGISTERED_CELLS if c not in UNBUILT)


def unbuilt_in(name=None):
    """The UNBUILT cells of the table `name` (default: this process's plan): none in 68s, which carries the burst knob."""
    return () if (name or plan_name()) in ("68s", "68t") else UNBUILT


def built_in(name=None):
    n = name or plan_name()
    return tuple(c for c in cells_of(n) if c not in unbuilt_in(n))
BUDGET_S = {"68": 11400, "68-rehearsal": 3000, "68r": 8000, "68r-rehearsal": 3600, "68s": 7400, "68s-rehearsal": 3600, "68t": 7200, "68t-rehearsal": 3600}   # 68: 27 big rows x 215 s + 27 paced rows x 90 s + two builds (~17 min) + tools tests; the rehearsal: builds + 3 rows + tests
# 68r: 18 big rows x 215 s + 18 paced rows x 90 s + builds + tests; its rehearsal: 2 big + 4 paced rows + builds + tests
# 68s: 9 big rows x 215 s + 21 paced rows x 90 s + builds + tests; its rehearsal: 1 big + 5 paced rows + builds + tests
SMOKE_CELL = "K2"
SMOKE_68R = ("GK2", "QK2", "BLR", "GLR", "BPC", "GPC")
SMOKE_68S = ("GK2", "BBU", "GBU", "BPH", "GPH", "TPH")
SMOKE_68T = ("GK2", "BW2", "RW2", "GW2", "BP7", "GP8")
LATE_T = 5.0


def arm_name(letter, code):
    return letter + code


def arms_of(code):
    """The arm letters a cell has: Q (gate alone) exists on the K cells only, T (the GATE twin) on PH only."""
    if code in K_CELLS:
        return ("B", "R", "G", "Q")
    return ("B", "R", "G", "T") if code == "PH" else ARMS_68


def arm_parse(arm):
    """`GK2` -> ('G', 'K2'); anything else (incl. a Q arm off the K cells) -> (None, None)."""
    if arm and len(arm) == 3 and arm[0] in ARM_LETTERS and arm[1:] in CELLS and arm[0] in arms_of(arm[1:]):
        return arm[0], arm[1:]
    return None, None


def seat_of(arm):
    """`BASE` or `CAND` (the binary the row runs)."""
    letter, _ = arm_parse(arm)
    return SEAT[letter] if letter else None


def all_arms():
    return [arm_name(a, c) for c in CELL_ORDER for a in arms_of(c)]


def cells_of(name):
    """The cells the table `name` holds, in report order."""
    return {"68": REGISTERED_CELLS, "68r": RERUN_CELLS, "68s": RERUN_S_CELLS, "68t": RERUN_T_CELLS}[name]


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


def burst_token(arm):
    return _cell(arm)[7]


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
    if letter in ("R", "G", "T"):      # Q keeps quinn's own BBR
        env.append("RWM_QUIC_CC=bbr_rs")
    if letter in ("G", "Q", "T"):      # T = the A/A twin of G: the same env
        env.append("RWM_HOFF_GATE=1")
    env.append("RWM_DIAG=1")
    if CELLS[code][6]:
        env.append("RWM_C_RELIABLE=1")
    tok = rate_token(arm)
    if tok is not None:
        env.append("RWM_PERF_RATE_MBIT=" + tok)
    if burst_token(arm) is not None:
        env.append("RWM_PERF_BURST=" + burst_token(arm))
    return env


def needs_pacer():
    return any(CELLS[c][5] is not None or CELLS[c][7] is not None for c in built_in())


def needs_burst(name=None):
    """True iff a row of the table `name` (default: this process's plan) uses RWM_PERF_BURST: the battery then requires the knob in both binaries (a binary without it would run the BU rows unpaced)."""
    return any(CELLS[arm_parse(r[2])[1]][7] is not None for r in plan(name or plan_name()))


def registered():
    """All 54 slots: [(cell, store, arm, seed, rep)], rep-major, then cell, with the arm order rotated by (rep - 1 + cell index)."""
    out = []
    for rep in range(1, N_REP + 1):
        for ci, code in enumerate(REGISTERED_CELLS):
            k = (rep - 1 + ci) % len(ARMS_68)
            for letter in ARMS_68[k:] + ARMS_68[:k]:
                out.append((CELLS[code][0], "def", arm_name(letter, code), SEED, rep))
    return out


def registered_rerun():
    """The 68r table: [(cell, store, arm, seed, rep)], rep-major, then cell (K2, K8, A2, LR, PC), the arms of a cell rotated by (rep - 1 + cell index): GATE and Q on a K cell, B / R4 / GATE on LR and PC."""
    out = []
    for rep in range(1, N_REP + 1):
        for ci, code in enumerate(RERUN_CELLS):
            letters = ("G", "Q") if code in K_CELLS else ARMS_68
            k = (rep - 1 + ci) % len(letters)
            for letter in letters[k:] + letters[:k]:
                out.append((CELLS[code][0], "def", arm_name(letter, code), SEED, rep))
    return out


def registered_rerun_s():
    """The 68s table: [(cell, store, arm, seed, rep)], rep-major, then cell (K2, K8, A2, BU, PH), the arms of a cell rotated by (rep - 1 + cell index): GATE on a K cell, B / R4 / GATE on BU, B / R4 / GATE / T on PH."""
    out = []
    for rep in range(1, N_REP + 1):
        for ci, code in enumerate(RERUN_S_CELLS):
            letters = ("G",) if code in K_CELLS else arms_of(code)
            k = (rep - 1 + ci) % len(letters)
            for letter in letters[k:] + letters[:k]:
                out.append((CELLS[code][0], "def", arm_name(letter, code), SEED, rep))
    return out


def registered_rerun_t():
    """The 68t table: [(cell, store, arm, seed, rep)], rep-major, then cell (K2, K8, A2, W1, W2, W4, P7, P8), the arms of a cell rotated by (rep - 1 + cell index): GATE alone on a K cell, B / R4 / GATE elsewhere."""
    out = []
    for rep in range(1, N_REP + 1):
        for ci, code in enumerate(RERUN_T_CELLS):
            letters = ("G",) if code in K_CELLS else ARMS_68
            k = (rep - 1 + ci) % len(letters)
            for letter in letters[k:] + letters[:k]:
                out.append((CELLS[code][0], "def", arm_name(letter, code), SEED, rep))
    return out


def plan(name="68", smoke=False, rehearsal=False):
    """The EXECUTED table: the registered table without its UNBUILT cells. The smoke and the rehearsal are one row per arm on K2 (seed 42, rep 1)."""
    if name == "68r":
        if smoke or rehearsal:
            return [(CELLS[arm_parse(a)[1]][0], "def", a, SEED, 1) for a in SMOKE_68R]
        return [r for r in registered_rerun() if arm_parse(r[2])[1] in BUILT]
    if name == "68t":
        if smoke or rehearsal:
            return [(CELLS[arm_parse(a)[1]][0], "def", a, SEED, 1) for a in SMOKE_68T]
        return registered_rerun_t()
    if name == "68s":
        if smoke or rehearsal:
            return [(CELLS[arm_parse(a)[1]][0], "def", a, SEED, 1) for a in SMOKE_68S]
        return registered_rerun_s()
    if name != "68":
        raise ValueError(name)
    if smoke or rehearsal:
        return [(CELLS[SMOKE_CELL][0], "def", arm_name(a, SMOKE_CELL), SEED, 1) for a in ARMS_68]
    return [r for r in registered() if arm_parse(r[2])[1] in BUILT]


def plan_name():
    """The plan this process serves: LD_PLAN when it is 68, 68r, 68s or 68t, else 68."""
    n = os.environ.get("LD_PLAN")
    return n if n in ("68", "68r", "68s", "68t") else "68"


def expect_rows(name=None):
    return len(plan(name or plan_name()))


def row_problems(cell, arm, cli, srv, sha=None, sha_cand=None, sha_base=None, paced_echo=None):
    """Only the binary (the row's bin_sha against its seat; a comparison that cannot run is a problem) and, on a paced cell, the pacing echo. The gate's execution, the cc echo and every readout are READINGS:
    a failing GATE row is a finding printed as STOP68, never a void."""
    import pool54_score
    if not valid_slot(cell, arm):
        return [f"unknown-arm-{arm}-at-{cell}"]
    p = pool54_score.sha_problems(seat_of(arm), sha, sha_cand, sha_base)
    lines = list(cli or []) + list(srv or [])
    if (_cell(arm)[5] is not None or _cell(arm)[7] is not None) and not any("offered-load pacing ACTIVE" in ln for ln in lines):
        p.append("pace-echo-missing")
    if _cell(arm)[7] is not None and not any("RWM_PERF_BURST=" + _cell(arm)[7] in ln for ln in lines):
        p.append("burst-echo-missing")
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
            print(" ".join(unbuilt_in(a[0] if a else None)))
            return 0
        if cmd == "needs-pacer":
            print(1 if needs_pacer() else 0)
            return 0
        if cmd == "needs-burst":
            print(1 if needs_burst(a[0] if a else None) else 0)
            return 0
        if cmd == "armspec":
            cell, arm = a[0], a[1]
            if not valid_slot(cell, arm) or arm_parse(arm)[1] not in built_in():
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
