#!/usr/bin/env python3
"""Rewrite the GENERATED regions of docs/roadmap.md from docs/roadmap/nodes.yaml.

Everything outside the `<!-- BEGIN GENERATED: x -->` / `<!-- END GENERATED: x -->`
markers is hand-written and preserved byte for byte.
"""
import argparse
import datetime
import re
import sys
import textwrap
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[2]
NODES = ROOT / "docs" / "roadmap" / "nodes.yaml"
MD = ROOT / "docs" / "roadmap.md"

# code -> (short name, scope); fixed by the node schema.
TRACKS = {
    "cod": ("coding core", "RaptorQ/FEC encoder and decoder, gf256, symbol framing, the generation seat"),
    "rec": ("recovery", "ARQ, SACK, the hole law, recovery clocks, gap horizon, snapshot order, tail sweep"),
    "rate": ("rate law", "the rate law and the (δ, ρ, r) triangle: FEC rate, P_fec, W*, hints as named points"),
    "store": ("store law", "flow control and the store law: store cap, SACK-clocked release, the time term, δ's seat"),
    "place": ("placement", "multipath placement: placement price, idle-leg lock-out, diversity weight, per-leg state"),
    "cc": ("congestion control", "substrate congestion control: quinn BBR, bbr_rs, Copa, the BBR/Copa hybrid (ADR-0068), the pacing seam"),
    "perf": ("performance", "threading and per-packet cost: threading redesign, I/O owner, receiver cost, allocation bounds"),
    "meas": ("measurement", "measurement and harness: the L1 harness, guards, stub run, instruments, measurement discipline"),
    "model": ("model and paper", "the model, paper and visualizer: paper, wasm L0 model, NO-MODE-SWITCH audits, model-vs-engine bounds"),
    "obs": ("observability", "the metrics registry, OTel, the flight recorder"),
    "infra": ("repo and simplification", "restructure, simplify batches, archived arms, early platform work (handshake, TLS, MTU, migration)"),
}
KINDS = ["law", "mechanism", "instrument", "infra", "research", "user-decision", "defect-verdict", "simplification"]
STATES = ["done", "in-progress", "next", "blocked", "parked", "abandoned"]
OUTCOMES = ["very-successful", "successful", "mixed", "bad", "dead-end", "superseded", "unrated", "n/a"]
LANES = ["vm", "host-cargo", "no-cargo", "user", "none"]

WRAP = 20           # label line width in characters
OVERVIEW_WIDTH = 3  # max nodes per row inside an overview track box
TRACK_WIDTH = 4     # max nodes per rank in a per-track tree
GANTT_TITLE = 40    # gantt task names are truncated to this many characters

REGION_ORDER = ["legend", "overview", "timeline"] + [f"track-{c}" for c in TRACKS] + ["table"]

CLASSDEFS = [
    ("vsucc", "fill:#2e9e4f,stroke:#1b6b34,color:#fff"),
    ("succ", "fill:#bfe8c8,stroke:#2e9e4f,color:#111"),
    ("mixed", "fill:#ffd37a,stroke:#c98a00,color:#111"),
    ("bad", "fill:#f28b82,stroke:#b3261e,color:#111"),
    ("dead", "fill:#e6e6e6,stroke:#777,stroke-dasharray:5 5,color:#555"),
    ("inprog", "fill:#a8cdf5,stroke:#1f6fc5,color:#111"),
    ("next", "fill:#fff,stroke:#333,color:#111"),
    ("blocked", "fill:#fff,stroke:#b3261e,stroke-dasharray:2 2,color:#111"),
    ("parked", "fill:#f3f3f3,stroke:#999,color:#555"),
    ("unrated", "fill:#fff,stroke:#333,stroke-dasharray:1 3,color:#111"),
    ("userdec", "fill:#d9c2f0,stroke:#7b3fb5,color:#111"),
    ("stub", "fill:#fff,stroke:#aaa,stroke-dasharray:2 2,color:#777,font-size:11px"),
]

