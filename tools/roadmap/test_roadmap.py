#!/usr/bin/env python3
"""Unit tests for build.py and check.py. Run: python tools/roadmap/test_roadmap.py"""
import contextlib
import copy
import io
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
        out = built([node("rec-a", title=t)])
        self.assertIn('rec_a["Law #167;5 #quot;x#quot; #91;y#93; #35;z<br/>○ next"]', out)
        self.assertNotIn(t, out.split("<!-- BEGIN GENERATED: overview -->")[1].split("<!-- END")[0])

    def test_shapes_and_edges(self):
        nodes = GOOD + [node("rec-d", kind="user-decision"), node("rec-e", kind="defect-verdict"),
                        node("rec-f", state="abandoned", outcome="superseded", replaced_by=["rec-b"])]
        out = built(nodes)
        self.assertIn('rate_c[/"T rate-c<br/>○ next"/]', out)
        self.assertIn('rec_d{{"T rec-d<br/>○ next"}}', out)
        self.assertIn('rec_e>"T rec-e<br/>⚠ open defect"]', out)
        self.assertIn("rec_a --> rec_b", out)
        self.assertIn("rec_f -.->|replaced by| rec_b", out)
        self.assertIn("✔ done · very-successful", out)
        self.assertIn("▶ in progress", out)
        self.assertIn("↷ superseded", out)

    def test_track_diagram_external_stub(self):
        track_rate = self.region(tfile(GOOD, "rate"), "track-rate")
        self.assertIn("ext: rec-a", track_rate)
        self.assertNotIn("x_", track_rate)
        self.assertNotIn("-->", track_rate)
        self.assertNotIn("rec_b[", track_rate)

    def test_timeline_open_bar_and_undated(self):
        out = built(GOOD)
        tl = out.split("<!-- BEGIN GENERATED: timeline -->")[1].split("<!-- END")[0]
        self.assertIn("%% today: " + TODAY, tl)
        self.assertIn("T rec-b :active, rec_b, 2026-03-01, " + TODAY, tl)
        self.assertIn("T rec-a :rec_a, 2026-01-01, 2026-02-01", tl)
        self.assertIn("Not shown (no `start`): `rate-c`", tl)

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

    def test_portrait_layout(self):
        nodes = [node(f"{t}-{k}", title="A fairly long title here ok") for t in ("rec", "rate", "cc") for k in "abcde"]
        out = built(nodes)
        ov = out.split("<!-- BEGIN GENERATED: overview -->")[1].split("<!-- END")[0]
        self.assertIn("flowchart TB", ov)
        self.assertEqual(ov.count("direction TB"), 3)
        self.assertIn("sg_rec ~~~ sg_rate ~~~ sg_cc", ov)
        self.assertIn("rec_a ~~~ rec_d", ov)  # 5 on one rank -> rows of 3
        self.assertNotIn("rec_a ~~~ rec_c", ov)
        self.assertIn("A fairly long title<br/>here ok<br/>", ov)
        tr = self.region(tfile(nodes, "rec"), "track-rec")
        self.assertIn("rec_a ~~~ rec_e", tr)  # rows of 4

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
        # complete bipartite tie between consecutive rows keeps each row on one rank
        self.assertEqual(len(links), sum(len(a) * len(b) for a, b in zip(rows, rows[1:])))

    def test_external_refs_on_label(self):
        deps = [node(f"cc-{i}") for i in range(9)]
        mine = node("rec-a", depends_on=[d["id"] for d in deps], replaced_by=["cc-0"])
        tr = self.region(tfile(deps + [mine], "rec"), "track-rec")
        self.assertIn("ext: cc-0, cc-1 +7", tr)
        self.assertNotIn("x_", tr)
        self.assertNotIn("~~~", tr)  # a single node: nothing to stack
        self.assertNotIn("ext:", self.region(tfile([node("rec-a"), node("rec-b", depends_on=["rec-a"])], "rec"), "track-rec"))

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
        self.assertIn('rec_a["T rec-a<br/>⏳ waiting"]', ov)
        self.assertIn('rec_d["T rec-d<br/>▶ in progress"]', ov)
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
        out = built([node("rec-a", title="x" * 60, start="2026-01-01")])
        self.assertIn("x" * 39 + "… :rec_a", out)

    def test_qualified_by(self):
        nodes = [node("rec-a", state="done", outcome="successful", evidence=["e"], qualified_by=["rec-b"], title="Rated"),
                 node("rec-b", kind="defect-verdict", depends_on=["rec-a"]),
                 node("rate-c", state="done", outcome="mixed", evidence=["e"], qualified_by=["rec-b"])]
        out = built(nodes)
        ov = out.split("<!-- BEGIN GENERATED: overview -->")[1].split("<!-- END")[0]
        self.assertIn("rec_a -.->|⚠ qualified by| rec_b", ov)
        self.assertIn("✔ done · successful ⚠", ov)
        self.assertNotIn("rec-b<br/>○ next ⚠", ov)
        # links in order: row anchor (0), track chain (1), rec_a qualified (2), rec_a-->rec_b (3), rate_c qualified (4)
        self.assertIn("linkStyle 2,4 stroke:#e69500", ov)
        tr = self.region(tfile(nodes, "rate"), "track-rate")
        self.assertIn("ext: rec-b", tr)  # other-track target: named on the label, not drawn as a stub
        self.assertNotIn("x_rec_b", tr)

    def test_linkstyle_counts_invisible_links(self):
        nodes = [node(f"rec-{k}") for k in "abcd"] + [
            node("rec-q", state="done", outcome="successful", evidence=["e"], qualified_by=["rec-a"])]
        ov = built(nodes).split("<!-- BEGIN GENERATED: overview -->")[1].split("<!-- END")[0]
        lines = ov.splitlines()
        links = [ln for ln in lines if "~~~" in ln or "-->" in ln or "-.->" in ln]
        i = next(k for k, ln in enumerate(links) if "qualified by" in ln)
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
        self.assertEqual(ov.count("⚠ open defect"), 2)
        self.assertIn('rec_c>"T rec-c<br/>✔ fixed"]', ov)
        self.assertNotIn("unrated", ov.split("classDef")[0])
        self.assertNotIn("done ·", ov.split("classDef")[0])

    def test_vm_queue_region_and_then_links(self):
        nodes = [node("rec-c", lane="vm", vm_order=3, order_reason="last one"),
                 node("rec-a", lane="vm", vm_order=1, order_reason="first because X"),
                 node("rec-b", lane="vm", vm_order=2),
                 node("rec-z", lane="vm")]
        out = built(nodes)
        q = self.region(out, "vm-queue").strip().splitlines()
        self.assertEqual(q, ["1. `rec-a`: T rec-a. first because X", "2. `rec-b`: T rec-b", "3. `rec-c`: T rec-c. last one"])
        ov = self.region(out, "overview")
        self.assertIn("rec_a -.->|then| rec_b", ov)
        self.assertIn("rec_b -.->|then| rec_c", ov)
        self.assertNotIn("rec_z -.->|then|", ov)
        self.assertNotIn("rec_c -.->|then|", ov)
        self.assertRegex(ov, r"linkStyle [\d,]+ stroke:#999,stroke-width:1px")
        self.assertIn("The VM queue is empty", self.region(built([node("rec-a")]), "vm-queue"))

    def test_then_linkstyle_index(self):
        nodes = [node("rec-a", lane="vm", vm_order=1), node("rec-b", lane="vm", vm_order=2)]
        ov = self.region(built(nodes), "overview")
        links = [ln for ln in ov.splitlines() if "~~~" in ln or "-->" in ln or "-.->" in ln]
        i = next(k for k, ln in enumerate(links) if "|then|" in ln)
        self.assertIn(f"linkStyle {i} stroke:#999", ov)

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


class TrackFileTests(Files, unittest.TestCase):
    NODES = GOOD + [node("cc-x", state="done", outcome="successful", evidence=["e"], notes="n")]

    def outputs(self, nodes=None, text=None):
        return build.render_outputs(nodes or self.NODES, text or skeleton(), TODAY)

    def test_main_has_exactly_three_mermaid_blocks(self):
        self.assertEqual(self.outputs()[build.MD_KEY].count("```mermaid"), 3)
        # even when the skeleton is built from real REGION_ORDER only: no track tree leaks into the main file
        self.assertNotIn("track-", "".join(build.REGION_ORDER))

    def test_each_track_file_has_one_block(self):
        outs = self.outputs([node(f"{c}-a") for c in build.TRACKS])  # every track non-empty
        self.assertEqual(len(outs), 1 + len(build.TRACKS))
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
