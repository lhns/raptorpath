"""Tests for tree_config.py, build.py and check.py.

Run from the repository root: python -m unittest discover -s skills/tech-tree/scripts/tests
"""
import contextlib
import copy
import datetime
import io
import re
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import yaml

HERE = Path(__file__).resolve().parent
SCRIPTS = HERE.parent
sys.path.insert(0, str(SCRIPTS))
import build  # noqa: E402
import check  # noqa: E402
import tree_config  # noqa: E402

CLEAN, LIBRARY, BROKEN = HERE / "clean", HERE / "library", HERE / "broken"
FIXTURE_TODAY = "2026-06-30"  # the date the committed fixture pages were built with
TODAY = "2026-11-13"
RAW = yaml.safe_load((CLEAN / "tech-tree.yaml").read_text(encoding="utf-8"))


def make_cfg(base, **over):
    """The clean fixture's config (with `over` changes) rooted in `base`."""
    raw = copy.deepcopy(RAW)
    raw.update(over)
    return tree_config.from_dict(raw, Path(base) / "tech-tree.yaml")


CFG = make_cfg(HERE / "unit")  # rendering needs no files


def node(i, **kw):
    n = {"id": i, "title": "T " + i, "track": i.split("-")[0], "kind": "feature", "state": "next",
         "outcome": "unrated", "evidence": [], "verdict_quote": "", "headline": "", "depends_on": [],
         "replaced_by": [], "lane": "none", "start": "", "end": "", "folded": [], "overview": True}
    n.update(kw)
    return n


def skeleton(cfg=CFG, hand="HAND-WRITTEN  text\twith odd   spacing\n"):
    parts = ["# Title\n\n", hand]
    for name in build.region_order(cfg):
        parts.append(f"<!-- BEGIN GENERATED: {name} -->\n<!-- END GENERATED: {name} -->\n\ntrailing {name}\n")
    return "".join(parts)


def built(nodes, text=None, cfg=CFG):
    return build.apply_regions(text or skeleton(cfg), build.render_regions(cfg, nodes, TODAY))


def tfile(nodes, code, cfg=CFG):
    return build.render_track_file(cfg, nodes, code, TODAY)


def region(out, name):
    return out.split(f"<!-- BEGIN GENERATED: {name} -->")[1].split("<!-- END")[0]


GOOD = [
    node("api-a", state="done", outcome="very-successful", evidence=["PR #1"], verdict_quote="q", start="2026-01-01",
         end="2026-02-01", confirmed="2026-02-02"),
    node("api-b", depends_on=["api-a"], start="2026-03-01", state="in-progress"),
    node("web-c", depends_on=["api-a"], kind="experiment"),
]


