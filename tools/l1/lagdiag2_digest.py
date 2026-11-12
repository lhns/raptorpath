#!/usr/bin/env python3
"""Digest of the lagdiag2 battery (lagdiag2_run.sh), round 2 of the c8-100
client [LAG] tail diagnosis:

    lagdiag2_digest.py smoke  <prefix>   # witness check of one row; exit 0/1
    lagdiag2_digest.py digest <dir>      # tables, pooled numbers, TWO OUTCOME lines

Row files are those of lagdiag_digest.py (<tag>-c.log, -s.log, -stat.txt; tags
`b<b>-s<seed>-rt<rtobs>`); the client is primary and only live RTOBS=2 rows
enter the outcomes. The round-1 helpers (parsers, Spearman, ratio) are imported
from lagdiag_digest.

(a) IS THE [LAG] TAIL SEEN BY AN OS REFERENCE CLOCK? E_lag = [LAG] xfer p99 -
    min_us, E_os = [OSLAG] xfer p99 - min_us (the same net-of-offset reading
    on the plain-OS-thread CLOCK_MONOTONIC clock). rho = Spearman(E_lag,
    E_os) over live rows; terciles by E_lag with k = live // 3, ratio = median
    E_os of the high tercile over the low one (0 low -> inf).
      OUTCOME-A OS-LEVEL   iff rho >= 0.5 AND ratio >= 1.5
      OUTCOME-A RUNTIME    iff rho <  0.3 AND ratio <  1.2
      OUTCOME-A MIXED      otherwise (an undefined rho satisfies neither)
      OUTCOME-A UNSETTLED-INSTRUMENT   with < 18 live rows
    Also reported: rho(E_lag, run_delay/s) and rho(E_os, run_delay/s), the
    worker threads' run-queue delay per second ([THR] os run_delay_us).

(b) WHERE DO THE SENDER'S LONG POLLS GO? Over every client task=sender [LPOLL]
    entry of the live rows: polls >= 10 ms when there are at least 20 of them,
    else all (>= 1 ms) and the line says so. I = iterations started in the
    poll (iters=), wall shares from secs= plus other=.
      OUTCOME-B UNBOUNDED-PER-WAKE(<top>)  iff p90(I) >= 2 (several loop
                                           iterations per wake, no yield);
                                           <top> = the section with the
                                           largest wall share (not `other`)
      OUTCOME-B LONG-ITERATION(<sec>)      iff p90(I) <= 1 and one section
                                           holds >= 50 % of the wall
      OUTCOME-B UNATTRIBUTED               iff `other` >= 50 % of the wall
      OUTCOME-B MIXED                      otherwise
      OUTCOME-B UNSETTLED-INSTRUMENT       with no sender poll
    (checked in that order).
"""
import glob
import math
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import l1common as L  # noqa: E402
import lagdiag_digest as D  # noqa: E402

MIN_LIVE_RT2 = D.MIN_LIVE_RT2          # 18
RHO_OS, RATIO_OS = 0.5, 1.5            # OS-LEVEL
RHO_RT, RATIO_RT = 0.3, 1.2            # RUNTIME
MIN_BIG_POLLS = 20
BIG_POLL_US = 10_000
SHARE_BAR = 0.5
UNBOUNDED_P90 = 2.0
LONG_ITER_P90 = 1.0

_SEC_TOK = re.compile(r"(?:^|\s)([a-z_]+)=(\d+)/(\d+)/(\d+)(?=\s|$)")


# ── parsing ──────────────────────────────────────────────────────────────

def _window_groups(lines, tag):
    """{window key: [lines]} of the `phase=xfer` lines carrying `tag`."""
    out = {}
    for ln in D.tagged(lines, tag):
        if L.field(ln, "phase") == "xfer":
            out.setdefault(L._window_of(ln), []).append(ln)
    return out


