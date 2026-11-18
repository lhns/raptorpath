"""Count rule, follow-up (count-rule.md §6): the law's floor across sessions, the flight mechanism against W, and the low mode on s62's floor pair.

Usage (from docs/):  python -I -X utf8 derivations/store-law/crfollow.py l1-raw/s62/rep62r0-diag.tgz l1-raw/s61/rep61c-diag-ld.tgz \
                         l1-raw/s61/rep61a-diag-ld.tgz l1-raw/s60/rep60a-diag-ld.tgz  > derivations/store-law/crfollow.out

Sections:
  A. b-hat0+ per row (net/slaw.rs WinSampler::close on slaw-gauge d416d59b: the max of brake_k = tmin_k - m-hat_k over the process's first K = 8
     windows), with the K raw brakes (b0k=) and R6.2's F per row: why F differs across sessions
  B. the flight mechanism against W: symbols per emit burst (the [DIAG] eb_depth = eb_syms / eb_bursts) and its end reasons, the window median, per arm
  C. the low mode on s62's floor pair R2 / R3 and on D12: the share of native windows whose minimum reaches it, and its run lengths
  D. what precedes the low mode: per [DIAG] tick, the retransmit (retx=) and sender-idle (sidle=) deltas, in the window that holds a low-mode
     minimum against the others; on D12 the send pauses >= 60 ms (from the dump) against the same deltas
"""
import re
import sys
import tarfile
import statistics as st

P = print
NAME = re.compile(r"(?:diag-ld|diag-smoke)/(c\w+-\d+)@def-([GD])(\d+)-s(\d+)-r(\d+)-a\d+-c\.log$")
TOK = re.compile(r"(\w+)=(\S+)")
DIAG_T = re.compile(r"^\[DIAG\] t=([\d.]+)s ")
F = lambda d, k: float(d[k]) if d.get(k, "-") not in ("-", None) else None


def med(v):
    v = [x for x in v if x is not None]
    return st.median(v) if v else None


def parse(lines):
    """-> (slaw windows leg 0, diag ticks, dump samples), after the last warm-up marker."""
    slaw, diag, dump, allw = [], [], [], []
    for ln in lines:
        if '"warmup":true' in ln:
            slaw, diag = [], []
            continue
        if ln.startswith("[SLAW]") and " p=0 " in ln:
            slaw.append(dict(TOK.findall(ln)))
            allw.append(slaw[-1])
        elif ln.startswith("[DIAG]"):
            d = {}
            for key in ("retx", "eb_bursts", "eb_syms", "eb_depth", "paused"):
                m = re.search(r"\b" + key + r"=([\d.]+)", ln)
                d[key] = float(m.group(1)) if m else None
            m = re.search(r"\bsidle=(\d+)ms/(\d+)/mx(\d+)ms", ln)
            d["sidle_ms"], d["sidle_n"] = (int(m.group(1)), int(m.group(2))) if m else (None, None)
            m = re.search(r"\bcum=(\d+)/(\d+)/(\d+)", ln)
            d["cum0"] = int(m.group(1)) if m else None
            m = re.search(r"eb_end=(\S+)", ln)
            d["eb_end"] = m.group(1) if m else None
            d["t"] = float(DIAG_T.match(ln).group(1))
            diag.append(d)
        elif ln.startswith("[RTTDUMP] p=0 "):
            m = re.match(r"^\[RTTDUMP\] p=0 t0=(\d+) n=\d+ d=(\S+)", ln)
            t = int(m.group(1))
            for pair in m.group(2).rstrip(";").split(";"):
                dt, r = pair.split(",")
                t += int(dt)
                dump.append((t, int(r)))
    return slaw, diag, dump, allw


def load(path):
    rows = {}
    with tarfile.open(path, "r:gz") as tf:
        for mb in tf:
            g = NAME.search(mb.name)
            if g and mb.isfile():
                rows[(g.group(1), g.group(2), int(g.group(3)), g.group(4), int(g.group(5)))] = parse(
                    tf.extractfile(mb).read().decode("utf-8", "replace").splitlines())
    return rows


