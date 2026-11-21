#!/usr/bin/env python3
"""Rewrite the GENERATED regions of the main page from the nodes file; write the track and timeline pages.

Everything outside the `<!-- BEGIN GENERATED: x -->` / `<!-- END GENERATED: x -->` markers is
hand-written and preserved byte for byte. Each track's tree and node tables go to
<page dir>/<page stem>/tracks/<code>.md and the Gantt chart to <page dir>/<page stem>/timeline.md;
both are generated in full (GitHub fails to render a page with too many Mermaid blocks).

All behaviour that depends on the project comes from tech-tree.yaml (tracks, kinds and their
attributes, lanes, queue_lane); no track, kind or lane name appears in this file.
"""
import argparse
import datetime
import heapq
import os
import re
import sys
import textwrap
from pathlib import Path

import yaml

sys.path.insert(0, str(Path(__file__).resolve().parent))
import tree_config  # noqa: E402

STATES = ["done", "in-progress", "next", "blocked", "parked", "abandoned"]
OUTCOMES = ["very-successful", "successful", "mixed", "bad", "dead-end", "superseded", "unrated", "n/a"]
NOT_RATED = ("unrated", "n/a")  # outcomes that need no verdict quote and count as no rating

WRAP = 22           # label line width in characters
# row widths and the month-band minimum are config (`layout`, see tree_config.LAYOUT_DEFAULTS)
GANTT_TITLE = 40    # gantt task names are truncated to this many characters

GENERATED_ON = "Generated on {}."  # check.py parses the date back out of this sentence
FORWARD_STATES = ("in-progress", "next", "blocked", "parked")  # nodes that still have work ahead
PLAIN_KIND = {"shape": "box", "rated": True, "owner_gate": False, "blocks": True}  # attributes of a sample box

# shape name -> (opening, closing) around the quoted label
SHAPES = {"box": ('["', '"]'), "slant": ('[/"', '"/]'), "hexagon": ('{{"', '"}}'), "flag": ('>"', '"]'),
          "round": ('("', '")')}

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
    ("gate", "fill:#d9c2f0,stroke:#7b3fb5,color:#111"),
    ("decided", "fill:#ece0f7,stroke:#7b3fb5,stroke-dasharray:2 2,color:#111"),
    ("finding", "fill:#ffe0b2,stroke:#d84315,stroke-width:2px,color:#111"),
    ("waiting", "fill:#e3f2fd,stroke:#1f6fc5,stroke-dasharray:5 5,color:#111"),
]

