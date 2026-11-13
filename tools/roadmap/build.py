#!/usr/bin/env python3
"""Rewrite the GENERATED regions of docs/roadmap.md from docs/roadmap/nodes.yaml.

Everything outside the `<!-- BEGIN GENERATED: x -->` / `<!-- END GENERATED: x -->`
markers is hand-written and preserved byte for byte.

Each track's tree and node tables go to their own file, docs/roadmap/tracks/<code>.md,
which is generated in full (GitHub fails to render a page with too many Mermaid blocks).
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
EXT_SHOWN = 2       # other-track references named in a node label before '+N'
GANTT_TITLE = 40    # gantt task names are truncated to this many characters

REGION_ORDER = ["legend", "overview", "plain-words", "ready-now", "running", "built-waiting", "waiting-on-owner",
                "vm-queue", "timeline", "tracks"]
MD_KEY = "roadmap.md"
TRACK_DIR = "tracks"  # per-track files live in <md stem>/tracks/, i.e. docs/roadmap/tracks/
FORWARD_STATES = ("in-progress", "next", "blocked", "parked")  # nodes that still have work ahead

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
    ("defect", "fill:#ffe0b2,stroke:#d84315,stroke-width:2px,color:#111"),
    ("waiting", "fill:#e3f2fd,stroke:#1f6fc5,stroke-dasharray:5 5,color:#111"),
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
    if n["kind"] == "defect-verdict":  # a defect is open or fixed; it never carries an outcome word
        if s in ("in-progress", "next"):
            return "⚠ open defect"
        if s == "done":
            return "✔ fixed"
    if s == "done":
        return f"✔ done · {o}"
    if s == "abandoned":
        return "↷ superseded" if o == "superseded" else "✖ dead end"
    if s == "in-progress" and n["waiting_on"]:  # built, waiting on something outside the work itself
        return "⏳ waiting"
    return {"in-progress": "▶ in progress", "next": "○ next", "blocked": "⛔ blocked", "parked": "⏸ parked"}[s]


def node_class(n):
    s, o = n["state"], n["outcome"]
    if n["kind"] == "user-decision" and s not in ("done", "abandoned"):
        return "userdec"
    if n["kind"] == "defect-verdict" and s in ("in-progress", "next"):
        return "defect"
    if n["kind"] == "defect-verdict" and s == "done":
        return "succ"
    if s == "in-progress" and n["waiting_on"]:
        return "waiting"
    if s == "abandoned":
        return "dead"
    if s == "done":
        return {"very-successful": "vsucc", "successful": "succ", "mixed": "mixed", "bad": "bad",
                "dead-end": "dead", "superseded": "dead", "unrated": "unrated", "n/a": "userdec"}[o]
    return {"in-progress": "inprog", "next": "next", "blocked": "blocked", "parked": "parked"}[s]


def external_refs(n, ids):
    """Label line naming other-track nodes this node points at (prerequisites, replacement, qualifier)."""
    ext = list(dict.fromkeys(t for f in ("depends_on", "replaced_by", "qualified_by") for t in n[f] if t not in ids))
    if not ext:
        return ""
    return "ext: " + ", ".join(ext[:EXT_SHOWN]) + (f" +{len(ext) - EXT_SHOWN}" if len(ext) > EXT_SHOWN else "")


def node_def(n, ident=None, extra=""):
    title = "<br/>".join(esc(line) for line in textwrap.wrap(" ".join(n["title"].split()), WRAP) or [""])
    label = f"{title}<br/>{esc(tag(n))}{' ⚠' if n['qualified_by'] else ''}"
    if extra:
        label += "<br/>" + "<br/>".join(esc(line) for line in textwrap.wrap(extra, WRAP))
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


def reduced_deps(nodes):
    """depends_on with transitively implied edges dropped (a -> c is omitted when a -> b -> c exists).

    The tables keep the full depends_on; diagrams draw only this reduction so long redundant
    edges do not widen the layout. Assumes depends_on is acyclic (check.py enforces it).
    """
    by = {n["id"]: n for n in nodes}
    memo = {}

    def reach(i):  # every prerequisite of i, direct or not
        if i not in memo:
            memo[i] = set()
            for d in by[i]["depends_on"]:
                memo[i] |= {d} | reach(d)
        return memo[i]

    return {n["id"]: [d for d in n["depends_on"] if not any(d in reach(o) for o in n["depends_on"] if o != d)]
            for n in nodes}


def edges(nodes, keep):
    """Yield (src, dst, kind) for edges where keep(src_node, dst_node) holds.

    kind: "dep" (prerequisite -> dependant), "repl" (replaced by), "qual" (rated node -> qualifying defect).
    """
    by = {n["id"]: n for n in nodes}
    direct = reduced_deps(nodes)
    for n in nodes:
        for d in direct[n["id"]]:
            if keep(by[d], n):
                yield d, n["id"], "dep"
        for r in n["replaced_by"]:
            if keep(n, by[r]):
                yield n["id"], r, "repl"
        for q in n["qualified_by"]:
            if keep(n, by[q]):
                yield n["id"], q, "qual"
    queue = vm_queue(nodes)
    for a, b in zip(queue, queue[1:]):  # queue ORDER, not a dependency
        if keep(a, b):
            yield a["id"], b["id"], "then"


EDGE_ARROWS = {"dep": "-->", "repl": "-.->|replaced by|", "qual": "-.->|⚠ qualified by|", "then": "-.->|then|"}
QUAL_STYLE = "stroke:#e69500,stroke-width:2px,stroke-dasharray:2 3,color:#a86400"
THEN_STYLE = "stroke:#999,stroke-width:1px,color:#777"
_LINK_RE = re.compile(r"^\s+\S+ (?:-->|-\.->\|[^|]*\||~~~)")


def edge_line(a, b, kind, sa=None, sb=None):
    a, b = sa or sid(a), sb or sid(b)
    return f"    {a} {EDGE_ARROWS[kind]} {b}"


def link_styles(lines):
    """linkStyle lines for qualified_by and vm-queue 'then' links (Mermaid indexes links in order of appearance)."""
    idx, hits = 0, {"qual": [], "then": []}
    for line in lines:
        if _LINK_RE.match(line):
            if "⚠ qualified by" in line:
                hits["qual"].append(idx)
            elif "|then|" in line:
                hits["then"].append(idx)
            idx += line.count("~~~") or 1
    return [f"    linkStyle {','.join(map(str, h))} {style}"
            for h, style in ((hits["qual"], QUAL_STYLE), (hits["then"], THEN_STYLE)) if h]


def vm_queue(nodes):
    return sorted((n for n in nodes if n["vm_order"] is not None), key=lambda n: n["vm_order"])


def pack_rows(group, width):
    """Pack ALL nodes of `group` (connected or not) into rows of at most `width` node ids.

    Nodes are taken in rank order (rank = longest depends_on path inside the group) and
    filled greedily; a node never shares a row with one of its prerequisites.
    """
    ids = {n["id"] for n in group}
    by = {n["id"]: n for n in group}
    rank = {}

    def depth(i, seen=()):
        if i not in rank:
            rank[i] = 1 + max([depth(d, seen + (i,)) for d in by[i]["depends_on"] if d in ids and d not in seen], default=-1)
        return rank[i]

    rows = []
    for n in sorted(group, key=lambda n: depth(n["id"])):
        deps = set(n["depends_on"])
        if rows and len(rows[-1]) < width and not deps & set(rows[-1]):
            rows[-1].append(n["id"])
        else:
            rows.append([n["id"]])
    return rows


def row_links(rows, indent="    ", ident=sid):
    """Invisible links from every node of a row to every node of the next row.

    Mermaid puts nodes without edges on the first rank and lets edge-pulled nodes drift
    between ranks; a complete bipartite tie between consecutive rows holds each row on
    one rank, so rows stack vertically.
    """
    return [f"{indent}{ident(a)} ~~~ {ident(b)}" for prev, row in zip(rows, rows[1:]) for a in prev for b in row]


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
        out += row_links(pack_rows(group, OVERVIEW_WIDTH), "        ")
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
        out += [f"    {node_def(n, extra=external_refs(n, ids))}" for n in sel]
        out += row_links(pack_rows(sel, TRACK_WIDTH))
        # edges to other tracks are not drawn (stub nodes made trees wide and unreadable);
        # they appear as an "ext:" line in the node label and in that track's own tree
        out += [edge_line(a, b, kind) for a, b, kind in edges(nodes, lambda x, y: x["id"] in ids and y["id"] in ids)]
        out += link_styles(out)
        out += classdef_lines()
        out += class_lines([(sid(n["id"]), node_class(n)) for n in sel])
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


def r_table(code):
    """The track's node table, then its per-node details table (only nodes with notes)."""
    def render(nodes, today):
        sel = [n for n in nodes if n["track"] == code]
        if not sel:
            return "_No nodes yet._"
        rows = ["| id | title | state | outcome | headline | evidence | depends_on |", "|---|---|---|---|---|---|---|"]
        for n in sel:
            rows.append("| " + " | ".join([
                f"`{n['id']}`", cell(n["title"]), n["state"], n["outcome"], cell(n["headline"]),
                cell("; ".join(n["evidence"])), cell(", ".join(n["depends_on"])),
            ]) + " |")
        parts = ["\n".join(rows)]
        noted = [n for n in sel if n["notes"]]
        if noted:
            rows = ["| id | title | notes | evidence |", "|---|---|---|---|"]
            rows += [f"| `{n['id']}` | {cell(n['title'])} | {cell(n['notes'])} | {cell('; '.join(n['evidence']))} |" for n in noted]
            parts += ["### Details per node", "\n".join(rows)]
        return "\n\n".join(parts)
    return render