def parse_oslag(lines):
    """-> dict(n, p50, p99, max, min, skipped, E) of the first xfer [OSLAG]
    line, or None."""
    ln = next(iter(D.tagged(lines, "[OSLAG] phase=xfer")), None)
    if ln is None:
        return None
    d = {k: L.fnum(L.field(ln, k + "_us")) for k in ("p50", "p99", "max", "min")}
    d["n"] = L.inum(L.field(ln, "n"))
    d["skipped"] = L.inum(L.field(ln, "skipped"))
    d["E"] = d["p99"] - d["min"] if d["p99"] is not None and d["min"] is not None else None
    return d


def parse_thr_delay(lines):
    """-> dict(os_lines, with_delay, wall_s, worker_delay_us, per_s) over the
    xfer window with the longest [THR] os wall (the measured object).
    worker_delay_us sums run_delay_us over the rp-w-* threads."""
    groups = _window_groups([ln for ln in lines if "[THR] os " in ln and "unavailable" not in ln], "[THR] os ")
    if not groups:
        return {"os_lines": 0, "with_delay": 0, "wall_s": None, "worker_delay_us": None, "per_s": None}
    key = max(groups, key=lambda k: max(L.fnum(L.field(x, "wall_s")) or 0 for x in groups[k]))
    ls = groups[key]
    wall = max(L.fnum(L.field(x, "wall_s")) or 0 for x in ls)
    # The token must be present (`-` = unreadable thread, e.g. one that
    # exited mid-read); the sum is over the readable worker threads.
    with_delay = [x for x in ls if " run_delay_us=" in x]
    tot = None
    for x in with_delay:
        v = L.inum(L.field(x, "run_delay_us"))
        if v is not None and (L.field(x, "comm") or "").startswith("rp-w-"):
            tot = (tot or 0) + v
    return {"os_lines": len(ls), "with_delay": len(with_delay), "wall_s": wall,
            "worker_delay_us": tot, "per_s": tot / wall if tot is not None and wall > 0 else None}


def parse_txsec(lines):
    """-> dict(iters, secs={name: (ns_us, n, max_us)}, awaits=[names]) of the
    xfer [TXSEC] line with the longest wall_s, or None."""
    groups = _window_groups(lines, "[TXSEC] ")
    if not groups:
        return None
    ln = max((x for ls in groups.values() for x in ls), key=lambda x: L.fnum(L.field(x, "wall_s")) or 0)
    secs = {m.group(1): (int(m.group(2)), int(m.group(3)), int(m.group(4))) for m in _SEC_TOK.finditer(ln)}
    aw = L.field(ln, "await")
    return {"iters": L.inum(L.field(ln, "iters")), "secs": secs, "awaits": aw.split(",") if aw else []}


def parse_secs(tok):
    """`in:0,pre:3000` -> {name: us}; None if absent or malformed."""
    if not tok:
        return None
    out = {}
    for part in tok.split(","):
        name, _, v = part.partition(":")
        n = L.inum(v)
        if not name or n is None:
            return None
        out[name] = n
    return out


def parse_sender_polls(lines):
    """Every task=sender [LPOLL] entry: dict(end, wall, iters, secs, other);
    iters / secs / other are None on an entry that lacks them."""
    out = []
    for ln in D.tagged(lines, "[LPOLL] "):
        if " summary " in ln or L.field(ln, "task") != "sender":
            continue
        wall = L.inum(L.field(ln, "wall_us"))
        if wall is None:
            continue
        out.append({"end": L.inum(L.field(ln, "end_unix_us")), "wall": wall,
                    "iters": L.inum(L.field(ln, "iters")),
                    "secs": parse_secs(L.field(ln, "secs")),
                    "other": L.inum(L.field(ln, "other"))})
    return out