_ESC = {
    "#": "#35;", '"': "#quot;", "&": "#amp;", "<": "#lt;", ">": "#gt;",
    "[": "#91;", "]": "#93;", "{": "#123;", "}": "#125;", "|": "#124;",
    "`": "#96;", chr(0xA7): "#167;", "~": "#126;",
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


def kind_of(cfg, n):
    """The configured attributes of a node's kind."""
    return cfg.kinds[n["kind"]]


def is_rated(n):
    """A node whose outcome is a real rating (the quote it needs is checked by check.py)."""
    return n["outcome"] not in NOT_RATED


# ---------------------------------------------------------------- status, class, shape
def tag(n, k):
    s, o = n["state"], n["outcome"]
    if not k["rated"]:  # a finding is open or fixed; it never carries an outcome word
        if s in ("in-progress", "next"):
            return "⚠ open"
        if s == "done":
            return "✔ fixed"
    if s == "done":
        return f"✔ done · {o}"
    if s == "abandoned":
        return "↷ superseded" if o == "superseded" else "✖ dead end"
    if s == "in-progress" and n["waiting_on"]:  # built, waiting on something outside the work itself
        return "⏳ waiting"
    return {"in-progress": "▶ in progress", "next": "○ next", "blocked": "⛔ blocked",
            "parked": "⏸ parked"}[s]


def glyph(n, k):
    """The one status glyph on a diagram label (the sentence form, tag(), is for the tables)."""
    return tag(n, k).split()[0]


def node_class(n, k):
    s, o = n["state"], n["outcome"]
    if k["owner_gate"] and s not in ("done", "abandoned"):
        return "gate"
    if k["owner_gate"] and s == "done":  # a decided owner call: drawn in the overview only
        return "decided"
    if not k["rated"] and s in ("in-progress", "next"):
        return "finding"
    if not k["rated"] and s == "done":
        return "succ"
    if s == "in-progress" and n["waiting_on"]:
        return "waiting"
    if s == "abandoned":
        return "dead"
    if s == "done":
        return {"very-successful": "vsucc", "successful": "succ", "mixed": "mixed", "bad": "bad",
                "dead-end": "dead", "superseded": "dead", "unrated": "unrated", "n/a": "gate"}[o]
    return {"in-progress": "inprog", "next": "next", "blocked": "blocked", "parked": "parked"}[s]


def node_def(n, k, ident=None, prefix=""):
    """One box: [track prefix .] glyph + title. Colour (classDef) carries state and outcome, shape the kind."""
    text = f"{prefix} · " if prefix else ""
    lines = textwrap.wrap(f"{text}{glyph(n, k)} " + " ".join(n["title"].split()), WRAP) or [""]
    label = "<br/>".join(esc(line) for line in lines)
    opening, closing = SHAPES[k["shape"]]
    return f"{ident or sid(n['id'])}{opening}{label}{closing}"


def class_lines(pairs, indent="    "):
    groups = {}
    for ident, cls in pairs:
        groups.setdefault(cls, []).append(ident)
    return [f"{indent}class {','.join(ids)} {cls}" for cls, ids in groups.items()]


def classdef_lines(indent="    "):
    return [f"{indent}classDef {name} {style}" for name, style in CLASSDEFS]


# ---------------------------------------------------------------- graph
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


def precedence(nodes):
    """{id: ids that must come before it}: its depends_on, and the node it replaces (old before new)."""
    ids = {n["id"] for n in nodes}
    pre = {n["id"]: [d for d in n["depends_on"] if d in ids] for n in nodes}
    for n in nodes:
        for r in n["replaced_by"]:
            if r in ids and n["id"] not in pre[r]:
                pre[r].append(n["id"])
    return pre


def hidden(cfg, n, owner_done=False):
    """Owner-gate nodes (and nodes with `diagram: false`) are not drawn; tables and lists still carry them.
    With owner_done (the overview only) a done owner gate is drawn; an open one never is."""
    if not n["diagram"]:
        return True
    return kind_of(cfg, n)["owner_gate"] and not (owner_done and n["state"] == "done")


def diagram_view(cfg, nodes, owner_done=False):
    """The nodes the diagrams draw: hidden ones removed, a dependency that ran through one rewired
    from its prerequisites to its dependants. replaced_by / qualified_by into a hidden node are dropped.
    owner_done: also draw done owner gates (the overview does; the track trees do not)."""
    by = {n["id"]: n for n in nodes}
    pre = precedence(nodes)

    def hid(n):
        return hidden(cfg, n, owner_done)

    def visible_pre(i, seen=()):
        out = []
        for p in pre[i]:
            if hid(by[p]):
                out += [q for q in visible_pre(p, seen + (i,)) if p not in seen]
            else:
                out.append(p)
        return out

    view = []
    for n in nodes:
        if hid(n):
            continue
        m = dict(n)
        m["depends_on"] = list(dict.fromkeys(d for x in n["depends_on"]
                                             for d in ([x] if not hid(by[x]) else visible_pre(x))))
        m["replaced_by"] = [r for r in n["replaced_by"] if not hid(by[r])]
        m["qualified_by"] = [q for q in n["qualified_by"] if not hid(by[q])]
        view.append(m)
    return view


def time_order(nodes):
    """ONE global order of node ids: topological on precedence(), ties by start date, undated last.

    Every diagram fills its rows in this order, so every drawn edge points down = forward in time.
    """
    pre = precedence(nodes)
    pos = {n["id"]: k for k, n in enumerate(nodes)}
    start = {n["id"]: datestr(n.get("start")) for n in nodes}
    waiting = {i: len(p) for i, p in pre.items()}
    succ = {i: [] for i in pre}
    for i, ps in pre.items():
        for p in ps:
            succ[p].append(i)

    def key(i):
        return (0 if start[i] else 1, start[i], pos[i])

    ready = [(key(i), i) for i, w in waiting.items() if w == 0]
    heapq.heapify(ready)
    order = []
    while ready:
        _, i = heapq.heappop(ready)
        order.append(i)
        for j in succ[i]:
            waiting[j] -= 1
            if waiting[j] == 0:
                heapq.heappush(ready, (key(j), j))
    order += [i for i in pre if i not in set(order)]  # only on a cycle, which check.py rejects
    return order


EDGE_ARROWS = {"dep": "-->", "repl": "-.->", "qual": "-.->"}
QUAL_STYLE = "stroke:#e69500,stroke-width:3px,stroke-dasharray:2 3"


def queue_nodes(nodes):
    """Nodes with a queue_order, in that order."""
    return sorted((n for n in nodes if n["queue_order"] is not None), key=lambda n: n["queue_order"])


def pack_rows(ordered, width):
    """Pack the nodes, already in time order, into rows of at most `width` ids.

    A node starts a new row when the row is full or holds one of its prerequisites, so every
    depends_on / replaced_by edge goes from a row to a LATER row.
    """
    ids = {n["id"] for n in ordered}
    pre = precedence(ordered)
    rows = []
    for n in ordered:
        if rows and len(rows[-1]) < width and not set(pre[n["id"]]) & set(rows[-1]):
            rows[-1].append(n["id"])
        else:
            rows.append([n["id"]])
    assert {i for r in rows for i in r} == ids
    return rows


def row_links(rows, indent="    ", ident=sid):
    """Invisible links tying each row to the next, column by column (a short row's last node takes the spare columns).

    Mermaid puts nodes without edges on the first rank and lets edge-pulled nodes drift
    between ranks; a tie into and out of every node holds each row on one rank, and tying
    columns keeps the rows aligned like a grid instead of spreading sideways.
    """
    out = []
    for prev, row in zip(rows, rows[1:]):
        pairs = dict.fromkeys((prev[min(k, len(prev) - 1)], row[min(k, len(row) - 1)]) for k in range(max(len(prev), len(row))))
        out += [f"{indent}{ident(a)} ~~~ {ident(b)}" for a, b in pairs]
    return out


def drawn_edges(nodes, rows):
    """([(src, dst, kind)], dropped): edges among the nodes in `rows`, only those pointing to a later row.

    dependency edges are transitively reduced (the tables keep the full depends_on). `dropped` counts
    qualified_by edges that point to the same or an earlier row; they stay in the tables.
    """
    row_of = {i: k for k, r in enumerate(rows) for i in r}
    by = {n["id"]: n for n in nodes}
    direct = reduced_deps(nodes)
    out, dropped = [], 0
    for i in row_of:
        n = by[i]
        cand = ([(d, i, "dep") for d in direct[i] if d in row_of] + [(i, r, "repl") for r in n["replaced_by"] if r in row_of]
                + [(i, q, "qual") for q in n["qualified_by"] if q in row_of])
        for a, b, kind in cand:
            if row_of[a] < row_of[b]:
                out.append((a, b, kind))
            else:
                dropped += 1
    return out, dropped


def edge_block(nodes, rows, indent="    "):
    """Row ties, then edges, then the linkStyle for the qualified ones (links are indexed in order of appearance)."""
    ties = row_links(rows, indent)
    es, _ = drawn_edges(nodes, rows)
    lines = ties + [f"{indent}{sid(a)} {EDGE_ARROWS[k]} {sid(b)}" for a, b, k in es]
    qual = [len(ties) + j for j, (_, _, k) in enumerate(es) if k == "qual"]
    if qual:
        lines.append(f"{indent}linkStyle {','.join(map(str, qual))} {QUAL_STYLE}")
    return lines


def fence(lines):
    return "```mermaid\n" + "\n".join(lines) + "\n```"


MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]


