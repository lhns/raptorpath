#!/usr/bin/env python3
"""Per-window join of 68r [HOFF] lines and the classification of RULE.txt. Usage: join68r.py <diag-ld dir> [--dump]"""
import os
import re
import sys
from collections import Counter, defaultdict

PATH = re.compile(r"\bp(\d+)\[([^\]]*)\]")
NUM = re.compile(r"^(-?[\d.]+(?:e[-+]?\d+)?)")


def num(s):
    m = NUM.match(s)
    return float(m.group(1)) if m else None


def parse(line):
    t = float(re.search(r"\bt=([\d.]+)s", line).group(1))
    gate = re.search(r"\bgate=(\w+)", line).group(1)
    paths = {}
    for pid, body in PATH.findall(line):
        d = {}
        for tok in body.split():
            if "=" not in tok:
                continue
            k, v = tok.split("=", 1)
            if "/" in v:
                d[k] = [num(x) for x in v.split("/")]
            else:
                d[k] = num(v)
        paths[int(pid)] = d
    return t, gate, paths


def windows(fn):
    out = []
    with open(fn, "rb") as f:
        for raw in f:
            ln = raw.decode("utf-8", "replace")
            if "[HOFF]" in ln and "gate=" in ln:
                ln = re.sub(r"\x1b\[[0-9;]*m", "", ln[ln.index("[HOFF]"):])
                try:
                    out.append(parse(ln))
                except Exception:
                    pass
    return out


def qcc_steps(fn):
    """[(t?, qrn per path)] from [QCC] lines: returns list of (line index order, {pid: qrn})"""
    out = []
    with open(fn, "rb") as f:
        for raw in f:
            ln = raw.decode("utf-8", "replace")
            if "[QCC]" in ln:
                mt = re.search(r"\bt=([\d.]+)s", ln)
                q = {int(a): int(b) for a, b in re.findall(r"\bp(\d+):.*?\bqrn=(\d+)", ln)}
                out.append((float(mt.group(1)) if mt else None, q))
    return out


def rows(d):
    for fn in sorted(os.listdir(d)):
        m = re.match(r"(c\d)-100@def-([A-Z]+\d*)-s42-r(\d)-a1-c\.log$", fn)
        if m:
            yield m.group(2), int(m.group(3)), os.path.join(d, fn)


def g(p, k, i=None):
    v = p.get(k)
    if i is not None:
        return v[i] if isinstance(v, list) and len(v) > i else None
    return v


def classify(ws, j, pid):
    t, gate, P = ws[j]
    p = P[pid]
    prev = ws[j - 1][2].get(pid, {}) if j > 0 else {}
    nxt = ws[j + 1][2].get(pid, {}) if j + 1 < len(ws) else {}
    b95, bmax, beta, batch = g(p, "B", 1), g(p, "B", 2), g(p, "beta"), g(p, "batch")
    bq5, bq50, bq95 = g(p, "beta_q", 0), g(p, "beta_q", 1), g(p, "beta_q", 2)
    L = g(p, "L")
    pend, og = g(p, "pend_max"), g(p, "own_gap_max")
    og1 = g(nxt, "own_gap_max") if nxt else None
    ext, exf, exx = g(p, "ex_max", 0), g(p, "ex_max", 1), g(p, "ex_max", 2)
    bq95p = g(prev, "beta_q", 2) if prev else None
    if None in (b95, beta, batch, bq50, bq95, bmax):
        return None
    E = b95 - beta
    A = (b95 - bq50) <= batch
    U = bmax <= max(bq95, bq95p or 0) + batch
    ogm = max(og or 0, og1 or 0) / 1e3  # ms
    O = (pend or 0) >= 3 * batch and L is not None and ogm >= L
    X = ext is not None and ext > 0 and exx is not None and exx >= 0.5 * ext
    prim = "beta" if (A or U) else "owner" if O else "exempt" if X else "unexplained"
    reo_max = g(p, "reopen", 1)
    return dict(t=t, E=E, b50=g(p, "B", 0), b95=b95, bmax=bmax, beta=beta, bq5=bq5, bq50=bq50, bq95=bq95, bq95p=bq95p, batch=batch,
                pend=pend, og=og, og1=og1, L=L, ext=ext, exf=exf, exx=exx, dgq=g(p, "dgq"), reo=reo_max, reo_n=g(p, "reopen", 2),
                fs_n=g(p, "fs_n"), hb=g(p, "hand_bind"), A=A, U=U, O=O, X=X, prim=prim,
                spread=(bq5 is not None and bq50 and bq5 < 0.5 * bq50))