def load_row(prefix):
    """The round-1 row (client / server, stat) plus the round-2 client data."""
    row = D.load_row(prefix)
    lines = L.read(prefix + "-c.log")
    row["c2"] = {
        "oslag": parse_oslag(lines),
        "oslagraw": next(iter(D.tagged(lines, "[OSLAGRAW] ")), None) is not None,
        "thr": parse_thr_delay(lines),
        "txsec": parse_txsec(lines),
        "sender": parse_sender_polls(lines),
    }
    row["io_avg10_max"] = io_full_avg10_max(prefix + "-stat.txt")
    row["witness"] = witness(row)
    return row


# Status §26 amendment 3 (supersedes amendment 1's void): `/proc/pressure/io`
# `full avg10` is a covariate; a row that reached IOSTALL_AVG10 at any 10 Hz
# sample is FLAGGED when its E_lag is an outlier (`io_flags`), never voided.
IOSTALL_AVG10 = 20.0


def io_full_avg10_max(path):
    """Max `full avg10` over the stat file's `IO full ...` lines; None if none."""
    best = None
    try:
        with open(path, encoding="utf-8", errors="replace") as f:
            for ln in f:
                if not ln.startswith("IO "):
                    continue
                for tok in ln.split():
                    if tok.startswith("avg10="):
                        v = L.fnum(tok[6:])
                        if v is not None and (best is None or v > best):
                            best = v
    except OSError:
        return None
    return best


def witness(row):
    """None when live, else the reason (WITNESS-FAIL). RTOBS=2 rows need the
    round-1 witnesses and every round-2 instrument; RTOBS=1 rows only the
    round-1 ones (they print none of the deep gauges)."""
    w = D.witness(row)
    if w or row["rt"] != 2:
        return w
    c2 = row["c2"]
    os_ = c2["oslag"]
    if os_ is None or os_["min"] is None or os_["p99"] is None:
        return "client [OSLAG] xfer line / min_us missing"
    if not c2["oslagraw"]:
        return "client [OSLAGRAW] missing"
    tx = c2["txsec"]
    if tx is None or not tx["iters"]:
        return "client [TXSEC] xfer missing or iters=0"
    sp = c2["sender"]
    if not sp:
        return "client [LPOLL] has no task=sender entry"
    if any(p["iters"] is None or p["secs"] is None for p in sp):
        return "client [LPOLL] task=sender entry without iters=/secs="
    th = c2["thr"]
    if th["os_lines"] == 0 or th["with_delay"] != th["os_lines"]:
        return "client [THR] os lines without run_delay_us="
    if th["worker_delay_us"] is None:
        return "client [THR] run_delay_us unreadable on every rp-w-* thread"
    return None


# ── smoke ────────────────────────────────────────────────────────────────

def smoke(prefix):
    row = load_row(prefix)
    c, c2 = row["client"], row["c2"]
    sp = c2["sender"]
    th, os_, tx = c2["thr"], c2["oslag"], c2["txsec"]
    checks = []
    for side in ("client", "server"):
        s = row[side]
        checks.append((f"{side} [LAGRAW]", s["raw"] is not None))
        checks.append((f"{side} [LPOLL] summary", s["lpoll_summary"] is not None))
        checks.append((f"{side} [TASK] max_poll_us", bool(s["tasks"]["max_poll_us"])))
    checks.append(("client [LAG] xfer min_us", c["lag"] is not None and c["lag"]["min"] is not None))
    checks.append((f"stat samples >= {D.STAT_MIN_SAMPLES} ({row['stat_samples']})", row["stat_samples"] >= D.STAT_MIN_SAMPLES))
    checks.append(("client [OSLAG] xfer with min_us", os_ is not None and os_["min"] is not None))
    checks.append(("client [OSLAGRAW]", c2["oslagraw"]))
    checks.append(("client [TXSEC] iters>0", tx is not None and bool(tx["iters"])))
    checks.append((f"client [LPOLL] task=sender entries ({len(sp)}) carry iters= and secs=",
                   bool(sp) and all(p["iters"] is not None and p["secs"] is not None for p in sp)))
    checks.append((f"client [THR] os lines ({th['os_lines']}) carry run_delay_us=",
                   th["os_lines"] > 0 and th["with_delay"] == th["os_lines"]))
    ok = all(v for _, v in checks)
    for name, v in checks:
        print(("ok   " if v else "FAIL ") + name)
    print("SMOKE-PASS" if ok else "SMOKE-FAIL")
    return ok