def month_name(ym):
    return f"{MONTHS[int(ym[5:7]) - 1]} {ym[:4]}"


def overview_bands(cfg, sel, order):
    """[(label, [node ids])] in time order: past months (merged until a band holds layout.band_min nodes), now, next, later.

    A node's band follows its own state and date: done, abandoned and open finding nodes go to the month of
    their start; in-progress to Now, next to Next, blocked and parked to Later. An edge that would then point
    up is not drawn (drawn_edges drops it); the tables keep it.
    """
    by = {n["id"]: n for n in sel}

    def base(n):
        if n["state"] in ("done", "abandoned") or (not kind_of(cfg, n)["rated"] and n["start"]):
            return (0, n["start"][:7])
        return {"in-progress": (1, ""), "next": (2, "")}.get(n["state"], (3, ""))

    rank = {i: base(by[i]) for i in order if i in by}
    ordered = sorted(rank, key=lambda i: rank[i])  # stable: keeps the time order inside a rank
    past = {}
    for i in ordered:
        if rank[i][0] == 0:
            past.setdefault(rank[i][1], []).append(i)
    bands, months, acc = [], [], []
    for ym, ids in past.items():  # month order; "" (undated) comes first
        acc += ids
        months.append(ym)
        if len(acc) >= cfg.layout["band_min"] or ym == "":
            bands.append((months, acc))
            months, acc = [], []
    if acc:
        if bands and bands[-1][0] != [""]:
            bands[-1] = (bands[-1][0] + months, bands[-1][1] + acc)
        else:
            bands.append((months, acc))
    out = []
    for months, ids in bands:
        dated = [m for m in months if m]
        if not dated:
            label = "Earlier (no start date)"
        elif len(dated) == 1:
            label = month_name(dated[0])
        else:
            label = f"{month_name(dated[0])} to {month_name(dated[-1])}"
        out.append((label, ids))
    for r, label in ((1, "Now (in progress)"), (2, "Next"), (3, "Later (blocked or parked)")):
        ids = [i for i in ordered if rank[i][0] == r]
        if ids:
            out.append((label, ids))
    return out


