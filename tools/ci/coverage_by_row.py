#!/usr/bin/env python3
"""Map per-seat lcov files to law-ledger rows.

For each ledger row (docs/derivations/law-ledger.yaml, key `loc`) the decision lines are
the file:line / file:a-b references in `loc`. Per seat group the table shows
hit/instrumented lines. Also: named shed sites (coverage_sites.json), and the
liveness audit (docs/derivations/liveness-audit.md) as a cross-check.

Usage: coverage_by_row.py LCOV_DIR [--root REPO] [--md OUT.md] [--json OUT.json]
Report only; always exits 0. Regenerate with tools/ci/run_seat_coverage.py + this script.
"""
import argparse, json, pathlib, re
import yaml

HERE = pathlib.Path(__file__).resolve().parent
ENGINE = "raptorpath/src/"
# seat group -> liveness-audit columns (c2 B, c2 A, c2 R, c8 B, c8 A, c8 R, E); c2 = N=1, c8 = N=2
AUDIT_COLS = {"bulk_rho1": [0, 3], "auto_rho1": [1, 4], "realtime_rho1": [2, 5], "evicting_rho_lt1": [6],
              "n1": [0, 1, 2], "n2": [3, 4, 5]}


def read_lcov(path):
    files, cur = {}, None
    for ln in path.read_text(encoding="utf-8", errors="replace").splitlines():
        if ln.startswith("SF:"):
            cur = files.setdefault(ln[3:].replace("\\", "/"), {})
        elif ln.startswith("DA:") and cur is not None:
            a = ln[3:].split(",")
            cur[int(a[0])] = cur.get(int(a[0]), 0) + int(a[1])
    return files


def find_file(files, rel):
    """Match a repo-relative path against lcov SF paths (absolute) by suffix."""
    for sf, d in files.items():
        if sf.endswith("/" + rel):
            return d
    return None


def resolve_paths(tok, root, index):
    if tok.startswith(("raptorpath-math/", "raptorpath-wasm/", "raptorpath-core/", "raptorpath/")):
        c = [tok]
    elif "/" in tok:
        c = [ENGINE + tok]
    else:
        c = index.get(tok, [])
    return [p for p in c if (root / p).is_file()]


def parse_loc(loc, root, index):
    """-> list of (repo_rel_file, lo, hi, ident)"""
    refs, cur = [], None
    for m in re.finditer(r"([\w./-]+\.rs)|:(\d+)(?:-(\d+))?(?:\s+([A-Za-z_][A-Za-z0-9_]{3,}))?", loc):
        if m.group(1):
            cur = m.group(1)
        elif cur:
            lo = int(m.group(2))
            hi = int(m.group(3) or lo)
            for p in resolve_paths(cur, root, index):
                refs.append((p, lo, hi, m.group(4)))
    return refs


def reanchor(root, p, lo, hi, ident, cache):
    """Loc lines drift; a `:N ident` ref whose line is no longer near `fn ident` moves to it."""
    if not ident or lo != hi:
        return lo, hi, False
    lines = cache.setdefault(p, (root / p).read_text(encoding="utf-8", errors="replace").splitlines())
    if any(ident in lines[i] for i in range(max(0, lo - 3), min(len(lines), lo + 2))):
        return lo, hi, False
    hits = [i + 1 for i, l in enumerate(lines) if re.search(r"\bfn\s+" + re.escape(ident) + r"\b", l)]
    if len(hits) == 1:
        return hits[0], hits[0], True
    return lo, hi, False


def row_status(refs, lcovs):
    out = {}
    for g, files in lcovs.items():
        hit = n = 0
        seen_file = False
        for p, lo, hi in refs:
            d = find_file(files, p)
            if d is None:
                continue
            seen_file = True
            for ln, c in d.items():
                if lo <= ln <= hi:
                    n += 1
                    hit += c > 0
        out[g] = (hit, n, "HIT" if hit else ("MISS" if n else ("none" if seen_file else "nofile")))
    return out


def audit_table(path):
    rows = {}
    for ln in path.read_text(encoding="utf-8", errors="replace").splitlines():
        m = re.match(r"\|\s*([A-Z]\d\d)\s*\|(.*)", ln)
        if m:
            c = [x.strip().replace("*", "").replace("\\", "") for x in m.group(2).split("|")]
            if len(c) >= 7:
                rows[m.group(1)] = c[:7]
    return rows


