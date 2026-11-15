#!/usr/bin/env python3
"""Parser for the store law's gauge-only build (B-1; docs/derivations/store-law.md
R4.8, R5.6, R6.2): `[WSTAT]`, `[WSTORE]`, `[SLAW]`, `[SLAWC]` and `[CAPLOG]`.

The engine prints these under RWM_DIAG, once per 250 ms window close. This
extracts each field's per-row median over the measured windows (t >= WARM_S),
per leg for the per-leg tags, store-scope for `[WSTORE]`, and the last value of
the cumulative tags (`[SLAWC]`, the R-crossing counts `xs`/`xb`, `ancf`, `qcol`).

    slaw_parse.py <client.log> [--bdp-bytes N]        JSON on stdout

Field rules (one definition, shared with l1common): `-` is None, never 0.
A compound token `a/b/c` (relb, emb, ancf, qcol, ra, ...) becomes the numeric
components `field.0`, `field.1`, ...; a comma list (`b0k`, `b0kn`, printed once
at window K) is kept as a list of floats and NOT medianed.

The registered quinn-collapse rule (§56 Amendment 2, absolute form) needs the
CELL's BDP in bytes, a harness constant the engine cannot know: pass it as
`--bdp-bytes` and the parser applies it to the per-leg `qcwnd` of [WSTAT]
(share of windows t >= 1 s with qcwnd below it; flagged if > 0.10). The
engine's own `qcol=` is a proxy against Copa's anchor and is reported as is.
"""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import l1common as lc  # noqa: E402

WARM_S = 1.0
QC_SHARE = 0.10
TAGS = ("WSTAT", "WSTORE", "SLAW", "SLAWC")
# Fields that are identifiers or once-only lists, never medianed.
_ID = ("t", "k", "p")
_LISTS = ("b0k", "b0kn")
# Cumulative per-leg readings: last value, not a median.
_CUMULATIVE = ("xs", "xb", "ancf", "qcol", "ra", "kw")


def _tok(raw):
    """Raw token -> None | float | [float..] (compound `/` split by the caller)."""
    if raw is None:
        return None
    return lc.fnum(raw)


def parse_line(line):
    """A `[TAG] k=v ...` line -> (tag, {field: value}) with compounds expanded,
    or None. Values are floats, None, or (for the list fields) lists."""
    for tag in TAGS:
        if line.startswith(f"[{tag}] "):
            break
    else:
        return None
    out = {}
    for tok in line[len(tag) + 3:].split():
        if "=" not in tok:
            continue
        k, v = tok.split("=", 1)
        if v == "-":
            out[k] = None
        elif k in _LISTS:
            out[k] = [lc.fnum(x) for x in v.split(",")]
        elif "/" in v:
            for i, part in enumerate(v.split("/")):
                out[f"{k}.{i}"] = lc.fnum(part)
        else:
            out[k] = lc.fnum(v)
    return tag, out


def parse(lines):
    """All gauge lines -> {tag: [dict...]} (file order), plus the CAPLOG count."""
    out = {t: [] for t in TAGS}
    caplog = 0
    for ln in lines:
        if ln.startswith("[CAPLOG] "):
            caplog += 1
            continue
        r = parse_line(ln)
        if r:
            out[r[0]].append(r[1])
    out["CAPLOG"] = caplog
    return out


def _fields(rows):
    ks = []
    for r in rows:
        for k in r:
            if k not in ks:
                ks.append(k)
    return ks


def _summ(rows, warm_s):
    """rows (one leg or the store scope) -> {field: median | last | list}."""
    meas = [r for r in rows if (r.get("t") or 0.0) >= warm_s]
    out = {"windows": len(meas)}
    for k in _fields(rows):
        if k in _ID:
            continue
        base = k.split(".")[0]
        if base in _LISTS:
            vals = [r[k] for r in rows if r.get(k)]
            out[k] = vals[-1] if vals else None  # printed once, at window K (any t)
        elif base in _CUMULATIVE:
            vals = [r[k] for r in rows if r.get(k) is not None]
            out[k] = vals[-1] if vals else None
        else:
            out[k] = lc.med([r[k] for r in meas if r.get(k) is not None])
    return out


def row_medians(lines, warm_s=WARM_S):
    """Per-row medians of every field.

    -> {"SLAW": {leg: {...}}, "WSTAT": {leg: {...}}, "WSTORE": {...},
        "SLAWC": {last cumulative}, "caplog_records": n}"""
    g = parse(lines)
    res = {"caplog_records": g["CAPLOG"]}
    for tag in ("SLAW", "WSTAT"):
        legs = {}
        for r in g[tag]:
            legs.setdefault(int(r["p"]), []).append(r)
        res[tag] = {f"p{p}": _summ(rs, warm_s) for p, rs in sorted(legs.items())}
    res["WSTORE"] = _summ(g["WSTORE"], warm_s)
    last = g["SLAWC"][-1] if g["SLAWC"] else {}
    res["SLAWC"] = {k: v for k, v in last.items() if k not in _ID}
    return res


def qcwnd_collapse(lines, bdp_bytes, warm_s=WARM_S):
    """The registered absolute collapse rule on [WSTAT] `qcwnd`, per leg:
    {leg: {"n": windows, "share": below/n, "flag": share > 0.10}}."""
    legs = {}
    for r in parse(lines)["WSTAT"]:
        if (r.get("t") or 0.0) < warm_s or not r.get("qcwnd"):
            continue
        legs.setdefault(int(r["p"]), []).append(r["qcwnd"] < bdp_bytes)
    return {f"p{p}": {"n": len(v), "share": sum(v) / len(v), "flag": sum(v) / len(v) > QC_SHARE}
            for p, v in sorted(legs.items())}


def main(argv):
    if not argv:
        print(__doc__)
        return 2
    path, bdp = argv[0], None
    if "--bdp-bytes" in argv:
        bdp = float(argv[argv.index("--bdp-bytes") + 1])
    lines = lc.read(path)
    out = row_medians(lines)
    if bdp:
        out["qcwnd_collapse"] = qcwnd_collapse(lines, bdp)
    print(json.dumps(out, indent=1, sort_keys=True))
    n = sum(v["windows"] for v in out["SLAW"].values())
    return 0 if n else 1  # no [SLAW] window at all: the gauge never ran


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
