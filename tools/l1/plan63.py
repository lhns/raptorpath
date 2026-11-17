#!/usr/bin/env python3
"""Status 63: F2's first VM contact (docs: the §63 draft registration). Ten rows on two seats: PL and PM at seeds 42 and 7, U at seed 42, each run on the CONTROL seat (the e-tree binary, the one that failed E's
warm-up) and then on the CAND seat (e-tree + the F2 engine commits), on c2-100 at the Realtime hint with RWM_C_RELIABLE=0. The arm definitions (bytes, pace, timeout, env) are plan 51e's own (plan51e.py); this module only
names the seat in the arm (`C<arm>` = CONTROL, `F<arm>` = CAND) and orders the rows. Tables, per-row run spec, the reported fields and the slot check; the shell asks `armspec`, the row parser asks `row_problems` /
`row_metrics`, and `report` / `smoke` read the ledger.

    plan63.py plan 63 [smoke]            the table, `cell store arm seed rep` per line (smoke = ONE CAND PL s42 row, which is also the rehearsal's row)
    plan63.py count 63 [smoke]           the number of rows the table holds (the battery asserts it)
    plan63.py armspec <cell> <arm>       `BYTES TIMEOUT_S HINT ENV...` (plan51e's spec for the arm without its seat prefix)
    plan63.py check 63 <ledger>          rc 0 iff the battery finished and the ledger holds a row for every slot
    plan63.py smoke <smoke|rehearsal> <ledger>    the smoke / rehearsal check: harness criteria gate (SMOKE-PASS | SMOKE-FAIL), the CAND row's reading is REPORTED
    plan63.py report <ledger>            per row: warm-up outcome, undeliv, [UNFRAMABLE] count, [SYMSZ] from both ends, the seat's bin_sha, and the seat's PASS / FAIL against the draft's predictions

Not a void: F2 is expected to fail on CONTROL, so undeliv / [UNFRAMABLE] are READINGS here (l1common.f2_counts), not a VOID-F2. The scored blocks (51, 51e) void on them (l1common.f2_void)."""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import plan51e  # noqa: E402

CELL = plan51e.CELL
CONTROL_SHA = "8013a3123aed0f4177a05efb15e4cf96117341bfaae743654f5221b434fc4caf"      # §51 Amendment 1's registered E bin_sha: the e-tree binary (LD_BIN_SHA_CONTROL overrides it, a test seam)
BASES = ("PL", "PM", "U")
PREFIX = {"CONTROL": "C", "CAND": "F"}
ROWS = (("PL", "42"), ("PM", "42"), ("U", "42"), ("PL", "7"), ("PM", "7"))                 # (arm, seed); each runs CONTROL then CAND
SMOKE = [(CELL, "def", "FPL", "42", 1)]
BUDGET_S = {"63": 1920, "63r": 1500}                                                       # 10 rows x (90 s paced / 45 s U) = 810 + 2 builds 720 + smoke 90 + tests 300 (= 1 920); the rehearsal 25 min. LD_HARD = launch + 1 h
WARM_TOKEN = "warm-up object never acked"
LATE_T = plan51e.LATE_T


def arm_parse(arm):
    """`FPL` -> ('CAND', 'PL'); `CU` -> ('CONTROL', 'U'); anything else -> (None, None)."""
    for seat, p in PREFIX.items():
        if arm and arm.startswith(p) and arm[1:] in BASES:
            return seat, arm[1:]
    return None, None


def seat_arm(seat, base):
    return PREFIX[seat] + base


def all_arms():
    return [seat_arm(s, b) for s in PREFIX for b in BASES]


def valid_slot(cell, arm):
    return cell == CELL and arm_parse(arm)[0] is not None


def arm_bytes(cell, arm):
    return plan51e.arm_bytes(cell, arm_parse(arm)[1])


def arm_timeout(cell, arm):
    return plan51e.arm_timeout(cell, arm_parse(arm)[1])


def arm_hint(arm):
    return "realtime"


def arm_env(cell, arm):
    return plan51e.arm_env(cell, arm_parse(arm)[1])


def plan(name="63", smoke=False, rehearsal=False):
    """[(cell, store, arm, seed, rep)] in run order: for each (arm, seed) of ROWS the CONTROL row, then the CAND row, rep = the position in ROWS. The smoke and the rehearsal are the one CAND PL s42 row."""
    if name != "63":
        raise ValueError(name)
    if smoke or rehearsal:
        return list(SMOKE)
    out = []
    for k, (b, sd) in enumerate(ROWS, 1):
        out += [(CELL, "def", seat_arm(s, b), sd, k) for s in ("CONTROL", "CAND")]
    return out


def expect_rows(name="63"):
    return len(plan(name))


