#!/usr/bin/env python3
"""Per-window series of one row: HOFF end samples joined with QCC. Usage: series.py <client log> [pid] [t0] [t1]"""
import re, sys, os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import join68r as J

def qcc(fn):
    out = {}
    with open(fn, "rb") as f:
        for raw in f:
            ln = raw.decode("utf-8", "replace")
            if "[QCC]" in ln:
                ln = re.sub(r"\x1b\[[0-9;]*m", "", ln[ln.index("[QCC]"):])
                t = float(re.search(r"\bt=([\d.]+)s", ln).group(1))
                for pid, body in re.findall(r"\bp(\d+):(.*?)(?=\sp\d+:|$)", ln):
                    d = dict(kv.split("=", 1) for kv in body.split() if "=" in kv)
                    out[(t, int(pid))] = d
    return out

def main(fn, pid=0, t0=0, t1=1e9):
    Q = qcc(fn)
    for t, gate, P in J.windows(fn):
        if gate != "on" or not (t0 <= t <= t1) or pid not in P:
            continue
        p = P[pid]
        win, R, beta, L = p.get("win"), p.get("R"), p.get("beta"), p.get("L")
        srtt = 1.25 * win / R * 1e3 if win and R else float("nan")
        q = Q.get((t, pid), {})
        bq = p.get("beta_q") or [0, 0, 0]
        B = p.get("B") or [0, 0, 0]
        print(f"t={t:5.1f} B={B[0]:4.0f}/{B[1]:4.0f}/{B[2]:4.0f} beta={beta:6.1f} bq={bq[0]:6.1f}/{bq[1]:6.1f}/{bq[2]:6.1f} win={win/1e3:6.1f}k F={p.get('F',0)/1e3:6.1f}k srtt={srtt:5.1f} L={L:4.2f} "
              f"qm={q.get('qm','?')} m={int(q.get('qrt_us',0))/1e3:4.1f} qbw={int(q.get('qbw',0))/1e6:5.2f} qrn={q.get('qrn','?')} hb={p.get('hand_bind')} dgq={p.get('dgq')} ex={p.get('ex_max')}")

if __name__ == "__main__":
    a = sys.argv
    main(a[1], int(a[2]) if len(a) > 2 else 0, float(a[3]) if len(a) > 3 else 0, float(a[4]) if len(a) > 4 else 1e9)
