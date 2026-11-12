#!/usr/bin/env python3
"""Digest of the lagdiag battery (lagdiag_run.sh): are the c8-100 client
[LAG] tail ticks late because a task poll ran long in-process, or because the
VM / guest CPU stole time?

    lagdiag_digest.py smoke  <prefix>   # witness check of one row; exit 0/1
    lagdiag_digest.py digest <dir>      # table, pooled numbers, ONE OUTCOME line

A row is the files <tag>-c.log (client), <tag>-s.log (server) and
<tag>-stat.txt (10 Hz `T <epoch.ns>` + `cpu*` lines of /proc/stat). Tags are
`b<b>-s<seed>-rt<rtobs>`; the client is primary.

Definitions (pre-registered):
- floor = min lateness of the xfer-window [LAGRAW] samples; a LATE TICK has
  late - floor >= LATE_US; its stall interval, in unix us, is
  [origin + at - (late - floor), origin + at].
- a long poll ([LPOLL], poll wall >= 1 ms) is [end - wall, end]; a late tick
  is OVERLAPPED if some long poll intersects its stall interval by >=
  OVERLAP_US; it is credited to the task with the largest intersection.
  F = overlapped / late ticks.
- E = [LAG] p99 - min (the p99 net of the per-process timer-rounding offset).
- steal / busy come from the stat file per 100 ms interval: steal jiffies
  summed over vCPUs; a vCPU is busy if its (idle + iowait) share < 0.5.
"""
import glob
import math
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import l1common as L  # noqa: E402

LATE_US = 1000
OVERLAP_US = 500
BUSY_MIN = 5          # "busy vCPUs >= 5"
MIN_LIVE_RT2 = 18
STAT_MIN_SAMPLES = 20


# ── parsing ──────────────────────────────────────────────────────────────

def tagged(lines, tag):
    """The lines carrying `tag`, each cut to start at the tag."""
    out = []
    for ln in lines:
        i = ln.find(tag)
        if i >= 0:
            out.append(ln[i:])
    return out


def parse_lagraw(line):
    """-> dict(origin, xfer=(a,b)|None, n, samples=[(at, late)]) or None."""
    origin = L.inum(L.field(line, "origin_unix_us"))
    if origin is None:
        return None
    xf = L.field(line, "xfer")
    xfer = None
    if xf and "-" in xf:
        a, b = xf.split("-", 1)
        if L.inum(a) is not None and L.inum(b) is not None:
            xfer = (int(a), int(b))
    s = L.field(line, "s")
    samples = []
    if s:
        for tok in s.split(","):
            if ":" in tok:
                a, l = tok.split(":", 1)
                if L.inum(a) is not None and L.inum(l) is not None:
                    samples.append((int(a), int(l)))
    return {"origin": origin, "xfer": xfer, "n": L.inum(L.field(line, "n")), "samples": samples}


def parse_lpoll(lines):
    """-> (polls, summary|None). polls: dict(task,end,wall,cpu,vcsw,ivcsw)."""
    polls, summary = [], None
    for ln in tagged(lines, "[LPOLL] "):
        if " summary " in ln:
            summary = {
                "n": L.inum(L.field(ln, "n")),
                "dropped": L.inum(L.field(ln, "dropped")),
                "printed": L.inum(L.field(ln, "printed")),
            }
            continue
        end, wall = L.inum(L.field(ln, "end_unix_us")), L.inum(L.field(ln, "wall_us"))
        task = L.field(ln, "task")
        if end is None or wall is None or task is None:
            continue
        polls.append({
            "task": task, "end": end, "wall": wall,
            "cpu": L.inum(L.field(ln, "cpu_us")),
            "vcsw": L.inum(L.field(ln, "vcsw")), "ivcsw": L.inum(L.field(ln, "ivcsw")),
        })
    return polls, summary