# ---------------------------------------------------------------- regions
def r_overview(cfg, nodes, today):
    nodes = diagram_view(cfg, nodes, owner_done=True)
    sel = [n for n in nodes if n["overview"]]
    if not sel:
        return "_No nodes have `overview: true` yet._"
    by = {n["id"]: n for n in sel}
    bands = overview_bands(cfg, sel, time_order(nodes))
    out = ["flowchart TB"]
    rows = []
    for k, (label, ids) in enumerate(bands):
        out.append(f'    subgraph bd_{k}["{esc(label)}"]')
        out += [f"        {node_def(by[i], kind_of(cfg, by[i]), prefix=by[i]['track'])}" for i in ids]
        out.append("    end")
        rows += pack_rows([by[i] for i in ids], cfg.layout["overview_width"])
    out += edge_block(nodes, rows)
    out += classdef_lines()
    out += class_lines([(sid(n["id"]), node_class(n, kind_of(cfg, n))) for n in sel])
    return fence(out)


def track_nodes(nodes, code):
    """The track's nodes in the global time order."""
    by = {n["id"]: n for n in nodes if n["track"] == code}
    return [by[i] for i in time_order(nodes) if i in by]


def r_track(cfg, code):
    def render(nodes, today):
        nodes = diagram_view(cfg, nodes)
        sel = track_nodes(nodes, code)
        if not sel:
            return "_No nodes in this track yet._"
        rows = pack_rows(sel, cfg.layout["track_width"])
        # edges to other tracks are not drawn (stub nodes made trees wide and unreadable);
        # they stay in the node table's depends_on column and in the other track's own tree
        out = ["flowchart TB"]
        out += [f"    {node_def(n, kind_of(cfg, n))}" for n in sel]
        out += edge_block(nodes, rows)
        out += classdef_lines()
        out += class_lines([(sid(n["id"]), node_class(n, kind_of(cfg, n))) for n in sel])
        return fence(out)
    return render


def gantt_text(s, limit=None):
    t = " ".join(str(s).replace(":", " -").replace(";", ",").replace("#", "").replace('"', "'").split())
    return t if limit is None or len(t) <= limit else t[: limit - 1].rstrip() + "…"


def r_timeline(cfg, nodes, today):
    """The Gantt chart (timeline page)."""
    out = ["gantt", "    dateFormat YYYY-MM-DD", "    axisFormat %Y-%m-%d"]
    undated, any_task = [], False
    for code, (name, _) in cfg.tracks.items():
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
    parts = [fence(out) if any_task else "_No node has a start date yet._"]
    if undated:
        parts.append("Not shown (no `start`): " + ", ".join(f"`{i}`" for i in undated))
    return "\n\n".join(parts)


def r_timeline_link(cfg, nodes, today):
    """Main page: a link to the timeline page, and the build date."""
    return (f"[The timeline]({cfg.page_stem}/timeline.md) shows every dated node as a bar from its start to its end.\n\n"
            + GENERATED_ON.format(today))


def cell(s):
    return " ".join(str(s).split()).replace("|", "\\|")


# a node gets a row in the details table when any of these is set
DETAIL_FIELDS = ("verdict_quote", "headline", "notes", "plain", "confirmed", "waiting_on", "queue_order", "order_reason")


def r_table(cfg, code):
    """The track's node table, its per-node details (verdict, headline, plain words, notes) and its folded items."""
    def render(nodes, today):
        sel = [n for n in nodes if n["track"] == code]
        if not sel:
            return "_No nodes yet._"
        rows = ["| id | title | kind | lane | state | outcome | start | end | depends_on | replaced_by | qualified_by | headline | evidence |",
                "|" + "---|" * 13]
        for n in sel:
            rows.append("| " + " | ".join([
                f"`{n['id']}`", cell(n["title"]), cell(n["kind"]), cell(n["lane"]), n["state"], n["outcome"],
                n["start"], n["end"], cell(", ".join(n["depends_on"])), cell(", ".join(n["replaced_by"])),
                cell(", ".join(n["qualified_by"])), cell(n["headline"]), cell("; ".join(n["evidence"])),
            ]) + " |")
        parts = ["\n".join(rows)]
        detailed = [n for n in sel if any(n[f] not in ("", None) for f in DETAIL_FIELDS)]
        if detailed:
            rows = ["| id | title | verdict | headline | in plain words | notes | confirmed | waiting on | queue order | order reason |",
                    "|" + "---|" * 10]
            rows += [f"| `{n['id']}` | {cell(n['title'])} | {cell(n['verdict_quote'])} | {cell(n['headline'])} | "
                     f"{cell(n['plain'])} | {cell(n['notes'])} | {n['confirmed']} | {cell(n['waiting_on'])} | "
                     f"{'' if n['queue_order'] is None else n['queue_order']} | {cell(n['order_reason'])} |" for n in detailed]
            parts += ["### Details per node", "\n".join(rows)]
        withf = [n for n in sel if n["folded"]]
        if withf:
            lines = []
            for n in withf:
                lines.append(f"- `{n['id']}`: {cell(n['title'])}")
                lines += [f"  - {cell(f)}" for f in n["folded"]]
            parts += ["### Folded items per node", "\n".join(lines)]
        return "\n\n".join(parts)
    return render


