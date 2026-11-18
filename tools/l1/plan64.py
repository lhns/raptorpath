#!/usr/bin/env python3
"""Status 64: F2's merge guard (docs/derivations/f2-guard-registration.md, f2-guard-design 39269746). Plan 64 is plan 41d's guard (stage G, Bulk, LD_PLAN=41d with LD_WITNESS64=1) plus the F2 row witnesses
on every CAND row, then stage R (LD_PLAN=64): the Realtime cell at rho = 1, arms RB (BASE's binary) and RC (CAND's), scored on the F2 reading only. The tables, the row witnesses and labels, the replacement
rule and the smoke / slot checks are here; the readouts and the stage R score are rt64_score.py, the framing bench's reading is framing64.py.

    plan64.py plan [smoke]                    stage R's table, `cell store arm seed rep` per line (smoke = RB s42 then RC s42, the rehearsal's rows)
    plan64.py count [smoke]                   the number of rows the table holds
    plan64.py replace <ledger> <cell> <arm> <seed> <rep>    battery helper: prints the reason and returns 0 iff the slot's one row so far is a non-F2 void and fewer than 2 slots were replaced
    plan64.py check <ledger>                  rc 0 iff the stage R battery finished and the ledger holds a row for every slot
    plan64.py smoke <ledger>                  stage R's smoke / rehearsal check (SMOKE-PASS | SMOKE-FAIL)
    plan64.py budget <64|64r>                 the registered worst-case seconds the launch / rehearsal needs before LD_HARD

Stage G status precedence (registration 2.1; applied in place38_parse.make_row behind LD_WITNESS64=1, CAND rows only):
  1. an infrastructure void (VOID-COTENANT, VOID-ONDISK, CONTAMINATED, a seat / sha problem) comes first and is replaced under plan 41d's rule;
  2. undeliv > 0, an [UNFRAMABLE] line or a [SYMSZ] that is not 1200 / 1200 is F2-FAIL, even on a DNF or a VOID-RC row (a fail-closed tunnel looks like a DNF);
  3. a missing witness line ([SYMSZ] on either end, the server's [CTLD] with undeliv=) is F2-FAIL only on a row that is otherwise LIVE (a DNF is LIVE with dnf true);
  4. then plan 41d's own labels.
An F2-FAIL row is never replaced and never counted toward NO VERDICT: the ledger keeps status F2-FAIL (REPLACEABLE skips it) and the label the row would have had (status_41d); score64 scores the 41d view."""
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import l1common as lc  # noqa: E402

CELL = "c2-100"
ARMS_R = ("RB", "RC")
SEAT_OF = {"RB": "BASE", "RC": "CAND"}
BYTES_R = 100000000
TIMEOUT_R = 150
BLOCKS_R = [(sd, rp) for rp in range(1, 7) for sd in ("42", "7")]       # 12 groups, rep-major, seed 42 before 7: 6 + 6 per seed per arm (plan 41c's)
SMOKE_R = [(CELL, "def", "RB", "42", 1), (CELL, "def", "RC", "42", 1)]
MAX_REPLACEMENTS_R = 2                                                    # registration 2.2: at most 2 slots are replaced in stage R
WARM_TOKEN = "warm-up object never acked"
REALTIME_ECHO = "RWM_UNIFIED: Realtime rides the RLC span machine"         # place38_parse.REALTIME_ECHO59 (test_harness64 asserts it)
SYMSZ_G = (1200, 1200)
SYMSZ_R = (512, 512)
# registered worst cases (seconds the launch / rehearsal needs before LD_HARD; the launch sets LD_HARD = launch + 2.5 h, the rehearsal + 1.25 h): section 4 with every RB row at its full timeout.
# launch: smoke 300 + stage G 2880 + 4 replaced G slots 1000 + stage R smoke 200 + 12 RC rows 360 + 12 RB rows at the full 150 s timeout 1980 + 2 replaced R slots 330 + scoring 600
BUDGET_S = {"64": 7650, "64r": 3300}
F2_FAIL = "F2-FAIL"
INFRA_STATUSES = ("VOID-COTENANT", "VOID-ONDISK", "CONTAMINATED")
R_REPLACEABLE = ("WITNESS-FAIL", "CONTAMINATED", "VOID-COTENANT", "VOID-ONDISK", "VOID-RHO", "VOID-RC", "NO_DATA")      # stage R's non-F2 voids (replaced once)
R_VOID = R_REPLACEABLE
_WR = re.compile(r"window_reliable: (true|false)")