def parse_stat(path):
    """-> list of (t_epoch, {cpu_name: [jiffies...]}) snapshots (vCPU lines
    only, the aggregate `cpu` line excluded)."""
    snaps, cur, t = [], None, None
    try:
        with open(path, encoding="utf-8", errors="replace") as f:
            for ln in f:
                p = ln.split()
                if not p:
                    continue
                if p[0] == "T" and len(p) >= 2:
                    if cur is not None and t is not None:
                        snaps.append((t, cur))
                    t = L.fnum(p[1])
                    cur = {}
                elif p[0].startswith("cpu") and p[0] != "cpu" and cur is not None:
                    try:
                        cur[p[0]] = [int(x) for x in p[1:]]
                    except ValueError:
                        pass
    except OSError:
        return []
    if cur is not None and t is not None:
        snaps.append((t, cur))
    return snaps


def stat_intervals(snaps):
    """Per consecutive-snapshot interval: dict(t0, t1, steal, total, busy).
    steal/total are jiffies summed over vCPUs; busy = vCPUs whose
    (idle + iowait) share of the interval is < 0.5."""
    out = []
    for (t0, a), (t1, b) in zip(snaps, snaps[1:]):
        steal = total = busy = 0
        for cpu, vb in b.items():
            va = a.get(cpu)
            if va is None or len(vb) < 5 or len(va) != len(vb):
                continue
            d = [max(0, y - x) for x, y in zip(va, vb)]
            tot = sum(d[:8])          # user..steal (guest is already inside user)
            steal += d[7] if len(d) > 7 else 0
            total += tot
            if tot > 0 and (d[3] + d[4]) / tot < 0.5:
                busy += 1
        out.append({"t0": t0, "t1": t1, "steal": steal, "total": total, "busy": busy})
    return out


# ── per-side analysis ────────────────────────────────────────────────────

def late_ticks(raw):
    """The late ticks of the xfer window: dicts(at, late, excess, t_end, t_start)
    (times in unix us), plus the floor. ([], None) with no samples."""
    if raw is None or not raw["xfer"]:
        return [], None
    a, b = raw["xfer"]
    xs = [(at, late) for at, late in raw["samples"] if a <= at <= b]
    if not xs:
        return [], None
    floor = min(l for _, l in xs)
    out = []
    for at, late in xs:
        ex = late - floor
        if ex >= LATE_US:
            t_end = raw["origin"] + at
            out.append({"at": at, "late": late, "excess": ex, "t_end": t_end, "t_start": t_end - ex})
    return out, floor


def intersect(a0, a1, b0, b1):
    return max(0, min(a1, b1) - max(a0, b0))


def attribute(ticks, polls):
    """Mark each tick overlapped/credited. A tick is OVERLAPPED if some long
    poll intersects its stall interval by >= OVERLAP_US; it is credited to the
    task with the largest intersection (ties: first by task name)."""
    for t in ticks:
        best, who = 0, None
        for p in polls:
            x = intersect(t["t_start"], t["t_end"], p["end"] - p["wall"], p["end"])
            if x > best or (x == best and who is not None and x > 0 and p["task"] < who):
                best, who = x, p["task"]
        t["overlapped"] = best >= OVERLAP_US
        t["task"] = who if t["overlapped"] else None
    return ticks


def lag_stats(lines):
    ln = next((x for x in tagged(lines, "[LAG] phase=xfer")), None)
    if ln is None:
        return None
    d = {k: L.fnum(L.field(ln, k + "_us")) for k in ("p50", "p99", "max", "min")}
    d["n"] = L.inum(L.field(ln, "n"))
    d["E"] = d["p99"] - d["min"] if d["p99"] is not None and d["min"] is not None else None
    return d


def task_stats(lines):
    asl_inv = asl_none = 0
    maxpoll, seen = {}, 0
    for ln in tagged(lines, "[TASK] phase=xfer"):
        seen += 1
        asl_inv += L.inum(L.field(ln, "asl_inv_us")) or 0
        asl_none += L.inum(L.field(ln, "asl_none_us")) or 0
        mp = L.inum(L.field(ln, "max_poll_us"))
        if mp is not None:
            maxpoll[L.field(ln, "task")] = mp
    return {"lines": seen, "asl_inv_us": asl_inv, "asl_none_us": asl_none, "max_poll_us": maxpoll}


