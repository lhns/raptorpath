#!/usr/bin/env python3
"""Status 63: F2's first VM contact (docs: the §63 draft registration). Ten rows on two seats: PL and PM at seeds 42 and 7, U at seed 42, each run on the CONTROL seat (the e-tree binary, the one that failed E's
warm-up) and then on the CAND seat (e-tree + the F2 engine commits), on c2-100 at the Realtime hint with RWM_C_RELIABLE=0. The arm definitions (bytes, pace, timeout, env) are plan 51e's own (plan51e.py); this module only
names the seat in the arm (`C<arm>` = CONTROL, `F<arm>` = CAND) and orders the rows. Tables, per-row run spec, the reported fields and the slot check; the shell asks `armspec`, the row parser asks `row_problems` /
`row_metrics`, and `report` / `smoke` read the ledger.

    plan63.py plan 63 [smoke|rehearsal]  the table, `cell store arm seed rep` per line (smoke = ONE CAND PL s42 row; rehearsal = CONTROL PL s42 then CAND PL s42, both reported, never re-run)
    plan63.py count 63 [smoke|rehearsal] the number of rows the table holds (the battery asserts it)
    plan63.py armspec <cell> <arm>       `BYTES TIMEOUT_S HINT ENV...` (plan51e's spec for the arm without its seat prefix)
    plan63.py check 63 <ledger>          rc 0 iff the battery finished and the ledger holds a row for every slot
    plan63.py smoke <smoke|rehearsal> <ledger>    the smoke / rehearsal check: harness criteria gate (SMOKE-PASS | SMOKE-FAIL), the rows' readings are REPORTED
    plan63.py report <ledger> [<raw diag-ld dir>]    per row (the optional dir recomputes each row's t63 from its raw logs, for a ledger older than a field): warm-up outcome, [CTLD] undeliv (CAND only: CONTROL has no counter), [UNFRAMABLE] count, [SYMSZ] from both ends, the transfer outcome (separately), the seat's bin_sha, the CAND's denominator (the source symbols the server received: the last [RFA] src_n, and the last [CTLD] rx= / frames), the CAND's
                                         PASS / FAIL on the F2 reading, and CONTROL's warm-up failures out of its rows (reported, no per-row prediction)

Not a void: CONTROL may fail the warm-up (the datum), so the F2 counters are READINGS here (l1common.f2_counts), not a VOID-F2. The scored blocks (51, 51e) void on them (l1common.f2_void).
There is no pacer correlation (§63): every row's warm-up is the same unpaced Realtime warm-up, so CONTROL has NO per-row prediction; its warm-up failures out of 5 are reported. The CAND's F2 reading is the warm-up ack plus
the witnesses (undeliv=0 from a PRESENT [CTLD] line, no [UNFRAMABLE], [SYMSZ] local = decoder); transfer completion is reported separately."""
import json
import os
import re
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
REHEARSAL = [(CELL, "def", "CPL", "42", 1), (CELL, "def", "FPL", "42", 1)]               # section 63: LD_REHEARSE runs CONTROL PL s42 then CAND PL s42, both reported; a rehearsal row is never re-run
BUDGET_S = {"63": 5400, "63r": 1800}                                                       # section 63: two builds (~8 min each) + the tools tests (~8 min) + 10 rows of at most 150 s = 1.5 h worst case; the rehearsal ~30 min. LD_HARD = launch + 1.5 h
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
    """[(cell, store, arm, seed, rep)] in run order: for each (arm, seed) of ROWS the CONTROL row, then the CAND row, rep = the position in ROWS. The smoke is the one CAND PL s42 row, the rehearsal CONTROL PL s42 then CAND PL s42."""
    if name != "63":
        raise ValueError(name)
    if rehearsal:
        return list(REHEARSAL)
    if smoke:
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
    """The `t63` of one row: the warm-up outcome (NEVER-ACKED when the engine's `warm-up object never acked` is anywhere in the logs, NO-LOG when the client log is empty, else ACKED), the transfer outcome SEPARATELY
    (COMPLETED / DNF / OTHER), the F2 counters ([CTLD] undeliv from the PRESENT lines of the RECEIVING end (the perf server): `ctld` counts that end's lines that carry an `undeliv=` value, so a missing line is a missing witness, not a zero; the sending end's are reported beside) and the [SYMSZ]
    echoes from both ends. Never raises on log content."""
    import l1common as lc
    cli, srv, drv = list(cli or []), list(srv or []), list(drv or [])
    allv = cli + srv + drv
    out = {"v": 1, "warm": "NO-LOG", "transfer": "OTHER", "undeliv": 0, "ctld": 0, "unframable": 0, "symsz": {"cli": [], "srv": []}, "rate_echo": None, "summary": False, "dnf": False}
    try:
        _, unf = lc.f2_counts(allv)
        # The perf SERVER receives the bulk transfer (perf_rwm_c.sh: the --client is the sender), so the F2 witness is the server's [CTLD] undeliv= (`ctld` = how many such lines it printed). The client's
        # [CTLD] lines are the sending end's: reported (`ctld_cli`), never a substitute for the receiver's. `undeliv` is the SUM of both ends' largest undeliv= (the reported field).
        def ends(lines):
            return sum(1 for ln in lines if "[CTLD]" in ln and lc.inum(lc.field(lc.strip(ln), "undeliv")) is not None), lc.f2_counts(lines)[0]
        out["ctld"], out["undeliv_srv"] = ends(srv)
        out["ctld_cli"], out["undeliv_cli"] = ends(cli)
        # The denominator of `undeliv=0`: what the receiving server took in. src_n = the source symbols the server counted (the last [RFA] line, its final=1 flush), srv_rx / srv_fr = the datagrams and frames its
        # last [CTLD] line counted (rx= / dgrx0[fr=...]). None when the server printed no such line.
        def last(lines, tag, key):
            v = None
            for ln in lines:
                if tag in ln:
                    x = lc.inum(lc.field(lc.strip(ln), key))
                    v = x if x is not None else v
            return v
        out["src_n"] = last(srv, "[RFA]", "src_n")
        out["srv_rx"] = last(srv, "[CTLD]", "rx")
        _fr = [m for m in (re.search(r"dgrx0\[fr=(\d+)", lc.strip(ln)) for ln in srv if "[CTLD]" in ln) if m]
        out["srv_fr"] = int(_fr[-1].group(1)) if _fr else None
        out["undeliv"], out["unframable"] = out["undeliv_srv"] + out["undeliv_cli"], unf
        out["symsz"] = {"cli": lc.symsz(cli), "srv": lc.symsz(srv)}
        out["rate_echo"] = lc.gate_tok(cli, "RWM_PERF_RATE_MBIT")
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
        never = any(WARM_TOKEN in ln for ln in allv)
        out["warm"] = "NEVER-ACKED" if never else ("ACKED" if cli else "NO-LOG")
        out["transfer"] = "COMPLETED" if out["summary"] else ("DNF" if dnf else "OTHER")
    except Exception as e:      # a reported readout never voids a row
        out["error"] = f"{type(e).__name__}: {e}"
    return out