def plan(smoke=False):
    """[(cell, store, arm, seed, rep)] in run order: plan 41c's interleave, twelve groups (seed, rep), per group the pair RB, RC (even groups) or RC, RB (odd groups)."""
    if smoke:
        return list(SMOKE_R)
    out = []
    for g, (seed, rep) in enumerate(BLOCKS_R):
        for a in (("RB", "RC") if g % 2 == 0 else ("RC", "RB")):
            out.append((CELL, "def", a, seed, rep))
    return out


def expect_rc():
    return sum(1 for x in plan() if x[2] == "RC")


def valid_slot(cell, arm):
    return cell == CELL and arm in ARMS_R


# ── the F2 reading of one row (both stages) ──
def window_reliable(lines):
    """The resolved config each end prints (`perf configuration ... window_reliable: <bool>`): the last such line's value, None when the end printed none."""
    v = None
    for ln in lines or []:
        s = lc.strip(ln)
        if "perf configuration" in s:
            m = _WR.search(s)
            if m:
                v = m.group(1) == "true"
    return v


def f2_reading(cli, srv, drv=()):
    """The F2 witnesses of a row. Presence is read from the logs' [CTLD] lines, never from the driver's `--- F2` line (that line prints undeliv=0 when no counter exists, as in BASE).
    ctld_* = how many of an end's [CTLD] lines carry `undeliv=`; undeliv_* = the largest value on that end (the sum over both ends is the reported field); src_n / srv_rx / srv_fr = the
    received-symbols denominator of the receiving server (the last [RFA]'s src_n; the last [CTLD]'s rx= and dgrx0[fr=...])."""
    cli, srv, drv = list(cli or []), list(srv or []), list(drv or [])

    def ends(lines):
        n = sum(1 for ln in lines if "[CTLD]" in ln and lc.inum(lc.field(lc.strip(ln), "undeliv")) is not None)
        return n, lc.f2_counts(lines)[0], lc.f2_counts(lines)[1]

    def last(lines, tag, key):
        v = None
        for ln in lines:
            if tag in ln:
                x = lc.inum(lc.field(lc.strip(ln), key))
                v = x if x is not None else v
        return v
    n_s, u_s, unf_s = ends(srv)
    n_c, u_c, unf_c = ends(cli)
    fr = [m for m in (re.search(r"dgrx0\[fr=(\d+)", lc.strip(ln)) for ln in srv if "[CTLD]" in ln) if m]
    return {"v": 1, "ctld_srv": n_s, "ctld_cli": n_c, "undeliv_srv": u_s, "undeliv_cli": u_c, "undeliv": u_s + u_c, "unframable": unf_s + unf_c,
            "symsz_cli": lc.symsz(cli), "symsz_srv": lc.symsz(srv),
            "src_n": last(srv, "[RFA]", "src_n"), "srv_rx": last(srv, "[CTLD]", "rx"), "srv_fr": int(fr[-1].group(1)) if fr else None,
            "warm": "NEVER-ACKED" if any(WARM_TOKEN in ln for ln in cli + drv) else ("ACKED" if cli else "NO-LOG"),
            "wr_cli": window_reliable(cli), "wr_srv": window_reliable(srv),
            "echo_cli": any(REALTIME_ECHO in ln for ln in cli), "echo_srv": any(REALTIME_ECHO in ln for ln in srv)}


def symsz_state(rd, want):
    """-> (missing sides, wrong sides): a side with no [SYMSZ] line is missing; a side with a line that is not local = decoder = want is wrong."""
    miss = [s for s in ("cli", "srv") if not rd["symsz_" + s]]
    bad = [s for s in ("cli", "srv") if rd["symsz_" + s] and any(p != want for p in rd["symsz_" + s])]
    return miss, bad


def ctld_missing(rd):
    return None if rd["ctld_srv"] else ("no [CTLD] line with undeliv= on the receiving end (the perf server): missing witness" + (" (the sending end printed one)" if rd["ctld_cli"] else ""))