def analyze_side(log_path, intervals):
    lines = L.read(log_path)
    s = {"lag": lag_stats(lines), "tasks": task_stats(lines)}
    raws = tagged(lines, "[LAGRAW] ")
    raw = parse_lagraw(raws[0]) if raws else None
    polls, summ = parse_lpoll(lines)
    s["raw"], s["lpoll_summary"], s["polls"] = raw, summ, polls
    ticks, floor = late_ticks(raw)
    attribute(ticks, polls)
    s["floor"], s["ticks"] = floor, ticks
    s["late_n"] = len(ticks)
    s["ovl_n"] = sum(1 for t in ticks if t["overlapped"])
    s["F"] = s["ovl_n"] / len(ticks) if ticks else None
    s["by_task"] = {}
    for t in ticks:
        if t["task"]:
            s["by_task"][t["task"]] = s["by_task"].get(t["task"], 0) + 1
    # long-poll wall per second over the xfer window (by poll end)
    s["lpw_s"], s["lpw_task"] = None, {}
    s["steal_frac"] = s["base_n"] = s["base_steal"] = s["base_busy"] = None
    s["cond_n"] = s["cond_steal"] = s["cond_busy"] = 0
    if raw and raw["xfer"]:
        a, b = raw["xfer"]
        u0, u1 = raw["origin"] + a, raw["origin"] + b
        secs = max(1e-9, (b - a) / 1e6)
        tot = 0
        for p in polls:
            if u0 <= p["end"] <= u1:
                tot += p["wall"]
                s["lpw_task"][p["task"]] = s["lpw_task"].get(p["task"], 0) + p["wall"] / 1e6 / secs
        s["lpw_s"] = tot / 1e6 / secs
        win = [iv for iv in intervals if iv["t1"] * 1e6 > u0 and iv["t0"] * 1e6 < u1]
        if win:
            den = sum(iv["total"] for iv in win)
            s["steal_frac"] = sum(iv["steal"] for iv in win) / den if den else None
            s["base_n"] = len(win)
            s["base_steal"] = sum(1 for iv in win if iv["steal"] > 0)
            s["base_busy"] = sum(1 for iv in win if iv["busy"] >= BUSY_MIN)
            for t in ticks:
                if t["overlapped"]:
                    continue
                iv = next((x for x in win if x["t0"] * 1e6 <= t["t_end"] < x["t1"] * 1e6), None)
                if iv is None:
                    continue
                s["cond_n"] += 1
                s["cond_steal"] += iv["steal"] > 0
                s["cond_busy"] += iv["busy"] >= BUSY_MIN
    return s


def load_row(prefix):
    tag = os.path.basename(prefix)
    m = re.search(r"-rt(\d)$", tag)
    snaps = parse_stat(prefix + "-stat.txt")
    iv = stat_intervals(snaps)
    row = {
        "tag": tag, "rt": int(m.group(1)) if m else None,
        "stat_samples": len(snaps),
        "client": analyze_side(prefix + "-c.log", iv),
        "server": analyze_side(prefix + "-s.log", iv),
    }
    row["witness"] = witness(row)
    return row


def witness(row):
    """None when live, else the reason (WITNESS-FAIL)."""
    c = row["client"]
    if c["lag"] is None or c["lag"]["min"] is None:
        return "client [LAG] xfer line / min_us missing"
    if row["stat_samples"] < STAT_MIN_SAMPLES:
        return f"stat samples {row['stat_samples']} < {STAT_MIN_SAMPLES}"
    if row["rt"] == 2:
        if c["raw"] is None:
            return "client [LAGRAW] missing"
        if c["lpoll_summary"] is None:
            return "client [LPOLL] summary missing"
    return None


