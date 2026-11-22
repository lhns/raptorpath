#!/usr/bin/env python3
"""The design of the disorder-price (omega) measurement as data (registration 6, 7.2, 12). Pure: no I/O but the CLI's prints.

  omega_cells.py configs                         the 8 core configurations
  omega_cells.py arms <cfg> [pins.json]          the arms of one configuration: name, kind, label, dt_ms
  omega_cells.py count                           cell counts (130 + 100 + 3 + 5 = 238)
  omega_cells.py rows <session> [pins.json]      the rows of a session, one per line:  cfg arm rep seedhalf stage   (R, R-rehearsal, D<k>)
  omega_cells.py budget <session> [pins.json]    estimated wall seconds of the session
  omega_cells.py sessions [pins.json]            how many D sessions the plan needs
  omega_cells.py param <cfg> <arm> [pins.json]   JSON of the row's parameters (g rtt limit B tau_q_ms kind label dt_ms entries spacing_s L)
  omega_cells.py pseudo <cfg> [pins.json]        JSON label -> [dt_ms, spacing_s] for a CLEAN row's pseudo-events

OMEGA_TINY=1 shrinks the design to one configuration, two arms of each kind and n = 2 (the stub run's seam; never a launch).
"""
import json
import math
import os
import random
import sys

RATES = (1000, 10000)          # packets/s (1500 B)
RTTS = (10, 50)                # ms
F = 8
L_DEFAULT = 16384
SEEDS = (42, 7)
REP_S = {"Qt": 41.0, "Qs": 19.0}       # registration 12, worst case on paper
SESSION_CAP_S = 4.5 * 3600
PHASE_CAP_S = 20 * 3600
NMIN, NMAX = 8, 24
RULE_HOLDS_MS = (11, 36, 61)
HOLD_SETS = {"Bulk": (1.0, 61.0), "Auto": (1.0, 26.0), "Realtime": (0.0, 0.35)}   # registration 4.4, ms
OUT_CFG = "g10000-rtt50-Qt"


def tiny():
    return os.environ.get("OMEGA_TINY") == "1"


def cfg_id(g, rtt, q):
    return "g%d-rtt%d-%s" % (g, rtt, q)


def parse_cfg(cid):
    a, b, c = cid.split("-")
    return int(a[1:]), int(b[3:]), c


def configs():
    out = [cfg_id(g, r, q) for q in ("Qt", "Qs") for g in RATES for r in RTTS]
    return out[:1] if tiny() else out


def bdp(g, rtt):
    return g * rtt / 1000.0


def q_limit_pkts(cid, pins):
    """Queue limit in packets: Q_s = max(ceil(W/4), 4); Q_t = the pinned measured standing backlog B_t (a rate-netem limit)."""
    g, rtt, q = parse_cfg(cid)
    if q == "Qs":
        return max(int(math.ceil(bdp(g, rtt) / 4.0)), 4)
    bt = (pins or {}).get("B_t_pkts", {}).get(str(g))
    return int(bt) if bt else 100          # placeholder for R and the rehearsal; a D session refuses a pins file without it


def standing_b(cid, pins):
    return q_limit_pkts(cid, pins)         # B: B_t at Q_t, the limit at Q_s (4.4)


def tau_q_ms(cid, pins):
    g, _, _ = parse_cfg(cid)
    return standing_b(cid, pins) / float(g) * 1000.0


def _arm(label, dt_ms):
    return {"label": label, "dt_ms": round(dt_ms, 6)}