def r_tracks(cfg, nodes, today):
    """One line per track: link to its file, node count, counts by state."""
    lines = ["Each track has its own page with its tree, node table and per-node details.", ""]
    for code, (name, _) in cfg.tracks.items():
        sel = [n for n in nodes if n["track"] == code]
        counts = ", ".join(f"{k} {s.replace('-', ' ')}" for s in STATES if (k := sum(1 for n in sel if n["state"] == s)))
        lines.append(f"- [{code} - {name}]({cfg.page_stem}/tracks/{code}.md): {len(sel)} nodes" + (f" ({counts})" if counts else ""))
    return "\n".join(lines)


LEGEND_MARK = "%% legend"  # first line after the flowchart header; tests that walk the trees skip this block

# (edge kind, from label, to label); drawn with EDGE_ARROWS and, for qual, QUAL_STYLE
LEGEND_EDGES = [("dep", "prerequisite", "depends on it"), ("repl", "old design", "replaced by"),
                ("qual", "rated node", "qualified by a later finding")]


def _sample(title, state="next", outcome="unrated", waiting_on=""):
    return {"title": title, "state": state, "outcome": outcome, "waiting_on": waiting_on}


def legend_status(cfg):
    """[(subgraph title, [(class, sample node, kind attributes)])]: one sample per colour the trees can draw,
    built by the same node_def/node_class as the trees, so a change to a glyph, shape or classDef shows up here too."""
    def row(*specs):
        return [(node_class(s, k), s, k) for s, k in ((s, PLAIN_KIND) for s in specs)]

    done = row(_sample("very successful", "done", "very-successful"), _sample("successful, or fixed", "done", "successful"),
               _sample("mixed", "done", "mixed"), _sample("bad (regression)", "done", "bad"),
               _sample("done, not rated", "done"))
    closed = row(_sample("dead end", "abandoned", "dead-end"), _sample("superseded", "abandoned", "superseded"))
    open_ = row(_sample("in progress", "in-progress"), _sample("built, waiting", "in-progress", waiting_on="x"),
                _sample("next"), _sample("blocked", "blocked"), _sample("parked", "parked"))
    unrated_kinds = [k for k in cfg.kinds.values() if not k["rated"]]
    if unrated_kinds:
        s = _sample("open, never rated", "in-progress")
        open_.append((node_class(s, unrated_kinds[0]), s, unrated_kinds[0]))
    return [("Done: colour is the rating", done), ("Closed", closed), ("Open: colour is the state", open_)]


def legend_shapes(cfg):
    """One sample per configured kind, drawn with the kind's shape: [(class, sample node, kind attributes)]."""
    out = []
    for name, k in cfg.kinds.items():
        s = _sample(name + (" (open, not drawn)" if k["owner_gate"] else ""), "next" if k["rated"] else "in-progress")
        out.append((node_class(s, k), s, k))
        if k["owner_gate"]:  # a decided owner call is drawn in the overview, in its own colour
            d = _sample(name + " (decided)", "done", "n/a")
            out.append((node_class(d, k), d, k))
    return out


def r_legend(cfg, nodes, today):
    out, pairs, groups = ["flowchart TB", f"    {LEGEND_MARK}"], [], []

    def group(ident, title, lines):
        groups.append(ident)
        return [f'    subgraph {ident}["{esc(title)}"]', "        direction LR"] + [f"        {x}" for x in lines] + ["    end"]

    k = 0
    for g, (title, samples) in enumerate(legend_status(cfg)):
        lines = []
        for cls, s, attrs in samples:
            lines.append(node_def(s, attrs, ident=f"lg_s{k}"))
            pairs.append((f"lg_s{k}", cls))
            k += 1
        out += group(f"lg_g{g}", title, lines)
    shapes = legend_shapes(cfg)
    out += group("lg_shape", "Shape: kind", [node_def(s, attrs, ident=f"lg_k{j}") for j, (_, s, attrs) in enumerate(shapes)])
    pairs += [(f"lg_k{j}", cls) for j, (cls, _, _) in enumerate(shapes)]
    out += group("lg_edge", "Arrow (in the trees every arrow points down, forward in time)",
                 [f'lg_e{j}{end}["{esc(t)}"]' for j, (_, a, b) in enumerate(LEGEND_EDGES) for end, t in (("a", a), ("b", b))])
    pairs += [(f"lg_e{j}{end}", "next") for j in range(len(LEGEND_EDGES)) for end in "ab"]
    out.append("    " + " ~~~ ".join(groups))  # stacks the rows; these ties are the first len(groups) - 1 links
    out += [f"    lg_e{j}a {EDGE_ARROWS[kind]} lg_e{j}b" for j, (kind, _, _) in enumerate(LEGEND_EDGES)]
    qual = [str(len(groups) - 1 + j) for j, (kind, _, _) in enumerate(LEGEND_EDGES) if kind == "qual"]
    out += [f"    linkStyle {','.join(qual)} {QUAL_STYLE}"]
    out += classdef_lines() + class_lines(pairs)
    notes = ["The prefix on an overview box is its track. A prerequisite implied by a longer chain is not drawn; "
             "the node tables keep every edge."]
    if any(a["owner_gate"] for a in cfg.kinds.values()):
        notes.insert(0, "Owner calls are the purple shape. A decided owner call is drawn in the overview only, never "
                        "in the track trees; an open one is listed under *Waiting on the owner*.")
    return "\n".join([fence(out), "", " ".join(notes)])


