#!/usr/bin/env python3
"""Validate the nodes file against tech-tree.yaml and that every generated page is up to date.

Exit 0: no errors (warnings are printed). Exit 1: errors. Exit 2: bad config.
"""
import argparse
import collections
import datetime
import re
import sys
from pathlib import Path

import yaml

sys.path.insert(0, str(Path(__file__).resolve().parent))
import build  # noqa: E402
import tree_config  # noqa: E402

FIELDS = ["id", "title", "track", "kind", "state", "outcome", "evidence", "verdict_quote", "headline",
          "depends_on", "replaced_by", "lane", "start", "end", "folded", "overview"]
LIST_FIELDS = ["evidence", "depends_on", "replaced_by", "folded"]
# absent == [] / "" / None / true
OPTIONAL = ["qualified_by", "notes", "queue_order", "order_reason", "plain", "waiting_on", "diagram", "confirmed"]
# Display coverage: every recorded field must be visible on a generated page (see coverage_errors).
TEXT_FIELDS = ["title", "verdict_quote", "headline", "plain", "notes", "waiting_on", "order_reason"]  # on some page
TEXT_LIST_FIELDS = ["evidence", "folded"]  # each item on some page
ID_LIST_FIELDS = ["depends_on", "replaced_by", "qualified_by"]  # each target id in the node's own table row
SCALAR_FIELDS = ["kind", "lane", "state", "outcome", "start", "end", "confirmed", "queue_order"]  # a cell of the node's rows
IMPLICIT_FIELDS = {"id": "the row's first cell", "track": "the page the row is on"}
COVERAGE_EXEMPT = {"overview": "a boolean shown by whether the node is drawn on the main overview",
                   "diagram": "a boolean shown by whether the node is drawn in the trees"}
# title readability: a title token that looks like an identifier (see title_identifier)
CODE_EXTENSIONS = (".py", ".rs", ".js", ".ts", ".go", ".java", ".rb")
SNAKE_RE = re.compile(r"[a-z0-9]+_[a-z0-9_]+")
ENV_RE = re.compile(r"[A-Z][A-Z0-9]*_[A-Z0-9_]+")
KEBAB_RE = re.compile(r"[a-z0-9]+(-[a-z0-9]+){2,}")
# plain compounds ("end-to-end", "up-to-date", "copy-on-write") join words with a connective; branch names do not
CONNECTIVES = {"to", "of", "on", "in", "the", "and", "or", "by", "for", "at"}
ID_RE = re.compile(r"^[a-z][a-z0-9]*(-[a-z0-9]+)*$")
# GitHub fails to render some Mermaid blocks ("Unable to render rich display") when one page holds too many;
# the cause is not known, the cap is empirical: 3 blocks per file rendered, more failed from some block on.
# The main page has 2 (the legend and the overview); every other page has 1.
MAX_MERMAID_PER_FILE = 3
# the build date, written as a visible sentence on the main page and on the timeline page
TODAY_RE = re.compile(re.escape(build.GENERATED_ON).replace(re.escape("{}"), r"(\d{4}-\d{2}-\d{2})"))


def _date(v):
    """Return a date for a valid date value ('' -> None), or raise ValueError."""
    if v in ("", None):
        return None
    if isinstance(v, datetime.datetime):
        raise ValueError("must be a date, not a datetime")
    if isinstance(v, datetime.date):
        return v
    return datetime.date.fromisoformat(str(v))


def _count(n, noun):
    """'1 done node is' / '3 done nodes are'."""
    return f"{n} {noun} is" if n == 1 else f"{n} {noun}s are"


def title_identifier(title):
    """The first token of a title that looks like an identifier (snake_case, ENV_NAME, a path or code file, a
    branch-style kebab name), else None. Words, product names and short hyphenated words do not match."""
    for raw in title.split():
        t = raw.strip(".,;:!?()[]\"'`")
        if (SNAKE_RE.fullmatch(t) or ENV_RE.fullmatch(t) or t.count("/") >= 2 or (t.islower() and t.endswith(CODE_EXTENSIONS))
                or (KEBAB_RE.fullmatch(t) and not CONNECTIVES & set(t.split("-")))):
            return t
    return None


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