_ESC = {
    "#": "#35;", '"': "#quot;", "&": "#amp;", "<": "#lt;", ">": "#gt;",
    "[": "#91;", "]": "#93;", "{": "#123;", "}": "#125;", "|": "#124;",
    "`": "#96;", "§": "#167;", "~": "#126;",
}
_ESC_RE = re.compile("[" + re.escape("".join(_ESC)) + "]")


def esc(s):
    """Make text safe inside a quoted Mermaid label (single pass, so '#' is not re-escaped)."""
    s = " ".join(str(s).split())
    return _ESC_RE.sub(lambda m: _ESC[m.group(0)], s)


def sid(node_id):
    return re.sub(r"[^A-Za-z0-9_]", "_", node_id)


def datestr(v):
    return "" if v in (None, "") else str(v)


def tag(n):
    s, o = n["state"], n["outcome"]
    if s == "done":
        return f"✔ done · {o}"
    if s == "abandoned":
        return "↷ superseded" if o == "superseded" else "✖ dead end"
    return {"in-progress": "▶ in progress", "next": "○ next", "blocked": "⛔ blocked", "parked": "⏸ parked"}[s]


def node_class(n):
    s, o = n["state"], n["outcome"]
    if n["kind"] == "user-decision" and s not in ("done", "abandoned"):
        return "userdec"
    if s == "abandoned":
        return "dead"
    if s == "done":
        return {"very-successful": "vsucc", "successful": "succ", "mixed": "mixed", "bad": "bad",
                "dead-end": "dead", "superseded": "dead", "unrated": "unrated", "n/a": "userdec"}[o]
    return {"in-progress": "inprog", "next": "next", "blocked": "blocked", "parked": "parked"}[s]


def node_def(n, ident=None):
    title = "<br/>".join(esc(line) for line in textwrap.wrap(" ".join(n["title"].split()), WRAP) or [""])
    label = f"{title}<br/>{esc(tag(n))}{' ⚠' if n['qualified_by'] else ''}"
    i = ident or sid(n["id"])
    k = n["kind"]
    if k == "user-decision":
        return f'{i}{{{{"{label}"}}}}'
    if k == "law":
        return f'{i}[/"{label}"/]'
    if k == "defect-verdict":
        return f'{i}>"{label}"]'
    return f'{i}["{label}"]'


def class_lines(pairs, indent="    "):
    groups = {}
    for ident, cls in pairs:
        groups.setdefault(cls, []).append(ident)
    return [f"{indent}class {','.join(ids)} {cls}" for cls, ids in groups.items()]


def classdef_lines(indent="    "):
    return [f"{indent}classDef {name} {style}" for name, style in CLASSDEFS]


def edges(nodes, keep):
    """Yield (src, dst, kind) for edges where keep(src_node, dst_node) holds.

    kind: "dep" (prerequisite -> dependant), "repl" (replaced by), "qual" (rated node -> qualifying defect).
    """
    by = {n["id"]: n for n in nodes}
    for n in nodes:
        for d in n["depends_on"]:
            if keep(by[d], n):
                yield d, n["id"], "dep"
        for r in n["replaced_by"]:
            if keep(n, by[r]):
                yield n["id"], r, "repl"
        for q in n["qualified_by"]:
            if keep(n, by[q]):
                yield n["id"], q, "qual"


EDGE_ARROWS = {"dep": "-->", "repl": "-.->|replaced by|", "qual": "-.->|⚠ qualified by|"}
QUAL_STYLE = "stroke:#e69500,stroke-width:2px,stroke-dasharray:2 3,color:#a86400"
_LINK_RE = re.compile(r"^\s+\S+ (?:-->|-\.->\|[^|]*\||~~~)")


def edge_line(a, b, kind, sa=None, sb=None):
    a, b = sa or sid(a), sb or sid(b)
    return f"    {a} {EDGE_ARROWS[kind]} {b}"


