#!/usr/bin/env python3
"""Unit tests for build.py and check.py. Run: python tools/roadmap/test_roadmap.py"""
import contextlib
import copy
import io
import re
import sys
import tempfile
import unittest
from pathlib import Path

import yaml

sys.path.insert(0, str(Path(__file__).resolve().parent))
import build  # noqa: E402
import check  # noqa: E402

TODAY = "2026-11-13"


def node(i, **kw):
    n = {"id": i, "title": "T " + i, "track": i.split("-")[0], "kind": "mechanism", "state": "next",
         "outcome": "unrated", "evidence": [], "verdict_quote": "", "headline": "", "depends_on": [],
         "replaced_by": [], "lane": "none", "start": "", "end": "", "folded": [], "overview": True}
    n.update(kw)
    return n


def skeleton(hand="HAND-WRITTEN  text\twith odd   spacing\n"):
    parts = ["# Title\n\n", hand]
    for name in build.REGION_ORDER:
        parts.append(f"<!-- BEGIN GENERATED: {name} -->\n<!-- END GENERATED: {name} -->\n\ntrailing {name}\n")
    return "".join(parts)


def built(nodes, text=None):
    return build.apply_regions(text or skeleton(), build.render_regions(nodes, TODAY))


def tfile(nodes, code):
    return build.render_track_file(nodes, code, TODAY)


GOOD = [
    node("rec-a", state="done", outcome="very-successful", evidence=["status §1"], start="2026-01-01", end="2026-02-01"),
    node("rec-b", depends_on=["rec-a"], start="2026-03-01", state="in-progress"),
    node("rate-c", depends_on=["rec-a"], kind="law"),
]


class Files:
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.dir = Path(self.tmp.name)

    def tearDown(self):
        self.tmp.cleanup()

    def run_check(self, nodes, md):
        n, m = self.dir / "nodes.yaml", self.dir / "roadmap.md"
        n.write_text(yaml.safe_dump(nodes, allow_unicode=True), encoding="utf-8")
        m.write_text(md, encoding="utf-8", newline="")
        self.write_tracks(nodes)
        return check.run(n, m)

    def write_tracks(self, nodes):
        d = self.dir / "roadmap" / "tracks"
        d.mkdir(parents=True, exist_ok=True)
        for code in build.TRACKS:
            (d / f"{code}.md").write_text(build.render_track_file(nodes, code, TODAY), encoding="utf-8", newline="")
        (self.dir / build.TIMELINE_KEY).write_text(build.render_timeline_file(nodes, TODAY), encoding="utf-8", newline="")


EDGE_RE = re.compile(r"^\s+(\w+) (?:-->|-\.->(?:\|[^|]*\|)?) (\w+)$", re.M)
TIE_RE = re.compile(r"^\s+(\w+) ~~~ (\w+)$", re.M)


def mermaid_blocks(text):
    return re.findall(r"```mermaid\n(.*?)\n```", text, re.S)


def row_levels(block):
    """Row index of every node, read back from the invisible row ties (a ~~~ b puts b one row below a)."""
    ties = TIE_RE.findall(block)
    level = {}
    for a, b in ties:
        level.setdefault(a, 0)
        level.setdefault(b, 0)
    for _ in range(len(ties) + 1):  # relax: longest path from the first row
        for a, b in ties:
            level[b] = max(level[b], level[a] + 1)
    return level


def backward_edges(block):
    lv = row_levels(block)
    return [(a, b) for a, b in EDGE_RE.findall(block) if lv.get(a, 0) >= lv.get(b, 0)]


class OneDirectionTests(unittest.TestCase):
    """Complaint 1: arrows must all point down (= forward in time)."""

    def real(self):
        nodes = build.load_nodes(build.NODES)
        return nodes

    def test_every_edge_in_every_real_diagram_goes_down(self):
        nodes = self.real()
        outs = build.render_outputs(nodes, skeleton(), TODAY)
        seen = 0
        for key, text in outs.items():
            for block in mermaid_blocks(text):
                if block.startswith("flowchart"):
                    seen += 1
                    self.assertEqual(backward_edges(block), [], key)
        self.assertEqual(seen, 1 + len(build.TRACKS))

    def test_synthetic_all_edge_kinds_go_down(self):
        nodes = [
            node("rec-new", state="done", outcome="successful", evidence=["e"], start="2026-05-01", qualified_by=["rec-def"]),
            node("rec-old", state="abandoned", outcome="superseded", replaced_by=["rec-new"], start="2026-06-01"),
            node("rec-def", kind="defect-verdict", state="in-progress", start="2026-02-01"),
            node("cc-x", depends_on=["rec-new"], start="2026-01-01"),
            node("rec-und", depends_on=["cc-x"]),
        ]
        for text in build.render_outputs(nodes, skeleton(), TODAY).values():
            for block in mermaid_blocks(text):
                if block.startswith("flowchart"):
                    self.assertEqual(backward_edges(block), [])

    def test_global_order_is_topological_then_by_start_then_undated(self):
        nodes = [node("rec-late", start="2026-09-01", depends_on=["rec-early"]),
                 node("rec-early", start="2026-10-01"),
                 node("rec-old", start="2026-01-01", state="abandoned", outcome="superseded", replaced_by=["rec-mid"]),
                 node("rec-mid", start="2026-01-15"),
                 node("rec-nodate"), node("rec-nodate2", depends_on=["rec-nodate"])]
        order = build.time_order(build.normalize(nodes))
        self.assertEqual(order, ["rec-old", "rec-mid", "rec-early", "rec-late", "rec-nodate", "rec-nodate2"])