class Files:
    """A temp dir holding a config-rooted project (nodes.yaml, roadmap.md and the generated pages)."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.dir = Path(self.tmp.name)
        self.cfg = make_cfg(self.dir)

    def tearDown(self):
        self.tmp.cleanup()

    def write_nodes(self, nodes):
        self.cfg.nodes_path.write_text(yaml.safe_dump(nodes, allow_unicode=True), encoding="utf-8")

    def write_page(self, md):
        self.cfg.page_path.write_text(md, encoding="utf-8", newline="")

    def write_generated(self, nodes, day=TODAY):
        self.cfg.tracks_dir.mkdir(parents=True, exist_ok=True)
        for code in self.cfg.tracks:
            self.cfg.track_file(code).write_text(build.render_track_file(self.cfg, nodes, code, day), encoding="utf-8", newline="")
        self.cfg.timeline_path.write_text(build.render_timeline_file(self.cfg, nodes, day), encoding="utf-8", newline="")

    def run_check(self, nodes, md):
        self.write_nodes(nodes)
        self.write_page(md)
        self.write_generated(nodes)
        return check.run(self.cfg)

    def write_config(self):
        (self.dir / "tech-tree.yaml").write_text(yaml.safe_dump(RAW, sort_keys=False), encoding="utf-8")


def quiet(fn, *args):
    with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
        return fn(*args)


def run_script(name, *args, cwd=None):
    p = subprocess.run([sys.executable, str(SCRIPTS / name), *map(str, args)], capture_output=True, text=True,
                       encoding="utf-8", cwd=cwd)
    return p.returncode, p.stdout, p.stderr


EDGE_RE = re.compile(r"^\s+(\w+) (?:-->|-\.->(?:\|[^|]*\|)?) (\w+)$", re.M)
TIE_RE = re.compile(r"^\s+(\w+) ~~~ (\w+)$", re.M)


def mermaid_blocks(text):
    return re.findall(r"```mermaid\n(.*?)\n```", text, re.S)


def tree_blocks(text):
    """The flowchart blocks that draw nodes.yaml (the legend's sample boxes are not a tree)."""
    return [b for b in mermaid_blocks(text) if b.startswith("flowchart") and build.LEGEND_MARK not in b]


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


# ---------------------------------------------------------------- config
class ConfigTests(unittest.TestCase):
    def bad(self, **over):
        raw = copy.deepcopy(RAW)
        raw.update(over)
        with self.assertRaises(tree_config.ConfigError) as cm:
            tree_config.from_dict(raw, HERE / "tech-tree.yaml")
        return str(cm.exception)

    def test_example_loads_with_defaults_and_paths_relative_to_the_file(self):
        cfg = tree_config.load(CLEAN / "tech-tree.yaml")
        self.assertEqual(cfg.nodes_path, CLEAN / "nodes.yaml")
        self.assertEqual(cfg.page_path, CLEAN / "roadmap.md")
        self.assertEqual(cfg.track_file("api"), CLEAN / "roadmap" / "tracks" / "api.md")
        self.assertEqual(cfg.timeline_path, CLEAN / "roadmap" / "timeline.md")
        self.assertEqual(cfg.kinds["feature"], {"shape": "box", "rated": True, "owner_gate": False, "blocks": True})
        self.assertEqual(cfg.kinds["defect"], {"shape": "flag", "rated": False, "owner_gate": False, "blocks": False})
        self.assertEqual(cfg.queue_lane, "staging")
        lib = tree_config.load(LIBRARY / "tech-tree.yaml")
        self.assertIsNone(lib.queue_lane)
        self.assertIsNone(lib.overview)
        self.assertEqual(lib.history, {})

    def test_defaults_when_nodes_and_page_are_absent(self):
        raw = {k: v for k, v in RAW.items() if k not in ("nodes", "page")}
        cfg = tree_config.from_dict(raw, HERE / "x" / "tech-tree.yaml")
        self.assertEqual(cfg.nodes_path, HERE / "x" / "docs" / "roadmap" / "nodes.yaml")
        self.assertEqual(cfg.page_path, HERE / "x" / "docs" / "roadmap.md")
        self.assertEqual(cfg.tracks_dir, HERE / "x" / "docs" / "roadmap" / "tracks")

    def test_bad_values_are_named(self):
        self.assertIn("unknown key 'colour'", self.bad(colour=1))
        self.assertIn("tracks must be", self.bad(tracks={}))
        self.assertIn("needs a 'name'", self.bad(tracks={"api": {"scope": "x"}}))
        self.assertIn("shape 'blob'", self.bad(kinds={"feature": {"shape": "blob"}}))
        self.assertIn("unknown attribute 'colour'", self.bad(kinds={"feature": {"colour": 1}}))
        self.assertIn("rated must be true or false", self.bad(kinds={"feature": {"rated": "no"}}))
        self.assertIn("lanes must be", self.bad(lanes=[]))
        self.assertIn("queue_lane 'moon'", self.bad(queue_lane="moon"))
        self.assertIn("overview must be", self.bad(overview={"min": 9, "max": 3}))
        self.assertIn("exactly 1 capture group", self.bad(history={"weak_refs": {"x": "no-group"}}))
        self.assertIn("not a valid regex", self.bad(history={"weak_refs": {"x": "("}}))
        self.assertIn("reserved", self.bad(history={"weak_refs": {"sha": "(1)"}}))
        self.assertIn("needs a capture group", self.bad(history={"merge_subject": "plain"}))

    def test_cli_exit_2_on_bad_config(self):
        with tempfile.TemporaryDirectory() as d:
            bad = Path(d) / "tech-tree.yaml"
            bad.write_text("tracks: []\n", encoding="utf-8")
            for script in ("build.py", "check.py"):
                code, _, err = run_script(script, "--config", bad)
                self.assertEqual(code, 2, script)
                self.assertIn("tracks must be", err)
                code, _, err = run_script(script, "--config", Path(d) / "missing.yaml")
                self.assertEqual(code, 2)
                self.assertIn("cannot read config", err)
            bad.write_text("tracks: [unclosed\n", encoding="utf-8")
            self.assertEqual(run_script("check.py", "--config", bad)[0], 2)


# ---------------------------------------------------------------- graph and layout
class OneDirectionTests(unittest.TestCase):
    """Arrows must all point down (= forward in time)."""

    def test_every_edge_in_every_fixture_diagram_goes_down(self):
        for d in (CLEAN, LIBRARY):
            cfg = tree_config.load(d / "tech-tree.yaml")
            nodes = build.load_nodes(cfg.nodes_path)
            outs = build.render_outputs(cfg, nodes, skeleton(cfg), TODAY)
            seen = 0
            for key, text in outs.items():
                for block in tree_blocks(text):
                    seen += 1
                    self.assertEqual(backward_edges(block), [], f"{d.name} {key}")
            self.assertEqual(seen, 1 + len(cfg.tracks), d.name)

    def test_synthetic_all_edge_kinds_go_down(self):
        nodes = [
            node("api-new", state="done", outcome="successful", evidence=["e"], start="2026-05-01", qualified_by=["api-def"]),
            node("api-old", state="abandoned", outcome="superseded", replaced_by=["api-new"], start="2026-06-01"),
            node("api-def", kind="defect", state="in-progress", start="2026-02-01"),
            node("web-x", depends_on=["api-new"], start="2026-01-01"),
            node("api-und", depends_on=["web-x"]),
        ]
        for text in build.render_outputs(CFG, nodes, skeleton(), TODAY).values():
            for block in tree_blocks(text):
                self.assertEqual(backward_edges(block), [])

    def test_global_order_is_topological_then_by_start_then_undated(self):
        nodes = [node("api-late", start="2026-09-01", depends_on=["api-early"]),
                 node("api-early", start="2026-10-01"),
                 node("api-old", start="2026-01-01", state="abandoned", outcome="superseded", replaced_by=["api-mid"]),
                 node("api-mid", start="2026-01-15"),
                 node("api-nodate"), node("api-nodate2", depends_on=["api-nodate"])]
        order = build.time_order(build.normalize(nodes))
        self.assertEqual(order, ["api-old", "api-mid", "api-early", "api-late", "api-nodate", "api-nodate2"])

    def test_pack_rows_widths_and_dependencies(self):
        group = [node(f"api-{c}") for c in "abcdefghij"] + [node("api-k", depends_on=["api-a", "api-b"])]
        rows = build.pack_rows(group, 4)
        self.assertTrue(all(len(r) <= 4 for r in rows))
        self.assertEqual(sorted(x for r in rows for x in r), sorted(n["id"] for n in group))
        row_of = {x: i for i, r in enumerate(rows) for x in r}
        self.assertGreater(row_of["api-k"], max(row_of["api-a"], row_of["api-b"]))
        links = build.row_links(rows)
        # every node has a tie into the next row and one from the previous row, so each row stays on one rank
        self.assertEqual({ln.split()[0] for ln in links}, {build.sid(x) for r in rows[:-1] for x in r})
        self.assertEqual({ln.split()[2] for ln in links}, {build.sid(x) for r in rows[1:] for x in r})

    def test_replaced_by_is_a_precedence_edge(self):
        old = node("api-old", state="abandoned", outcome="superseded", replaced_by=["api-new"], start="2026-09-01")
        new = node("api-new", start="2026-01-01")  # starts earlier, yet the old node comes first
        self.assertEqual(build.time_order(build.normalize([new, old])), ["api-old", "api-new"])
        tr = region(tfile([new, old], "api"), "track-api")
        self.assertIn("api_old -.-> api_new", tr)
        self.assertEqual(backward_edges(tr), [])

    def test_transitive_prerequisites_not_drawn(self):
        nodes = [node("api-a"), node("api-b", depends_on=["api-a"]), node("api-c", depends_on=["api-a", "api-b"])]
        out = tfile(nodes, "api")
        tr = region(out, "track-api")
        self.assertIn("api_a --> api_b", tr)
        self.assertIn("api_b --> api_c", tr)
        self.assertNotIn("api_a --> api_c", tr)
        self.assertIn("| api-a, api-b |", out)  # the table keeps the full depends_on

    def test_band_follows_own_state_and_date(self):
        nodes = [node("api-d", kind="defect", state="in-progress", start="2026-07-05"),
                 node("api-fixed", state="done", outcome="bad", evidence=["e"], depends_on=["api-d"], start="2026-08-05"),
                 node("api-run", state="in-progress", start="2026-09-01", depends_on=["api-fixed"]),
                 node("api-late", state="blocked", depends_on=["api-run"])]
        norm = build.normalize(nodes)
        bands = {i: label for label, ids in build.overview_bands(CFG, norm, build.time_order(norm)) for i in ids}
        self.assertNotIn("Now", bands["api-d"])  # an open finding sits in the month it was found
        self.assertIn("2026", bands["api-d"])
        self.assertIn("2026", bands["api-fixed"])
        self.assertIn("Now", bands["api-run"])
        self.assertIn("Later", bands["api-late"])
        # an edge that would point up is dropped from the diagram, never drawn upward
        up = [node("api-a", state="done", outcome="successful", evidence=["e"], start="2026-02-01", depends_on=["api-b"]),
              node("api-b", state="blocked")]
        self.assertEqual(backward_edges(region(built(up), "overview")), [])


# ---------------------------------------------------------------- validation
class ValidateTests(unittest.TestCase):
    def errs(self, nodes, cfg=CFG):
        return check.validate_nodes(cfg, nodes)[0]

    def warns(self, nodes, cfg=CFG):
        return check.validate_nodes(cfg, nodes)[1]

    def test_good_passes(self):
        self.assertEqual(self.errs(GOOD), [])

    def test_dangling_edge(self):
        bad = copy.deepcopy(GOOD)
        bad[1]["depends_on"].append("api-missing")
        bad[2]["replaced_by"] = ["nope-x"]
        e = "\n".join(self.errs(bad))
        self.assertIn("dangling depends_on target 'api-missing'", e)
        self.assertIn("dangling replaced_by target 'nope-x'", e)

    def test_cycle(self):
        bad = [node("api-a", depends_on=["api-b"], state="parked"), node("api-b", depends_on=["api-c"], state="parked"),
               node("api-c", depends_on=["api-a"], state="parked")]
        e = self.errs(bad)
        self.assertTrue(any("cycle" in x and "api-a" in x and "api-b" in x for x in e), e)

    def test_cycle_through_replaced_by(self):
        bad = [node("api-a", replaced_by=["api-b"], state="abandoned", outcome="superseded", verdict_quote="q"),
               node("api-b", depends_on=["api-c"], state="parked"), node("api-c", depends_on=["api-b"], state="parked")]
        self.assertTrue(any("cycle" in x for x in self.errs(bad)))
        loop = [node("api-a", replaced_by=["api-b"], state="abandoned", outcome="superseded", verdict_quote="q"),
                node("api-b", replaced_by=["api-a"], state="abandoned", outcome="superseded", verdict_quote="q")]
        self.assertTrue(any("cycle" in x for x in self.errs(loop)))
        old_depends_on_new = [node("api-a", replaced_by=["api-b"], depends_on=["api-b"], state="abandoned",
                                   outcome="superseded", verdict_quote="q"), node("api-b")]
        self.assertTrue(any("cycle" in x for x in self.errs(old_depends_on_new)))

    def test_prerequisite_starting_later_is_a_warning(self):
        nodes = [node("api-a", start="2026-05-01"), node("api-b", depends_on=["api-a"], start="2026-02-01", state="in-progress")]
        errs, warns = check.validate_nodes(CFG, nodes)
        self.assertEqual(errs, [])
        self.assertTrue(any("api-b" in w and "api-a" in w and "starts after" in w for w in warns), warns)
        ok = [node("api-a", start="2026-01-01"), node("api-b", depends_on=["api-a"], start="2026-02-01", state="in-progress")]
        self.assertFalse(any("starts after" in w for w in self.warns(ok)))

    def test_self_dependency(self):
        self.assertTrue(self.errs([node("api-a", depends_on=["api-a"])]))

    def test_missing_field_unknown_field_and_bad_enum(self):
        n = node("api-a")
        del n["lane"]
        self.assertIn("missing field 'lane'", "\n".join(self.errs([n])))
        self.assertIn("unknown field 'colour'", "\n".join(self.errs([node("api-a", colour=1)])))
        self.assertIn("state 'weird'", "\n".join(self.errs([node("api-a", state="weird")])))
        self.assertIn("kind 'gadget'", "\n".join(self.errs([node("api-a", kind="gadget")])))
        self.assertIn("lane 'moon'", "\n".join(self.errs([node("api-a", lane="moon")])))
        self.assertIn("track 'zzz'", "\n".join(self.errs([node("api-a", track="zzz")])))

    def test_enums_come_from_the_config(self):
        lib = tree_config.load(LIBRARY / "tech-tree.yaml")
        self.assertIn("kind 'feature'", "\n".join(check.validate_nodes(lib, [node("core-a")])[0]))
        self.assertEqual(check.validate_nodes(lib, [node("core-a", kind="change", plain="p")])[0], [])

    def test_duplicate_and_bad_id(self):
        self.assertIn("duplicate id", "\n".join(self.errs([node("api-a"), node("api-a")])))
        self.assertIn("kebab-case", "\n".join(self.errs([node("api_a")])))
        self.assertIn("track code", "\n".join(self.errs([node("api-a", track="web")])))

    def test_done_rules(self):
        e = "\n".join(self.errs([node("api-a", state="done", outcome="successful", verdict_quote="q")]))
        self.assertIn("evidence", e)
        errs, warns = check.validate_nodes(CFG, [node("api-a", state="done", outcome="unrated", evidence=["x"])])
        self.assertEqual(errs, [])
        self.assertIn("1 done node is unrated (no quoted verdict yet): api-a", warns)
        # an unrated node may keep its source quote as evidence: a warning, not an error
        errs, warns = check.validate_nodes(CFG, [node("api-a", state="done", outcome="unrated", evidence=["x"], verdict_quote="q"),
                                                 node("api-b", state="done", outcome="unrated", evidence=["x"])])
        self.assertEqual(errs, [])
        self.assertEqual([w for w in warns if "unrated" in w],
                         ["2 done nodes are unrated (no quoted verdict yet): api-a, api-b"])  # one aggregate warning

    def test_rated_outcome_needs_a_verdict_quote(self):
        for outcome in ("very-successful", "successful", "mixed", "bad", "dead-end"):
            state = "abandoned" if outcome == "dead-end" else "done"
            n = node("api-a", state=state, outcome=outcome, evidence=["e"])
            self.assertIn("needs a verdict_quote", "\n".join(self.errs([n])), outcome)
            n["verdict_quote"] = "   "
            self.assertIn("needs a verdict_quote", "\n".join(self.errs([n])), outcome)
            n["verdict_quote"] = "the sentence"
            self.assertEqual(self.errs([n]), [], outcome)
        n = node("api-a", state="abandoned", outcome="superseded", replaced_by=["api-b"])
        self.assertIn("needs a verdict_quote", "\n".join(self.errs([n, node("api-b")])))
        self.assertEqual(self.errs([node("api-a", state="done", outcome="unrated", evidence=["e"])]), [])

    def test_superseded_and_abandoned(self):
        e = "\n".join(self.errs([node("api-a", state="abandoned", outcome="superseded", verdict_quote="q")]))
        self.assertIn("replaced_by", e)
        self.assertIn("abandoned", "\n".join(self.errs([node("api-a", state="abandoned", outcome="successful", verdict_quote="q")])))
        self.assertEqual(self.errs([node("api-a", state="abandoned", outcome="dead-end", verdict_quote="q")]), [])

    def test_qualified_by_validation(self):
        bad = [node("api-a", state="done", outcome="successful", verdict_quote="q", evidence=["e"], qualified_by=["api-zzz"])]
        self.assertIn("dangling qualified_by target 'api-zzz'", "\n".join(self.errs(bad)))
        ok = [node("api-a", state="done", outcome="successful", verdict_quote="q", evidence=["e"], qualified_by=["api-b"], notes="n"),
              node("api-b", depends_on=["api-a"])]  # qualified_by is not a dependency: no cycle
        self.assertEqual(self.errs(ok), [])
        self.assertTrue(self.errs([node("api-a", notes=3)]))

    def test_na_outcome_only_on_owner_gates(self):
        self.assertIn("n/a", "\n".join(self.errs([node("api-a", outcome="n/a")])))
        gate = dict(kind="owner-decision", state="done", evidence=["e"])
        for outcome in ("n/a", "unrated"):
            errs, warns = check.validate_nodes(CFG, [node("api-a", outcome=outcome, **gate)])
            self.assertEqual((errs, [w for w in warns if "unrated" in w]), ([], []))

    def test_unrated_kind_never_carries_an_outcome(self):
        e = "\n".join(self.errs([node("api-a", kind="defect", state="done", outcome="successful", verdict_quote="q", evidence=["e"])]))
        self.assertIn("is not rated", e)
        errs, warns = check.validate_nodes(CFG, [node("api-a", kind="defect", state="done", evidence=["e"], plain="p")])
        self.assertEqual((errs, [w for w in warns if "unrated" in w]), ([], []))

    def test_readiness_rule(self):
        done = node("api-a", state="done", outcome="successful", verdict_quote="q", evidence=["e"])
        for st in ("in-progress", "next", "parked"):
            blocked = node("api-b", depends_on=["api-c"])
            other = node("api-c", state=st)
            e = "\n".join(self.errs([done, blocked, other]))
            self.assertIn("depends_on 'api-c'", e, st)
        self.assertEqual(self.errs([done, node("api-b", depends_on=["api-a"])]), [])
        # owner gates and unrated kinds are exempt themselves
        for kind in ("owner-decision", "defect"):
            ex = node("api-b", kind=kind, depends_on=["api-c"], outcome="n/a" if kind == "owner-decision" else "unrated")
            self.assertEqual(self.errs([ex, node("api-c")]), [])

    def test_readiness_honours_blocks_false(self):
        diag = node("api-b", depends_on=["api-d"])
        open_defect = node("api-d", kind="defect", state="in-progress")
        self.assertEqual(self.errs([diag, open_defect]), [])  # defect has blocks: false
        # the same dependency on a kind that blocks is an error
        other = node("api-d", state="in-progress")
        self.assertIn("depends_on 'api-d'", "\n".join(self.errs([diag, other])))
        # and a config that flips the attribute flips the result, with no change to the nodes
        flipped = make_cfg(HERE / "unit", kinds={**RAW["kinds"], "defect": {"shape": "flag", "rated": False}})
        self.assertIn("depends_on 'api-d'", "\n".join(self.errs([diag, open_defect], flipped)))
        loose = make_cfg(HERE / "unit", kinds={**RAW["kinds"], "feature": {"blocks": False}})
        self.assertEqual(self.errs([diag, other], loose), [])

    def test_queue_order_validation(self):
        a = node("api-a", lane="staging", queue_order=1, plain="p")
        b = node("api-b", lane="staging", queue_order=1, plain="p")
        self.assertIn("queue_order 1 already used", "\n".join(self.errs([a, b])))
        self.assertTrue(self.errs([node("api-a", queue_order="x")]))
        self.assertTrue(self.errs([node("api-a", queue_order=True)]))
        self.assertTrue(any("queue lane" in w for w in self.warns([node("api-a", queue_order=1, plain="p")])))
        self.assertFalse(any("queue lane" in w for w in self.warns([a])))
        no_queue = make_cfg(HERE / "unit", queue_lane=None)
        self.assertIn("no queue_lane", "\n".join(self.errs([a], no_queue)))

    def test_confirmed_validation_and_one_aggregate_warning(self):
        self.assertIn("confirmed:", "\n".join(self.errs([node("api-a", confirmed="soon")])))
        nodes = [node(f"api-{c}", state="done", outcome="mixed", verdict_quote="q", evidence=["e"]) for c in "abc"]
        nodes[0]["confirmed"] = "2026-01-02"
        agg = [w for w in self.warns(nodes) if "not confirmed" in w]
        self.assertEqual(agg, ["2 rated nodes are not confirmed by the owner"])
        nodes[1]["confirmed"] = nodes[2]["confirmed"] = "2026-01-03"
        self.assertFalse([w for w in self.warns(nodes) if "not confirmed" in w])

    def test_plain_warning(self):
        warns = self.warns([node("api-a", plain="ok"), node("api-b"), node("api-c", overview=False, state="done",
                                                                           outcome="successful", verdict_quote="q", evidence=["e"])])
        self.assertTrue(any("api-b" in w and "plain" in w for w in warns))
        self.assertFalse(any("api-a" in w and "plain" in w for w in warns))
        self.assertFalse(any("api-c" in w and "plain" in w for w in warns))  # done, not in overview: no warning

    def test_overview_size_warning_only_when_configured(self):
        self.assertTrue(any("overview has 3 nodes (target 12-30)" in w for w in self.warns(GOOD)))
        lib = tree_config.load(LIBRARY / "tech-tree.yaml")
        self.assertFalse(any("overview has" in w for w in check.validate_nodes(lib, [node("core-a", kind="change", plain="p")])[1]))

    def test_waiting_on_validation(self):
        self.assertTrue(self.errs([node("api-a", waiting_on=3)]))
        self.assertTrue(any("waiting_on" in w for w in self.warns([node("api-a", waiting_on="x", plain="p")])))
        self.assertFalse(any("waiting_on" in w for w in self.warns([node("api-a", state="in-progress", waiting_on="x", plain="p")])))

    def test_diagram_field(self):
        self.assertTrue(self.errs([node("api-a", diagram="no")]))

    def test_long_title_and_quote(self):
        self.assertTrue(any("longer than 6 words" in w for w in self.warns([node("api-a", title="a b c d e f g")])))
        self.assertIn("longer than 300", "\n".join(self.errs([node("api-a", verdict_quote="x" * 301, outcome="mixed")])))


# ---------------------------------------------------------------- build
class BuildTests(unittest.TestCase):
    def test_label_escaping(self):
        t = 'Item ' + chr(0xA7) + '5 "x" [y] #z'
        out = tfile([node("api-a", title=t)], "api")
        self.assertIn('api_a["○ Item #167;5 #quot;x#quot; #91;y#93; #35;z"]', out)
        self.assertNotIn(t, region(out, "track-api"))

    def test_shapes_come_from_the_kind_and_edges(self):
        nodes = GOOD + [node("api-e", kind="defect"), node("api-f", state="abandoned", outcome="superseded", replaced_by=["api-b"])]
        api = region(tfile(nodes, "api"), "track-api")
        self.assertIn('web_c[/"○ T web-c"/]', region(tfile(nodes, "web"), "track-web"))
        self.assertIn('api_e>"⚠ T api-e"]', api)
        self.assertIn("api_a --> api_b", api)
        self.assertIn("api_f -.-> api_b", api)
        self.assertIn('api_a["✔ T api-a"]', api)
        self.assertIn('api_b["▶ T api-b"]', api)
        self.assertIn('api_f["↷ T api-f"]', api)
        self.assertNotIn("|", api.split("classDef")[0])  # no text on edges

    def test_every_shape_name_has_a_drawing(self):
        self.assertEqual(set(build.SHAPES), set(tree_config.SHAPES))
        for shape, (opening, closing) in build.SHAPES.items():
            cfg = make_cfg(HERE / "unit", kinds={**RAW["kinds"], "feature": {"shape": shape}})
            n = build.normalize([node("api-a")])[0]
            self.assertEqual(build.node_def(n, cfg.kinds["feature"]), f'api_a{opening}○ T api-a{closing}')

    def test_owner_gate_nodes_are_never_drawn(self):
        nodes = [node("api-a"), node("api-g", kind="owner-decision", outcome="n/a")]
        for text in build.render_outputs(CFG, nodes, skeleton(), TODAY).values():
            for block in mermaid_blocks(text):
                if build.LEGEND_MARK not in block:
                    self.assertNotIn("api_g", block)

    def test_track_diagram_has_no_cross_track_edges(self):
        track_web = region(tfile(GOOD, "web"), "track-web")
        self.assertNotIn("-->", track_web)
        self.assertNotIn("api_b[", track_web)

    def test_timeline_open_bar_and_undated(self):
        tl = build.render_timeline_file(CFG, GOOD, TODAY)
        self.assertIn("Generated on " + TODAY + ".", tl)
        self.assertIn("T api-b :active, api_b, 2026-03-01, " + TODAY, tl)
        self.assertIn("T api-a :api_a, 2026-01-01, 2026-02-01", tl)
        self.assertIn("Not shown (no `start`): `web-c`", tl)
        self.assertEqual(tl.count("```mermaid"), 1)
        main = built(GOOD)
        self.assertNotIn("gantt", main)
        self.assertIn("(roadmap/timeline.md)", region(main, "timeline"))
        self.assertEqual(check.TODAY_RE.search(main).group(1), TODAY)
        self.assertEqual(check.TODAY_RE.search(tl).group(1), TODAY)

    def test_preservation_and_idempotence(self):
        text = skeleton(hand="keep this\r\n\ttabs  and trailing spaces   \n\n")
        once = built(GOOD, text)
        self.assertEqual(built(GOOD, once), once)
        self.assertEqual(once.split("<!-- BEGIN GENERATED: legend -->")[0], text.split("<!-- BEGIN GENERATED: legend -->")[0])
        for name in build.region_order(CFG):
            self.assertIn(f"<!-- END GENERATED: {name} -->\n\ntrailing {name}\n", once)

    def test_crlf_preserved(self):
        out = built(GOOD, skeleton().replace("\n", "\r\n"))
        self.assertNotIn("\n", out.replace("\r\n", ""))

    def test_missing_marker(self):
        with self.assertRaises(ValueError):
            build.apply_regions("# nothing here\n", build.render_regions(CFG, GOOD, TODAY))

    def test_overview_bands_and_grid_rows(self):
        nodes = [node(f"{t}-{k}", title="A fairly long title here ok", state="done", outcome="successful", evidence=["e"],
                      start="2026-0%d-01" % (1 + "abcde".index(k)))
                 for t in ("api", "web", "data") for k in "abcde"]
        nodes += [node("api-p", state="in-progress"), node("api-q"), node("api-r", state="blocked")]
        ov = region(built(nodes), "overview")
        self.assertIn("flowchart TB", ov)
        self.assertNotIn("direction", ov)
        for label in ('"Now (in progress)"', '"Next"', '"Later (blocked or parked)"'):
            self.assertIn(label, ov)
        self.assertRegex(ov, r'subgraph bd_0\["Jan 2026')  # past bands are months, one band per month: layout.band_min is 1
        self.assertIn("api · ✔ A fairly long<br/>title here ok", ov)  # track prefix, one glyph, wrapped title
        rows = {}
        for i, k in row_levels(ov).items():
            rows.setdefault(k, []).append(i)
        self.assertLessEqual(max(map(len, rows.values())), CFG.layout["overview_width"])
        tr = region(tfile([node(f"api-{c}") for c in "abcdefg"], "api"), "track-api")
        rows = {}
        for i, k in row_levels(tr).items():
            rows.setdefault(k, []).append(i)
        self.assertEqual([len(rows[k]) for k in sorted(rows)], [3, 3, 1])

    def test_track_tree_packs_disconnected_nodes_into_rows(self):
        nodes = [node(f"api-n{i:02d}") for i in range(13)] + [node("api-z", depends_on=["api-n00"])]
        nodes[0].update(state="done", outcome="successful", evidence=["e"])
        tr = region(tfile(nodes, "api"), "track-api")
        down = {}
        for a, _, b in (ln.split() for ln in tr.splitlines() if " ~~~ " in ln):
            down.setdefault(a, []).append(b)
        sel = [f"api_n{i:02d}" for i in range(13)] + ["api_z"]
        targets = {b for bs in down.values() for b in bs}
        roots = [x for x in sel if x not in targets]
        self.assertLessEqual(len(roots), TRACK_ROW)
        seen, todo = set(roots), list(roots)
        while todo:
            for k in down.get(todo.pop(), []):
                if k not in seen:
                    seen.add(k)
                    todo.append(k)
        self.assertTrue(set(sel) <= seen)
        self.assertNotIn("api_z", roots)  # a node never shares a row with its prerequisite

    def test_waiting_state(self):
        nodes = [node("api-a", state="in-progress", waiting_on="a freeze"),
                 node("api-b", state="in-progress", waiting_on="a freeze"),
                 node("api-c", state="in-progress", waiting_on="an owner call"),
                 node("api-d", state="in-progress"),
                 node("api-e", kind="defect", state="in-progress", waiting_on="x")]
        out = built(nodes)
        ov = region(out, "overview")
        self.assertIn('api_a["api · ⏳ T api-a"]', ov)
        self.assertIn('api_d["api · ▶ T api-d"]', ov)
        self.assertIn("class api_a,api_b,api_c waiting", ov)
        self.assertIn("classDef waiting fill:#e3f2fd", ov)
        running = region(out, "running")
        self.assertIn("`api-d`", running)
        for i in "abce":
            self.assertNotIn(f"api-{i}", running)
        bw = region(out, "built-waiting")
        self.assertIn("**a freeze**\n\n- `api-a`: T api-a\n- `api-b`: T api-b", bw)
        self.assertIn("**an owner call**\n\n- `api-c`: T api-c", bw)
        self.assertNotIn("api-d", bw)
        self.assertNotIn("api-e", bw)  # unrated kinds are not "built, waiting"
        self.assertEqual(region(built([node("api-a")]), "built-waiting").strip(), "_None._")

    def test_gantt_title_truncated(self):
        out = build.render_timeline_file(CFG, [node("api-a", title="x" * 60, start="2026-01-01")], TODAY)
        self.assertIn("x" * 39 + "… :api_a", out)

    def test_qualified_by(self):
        nodes = [node("api-a", state="done", outcome="successful", evidence=["e"], qualified_by=["api-b"], title="Rated"),
                 node("api-b", kind="defect", depends_on=["api-a"]),
                 node("data-c", state="done", outcome="mixed", evidence=["e"], qualified_by=["api-b"])]
        ov = region(built(nodes), "overview")
        links = [ln.strip() for ln in ov.splitlines() if "~~~" in ln or "-->" in ln or "-.->" in ln]
        self.assertIn("api_a -.-> api_b", links)
        self.assertIn("data_c -.-> api_b", links)
        want = sorted(links.index(x) for x in ("api_a -.-> api_b", "data_c -.-> api_b"))
        self.assertIn("linkStyle " + ",".join(map(str, want)) + " stroke:#e69500,stroke-width:3px", ov)
        self.assertNotIn("api_b", region(tfile(nodes, "data"), "track-data"))  # other-track target: not drawn

    def test_details_table_shows_verdict_headline_plain_and_notes(self):
        n = node("api-a", notes="why | so", verdict_quote="Merged; fast.", headline="12 ms", plain="Plain words.",
                 evidence=["PR #5"])
        tb = region(tfile([n, node("api-b")], "api"), "table-api")
        self.assertLess(tb.index("| id | title | kind"), tb.index("### Details per node"))
        self.assertIn("| `api-a` | T api-a | Merged; fast. | 12 ms | Plain words. | why \\| so |  |  |  |  |", tb)
        self.assertNotIn("`api-b` | T api-b | ", tb.split("### Details")[1])
        for field in build.DETAIL_FIELDS:  # any one of them earns a row
            value = 3 if field == "queue_order" else "only this"
            one = region(tfile([node("api-a", **{field: value})], "api"), "table-api")
            self.assertIn("### Details per node", one, field)

    def test_table_row(self):
        out = tfile([node("api-a", headline="a|b", evidence=["x", "y"])], "api")
        self.assertIn("| `api-a` | T api-a | feature | none | next | unrated |  |  |  |  |  | a\\|b | x; y |", out)

    def test_done_by_month_groups_every_done_node(self):
        out = region(built([
            node("api-a", state="done", overview=False, start="2026-07-02", end="2026-07-20"),
            node("api-b", state="done", overview=False, start="2026-08-01"),
            node("api-c", state="done", overview=False),
            node("api-d", state="next"),
        ]), "done-by-month")
        self.assertLess(out.index("Aug 2026"), out.index("Jul 2026"))
        self.assertLess(out.index("Jul 2026"), out.index("Undated"))
        for i in ("api-a", "api-b", "api-c"):
            self.assertIn(f"`{i}`", out)
        self.assertNotIn("api-d", out)

    def test_done_by_month_includes_abandoned_with_its_tag(self):
        out = region(built([
            node("api-a", state="done", overview=False, start="2026-07-02"),
            node("api-b", state="abandoned", outcome="dead-end", overview=False, start="2026-07-03"),
            node("api-c", state="abandoned", outcome="superseded", replaced_by=["api-a"], overview=False, start="2026-07-04"),
        ]), "done-by-month")
        self.assertIn("<b>Jul 2026</b>: 1 done, 2 abandoned", out)
        self.assertIn("- `api-b`: T api-b (✖ dead end)", out)
        self.assertIn("- `api-c`: T api-c (↷ superseded)", out)

    def test_done_by_month_lists_labelled_folded_items_only(self):
        folded = ["Reorder buffer (2026-03-17, commit ed1854de)",
                  "Two shas (2026-07-04, commit 02edf367, commit beda623b)",
                  "Owner rule (2026-07-02, session record)",
                  "commit 1234abcd", "branch P7 fix", "see open question 7",
                  "not a label (2026-07-02, commit xyz)"]
        out = region(built([node("api-a", state="done", overview=False, start="2026-07-02", folded=folded)]), "done-by-month")
        lines = out.splitlines()
        i = lines.index("- `api-a`: T api-a (✔ done · unrated)")
        self.assertEqual(lines[i + 1:i + 4], ["  - Reorder buffer (2026-03-17)", "  - Two shas (2026-07-04)",
                                              "  - Owner rule (2026-07-02)"])
        for bare in ("1234abcd", "P7 fix", "open question 7", "not a label"):
            self.assertNotIn(bare, out)

    def test_track_page_lists_every_folded_item(self):
        tb = region(tfile([node("api-a", folded=["commit 1234abcd", "Fix | it (2026-07-02, commit 1234abcd)"]),
                           node("api-b")], "api"), "table-api")
        part = tb.split("### Folded items per node")[1]
        self.assertIn("- `api-a`: T api-a\n  - commit 1234abcd\n  - Fix \\| it (2026-07-02, commit 1234abcd)", part)
        self.assertNotIn("api-b", part)
        self.assertNotIn("### Folded items", region(tfile([node("api-b")], "api"), "table-api"))

    def test_region_order_and_queue_region_follows_the_config(self):
        order = build.region_order(CFG)
        self.assertEqual(order, ["legend", "ratings", "overview", "plain-words", "ready-now", "running", "built-waiting",
                                 "waiting-on-owner", "queue", "timeline", "done-by-month", "tracks"])
        pos = [built([node("api-a")]).index(f"<!-- BEGIN GENERATED: {n} -->") for n in order]
        self.assertEqual(pos, sorted(pos))
        plain = make_cfg(HERE / "unit", queue_lane=None)
        self.assertNotIn("queue", build.region_order(plain))
        self.assertNotIn("queue", build.render_regions(plain, GOOD, TODAY))

    def test_queue_region_and_no_then_links(self):
        nodes = [node("api-c", lane="staging", queue_order=3, order_reason="last one"),
                 node("api-a", lane="staging", queue_order=1, order_reason="first because X"),
                 node("api-b", lane="staging", queue_order=2),
                 node("api-z", lane="staging")]
        out = built(nodes)
        q = region(out, "queue").strip().splitlines()
        self.assertEqual(q, ["1. `api-a`: T api-a. first because X", "2. `api-b`: T api-b", "3. `api-c`: T api-c. last one"])
        ov = region(out, "overview")
        self.assertNotIn("then", ov)
        self.assertNotIn("-.->", ov)  # the queue is a list, not edges
        self.assertIn("_The queue is empty._", region(built([node("api-a")]), "queue"))

    def test_ratings_region(self):
        nodes = [node("api-a", state="done", outcome="successful", confirmed="2026-01-01"),
                 node("api-b", state="done", outcome="mixed"), node("api-c", state="done", outcome="bad"),
                 node("api-d", state="done", outcome="unrated"), node("api-e", kind="owner-decision", outcome="n/a")]
        r = region(built(nodes), "ratings").strip()
        self.assertEqual(r.splitlines()[0], "1 of 3 rated nodes are confirmed by the owner.")
        self.assertIn("Not yet confirmed: `api-b`, `api-c`.", r)
        nodes[1]["confirmed"] = nodes[2]["confirmed"] = "2026-01-02"
        self.assertEqual(region(built(nodes), "ratings").strip(), "3 of 3 rated nodes are confirmed by the owner.")
        self.assertEqual(region(built([node("api-a")]), "ratings").strip(), "No node carries a rating yet.")

    def test_defect_tags(self):
        nodes = [node("api-a", kind="defect", state="in-progress"), node("api-b", kind="defect"),
                 node("api-c", kind="defect", state="done", evidence=["e"])]
        ov = region(built(nodes), "overview")
        head = ov.split("classDef")[0]
        self.assertEqual(head.count("⚠ T api-"), 2)
        self.assertIn('api_c>"api · ✔ T api-c"]', ov)
        self.assertNotIn("unrated", head)

    def test_plain_words_region(self):
        nodes = [node("api-a", plain="Does a thing, simply."),
                 node("api-b", overview=False, state="done", outcome="successful", evidence=["e"]),
                 node("api-c", overview=False, state="parked", plain="Waiting."),
                 node("api-d", overview=False, plain="forward")]
        t = region(built(nodes), "plain-words")
        self.assertIn("| T api-a | ○ next | Does a thing, simply. |", t)
        self.assertIn("| T api-c | ⏸ parked | Waiting. |", t)
        self.assertIn("| T api-d | ○ next |", t)
        self.assertNotIn("T api-b", t)  # done and not in the overview
        self.assertIn("| title | status | in plain words |", t)

    def test_lane_lists_follow_the_config_and_exclude_owner_gates(self):
        nodes = [node("api-a", lane="staging"), node("api-b", lane="owner"),
                 node("api-c", lane="staging", state="in-progress"),
                 node("api-d", lane="local", state="in-progress"),
                 node("api-e", kind="defect", state="in-progress", lane="staging"),
                 node("api-f", kind="owner-decision", lane="owner", outcome="n/a"),
                 node("api-g", kind="owner-decision", state="done", outcome="n/a", evidence=["e"]),
                 node("api-h", kind="owner-decision", state="abandoned", outcome="dead-end")]
        out = built(nodes)
        ready = region(out, "ready-now")
        self.assertIn("**staging**", ready)
        self.assertIn("- `api-a`: T api-a", ready)
        self.assertIn("`api-b`", ready)  # a plain node in the owner lane is ready work for the owner
        self.assertNotIn("api-f", ready)  # owner gates are listed under "Waiting on the owner"
        self.assertNotIn("api-c", ready)
        running = region(out, "running")
        self.assertLess(running.index("`api-d`"), running.index("`api-c`"))  # lane order follows the config
        self.assertNotIn("api-e", running)
        waiting = region(out, "waiting-on-owner")
        self.assertIn("`api-f`", waiting)
        self.assertNotIn("api-g", waiting)
        self.assertNotIn("api-h", waiting)
        self.assertEqual(region(built([node("api-a", state="done", outcome="successful", evidence=["e"])]), "running").strip(), "_None._")


TRACK_ROW = CFG.layout["track_width"] + 1  # at most one row's worth of root nodes plus the node with a prerequisite


class HiddenNodeTests(unittest.TestCase):
    """Owner gates and `diagram: false` nodes are not drawn; their dependencies are rewired around them."""

    NODES = [node("api-a", state="done", outcome="successful", verdict_quote="q", evidence=["e"], start="2026-01-01"),
             node("api-u", kind="owner-decision", outcome="n/a", depends_on=["api-a"], start="2026-02-01", lane="owner"),
             node("api-h", depends_on=["api-u"], diagram=False, start="2026-03-01", state="in-progress"),
             node("api-b", depends_on=["api-h"], start="2026-04-01", state="in-progress"),
             node("api-c", depends_on=["api-u"], start="2026-05-01", state="in-progress")]

    def test_hidden_nodes_absent_from_diagrams_but_present_in_tables_and_lists(self):
        out = built(self.NODES)
        files = build.render_outputs(CFG, self.NODES, skeleton(), TODAY)
        api = files[CFG.track_file("api")]
        for text in (out, api):
            for block in mermaid_blocks(text):
                self.assertNotIn("api_u", block)
                self.assertNotIn("api_h", block)
        self.assertIn("`api-u`", api)
        self.assertIn("`api-h`", api)
        self.assertIn("`api-u`", region(out, "waiting-on-owner"))
        self.assertIn("`api-h`", region(out, "running"))

    def test_bypass_edges_connect_prerequisites_to_dependants(self):
        tr = region(tfile(self.NODES, "api"), "track-api")
        self.assertIn("api_a --> api_b", tr)
        self.assertIn("api_a --> api_c", tr)
        self.assertEqual(backward_edges(tr), [])
        view = {n["id"]: n for n in build.diagram_view(CFG, build.normalize(self.NODES))}
        self.assertEqual(view["api-b"]["depends_on"], ["api-a"])
        self.assertEqual(sorted(view), ["api-a", "api-b", "api-c"])

    def test_check_accepts_the_nodes(self):
        self.assertEqual(check.validate_nodes(CFG, self.NODES)[0], [])

    def test_a_done_owner_gate_is_drawn_in_the_overview_only(self):
        nodes = [node("api-a", state="done", outcome="successful", verdict_quote="q", evidence=["e"], start="2026-01-01"),
                 node("api-g", kind="owner-decision", state="done", outcome="n/a", evidence=["e"], depends_on=["api-a"],
                      start="2026-02-01", lane="owner"),
                 node("api-o", kind="owner-decision", outcome="n/a", depends_on=["api-a"], start="2026-02-05", lane="owner"),
                 node("api-b", depends_on=["api-g"], state="in-progress", start="2026-03-01")]
        files = build.render_outputs(CFG, nodes, skeleton(), TODAY)
        ov = mermaid_blocks(region(files[CFG.page_path], "overview"))[0]
        self.assertIn('api_g{{"api \u00b7 \u2714 T api-g"}}', ov)  # the kind's own shape
        self.assertIn("class api_g decided", ov)
        self.assertIn("api_g --> api_b", ov)
        self.assertNotIn("api_o", ov)  # an open gate is drawn nowhere
        for block in mermaid_blocks(files[CFG.track_file("api")]):
            self.assertNotIn("api_g", block)
            self.assertNotIn("api_o", block)
        self.assertIn("`api-o`", region(files[CFG.page_path], "waiting-on-owner"))
        self.assertNotIn("api-g", region(files[CFG.page_path], "waiting-on-owner"))
        self.assertIn("overview only", region(files[CFG.page_path], "legend"))

    def test_a_dependency_through_an_open_gate_is_still_rewired(self):
        view = {n["id"]: n for n in build.diagram_view(CFG, build.normalize(HiddenNodeTests.NODES), owner_done=True)}
        self.assertEqual(view["api-c"]["depends_on"], ["api-a"])
        self.assertNotIn("api-u", view)


# ---------------------------------------------------------------- legend
class LegendTests(unittest.TestCase):
    """The legend is a diagram built from the trees' own classDefs, shapes and arrows, so it cannot drift."""

    def legend(self, cfg=CFG):
        nodes = GOOD if cfg is CFG else build.load_nodes(cfg.nodes_path)
        blocks = mermaid_blocks(region(built(nodes, cfg=cfg), "legend"))
        self.assertEqual(len(blocks), 1)
        self.assertIn(build.LEGEND_MARK, blocks[0])
        return blocks[0]

    @staticmethod
    def classes_in(block):
        out = {}
        for ids, cls in re.findall(r"^\s+class (\S+) (\w+)$", block, re.M):
            out.setdefault(cls, []).extend(ids.split(","))
        return out

    def drawable_classes(self, cfg):
        drawable = set()
        for kind in cfg.kinds:
            for state in build.STATES:
                for outcome in build.OUTCOMES:
                    for waiting in ("", "x"):
                        n = node("api-a", kind=kind, state=state, outcome=outcome, waiting_on=waiting, evidence=["e"],
                                 verdict_quote="q", replaced_by=["api-b"] if outcome == "superseded" else [])
                        if check.validate_nodes(CFG, [n, node("api-b")])[0]:
                            continue  # a combination check.py rejects is never drawn
                        m = build.normalize([n])[0]
                        if not build.hidden(cfg, m, owner_done=True):  # the overview draws decided owner calls
                            drawable.add(build.node_class(m, build.kind_of(cfg, m)))
        return drawable

    def test_legend_uses_the_diagram_classdefs_verbatim(self):
        lg, ov = self.legend(), mermaid_blocks(region(built(GOOD), "overview"))[0]
        for line in build.classdef_lines():
            self.assertIn(line, lg.splitlines())
            self.assertIn(line, ov.splitlines())

    def test_every_class_a_tree_can_draw_has_a_sample(self):
        drawable = self.drawable_classes(CFG)
        shown = set(self.classes_in(self.legend()))
        self.assertLessEqual(drawable, shown)
        self.assertNotIn("gate", drawable)  # an open owner gate is drawn nowhere
        self.assertIn("decided", drawable)  # a decided one is drawn in the overview
        self.assertIn("finding", drawable)
        self.assertLessEqual(shown, {name for name, _ in build.CLASSDEFS})

    def test_legend_follows_the_config_for_a_project_without_unrated_kinds(self):
        lib = tree_config.load(LIBRARY / "tech-tree.yaml")
        drawable = self.drawable_classes(lib)
        lg = self.legend(lib)
        shown = self.classes_in(lg)
        self.assertLessEqual(drawable, set(shown))
        self.assertNotIn("finding", drawable)
        self.assertNotIn("finding", shown)
        self.assertIn("gate", shown)  # the owner-gate kind still has its shape samples
        self.assertIn("decided", shown)

    def test_one_shape_sample_per_configured_kind(self):
        for cfg in (CFG, tree_config.load(LIBRARY / "tech-tree.yaml")):
            lg = self.legend(cfg)
            samples = build.legend_shapes(cfg)
            gates = sum(1 for k in cfg.kinds.values() if k["owner_gate"])
            self.assertEqual(len(samples), len(cfg.kinds) + gates)  # a decided sample beside each owner-gate kind
            expected = [(n, k) for n, k in cfg.kinds.items() for _ in range(1 + k["owner_gate"])]
            for j, ((cls, s, attrs), (name, k)) in enumerate(zip(samples, expected)):
                self.assertIn(name, s["title"])
                self.assertIn(build.node_def(s, attrs, ident=f"lg_k{j}"), lg)
                self.assertIn(f"lg_k{j}", self.classes_in(lg)[cls])
                self.assertEqual(attrs, k)

    def test_each_status_sample_is_drawn_by_node_def_with_its_class(self):
        lg = self.legend()
        cls = self.classes_in(lg)
        k = 0
        for _, samples in build.legend_status(CFG):
            for want, s, attrs in samples:
                self.assertEqual(build.node_class(s, attrs), want)
                self.assertIn(build.node_def(s, attrs, ident=f"lg_s{k}"), lg)
                self.assertIn(f"lg_s{k}", cls[want])
                k += 1

    def test_arrow_kinds_are_shown(self):
        lg = self.legend()
        self.assertEqual({k for k, _, _ in build.LEGEND_EDGES}, set(build.EDGE_ARROWS))
        for j, (kind, _, _) in enumerate(build.LEGEND_EDGES):
            self.assertIn(f"lg_e{j}a {build.EDGE_ARROWS[kind]} lg_e{j}b", lg)
        # the qualified-by sample carries the trees' linkStyle at its own link index (the row ties come first)
        q = [j for j, (kind, _, _) in enumerate(build.LEGEND_EDGES) if kind == "qual"]
        lines = [x.strip() for x in lg.splitlines() if re.search(r" (-->|-\.->|~~~) ", x)]
        ties = lines[0].count("~~~")
        self.assertEqual(ties, len(build.legend_status(CFG)) + 1)  # status rows, shapes, arrows
        self.assertIn(f"linkStyle {ties + q[0]} {build.QUAL_STYLE}", lg)
        self.assertEqual(lines[1 + q[0]], f"lg_e{q[0]}a -.-> lg_e{q[0]}b")


class LayoutAndWarningTests(unittest.TestCase):
    def test_layout_defaults_and_validation(self):
        lib = tree_config.load(LIBRARY / "tech-tree.yaml")
        self.assertEqual(lib.layout, {"overview_width": 3, "track_width": 3, "band_min": 1})
        cfg = make_cfg(HERE / "unit", layout={"overview_width": 2})
        self.assertEqual(cfg.layout, {"overview_width": 2, "track_width": 3, "band_min": 1})
        for bad in ({"overview_width": 0}, {"track_width": "3"}, {"band_min": True}, {"colour": 1}):
            with self.assertRaises(tree_config.ConfigError):
                make_cfg(HERE / "unit", layout=bad)

    def test_the_example_config_validates(self):
        cfg = tree_config.load(SCRIPTS.parent / "config.example.yaml")
        self.assertEqual(cfg.layout, {"overview_width": 3, "track_width": 3, "band_min": 1})

    def test_layout_changes_the_diagrams(self):
        nodes = [node(f"api-{c}", state="done", outcome="unrated", evidence=["e"], start=f"2026-0{1 + i}-01")
                 for i, c in enumerate("abcdef")]
        merged = make_cfg(HERE / "unit", layout={"band_min": 4})
        one = region(built(nodes), "overview")
        many = region(built(nodes, cfg=merged), "overview")
        self.assertEqual(one.count("subgraph bd_"), 6)  # band_min 1: one band per month
        self.assertLess(many.count("subgraph bd_"), 6)
        narrow = region(built(nodes, cfg=make_cfg(HERE / "unit", layout={"overview_width": 1})), "overview")
        rows = {}
        for i, k in row_levels(narrow).items():
            rows.setdefault(k, []).append(i)
        self.assertEqual(max(map(len, rows.values())), 1)

    def test_done_by_month_lines_carry_plain_or_headline_and_gap_months_are_named(self):
        nodes = [node("api-a", state="done", start="2026-01-05", end="2026-01-09", plain="Plain one."),
                 node("api-b", state="done", start="2026-01-06", end="2026-01-10", headline="12 ms"),
                 node("api-c", state="abandoned", outcome="dead-end", start="2026-04-02", end="2026-04-03", plain="Dropped it."),
                 node("api-d", state="done", start="2026-04-04", end="2026-04-05")]
        out = region(built(nodes), "done-by-month")
        self.assertIn("- `api-a`: T api-a (\u2714 done \u00b7 unrated). Plain one.", out)
        self.assertIn("- `api-b`: T api-b (\u2714 done \u00b7 unrated). 12 ms", out)
        self.assertIn("- `api-c`: T api-c (\u2716 dead end). Dropped it.", out)
        self.assertIn("- `api-d`: T api-d (\u2714 done \u00b7 unrated)\n", out + "\n")
        self.assertIn("<b>Feb 2026 to Mar 2026</b>: nothing finished", out)  # a quiet run is one line
        self.assertEqual(out.count("nothing finished"), 1)
        self.assertNotIn("<details><summary><b>Feb", out)
        order = [out.index(m) for m in ("Apr 2026", "Feb 2026 to Mar 2026", "Jan 2026")]
        self.assertEqual(order, sorted(order))
        self.assertNotIn("May 2026", out)  # only months between the first and the last shown

    def test_months_without_an_overview_node_are_one_warning(self):
        nodes = [node("api-a", state="done", start="2020-01-05", overview=False),
                 node("api-b", state="done", start="2020-01-09", overview=False),
                 node("api-c", state="done", start="2020-02-01", overview=True, plain="p"),
                 node("api-d", state="next", start="2020-03-01", overview=False),
                 node("api-e", state="done", overview=False)]
        warns = [w for w in check.validate_nodes(CFG, nodes)[1] if w.startswith("overview: months")]
        self.assertEqual(warns, ["overview: months with finished work but no overview node (pick their turning points): Jan 2020 (2)"])
        this_month = datetime.date.today().isoformat()[:7] + "-01"
        cur = [node("api-a", state="done", start=this_month, overview=False)]
        self.assertFalse([w for w in check.validate_nodes(CFG, cur)[1] if w.startswith("overview: months")])

    def test_title_identifier_heuristic(self):
        for title, token in (("Fix parse_config now", "parse_config"), ("Set MAX_UPLOAD_MB default", "MAX_UPLOAD_MB"),
                             ("Rewrite src/core/mod", "src/core/mod"), ("Move lib.rs out", "lib.rs"),
                             ("Merge feat-new-cache branch", "feat-new-cache"), ("Update app.py", "app.py")):
            self.assertEqual(check.title_identifier(title), token, title)
            w = check.validate_nodes(CFG, [node("api-a", title=title, plain="p")])[1]
            self.assertIn(f"api-a: title looks like an identifier ('{token}'); use plain words, and explain real names in 'plain'", w)
        for title in ("Per-key rate limiting", "GitHub login via OAuth", "p99 latency target", "zstd compression",
                      "Blue-green deploys", "Plain words only", "Version 2.0 release", "Upgrade to Node.js 22",
                      "Move to Next.js", "Add end-to-end tests", "Keep docs up-to-date", "Use copy-on-write snapshots",
                      "A/B test rollout", "Read and/or write"):
            self.assertIsNone(check.title_identifier(title), title)
            self.assertFalse([w for w in check.validate_nodes(CFG, [node("api-a", title=title, plain="p")])[1]
                              if "looks like an identifier" in w], title)


# ---------------------------------------------------------------- generated pages
class TrackFileTests(Files, unittest.TestCase):
    NODES = GOOD + [node("data-x", state="done", outcome="successful", verdict_quote="q", evidence=["e"], notes="n")]

    def outputs(self, nodes=None, text=None, cfg=None):
        cfg = cfg or self.cfg
        return build.render_outputs(cfg, nodes or self.NODES, text or skeleton(cfg), TODAY)

    def test_main_page_is_light(self):
        main = self.outputs()[self.cfg.page_path]
        self.assertEqual(main.count("```mermaid"), 2)  # the legend and the overview; no gantt
        self.assertLessEqual(main.count("```mermaid"), check.MAX_MERMAID_PER_FILE)
        self.assertNotIn("gantt", main)
        self.assertIn("(roadmap/timeline.md)", main)
        self.assertIn("Generated on " + TODAY + ".", main)

    def test_timeline_page_has_one_block(self):
        tl = self.outputs()[self.cfg.timeline_path]
        self.assertEqual(tl.count("```mermaid"), 1)
        self.assertIn("gantt", tl)

    def test_labels_are_short(self):
        outs = self.outputs([node(f"{c}-a", state="done", outcome="successful", evidence=["e"], start="2026-01-01",
                                  notes="n") for c in self.cfg.tracks] + [node("api-z", depends_on=["data-a"])])
        for key, text in outs.items():
            for block in tree_blocks(text):
                for sentence in ("done ·", "in progress", "blocked", "superseded", "dead end", "qualified by", "|then|"):
                    self.assertNotIn(sentence, block.split("classDef")[0], key)

    def test_each_track_file_has_one_block_and_a_nodes_banner(self):
        outs = self.outputs([node(f"{c}-a") for c in self.cfg.tracks])  # every track non-empty
        self.assertEqual(len(outs), 2 + len(self.cfg.tracks))
        for code, (name, _) in self.cfg.tracks.items():
            text = outs[self.cfg.track_file(code)]
            self.assertEqual(text.count("```mermaid"), 1, code)
            self.assertTrue(text.startswith("<!-- Generated by build.py from nodes.yaml. Do not edit. -->"))
            self.assertIn(f"# {code} - {name}", text)
            self.assertIn("(../../roadmap.md)", text)

    def test_paths_and_links_follow_the_page_and_nodes_paths(self):
        cfg = make_cfg(self.dir, page="docs/plan.md", nodes="meta/nodes.yaml")
        outs = self.outputs(cfg=cfg)
        self.assertEqual(sorted(p.relative_to(self.dir).as_posix() for p in outs),
                         sorted(["docs/plan.md", "docs/plan/timeline.md"] + [f"docs/plan/tracks/{c}.md" for c in cfg.tracks]))
        api = outs[cfg.track_file("api")]
        self.assertIn("from ../meta/nodes.yaml.", api)
        self.assertIn("(../../plan.md)", api)
        self.assertIn("(../plan.md)", outs[cfg.timeline_path])
        self.assertIn("(plan/timeline.md)", region(outs[cfg.page_path], "timeline"))
        self.assertIn("(plan/tracks/api.md)", region(outs[cfg.page_path], "tracks"))

    def test_tracks_region_links_every_track_file(self):
        reg = region(self.outputs()[self.cfg.page_path], "tracks")
        for code, (name, _) in self.cfg.tracks.items():
            self.assertIn(f"[{code} - {name}](roadmap/tracks/{code}.md)", reg)
        self.assertIn("[api - API](roadmap/tracks/api.md): 2 nodes (1 done, 1 in progress)", reg)
        self.assertIn("[ops - operations](roadmap/tracks/ops.md): 0 nodes", reg)

    def test_empty_track_file(self):
        self.assertIn("_No nodes in this track yet._", self.outputs()[self.cfg.track_file("ops")])

    def test_rebuild_is_idempotent_on_disk(self):
        self.write_config()
        self.write_nodes(self.NODES)
        self.write_page(skeleton())
        args = ["--config", str(self.dir / "tech-tree.yaml"), "--today", TODAY]
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            self.assertEqual(build.main(args), 0)
            self.assertEqual(build.main(args), 0)
        self.assertIn("updated", buf.getvalue().splitlines()[0])
        self.assertIn("already up to date", buf.getvalue().splitlines()[1])
        self.assertTrue(self.cfg.track_file("api").exists())

    def test_check_flags_stale_missing_and_extra_track_files(self):
        self.write_config()
        self.write_nodes(self.NODES)
        self.write_page(skeleton())
        quiet(build.main, ["--config", str(self.dir / "tech-tree.yaml"), "--today", TODAY])
        self.assertEqual(check.run(self.cfg)[0], [])
        api = self.cfg.track_file("api")
        api.write_text(api.read_text(encoding="utf-8").replace("T api-a", "T api-X"), encoding="utf-8")
        self.cfg.track_file("data").unlink()
        (self.cfg.tracks_dir / "gone.md").write_text("x", encoding="utf-8")
        errs = "\n".join(check.run(self.cfg)[0])
        self.assertIn("api.md: generated track file is stale", errs)
        self.assertIn("data.md: track file missing", errs)
        self.assertIn("gone.md: no such track", errs)
        quiet(build.main, ["--config", str(self.dir / "tech-tree.yaml"), "--today", TODAY])  # build repairs all three
        self.assertEqual(check.run(self.cfg)[0], [])
        self.assertFalse((self.cfg.tracks_dir / "gone.md").exists())

    def test_check_flags_stale_timeline_and_old_date(self):
        self.write_config()
        self.write_nodes([node("api-a", start="2026-01-01")])
        self.write_page(skeleton())
        quiet(build.main, ["--config", str(self.dir / "tech-tree.yaml"), "--today", "2026-01-02"])
        errs, warns = check.run(self.cfg)
        self.assertEqual(errs, [])
        self.assertTrue(any("open timeline bars end at 2026-01-02" in w for w in warns), warns)  # not the real today
        tl = self.cfg.timeline_path
        tl.write_text(tl.read_text(encoding="utf-8").replace("api_a", "api_X"), encoding="utf-8")
        self.assertTrue(any("timeline file is stale" in e for e in check.run(self.cfg)[0]))
        tl.unlink()
        self.assertTrue(any("timeline file missing" in e for e in check.run(self.cfg)[0]))

    def test_mermaid_cap(self):
        block = "```mermaid\n"
        self.assertEqual(check.mermaid_errors({"a.md": block * check.MAX_MERMAID_PER_FILE}), [])
        self.assertEqual(len(check.mermaid_errors({"b.md": block * (check.MAX_MERMAID_PER_FILE + 1)})), 1)


class StaleTests(Files, unittest.TestCase):
    def test_fresh_passes_and_stale_fails(self):
        md = built(GOOD)
        self.assertEqual(self.run_check(GOOD, md)[0], [])
        changed = copy.deepcopy(GOOD)
        changed[0]["title"] = "Renamed"
        errs, _ = self.run_check(changed, md)
        self.assertTrue(any("stale" in e and "overview" in e for e in errs), errs)

    def test_hand_edit_inside_region_is_stale(self):
        errs, _ = self.run_check(GOOD, built(GOOD).replace("T api-a", "T api-X", 1))
        self.assertTrue(any("stale" in e for e in errs), errs)

    def test_hand_text_edit_is_not_stale(self):
        self.assertEqual(self.run_check(GOOD, built(GOOD).replace("HAND-WRITTEN", "edited by hand"))[0], [])

    def test_missing_region(self):
        errs, _ = self.run_check(GOOD, built(GOOD).replace("<!-- BEGIN GENERATED: legend -->", ""))
        self.assertTrue(any("legend" in e for e in errs), errs)

    def test_missing_queue_region_when_a_queue_lane_is_configured(self):
        md = built(GOOD).replace("<!-- BEGIN GENERATED: queue -->", "")
        errs, _ = self.run_check(GOOD, md)
        self.assertTrue(any("queue" in e for e in errs), errs)

    def test_cli_exit_codes(self):
        self.write_config()
        self.write_nodes(GOOD)
        self.write_page(skeleton())
        args = ["--config", str(self.dir / "tech-tree.yaml")]
        self.assertEqual(quiet(check.main, args), 1)  # regions empty -> stale
        self.assertEqual(quiet(build.main, args + ["--today", TODAY]), 0)
        self.assertEqual(quiet(check.main, args), 0)
        self.write_nodes([node("api-a", depends_on=["api-ghost"])])
        self.assertEqual(quiet(build.main, args), 1)  # invalid nodes: build refuses

    def test_default_config_is_read_from_the_working_directory(self):
        self.write_config()
        self.write_nodes(GOOD)
        self.write_page(skeleton())
        code, out, err = run_script("build.py", "--today", TODAY, cwd=self.dir)
        self.assertEqual(code, 0, err)
        code, out, err = run_script("check.py", cwd=self.dir)
        self.assertEqual(code, 0, err)
        self.assertIn("check: OK", out)

    def test_bad_today(self):
        self.write_config()
        self.assertEqual(run_script("build.py", "--config", self.dir / "tech-tree.yaml", "--today", "yesterday")[0], 2)

    def test_top_level_forms(self):
        n = self.dir / "nodes.yaml"
        for doc in (GOOD, {"nodes": GOOD}):
            n.write_text(yaml.safe_dump(doc, allow_unicode=True), encoding="utf-8")
            self.assertEqual(len(build.load_nodes(n)), 3)
        for doc in ({"nodes": GOOD, "bogus": 1}, {"nodes": GOOD, "dropped": []}, {"nodes": "x"}):
            n.write_text(yaml.safe_dump(doc), encoding="utf-8")
            with self.assertRaises(ValueError):
                build.load_nodes(n)

    def test_dict_form_end_to_end(self):
        n = self.dir / "nodes.yaml"
        n.write_text(yaml.safe_dump({"nodes": GOOD}, allow_unicode=True), encoding="utf-8")
        self.write_page(built(GOOD))
        self.write_generated(GOOD)
        self.assertEqual(check.run(self.cfg)[0], [])

    def test_bad_yaml(self):
        self.cfg.nodes_path.write_text("- id: [unclosed\n", encoding="utf-8")
        self.assertTrue(check.run(self.cfg)[0])


# ---------------------------------------------------------------- display coverage
class DisplayCoverageTests(unittest.TestCase):
    """Every text field of a node reaches a generated page; a field with no renderer fails here."""

    def sentinel_nodes(self):
        """One node with a value in every recorded field, and one target node in each other track."""
        n = node("api-sentinel", kind="experiment", state="in-progress", lane=CFG.queue_lane, queue_order=7,
                 start="2026-01-01", end="2026-02-02", confirmed="2026-01-05", depends_on=["web-target"],
                 replaced_by=["data-target"], qualified_by=["ops-target"])
        for f in check.TEXT_FIELDS:
            n[f] = f"SENT_{f}"
        n["evidence"] = ["SENT_evidence"]
        n["folded"] = ["SENT_folded (2026-01-02, commit abcdef12)", "SENT_folded_bare"]
        return [n, node("web-target"), node("data-target"), node("ops-target")]

    def test_every_schema_field_is_covered_or_exempt(self):
        fields = set(check.FIELDS) | set(check.OPTIONAL)
        covered = (set(check.TEXT_FIELDS) | set(check.TEXT_LIST_FIELDS) | set(check.ID_LIST_FIELDS)
                   | set(check.SCALAR_FIELDS) | set(check.IMPLICIT_FIELDS))
        self.assertEqual(fields - covered, set(check.COVERAGE_EXEMPT),
                         "a new node field must be checked by coverage_errors (and shown on a page), or be exempt with a reason")
        self.assertEqual(set(check.COVERAGE_EXEMPT), {"overview", "diagram"})
        self.assertFalse(covered - fields)

    def test_each_sentinel_appears_where_it_is_checked(self):
        nodes = self.sentinel_nodes()
        self.assertEqual(check.validate_nodes(CFG, nodes)[0], [])
        outs = build.render_outputs(CFG, nodes, skeleton(), TODAY)
        blob = "\n".join(outs.values())
        for f in check.TEXT_FIELDS + ["evidence"]:
            self.assertIn(f"SENT_{f}", blob, f"field {f} reaches no generated page")
        self.assertIn("SENT_folded_bare", blob)
        # ids and scalars are in the source node's own track page, even when the target is in another track
        api = outs[CFG.track_file("api")]
        for target in ("web-target", "data-target", "ops-target", "experiment", CFG.queue_lane, "2026-02-02", "2026-01-05"):
            self.assertIn(target, "\n".join(ln for ln in api.splitlines() if ln.startswith("| `api-sentinel` |")), target)
        self.assertEqual(check.coverage_errors(CFG, nodes, TODAY), [])

    def test_coverage_check_reports_a_field_no_renderer_shows(self):
        silent = lambda cfg, code: (lambda nodes, today: "nothing")  # noqa: E731
        with mock.patch.object(build, "r_table", silent):
            errs = check.coverage_errors(CFG, self.sentinel_nodes(), TODAY)
        for expect in ("api-sentinel: notes appears on no generated page",
                       "api-sentinel: folded item 2 appears on no generated page",
                       "api-sentinel: replaced_by target data-target is not in the node's table row",
                       "api-sentinel: qualified_by target ops-target is not in the node's table row",
                       "api-sentinel: lane value staging is not in the node's table row",
                       "api-sentinel: confirmed value 2026-01-05 is not in the node's table row",
                       "api-sentinel: queue_order value 7 is not in the node's table row"):
            self.assertIn(expect, errs)

    def test_coverage_uses_the_tables_cell_normalization(self):
        n = node("api-a", notes="two  words | and\na pipe", plain="x")
        self.assertEqual(check.coverage_errors(CFG, [n], TODAY), [])

    def test_check_run_fails_on_stale_free_but_undisplayed_text(self):
        with tempfile.TemporaryDirectory() as d:
            cfg = make_cfg(d)
            (Path(d) / "tech-tree.yaml").write_text(yaml.safe_dump(RAW, sort_keys=False), encoding="utf-8")
            cfg.nodes_path.write_text(yaml.safe_dump([node("api-a", notes="N1", plain="p")]), encoding="utf-8")
            cfg.page_path.write_text(skeleton(cfg), encoding="utf-8", newline="")
            self.assertEqual(quiet(build.main, ["--config", str(cfg.path), "--today", TODAY]), 0)
            silent = lambda c, code: (lambda nodes, today: "nothing")  # noqa: E731
            with mock.patch.object(build, "r_table", silent):
                errs, _ = check.run(cfg)
            self.assertTrue(any("notes appears on no generated page" in e for e in errs), errs)


# ---------------------------------------------------------------- fixtures
class FixtureTests(unittest.TestCase):
    """The committed fixture projects are commit-ready: check passes and a rebuild changes no byte."""

    def copy_fixture(self, name, dest):
        shutil.copytree(HERE / name, dest, ignore=shutil.ignore_patterns("__pycache__"))

    def test_clean_and_library_pass_check_and_rebuild_changes_nothing(self):
        for name in ("clean", "library"):
            with self.subTest(name), tempfile.TemporaryDirectory() as d:
                dest = Path(d) / name
                self.copy_fixture(name, dest)
                cfgfile = dest / "tech-tree.yaml"
                code, out, err = run_script("check.py", "--config", HERE / name / "tech-tree.yaml")
                self.assertEqual(code, 0, out + err)
                self.assertNotIn("overview has", out) if name == "library" else None
                before = {p.relative_to(dest): p.read_bytes() for p in dest.rglob("*") if p.is_file()}
                code, out, err = run_script("build.py", "--config", cfgfile, "--today", FIXTURE_TODAY)
                self.assertEqual(code, 0, err)
                self.assertIn("already up to date", out)
                after = {p.relative_to(dest): p.read_bytes() for p in dest.rglob("*") if p.is_file()}
                self.assertEqual(before, after)
                for p in after:  # LF line endings
                    self.assertNotIn(b"\r", after[p], str(p))

    def test_clean_covers_every_state_outcome_and_kind(self):
        cfg = tree_config.load(CLEAN / "tech-tree.yaml")
        nodes = build.load_nodes(cfg.nodes_path)
        self.assertEqual({n["state"] for n in nodes}, set(build.STATES))
        self.assertEqual({n["outcome"] for n in nodes}, set(build.OUTCOMES))
        self.assertEqual({n["kind"] for n in nodes}, set(cfg.kinds))
        self.assertEqual({n["track"] for n in nodes}, set(cfg.tracks))
        self.assertTrue(any(n.get("queue_order") for n in nodes))
        self.assertTrue(any(n.get("qualified_by") for n in nodes))
        self.assertTrue(any(n["replaced_by"] for n in nodes))
        self.assertTrue(any(n.get("waiting_on") for n in nodes))
        self.assertTrue(any(n.get("confirmed") for n in nodes))
        self.assertTrue(any(build.labelled(f) for n in nodes for f in n["folded"]))
        self.assertTrue(12 <= sum(1 for n in nodes if n["overview"]) <= 30)

    def test_library_needs_no_queue_and_no_special_lanes(self):
        cfg = tree_config.load(LIBRARY / "tech-tree.yaml")
        page = (LIBRARY / "roadmap.md").read_text(encoding="utf-8")
        self.assertNotIn("GENERATED: queue", page)
        self.assertEqual(cfg.lanes, ["local", "owner", "none"])
        self.assertEqual(len(build.load_nodes(cfg.nodes_path)), 6)


class BrokenTests(unittest.TestCase):
    EXPECTED = [
        "api-dup: duplicate id",
        "API_bad: id must be kebab-case",
        "data-wrongtrack: id must start with its track code 'web-'",
        "api-badtrack: track 'zzz' not one of ['api', 'web', 'data', 'ops']",
        "api-badkind: kind 'gadget' not one of",
        "api-badlane: lane 'moon' not one of ['local', 'staging', 'owner', 'none']",
        "api-badstate: state 'weird' not one of",
        "api-badoutcome: outcome 'great' not one of",
        "api-nofield: missing field 'lane'",
        "api-unknownfield: unknown field 'colour'",
        "api-emptytitle: title is empty",
        "api-badflags: diagram must be true or false",
        "api-badflags: overview must be true or false",
        "api-noquote: outcome successful needs a verdict_quote",
        "data-na-wrong: outcome n/a is only for owner-gate kinds",
        "data-defect-rated: kind 'defect' is not rated (rated: false) and never carries an outcome (got successful)",
        "ops-no-evidence: state done needs at least one evidence entry",
        "ops-super-noreplace: outcome superseded needs a non-empty replaced_by",
        "ops-abandoned-wrong: state abandoned needs outcome dead-end or superseded (got successful)",
        "api-dangling: dangling depends_on target 'api-ghost'",
        "cycle in depends_on/replaced_by: web-cyc-a -> web-cyc-b -> web-cyc-a",
        "web-self: depends_on points at itself",
        "data-notready: state next but depends_on 'data-open-prereq' is in-progress, not done",
        "ops-endbefore: end is before start",
        "ops-endnostart: end set without start",
        "ops-badconfirm: confirmed:",
        "api-longquote: verdict_quote longer than 300 chars",
        "ops-queue-str: queue_order must be an integer",
        "ops-queue-b: queue_order 1 already used by ops-queue-a",
        "WARNING: api-longtitle: title longer than 6 words",
        "WARNING: api-late-dep: prerequisite 'api-late-pre' starts after it",
        "WARNING: data-qorder-warn: queue_order on a node that is not a forward node in the queue lane",
        "WARNING: 1 done node is unrated (no quoted verdict yet): ops-done-unrated",
        "WARNING: web-waiting-warn: waiting_on on a node that is not in-progress",
        "WARNING: web-qual-warn: qualified_by on a node that is not done",
        "WARNING: web-noplain: overview/forward node without a 'plain' sentence",
        "WARNING: 4 rated nodes are not confirmed by the owner",
        "WARNING: overview has 1 nodes (target 12-30)",
    ]

    def test_every_planted_error_is_reported_and_exit_code_is_1(self):
        code, out, err = run_script("check.py", "--config", BROKEN / "tech-tree.yaml")
        text = out + err
        self.assertEqual(code, 1, text)
        for msg in self.EXPECTED:
            self.assertIn(msg, text)
        self.assertIn("check: FAILED", err)
        self.assertEqual(text.count("not confirmed by the owner"), 1)  # one aggregate warning, not one per node
        self.assertNotIn("ERROR: api-late", text)  # the warning-only nodes raise no error
        self.assertNotIn("ERROR: api-ok", text)

    def test_queue_order_without_a_queue_lane_is_an_error(self):
        code, out, err = run_script("check.py", "--config", BROKEN / "noqueue" / "tech-tree.yaml")
        self.assertEqual(code, 1)
        self.assertIn("api-queue-no-lane: queue_order set, but the config has no queue_lane", err)
        self.assertNotIn("overview has", out)  # no overview bounds configured

    def test_build_refuses_invalid_nodes(self):
        code, out, err = run_script("build.py", "--config", BROKEN / "tech-tree.yaml")
        self.assertEqual(code, 1)
        self.assertIn("the nodes file is invalid", err)


# ---------------------------------------------------------------- project neutrality
class NeutralityTests(unittest.TestCase):
    def test_no_track_kind_or_lane_name_is_a_literal_in_the_script_code(self):
        names = set()
        for d in (CLEAN, LIBRARY):
            cfg = tree_config.load(d / "tech-tree.yaml")
            names |= set(cfg.tracks) | set(cfg.kinds) | set(cfg.lanes) | {cfg.queue_lane}
        names.discard(None)
        for script in ("tree_config.py", "build.py", "check.py", "history_gap.py", "validate_records.py", "transcript_digest.py"):
            text = (SCRIPTS / script).read_text(encoding="utf-8")
            # the history scripts' own "none" is a match bucket, not a lane
            for name in sorted(names - ({"none"} if script in ("history_gap.py", "validate_records.py") else set())):
                self.assertIsNone(re.search(r"""["']""" + re.escape(name) + r"""["']""", text),
                                  f"{script} has the literal {name!r}")

    def test_scripts_use_lf_line_endings(self):
        for p in SCRIPTS.glob("*.py"):
            self.assertNotIn(b"\r", p.read_bytes(), p.name)


class FirstBuildTests(unittest.TestCase):
    def test_missing_page_gets_a_skeleton_that_then_checks_clean(self):
        with tempfile.TemporaryDirectory() as d:
            for name in ("tech-tree.yaml", "nodes.yaml"):
                shutil.copy(HERE / "library" / name, Path(d) / name)
            code, out = run_script("build.py", "--config", Path(d) / "tech-tree.yaml")[:2]
            self.assertEqual(code, 0, out)
            page = (Path(d) / "roadmap.md").read_text(encoding="utf-8")
            for r in build.region_order(tree_config.load(Path(d) / "tech-tree.yaml")):
                self.assertEqual(page.count(f"<!-- BEGIN GENERATED: {r} -->"), 1, r)
            self.assertEqual(run_script("check.py", "--config", Path(d) / "tech-tree.yaml")[0], 0)


if __name__ == "__main__":
    unittest.main()