def link_styles(lines):
    """linkStyle lines colouring qualified_by links (Mermaid indexes links in order of appearance)."""
    idx, hits = 0, []
    for line in lines:
        if _LINK_RE.match(line):
            if "⚠ qualified by" in line:
                hits.append(idx)
            idx += line.count("~~~") or 1
    return [f"    linkStyle {','.join(map(str, hits))} {QUAL_STYLE}"] if hits else []


def row_links(group, width, indent="    "):
    """Invisible links that break any over-wide rank of `group` into rows of at most `width`.

    Rank = longest depends_on path inside the group. Within a rank, row r node i is tied to
    row r+1 node i, so wide fan-outs stack instead of spreading sideways.
    """
    ids = {n["id"] for n in group}
    by = {n["id"]: n for n in group}
    rank = {}

    def depth(i, seen=()):
        if i not in rank:
            rank[i] = 1 + max([depth(d, seen + (i,)) for d in by[i]["depends_on"] if d in ids and d not in seen], default=-1)
        return rank[i]

    ranks = {}
    for n in group:
        ranks.setdefault(depth(n["id"]), []).append(sid(n["id"]))
    out = []
    for members in ranks.values():
        rows = [members[i:i + width] for i in range(0, len(members), width)]
        for a, b in zip(rows, rows[1:]):
            out += [f"{indent}{x} ~~~ {y}" for x, y in zip(a, b)]
    return out


def fence(lines):
    return "```mermaid\n" + "\n".join(lines) + "\n```"


def r_overview(nodes, today):
    sel = [n for n in nodes if n["overview"]]
    if not sel:
        return "_No nodes have `overview: true` yet._"
    ids = {n["id"] for n in sel}
    out = ["flowchart TB"]
    present = []
    for code, (name, _) in TRACKS.items():
        group = [n for n in sel if n["track"] == code]
        if not group:
            continue
        present.append(f"sg_{code}")
        out.append(f'    subgraph sg_{code}["{esc(code + " - " + name)}"]')
        out.append("        direction TB")
        out += [f"        {node_def(n)}" for n in group]
        out += row_links(group, OVERVIEW_WIDTH, "        ")
        out.append("    end")
    if len(present) > 1:
        out.append("    " + " ~~~ ".join(present))
    out += [edge_line(a, b, d) for a, b, d in edges(nodes, lambda x, y: x["id"] in ids and y["id"] in ids)]
    out += link_styles(out)
    out += classdef_lines()
    out += class_lines([(sid(n["id"]), node_class(n)) for n in sel])
    return fence(out)


def r_track(code):
    def render(nodes, today):
        sel = [n for n in nodes if n["track"] == code]
        if not sel:
            return "_No nodes in this track yet._"
        ids = {n["id"] for n in sel}
        out = ["flowchart TB"]
        out += [f"    {node_def(n)}" for n in sel]
        out += row_links(sel, TRACK_WIDTH)
        es = list(edges(nodes, lambda x, y: x["id"] in ids or y["id"] in ids))
        stubs = sorted({e for a, b, _ in es for e in (a, b) if e not in ids})
        out += [f'    x_{sid(s)}["{esc(s)}"]' for s in stubs]
        for a, b, kind in es:
            out.append(edge_line(a, b, kind, None if a in ids else f"x_{sid(a)}", None if b in ids else f"x_{sid(b)}"))
        out += link_styles(out)
        out += classdef_lines()
        out += class_lines([(sid(n["id"]), node_class(n)) for n in sel] + [(f"x_{sid(s)}", "stub") for s in stubs])
        return fence(out)
    return render


def gantt_text(s, limit=None):
    t = " ".join(str(s).replace(":", " -").replace(";", ",").replace("#", "").replace('"', "'").split())
    return t if limit is None or len(t) <= limit else t[: limit - 1].rstrip() + "…"