# ── stage G ──
def g_witness(rd):
    """-> (hard, missing): the CAND row's F2 failures. hard = undeliv > 0, [UNFRAMABLE], a wrong [SYMSZ]; missing = no [SYMSZ] on an end, no server [CTLD] with undeliv=."""
    hard, missing = [], []
    if rd["undeliv"] > 0:
        hard.append(f"undeliv={rd['undeliv']}")
    if rd["unframable"] > 0:
        hard.append(f"[UNFRAMABLE] x{rd['unframable']}")
    miss, bad = symsz_state(rd, SYMSZ_G)
    if bad:
        hard.append("[SYMSZ] not 1200/1200 on " + "+".join(bad))
    if miss:
        missing.append("[SYMSZ] missing on " + "+".join(miss))
    m = ctld_missing(rd)
    if m:
        missing.append(m)
    return hard, missing


def g_row(row, arm, cli, srv, drv, sha, sha_cand, sha_base):
    """Stage G's F2 side of one row (LD_WITNESS64=1, plan 41d): fills row["w64"], returns the problems that void the row (the seat / sha comparison), and applies the status precedence at the END of
    make_row via g_status. BASE rows record f2_void (expected None); CAND rows the reading and the F2 failures."""
    import pool54_score
    p = pool54_score.sha_problems(arm, sha, sha_cand, sha_base)
    rd = f2_reading(cli, srv, drv)
    w = {"v": 1, "sha_checked": pool54_score.sha_checked(arm, sha, sha_cand, sha_base)}
    if arm == "CAND":
        hard, missing = g_witness(rd)
        w.update({k: rd[k] for k in ("ctld_srv", "ctld_cli", "undeliv_srv", "undeliv_cli", "undeliv", "unframable", "symsz_cli", "symsz_srv", "src_n", "srv_rx", "srv_fr")})
        w.update({"hard": hard, "missing": missing, "executed": bool(rd["symsz_cli"]) and bool(rd["symsz_srv"])})
    else:
        w.update({"f2_void": lc.f2_void(list(cli) + list(srv) + list(drv)), "undeliv": rd["undeliv"], "symsz_cli": rd["symsz_cli"], "symsz_srv": rd["symsz_srv"]})
    row["w64"] = w
    return p


def g_status(row):
    """The F2-FAIL precedence on a finished stage G row (the row keeps what it would have been in status_41d)."""
    w = row.get("w64") or {}
    if row.get("arm") != "CAND":
        return
    st = row["status"]
    if st in INFRA_STATUSES or any(x.startswith("bin-sha") for x in row.get("problems") or []):
        return                                                       # 1. infrastructure first
    hard, missing = w.get("hard") or [], w.get("missing") or []
    why = None
    if hard:
        why = hard + missing                                         # 2. a fail-closed tunnel: even on a DNF or a VOID-RC row
    elif missing and st == "LIVE":
        why = missing                                                # 3. a missing witness: only on an otherwise LIVE (or DNF) row
    if why:
        row["status_41d"] = st
        row["status"] = F2_FAIL
        row["f2_why"] = why


def view41d(row):
    """A stage G row as score41d must read it: an F2-FAIL row is scored under the label it would have had (a DNF stays its worst rank, a crash stays a crash); a label that would have been a
    replaceable void reads as a crash (VOID-RC: the worst rank, never replaced, never counted toward NO VERDICT)."""
    if row.get("status") != F2_FAIL:
        return row
    import place38_parse as pp
    st = row.get("status_41d") or "VOID-RC"
    if st in pp.REPLACEABLE:
        st = "VOID-RC"
    return dict(row, status=st)


def rows_41d_view(paths):
    import place38_parse as pp
    rows, tokens = pp.rows_of(paths)
    return [view41d(r) for r in rows], tokens