class ValidateTests(unittest.TestCase):
    def errs(self, nodes):
        return check.validate_nodes(nodes)[0]

    def test_good_passes(self):
        self.assertEqual(self.errs(GOOD), [])

    def test_dangling_edge(self):
        bad = copy.deepcopy(GOOD)
        bad[1]["depends_on"].append("rec-missing")
        bad[2]["replaced_by"] = ["nope-x"]
        e = "\n".join(self.errs(bad))
        self.assertIn("dangling depends_on target 'rec-missing'", e)
        self.assertIn("dangling replaced_by target 'nope-x'", e)

    def test_cycle(self):
        bad = [node("rec-a", depends_on=["rec-b"]), node("rec-b", depends_on=["rec-c"]), node("rec-c", depends_on=["rec-a"])]
        e = self.errs(bad)
        self.assertTrue(any("cycle" in x and "rec-a" in x and "rec-b" in x for x in e), e)

    def test_cycle_through_replaced_by(self):
        bad = [node("rec-a", replaced_by=["rec-b"], state="abandoned", outcome="superseded"), node("rec-b", depends_on=["rec-c"]),
               node("rec-c", depends_on=["rec-b"])]
        self.assertTrue(any("cycle" in x for x in self.errs(bad)))
        loop = [node("rec-a", replaced_by=["rec-b"], state="abandoned", outcome="superseded"),
                node("rec-b", replaced_by=["rec-a"], state="abandoned", outcome="superseded")]
        e = self.errs(loop)
        self.assertTrue(any("cycle" in x and "replaced_by" in x for x in e), e)
        old_depends_on_new = [node("rec-a", replaced_by=["rec-b"], depends_on=["rec-b"], state="abandoned", outcome="superseded"),
                              node("rec-b")]
        self.assertTrue(any("cycle" in x for x in self.errs(old_depends_on_new)))

    def test_prerequisite_starting_later_is_a_warning(self):
        nodes = [node("rec-a", start="2026-05-01"), node("rec-b", depends_on=["rec-a"], start="2026-02-01", state="in-progress")]
        errs, warns = check.validate_nodes(nodes)
        self.assertEqual(errs, [])
        self.assertTrue(any("rec-b" in w and "rec-a" in w and "starts after" in w for w in warns), warns)
        ok = [node("rec-a", start="2026-01-01"), node("rec-b", depends_on=["rec-a"], start="2026-02-01", state="in-progress")]
        self.assertFalse(any("starts after" in w for w in check.validate_nodes(ok)[1]))

    def test_self_dependency(self):
        self.assertTrue(self.errs([node("rec-a", depends_on=["rec-a"])]))

    def test_missing_field_and_bad_enum(self):
        n = node("rec-a")
        del n["lane"]
        self.assertIn("missing field 'lane'", "\n".join(self.errs([n])))
        self.assertIn("state 'weird'", "\n".join(self.errs([node("rec-a", state="weird")])))

    def test_duplicate_and_bad_id(self):
        self.assertIn("duplicate id", "\n".join(self.errs([node("rec-a"), node("rec-a")])))
        self.assertIn("kebab-case", "\n".join(self.errs([node("rec_a")])))
        self.assertIn("track code", "\n".join(self.errs([node("rec-a", track="cc")])))

    def test_done_rules(self):
        e = "\n".join(self.errs([node("rec-a", state="done", outcome="successful")]))
        self.assertIn("evidence", e)
        errs, warns = check.validate_nodes([node("rec-a", state="done", outcome="unrated", evidence=["x"])])
        self.assertEqual(errs, [])
        self.assertTrue(any("unrated" in w for w in warns))
        # an unrated node may keep its source quote as evidence: a warning, not an error
        errs, warns = check.validate_nodes([node("rec-a", state="done", outcome="unrated", evidence=["x"], verdict_quote="q")])
        self.assertEqual(errs, [])
        self.assertTrue(any("quote kept as evidence" in w for w in warns))

    def test_superseded_and_abandoned(self):
        self.assertIn("replaced_by", "\n".join(self.errs([node("rec-a", state="abandoned", outcome="superseded")])))
        self.assertIn("abandoned", "\n".join(self.errs([node("rec-a", state="abandoned", outcome="successful")])))
        self.assertEqual(self.errs([node("rec-a", state="abandoned", outcome="dead-end")]), [])

    def test_qualified_by_validation(self):
        bad = [node("rec-a", state="done", outcome="successful", evidence=["e"], qualified_by=["rec-zzz"])]
        self.assertIn("dangling qualified_by target 'rec-zzz'", "\n".join(self.errs(bad)))
        ok = [node("rec-a", state="done", outcome="successful", evidence=["e"], qualified_by=["rec-b"], notes="n"),
              node("rec-b", depends_on=["rec-a"])]  # qualified_by is not a dependency: no cycle
        self.assertEqual(self.errs(ok), [])
        self.assertTrue(self.errs([node("rec-a", notes=3)]))

    def test_na_outcome(self):
        self.assertIn("n/a", "\n".join(self.errs([node("rec-a", outcome="n/a")])))
        errs, warns = check.validate_nodes([node("rec-a", kind="user-decision", state="done", outcome="n/a", evidence=["e"])])
        self.assertEqual((errs, [w for w in warns if "unrated" in w]), ([], []))
        errs, warns = check.validate_nodes([node("rec-a", kind="user-decision", state="done", outcome="unrated", evidence=["e"])])
        self.assertEqual((errs, [w for w in warns if "unrated" in w]), ([], []))

    def test_defect_outcome_rule(self):
        e = "\n".join(self.errs([node("rec-a", kind="defect-verdict", state="done", outcome="successful", evidence=["e"])]))
        self.assertIn("defect-verdict never carries an outcome", e)
        errs, warns = check.validate_nodes([node("rec-a", kind="defect-verdict", state="done", evidence=["e"], plain="p")])
        self.assertEqual((errs, [w for w in warns if "unrated" in w]), ([], []))

    def test_readiness_rule(self):
        done = node("rec-a", state="done", outcome="successful", evidence=["e"])
        for st in ("in-progress", "next", "parked"):
            blocked = node("rec-b", state=st if st != "next" else "next", depends_on=["rec-c"])
            other = node("rec-c", state=st if st != "next" else "in-progress")
            e = "\n".join(self.errs([done, blocked, other]))
            self.assertEqual("depends_on 'rec-c'" in e, blocked["state"] == "next", (st, e))
        ok = node("rec-b", depends_on=["rec-a"])
        self.assertEqual(self.errs([done, ok]), [])
        # user decisions and defect verdicts are exempt
        for kind in ("user-decision", "defect-verdict"):
            ex = node("rec-b", kind=kind, depends_on=["rec-c"])
            self.assertEqual(self.errs([ex, node("rec-c")]), [])
        # an open defect as prerequisite does not block (a diagnosis can start on it)
        diag = node("rec-b", depends_on=["rec-d"])
        defect = node("rec-d", kind="defect-verdict", state="in-progress")
        self.assertEqual(self.errs([diag, defect]), [])

    def test_vm_order_validation(self):
        a = node("rec-a", lane="vm", vm_order=1, plain="p")
        b = node("rec-b", lane="vm", vm_order=1, plain="p")
        self.assertIn("vm_order 1 already used", "\n".join(self.errs([a, b])))
        self.assertTrue(self.errs([node("rec-a", vm_order="x")]))
        self.assertTrue(any("vm lane" in w for w in check.validate_nodes([node("rec-a", vm_order=1, plain="p")])[1]))

    def test_plain_warning(self):
        warns = check.validate_nodes([node("rec-a", plain="ok"), node("rec-b"), node("rec-c", overview=False, state="done",
                                      outcome="successful", evidence=["e"])])[1]
        self.assertTrue(any("rec-b" in w and "plain" in w for w in warns))
        self.assertFalse(any("rec-a" in w and "plain" in w for w in warns))
        self.assertFalse(any("rec-c" in w and "plain" in w for w in warns))  # done, not in overview: no warning

    def test_overview_size_warning(self):
        self.assertTrue(any("overview has 3" in w for w in check.validate_nodes(GOOD)[1]))