# ── outcome A ────────────────────────────────────────────────────────────

def e_lag(r):
    return (r["client"]["lag"] or {}).get("E")


def e_os(r):
    return (r["c2"]["oslag"] or {}).get("E")


def rd(r):
    return r["c2"]["thr"]["per_s"]


def decide_a(live2):
    """-> (outcome, evidence). live2: the live RTOBS=2 rows."""
    ev = {"live": len(live2)}
    if len(live2) < MIN_LIVE_RT2:
        return "UNSETTLED-INSTRUMENT", ev
    rows = [r for r in live2 if e_lag(r) is not None and e_os(r) is not None]
    rho = D.spearman([e_lag(r) for r in rows], [e_os(r) for r in rows])
    srt = sorted(rows, key=lambda r: (e_lag(r), r["tag"]))
    k = len(live2) // 3
    hi, lo = (srt[-k:], srt[:k]) if k and len(srt) >= 2 * k else ([], [])
    m_hi = D.med([e_os(r) for r in hi])
    m_lo = D.med([e_os(r) for r in lo])
    ratio = D.ratio(m_hi, m_lo)
    rdr = [r for r in rows if rd(r) is not None]
    ev.update(rho=rho, ratio=ratio, e_os_hi=m_hi, e_os_lo=m_lo, k=k, hi=hi, lo=lo,
              rho_lag_rd=D.spearman([e_lag(r) for r in rdr], [rd(r) for r in rdr]),
              rho_os_rd=D.spearman([e_os(r) for r in rdr], [rd(r) for r in rdr]),
              n_rd=len(rdr))
    if rho is not None and ratio is not None and rho >= RHO_OS and ratio >= RATIO_OS:
        return "OS-LEVEL", ev
    if rho is not None and ratio is not None and rho < RHO_RT and ratio < RATIO_RT:
        return "RUNTIME", ev
    return "MIXED", ev


# ── outcome B ────────────────────────────────────────────────────────────

def decide_b(live2):
    """-> (outcome, evidence). The polls are the live rows' client sender polls."""
    polls = [p for r in live2 for p in r["c2"]["sender"]]
    ev = {"n_all": len(polls)}
    if not polls:
        return "UNSETTLED-INSTRUMENT", ev
    big = [p for p in polls if p["wall"] >= BIG_POLL_US]
    use_big = len(big) >= MIN_BIG_POLLS
    chosen = big if use_big else polls
    ev.update(n_big=len(big), basis=">=10ms" if use_big else ">=1ms (fewer than %d polls >= 10 ms)" % MIN_BIG_POLLS)

    def iters_q(ps):
        v = [p["iters"] for p in ps]
        return (len(ps), L.q(v, 0.5), L.q(v, 0.9))

    ev["iters_all"], ev["iters_big"] = iters_q(polls), iters_q(big)
    ev["shares_all"], ev["shares_big"] = shares(polls), shares(big)
    ev["shares"] = ev["shares_big"] if use_big else ev["shares_all"]
    p90 = L.q([p["iters"] for p in chosen], 0.9)
    ev["p90"] = p90
    named = {k: v for k, v in ev["shares"].items() if k != "other"}
    top = max(named.items(), key=lambda kv: (kv[1], kv[0]))[0] if named else "-"
    top_share = named.get(top, 0.0)
    other = ev["shares"].get("other", 0.0)
    ev.update(top=top, top_share=top_share, other=other)
    if p90 >= UNBOUNDED_P90:
        return f"UNBOUNDED-PER-WAKE({top})", ev
    if p90 <= LONG_ITER_P90 and top_share >= SHARE_BAR:
        return f"LONG-ITERATION({top})", ev
    if other >= SHARE_BAR:
        return "UNATTRIBUTED", ev
    return "MIXED", ev