# ── the reported labels (§63) ──
def cand_label(t):
    """CAND: the F2 reading. PASS iff the warm-up acked, `undeliv=0` was read from a PRESENT [CTLD] line OF THE RECEIVING END, the perf server (a missing line is a missing witness, not a pass, and a [CTLD] line on the sending end does not stand in for it), there is no [UNFRAMABLE] line, and [SYMSZ] is present on both
    ends with local = decoder everywhere. Transfer completion is NOT part of it (reported separately). -> (label, why list)."""
    why = []
    if t["warm"] != "ACKED":
        why.append(f"warm-up {t['warm']}")
    if not t.get("ctld"):
        why.append("no [CTLD] line with undeliv= on the receiving end (the perf server): missing witness" + (" (the sending end printed one)" if t.get("ctld_cli") else ""))
    elif t["undeliv"]:
        why.append(f"undeliv={t['undeliv']}")
    if t["unframable"]:
        why.append(f"unframable={t['unframable']}")
    sz = t["symsz"]
    if not sz["cli"] or not sz["srv"]:
        why.append("[SYMSZ] missing on " + ("both ends" if not sz["cli"] and not sz["srv"] else ("the client" if not sz["cli"] else "the server")))
    elif any(a != b for a, b in sz["cli"] + sz["srv"]):
        why.append("[SYMSZ] local != decoder")
    return ("PASS" if not why else "FAIL"), why


