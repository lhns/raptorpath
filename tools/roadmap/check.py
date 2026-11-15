#!/usr/bin/env python3
"""Validate docs/roadmap/nodes.yaml and that docs/roadmap.md is up to date. Exit 1 on errors."""
import argparse
import datetime
import re
import sys
from pathlib import Path

import yaml

sys.path.insert(0, str(Path(__file__).resolve().parent))
import build  # noqa: E402

FIELDS = ["id", "title", "track", "kind", "state", "outcome", "evidence", "verdict_quote", "headline",
          "depends_on", "replaced_by", "lane", "start", "end", "folded", "overview"]
LIST_FIELDS = ["evidence", "depends_on", "replaced_by", "folded"]
OPTIONAL = ["qualified_by", "notes", "vm_order", "order_reason", "plain", "waiting_on", "diagram"]  # absent == [] / "" / None
ID_RE = re.compile(r"^[a-z][a-z0-9]*(-[a-z0-9]+)*$")
# GitHub fails to render some Mermaid blocks ("Unable to render rich display") when one page holds too many;
# which blocks fail varies per page load (2026-11-13: 14 blocks in one 231 KB file failed from block 6 to 10 on;
# 3 and 8 per file rendered). The cause is not known; the cap is empirical and set to the most the shipped layout
# was verified with (3 blocks on the main page; it has 1 now, every other page has 1).
MAX_MERMAID_PER_FILE = 3
# the build date, written as a visible sentence on the main page and on the timeline page
TODAY_RE = re.compile(re.escape(build.GENERATED_ON).replace(re.escape("{}"), r"(\d{4}-\d{2}-\d{2})"))


def _date(v):
    """Return a date for a valid start/end value ('' -> None), or raise ValueError."""
    if v in ("", None):
        return None
    if isinstance(v, datetime.datetime):
        raise ValueError("must be a date, not a datetime")
    if isinstance(v, datetime.date):
        return v
    return datetime.date.fromisoformat(str(v))


def find_cycle(nodes):
    """Return one cycle in depends_on + replaced_by as a list of ids (first == last), or None.

    Edges run prerequisite -> dependant, and old node -> its replacement (the old one comes first).
    """
    pre = build.precedence(nodes)
    color = {}

    def visit(u, path):
        color[u] = 1
        for v in pre[u]:
            if color.get(v) == 1:
                return path[path.index(v):] + [v]
            if v not in color:
                found = visit(v, path + [v])
                if found:
                    return found
        color[u] = 2
        return None

    for u in pre:
        if u not in color:
            found = visit(u, [u])
            if found:
                return list(reversed(found))
    return None