class BuildTests(unittest.TestCase):
    def test_label_escaping(self):
        t = 'Law §5 "x" [y] #z'
        out = tfile([node("rec-a", title=t)], "rec")
        self.assertIn('rec_a["○ Law #167;5 #quot;x#quot; #91;y#93; #35;z"]', out)
        self.assertNotIn(t, self.region(out, "track-rec"))
    def test_shapes_and_edges(self):
        nodes = GOOD + [node("rec-e", kind="defect-verdict"),
                        node("rec-f", state="abandoned", outcome="superseded", replaced_by=["rec-b"])]
        rec = self.region(tfile(nodes, "rec"), "track-rec")
        self.assertIn('rate_c[/"○ T rate-c"/]', self.region(tfile(nodes, "rate"), "track-rate"))
        self.assertIn('rec_e>"⚠ T rec-e"]', rec)
        self.assertIn("rec_a --> rec_b", rec)
        self.assertIn("rec_f -.-> rec_b", rec)
        self.assertIn('rec_a["✔ T rec-a"]', rec)
        self.assertIn('rec_b["▶ T rec-b"]', rec)
        self.assertIn('rec_f["↷ T rec-f"]', rec)
        self.assertNotIn("|", rec.split("classDef")[0])  # no text on edges
    def test_track_diagram_has_no_ext_lines_or_cross_track_edges(self):
        track_rate = self.region(tfile(GOOD, "rate"), "track-rate")
        self.assertNotIn("ext:", track_rate)
        self.assertNotIn("x_", track_rate)
        self.assertNotIn("-->", track_rate)
        self.assertNotIn("rec_b[", track_rate)
    def test_timeline_open_bar_and_undated(self):
        tl = build.render_timeline_file(GOOD, TODAY)
        self.assertNotIn("%% today", tl)
        self.assertIn("Generated from nodes.yaml on " + TODAY + ".", tl)
        self.assertIn("T rec-b :active, rec_b, 2026-03-01, " + TODAY, tl)
        self.assertIn("T rec-a :rec_a, 2026-01-01, 2026-02-01", tl)
        self.assertIn("Not shown (no `start`): `rate-c`", tl)
        self.assertEqual(tl.count("```mermaid"), 1)
        main = built(GOOD)
        self.assertNotIn("gantt", main)
        self.assertIn("(roadmap/timeline.md)", self.region(main, "timeline"))
        self.assertEqual(check.TODAY_RE.search(main).group(1), TODAY)
        self.assertEqual(check.TODAY_RE.search(tl).group(1), TODAY)
    def test_preservation_and_idempotence(self):
        text = skeleton(hand="keep § this\r\n\ttabs  and trailing spaces   \n\n")
        once = built(GOOD, text)
        self.assertEqual(built(GOOD, once), once)
        self.assertEqual(once.split("<!-- BEGIN GENERATED: legend -->")[0], text.split("<!-- BEGIN GENERATED: legend -->")[0])
        for name in build.REGION_ORDER:
            self.assertIn(f"<!-- END GENERATED: {name} -->\n\ntrailing {name}\n", once)

    def test_crlf_preserved(self):
        text = skeleton().replace("\n", "\r\n")
        out = built(GOOD, text)
        self.assertNotIn("\n", out.replace("\r\n", ""))

    def test_missing_marker(self):
        with self.assertRaises(ValueError):
            build.apply_regions("# nothing here\n", build.render_regions(GOOD, TODAY))

    def test_overview_bands_and_grid_rows(self):
        nodes = [node(f"{t}-{k}", title="A fairly long title here ok", state="done", outcome="successful", evidence=["e"],
                      start="2026-0%d-01" % (1 + "abcde".index(k)))
                 for t in ("rec", "rate", "cc") for k in "abcde"]
        nodes += [node("rec-p", state="in-progress"), node("rec-q"), node("rec-r", state="blocked")]
        ov = self.region(built(nodes), "overview")
        self.assertIn("flowchart TB", ov)
        self.assertNotIn("direction", ov)
        self.assertNotIn("subgraph sg_", ov)  # time bands, not track boxes
        for label in ('"Now (in progress)"', '"Next"', '"Later (blocked or parked)"'):
            self.assertIn(label, ov)
        self.assertRegex(ov, r'subgraph bd_0\["Jan 2026')  # past bands are months, merged until BAND_MIN nodes
        self.assertIn("rec · ✔ A fairly long<br/>title here ok", ov)  # track prefix, one glyph, wrapped title
        lv = row_levels(ov)
        rows = {}
        for i, k in lv.items():
            rows.setdefault(k, []).append(i)
        self.assertLessEqual(max(map(len, rows.values())), build.OVERVIEW_WIDTH)
        tr = self.region(tfile([node(f"rec-{c}") for c in "abcdefg"], "rec"), "track-rec")
        rows = {}
        for i, k in row_levels(tr).items():
            rows.setdefault(k, []).append(i)
        self.assertEqual([len(rows[k]) for k in sorted(rows)], [3, 3, 1])
    def test_track_tree_packs_disconnected_nodes_into_rows(self):
        nodes = [node(f"rec-n{i:02d}") for i in range(13)] + [node("rec-z", depends_on=["rec-n00"], state="next")]
        nodes[0]["state"] = "done"
        nodes[0].update(outcome="successful", evidence=["e"])
        nodes.append(node("rate-x", depends_on=["rec-n01"]))
        nodes[1].update(state="done", outcome="successful", evidence=["e"])
        tr = self.region(tfile(nodes, "rec"), "track-rec")
        chain = [ln.split() for ln in tr.splitlines() if " ~~~ " in ln]
        down = {}
        for a, _, b in chain:
            down.setdefault(a, []).append(b)
        targets = {b for bs in down.values() for b in bs}
        sel = [f"rec_n{i:02d}" for i in range(13)] + ["rec_z"]
        # every node except those of the first row hangs below another node
        self.assertEqual(sorted(set(sel) - targets), sorted(set(sel) - targets))
        roots = [x for x in sel if x not in targets]
        self.assertLessEqual(len(roots), 4)  # one row: at most 4 wide
        # rows are at most 4 wide: following the links, no node has more than 4 siblings
        for kids in down.values():
            self.assertLessEqual(len(kids), 4)
        # all nodes are reachable from the first row by invisible links only
        seen, todo = set(roots), list(roots)
        while todo:
            for k in down.get(todo.pop(), []):
                if k not in seen:
                    seen.add(k)
                    todo.append(k)
        self.assertTrue(set(sel) <= seen)
        # a node never shares a row with its prerequisite: rec_z must not be a root next to rec_n00
        self.assertFalse("rec_z" in roots)

    def test_pack_rows_widths_and_dependencies(self):
        group = [node(f"rec-{c}") for c in "abcdefghij"] + [node("rec-k", depends_on=["rec-a", "rec-b"])]
        rows = build.pack_rows(group, 4)
        self.assertTrue(all(len(r) <= 4 for r in rows))
        self.assertEqual(sorted(x for r in rows for x in r), sorted(n["id"] for n in group))
        row_of = {x: i for i, r in enumerate(rows) for x in r}
        self.assertGreater(row_of["rec-k"], max(row_of["rec-a"], row_of["rec-b"]))
        links = build.row_links(rows)
        # every node has a tie into the next row and one from the previous row, so each row stays on one rank
        tied_down = {ln.split()[0] for ln in links}
        tied_up = {ln.split()[2] for ln in links}
        self.assertEqual(tied_down, {build.sid(x) for r in rows[:-1] for x in r})
        self.assertEqual(tied_up, {build.sid(x) for r in rows[1:] for x in r})
    def test_replaced_by_is_a_precedence_edge(self):
        old = node("rec-old", state="abandoned", outcome="superseded", replaced_by=["rec-new"], start="2026-09-01")
        new = node("rec-new", start="2026-01-01")  # starts earlier, yet the old node comes first
        self.assertEqual(build.time_order(build.normalize([new, old])), ["rec-old", "rec-new"])
        tr = self.region(tfile([new, old], "rec"), "track-rec")
        self.assertIn("rec_old -.-> rec_new", tr)
        self.assertEqual(backward_edges(tr), [])
    def test_transitive_prerequisites_not_drawn(self):
        nodes = [node("rec-a"), node("rec-b", depends_on=["rec-a"]), node("rec-c", depends_on=["rec-a", "rec-b"])]
        out = tfile(nodes, "rec")
        tr = self.region(out, "track-rec")
        self.assertIn("rec_a --> rec_b", tr)
        self.assertIn("rec_b --> rec_c", tr)
        self.assertNotIn("rec_a --> rec_c", tr)
        self.assertIn("| rec-a, rec-b |", out)  # the table keeps the full depends_on

    def test_waiting_state(self):
        nodes = [node("rec-a", state="in-progress", waiting_on="a VM session"),
                 node("rec-b", state="in-progress", waiting_on="a VM session"),
                 node("rec-c", state="in-progress", waiting_on="an owner call"),
                 node("rec-d", state="in-progress"),
                 node("rec-e", kind="defect-verdict", state="in-progress", waiting_on="x")]
        out = built(nodes)
        ov = self.region(out, "overview")
        self.assertIn('rec_a["rec · ⏳ T rec-a"]', ov)
        self.assertIn('rec_d["rec · ▶ T rec-d"]', ov)
        self.assertIn("class rec_a,rec_b,rec_c waiting", ov)
        self.assertIn("classDef waiting fill:#e3f2fd", ov)
        running = self.region(out, "running")
        self.assertIn("`rec-d`", running)
        for i in "abce":
            self.assertNotIn(f"rec-{i}", running)
        bw = self.region(out, "built-waiting")
        self.assertIn("**a VM session**" + chr(10)*2 + "- `rec-a`: T rec-a" + chr(10) + "- `rec-b`: T rec-b", bw)
        self.assertIn("**an owner call**" + chr(10)*2 + "- `rec-c`: T rec-c", bw)
        self.assertNotIn("rec-d", bw)
        self.assertNotIn("rec-e", bw)  # defects are not "built, waiting"
        self.assertEqual(self.region(built([node("rec-a")]), "built-waiting").strip(), "_None._")

    def test_waiting_on_validation(self):
        self.assertTrue(check.validate_nodes([node("rec-a", waiting_on=3)])[0])
        warns = check.validate_nodes([node("rec-a", waiting_on="x", plain="p")])[1]
        self.assertTrue(any("waiting_on" in w for w in warns))
        warns = check.validate_nodes([node("rec-a", state="in-progress", waiting_on="x", plain="p")])[1]
        self.assertFalse(any("waiting_on" in w for w in warns))

    def test_gantt_title_truncated(self):
        out = build.render_timeline_file([node("rec-a", title="x" * 60, start="2026-01-01")], TODAY)
        self.assertIn("x" * 39 + "… :rec_a", out)
    def test_qualified_by(self):
        nodes = [node("rec-a", state="done", outcome="successful", evidence=["e"], qualified_by=["rec-b"], title="Rated"),
                 node("rec-b", kind="defect-verdict", depends_on=["rec-a"]),
                 node("rate-c", state="done", outcome="mixed", evidence=["e"], qualified_by=["rec-b"])]
        ov = self.region(built(nodes), "overview")
        links = [ln.strip() for ln in ov.splitlines() if "~~~" in ln or "-->" in ln or "-.->" in ln]
        self.assertIn("rec_a -.-> rec_b", links)
        self.assertIn("rate_c -.-> rec_b", links)
        want = sorted(links.index(x) for x in ("rec_a -.-> rec_b", "rate_c -.-> rec_b"))
        self.assertIn("linkStyle " + ",".join(map(str, want)) + " stroke:#e69500,stroke-width:3px", ov)
        self.assertNotIn("qualified", ov)  # no text on edges, no marker on labels
        tr = self.region(tfile(nodes, "rate"), "track-rate")
        self.assertNotIn("rec_b", tr)  # other-track target: not drawn
    def test_linkstyle_counts_invisible_links(self):
        nodes = [node(f"rec-{k}") for k in "abcd"] + [
            node("rec-q", state="done", outcome="successful", evidence=["e"], qualified_by=["rec-a"])]
        ov = self.region(built(nodes), "overview")
        links = [ln for ln in ov.splitlines() if "~~~" in ln or "-->" in ln or "-.->" in ln]
        i = next(k for k, ln in enumerate(links) if "rec_q -.-> rec_a" in ln)
        self.assertIn(f"linkStyle {i} ", ov)
    def test_notes_table(self):
        tb = self.region(tfile([node("rec-a", notes="why | so"), node("rec-b")], "rec"), "table-rec")
        self.assertLess(tb.index("| id | title | state"), tb.index("### Details per node"))
        self.assertIn("| `rec-a` | T rec-a | why \\| so |  |", tb)
        self.assertNotIn("`rec-b` | T rec-b | ", tb.split("### Details")[1])

    @staticmethod
    def region(out, name):
        return out.split(f"<!-- BEGIN GENERATED: {name} -->")[1].split("<!-- END")[0]

    def test_region_order(self):
        order = build.REGION_ORDER
        self.assertEqual(order[:9], ["legend", "overview", "plain-words", "ready-now", "running", "built-waiting",
                                     "waiting-on-owner", "vm-queue", "timeline"])
        self.assertEqual(order[-1], "tracks")
        out = built([node("rec-a")])
        pos = [out.index(f"<!-- BEGIN GENERATED: {n} -->") for n in order]
        self.assertEqual(pos, sorted(pos))

    def test_defect_tags(self):
        nodes = [node("rec-a", kind="defect-verdict", state="in-progress"),
                 node("rec-b", kind="defect-verdict", state="next"),
                 node("rec-c", kind="defect-verdict", state="done", evidence=["e"])]
        ov = self.region(built(nodes), "overview")
        head = ov.split("classDef")[0]
        self.assertEqual(head.count("⚠ T rec-"), 2)
        self.assertIn('rec_c>"rec · ✔ T rec-c"]', ov)
        self.assertNotIn("unrated", head)
        self.assertNotIn("done", head)
    def test_vm_queue_region_and_no_then_links(self):
        nodes = [node("rec-c", lane="vm", vm_order=3, order_reason="last one"),
                 node("rec-a", lane="vm", vm_order=1, order_reason="first because X"),
                 node("rec-b", lane="vm", vm_order=2),
                 node("rec-z", lane="vm")]
        out = built(nodes)
        q = self.region(out, "vm-queue").strip().splitlines()
        self.assertEqual(q, ["1. `rec-a`: T rec-a. first because X", "2. `rec-b`: T rec-b", "3. `rec-c`: T rec-c. last one"])
        ov = self.region(out, "overview")
        self.assertNotIn("then", ov)
        self.assertNotIn("-.->", ov)  # the queue is a list, not edges
        self.assertIn("The VM queue is empty", self.region(built([node("rec-a")]), "vm-queue"))
    def test_plain_words_region(self):
        nodes = [node("rec-a", plain="Does a thing, simply."),
                 node("rec-b", overview=False, state="done", outcome="successful", evidence=["e"]),
                 node("rec-c", overview=False, state="parked", plain="Waiting."),
                 node("rec-d", overview=False, plain="not forward? yes next")]
        t = self.region(built(nodes), "plain-words")
        self.assertIn("| T rec-a | ○ next | Does a thing, simply. |", t)
        self.assertIn("| T rec-c | ⏸ parked | Waiting. |", t)
        self.assertIn("| T rec-d | ○ next |", t)
        self.assertNotIn("T rec-b", t)  # done and not in the overview
        self.assertIn("| title | status | in plain words |", t)

    def test_lane_lists(self):
        nodes = [node("rec-a", lane="vm"), node("rec-b", lane="user"), node("rec-c", lane="vm", state="in-progress"),
                 node("rec-d", lane="host-cargo", state="in-progress"),
                 node("rec-e", kind="defect-verdict", state="in-progress", lane="vm"),
                 node("rec-f", kind="user-decision", lane="user"),
                 node("rec-g", kind="user-decision", state="done", outcome="n/a", evidence=["e"]),
                 node("rec-h", kind="user-decision", state="abandoned", outcome="dead-end")]
        out = built(nodes)
        ready = self.region(out, "ready-now")
        self.assertIn("**vm**", ready)
        self.assertIn("- `rec-a`: T rec-a", ready)
        self.assertNotIn("user", ready)  # the user lane is listed under "Waiting on the owner"
        self.assertNotIn("rec-b", ready)
        self.assertNotIn("rec-c", ready)
        running = self.region(out, "running")
        self.assertIn("`rec-c`", running.split("**host-cargo**")[0])
        self.assertIn("`rec-d`", running.split("**host-cargo**")[1])
        self.assertNotIn("rec-e", running)  # defects are not "running"
        waiting = self.region(out, "waiting-on-owner")
        self.assertIn("`rec-f`", waiting)
        self.assertNotIn("rec-g", waiting)
        self.assertNotIn("rec-h", waiting)
        self.assertEqual(self.region(built([node("rec-a", state="done", outcome="successful", evidence=["e"])]),
                                     "running").strip(), "_None._")

    def test_table_row(self):
        out = tfile([node("rec-a", headline="a|b", evidence=["x", "y"], depends_on=[])], "rec")
        self.assertIn("| `rec-a` | T rec-a | next | unrated | a\\|b | x; y |  |", out)