def track_path(code):
    return f"{TRACK_DIR}/{code}.md"


def r_tracks(nodes, today):
    """One line per track: link to its file, node count, counts by state."""
    lines = ["Each track has its own page with its tree, node table and per-node details.", ""]
    for code, (name, _) in TRACKS.items():
        sel = [n for n in nodes if n["track"] == code]
        counts = ", ".join(f"{k} {s.replace('-', ' ')}" for s in STATES if (k := sum(1 for n in sel if n["state"] == s)))
        lines.append(f"- [{code} - {name}](roadmap/{track_path(code)}): {len(sel)} nodes" + (f" ({counts})" if counts else ""))
    return "\n".join(lines)


def r_legend(nodes, today):
    samples = [
        ("vsucc", "very successful"), ("succ", "successful"), ("mixed", "mixed"), ("bad", "bad"),
        ("dead", "dead end / superseded / abandoned"), ("inprog", "in progress"), ("next", "next"),
        ("blocked", "blocked"), ("parked", "parked"), ("unrated", "done, unrated"),
        ("userdec", "user decision pending"), ("defect", "open defect"), ("waiting", "built, waiting"),
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
        "Diagrams omit prerequisite arrows that are implied by a longer chain (the table keeps the full `depends_on`). "
        "A per-track tree draws only arrows inside its track; a node's references to other tracks are written on its label as `ext: ...`.",
        "Thin grey dotted arrow = `then`: VM queue order, not a dependency. A defect verdict reads `⚠ open defect` or `✔ fixed` and never shows an outcome.",
        "Amber dotted arrow = `⚠ qualified by`: a later defect or verdict limits the rating of the done node it leaves; that node's label ends in `⚠`.",
    ]
    return "\n".join(text)


