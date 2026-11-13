#!/usr/bin/env python3
"""Unit tests for build.py and check.py. Run: python tools/roadmap/test_roadmap.py"""
import copy
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
        return check.run(n, m)


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
        e = "\n".join(self.errs([node("rec-a", state="done", outcome="unrated", evidence=["x"], verdict_quote="q")]))
        self.assertIn("verdict_quote", e)

    def test_superseded_and_abandoned(self):
        self.assertIn("replaced_by", "\n".join(self.errs([node("rec-a", state="abandoned", outcome="superseded")])))
        self.assertIn("abandoned", "\n".join(self.errs([node("rec-a", state="abandoned", outcome="successful")])))
        self.assertEqual(self.errs([node("rec-a", state="abandoned", outcome="dead-end")]), [])

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
        self.assertIn('rec_e>"T rec-e<br/>○ next"]', out)
        self.assertIn("rec_a --> rec_b", out)
        self.assertIn("rec_f -.->|replaced by| rec_b", out)
        self.assertIn("✔ done · very-successful", out)
        self.assertIn("▶ in progress", out)
        self.assertIn("↷ superseded", out)

    def test_track_diagram_external_stub(self):
        out = built(GOOD)
        track_rate = out.split("<!-- BEGIN GENERATED: track-rate -->")[1].split("<!-- END")[0]
        self.assertIn('x_rec_a["rec-a"]', track_rate)
        self.assertIn("x_rec_a --> rate_c", track_rate)
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
        tr = out.split("<!-- BEGIN GENERATED: track-rec -->")[1].split("<!-- END")[0]
        self.assertIn("rec_a ~~~ rec_e", tr)  # rows of 4

    def test_gantt_title_truncated(self):
        out = built([node("rec-a", title="x" * 60, start="2026-01-01")])
        self.assertIn("x" * 39 + "… :rec_a", out)

    def test_table_row(self):
        out = built([node("rec-a", headline="a|b", evidence=["x", "y"], depends_on=[])])
        self.assertIn("| `rec-a` | T rec-a | next | unrated | a\\|b | x; y |  |", out)


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

    def test_bad_yaml(self):
        n = self.dir / "nodes.yaml"
        n.write_text("- id: [unclosed\n", encoding="utf-8")
        errs, _ = check.run(n, self.dir / "roadmap.md")
        self.assertTrue(errs)


if __name__ == "__main__":
    unittest.main()