def label_row(arm, seed, t):
    """CAND rows are PASS / FAIL on the F2 reading; CONTROL rows have NO per-row prediction (the warm-up failure is a stochastic datum under one netem seed): they are REPORTED with their outcome."""
    seat, base = arm_parse(arm)
    if seat == "CAND":
        lab, why = cand_label(t)
        return lab, "; ".join(why) if why else "warm-up acked, undeliv=0 on a present [CTLD], no [UNFRAMABLE], [SYMSZ] local = decoder"
    return "REPORTED", f"warm-up {t['warm']}" + (" (a datum, not an abort)" if t["warm"] == "NEVER-ACKED" else "")


def _rows(path):
    import place38_parse as pp
    import pool56_score as ps
    rows, tokens = pp.rows_of([path])
    cnt, replaced = ps.counting([r for r in rows if r.get("cell") == CELL and valid_slot(r.get("cell"), r.get("arm"))])
    return cnt, tokens


def _fmt_und(seat, t):
    if seat == "CONTROL":
        return "n/a"
    return f"{t['undeliv']}(srv {t.get('undeliv_srv', 0)}+cli {t.get('undeliv_cli', 0)})" if t.get("ctld") else "MISSING"


def _fmt_sz(v):
    return ",".join(f"{a}/{b}" for a, b in v) or "none"


def _reparse(r, raw):
    """The row's t63 recomputed from its raw logs (`<raw>/<cell>@def-<arm>-s<seed>-r<rep>-a1-{c.log,s.log,drv.out}`), for a ledger written before a field existed; None when the logs are missing."""
    base = os.path.join(raw, f"{r['cell']}@def-{r['arm']}-s{r['seed']}-r{r['rep']}-a1-")
    try:
        rd = lambda suf: open(base + suf, errors="replace").read().splitlines()      # noqa: E731
        return row_metrics(rd("c.log"), rd("s.log"), rd("drv.out"))
    except OSError:
        return None


def report(path, out=print, raw=None):
    """One ROW63 line per row and one LABEL63 line per row, then the per-seat bin_sha and VERDICT63. All reported: nothing here gates a launch. CONTROL: warm-up failures out of its rows (no per-row prediction; 0 of 5
    means the CAND reading is weak: nothing was reproduced). CAND: the F2 reading per row; a failed CAND row blocks E-pre."""
    cnt, _ = _rows(path)
    out(f"S63 REPORT rows={len(cnt)} (F2 first contact; reported only, no void)")
    shas = {"CONTROL": set(), "CAND": set()}
    n = {"CONTROL": 0, "CAND": 0}
    ctl_fail = cand_fail = 0
    for r in sorted(cnt, key=lambda r: (int(r["rep"]), r["arm"][0] != "C")):
        seat, base = arm_parse(r["arm"])
        t = (_reparse(r, raw) if raw else None) or r.get("t63") or {}
        if not t:
            out(f"ROW63 {r['arm']} s{r['seed']} r{r['rep']}: no t63 ({r.get('t63_error')})")
            continue
        n[seat] += 1
        shas[seat].add(r.get("bin_sha") or "-")
        lab, note = label_row(r["arm"], r["seed"], t)
        ctl_fail += seat == "CONTROL" and t["warm"] == "NEVER-ACKED"
        cand_fail += lab == "FAIL"
        out(f"ROW63 {r['arm']} s{r['seed']} r{r['rep']}: status={r.get('status')} warm={t['warm']} transfer={t['transfer']} undeliv={_fmt_und(seat, t)} unframable={t['unframable']} "
            f"symsz(cli)={_fmt_sz(t['symsz']['cli'])} symsz(srv)={_fmt_sz(t['symsz']['srv'])} rate_echo={t['rate_echo']} bin_sha={str(r.get('bin_sha'))[:16]}"
            + ("" if seat == "CONTROL" else f" | denominator (server received): src_n={t.get('src_n')} ctld_rx={t.get('srv_rx')} ctld_fr={t.get('srv_fr')}"))
        out(f"LABEL63 {r['arm']}-s{r['seed']} {lab} ({note})")
    want = {"CONTROL": os.environ.get("LD_BIN_SHA_CONTROL") or CONTROL_SHA, "CAND": os.environ.get("LD_BIN_SHA_CAND") or None}      # run_all exports both pins
    for seat in ("CONTROL", "CAND"):
        v = sorted(shas[seat])
        ok = bool(v) and all(x == want[seat] for x in v) if want[seat] else None
        out(f"SEAT63 {seat} bin_sha {','.join(x[:16] for x in v) or '-'} " + ("(equals the pinned value)" if ok else "(NOT the pinned value)" if ok is False else "(no pin in this environment: recorded)"))
    weak = n["CONTROL"] > 0 and ctl_fail == 0
    out(f"VERDICT63 REPORTED-ONLY rows={len(cnt)}/{expect_rows()} control_warmup_fail={ctl_fail}/{n['CONTROL']} cand_fail={cand_fail}/{n['CAND']}" + (" CAND-READING-WEAK (CONTROL reproduced nothing: reported, not voided)" if weak else "")
        + (" BLOCKS-E-PRE (a failed candidate row is a finding against the fix)" if cand_fail else ""))
    return 0