def r_ratings(cfg, nodes, today):
    rated = [n for n in nodes if is_rated(n)]
    if not rated:
        return "No node carries a rating yet."
    unconfirmed = [n for n in rated if not n["confirmed"]]
    text = f"{len(rated) - len(unconfirmed)} of {len(rated)} rated nodes are confirmed by the owner."
    if unconfirmed:
        text += "\n\nNot yet confirmed: " + ", ".join(f"`{n['id']}`" for n in unconfirmed) + "."
    return text


def r_plain(cfg, nodes, today):
    sel = [n for n in nodes if n["overview"] or n["state"] in FORWARD_STATES]
    if not sel:
        return "_No overview or forward nodes yet._"
    rows = ["| title | status | in plain words |", "|---|---|---|"]
    rows += [f"| {cell(n['title'])} | {cell(tag(n, kind_of(cfg, n)))}{' ⚠' if n['qualified_by'] else ''} | "
             f"{cell(n['plain']) or '_(missing)_'} |"
             for code in cfg.tracks for n in sel if n["track"] == code]
    return "\n".join(rows)


def by_lane(cfg, sel):
    """Markdown bullet list of nodes grouped by lane (the config's lane order), or a note saying so."""
    if not sel:
        return "_None._"
    parts = []
    for lane in cfg.lanes:
        group = [n for n in sel if n["lane"] == lane]
        if group:
            parts.append(f"**{lane}**\n\n" + "\n".join(f"- `{n['id']}`: {cell(n['title'])}" for n in group))
    return "\n\n".join(parts)


def r_ready(cfg, nodes, today):
    # owner-gate nodes are listed under "Waiting on the owner"
    return by_lane(cfg, [n for n in nodes if n["state"] == "next" and not kind_of(cfg, n)["owner_gate"]])


def r_running(cfg, nodes, today):
    return by_lane(cfg, [n for n in nodes if n["state"] == "in-progress" and kind_of(cfg, n)["rated"] and not n["waiting_on"]])


def r_built_waiting(cfg, nodes, today):
    sel = [n for n in nodes if n["state"] == "in-progress" and kind_of(cfg, n)["rated"] and n["waiting_on"]]
    if not sel:
        return "_None._"
    parts = []
    for why in dict.fromkeys(n["waiting_on"] for n in sel):  # first-seen order
        parts.append(f"**{cell(why)}**\n\n" + "\n".join(f"- `{n['id']}`: {cell(n['title'])}" for n in sel if n["waiting_on"] == why))
    return "\n\n".join(parts)


def r_waiting(cfg, nodes, today):
    sel = [n for n in nodes if kind_of(cfg, n)["owner_gate"] and n["state"] not in ("done", "abandoned")]
    return "\n".join(f"- `{n['id']}`: {cell(n['title'])}" for n in sel) or "_None._"


def r_queue(cfg, nodes, today):
    q = queue_nodes(nodes)
    return "\n".join(
        f"{i}. `{n['id']}`: {cell(n['title'])}" + (f". {cell(n['order_reason'])}" if n["order_reason"] else "")
        for i, n in enumerate(q, 1)) or "_The queue is empty._"


# A folded item written "<title> (<date>, commit <sha>[, commit <sha>...])" or "<title> (<date>, session record)"
# is a labelled record; the older bare refs ("commit <sha>", "branch <name>", ...) are not.
LABELLED_RE = re.compile(r"^(?P<title>.+) \((?P<date>\d{4}-\d{2}-\d{2}), "
                         r"(?:commit [0-9a-f]{7,40}(?:, commit [0-9a-f]{7,40})*|session record)\)$")


def labelled(item):
    """(title, date) of a labelled folded item, else None."""
    m = LABELLED_RE.match(item)
    return (m.group("title"), m.group("date")) if m else None