def r_plain(nodes, today):
    sel = [n for n in nodes if n["overview"] or n["state"] in FORWARD_STATES]
    if not sel:
        return "_No overview or forward nodes yet._"
    rows = ["| title | status | in plain words |", "|---|---|---|"]
    rows += [f"| {cell(n['title'])} | {cell(tag(n))}{' ⚠' if n['qualified_by'] else ''} | {cell(n['plain']) or '_(missing)_'} |"
             for code in TRACKS for n in sel if n["track"] == code]
    return "\n".join(rows)


def by_lane(sel):
    """Markdown bullet list of nodes grouped by lane (fixed lane order), or a 'none' note."""
    if not sel:
        return "_None._"
    parts = []
    for lane in LANES:
        group = [n for n in sel if n["lane"] == lane]
        if group:
            parts.append(f"**{lane}**\n\n" + "\n".join(f"- `{n['id']}`: {cell(n['title'])}" for n in group))
    return "\n\n".join(parts)


def r_ready(nodes, today):
    # the user lane is listed under "Waiting on the owner"
    return by_lane([n for n in nodes if n["state"] == "next" and n["lane"] != "user"])


def r_running(nodes, today):
    return by_lane([n for n in nodes if n["state"] == "in-progress" and n["kind"] != "defect-verdict" and not n["waiting_on"]])


def r_built_waiting(nodes, today):
    sel = [n for n in nodes if n["state"] == "in-progress" and n["kind"] != "defect-verdict" and n["waiting_on"]]
    if not sel:
        return "_None._"
    parts = []
    for why in dict.fromkeys(n["waiting_on"] for n in sel):  # first-seen order
        parts.append(f"**{cell(why)}**\n\n" + "\n".join(f"- `{n['id']}`: {cell(n['title'])}" for n in sel if n["waiting_on"] == why))
    return "\n\n".join(parts)