def validate_nodes(nodes):
    errors, warnings = [], []
    if not isinstance(nodes, list):
        return ["top level must be a list of nodes"], warnings
    good = []  # nodes whose structure is sound enough for the graph checks
    seen = set()
    for idx, n in enumerate(nodes):
        if not isinstance(n, dict):
            errors.append(f"node #{idx}: not a mapping")
            continue
        nid = n.get("id", f"#{idx}")
        e0 = len(errors)

        def err(msg, nid=nid):
            errors.append(f"{nid}: {msg}")

        for f in FIELDS:
            if f not in n:
                err(f"missing field '{f}'")
        for f in n:
            if f not in FIELDS and f not in OPTIONAL:
                err(f"unknown field '{f}'")
        if len(errors) > e0:
            continue
        if not isinstance(nid, str) or not ID_RE.match(nid):
            err("id must be kebab-case (lowercase letters, digits, '-')")
        elif nid in seen:
            err("duplicate id")
        seen.add(nid)
        for f, allowed in [("track", build.TRACKS), ("kind", build.KINDS), ("state", build.STATES),
                           ("outcome", build.OUTCOMES), ("lane", build.LANES)]:
            if n[f] not in allowed:
                err(f"{f} '{n[f]}' not one of {list(allowed)}")
        if isinstance(nid, str) and n["track"] in build.TRACKS and not nid.startswith(n["track"] + "-"):
            err(f"id must start with its track code '{n['track']}-'")
        for f in ("title", "verdict_quote", "headline"):
            if not isinstance(n[f], str):
                err(f"{f} must be a string")
        if isinstance(n["title"], str):
            if not n["title"].strip():
                err("title is empty")
            elif len(n["title"].split()) > 6:
                warnings.append(f"{nid}: title longer than 6 words")
        if isinstance(n["verdict_quote"], str) and len(n["verdict_quote"]) > 300:
            err("verdict_quote longer than 300 chars")
        for f in LIST_FIELDS:
            if not isinstance(n[f], list) or not all(isinstance(x, str) for x in n[f]):
                err(f"{f} must be a list of strings")
        q = n.get("qualified_by", [])
        if not isinstance(q, list) or not all(isinstance(x, str) for x in q):
            err("qualified_by must be a list of strings")
        if not isinstance(n.get("notes", ""), str):
            err("notes must be a string")
        if n["outcome"] == "n/a" and n["kind"] != "user-decision":
            err("outcome n/a is only for kind user-decision")
        if n["kind"] == "defect-verdict" and n["outcome"] != "unrated":
            err(f"a defect-verdict never carries an outcome (got {n['outcome']}); use unrated")
        for f in ("order_reason", "plain", "waiting_on"):
            if not isinstance(n.get(f, ""), str):
                err(f"{f} must be a string")
        vo = n.get("vm_order")
        if vo is not None and (not isinstance(vo, int) or isinstance(vo, bool)):
            err("vm_order must be an integer")
        if not isinstance(n.get("diagram", True), bool):
            err("diagram must be true or false")
        if not isinstance(n["overview"], bool):
            err("overview must be true or false")
        dates = []
        for f in ("start", "end"):
            try:
                dates.append(_date(n[f]))
            except ValueError as ex:
                err(f"{f}: {ex}")
        if len(dates) == 2:
            if dates[0] and dates[1] and dates[1] < dates[0]:
                err("end is before start")
            if dates[1] and not dates[0]:
                err("end set without start")
        if len(errors) == e0:
            good.append(n)

    ids = {n["id"] for n in good}
    by = {n["id"]: n for n in good}
    orders = {}
    # drop nodes with a dangling edge from the graph, but report each
    for n in good:
        nid = n["id"]
        if n.get("vm_order") is not None:
            if n["vm_order"] in orders:
                errors.append(f"{nid}: vm_order {n['vm_order']} already used by {orders[n['vm_order']]}")
            orders.setdefault(n["vm_order"], nid)
            if n["lane"] != "vm" or n["state"] not in build.FORWARD_STATES:
                warnings.append(f"{nid}: vm_order on a node that is not a forward node in the vm lane")
        if (n["overview"] or n["state"] in build.FORWARD_STATES) and not n.get("plain"):
            warnings.append(f"{nid}: overview/forward node without a 'plain' sentence")
        if n["state"] == "next" and n["kind"] not in ("user-decision", "defect-verdict"):
            for d in n["depends_on"]:
                # A known defect does not block work on it (a diagnosis can start).
                if d in by and by[d]["state"] != "done" and by[d]["kind"] != "defect-verdict":
                    errors.append(f"{nid}: state next but depends_on '{d}' is {by[d]['state']}, not done")
        if n.get("waiting_on") and n["state"] != "in-progress":
            warnings.append(f"{nid}: waiting_on on a node that is not in-progress")
        if n.get("qualified_by") and n["state"] != "done":
            warnings.append(f"{nid}: qualified_by on a node that is not done")
        for f in ("depends_on", "replaced_by", "qualified_by"):
            for t in n.get(f, []):
                if t == nid:
                    errors.append(f"{nid}: {f} points at itself")
                elif t not in ids:
                    errors.append(f"{nid}: dangling {f} target '{t}'")
        state, outcome = n["state"], n["outcome"]
        if state == "done":
            if not n["evidence"]:
                errors.append(f"{nid}: state done needs at least one evidence entry")
            if outcome == "unrated" and n["kind"] not in ("user-decision", "defect-verdict"):
                # A quote may stay on an unrated node: the source sentence is kept as
                # evidence even when no later node builds on it (rating rule).
                warnings.append(f"{nid}: done but unrated" + (" (quote kept as evidence)" if n["verdict_quote"] else ""))
        if outcome == "superseded" and not n["replaced_by"]:
            errors.append(f"{nid}: outcome superseded needs a non-empty replaced_by")
        if state == "abandoned" and outcome not in ("dead-end", "superseded"):
            errors.append(f"{nid}: state abandoned needs outcome dead-end or superseded (got {outcome})")

    cyc = find_cycle(good)
    if cyc:
        errors.append("cycle in depends_on/replaced_by: " + " -> ".join(cyc))
    # the diagrams run in time order: a prerequisite that starts after its dependant is drawn as a forward arrow
    pre = build.precedence(good)
    for n in good:
        for d in pre[n["id"]]:
            if by[d]["start"] and n["start"] and str(by[d]["start"]) > str(n["start"]):
                warnings.append(f"{n['id']}: prerequisite '{d}' starts after it ({by[d]['start']} > {n['start']})")

    size = sum(1 for n in good if n["overview"])
    if not 20 <= size <= 40:
        warnings.append(f"overview has {size} nodes (target 20-40)")
    return errors, warnings