# ── the row parser's side ──
def row_problems(cell, arm, cli, srv, sha=None, sha_cand=None, sha_base=None):
    """Only the binary: the row's bin_sha against its seat (pool54_score.sha_problems; a comparison that cannot run is a problem). Nothing else voids a row: a failing CONTROL row is the expected datum."""
    import pool54_score
    if not valid_slot(cell, arm):
        return [f"unknown-arm-{arm}-at-{cell}"]
    return pool54_score.sha_problems("BASE" if arm_parse(arm)[0] == "CONTROL" else "CAND", sha, sha_cand, sha_base)


def row_metrics(cli, srv, drv):
    """The `t63` of one row: the warm-up outcome, the F2 counters and the [SYMSZ] echoes from both ends. Never raises on log content."""
    import l1common as lc
    cli, srv, drv = list(cli or []), list(srv or []), list(drv or [])
    allv = cli + srv + drv
    out = {"v": 1, "warm": "OTHER", "undeliv": 0, "unframable": 0, "symsz": {"cli": [], "srv": []}, "rate_echo": None, "summary": False, "dnf": False}
    try:
        und, unf = lc.f2_counts(allv)
        out["undeliv"], out["unframable"] = und, unf
        out["symsz"] = {"cli": lc.symsz(cli), "srv": lc.symsz(srv)}
        out["rate_echo"] = lc.gate_tok(cli, "RWM_PERF_RATE_MBIT")
        never = any(WARM_TOKEN in ln for ln in allv)
        summ, dnf = None, False
        for ln in cli + drv:      # the driver's JSON lines: a summary carries `"dnf":0` on a completed run and `"dnf":1` on a DNF; a per-run DNF line has `"dnf":true`
            if ln.lstrip().startswith("{"):
                try:
                    d = json.loads(ln)
                except ValueError:
                    continue
                if isinstance(d, dict):
                    if d.get("summary"):
                        summ = d
                    dnf = dnf or bool(d.get("dnf"))
        out["summary"] = summ is not None and not summ.get("dnf")
        out["dnf"] = dnf
        out["warm"] = "NEVER-ACKED" if never else ("COMPLETED" if out["summary"] else ("TRANSFER-DNF" if out["dnf"] else "OTHER"))
    except Exception as e:      # a reported readout never voids a row
        out["error"] = f"{type(e).__name__}: {e}"
    return out


# ── the reported labels against the draft's predictions ──
def cand_label(t):
    """CAND: PASS iff the warm-up acked and the transfer completed, undeliv 0, no [UNFRAMABLE], and [SYMSZ] present on both ends with local = decoder everywhere. -> (label, why list)."""
    why = []
    if t["warm"] != "COMPLETED":
        why.append(f"warm-up/transfer {t['warm']}")
    if t["undeliv"]:
        why.append(f"undeliv={t['undeliv']}")
    if t["unframable"]:
        why.append(f"unframable={t['unframable']}")
    sz = t["symsz"]
    if not sz["cli"] or not sz["srv"]:
        why.append("[SYMSZ] missing on " + ("both ends" if not sz["cli"] and not sz["srv"] else ("the client" if not sz["cli"] else "the server")))
    elif any(a != b for a, b in sz["cli"] + sz["srv"]):
        why.append("[SYMSZ] local != decoder")
    return ("PASS" if not why else "FAIL"), why


def control_label(arm_base, seed, t):
    """CONTROL: s42 PL / PM are predicted to FAIL the warm-up (never acked); U s42 is predicted to pass; s7 has no prediction (the committed base rate is 1 of 6 Realtime rows). -> (label, note)."""
    if seed == "7":
        return "NO-PREDICTION", f"warm-up {t['warm']}"
    if arm_base in ("PL", "PM"):
        if t["warm"] == "NEVER-ACKED":
            return "PASS", "reproduced: the warm-up never acked"
        return "FAIL", f"not reproduced ({t['warm']}): the inferred trigger is not deterministic under seed 42; the candidate's reading is weakened, not voided"
    if t["warm"] == "COMPLETED":
        return "PASS", "the unpaced control completed"
    return "FAIL", f"U s42 did not complete ({t['warm']}): the pacer is not part of the trigger"


def label_row(arm, seed, t):
    seat, base = arm_parse(arm)
    if seat == "CAND":
        lab, why = cand_label(t)
        return lab, "; ".join(why) if why else "warm-up acked, completed, undeliv 0, no [UNFRAMABLE], [SYMSZ] local = decoder"
    return control_label(base, str(seed), t)


def _rows(path):
    import place38_parse as pp
    import pool56_score as ps
    rows, tokens = pp.rows_of([path])
    cnt, replaced = ps.counting([r for r in rows if r.get("cell") == CELL and valid_slot(r.get("cell"), r.get("arm"))])
    return cnt, tokens