def r_timeline(nodes, today):
    out = ["gantt", "    dateFormat YYYY-MM-DD", "    axisFormat %Y-%m-%d"]
    undated, any_task = [], False
    for code, (name, _) in TRACKS.items():
        rows = []
        for n in (x for x in nodes if x["track"] == code):
            if not datestr(n["start"]):
                undated.append(n["id"])
                continue
            mod = {"abandoned": "crit", "in-progress": "active"}.get(n["state"], "")
            end = datestr(n["end"]) or today
            rows.append(f"    {gantt_text(n['title'], GANTT_TITLE)} :{mod + ', ' if mod else ''}{sid(n['id'])}, {datestr(n['start'])}, {end}")
        if rows:
            any_task = True
            out.append(f"    section {code} - {gantt_text(name)}")
            out += rows
    parts = [f"%% today: {today}"]
    parts.append(fence(out) if any_task else "_No node has a start date yet._")
    if undated:
        parts.append("Not shown (no `start`): " + ", ".join(f"`{i}`" for i in undated))
    return "\n\n".join(parts)


def cell(s):
    return " ".join(str(s).split()).replace("|", "\\|")


def r_table(nodes, today):
    parts = []
    for code, (name, _) in TRACKS.items():
        sel = [n for n in nodes if n["track"] == code]
        if not sel:
            continue
        parts.append(f"### {code} - {name}")
        rows = ["| id | title | state | outcome | headline | evidence | depends_on |", "|---|---|---|---|---|---|---|"]
        for n in sel:
            rows.append("| " + " | ".join([
                f"`{n['id']}`", cell(n["title"]), n["state"], n["outcome"], cell(n["headline"]),
                cell("; ".join(n["evidence"])), cell(", ".join(n["depends_on"])),
            ]) + " |")
        parts.append("\n".join(rows))
    for code, (name, _) in TRACKS.items():
        sel = [n for n in nodes if n["track"] == code and n["notes"]]
        if sel:
            parts.append(f"#### Details per node: {code} - {name}")
            rows = ["| id | title | notes | evidence |", "|---|---|---|---|"]
            rows += [f"| `{n['id']}` | {cell(n['title'])} | {cell(n['notes'])} | {cell('; '.join(n['evidence']))} |" for n in sel]
            parts.append("\n".join(rows))
    return "\n\n".join(parts) if parts else "_No nodes yet._"


def r_legend(nodes, today):
    samples = [
        ("vsucc", "very successful"), ("succ", "successful"), ("mixed", "mixed"), ("bad", "bad"),
        ("dead", "dead end / superseded / abandoned"), ("inprog", "in progress"), ("next", "next"),
        ("blocked", "blocked"), ("parked", "parked"), ("unrated", "done, unrated"),
        ("userdec", "user decision pending"),
    ]
    out = ["flowchart LR"]
    out += [f'    lg_{c}["{esc(t)}"]' for c, t in samples]
    out += ['    lg_k1{{"user-decision"}}', '    lg_k2[/"law"/]', '    lg_k3>"defect-verdict"]', '    lg_k4["other kinds"]']
    out += ["    lg_k1 --> lg_k2", "    lg_k3 -.->|replaced by| lg_k4"]
    out += classdef_lines()
    out += class_lines([(f"lg_{c}", c) for c, _ in samples])
    text = [
        "Colour is never the only signal: every node label carries a status tag on its second line.",
        "",
        fence(out),
        "",
        "| Tag | Meaning |",
        "|---|---|",
        "| `✔ done · <outcome>` | finished; outcome is the recorded verdict (green = very successful, light green = successful, amber = mixed, red = bad) |",
        "| `▶ in progress` | being worked on (blue) |",
        "| `○ next` | planned, not started (white) |",
        "| `⛔ blocked` | waiting on a prerequisite or an outside call (white, red dashed border) |",
        "| `⏸ parked` | deliberately set aside (light grey) |",
        "| `✖ dead end` | refuted or archived, nothing reused (grey, dashed border) |",
        "| `↷ superseded` | replaced by another node, drawn with a dashed `replaced by` arrow (grey, dashed border) |",
        "",
        "Shapes: hexagon = user decision (purple while pending), parallelogram = law, flag = defect verdict, rectangle = everything else.",
        "Solid arrow = hard prerequisite (prerequisite to dependant). Dashed arrow = `replaced by`.",
        "Amber dotted arrow = `⚠ qualified by`: a later defect or verdict limits the rating of the done node it leaves; that node's label ends in `⚠`.",
    ]
    return "\n".join(text)