# ── smoke ────────────────────────────────────────────────────────────────

def smoke(prefix):
    row = load_row(prefix)
    checks = []
    for side in ("client", "server"):
        s = row[side]
        checks.append((f"{side} [LAGRAW]", s["raw"] is not None))
        checks.append((f"{side} [LPOLL] summary", s["lpoll_summary"] is not None))
        checks.append((f"{side} [TASK] max_poll_us", bool(s["tasks"]["max_poll_us"])))
    c = row["client"]
    checks.append(("client [LAG] xfer min_us", c["lag"] is not None and c["lag"]["min"] is not None))
    checks.append((f"stat samples >= {STAT_MIN_SAMPLES} ({row['stat_samples']})", row["stat_samples"] >= STAT_MIN_SAMPLES))
    ok = all(v for _, v in checks)
    for name, v in checks:
        print(("ok   " if v else "FAIL ") + name)
    print("SMOKE-PASS" if ok else "SMOKE-FAIL")
    return ok


# ── digest ───────────────────────────────────────────────────────────────

def spearman(xs, ys):
    """Spearman rho (average ranks); None if undefined (n < 3 or constant)."""
    def ranks(v):
        order = sorted(range(len(v)), key=lambda i: v[i])
        r = [0.0] * len(v)
        i = 0
        while i < len(order):
            j = i
            while j + 1 < len(order) and v[order[j + 1]] == v[order[i]]:
                j += 1
            for k in range(i, j + 1):
                r[order[k]] = (i + j) / 2 + 1
            i = j + 1
        return r
    if len(xs) < 3:
        return None
    rx, ry = ranks(xs), ranks(ys)
    mx, my = sum(rx) / len(rx), sum(ry) / len(ry)
    sxx = sum((a - mx) ** 2 for a in rx)
    syy = sum((b - my) ** 2 for b in ry)
    if sxx == 0 or syy == 0:
        return None
    return sum((a - mx) * (b - my) for a, b in zip(rx, ry)) / math.sqrt(sxx * syy)


def med(v):
    v = [x for x in v if x is not None]
    return L.med(v) if v else None


def ratio(hi, lo):
    """hi / lo; lo == 0 gives inf when hi > 0 (else 0)."""
    if hi is None or lo is None:
        return None
    if lo == 0:
        return math.inf if hi > 0 else 0.0
    return hi / lo


def fmt(x, nd=3):
    if x is None:
        return "-"
    if isinstance(x, float):
        return "inf" if math.isinf(x) else f"{x:.{nd}f}".rstrip("0").rstrip(".") if nd else f"{x:.0f}"
    return str(x)


def pct(n, d):
    return None if not d else n / d


def terciles(rows):
    """(high, low) groups of the live RTOBS=2 rows by client E; ties by tag."""
    rs = sorted((r for r in rows if r["client"]["lag"]["E"] is not None),
                key=lambda r: (r["client"]["lag"]["E"], r["tag"]))
    k = len(rs) // 3
    if k == 0:
        return [], []
    return rs[-k:], rs[:k]