def validate_nodes(cfg, nodes):
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
        for f, allowed in [("track", cfg.tracks), ("kind", cfg.kinds), ("state", build.STATES),
                           ("outcome", build.OUTCOMES), ("lane", cfg.lanes)]:
            if n[f] not in allowed:
                err(f"{f} '{n[f]}' not one of {list(allowed)}")
        if isinstance(nid, str) and n["track"] in cfg.tracks and not nid.startswith(n["track"] + "-"):
            err(f"id must start with its track code '{n['track']}-'")
        for f in ("title", "verdict_quote", "headline"):
            if not isinstance(n[f], str):
                err(f"{f} must be a string")
        if isinstance(n["title"], str):
            if not n["title"].strip():
                err("title is empty")
            elif len(n["title"].split()) > 6:
                warnings.append(f"{nid}: title longer than 6 words")
            ident = title_identifier(n["title"])
            if ident:
                warnings.append(f"{nid}: title looks like an identifier ('{ident}'); "
                                "use plain words, and explain real names in 'plain'")
        if isinstance(n["verdict_quote"], str) and len(n["verdict_quote"]) > 300:
            err("verdict_quote longer than 300 chars")
        for f in LIST_FIELDS:
            if not isinstance(n[f], list) or not all(isinstance(x, str) for x in n[f]):
                err(f"{f} must be a list of strings")
        q = n.get("qualified_by", [])
        if not isinstance(q, list) or not all(isinstance(x, str) for x in q):
            err("qualified_by must be a list of strings")
        for f in ("notes", "order_reason", "plain", "waiting_on"):
            if not isinstance(n.get(f, ""), str):
                err(f"{f} must be a string")
        kind = cfg.kinds.get(n["kind"])
        if kind:
            if n["outcome"] == "n/a" and not kind["owner_gate"]:
                err("outcome n/a is only for owner-gate kinds")
            if not kind["rated"] and n["outcome"] != "unrated":
                err(f"kind '{n['kind']}' is not rated (rated: false) and never carries an outcome (got {n['outcome']}); use unrated")
        if (n["outcome"] in build.OUTCOMES and n["outcome"] not in build.NOT_RATED
                and isinstance(n["verdict_quote"], str) and not n["verdict_quote"].strip()):
            err(f"outcome {n['outcome']} needs a verdict_quote (the sentence the rating is read from)")
        qo = n.get("queue_order")
        if qo is not None and (not isinstance(qo, int) or isinstance(qo, bool)):
            err("queue_order must be an integer")
        elif qo is not None and not cfg.queue_lane:
            err("queue_order set, but the config has no queue_lane")
        if not isinstance(n.get("diagram", True), bool):
            err("diagram must be true or false")
        if not isinstance(n["overview"], bool):
            err("overview must be true or false")
        try:
            _date(n.get("confirmed"))
        except ValueError as ex:
            err(f"confirmed: {ex}")
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
    done_unrated = []
    for n in good:
        nid = n["id"]
        k = cfg.kinds[n["kind"]]
        if n.get("queue_order") is not None and cfg.queue_lane:
            if n["queue_order"] in orders:
                errors.append(f"{nid}: queue_order {n['queue_order']} already used by {orders[n['queue_order']]}")
            orders.setdefault(n["queue_order"], nid)
            if n["lane"] != cfg.queue_lane or n["state"] not in build.FORWARD_STATES:
                warnings.append(f"{nid}: queue_order on a node that is not a forward node in the queue lane")
        if (n["overview"] or n["state"] in build.FORWARD_STATES) and not n.get("plain"):
            warnings.append(f"{nid}: overview/forward node without a 'plain' sentence")
        if n["state"] == "next" and not k["owner_gate"] and k["rated"]:
            for d in n["depends_on"]:
                # an open prerequisite of a kind with `blocks: false` does not block (a diagnosis can start on it)
                if d in by and by[d]["state"] != "done" and cfg.kinds[by[d]["kind"]]["blocks"]:
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
            if outcome == "unrated" and k["rated"] and not k["owner_gate"]:
                done_unrated.append(nid)  # a quote may stay on an unrated node as evidence
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

    if done_unrated:
        warnings.append(f"{_count(len(done_unrated), 'done node')} unrated (no quoted verdict yet): " + ", ".join(done_unrated))
    finished = collections.defaultdict(int)  # start month -> done or abandoned nodes
    drawn = set()  # start months holding an overview node
    for n in good:
        ym = str(n["start"])[:7]
        if ym:
            finished[ym] += n["state"] in ("done", "abandoned")
            if n["overview"]:
                drawn.add(ym)
    now = datetime.date.today().strftime("%Y-%m")
    bare = [ym for ym in sorted(finished) if finished[ym] and ym not in drawn and ym != now]
    if bare:
        warnings.append("overview: months with finished work but no overview node (pick their turning points): "
                        + ", ".join(f"{build.month_name(ym)} ({finished[ym]})" for ym in bare))
    unconfirmed = [n for n in good if build.is_rated(n) and not n.get("confirmed")]
    if unconfirmed:
        warnings.append(f"{_count(len(unconfirmed), 'rated node')} not confirmed by the owner")
    if cfg.overview:
        size = sum(1 for n in good if n["overview"])
        if not cfg.overview["min"] <= size <= cfg.overview["max"]:
            warnings.append(f"overview has {size} nodes (target {cfg.overview['min']}-{cfg.overview['max']})")
    return errors, warnings


def parse_date(text):
    m = TODAY_RE.search(text)
    return m.group(1) if m else None


def check_md(cfg, nodes, md_text):
    """Compare the generated regions in md_text with a fresh in-memory rebuild."""
    regions = build.render_regions(cfg, nodes, parse_date(md_text))
    page = cfg.page_path.name
    try:
        build.apply_regions(md_text, regions)
    except ValueError as e:
        return [f"{page}: {e}"]
    return [f"{page}: generated region '{name}' is stale (run build.py)"
            for name, body in regions.items() if build.apply_regions(md_text, {name: body}) != md_text]