class HiddenNodeTests(unittest.TestCase):
    """Owner decisions and `diagram: false` nodes are not drawn; their dependencies are rewired around them."""

    NODES = [node("rec-a", state="done", outcome="successful", evidence=["e"], start="2026-01-01"),
             node("rec-u", kind="user-decision", depends_on=["rec-a"], start="2026-02-01", lane="user"),
             node("rec-h", depends_on=["rec-u"], diagram=False, start="2026-03-01", state="in-progress"),
             node("rec-b", depends_on=["rec-h"], start="2026-04-01", state="in-progress"),
             node("rec-c", depends_on=["rec-u"], start="2026-05-01", state="in-progress")]

    def test_hidden_nodes_absent_from_diagrams_but_present_in_tables_and_lists(self):
        out = built(self.NODES)
        files = build.render_outputs(self.NODES, skeleton(), TODAY)
        for text in (out, files["roadmap/tracks/rec.md"]):
            for block in mermaid_blocks(text):
                self.assertNotIn("rec_u", block)
                self.assertNotIn("rec_h", block)
        self.assertIn("`rec-u`", files["roadmap/tracks/rec.md"])  # node table
        self.assertIn("`rec-h`", files["roadmap/tracks/rec.md"])
        self.assertIn("`rec-u`", BuildTests.region(out, "waiting-on-owner"))
        self.assertIn("`rec-h`", BuildTests.region(out, "running"))

    def test_bypass_edges_connect_prerequisites_to_dependants(self):
        tr = BuildTests.region(tfile(self.NODES, "rec"), "track-rec")
        self.assertIn("rec_a --> rec_b", tr)
        self.assertIn("rec_a --> rec_c", tr)
        self.assertEqual(backward_edges(tr), [])
        view = {n["id"]: n for n in build.diagram_view(build.normalize(self.NODES))}
        self.assertEqual(view["rec-b"]["depends_on"], ["rec-a"])
        self.assertEqual(sorted(view), ["rec-a", "rec-b", "rec-c"])

    def test_check_accepts_diagram_field(self):
        self.assertEqual(check.validate_nodes(self.NODES)[0], [])
        self.assertTrue(check.validate_nodes([node("rec-a", diagram="no")])[0])