def _fmt_sz(v):
    return ",".join(f"{a}/{b}" for a, b in v) or "none"


def report(path, out=print):
    """One ROW63 line per row and one LABEL63 line per row, then the per-seat bin_sha and VERDICT63. All reported: nothing here gates a launch."""
    cnt, _ = _rows(path)
    out(f"S63 REPORT rows={len(cnt)} (F2 first contact; reported only, no void: CONTROL is expected to fail)")
    shas = {"CONTROL": set(), "CAND": set()}
    fails = {"CONTROL": 0, "CAND": 0}
    for r in cnt:
        seat, base = arm_parse(r["arm"])
        t = r.get("t63") or {}
        if not t:
            out(f"ROW63 {r['arm']} s{r['seed']} r{r['rep']}: no t63 ({r.get('t63_error')})")
            continue
        shas[seat].add(r.get("bin_sha") or "-")
        lab, note = label_row(r["arm"], r["seed"], t)
        fails[seat] += lab == "FAIL"
        out(f"ROW63 {r['arm']} s{r['seed']} r{r['rep']}: status={r.get('status')} warm={t['warm']} dnf={int(bool(r.get('dnf')))} undeliv={t['undeliv']} unframable={t['unframable']} "
            f"symsz(cli)={_fmt_sz(t['symsz']['cli'])} symsz(srv)={_fmt_sz(t['symsz']['srv'])} rate_echo={t['rate_echo']} bin_sha={str(r.get('bin_sha'))[:16]}")
        out(f"LABEL63 {r['arm']}-s{r['seed']} {lab} ({note})")
    want = {"CONTROL": os.environ.get("LD_BIN_SHA_CONTROL") or CONTROL_SHA, "CAND": os.environ.get("LD_BIN_SHA_CAND") or None}      # run_all exports both pins
    for seat in ("CONTROL", "CAND"):
        v = sorted(shas[seat])
        ok = bool(v) and all(x == want[seat] for x in v) if want[seat] else None
        out(f"SEAT63 {seat} bin_sha {','.join(x[:16] for x in v) or '-'} " + ("(equals the pinned value)" if ok else "(NOT the pinned value)" if ok is False else "(no pin in this environment: recorded)"))
    blocked = fails["CAND"] > 0
    out(f"VERDICT63 REPORTED-ONLY rows={len(cnt)}/{expect_rows()} control_fail={fails['CONTROL']} cand_fail={fails['CAND']}" + (" BLOCKS-E-PRE (a failed candidate row is a finding against the fix)" if blocked else ""))
    return 0


def smoke(kind, path, out=print):
    """The smoke / rehearsal: the one CAND PL s42 row must have run with its binary right and its t63 computed (these gate: SMOKE-PASS | SMOKE-FAIL). Its reading against the draft's CAND prediction is REPORTED."""
    cnt, _ = _rows(path)
    got = {(r["arm"], str(r["seed"]), str(r["rep"])): r for r in cnt}
    bad = []

    def crit(name, fails):
        out(("PASS " if not fails else "FAIL ") + name + ("" if not fails else ": " + "; ".join(fails)))
        bad.extend(fails)
    out(f"SMOKE63 kind={kind} rows={len(cnt)} slots={len(SMOKE)}")
    miss = [f"{a} s{s} r{r}: no row" for (_, _, a, s, r) in SMOKE if (a, s, str(r)) not in got]
    crit("the CAND PL s42 row ran", miss)
    for (_, _, a, s, r) in SMOKE:
        row = got.get((a, s, str(r)))
        if row is None:
            continue
        t = row.get("t63") or {}
        crit("the row carries its t63 readouts", [] if t and not row.get("t63_error") else [f"{a}: no t63 ({row.get('t63_error')})"])
        crit("the row ran its seat's binary (no bin-sha problem)", [x for x in row.get("problems") or [] if x.startswith("bin-sha")])
        if t:
            lab, note = label_row(a, s, t)
            out(f"  REPORTED CAND-PL-s42 {lab}: {note}; warm={t['warm']} undeliv={t['undeliv']} unframable={t['unframable']} symsz(cli)={_fmt_sz(t['symsz']['cli'])} symsz(srv)={_fmt_sz(t['symsz']['srv'])}")
    if bad:
        out("SMOKE-FAIL " + "; ".join(bad))
        return 1
    out("SMOKE-PASS")
    return 0


def check(name, path):
    """rc 0 iff the battery finished and the ledger holds a row for every slot of the table."""
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
        if cmd == "check":
            return check(a[0], a[1])
        if cmd == "smoke":
            return smoke(a[0], a[1])
        if cmd == "report":
            return report(a[0])
    except ValueError as e:
        print(f"ABORT-PLAN63 {e}")
        return 3
    print(f"unknown command {cmd}")
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