def check_timeline(cfg, nodes):
    """The timeline page must equal a fresh rebuild (dated like the file); open bars should end today."""
    p = cfg.timeline_path
    try:
        text = p.read_bytes().decode("utf-8")
    except OSError:
        return [f"{p.name}: timeline file missing (run build.py)"], [], {}
    day = parse_date(text)
    errors, warnings = [], []
    if text != build.render_timeline_file(cfg, nodes, day):
        errors.append(f"{p.name}: generated timeline file is stale (run build.py)")
    if day and day != str(datetime.date.today()) and any(n["start"] and not n["end"] for n in build.normalize(nodes)):
        warnings.append(f"{p.name}: open timeline bars end at {day}, not today")
    return errors, warnings, {p: text}


def check_tracks(cfg, nodes):
    """Per-track files must equal a fresh rebuild; no file for a track outside the config."""
    errors, texts = [], {}
    for code in cfg.tracks:
        p = cfg.track_file(code)
        try:
            texts[p] = p.read_bytes().decode("utf-8")
        except OSError:
            errors.append(f"{p.name}: track file missing (run build.py)")
            continue
        if texts[p] != build.render_track_file(cfg, nodes, code):
            errors.append(f"tracks/{p.name}: generated track file is stale (run build.py)")
    errors += [f"tracks/{p.name}: no such track in the config (run build.py to delete it)" for p in build.stale_track_files(cfg)]
    return errors, texts


def mermaid_errors(texts):
    """texts: {path: text} of generated Markdown files."""
    return [f"{Path(p).name}: {t.count('```mermaid')} Mermaid blocks, max {MAX_MERMAID_PER_FILE} per file"
            for p, t in texts.items() if t.count("```mermaid") > MAX_MERMAID_PER_FILE]


def _row_cells(page_text, nid):
    """The cells of every table row of a node (its main row and its details row) on its track page."""
    cells = []
    for ln in page_text.splitlines():
        if ln.startswith(f"| `{nid}` |"):
            cells += [c.strip() for c in ln.strip().strip("|").split(" | ")]
    return cells


def coverage_errors(cfg, nodes, today=None):
    """Every recorded field of every node must be visible on a generated page; a field no renderer shows is
    reported here. Text: on some page, with the tables' cell normalization. Id lists: each target id in the
    node's own row. Scalars: the value is a cell of the node's rows. Only COVERAGE_EXEMPT fields are skipped."""
    regions = build.render_regions(cfg, nodes, today)
    tracks = {c: build.render_track_file(cfg, nodes, c, today) for c in cfg.tracks}
    blob = "\n".join(list(regions.values()) + [build.render_timeline_file(cfg, nodes, today)] + list(tracks.values()))
    out = []
    for n in build.normalize(nodes):
        cells = _row_cells(tracks[n["track"]], n["id"])
        for f in TEXT_FIELDS:
            if n[f] and build.cell(n[f]) not in blob:
                out.append(f"{n['id']}: {f} appears on no generated page")
        for f in TEXT_LIST_FIELDS:
            for i, item in enumerate(n[f]):
                if build.cell(item) not in blob:
                    out.append(f"{n['id']}: {f} item {i + 1} appears on no generated page")
        for f in ID_LIST_FIELDS:
            ids = {x.strip() for c in cells for x in c.split(",")}
            out += [f"{n['id']}: {f} target {t} is not in the node's table row" for t in n[f] if t not in ids]
        for f in SCALAR_FIELDS:
            if n[f] not in ("", None) and build.cell(n[f]) not in cells:
                out.append(f"{n['id']}: {f} value {n[f]} is not in the node's table row")
    return out


def run(cfg):
    try:
        nodes = build.load_nodes(cfg.nodes_path)
    except (OSError, ValueError, yaml.YAMLError) as e:
        return [f"nodes file: {e}"], []
    errors, warnings = validate_nodes(cfg, nodes)
    if errors:
        return errors, warnings
    try:
        text = cfg.page_path.read_text(encoding="utf-8")
    except OSError as e:
        return [f"{cfg.page_path.name}: {e}"], warnings
    e2 = check_md(cfg, nodes, text)
    e3, texts = check_tracks(cfg, nodes)
    e4, w4, tl = check_timeline(cfg, nodes)
    texts.update(tl)
    texts[cfg.page_path] = text
    e5 = coverage_errors(cfg, nodes, parse_date(text))  # independent of the files on disk
    return e2 + e3 + e4 + mermaid_errors(texts) + e5, warnings + w4


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__)
    tree_config.add_config_arg(ap)
    a = ap.parse_args(argv)
    tree_config.utf8_stdio()
    cfg = tree_config.load_for_cli(a.config, "check")
    if cfg is None:
        return 2
    errors, warnings = run(cfg)
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