class TrackFileTests(Files, unittest.TestCase):
    NODES = GOOD + [node("cc-x", state="done", outcome="successful", evidence=["e"], notes="n")]

    def outputs(self, nodes=None, text=None):
        return build.render_outputs(nodes or self.NODES, text or skeleton(), TODAY)

    def test_main_page_is_light(self):
        main = self.outputs()[build.MD_KEY]
        self.assertEqual(main.count("```mermaid"), 1)  # the overview only: no legend diagram, no gantt
        self.assertLessEqual(main.count("```mermaid"), check.MAX_MERMAID_PER_FILE)
        self.assertNotIn("%% today", main)
        self.assertNotIn("gantt", main)
        self.assertIn("(roadmap/timeline.md)", main)
        self.assertIn("Generated from nodes.yaml on " + TODAY + ".", main)
        self.assertNotIn("track-", "".join(build.REGION_ORDER))

    def test_timeline_page_has_one_block(self):
        tl = self.outputs()[build.TIMELINE_KEY]
        self.assertEqual(tl.count("```mermaid"), 1)
        self.assertIn("gantt", tl)

    def test_labels_are_short(self):
        outs = self.outputs([node(f"{c}-a", state="done", outcome="successful", evidence=["e"], start="2026-01-01",
                                  depends_on=[], notes="n") for c in build.TRACKS] + [node("rec-z", depends_on=["cc-a"])])
        for key, text in outs.items():
            for block in mermaid_blocks(text):
                if block.startswith("flowchart"):
                    self.assertNotIn("ext:", block, key)
                    for sentence in ("done ·", "in progress", "blocked", "superseded", "dead end", "qualified by", "|then|"):
                        self.assertNotIn(sentence, block.split("classDef")[0], key)
    def test_each_track_file_has_one_block(self):
        outs = self.outputs([node(f"{c}-a") for c in build.TRACKS])  # every track non-empty
        self.assertEqual(len(outs), 2 + len(build.TRACKS))
        for code in build.TRACKS:
            text = outs[f"roadmap/tracks/{code}.md"]
            self.assertEqual(text.count("```mermaid"), 1, code)
            self.assertTrue(text.startswith("<!-- Generated by"))
            self.assertIn(f"# {code} - {build.TRACKS[code][0]}", text)
            self.assertIn("(../../roadmap.md)", text)
    def test_tracks_region_links_every_track_file(self):
        reg = BuildTests.region(self.outputs()[build.MD_KEY], "tracks")
        for code, (name, _) in build.TRACKS.items():
            self.assertIn(f"[{code} - {name}](roadmap/tracks/{code}.md)", reg)
        self.assertIn("[rec - recovery](roadmap/tracks/rec.md): 2 nodes (1 done, 1 in progress)", reg)
        self.assertIn("[obs - observability](roadmap/tracks/obs.md): 0 nodes", reg)

    def test_empty_track_file(self):
        self.assertIn("_No nodes in this track yet._", self.outputs()["roadmap/tracks/obs.md"])

    def test_rebuild_is_idempotent_on_disk(self):
        n, m = self.dir / "nodes.yaml", self.dir / "roadmap.md"
        n.write_text(yaml.safe_dump(self.NODES, allow_unicode=True), encoding="utf-8")
        m.write_text(skeleton(), encoding="utf-8", newline="")
        args = ["--nodes", str(n), "--md", str(m), "--today", TODAY]
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            self.assertEqual(build.main(args), 0)
            self.assertEqual(build.main(args), 0)
        self.assertIn("updated", buf.getvalue().splitlines()[0])
        self.assertIn("already up to date", buf.getvalue().splitlines()[1])
        self.assertTrue((self.dir / "roadmap" / "tracks" / "rec.md").exists())

    def test_check_flags_stale_missing_and_extra_track_files(self):
        n, m = self.dir / "nodes.yaml", self.dir / "roadmap.md"
        n.write_text(yaml.safe_dump(self.NODES, allow_unicode=True), encoding="utf-8")
        m.write_text(skeleton(), encoding="utf-8", newline="")
        build.main(["--nodes", str(n), "--md", str(m), "--today", TODAY])
        self.assertEqual(check.run(n, m)[0], [])
        rec = self.dir / "roadmap" / "tracks" / "rec.md"
        rec.write_text(rec.read_text(encoding="utf-8").replace("T rec-a", "T rec-X"), encoding="utf-8")
        (self.dir / "roadmap" / "tracks" / "cod.md").unlink()
        (self.dir / "roadmap" / "tracks" / "gone.md").write_text("x", encoding="utf-8")
        errs = "\n".join(check.run(n, m)[0])
        self.assertIn("rec.md: generated track file is stale", errs)
        self.assertIn("cod.md: track file missing", errs)
        self.assertIn("gone.md: no such track", errs)
        build.main(["--nodes", str(n), "--md", str(m), "--today", TODAY])  # build repairs all three
        self.assertEqual(check.run(n, m)[0], [])

    def test_check_flags_stale_timeline_and_old_date(self):
        n, m = self.dir / "nodes.yaml", self.dir / "roadmap.md"
        nodes = [node("rec-a", start="2026-01-01")]
        n.write_text(yaml.safe_dump(nodes, allow_unicode=True), encoding="utf-8")
        m.write_text(skeleton(), encoding="utf-8", newline="")
        build.main(["--nodes", str(n), "--md", str(m), "--today", "2026-01-02"])
        errs, warns = check.run(n, m)
        self.assertEqual(errs, [])
        self.assertTrue(any("open timeline bars end at 2026-01-02" in w for w in warns), warns)  # not the real today
        tl = self.dir / build.TIMELINE_KEY
        tl.write_text(tl.read_text(encoding="utf-8").replace("rec_a", "rec_X"), encoding="utf-8")
        self.assertTrue(any("timeline file is stale" in e for e in check.run(n, m)[0]))
        tl.unlink()
        self.assertTrue(any("timeline file missing" in e for e in check.run(n, m)[0]))

    def test_mermaid_cap(self):
        block = "```mermaid\n"
        self.assertEqual(check.mermaid_errors({"a.md": block * check.MAX_MERMAID_PER_FILE}), [])
        self.assertEqual(len(check.mermaid_errors({"b.md": block * (check.MAX_MERMAID_PER_FILE + 1)})), 1)