def dt_arms(cid, pins=None):
    """The Delta-t arms of one configuration, registration 6.1/6.2. Degenerate ones (dt*g < 1 packet) are dropped."""
    g, rtt, q = parse_cfg(cid)
    ks = (3, 8) if q == "Qs" else (1, 2, 3, 8, 32)
    ts = (1, 4) if q == "Qs" else (0.25, 1, 4)
    out = [_arm("k_a=%d" % k, (k + 0.5) / g * 1000.0) for k in ks]
    out += [_arm("t=%g" % t, t * rtt / 4.0) for t in ts]
    tq = tau_q_ms(cid, pins)
    out += [_arm("tau*%g" % m, m * tq) for m in (0.5, 1, 2)]
    out += [_arm("k_f=%d" % k, (k + 0.5) * F / g * 1000.0) for k in (3, 8)]
    out += [_arm("rule=%d" % r, float(r)) for r in RULE_HOLDS_MS]
    out = [a for a in out if a["dt_ms"] * g / 1000.0 >= 1.0]
    if tiny():
        out = [a for a in out if a["label"] in ("k_a=3", "k_f=3")] or out[:2]
    return out


def arms(cid, pins=None):
    """CLEAN, then REL and HOLD at every dt arm."""
    out = [{"arm": "CLEAN", "kind": "CLEAN", "label": "CLEAN", "dt_ms": 0.0}]
    for a in dt_arms(cid, pins):
        for kind in ("REL", "HOLD"):
            out.append({"arm": "%s@%s" % (kind, a["label"]), "kind": kind, "label": a["label"], "dt_ms": a["dt_ms"]})
    return out


def outage_arms():
    return [{"arm": n, "kind": n, "label": n, "dt_ms": 1000.0} for n in ("CLEAN", "OUT-REL", "OUT-HOLD")]


def stage_t_arms(min_rtt_ms):
    out = [{"arm": "CLEAN", "kind": "CLEAN", "label": "CLEAN", "dt_ms": 0.0}]
    for t in (1, 4):
        for kind in ("REL", "HOLD"):
            out.append({"arm": "%s@t=%d" % (kind, t), "kind": kind, "label": "t=%d" % t, "dt_ms": t * min_rtt_ms / 4.0})
    return out


def count_cells():
    """The registered cell count per group (full design, whatever OMEGA_TINY says)."""
    save = os.environ.pop("OMEGA_TINY", None)
    try:
        qt = sum(len(arms(cfg_id(g, r, "Qt"))) for g in RATES for r in RTTS)
        qs = sum(len(arms(cfg_id(g, r, "Qs"))) for g in RATES for r in RTTS)
    finally:
        if save is not None:
            os.environ["OMEGA_TINY"] = save
    return {"Qt": qt, "Qs": qs, "outage": len(outage_arms()), "stageT": len(stage_t_arms(10.0)), "total": qt + qs + 3 + 5}


def event_spacing(cid, dt_ms, pins, L):
    """(delayed entries, spacing s), or (None, None) = SKIPPED-TABLE. 1 s; 3 s where A_e exceeds 1 s and the table can say it, else the table's minimum p (6.2)."""
    g, rtt, _ = parse_cfg(cid)
    a_e = dt_ms / 1000.0 + 4 * rtt / 1000.0 + q_limit_pkts(cid, pins) / float(g)
    if a_e <= 1.0:
        n = int(round(L / (g * 1.0)))
        return (n, 1.0) if n >= 1 else (None, None)
    n = int(round(L / (g * 3.0)))
    return (n, 3.0) if n >= 1 else (1, L / float(g))


def n_reps(cid, pins):
    if tiny():
        return 2
    n = (pins or {}).get("n", {}).get(cid, NMIN)
    return max(NMIN, min(NMAX, int(n)))


def seedhalf(rep, n):
    return 1 if rep <= (n + 1) // 2 else 2


def shuffled(arms_, seed):
    a = list(arms_)
    random.Random(seed).shuffle(a)
    return a


def config_rows(cid, pins, rounds=None, stage="D"):
    """Rows of one configuration: round r runs every arm once in a seeded shuffle (rule 3)."""
    al = arms(cid, pins)
    n = n_reps(cid, pins)
    lo, hi = rounds if rounds else (1, n)
    rows = []
    for r in range(lo, hi + 1):
        for a in shuffled(al, "%s|%d" % (cid, r)):
            rows.append((cid, a["arm"], r, seedhalf(r, n), stage))
    return rows