def guard_table(paths, out=print):
    """The F2 witness table of stage G (after score41d's output): per cell the CAND rows, F2-FAIL rows, the executes witness, the readings; the BASE rows' f2_void; every F2-FAIL row with its reason."""
    import place38_parse as pp
    rows_all, _ = pp.rows_of(paths)
    rows, _ = pp.apply_replacements(rows_all)
    out("F2-WITNESS64 (registration 2.1: the F2 row witnesses on every CAND row; outside the min-p family; the verdict above is score41d's and does not read this)")
    for cell, _ in pp.GUARD_CELLS:
        rc_ = [r for r in rows if r["cell"] == cell and r["arm"] == "CAND"]
        if not rc_:
            continue
        ws = [r.get("w64") or {} for r in rc_]
        sz = sum(1 for w in ws if w.get("symsz_cli") and w.get("symsz_srv") and all(p == SYMSZ_G for p in w["symsz_cli"] + w["symsz_srv"]))
        ctl = sum(1 for w in ws if w.get("ctld_srv"))
        out(f"  {cell:<8} CAND rows {len(rc_):>2}  F2-FAIL {sum(1 for r in rc_ if r['status'] == F2_FAIL)}  [SYMSZ] 1200/1200 on both ends {sz}/{len(rc_)}  server [CTLD] with undeliv= {ctl}/{len(rc_)}  "
            f"undeliv sum {sum(w.get('undeliv') or 0 for w in ws)}  [UNFRAMABLE] {sum(w.get('unframable') or 0 for w in ws)}  executed {sum(1 for w in ws if w.get('executed'))}/{len(rc_)}")
    base = [r for r in rows if r["arm"] == "BASE"]
    odd = [r for r in base if (r.get("w64") or {}).get("f2_void")]
    out(f"  BASE rows {len(base)}  f2_void non-None {len(odd)} (expected 0; reported, never a verdict)  [SYMSZ] on BASE {sum(1 for r in base if (r.get('w64') or {}).get('symsz_cli') or (r.get('w64') or {}).get('symsz_srv'))} (expected 0)")
    for r in odd:
        out(f"    BASE-F2 {r['cell']} s{r['seed']} rep{r['rep']}: {r['w64']['f2_void']}")
    nfail = 0
    for r in rows_all:
        if r["status"] == F2_FAIL:
            nfail += 1
            out(f"  F2-FAIL {r['cell']} {r['arm']} s{r['seed']} rep{r['rep']} (would be {r.get('status_41d')}): {'; '.join(r.get('f2_why') or [])}")
    return nfail


def score64(paths, out=print):
    """Plan 64's stage G score: score41d's output byte for byte (on the 41d view of the ledger: an F2-FAIL row is scored under its own 41d label), then the F2 witness table, then
    `GATE-64 PASS` iff GATE-41d PASS and no F2-FAIL row, else `GATE-64 FAIL (<reason>)`."""
    import place38_parse as pp
    res = pp._score_guard(paths, out, "v2", rows_fn=rows_41d_view)
    out("")
    nfail = guard_table(paths, out)
    why = []
    if res["gate"] != "PASS":
        why.append("GATE-41d " + ("NO VERDICT, re-run" if res["gate"] == "NO VERDICT" else res["gate"]))
    if nfail:
        why.append(f"{nfail} F2-FAIL row(s)")
    out("GATE-64 PASS" if not why else "GATE-64 FAIL (" + "; ".join(why) + ")")
    return res, nfail


