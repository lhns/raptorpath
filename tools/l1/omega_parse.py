#!/usr/bin/env python3
"""Pure parsing for the disorder-price (omega) harness: pcap text digest, events, D_e, drops versus spurious, the warm-up witness,
the iperf3 window, and R0's reorderer checks (registration 4.4, 7.1, 7.2, 11.1).

  omega_parse.py digest <meta.json> <mid0.txt> <srv0.txt> <backlog.txt> [iperf.json]     print one ledger row (JSON)
  omega_parse.py warmup <backlog.txt> <B> <win_s> <nofill_s> [t_start]                    print WITNESS <t> | NO_FILL
  omega_parse.py r0 <mode> <dt_ms> <g> <k_expected> <pre.txt> <post.txt> [p|-] [t_switch]   print R0-PASS | R0-FAIL <why>
  omega_parse.py ipid <pcap.txt>                                                          print the share of consecutive per-flow IP ids (R0's selector check)
  omega_parse.py witness <kind> <g> <limit> <mid_before> <mid_after> <bot_before> <bot_after>   JSON of the tc-counter witness ("-" = file absent)
  omega_parse.py tunq <srtt_samples.txt> <iperf.json> [ceiling_mbit]                       JSON: the tunnel's standing queue B_t

Inputs are `tcpdump -nn -tt -v -r` text (a header line with the IP id, then the TCP line) and `<epoch> <backlog_pkts>` samples.
The digest never invents a value: a field it cannot compute is absent from the row, and the scorer reads an absent field as missing.
"""
import json
import math
import random
import re
import statistics
import sys

MIN_FLOW_PKTS = 20
RE_ID = re.compile(r"^(\d+\.\d+) .*?\bid (\d+),")
RE_TCP = re.compile(r"(\d+\.\d+\.\d+\.\d+)\.(\d+) > (\d+\.\d+\.\d+\.\d+)\.(\d+): .*?\bseq (\d+)(?::\d+)?,.*?\blength (\d+)")
RE_UDP = re.compile(r"(\d+\.\d+\.\d+\.\d+)\.(\d+) > (\d+\.\d+\.\d+\.\d+)\.(\d+): (?:\[[^\]]*\] )?UDP, length (\d+)")


def parse_pcap_text(text):
    """-> list of dicts (t, flow, seq, ipid, len) for data-carrying packets, in capture order. Seq is the relative or absolute number tcpdump printed."""
    out, pend = [], None
    for line in text.splitlines():
        m = RE_ID.match(line)
        if m:
            pend = (float(m.group(1)), int(m.group(2)))
            continue
        if pend is None:
            continue
        u = RE_UDP.search(line)
        if u:
            ln = int(u.group(5))
            out.append({"t": pend[0], "flow": (u.group(1), int(u.group(2)), u.group(3), int(u.group(4))), "seq": None, "ipid": pend[1], "len": ln})
            pend = None
            continue
        m = RE_TCP.search(line)
        if m:
            ln = int(m.group(6))
            if ln > 0:
                out.append({"t": pend[0], "flow": (m.group(1), int(m.group(2)), m.group(3), int(m.group(4))), "seq": int(m.group(5)), "ipid": pend[1], "len": ln})
        pend = None
    return out


def keep_data_flows(pkts):
    cnt = {}
    for p in pkts:
        cnt[p["flow"]] = cnt.get(p["flow"], 0) + 1
    return [p for p in pkts if cnt[p["flow"]] >= MIN_FLOW_PKTS]


def read_backlog(text):
    out = []
    for line in text.splitlines():
        f = line.split()
        if len(f) == 2:
            try:
                out.append((float(f[0]), float(f[1])))
            except ValueError:
                pass
    return out


def warmup(samples, B, win_s, nofill_s, t_start=None):
    """The warm-up witness (4.4): the first instant at which the 1 s (win_s) median backlog reaches 0.75 B. NO_FILL if none within nofill_s of t_start."""
    if not samples:
        return None, "NO_DATA"
    t0 = samples[0][0] if t_start is None else t_start
    j = 0
    for i, (t, _) in enumerate(samples):
        if t - t0 > nofill_s:
            break
        while samples[j][0] < t - win_s:
            j += 1
        if t - samples[0][0] >= win_s * 0.999 and statistics.median(v for _, v in samples[j:i + 1]) >= 0.75 * B:
            return t, "WITNESS"
    return None, "NO_FILL"