def config_cost_s(cid, n_rounds, pins=None):
    return n_rounds * len(arms(cid, pins)) * REP_S[parse_cfg(cid)[2]]


def plan_sessions(pins):
    """Pack the D configurations into sessions of at most 4.5 h, cut at balanced round boundaries: [[(cfg, r_lo, r_hi), ...], ...]."""
    sessions, cur, used = [], [], 0.0
    for cid in configs():
        n = n_reps(cid, pins)
        per_round = config_cost_s(cid, 1, pins)
        r = 1
        while r <= n:
            fit = int((SESSION_CAP_S - used) // per_round)
            if fit < 1:
                if not cur:
                    raise SystemExit("ABORT-BUDGET one round of %s exceeds the session cap" % cid)
                sessions.append(cur)
                cur, used = [], 0.0
                continue
            take = min(fit, n - r + 1)
            cur.append((cid, r, r + take - 1))
            used += take * per_round
            r += take
    if cur:
        sessions.append(cur)
    return sessions


def _one_per_kind(al):
    seen, out = set(), []
    for a in al:
        if a["kind"] not in seen:
            seen.add(a["kind"])
            out.append(a)
    return out


def design_rows(session, pins):
    """(rows, est_seconds). session: R, R-rehearsal, D<k>. A row is (cfg, arm, rep, seedhalf, stage)."""
    if session == "R-rehearsal":
        cid = configs()[0]
        rows = [(cid, a["arm"], 1, 1, "smoke") for a in _one_per_kind(arms(cid, pins))]
        rows += [(OUT_CFG, a["arm"], 1, 1, "smoke") for a in outage_arms() if a["kind"] != "CLEAN"]
        return rows, len(rows) * REP_S["Qt"] + 60
    if session == "R":
        rows = []
        for cid in configs():                       # calibration: 4 arms x 6 reps, nothing scored (11.1)
            al = arms(cid, pins)
            tq = tau_q_ms(cid, pins)
            within = [a for a in al if a["kind"] == "HOLD" and a["dt_ms"] <= tq]
            pick = [a for a in al if a["arm"] in ("CLEAN", "REL@k_f=8", "HOLD@k_f=8")]
            if within:
                pick.append(max(within, key=lambda a: a["dt_ms"]))
            nc = 2 if tiny() else 6
            for r in range(1, nc + 1):
                for a in shuffled(pick, "%s|calib|%d" % (cid, r)):
                    rows.append((cid, a["arm"], r, seedhalf(r, nc), "calib"))
        rows += [(configs()[0], a["arm"], 1, 1, "smoke") for a in _one_per_kind(arms(configs()[0], pins))]
        return rows, sum(REP_S[parse_cfg(r[0])[2]] for r in rows) + 2 * 90 + 900   # + the tunnel's two 90 s flows, R0, setup
    if session.startswith("D") and session[1:].isdigit():
        k = int(session[1:])
        plan = plan_sessions(pins)
        if not 1 <= k <= len(plan):
            raise SystemExit("ABORT-BUDGET session %s: the plan has %d D sessions" % (session, len(plan)))
        rows = []
        for cid, lo, hi in plan[k - 1]:
            rows += config_rows(cid, pins, (lo, hi))
        if k == len(plan) and not (pins or {}).get("skip_outage"):   # the outage check and stage T ride the last session (0.8 h); R0 may skip the outage
            no = 2 if tiny() else 8
            for r in range(1, no + 1):
                for a in shuffled(outage_arms(), "outage|%d" % r):
                    rows.append((OUT_CFG, a["arm"], r, seedhalf(r, no), "OUT"))
            mr = float((pins or {}).get("stage_t_min_rtt_ms", 10.0))
            for r in range(1, no + 1):
                for a in shuffled(stage_t_arms(mr), "T|%d" % r):
                    rows.append(("T", a["arm"], r, seedhalf(r, no), "T"))
        return rows, sum(REP_S["Qt"] if r[0] == "T" else REP_S[parse_cfg(r[0])[2]] for r in rows)
    raise SystemExit("unknown session %r" % session)


def row_params(cid, arm, pins, L=L_DEFAULT):
    """Everything one row needs: g rtt limit B tau_q kind dt_ms entries spacing."""
    g, rtt, q = parse_cfg(OUT_CFG if cid == "T" else cid)
    c = OUT_CFG if cid == "T" else cid
    if arm in ("OUT-REL", "OUT-HOLD", "CLEAN"):
        al = outage_arms() if cid == OUT_CFG else arms(c, pins)
    elif cid == "T":
        al = stage_t_arms(float((pins or {}).get("stage_t_min_rtt_ms", 10.0)))
    else:
        al = arms(c, pins)
    a = next((x for x in al if x["arm"] == arm), None)
    if a is None:
        raise SystemExit("unknown arm %r of %s" % (arm, cid))
    ent, sp = (0, 0.0) if a["kind"] in ("CLEAN", "OUT-REL", "OUT-HOLD") else event_spacing(c, a["dt_ms"], pins, L)
    return {"g": g, "rtt": rtt, "q": q, "limit": q_limit_pkts(c, pins), "B": standing_b(c, pins), "tau_q_ms": tau_q_ms(c, pins),
            "kind": a["kind"], "label": a["label"], "dt_ms": a["dt_ms"], "entries": ent, "spacing_s": sp, "L": L}


def pseudo_dts(cid, pins, L=L_DEFAULT):
    """label -> [dt_ms, event spacing s] of every dt arm: the instants and windows of a CLEAN row's pseudo-events (4.4: REL's event rate)."""
    c = OUT_CFG if cid == "T" else cid
    al = stage_t_arms(float((pins or {}).get("stage_t_min_rtt_ms", 10.0))) if cid == "T" else arms(c, pins)
    out = {}
    for a in al:
        if a["kind"] == "REL":
            ent, sp = event_spacing(c, a["dt_ms"], pins, L)
            if ent is not None:
                out[a["label"]] = [a["dt_ms"], sp]
    return out


def load_pins(path):
    if not path:
        return {}
    with open(path) as f:
        return json.load(f)


def main(argv):
    if len(argv) < 2:
        print(__doc__)
        return 3
    cmd, rest = argv[1], argv[2:]
    if cmd == "configs":
        print("\n".join(configs()))
    elif cmd == "arms" and rest:
        for a in arms(rest[0], load_pins(rest[1] if len(rest) > 1 else None)):
            print("%s %s %s %s" % (a["arm"], a["kind"], a["label"], a["dt_ms"]))
    elif cmd == "count":
        print(json.dumps(count_cells()))
    elif cmd == "rows" and rest:
        for r in design_rows(rest[0], load_pins(rest[1] if len(rest) > 1 else None))[0]:
            print(" ".join(str(x) for x in r))
    elif cmd == "budget" and rest:
        print(int(design_rows(rest[0], load_pins(rest[1] if len(rest) > 1 else None))[1]))
    elif cmd == "sessions":
        print(len(plan_sessions(load_pins(rest[0] if rest else None))))
    elif cmd == "param" and len(rest) >= 2:
        pins = load_pins(rest[2] if len(rest) > 2 else None)
        p = row_params(rest[0], rest[1], pins, int(pins.get("L", L_DEFAULT)))
        print(json.dumps(p))
    elif cmd == "pseudo" and rest:
        pins = load_pins(rest[1] if len(rest) > 1 else None)
        print(json.dumps(pseudo_dts(rest[0], pins, int(pins.get("L", L_DEFAULT)))))
    else:
        print(__doc__)
        return 3
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