def shares(polls):
    """Wall-weighted section shares: per section the sum over the polls of its
    us, over the sum of every section plus `other` (= the polls' wall)."""
    tot, den = {}, 0
    for p in polls:
        for k, v in p["secs"].items():
            tot[k] = tot.get(k, 0) + v
            den += v
        tot["other"] = tot.get("other", 0) + (p["other"] or 0)
        den += p["other"] or 0
    return {k: v / den for k, v in tot.items()} if den else {}


# ── digest ───────────────────────────────────────────────────────────────

def fmt(x, nd=3):
    return D.fmt(x, nd)


def digest(d):
    prefixes = sorted(p[: -len("-c.log")] for p in glob.glob(os.path.join(d, "*-c.log")))
    rows = [load_row(p) for p in prefixes if not os.path.basename(p).startswith("smoke")]
    live = [r for r in rows if r["witness"] is None]
    dead = [r for r in rows if r["witness"] is not None]
    live2 = [r for r in live if r["rt"] == 2]
    live1 = [r for r in live if r["rt"] == 1]

    print("== per-row table (client; E = p99 - min, us) ==")
    print("tag rt lag_min lag_p99 E_lag | os_n os_min os_p99 E_os os_skip | rd_us/s | sender_polls txsec_iters | status")
    for r in rows:
        lg = r["client"]["lag"] or {}
        os_ = r["c2"]["oslag"] or {}
        tx = r["c2"]["txsec"] or {}
        print(" ".join([
            r["tag"], f"rt{r['rt']}", fmt(lg.get("min"), 0), fmt(lg.get("p99"), 0), fmt(lg.get("E"), 0), "|",
            fmt(os_.get("n")), fmt(os_.get("min"), 0), fmt(os_.get("p99"), 0), fmt(os_.get("E"), 0),
            fmt(os_.get("skipped")), "|", fmt(rd(r), 0), "|",
            fmt(len(r["c2"]["sender"])), fmt(tx.get("iters")), "|",
            "WITNESS-FAIL(" + r["witness"] + ")" if r["witness"] else "live",
        ]))
    print(f"rows={len(rows)} live={len(live)} live_rt2={len(live2)} live_rt1={len(live1)} witness_fail={len(dead)}")

    # (a)
    out_a, ev = decide_a(live2)
    print("== (a) OS reference clock vs the [LAG] tail (client, live RTOBS=2 rows) ==")
    print(f"live_rt2={ev['live']}")
    if "rho" in ev:
        print(f"spearman_rho(E_lag,E_os)={fmt(ev['rho'])}")
        print(f"terciles by E_lag: k={ev['k']} E_lag_med high={fmt(D.med([e_lag(r) for r in ev['hi']]), 0)} "
              f"low={fmt(D.med([e_lag(r) for r in ev['lo']]), 0)}")
        print(f"E_os_med high={fmt(ev['e_os_hi'], 0)} low={fmt(ev['e_os_lo'], 0)} ratio={fmt(ev['ratio'])}")
        print(f"spearman_rho(E_lag,run_delay_us_per_s)={fmt(ev['rho_lag_rd'])} "
              f"spearman_rho(E_os,run_delay_us_per_s)={fmt(ev['rho_os_rd'])} n={ev['n_rd']}")
    print(f"OUTCOME-A {out_a}")

    # (b)
    out_b, evb = decide_b(live2)
    print("== (b) the sender's long polls (client task=sender [LPOLL], live RTOBS=2 rows) ==")
    print(f"sender_polls={evb['n_all']}")
    if evb["n_all"]:
        for name, key in ((">=1ms", "iters_all"), (">=10ms", "iters_big")):
            n, p50, p90 = evb[key]
            print(f"iters_per_poll {name}: n={n} p50={fmt(p50)} p90={fmt(p90)}")
        for name, key in ((">=1ms", "shares_all"), (">=10ms", "shares_big")):
            sh = evb[key]
            print(f"wall_share {name}: " + (" ".join(f"{k}={v:.3f}" for k, v in sorted(sh.items(), key=lambda kv: (-kv[1], kv[0]))) or "(none)"))
        print(f"outcome basis: polls {evb['basis']}; p90_iters={fmt(evb['p90'])} top={evb['top']}({fmt(evb['top_share'])}) other={fmt(evb['other'])}")
    # pooled [TXSEC] xfer table
    pooled, aw = {}, set()
    for r in live2:
        tx = r["c2"]["txsec"]
        if not tx:
            continue
        aw.update(tx["awaits"])
        for k, (us, n, mx) in tx["secs"].items():
            a = pooled.get(k, [0, 0, 0])
            pooled[k] = [a[0] + us, a[1] + n, max(a[2], mx)]
    print("== [TXSEC] xfer, pooled over the live RTOBS=2 client rows (ns share of all sections) ==")
    tot = sum(v[0] for v in pooled.values())
    for k, (us, n, mx) in sorted(pooled.items(), key=lambda kv: (-kv[1][0], kv[0])):
        print(f"sec={k}{'(await)' if k in aw else ''} us={us} share={fmt(us / tot if tot else None)} n={n} max_us={mx}")
    if not pooled:
        print("(none)")
    print(f"OUTCOME-B {out_b}")

    e2, e1 = D.med([e_lag(r) for r in live2]), D.med([e_lag(r) for r in live1])
    pr = D.ratio(e2, e1)
    print(f"PERTURB E_lag_med(rt2)/E_lag_med(rt1)={fmt(pr)} {'PERTURBING' if pr is not None and pr > 1.5 else 'ok'} "
          f"(n rt1={len(live1)} rt2={len(live2)})")
    print(f"WITNESS-FAIL rows excluded: {len(dead)}")
    for r in dead:
        print(f"  {r['tag']}: {r['witness']}")
    for ln in io_flags(live2):
        print(ln)
    return 0