def late(slaw):
    return [w for w in slaw if F(w, "t") is not None and F(w, "t") >= 15.0 and F(w, "nk") and F(w, "nk") >= 10]


def main(argv):
    s62, r61c, r61a, r60a = (load(p) for p in argv[1:5])
    P("count rule follow-up (crfollow.py); inputs " + ", ".join(a.replace("\\", "/").split("/")[-1] for a in argv[1:5]))

    # ── A ──
    P("\n== A. b-hat0+ per row: the K = 8 raw brakes tmin_k - m-hat_k (ms) of the process's first windows (b0k= at k = 8), b0p, the LATE m-hat, and R6.2's F ==")
    for lab, rows, cell, W in (("s62 J0", s62, "c3j0-25", 12), ("61c J5", r61c, "c3-25", 12), ("61a J5", r61a, "c3-25", 12), ("c2 s60", r60a, "c2-100", 16)):
        Fs = []
        for key, (slaw, diag, dump, allw) in sorted(rows.items()):
            if key[0] != cell or key[2] != W:
                continue
            lw = late(slaw)
            k8 = [w for w in allw if w.get("b0k", "-") != "-"]
            bk = [round(float(x), 2) for x in k8[0]["b0k"].split(",")] if k8 else []
            kn = k8[0].get("b0kn", "-") if k8 else "-"
            m = med([F(w, "m") for w in lw])
            b0 = med([F(w, "b0p") for w in lw])
            l = med([F(w, "l") for w in lw])
            Fv = min(m, l + b0) + b0
            Fs.append(Fv)
            m_k = [F(w, "m") for w in allw[:8]]
            P(f"  {lab} {key[1]}{W} s{key[3]} r{key[4]}: brakes {bk} (window counts {kn}); m-hat at k = 1..8 {[round(x, 2) if x else None for x in m_k]};"
              f" b0p {b0:.2f}; LATE m-hat {m:.2f}; F {Fv:.2f}; native tmin {med([F(w, 'tmin') for w in lw]):.2f}")
        P(f"  {lab}: F over rows {min(Fs):.2f} - {max(Fs):.2f} (median {med(Fs):.2f})")

    # ── B ──
    P("\n== B. The flight mechanism against W: symbols per emit burst (eb_depth at the last LATE tick), its end reasons, the window median (tp50) and the floor-window minimum ==")
    for lab, rows, cell, arms in (("61c J5", r61c, "c3-25", (12, 16, 24, 60, 80, 100, 108)), ("61a J5", r61a, "c3-25", (12, 16, 24, 68, 92)),
                                  ("s62 J0", s62, "c3j0-25", (12, 80, 128)), ("c2 s60", r60a, "c2-100", (16, 32, 48))):
        for W in arms:
            rs = [v for k, v in rows.items() if k[0] == cell and k[2] == W and k[1] == "G"]
            if not rs:
                continue
            ebd = med([d[-1]["eb_depth"] for _, d, _, _ in rs if d and d[-1]["eb_depth"] is not None])
            ends = [d[-1]["eb_end"] for _, d, _, _ in rs if d]
            tp = med([med([F(w, "tp50") for w in late(s)]) for s, _, _, _ in rs])
            tm = med([med([F(w, "tmin") for w in late(s)]) for s, _, _, _ in rs])
            nk = med([med([F(w, "nk") for w in late(s)]) for s, _, _, _ in rs])
            P(f"  {lab} W{W:<4} rows {len(rs)}: symbols per emit burst {ebd:.2f}; window tp50 {tp:.2f}, native tmin {tm:.2f}, nk {nk:.0f}; eb_end (row 1) {ends[0]}")

    # ── C ──
    P("\n== C. The low mode on s62's floor rows: a native window 'reaches' it when its minimum lies > 4 ms below the row's LATE median tp50 ==")
    for key, (slaw, diag, dump, allw) in sorted(s62.items()):
        if key[2] != 12:
            continue
        lw = late(slaw)
        ref = med([F(w, "tp50") for w in lw])
        low = [F(w, "tmin") < ref - 4.0 for w in lw]
        runs, cur = [], 0
        for x in low + [False]:
            if x:
                cur += 1
            elif cur:
                runs.append(cur)
                cur = 0
        gaps, cur = [], 0
        for x in low + [True]:
            if not x:
                cur += 1
            elif cur:
                gaps.append(cur)
                cur = 0
        P(f"  {key[1]}{key[2]} s{key[3]} r{key[4]}: {sum(low)} of {len(low)} native windows ({100 * sum(low) / len(low):.0f} %) reach the low mode (median tp50 {ref:.2f});"
          f" runs of low windows: n {len(runs)}, lengths median {med(runs)} max {max(runs) if runs else 0}; runs without: median {med(gaps)} max {max(gaps) if gaps else 0} windows")

    # ── D ──
    P("\n== D. What precedes the low mode: per [DIAG] tick, the retransmit delta (retx) and the sender-idle count delta (sidle), in low-mode windows against the rest ==")
    for lab, rows, cell, W in (("s62 J0", s62, "c3j0-25", 12), ("61c J5", r61c, "c3-25", 12)):
        for key, (slaw, diag, dump, allw) in sorted(rows.items()):
            if key[0] != cell or key[2] != W:
                continue
            n = min(len(slaw), len(diag))
            ref = med([F(w, "tp50") for w in late(slaw)])
            stats = {True: [], False: []}
            prevr = {True: [], False: []}
            for i in range(1, n):
                w = slaw[i]
                if F(w, "t") is None or F(w, "t") < 15.0 or not F(w, "nk") or F(w, "nk") < 10:
                    continue
                is_low = F(w, "tmin") < ref - 4.0
                dr = diag[i]["retx"] - diag[i - 1]["retx"] if diag[i]["retx"] is not None and diag[i - 1]["retx"] is not None else None
                dp = diag[i - 1]["retx"] - diag[i - 2]["retx"] if i >= 2 and diag[i - 1]["retx"] is not None and diag[i - 2]["retx"] is not None else None
                ds = diag[i]["sidle_n"] - diag[i - 1]["sidle_n"] if diag[i]["sidle_n"] is not None and diag[i - 1]["sidle_n"] is not None else None
                stats[is_low].append((dr, ds))
                prevr[is_low].append(dp)
            for flag in (True, False):
                v = stats[flag]
                if not v:
                    continue
                share_r = sum(1 for dr, _ in v if dr) / len(v)
                share_p = sum(1 for x in prevr[flag] if x) / max(1, len(prevr[flag]))
                P(f"  {lab} {key[1]}{W} s{key[3]} r{key[4]} {'low-mode windows' if flag else 'other windows   '} ({len(v)}): share with retx in the same tick {share_r:.2f},"
                  f" in the previous tick {share_p:.2f}; sender-idle episodes per tick median {med([ds for _, ds in v])}")
    # D12: send pauses >= 60 ms from the dump
    for key, (slaw, diag, dump, allw) in s62.items():
        if key[1] != "D" or not dump:
            continue
        idx, wk = 0, []
        for i, w in enumerate(slaw):
            nk = int(F(w, "nk") or 0)
            wk.append((idx, idx + nk, i))
            idx += nk
        snd = sorted(((t - r), r, j) for j, (t, r) in enumerate(dump))
        pauses = []
        for a, b in zip(snd, snd[1:]):
            if b[0] - a[0] >= 60000:
                pauses.append(b[2])
        def win_of(j):
            for lo, hi, i in wk:
                if lo <= j < hi:
                    return i
        hits, both = 0, 0
        for j in pauses:
            i = win_of(j)
            if i is None or i < 2 or i >= len(diag):
                continue
            r0 = diag[i]["retx"] - diag[i - 2]["retx"] if diag[i]["retx"] is not None and diag[i - 2]["retx"] is not None else 0
            hits += 1
            both += r0 > 0
        base = [diag[i]["retx"] - diag[i - 2]["retx"] for i in range(2, len(diag)) if diag[i]["retx"] is not None and diag[i - 2]["retx"] is not None]
        P(f"  D12: {len(pauses)} send pauses >= 60 ms; {both} of {hits} sit in a window whose tick or the one before carried a retransmit,"
          f" against {sum(1 for x in base if x > 0) / len(base):.2f} of all windows")


if __name__ == "__main__":
    main(sys.argv)