def audit_says(cells, cols):
    v = {cells[i].split()[0] if cells[i] else "" for i in cols}
    if v & {"EX", "PIN"}:
        return "runs"
    if v == {"NR"}:
        return "NR"
    return "?"


def find_sites(cfg, lcovs, root):
    res = {}
    for s in cfg["sites"]:
        res[s["id"]] = {"desc": s["desc"], "line": None, "by_group": {}, "audit": s.get("audit"), "audit_says": s.get("audit_says", {})}
        lines = (root / s["file"]).read_text(encoding="utf-8", errors="replace").splitlines()
        c = [i + 1 for i, l in enumerate(lines) if re.search(s["regex"], l) and abs(i + 1 - s["near"]) <= 80]
        if not c:
            res[s["id"]]["line"] = "not found"
            continue
        ln = min(c, key=lambda x: abs(x - s["near"]))
        res[s["id"]]["line"] = ln
        for g, files in lcovs.items():
            d = find_file(files, s["file"])
            res[s["id"]]["by_group"][g] = None if d is None or ln not in d else d[ln]
    return res


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("lcov_dir")
    ap.add_argument("--root", default=str(HERE.parents[1]))
    ap.add_argument("--md")
    ap.add_argument("--json")
    a = ap.parse_args()
    root = pathlib.Path(a.root).resolve()
    raw = {p.stem: read_lcov(p) for p in sorted(pathlib.Path(a.lcov_dir).glob("*.lcov"))}
    order = list(json.loads((HERE / "seat_groups.json").read_text(encoding="utf-8"))["groups"])
    groups = [g for g in order if g in raw]
    lcovs = {g: raw[g] for g in groups}
    runs = {}
    rj = pathlib.Path(a.lcov_dir) / "runs.json"
    if rj.exists():
        runs = json.loads(rj.read_text(encoding="utf-8"))
    index = {}
    for p in (root / "raptorpath/src").rglob("*.rs"):
        index.setdefault(p.name, []).append(p.relative_to(root).as_posix())
    ledger = yaml.safe_load((root / "docs/derivations/law-ledger.yaml").read_text(encoding="utf-8"))["rows"]
    audit = audit_table(root / "docs/derivations/liveness-audit.md")
    cache, table = {}, []
    for r in ledger:
        refs, moved = [], 0
        for p, lo, hi, ident in parse_loc(r.get("loc", ""), root, index):
            lo2, hi2, mv = reanchor(root, p, lo, hi, ident, cache)
            refs.append((p, lo2, hi2))
            moved += mv
        st = row_status(refs, lcovs)
        cells = audit.get(r["id"])
        row = {"id": r["id"], "name": r["name"], "refs": [f"{p}:{lo}-{hi}" if lo != hi else f"{p}:{lo}" for p, lo, hi in refs],
               "reanchored": moved, "audit": cells,
               "groups": {g: {"hit": h, "n": n, "status": s} for g, (h, n, s) in st.items()}, "flags": {}}
        for g, cols in AUDIT_COLS.items():
            if g in st and cells:
                says, s = audit_says(cells, cols), st[g][2]
                row["flags"][g] = ("FAIL" if says == "runs" and s == "MISS" else
                                   "AGREE" if (says == "runs" and s == "HIT") or (says == "NR" and s == "MISS") else
                                   "UNEXPECTED-HIT" if says == "NR" and s == "HIT" else "")
        table.append(row)
    stz = find_sites(json.loads((HERE / "coverage_sites.json").read_text(encoding="utf-8")), lcovs, root)
    if a.json:
        pathlib.Path(a.json).write_text(json.dumps({"groups": groups, "runs": runs, "sites": stz, "rows": table}, indent=1), encoding="utf-8")
    if a.md:
        pathlib.Path(a.md).write_text(render(groups, runs, stz, table), encoding="utf-8")