def r_done_by_month(cfg, nodes, today):
    """Every finished node (done or abandoned), grouped by the month it ended (else started), newest month
    first, undated last; each line carries the node's plain sentence (else its headline), and the node's labelled
    folded records follow as sub-bullets. A month between the first and the last with nothing finished is named."""
    sel = [n for n in nodes if n["state"] in ("done", "abandoned")]
    if not sel:
        return "_Nothing done yet._"
    groups = {}
    for n in sel:
        groups.setdefault((n["end"] or n["start"])[:7], []).append(n)
    parts = []
    dated = sorted(k for k in groups if k)
    months = []
    if dated:
        y, m = int(dated[0][:4]), int(dated[0][5:7])
        while f"{y:04d}-{m:02d}" <= dated[-1]:
            months.append(f"{y:04d}-{m:02d}")
            y, m = (y + 1, 1) if m == 12 else (y, m + 1)
    quiet = []  # a run of consecutive months with nothing finished, newest first; written as one line

    def flush_quiet():
        if quiet:
            span = month_name(quiet[0]) if len(quiet) == 1 else f"{month_name(quiet[-1])} to {month_name(quiet[0])}"
            parts.append(f"<b>{span}</b>: nothing finished")
            quiet.clear()

    for ym in sorted(months, reverse=True) + ([""] if "" in groups else []):
        if ym not in groups:
            quiet.append(ym)
            continue
        flush_quiet()
        group = sorted(groups[ym], key=lambda n: (n["end"] or n["start"], n["id"]))
        head = month_name(ym) if ym else "Undated"
        done = sum(1 for n in group if n["state"] == "done")
        count = f"{done} done" + (f", {len(group) - done} abandoned" if len(group) > done else "")
        lines = []
        for n in group:
            say = n["plain"] or n["headline"]
            lines.append(f"- `{n['id']}`: {cell(n['title'])} ({cell(tag(n, kind_of(cfg, n)))})" + (f". {cell(say)}" if say else ""))
            lines += [f"  - {cell(t)} ({d})" for t, d in filter(None, map(labelled, n["folded"]))]
        body = "\n".join(lines)
        parts.append(f"<details><summary><b>{head}</b>: {count}</summary>\n\n{body}\n\n</details>")
    return "\n\n".join(parts)


RENDERERS = {"legend": r_legend, "ratings": r_ratings, "overview": r_overview, "timeline": r_timeline_link,
             "tracks": r_tracks, "plain-words": r_plain, "ready-now": r_ready, "running": r_running,
             "built-waiting": r_built_waiting, "waiting-on-owner": r_waiting, "queue": r_queue,
             "done-by-month": r_done_by_month}


def region_order(cfg):
    """The regions of the main page, in page order; `queue` only when the config has a queue lane."""
    names = ["legend", "ratings", "overview", "plain-words", "ready-now", "running", "built-waiting", "waiting-on-owner"]
    if cfg.queue_lane:
        names.append("queue")
    return names + ["timeline", "done-by-month", "tracks"]


HEADINGS = {"legend": "Legend", "ratings": "Ratings", "overview": "Overview", "plain-words": "In plain words",
            "ready-now": "Ready now", "running": "Running", "built-waiting": "Built, waiting",
            "waiting-on-owner": "Waiting on the owner", "queue": "Queue", "timeline": "Timeline",
            "done-by-month": "Done, by month", "tracks": "Tracks"}


def skeleton(cfg):
    """A first main page: a heading per region around empty markers. Written only when the page does not exist;
    from then on it is hand-edited text and only the regions are rewritten."""
    parts = ["# Roadmap", "", f"Everything between the GENERATED markers is rebuilt from `{cfg.nodes_rel()}`; "
             "edit that file, not the regions. Text outside the markers is kept.", ""]
    for r in region_order(cfg):
        parts += [f"## {HEADINGS[r]}", "", f"<!-- BEGIN GENERATED: {r} -->", f"<!-- END GENERATED: {r} -->", ""]
    return "\n".join(parts)


def normalize(nodes):
    """Copy nodes with dates as strings and optional fields filled in, so renderers see plain data."""
    out = []
    for n in nodes:
        m = dict(n)
        m["start"], m["end"] = datestr(n.get("start")), datestr(n.get("end"))
        m["confirmed"] = datestr(n.get("confirmed"))
        m.setdefault("qualified_by", [])
        for f in ("notes", "plain", "waiting_on", "order_reason"):
            m[f] = n.get(f) or ""
        m["diagram"] = n.get("diagram", True)
        m["queue_order"] = n.get("queue_order")
        out.append(m)
    return out


def _today(today):
    return str(today or datetime.date.today())


def render_regions(cfg, nodes, today=None):
    today = _today(today)
    nodes = normalize(nodes)
    return {name: RENDERERS[name](cfg, nodes, today) for name in region_order(cfg)}


def _banner(cfg):
    return f"<!-- Generated by build.py from {cfg.nodes_rel()}. Do not edit. -->"