def smoke64(path, out=print):
    """Stage G's smoke / rehearsal check: plan 41d's own smoke criteria, no row VOID-RC (a completed row is LIVE), and on every CAND smoke row the F2 witnesses (SYMSZ 1200/1200 on both ends,
    the server's [CTLD] with undeliv=0). A missing line is a harness or log-level fault; undeliv > 0 / [UNFRAMABLE] is a finding against the fix (no launch): both print FAIL."""
    import io
    from contextlib import redirect_stdout
    import place38_parse as pp
    rows, _ = pp.rows_of([path])
    got = {(r["cell"], r["arm"]): r for r in rows}
    bad = []

    def crit(name, fails):
        out(("PASS " if not fails else "FAIL ") + name + ("" if not fails else ": " + "; ".join(fails)))
        bad.extend(fails)
    want = [(c, a) for c, _, a, _, _ in pp.SMOKE41D]
    out(f"SMOKE64 rows={len(rows)}/{len(want)} (plan 41d's smoke: c1s BASE and CAND, the 600 MB c8 CAND row; the values are not read, only the criteria below)")
    crit("every smoke row ran", [f"{c} {a}: no row" for c, a in want if (c, a) not in got])
    rs = [got[k] for k in want if k in got]
    for r in rs:
        out(f"  ROW {r['cell']} {r['arm']}: status={r['status']} rc={r['rc']} mbps={r.get('mbps')} wall={r.get('wall_s')} bytes={r.get('bytes')} last-tick={r.get('tick_span_s')} w64={r.get('w64')}")
    crit("a completed row is LIVE on each seat: no smoke row is VOID-RC (the section 63 defect), none is VOID-SHORT",
         [f"{r['cell']} {r['arm']}: {r['status']}" for r in rs if r["status"] in ("VOID-RC", "VOID-SHORT")])
    buf = io.StringIO()
    with redirect_stdout(buf):
        rc41 = pp.smoke(path, plan41c=True, plan41d=True)
    lines41 = [x for x in buf.getvalue().splitlines() if x]
    for x in lines41:
        if not x.startswith(("SMOKE-PASS", "SMOKE-FAIL")):
            out("  " + x)
    crit("every smoke row is LIVE with the readouts the guard reads, and the 600 MB c8 row carries its size and the in-order delay end to end (plan 41d's own smoke check)",
         [] if rc41 == 0 else [x for x in lines41 if x.startswith("SMOKE-FAIL")] or ["plan 41d's smoke failed"])
    crit("bin_sha matches the row's arm (BASE rows the BASE build, CAND rows the CAND build)", [f"{r['cell']} {r['arm']}: {x}" for r in rs for x in r["problems"] if x.startswith("bin-sha")])
    cand = [r for r in rs if r["arm"] == "CAND"]
    crit("on every CAND smoke row [SYMSZ] local=1200 decoder=1200 on both ends and the server's [CTLD] carries undeliv= (a missing line is a harness or log-level fault)",
         [f"{r['cell']} {r['arm']}: {'; '.join((r.get('w64') or {}).get('missing') or ['no w64'])}" for r in cand if not (r.get("w64") or {}).get("executed") or (r.get("w64") or {}).get("missing")])
    crit("undeliv = 0 on every [CTLD] line of both ends and no [UNFRAMABLE] line on any CAND smoke row (otherwise a finding against the fix: no launch)",
         [f"{r['cell']} {r['arm']}: {'; '.join((r.get('w64') or {}).get('hard') or [])}" for r in cand if (r.get("w64") or {}).get("hard")])
    if bad:
        out("SMOKE-FAIL " + "; ".join(bad))
        return 1
    out("SMOKE-PASS")
    return 0


# ── stage R ──
def rt_items(rd):
    """The F2 reading of an RC row, items 1-4 of registration 2.2 -> the failing item numbers: 1 warm-up not acked, 2 no server [CTLD] with undeliv= or undeliv > 0 on any [CTLD] line,
    3 an [UNFRAMABLE] line, 4 [SYMSZ] not local = decoder = 512 on both ends (missing counts as failed)."""
    items = []
    if rd["warm"] != "ACKED":
        items.append(1)
    if not rd["ctld_srv"] or rd["undeliv"] > 0:
        items.append(2)
    if rd["unframable"] > 0:
        items.append(3)
    miss, bad = symsz_state(rd, SYMSZ_R)
    if miss or bad:
        items.append(4)
    return items


def rt_row(row, arm, cli, srv, drv, sha, sha_cand, sha_base):
    """Stage R's side of one row (LD_PLAN=64): the Realtime pipe-echo witness, rho = 1 on both ends, the seat's binary, the F2 reading and the status precedence:
    infrastructure voids first (VOID-COTENANT, VOID-ONDISK, CONTAMINATED, a WITNESS-FAIL, then VOID-RHO), then an RB row with the warm-up failure is BASE-F2-DEAD (decided before the rc test),
    an RC row is RT-FAIL (items 1-4) or RT-OK; a crash (VOID-RC, NO_DATA) of a row with no F2 failure is a non-F2 void. Returns the problems to add to the row's."""
    import pool54_score
    from threadp1_parse import inorder_delay_stats, last_tick_s
    rd = f2_reading(cli, srv, drv)
    p = []
    for side, ok in (("cli", rd["echo_cli"]), ("srv", rd["echo_srv"])):
        if not ok:
            p.append(f"no-realtime-echo-{side}")
    if (row.get("pipe_cli") == ["window", "Rlc", "realtime"] and row.get("pipe_srv") == ["window", "Rlc", "realtime"] and rd["echo_cli"] and rd["echo_srv"]
            and "window-line-missing-on-window" in row["problems"]):
        row["problems"].remove("window-line-missing-on-window")          # a Realtime row never prints the reliable-window line (plan 59's rule)
        if row["status"] == "WITNESS-FAIL" and not row["problems"]:
            row["status"] = "LIVE"
    p += pool54_score.sha_problems(SEAT_OF[arm], sha, sha_cand, sha_base)
    rho_bad = not (rd["wr_cli"] is True and rd["wr_srv"] is True)
    st = inorder_delay_stats(cli)
    t = {k: rd[k] for k in ("ctld_srv", "ctld_cli", "undeliv_srv", "undeliv_cli", "undeliv", "unframable", "symsz_cli", "symsz_srv", "src_n", "srv_rx", "srv_fr", "warm", "wr_cli", "wr_srv", "echo_cli", "echo_srv")}
    t["items"] = rt_items(rd) if arm == "RC" else []
    t["sha_checked"] = pool54_score.sha_checked(SEAT_OF[arm], sha, sha_cand, sha_base)
    row["t64"] = t
    row["inorder_mean"] = st["mean"] * 1e3 if st else None
    row["inorder_p95"] = st["p95"] * 1e3 if st else None
    row["inorder_n"] = st["n"] if st else 0
    row["tick_span_s"] = last_tick_s(cli)
    row["_rt"] = (arm, rho_bad, rd, bool(p))
    return p