def decide(live2, n_witness_fail):
    """The pre-registered outcome rule. Returns (outcome, evidence dict)."""
    ev = {}
    if len(live2) < MIN_LIVE_RT2:
        return "UNSETTLED-INSTRUMENT", ev
    late = sum(r["client"]["late_n"] for r in live2)
    ovl = sum(r["client"]["ovl_n"] for r in live2)
    F = ovl / late if late else None
    hi, lo = terciles(live2)
    lpw_hi = med([r["client"]["lpw_s"] for r in hi])
    lpw_lo = med([r["client"]["lpw_s"] for r in lo])
    st_hi = med([r["client"]["steal_frac"] for r in hi])
    st_lo = med([r["client"]["steal_frac"] for r in lo])
    by_task = {}
    for r in live2:
        for t, n in r["client"]["by_task"].items():
            by_task[t] = by_task.get(t, 0) + n
    cn = sum(r["client"]["cond_n"] for r in live2)
    base_n = sum(r["client"]["base_n"] or 0 for r in live2)
    p_steal_base = pct(sum(r["client"]["base_steal"] or 0 for r in live2), base_n)
    p_busy_base = pct(sum(r["client"]["base_busy"] or 0 for r in live2), base_n)
    p_steal_cond = pct(sum(r["client"]["cond_steal"] for r in live2), cn)
    p_busy_cond = pct(sum(r["client"]["cond_busy"] for r in live2), cn)
    lp_ratio = ratio(lpw_hi, lpw_lo)
    ev.update(late=late, overlapped=ovl, F=F, lpw_hi=lpw_hi, lpw_lo=lpw_lo, lpw_ratio=lp_ratio,
              steal_hi=st_hi, steal_lo=st_lo, p_steal_base=p_steal_base, p_steal_cond=p_steal_cond,
              p_busy_base=p_busy_base, p_busy_cond=p_busy_cond, cond_n=cn, by_task=by_task)
    if F is None:
        return "UNSETTLED", ev
    # 1. in-process
    if F >= 0.5 and lp_ratio is not None and lp_ratio >= 1.5:
        ranked = sorted(by_task.items(), key=lambda kv: (-kv[1], kv[0]))
        if ranked and ranked[0][1] >= 0.5 * ovl:
            return f"IN-PROCESS({ranked[0][0]})", ev
        return "IN-PROCESS(MIXED:" + ",".join(t for t, _ in ranked[:2]) + ")", ev
    if F < 0.5:
        # 2. VM steal
        by_cond = (p_steal_cond is not None and p_steal_base is not None
                   and p_steal_cond > 0 and p_steal_cond >= 2 * p_steal_base)
        by_tercile = (st_hi is not None and st_lo is not None and st_hi > 0 and st_hi >= 2 * st_lo)
        if by_cond or by_tercile:
            return "VM-STEAL", ev
        # 3. guest CPU
        if (p_busy_cond is not None and p_busy_base is not None
                and p_busy_cond > 0 and p_busy_cond >= 2 * p_busy_base):
            return "GUEST-CPU", ev
    return "UNSETTLED", ev