def backlog_before(samples, t):
    best = None
    for ts, v in samples:
        if ts <= t:
            best = v
        else:
            break
    return best


def goodput(iperf_json, warm_s):
    """(bytes, seconds) of the event phase from `iperf3 -J -i 1` intervals starting at or after warm_s; None if the JSON has none."""
    try:
        j = json.loads(iperf_json)
        iv = [i for i in j["intervals"] if i["sum"]["start"] >= warm_s - 1e-9]
        if not iv:
            return None
        return sum(i["sum"]["bytes"] for i in iv), sum(i["sum"]["seconds"] for i in iv)
    except (ValueError, KeyError, TypeError):
        return None


def match(pre, post):
    """Attach post-arrival times to the pre packets by (flow, seq, ipid). Returns the matched list (pre order) with `t2`; unmatched have t2 None."""
    idx = {}
    for p in post:
        idx.setdefault((p["flow"], p["seq"], p["ipid"]), p["t"])
    out = []
    for p in pre:
        q = dict(p)
        q["t2"] = idx.get((p["flow"], p["seq"], p["ipid"]))
        out.append(q)
    return out


def base_delay(pk):
    d = sorted(p["t2"] - p["t"] for p in pk if p["t2"] is not None)
    return d[len(d) // 2] if d else 0.0


def classify_retx(pre):
    """Per packet: None (new data), 'spurious' (the same seq already reached mid0) or 'drop' (a seq below the flow's frontier that never reached mid0: its original was dropped upstream of the pcap)."""
    seen, front, out = {}, {}, []
    for p in pre:
        s = seen.setdefault(p["flow"], set())
        if p["seq"] is None:
            out.append(None)
            continue
        if p["seq"] in s:
            out.append("spurious")
        elif p["seq"] < front.get(p["flow"], -1):
            out.append("drop")
        else:
            out.append(None)
        s.add(p["seq"])
        front[p["flow"]] = max(front.get(p["flow"], -1), p["seq"])
    return out


def events_of(pk, kind, dt_s, base):
    """Event packets of a matched, time-ordered list. REL: delayed by more than dt/2. HOLD: a delayed packet whose predecessor was not delayed (the head of a stall)."""
    delayed = [p["t2"] is not None and (p["t2"] - p["t"] - base) > dt_s / 2.0 for p in pk]
    ev = []
    for i, p in enumerate(pk):
        if not delayed[i]:
            continue
        if kind == "HOLD" and i > 0 and delayed[i - 1]:
            continue
        ev.append(i)
    return ev, delayed


def overtakers(pk, i):
    """(aggregate, same-flow) successors of packet i that arrived at srv0 before it."""
    a = f = 0
    ti = pk[i]["t2"]
    for j in range(i + 1, len(pk)):
        if pk[j]["t"] - pk[i]["t"] > (ti - pk[i]["t"]):
            break
        t2 = pk[j]["t2"]
        if t2 is not None and t2 < ti:
            a += 1
            f += pk[j]["flow"] == pk[i]["flow"]
    return a, f


def window_cost(pk, retx, t_e, a_len, g):
    """D_e: idle slots + retransmitted segments in [t_e, t_e + a_len), all from the pre-reorderer capture. (idle, retx, drops, spurious)."""
    n = idle_pk = r = dr = sp = 0
    for p, c in zip(pk, retx):
        if p["t"] < t_e:
            continue
        if p["t"] >= t_e + a_len:
            break
        n += 1
        if c:
            r += 1
            dr += c == "drop"
            sp += c == "spurious"
    idle = max(0, int(math.floor(a_len * g)) - n)
    return idle, r, dr, sp


def build_events(pk, retx, idxs, meta, backlog, a_base):
    """Per event record. A_e = [t_e, t_e + dt + 4 RTT + Q/g) cut at the next event (7.1)."""
    g, B = meta["g"], meta["B"]
    a_full = meta["dt_ms"] / 1000.0 + 4 * meta["rtt"] / 1000.0 + meta["limit"] / float(g)
    ev, per_flow = [], {}
    for n, i in enumerate(idxs):
        t_e = pk[i]["t"]
        nxt = pk[idxs[n + 1]]["t"] if n + 1 < len(idxs) else float("inf")
        a_len = min(a_full, nxt - t_e)
        idle, r, dr, sp = window_cost(pk, retx, t_e, a_len, g)
        b = backlog_before(backlog, t_e)
        flow = pk[i]["flow"]
        per_flow[flow] = per_flow.get(flow, 0) + 1
        rec = {"t": round(t_e, 6), "flow": "%s:%d" % (flow[0], flow[1]), "idle": idle, "retx": r, "drops": dr, "spurious": sp, "D": idle + r,
               "a_len_s": round(a_len, 6), "rank_in_rep": n + 1, "rank": per_flow[flow]}
        if b is not None:
            rec["valid"] = bool(0.75 * B <= b <= 1.25 * B)
            rec["backlog"] = b
        ev.append(rec)
    return ev


def digest(meta, pre_txt, post_txt, backlog_txt, iperf_txt=None):
    """One ledger row from a rep's captures. `meta`: row identity and parameters (cfg, arm, kind, label, dt_ms, g, rtt, limit, B, ...); the capture
    window starts at meta['t_event'] (the instant the arm's table went live)."""
    row = {k: meta[k] for k in ("cfg", "arm", "kind", "label", "dt_ms", "rep", "seedhalf", "stage", "session", "g", "rtt", "q", "B", "tau_q_ms") if k in meta}
    row["kind"] = "row"
    row["armkind"] = meta["kind"]
    row["status"] = "OK"
    pre = keep_data_flows(parse_pcap_text(pre_txt))
    post = keep_data_flows(parse_pcap_text(post_txt))
    backlog = read_backlog(backlog_txt)
    wit = dict(meta.get("witness", {}))
    row["witness"] = wit
    miss = witness_missing(meta["kind"], wit)
    if miss:
        row["witness_missing"] = miss
    if not pre or not post:
        row["status"] = "NO_DATA"
        return row
    t0 = meta["t_event"]
    pre = [p for p in pre if p["t"] >= t0 - 1e-9]
    pk = match(pre, post)
    pk.sort(key=lambda p: p["t"])
    retx = classify_retx(pk)
    base = base_delay(pk)
    kind, dt_s, g = meta["kind"], meta["dt_ms"] / 1000.0, meta["g"]
    window = meta.get("event_s")
    row["n_pkts"] = len(pk)
    if kind in ("REL", "HOLD"):
        idxs, delayed = events_of(pk, kind, dt_s, base)
        ev = build_events(pk, retx, idxs, meta, backlog, base)
        tol = max(50e-6, 0.05 * dt_s)
        ok = sum(abs((pk[i]["t2"] - pk[i]["t"] - base) - dt_s) <= tol for i in idxs)
        wit["events_in_tolerance"] = [ok, len(idxs)]
        wit["delay_ok"] = bool(idxs) and ok >= 0.95 * len(idxs)
        if kind == "REL":
            for rec, i in zip(ev, idxs):
                rec["k_a"], rec["k_f"] = overtakers(pk, i)
        else:
            wit["hold_overtakers"] = sum(overtakers(pk, i)[0] for i in idxs)
        row["events"] = ev
    elif kind == "CLEAN":
        wit["clean_delayed"] = sum(1 for p in pk if p["t2"] is not None and (p["t2"] - p["t"] - base) > 50e-6)
        row["pseudo"] = {}
        for lab, (dtm, spacing) in meta.get("pseudo_dts", {}).items():
            m2 = dict(meta, dt_ms=dtm)
            rnd = random.Random("%s|%s|%s" % (meta["cfg"], meta.get("rep"), lab))
            span = (window or (pk[-1]["t"] - pk[0]["t"]))
            n_ev = max(1, int(span / spacing))
            times = sorted(rnd.uniform(pk[0]["t"], pk[-1]["t"]) for _ in range(n_ev))
            idxs = []
            for tt in times:
                j = next((jj for jj, p in enumerate(pk) if p["t"] >= tt), None)
                if j is not None and (not idxs or j != idxs[-1]):
                    idxs.append(j)
            row["pseudo"][lab] = build_events(pk, retx, idxs, m2, backlog, base)
    else:                                  # OUT-REL / OUT-HOLD: one outage per rep; D_e of the window after it, the registered unit
        t_out = meta.get("t_outage", t0)
        a_len = meta["dt_ms"] / 1000.0 + 4 * meta["rtt"] / 1000.0 + meta["limit"] / float(g)
        idle, r, dr, sp = window_cost(pk, retx, t_out, a_len, g)
        row["events"] = [{"t": t_out, "flow": "-", "idle": idle, "retx": r, "drops": dr, "spurious": sp, "D": idle + r, "rank": 1, "rank_in_rep": 1, "valid": True}]
    row["n_events"] = len(row.get("events", []))
    if iperf_txt is not None:
        gp = goodput(iperf_txt, meta.get("warm_s", 0.0))
        if gp:
            row["good_bytes"], row["window_s"] = gp
    return row


WIT_COUNTERS = ("mid1_sent_delta", "cli0_sent_delta")
WIT_FLAGS = ("reorderer_in_force", "bottleneck_in_force", "sysctl_ok", "warm_ok")


def witness_missing(kind, w):
    """Execution-witness fields a row must carry (tc counters moved, qdiscs in force, sysctls held, queue at level); absent ones are 'missing', never false."""
    return [k for k in WIT_COUNTERS + WIT_FLAGS if w.get(k) is None]


def row_void_reason(row):
    """VOID-WITNESS rules of 7.2 on a digested row; None when the witnesses hold."""
    w, k = row.get("witness", {}), row.get("armkind")
    if row.get("witness_missing"):
        return "witness missing: %s" % ",".join(row["witness_missing"])
    for f in WIT_FLAGS:
        if w.get(f) is False:
            return "witness %s does not hold" % f
    for f in WIT_COUNTERS:
        if w.get(f) is not None and w[f] <= 0:
            return "witness %s: the counter did not move (the impairment was not in force)" % f
    if w.get("sysctl_changed"):
        return "sysctl readback changed during the rep"
    if k in ("REL", "HOLD") and not w.get("delay_ok"):
        return "fewer than 95%% of events delayed within dt +- max(50us,5%%) (%s)" % (w.get("events_in_tolerance"),)
    if k == "HOLD" and w.get("hold_overtakers", 0) > 0:
        return "a HOLD rep has overtakers"
    if k == "CLEAN" and w.get("clean_delayed", 0) > 0:
        return "a CLEAN rep has delayed packets"
    return None


def parse_probe(text):
    """The probe's own send log: `<epoch> <ip id>` per line (the pre side of R0, whose reorderer is the only qdisc in its path)."""
    out = []
    for line in text.splitlines():
        f = line.split()
        if len(f) == 2:
            try:
                out.append({"t": float(f[0]), "flow": ("probe",), "seq": None, "ipid": int(f[1]), "len": 0})
            except ValueError:
                pass
    return out


def r0_check(mode, dt_ms, g, k_exp, pre_txt, post_txt, p=None, t_switch=None):
    """R0 properties 1-6 on a paced probe (11.1): (ok, why). pre is the probe's send log or a pcap text; post is a pcap text.
    p: the table's delayed share (property 4: the selection count inside the binomial 99% interval); t_switch: the table switch instant (property 5: no event before it)."""
    pre = parse_probe(pre_txt) or parse_pcap_text(pre_txt)
    post = parse_pcap_text(post_txt)
    pre = [dict(q, seq=q["ipid"], flow=0) for q in pre]       # one probe flow: match on the id alone
    post = [dict(q, seq=q["ipid"], flow=0) for q in post]
    pk = match(pre, post)
    if not pk or any(p["t2"] is None for p in pk):
        return False, "probe packets missing at the post capture"
    base = base_delay(pk)
    dt_s = dt_ms / 1000.0
    if mode == "CLEAN":
        bad = sum(1 for p in pk if p["t2"] - p["t"] - base > 50e-6)
        return (bad == 0, "%d delayed packets in CLEAN" % bad)
    idxs, _ = events_of(pk, "REL" if mode == "REL" else "HOLD", dt_s, base)
    if not idxs:
        return False, "no selected packet"
    tol = max(50e-6, 0.05 * dt_s)
    ok = sum(abs((pk[i]["t2"] - pk[i]["t"] - base) - dt_s) <= tol for i in idxs)
    if ok < 0.95 * len(idxs):
        return False, "selected packets delayed dt+-tol: %d of %d" % (ok, len(idxs))
    if t_switch is not None and any(pk[i]["t"] < t_switch for i in idxs):
        return False, "an event before the table switch (the warm-up witness)"
    if p is not None:
        n = sum(1 for q in pk if t_switch is None or q["t"] >= t_switch)
        if abs(len(idxs) - n * p) > 2.576 * math.sqrt(max(n * p * (1 - p), 1e-9)):
            return False, "selection count %d outside the binomial 99%% interval of p=%g over %d packets" % (len(idxs), p, n)
    ov = [overtakers(pk, i)[0] for i in idxs]
    if mode == "REL":
        tolk = 1 if dt_ms < 0.3 else 0
        good = sum(abs(o - k_exp) <= tolk for o in ov)
        if good < 0.9 * len(ov):
            return False, "exactly-k overtakers in %d of %d events (k=%d)" % (good, len(ov), k_exp)
    else:
        if sum(ov):
            return False, "HOLD has %d overtakers" % sum(ov)
    return True, "ok"


RE_SENT = re.compile(r"Sent (\d+) bytes (\d+) pkt \(dropped (\d+)")
RE_BKLG = re.compile(r"backlog (\d+)b (\d+)p")


def parse_tc(text):
    """`tc -s qdisc show dev X` -> {conf, sent_pkts, dropped, backlog_pkts}; a field the text does not carry is absent (never 0)."""
    out = {}
    for line in text.splitlines():
        if line.startswith("qdisc") and "conf" not in out:
            out["conf"] = line
        m = RE_SENT.search(line)
        if m and "sent_pkts" not in out:
            out["sent_pkts"], out["dropped"] = int(m.group(2)), int(m.group(3))
        m = RE_BKLG.search(line)
        if m and "backlog_pkts" not in out:
            out["backlog_pkts"] = int(m.group(2))
    return out


def witness_of(kind, g, limit, mid_before, mid_after, bot_before, bot_after):
    """The execution witness of a row (the impairment and the queue level were in force): tc counters before and after, and the live qdisc text.
    Returns {field: value}; a missing counter leaves its field absent, which the caller reads as a missing witness (ABORT-SMOKE in a rehearsal, VOID-WITNESS in a scored row)."""
    mb, ma, bb, ba = parse_tc(mid_before), parse_tc(mid_after), parse_tc(bot_before), parse_tc(bot_after)
    w = {}
    if "sent_pkts" in mb and "sent_pkts" in ma:
        w["mid1_sent_delta"] = ma["sent_pkts"] - mb["sent_pkts"]
        w["counters_moved_mid1"] = w["mid1_sent_delta"] > 0
    if "sent_pkts" in bb and "sent_pkts" in ba:
        w["cli0_sent_delta"] = ba["sent_pkts"] - bb["sent_pkts"]
        w["cli0_drops_delta"] = ba["dropped"] - bb["dropped"] if "dropped" in ba and "dropped" in bb else None
        w["counters_moved_cli0"] = w["cli0_sent_delta"] > 0
    if "conf" in ma:
        c = ma["conf"]
        if kind in ("REL", "HOLD", "CLEAN"):
            w["reorderer_in_force"] = "netem" in c and "delay" in c and ("rate" in c) == (kind == "HOLD")
        else:
            w["reorderer_in_force"] = "prio" in c or "netem" in c
    if "conf" in ba:
        w["bottleneck_in_force"] = "netem" in ba["conf"] and "rate" in ba["conf"] and (limit == 0 or ("limit %d" % limit) in ba["conf"])   # limit 0: stage T, the tunnel's own underlay queue
    return w


def tunq(samples_txt, iperf_json, tail_s=60.0, ceiling_mbit=100.0, sizes_B=1500):
    """The standing queue through the real tunnel (11.1): B_t = g_tun * median(srtt - minrtt) over the last tail_s s. Samples: `<epoch> <srtt_ms> <minrtt_ms>` lines.
    Returns a dict; absent evidence gives an error key, never a number."""
    rows = []
    for line in samples_txt.splitlines():
        f = line.split()
        if len(f) == 3:
            try:
                rows.append((float(f[0]), float(f[1]) - float(f[2])))
            except ValueError:
                pass
    try:
        j = json.loads(iperf_json)
        bps = j["end"]["sum_received"]["bits_per_second"]
    except (ValueError, KeyError, TypeError):
        return {"error": "no iperf3 goodput"}
    if not rows:
        return {"error": "no srtt samples"}
    t1 = rows[-1][0]
    tail = [d for t, d in rows if t >= t1 - tail_s]
    tau_ms = statistics.median(tail)
    g = bps / (8.0 * sizes_B)
    return {"tau_q_ms": tau_ms, "g_tun_pps": g, "B_t_tun_pkts": g * tau_ms / 1000.0, "goodput_mbit": bps / 1e6,
            "bottleneck_ok": bps / 1e6 >= 0.9 * ceiling_mbit, "samples": len(tail),
            "B_t_pkts": {str(r): int(round(r * tau_ms / 1000.0)) for r in (1000, 10000)}}


def main(argv):
    if len(argv) >= 6 and argv[1] == "digest":
        meta = json.load(open(argv[2]))
        rd = lambda p: open(p).read()
        row = digest(meta, rd(argv[3]), rd(argv[4]), rd(argv[5]), rd(argv[6]) if len(argv) > 6 else None)
        row["void_reason"] = row_void_reason(row) if row.get("status") == "OK" else None
        if row["void_reason"]:
            row["status"] = "VOID-WITNESS"
        print(json.dumps(row, sort_keys=True))
        return 0
    if len(argv) >= 6 and argv[1] == "warmup":
        t, st = warmup(read_backlog(open(argv[2]).read()), float(argv[3]), float(argv[4]), float(argv[5]), float(argv[6]) if len(argv) > 6 else None)
        print("WITNESS %.6f" % t if t is not None else st)
        return 0 if t is not None else 1
    if len(argv) >= 8 and argv[1] == "r0":
        ok, why = r0_check(argv[2], float(argv[3]), float(argv[4]), int(argv[5]), open(argv[6]).read(), open(argv[7]).read(),
                           float(argv[8]) if len(argv) > 8 and argv[8] != "-" else None, float(argv[9]) if len(argv) > 9 else None)
        print("R0-PASS" if ok else "R0-FAIL " + why)
        return 0 if ok else 1
    if len(argv) == 3 and argv[1] == "ipid":
        by = {}
        for q in parse_pcap_text(open(argv[2]).read()):
            by.setdefault(q["flow"], []).append(q["ipid"])
        steps = [(b - a) % 65536 for v in by.values() for a, b in zip(v, v[1:])]
        print("%.4f" % ((sum(1 for s in steps if s == 1) / len(steps)) if steps else 0.0))
        return 0
    if len(argv) == 9 and argv[1] == "witness":
        rd = lambda p: open(p).read() if p != "-" else ""
        print(json.dumps(witness_of(argv[2], float(argv[3]), int(argv[4]), rd(argv[5]), rd(argv[6]), rd(argv[7]), rd(argv[8]))))
        return 0
    if len(argv) >= 4 and argv[1] == "tunq":
        print(json.dumps(tunq(open(argv[2]).read(), open(argv[3]).read(), ceiling_mbit=float(argv[4]) if len(argv) > 4 else 100.0)))
        return 0
    print(__doc__)
    return 3


if __name__ == "__main__":
    sys.exit(main(sys.argv))