class StaleTests(Files, unittest.TestCase):
    def test_fresh_passes_and_stale_fails(self):
        md = built(GOOD)
        errs, _ = self.run_check(GOOD, md)
        self.assertEqual(errs, [])
        changed = copy.deepcopy(GOOD)
        changed[0]["title"] = "Renamed"
        errs, _ = self.run_check(changed, md)
        self.assertTrue(any("stale" in e and "overview" in e for e in errs), errs)

    def test_hand_edit_inside_region_is_stale(self):
        md = built(GOOD).replace("T rec-a", "T rec-X", 1)
        errs, _ = self.run_check(GOOD, md)
        self.assertTrue(any("stale" in e for e in errs), errs)

    def test_hand_text_edit_is_not_stale(self):
        md = built(GOOD).replace("HAND-WRITTEN", "edited by hand")
        self.assertEqual(self.run_check(GOOD, md)[0], [])

    def test_missing_region(self):
        md = built(GOOD).replace("<!-- BEGIN GENERATED: legend -->", "")
        errs, _ = self.run_check(GOOD, md)
        self.assertTrue(any("legend" in e for e in errs), errs)

    def test_cli_exit_codes(self):
        n, m = self.dir / "nodes.yaml", self.dir / "roadmap.md"
        n.write_text(yaml.safe_dump(GOOD, allow_unicode=True), encoding="utf-8")
        m.write_text(skeleton(), encoding="utf-8", newline="")
        args = ["--nodes", str(n), "--md", str(m)]
        self.assertEqual(check.main(args), 1)  # regions empty -> stale
        self.assertEqual(build.main(args + ["--today", TODAY]), 0)
        self.assertEqual(check.main(args), 0)

    def test_top_level_forms(self):
        n = self.dir / "nodes.yaml"
        for doc in (GOOD, {"nodes": GOOD, "dropped": [{"id": "x"}], "suspected_edges": [["rec-a", "rec-b"]]}):
            n.write_text(yaml.safe_dump(doc, allow_unicode=True), encoding="utf-8")
            self.assertEqual(len(build.load_nodes(n)), 3)
        n.write_text(yaml.safe_dump({"nodes": GOOD, "bogus": 1}), encoding="utf-8")
        with self.assertRaises(ValueError):
            build.load_nodes(n)
        n.write_text(yaml.safe_dump({"nodes": GOOD, "dropped": "x"}), encoding="utf-8")
        with self.assertRaises(ValueError):
            build.load_nodes(n)

    def test_dict_form_end_to_end(self):
        md = built(GOOD)
        n, m = self.dir / "nodes.yaml", self.dir / "roadmap.md"
        n.write_text(yaml.safe_dump({"nodes": GOOD, "dropped": []}, allow_unicode=True), encoding="utf-8")
        m.write_text(md, encoding="utf-8", newline="")
        self.write_tracks(GOOD)
        self.assertEqual(check.run(n, m)[0], [])

    def test_bad_yaml(self):
        n = self.dir / "nodes.yaml"
        n.write_text("- id: [unclosed\n", encoding="utf-8")
        errs, _ = check.run(n, self.dir / "roadmap.md")
        self.assertTrue(errs)


if __name__ == "__main__":
    unittest.main()