def render_track_file(cfg, nodes, code, today=None):
    """The whole text of one track's file; every byte is generated."""
    name, scope = cfg.tracks[code]
    nodes = normalize(nodes)
    today = _today(today)
    page = cfg.page_path.name
    renderers = {f"track-{code}": r_track(cfg, code), f"table-{code}": r_table(cfg, code)}

    def region(r):
        return f"<!-- BEGIN GENERATED: {r} -->\n{renderers[r](nodes, today)}\n<!-- END GENERATED: {r} -->"

    return "\n".join([
        _banner(cfg),
        "",
        f"# {code} - {name}",
        "",
        f"Scope: {scope}." if scope else f"Track: {name}.",
        "",
        f"[Back to the page](../../{page}) · [legend](../../{page}#legend)",
        "",
        region(f"track-{code}"),
        "",
        "## Node table",
        "",
        region(f"table-{code}"),
        "",
    ])


def render_timeline_file(cfg, nodes, today=None):
    """The whole text of the timeline page; every byte is generated."""
    nodes = normalize(nodes)
    today = _today(today)
    return "\n".join([
        _banner(cfg),
        "",
        "# Timeline",
        "",
        f"[Back to the page](../{cfg.page_path.name})",
        "",
        GENERATED_ON.format(today) + " Open bars end on that date.",
        "",
        f"<!-- BEGIN GENERATED: timeline -->\n{r_timeline(cfg, nodes, today)}\n<!-- END GENERATED: timeline -->",
        "",
    ])


def render_outputs(cfg, nodes, md_text, today=None):
    """All generated output as {absolute path: text}; no disk I/O. The main page is cfg.page_path."""
    out = {cfg.page_path: apply_regions(md_text, render_regions(cfg, nodes, today))}
    out[cfg.timeline_path] = render_timeline_file(cfg, nodes, today)
    out.update({cfg.track_file(c): render_track_file(cfg, nodes, c, today) for c in cfg.tracks})
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


def stale_track_files(cfg):
    """Files in the tracks directory that belong to no configured track."""
    d = cfg.tracks_dir
    return sorted(p for p in d.glob("*.md") if p.stem not in cfg.tracks) if d.is_dir() else []


def load_nodes(path):
    """The node list: a bare list, or a mapping whose only key is `nodes`."""
    with open(path, encoding="utf-8") as f:
        data = yaml.safe_load(f)
    if isinstance(data, dict):
        extra = set(data) - {"nodes"}
        if extra:
            raise ValueError(f"{path}: unknown top-level key(s) {sorted(extra)}")
        data = data.get("nodes")
    if not isinstance(data, list):
        raise ValueError(f"{path}: nodes must be a list (bare, or under a top-level 'nodes' key)")
    return data


def _date_arg(s):
    try:
        datetime.date.fromisoformat(s)
    except ValueError:
        raise argparse.ArgumentTypeError(f"{s!r} is not a YYYY-MM-DD date") from None
    return s


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__)
    tree_config.add_config_arg(ap)
    ap.add_argument("--today", type=_date_arg, help="YYYY-MM-DD used as the end of open bars (default: today)")
    a = ap.parse_args(argv)
    tree_config.utf8_stdio()
    cfg = tree_config.load_for_cli(a.config, "build")
    if cfg is None:
        return 2
    import check  # lazy: check imports this module
    try:
        nodes = load_nodes(cfg.nodes_path)
    except (OSError, ValueError, yaml.YAMLError) as e:
        print(f"build: {e}", file=sys.stderr)
        return 1
    errors, _ = check.validate_nodes(cfg, nodes)
    if errors:
        print("build: the nodes file is invalid, run check.py:", *errors, sep="\n  ", file=sys.stderr)
        return 1
    try:
        if cfg.page_path.exists():
            with open(cfg.page_path, encoding="utf-8", newline="") as f:
                old = f.read()
        else:
            old = skeleton(cfg)
        outputs = render_outputs(cfg, nodes, old, a.today)
    except (OSError, ValueError) as e:
        print(f"build: {cfg.page_path}: {e}", file=sys.stderr)
        return 1
    changed = []
    for path, text in outputs.items():
        try:
            current = path.read_bytes().decode("utf-8") if path.exists() else None
            if current != text:
                path.parent.mkdir(parents=True, exist_ok=True)
                with open(path, "w", encoding="utf-8", newline="") as f:
                    f.write(text)
                changed.append(path)
        except OSError as e:
            print(f"build: {path}: {e}", file=sys.stderr)
            return 1
    for path in stale_track_files(cfg):  # tracks that no longer exist in the config
        path.unlink()
        changed.append(path)
    print(f"build: {cfg.page_path} {'updated (%d file(s) written)' % len(changed) if changed else 'already up to date'} "
          f"({len(nodes)} nodes)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