def rt_status(row):
    """Applied at the end of make_row (after the problems are merged and the status of the 41d machinery is final)."""
    arm, rho_bad, rd, has_p = row.pop("_rt")
    st = row["status"]
    if st in INFRA_STATUSES:
        return
    if rho_bad:
        row["status"] = "VOID-RHO"
        row["problems"].append(f"rho-not-one-witness cli={rd['wr_cli']} srv={rd['wr_srv']} (want window_reliable: true on both ends)")
        return
    if st == "WITNESS-FAIL":
        return
    if arm == "RB":
        if rd["warm"] == "NEVER-ACKED":
            row["status"] = "BASE-F2-DEAD"                                # before the rc test: a dead warm-up exits non-zero after the full timeout
        return
    items = row["t64"]["items"]
    if items:
        row["status"] = "RT-FAIL"
        row["rt_items"] = items
    elif st in ("LIVE",):
        row["status"] = "RT-OK"


def slot_rows(rows):
    """{slot: [rows in ledger order]}"""
    d = {}
    for r in rows:
        d.setdefault((r["cell"], r["arm"], str(r["seed"]), str(r["rep"])), []).append(r)
    return d


def is_void_r(row):
    return row["status"] in R_VOID


def final_rows(rows):
    """Ledger rows -> {slot: the row that takes the slot}: the replacement (the second row) when the first was a non-F2 void, else the first; later rows are ignored."""
    out = {}
    for k, rs in slot_rows(rows).items():
        out[k] = rs[1] if (len(rs) > 1 and is_void_r(rs[0])) else rs[0]
    return out


def replace(ledger, cell, arm, seed, rep):
    """Battery helper: the slot's rows so far are exactly one, that row is a non-F2 void (R_REPLACEABLE), and fewer than MAX_REPLACEMENTS_R slots hold two rows -> print the reason, rc 0; else rc 1."""
    import place38_parse as pp
    rows, _ = pp.rows_of([ledger])
    sl = slot_rows(rows)
    mine = sl.get((cell, arm, str(seed), str(rep)), [])
    done = sum(1 for v in sl.values() if len(v) > 1)
    if len(mine) == 1 and mine[0]["status"] in R_REPLACEABLE and done < MAX_REPLACEMENTS_R:
        print(mine[0]["status"].lower())
        return 0
    return 1


def check(path):
    import place38_parse as pp
    rows, _ = pp.rows_of([path])
    t = open(path, errors="replace").read()
    got = {(r["cell"], r["arm"], str(r["seed"]), str(r["rep"])) for r in rows}
    want = {(c, a, sd, str(rp)) for c, _, a, sd, rp in plan()}
    ok = "LD-BATTERY-DONE" in t and got == want
    print(f"CHECK64R done={'LD-BATTERY-DONE' in t} slots={len(got & want)}/{len(want)} rows={len(rows)} extra={len(got - want)}")
    return 0 if ok else 1