RENDERERS = {"legend": r_legend, "overview": r_overview, "timeline": r_timeline, "table": r_table}
RENDERERS.update({f"track-{c}": r_track(c) for c in TRACKS})


def normalize(nodes):
    """Copy nodes with dates as strings so renderers see plain data."""
    out = []
    for n in nodes:
        m = dict(n)
        m["start"], m["end"] = datestr(n.get("start")), datestr(n.get("end"))
        m.setdefault("qualified_by", [])
        m["notes"] = m.get("notes") or ""
        out.append(m)
    return out


def render_regions(nodes, today=None):
    today = str(today or datetime.date.today())
    nodes = normalize(nodes)
    return {name: RENDERERS[name](nodes, today) for name in REGION_ORDER}


def apply_regions(text, regions):
    """Replace each region's body; raises ValueError on missing/duplicate markers."""
    eol = "\r\n" if "\r\n" in text else "\n"
    for name, body in regions.items():
        b, e = f"<!-- BEGIN GENERATED: {name} -->", f"<!-- END GENERATED: {name} -->"
        i = text.find(b)
        if i < 0 or text.count(b) != 1:
            raise ValueError(f"marker not found exactly once: {b}")
        j = text.find(e, i)
        if j < 0 or text.count(e) != 1:
            raise ValueError(f"marker not found exactly once after BEGIN: {e}")
        text = text[: i + len(b)] + eol + body.replace("\n", eol) + eol + text[j:]
    return text


def load_nodes(path):
    with open(path, encoding="utf-8") as f:
        data = yaml.safe_load(f)
    if isinstance(data, dict):
        extra = set(data) - {"nodes", "dropped", "suspected_edges"}
        if extra:
            raise ValueError(f"{path}: unknown top-level key(s) {sorted(extra)}")
        for k in ("dropped", "suspected_edges"):  # data only, never drawn
            if data.get(k) is not None and not isinstance(data[k], list):
                raise ValueError(f"{path}: '{k}' must be a list")
        data = data.get("nodes")
    if not isinstance(data, list):
        raise ValueError(f"{path}: nodes must be a list (bare, or under a top-level 'nodes' key)")
    return data


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--nodes", type=Path, default=NODES)
    ap.add_argument("--md", type=Path, default=MD)
    ap.add_argument("--today", help="YYYY-MM-DD used as the end of open bars (default: today)")
    a = ap.parse_args(argv)
    import check  # lazy: check imports this module
    try:
        nodes = load_nodes(a.nodes)
    except (OSError, ValueError, yaml.YAMLError) as e:
        print(f"build: {e}", file=sys.stderr)
        return 1
    errors, _ = check.validate_nodes(nodes)
    if errors:
        print("build: nodes.yaml is invalid, run check.py:", *errors, sep="\n  ", file=sys.stderr)
        return 1
    with open(a.md, encoding="utf-8", newline="") as f:
        old = f.read()
    try:
        new = apply_regions(old, render_regions(nodes, a.today))
    except ValueError as e:
        print(f"build: {a.md}: {e}", file=sys.stderr)
        return 1
    if new != old:
        with open(a.md, "w", encoding="utf-8", newline="") as f:
            f.write(new)
    print(f"build: {a.md} {'updated' if new != old else 'already up to date'} ({len(nodes)} nodes)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