def render(groups, runs, stz, table):
    o = ["# Coverage by seat", "",
         "Generated by `tools/ci/coverage_by_row.py` from the `seat-coverage` job (`tools/ci/run_seat_coverage.py`, cargo-llvm-cov, release build, Linux). Do not edit by hand. Report-only baseline: nothing fails on it yet.", "",
         "**Reading it.** A cell is `HIT h/n` or `MISS 0/n`: of the `n` instrumented lines among the row's `loc` lines (law-ledger.yaml), `h` were executed at least once by the tests of that seat group. `-` = no instrumented line (a doc or branch-only reference, or a line with no code). A seat group is a set of test binaries (`tools/ci/seat_groups.json`); a test that loops over several N or hints counts for each of them, so a HIT there over-approximates. `lib` = the lib unit tests, which build their own configs and carry no seat. Coverage shows execution, not effect. Loc lines are re-anchored to `fn name` when the line has drifted.", ""]
    if runs:
        o += ["## Seat groups and timing", "", "| group | seat | runs | wall (s) | failing test runs |", "|---|---|---|---|---|"]
        for g in groups:
            r = runs.get(g)
            if r:
                bad = [s["sel"] for s in r["steps"] if s["rc"]]
                o.append(f"| {g} | {r['seat']} | {len(r['steps'])} | {r['wall_s']} | {'; '.join(bad) or 'none'} |")
        o.append("")
    o += ["## Named shed sites", "", "| site | line | " + " | ".join(groups) + " |", "|---|---|" + "---|" * len(groups)]
    for k, s in stz.items():
        o.append(f"| {k}: {s['desc']} | {s['line']} | " + " | ".join(
            ("n/a" if s["by_group"].get(g) is None else ("HIT %d" % s["by_group"][g] if s["by_group"][g] else "MISS")) for g in groups) + " |")
    o += ["", "## Ledger rows", "", "`audit` = the liveness audit's seven cells (c2 B, c2 A, c2 R, c8 B, c8 A, c8 R, E).", "",
          "| row | " + " | ".join(groups) + " | audit |", "|---|" + "---|" * len(groups) + "---|"]
    for r in table:
        cs = []
        for g in groups:
            x = r["groups"][g]
            cs.append("-" if x["status"] in ("none", "nofile") else f"{x['status']} {x['hit']}/{x['n']}")
        o.append(f"| {r['id']} {r['name'][:38]} | " + " | ".join(cs) + f" | {' '.join(r['audit']) if r['audit'] else ''} |")
    o += ["", "## FAIL lines (report-only)", "",
          "A row the liveness audit reads as running (EX or PIN) at the seat, whose decision lines no test of that seat group hit. Seat to audit columns: " + ", ".join(f"{g} = {[('c2 B','c2 A','c2 R','c8 B','c8 A','c8 R','E')[i] for i in c]}" for g, c in AUDIT_COLS.items()) + ".", ""]
    for g in AUDIT_COLS:
        if g in groups:
            ids = [r["id"] for r in table if r["flags"].get(g) == "FAIL"]
            o.append(f"- **{g}** ({len(ids)}): {', '.join(ids) or 'none'}")
    ex = [(k, s) for k, s in stz.items() if s.get("audit")]
    if ex:
        o += ["", "## Branch sites vs the liveness audit", "", "| site | audit row says | " + " | ".join(groups) + " | agrees |", "|---|---|" + "---|" * len(groups) + "---|"]
        for k, s in ex:
            cells = ["n/a" if s["by_group"].get(g) is None else ("HIT" if s["by_group"][g] else "MISS") for g in groups]
            ag = ["%s %s" % (g, "yes" if (says == "NR") == (cells[groups.index(g)] == "MISS") else "NO") for g, says in s["audit_says"].items() if g in groups]
            o.append(f"| {k} (line {s['line']}) | {s['audit']}: " + ", ".join(f"{g} {v}" for g, v in s["audit_says"].items()) + " | " + " | ".join(cells) + " | " + "; ".join(ag) + " |")
    if "bulk_rho1" in groups:
        g = "bulk_rho1"
        red = [r["id"] for r in table if r["flags"].get(g) == "AGREE" and r["groups"][g]["status"] == "MISS"]
        green = [r["id"] for r in table if r["flags"].get(g) == "AGREE" and r["groups"][g]["status"] == "HIT"]
        un = [r["id"] for r in table if r["flags"].get(g) == "UNEXPECTED-HIT"]
        o += ["", "## Agreement with the liveness audit at Bulk", "",
              f"- audit NR at Bulk (c2 B and c8 B) and not hit here (agree, red) ({len(red)}): {', '.join(red) or 'none'}",
              f"- audit EX/PIN at Bulk and hit here (agree, green) ({len(green)}): {', '.join(green) or 'none'}",
              f"- audit NR at Bulk but hit here (shared lines with a seat that runs, or a test sets a non-default gate) ({len(un)}): {', '.join(un) or 'none'}"]
    o.append("")
    return "\n".join(o)


main()