def smoke(path, out=print):
    """Stage R's rehearsal rows (RB s42, RC s42): RC passes items 1-4, completes, is RT-OK (rc 0, not VOID-RC), shows window_reliable: true on both ends and has its readouts computed; RB is LIVE with
    window_reliable: true on both ends, or BASE-F2-DEAD (any other label, VOID-RC included, is a harness fault). A failed RC item is a finding against the fix. Rows are never re-run."""
    import place38_parse as pp
    rows, _ = pp.rows_of([path])
    got = {r["arm"]: r for r in rows if r["cell"] == CELL}
    bad = []

    def crit(name, fails):
        out(("PASS " if not fails else "FAIL ") + name + ("" if not fails else ": " + "; ".join(fails)))
        bad.extend(fails)
    out(f"SMOKE64R rows={len(rows)}/{len(SMOKE_R)} (stage R's rehearsal rows RB s42 and RC s42: the criteria below; a row is never re-run)")
    crit("both rows ran", [f"{a}: no row" for a in ARMS_R if a not in got])
    for a in ARMS_R:
        r = got.get(a)
        if r is None:
            continue
        t = r.get("t64") or {}
        out(f"  ROW {a}: status={r['status']} rc={r['rc']} dnf={r.get('dnf')} mbps={r.get('mbps')} warm={t.get('warm')} wr={t.get('wr_cli')}/{t.get('wr_srv')} items={t.get('items')} undeliv={t.get('undeliv')} symsz={t.get('symsz_cli')}/{t.get('symsz_srv')}")
    rc_, rb = got.get("RC"), got.get("RB")
    if rc_ is not None:
        t = rc_.get("t64") or {}
        crit("RC s42 passes the F2 reading (items 1-4: warm-up acked, server [CTLD] undeliv=0, no [UNFRAMABLE], [SYMSZ] 512/512) - a failed item is a finding against the fix, no launch",
             [f"RC: RT-FAIL items {rc_.get('rt_items')}"] if rc_["status"] == F2_FAIL or rc_["status"] == "RT-FAIL" else [])
        crit("RC s42 is RT-OK: LIVE with rc 0 (not VOID-RC, not VOID-RHO), completed inside its timeout",
             [] if rc_["status"] == "RT-OK" and rc_.get("dnf") is False and rc_.get("rc") == 0 else [f"RC: status={rc_['status']} rc={rc_.get('rc')} dnf={rc_.get('dnf')}"])
        crit("RC s42 shows window_reliable: true on both ends", [] if t.get("wr_cli") is True and t.get("wr_srv") is True else [f"RC: window_reliable cli={t.get('wr_cli')} srv={t.get('wr_srv')}"])
        crit("RC s42 has its readouts computed (goodput, cpu/GB both ends, lag_E both ends, the in-order delay mean: none is None)",
             [f"RC: no-{k}" for k in ("mbps", "cpu_cli", "cpu_srv", "cli_lag_E", "srv_lag_E", "inorder_mean") if rc_.get(k) is None])
    if rb is not None:
        t = rb.get("t64") or {}
        ok = rb["status"] == "BASE-F2-DEAD" or (rb["status"] == "LIVE" and t.get("wr_cli") is True and t.get("wr_srv") is True)
        crit("RB s42 is LIVE with window_reliable: true on both ends, or BASE-F2-DEAD (any other label, VOID-RC included, is a harness fault)",
             [] if ok else [f"RB: status={rb['status']} rc={rb.get('rc')} window_reliable cli={t.get('wr_cli')} srv={t.get('wr_srv')} problems={rb.get('problems')}"])
    crit("each row ran its seat's binary (no bin-sha problem)", [f"{a}: {x}" for a, r in got.items() for x in r.get("problems") or [] if x.startswith("bin-sha")])
    if bad:
        out("SMOKE-FAIL " + "; ".join(bad))
        return 1
    out("SMOKE-PASS")
    return 0


def main(argv):
    if not argv:
        print(__doc__)
        return 2
    cmd, a = argv[0], argv[1:]
    if cmd == "plan":
        for c, s, ar, sd, rp in plan(smoke=bool(a) and a[0] == "smoke"):
            print(f"{c} {s} {ar} {sd} {rp}")
        return 0
    if cmd == "count":
        print(len(plan(smoke=bool(a) and a[0] == "smoke")))
        return 0
    if cmd == "replace":
        return replace(*a[:5])
    if cmd == "check":
        return check(a[0])
    if cmd == "smoke":
        return smoke(a[0])
    if cmd == "budget":
        print(BUDGET_S[a[0]])
        return 0
    print(f"unknown command {cmd}")
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