def digest(d):
    prefixes = sorted(p[: -len("-c.log")] for p in glob.glob(os.path.join(d, "*-c.log")))
    rows = [load_row(p) for p in prefixes if not os.path.basename(p).startswith("smoke")]
    live = [r for r in rows if r["witness"] is None]
    dead = [r for r in rows if r["witness"] is not None]
    live2 = [r for r in live if r["rt"] == 2]
    live1 = [r for r in live if r["rt"] == 1]

    print("== per-row table (client primary; E = p99 - min, us) ==")
    print("tag rt n p50 p99 min max E | late ovl F | lpw/s steal_frac pBusy>=5 | srv_E srv_F | asl_inv asl_none | status")
    for r in rows:
        c, s = r["client"], r["server"]
        lg = c["lag"] or {}
        pb = pct(c["base_busy"] or 0, c["base_n"]) if c["base_n"] else None
        print(" ".join([
            r["tag"], f"rt{r['rt']}", fmt(lg.get("n")), fmt(lg.get("p50"), 0), fmt(lg.get("p99"), 0),
            fmt(lg.get("min"), 0), fmt(lg.get("max"), 0), fmt(lg.get("E"), 0), "|",
            fmt(c["late_n"]), fmt(c["ovl_n"]), fmt(c["F"], 2), "|",
            fmt(c["lpw_s"], 4), fmt(c["steal_frac"], 4), fmt(pb, 2), "|",
            fmt((s["lag"] or {}).get("E"), 0), fmt(s["F"], 2), "|",
            fmt(c["tasks"]["asl_inv_us"]), fmt(c["tasks"]["asl_none_us"]), "|",
            "WITNESS-FAIL(" + r["witness"] + ")" if r["witness"] else "live",
        ]))
    print(f"rows={len(rows)} live={len(live)} live_rt2={len(live2)} live_rt1={len(live1)} witness_fail={len(dead)}")

    outcome, ev = decide(live2, len(dead))
    print("== pooled over live RTOBS=2 rows (client) ==")
    for k in ("late", "overlapped", "F", "lpw_hi", "lpw_lo", "lpw_ratio", "steal_hi", "steal_lo",
              "p_steal_base", "p_steal_cond", "p_busy_base", "p_busy_cond", "cond_n"):
        if k in ev:
            print(f"{k}={fmt(ev[k], 4)}")
    hi, lo = terciles(live2)
    print(f"== tercile split by client E: high={len(hi)} rows, low={len(lo)} rows ==")
    for name, g in (("high", hi), ("low", lo)):
        print(f"{name}: E_med={fmt(med([r['client']['lag']['E'] for r in g]), 0)} "
              f"lpw_s_med={fmt(med([r['client']['lpw_s'] for r in g]), 4)} "
              f"steal_frac_med={fmt(med([r['client']['steal_frac'] for r in g]), 5)} "
              f"pBusy5_med={fmt(med([pct(r['client']['base_busy'] or 0, r['client']['base_n']) for r in g]), 3)}")
    print("== per-task share of overlapped late ticks ==")
    bt = ev.get("by_task", {})
    tot = sum(bt.values())
    for t, n in sorted(bt.items(), key=lambda kv: (-kv[1], kv[0])):
        print(f"task={t} overlapped={n} share={fmt(pct(n, tot), 3)}")
    if not bt:
        print("(none)")
    print("== max_poll_us per task (run-cumulative, client, max over live RTOBS=2 rows) ==")
    mp = {}
    for r in live2:
        for t, v in r["client"]["tasks"]["max_poll_us"].items():
            mp[t] = max(mp.get(t, 0), v)
    for t, v in sorted(mp.items(), key=lambda kv: (-kv[1], kv[0])):
        print(f"task={t} max_poll_us={v}")

    e2, e1 = med([r["client"]["lag"]["E"] for r in live2]), med([r["client"]["lag"]["E"] for r in live1])
    p2, p1 = med([r["client"]["lag"]["p99"] for r in live2]), med([r["client"]["lag"]["p99"] for r in live1])
    print(f"RTOBS-COMPARE client E_med rt1={fmt(e1, 0)} rt2={fmt(e2, 0)} p99_med rt1={fmt(p1, 0)} rt2={fmt(p2, 0)} "
          f"(n rt1={len(live1)} rt2={len(live2)})")

    mins = [r["client"]["lag"]["min"] for r in live]
    p50s = [r["client"]["lag"]["p50"] for r in live]
    rho = spearman(mins, p50s)
    if mins:
        print(f"PHASE-OFFSET client min_us min={fmt(min(mins), 0)} max={fmt(max(mins), 0)} "
              f"spread={fmt(max(mins) - min(mins), 0)} spearman_rho(min_us,p50)={fmt(rho, 3)} "
              f"n={len(mins)} (finding A predicts a wide spread and rho >= 0.6)")
    else:
        print("PHASE-OFFSET no live rows")
    pr = ratio(e2, e1)
    flag = "PERTURBING" if pr is not None and pr > 1.5 else "ok"
    print(f"PERTURB E_med(rt2)/E_med(rt1)={fmt(pr, 3)} {flag}")

    print(f"WITNESS-FAIL rows excluded: {len(dead)}")
    for r in dead:
        print(f"  {r['tag']}: {r['witness']}")
    print(f"OUTCOME {outcome}")
    return 0


def main(argv):
    if len(argv) == 3 and argv[1] == "smoke":
        return 0 if smoke(argv[2]) else 1
    if len(argv) == 3 and argv[1] == "digest":
        return digest(argv[2])
    print(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