def io_flags(live2):
    """§26 amendment 3: disk pressure is a covariate, not a void. Report every
    live RTOBS=2 row's max io `full avg10`, and FLAG a row that ran under high
    pressure (>= IOSTALL_AVG10) AND whose client E_lag is a Tukey outlier
    (outside [Q1 - 1.5 IQR, Q3 + 1.5 IQR] of the live RTOBS=2 rows)."""
    es = sorted(e_lag(r) for r in live2 if e_lag(r) is not None)
    out = []
    lo = hi = None
    if len(es) >= 4:
        q1, q3 = L.q(es, 0.25), L.q(es, 0.75)
        lo, hi = q1 - 1.5 * (q3 - q1), q3 + 1.5 * (q3 - q1)
    n_hi = 0
    for r in live2:
        io, e = r.get("io_avg10_max"), e_lag(r)
        high = io is not None and io >= IOSTALL_AVG10
        n_hi += high
        outl = lo is not None and e is not None and (e < lo or e > hi)
        if high and outl:
            out.append(f"IO-FLAG {r['tag']} io_full_avg10_max={io:.2f} E_lag={e:.0f} outside [{lo:.0f},{hi:.0f}]")
    out.insert(0, f"IO-COVARIATE live_rt2_rows_with_io_full_avg10>={IOSTALL_AVG10:g}: {n_hi}; flagged outliers: {len(out)}")
    return out


def main(argv):
    if len(argv) == 3 and argv[1] == "smoke":
        return 0 if smoke(argv[2]) else 1
    if len(argv) == 3 and argv[1] == "digest":
        return digest(argv[2])
    print(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