def r_waiting(nodes, today):
    sel = [n for n in nodes if n["kind"] == "user-decision" and n["state"] not in ("done", "abandoned")]
    return "\n".join(f"- `{n['id']}`: {cell(n['title'])}" for n in sel) or "_None._"


def r_vmqueue(nodes, today):
    q = vm_queue(nodes)
    return "\n".join(
        f"{i}. `{n['id']}`: {cell(n['title'])}" + (f". {cell(n['order_reason'])}" if n["order_reason"] else "")
        for i, n in enumerate(q, 1)) or "_The VM queue is empty._"


RENDERERS = {"legend": r_legend, "overview": r_overview, "timeline": r_timeline, "tracks": r_tracks,
             "plain-words": r_plain, "ready-now": r_ready, "running": r_running, "built-waiting": r_built_waiting,
             "waiting-on-owner": r_waiting, "vm-queue": r_vmqueue}
TRACK_RENDERERS = {f"track-{c}": r_track(c) for c in TRACKS} | {f"table-{c}": r_table(c) for c in TRACKS}


def normalize(nodes):
    """Copy nodes with dates as strings so renderers see plain data."""
    out = []
    for n in nodes:
        m = dict(n)
        m["start"], m["end"] = datestr(n.get("start")), datestr(n.get("end"))
        m.setdefault("qualified_by", [])
        m["notes"] = m.get("notes") or ""
        m["plain"] = m.get("plain") or ""
        m["waiting_on"] = m.get("waiting_on") or ""
        m["order_reason"] = m.get("order_reason") or ""
        m["vm_order"] = m.get("vm_order")
        out.append(m)
    return out


def render_regions(nodes, today=None):
    today = str(today or datetime.date.today())
    nodes = normalize(nodes)
    return {name: RENDERERS[name](nodes, today) for name in REGION_ORDER}


def render_track_file(nodes, code, today=None):
    """The whole text of one track's file; every byte is generated."""
    name, scope = TRACKS[code]
    nodes = normalize(nodes)
    today = str(today or datetime.date.today())

    def region(r):
        return f"<!-- BEGIN GENERATED: {r} -->\n{TRACK_RENDERERS[r](nodes, today)}\n<!-- END GENERATED: {r} -->"

    return "\n".join([
        "<!-- Generated by tools/roadmap/build.py from docs/roadmap/nodes.yaml. Do not edit. -->",
        "",
        f"# {code} - {name}",
        "",
        f"Scope: {scope}.",
        "",
        "[Back to the roadmap](../../roadmap.md)",
        "",
        region(f"track-{code}"),
        "",
        "## Node table",
        "",
        region(f"table-{code}"),
        "",
    ])


def render_outputs(nodes, md_text, today=None):
    """All generated output as {path relative to the roadmap.md directory: text}; no disk I/O.

    The key of roadmap.md itself is MD_KEY.
    """
    out = {MD_KEY: apply_regions(md_text, render_regions(nodes, today))}
    out.update({f"roadmap/{track_path(c)}": render_track_file(nodes, c, today) for c in TRACKS})
    return out


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


def stale_track_files(md_path):
    """Files in the tracks directory that belong to no code in TRACKS."""
    d = Path(md_path).parent / "roadmap" / TRACK_DIR
    return sorted(p for p in d.glob("*.md") if p.stem not in TRACKS) if d.is_dir() else []


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
        outputs = render_outputs(nodes, old, a.today)
    except ValueError as e:
        print(f"build: {a.md}: {e}", file=sys.stderr)
        return 1
    changed = []
    for key, text in outputs.items():
        path = a.md.parent / key if key != MD_KEY else a.md
        try:
            current = path.read_bytes().decode("utf-8") if path.exists() else None
        except OSError as e:
            print(f"build: {path}: {e}", file=sys.stderr)
            return 1
        if current != text:
            path.parent.mkdir(parents=True, exist_ok=True)
            with open(path, "w", encoding="utf-8", newline="") as f:
                f.write(text)
            changed.append(path)
    for path in stale_track_files(a.md):  # tracks that no longer exist in TRACKS
        path.unlink()
        changed.append(path)
    print(f"build: {a.md} {'updated (%d file(s) written)' % len(changed) if changed else 'already up to date'} ({len(nodes)} nodes)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