def main(d, dump=False):
    allw = defaultdict(list)   # (arm, rep, pid) -> [cls]
    for arm, rep, fn in rows(d):
        if arm[0] not in "GQ" or arm == "GLR":
            continue
        ws = [w for w in windows(fn) if w[1] == "on"]
        for j, (t, gate, P) in enumerate(ws):
            if t < 2.0:
                continue
            for pid in P:
                c = classify(ws, j, pid)
                if c:
                    allw[(arm, rep, pid)].append(c)
    print("=== per (row, path): over-batch windows and primary classes; top-3 detail ===")
    tot = defaultdict(Counter)
    top = defaultdict(Counter)
    flags = defaultdict(Counter)
    reo_tab = defaultdict(Counter)
    for key in sorted(allw):
        arm, rep, pid = key
        cs = allw[key]
        over = [c for c in cs if c["E"] > c["batch"]]
        grp = arm[:1] + ("PC" if "PC" in arm else "") + f" {arm[1:]} p{pid}"
        for c in over:
            tot[grp][c["prim"]] += 1
            flags[grp]["".join(f for f in "AUOX" if c[f]) or "-"] += 1
        for c in cs:
            miss = c["reo"] is not None and c["L"] and c["reo"] > c["L"]
            reo_tab[grp]["win"] += 1
            reo_tab[grp]["miss"] += miss
            reo_tab[grp]["O"] += c["O"]
            reo_tab[grp]["spread"] += bool(c["spread"])
            if miss:
                reo_tab[grp]["miss_O"] += c["O"]
                reo_tab[grp]["miss_spread"] += bool(c["spread"])
                reo_tab[grp]["miss_over"] += c["E"] > c["batch"]
        worst = sorted(over, key=lambda c: -c["E"])[:3]
        for c in worst:
            top[grp][c["prim"]] += 1
        print(f"{arm} r{rep} p{pid}: windows={len(cs)} over={len(over)} classes={dict(Counter(c['prim'] for c in over))}")
        for c in worst:
            print(f"   t={c['t']:5.2f} E={c['E']:6.1f} B={c['b50']:.0f}/{c['b95']:.0f}/{c['bmax']:.0f} beta_end={c['beta']:.1f} beta_q={c['bq5']:.1f}/{c['bq50']:.1f}/{c['bq95']:.1f}"
                  f" (prev p95 {c['bq95p'] if c['bq95p'] is None else round(c['bq95p'], 1)}) batch={c['batch']:.1f} pend_max={c['pend']:.0f} own_gap={c['og']/1e3:.1f}/{(c['og1'] or 0)/1e3:.1f}ms L={c['L']:.2f}"
                  f" reopen_max={c['reo']:.2f}ms ex_max={c['ext']:.0f}/{c['exf']:.0f}/{c['exx']:.0f} dgq={c['dgq']:.0f} fs_n={c['fs_n']:.0f} -> {''.join(f for f in 'AUOX' if c[f]) or '-'} {c['prim']}")
    print("\n=== counts by group (all over-batch windows) ===")
    for grp in sorted(tot):
        print(f"{grp}: primary={dict(tot[grp])} flags={dict(flags[grp])} top3={dict(top[grp])}")
    print("\n=== reopen misses (reopen max > L) vs O flag and beta spread (all windows t>=2s) ===")
    for grp in sorted(reo_tab):
        r = reo_tab[grp]
        w, m = r["win"], r["miss"]
        print(f"{grp}: windows={w} miss={m} | O share all={r['O']/w:.3f} miss={r['miss_O']/max(m,1):.3f} | spread share all={r['spread']/w:.3f} miss={r['miss_spread']/max(m,1):.3f} | miss&over={r['miss_over']}")
    if dump:
        return allw
    return None


if __name__ == "__main__":
    main(sys.argv[1], "--dump" in sys.argv)