def parse_date(text):
    m = TODAY_RE.search(text)
    return m.group(1) if m else None


def check_md(nodes, md_text):
    """Compare the generated regions in md_text with a fresh in-memory rebuild."""
    regions = build.render_regions(nodes, parse_date(md_text))
    try:
        build.apply_regions(md_text, regions)
    except ValueError as e:
        return [f"roadmap.md: {e}"], []
    errors = []
    for name, body in regions.items():
        if build.apply_regions(md_text, {name: body}) != md_text:
            errors.append(f"roadmap.md: generated region '{name}' is stale (run tools/roadmap/build.py)")
    return errors, []


def check_timeline(nodes, md_path):
    """timeline.md must equal a fresh rebuild (dated like the file); open bars should end today."""
    p = Path(md_path).parent / build.TIMELINE_KEY
    try:
        text = p.read_bytes().decode("utf-8")
    except OSError:
        return [f"{p.name}: timeline file missing (run tools/roadmap/build.py)"], [], {}
    day = parse_date(text)
    errors, warnings = [], []
    if text != build.render_timeline_file(nodes, day):
        errors.append(f"{p.name}: generated timeline file is stale (run tools/roadmap/build.py)")
    if day and day != str(datetime.date.today()) and any(n["start"] and not n["end"] for n in build.normalize(nodes)):
        warnings.append(f"{p.name}: open timeline bars end at {day}, not today")
    return errors, warnings, {p: text}


def check_tracks(nodes, md_path):
    """Per-track files must equal a fresh rebuild; no file for a code outside TRACKS; Mermaid cap per file."""
    errors = []
    md_path = Path(md_path)
    texts = {}
    for code in build.TRACKS:
        p = md_path.parent / "roadmap" / build.track_path(code)
        try:
            texts[p] = p.read_bytes().decode("utf-8")
        except OSError:
            errors.append(f"{p.name}: track file missing (run tools/roadmap/build.py)")
            continue
        if texts[p] != build.render_track_file(nodes, code):
            errors.append(f"tracks/{p.name}: generated track file is stale (run tools/roadmap/build.py)")
    errors += [f"tracks/{p.name}: no such track in TRACKS (run tools/roadmap/build.py to delete it)"
               for p in build.stale_track_files(md_path)]
    return errors, texts


def mermaid_errors(texts):
    """texts: {path: text} of generated Markdown files."""
    return [f"{Path(p).name}: {t.count('```mermaid')} Mermaid blocks, max {MAX_MERMAID_PER_FILE} per file"
            for p, t in texts.items() if t.count("```mermaid") > MAX_MERMAID_PER_FILE]


def run(nodes_path, md_path):
    try:
        nodes = build.load_nodes(nodes_path)
    except (OSError, ValueError, yaml.YAMLError) as e:
        return [f"nodes.yaml: {e}"], []
    errors, warnings = validate_nodes(nodes)
    if errors:
        return errors, warnings
    try:
        text = Path(md_path).read_text(encoding="utf-8")
    except OSError as e:
        return [f"roadmap.md: {e}"], warnings
    e2, w2 = check_md(nodes, text)
    e3, texts = check_tracks(nodes, md_path)
    e4, w4, tl = check_timeline(nodes, md_path)
    texts.update(tl)
    texts[Path(md_path)] = text
    return e2 + e3 + e4 + mermaid_errors(texts), warnings + w2 + w4


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--nodes", type=Path, default=build.NODES)
    ap.add_argument("--md", type=Path, default=build.MD)
    a = ap.parse_args(argv)
    errors, warnings = run(a.nodes, a.md)
    for w in warnings:
        print(f"WARNING: {w}")
    for e in errors:
        print(f"ERROR: {e}", file=sys.stderr)
    if errors:
        print(f"check: FAILED ({len(errors)} error(s), {len(warnings)} warning(s))", file=sys.stderr)
        return 1
    print(f"check: OK ({len(warnings)} warning(s))")
    return 0


if __name__ == "__main__":
    sys.exit(main())