def smoke(kind, path, out=print):
    """The smoke (the CAND PL s42 row) / rehearsal (CONTROL PL s42 then CAND PL s42): each row must have run with its binary right and its t63 computed (these gate: SMOKE-PASS | SMOKE-FAIL). Their readings are
    REPORTED (the CAND's against the F2 reading; a failed CAND rehearsal row is the finding), and a rehearsal row is never re-run."""
    cnt, _ = _rows(path)
    slots = REHEARSAL if kind == "rehearsal" else SMOKE
    got = {(r["arm"], str(r["seed"]), str(r["rep"])): r for r in cnt}
    bad = []

    def crit(name, fails):
        out(("PASS " if not fails else "FAIL ") + name + ("" if not fails else ": " + "; ".join(fails)))
        bad.extend(fails)
    out(f"SMOKE63 kind={kind} rows={len(cnt)} slots={len(slots)}")
    miss = [f"{a} s{s} r{r}: no row" for (_, _, a, s, r) in slots if (a, s, str(r)) not in got]
    crit("every smoke / rehearsal row ran", miss)
    for (_, _, a, s, r) in slots:
        row = got.get((a, s, str(r)))
        if row is None:
            continue
        t = row.get("t63") or {}
        crit(f"{a}: the row carries its t63 readouts", [] if t and not row.get("t63_error") else [f"{a}: no t63 ({row.get('t63_error')})"])
        crit(f"{a}: the row ran its seat's binary (no bin-sha problem)", [x for x in row.get("problems") or [] if x.startswith("bin-sha")])
        if t:
            seat = arm_parse(a)[0]
            lab, note = label_row(a, s, t)
            out(f"  REPORTED {a}-s{s} {lab}: {note}; warm={t['warm']} transfer={t['transfer']} undeliv={_fmt_und(seat, t)} unframable={t['unframable']} symsz(cli)={_fmt_sz(t['symsz']['cli'])} symsz(srv)={_fmt_sz(t['symsz']['srv'])}")
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
            for c, s, ar, sd, rp in plan(a[0], smoke=len(a) > 1 and a[1] == "smoke", rehearsal=len(a) > 1 and a[1] == "rehearsal"):
                print(f"{c} {s} {ar} {sd} {rp}")
            return 0
        if cmd == "count":
            print(len(plan(a[0], smoke=len(a) > 1 and a[1] == "smoke", rehearsal=len(a) > 1 and a[1] == "rehearsal")))
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
            return report(a[0], raw=a[1] if len(a) > 1 else None)
    except ValueError as e:
        print(f"ABORT-PLAN63 {e}")
        return 3
    print(f"unknown command {cmd}")
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
