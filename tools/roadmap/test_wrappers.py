#!/usr/bin/env python3
"""The roadmap commands keep working on the vendored skill: config loads, pages are fresh, CI contract holds.

Run: python tools/roadmap/test_wrappers.py (the skill's own tests: python -m unittest discover -s tools/roadmap/skill/scripts/tests)
"""
import subprocess
import sys
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
sys.path.insert(0, str(HERE / "skill" / "scripts"))
sys.path.insert(0, str(HERE / "skill" / "scripts" / "tests"))
import tree_config  # noqa: E402
import test_tree  # noqa: E402  (its edge-direction helpers)


def run(script, *args):
    return subprocess.run([sys.executable, str(HERE / script), *args], capture_output=True, text=True,
                          encoding="utf-8", cwd=ROOT)


class Wrappers(unittest.TestCase):
    def test_config_keeps_the_raptorpath_mapping(self):
        c = tree_config.load(ROOT / "docs" / "roadmap" / "tech-tree.yaml")
        self.assertEqual(c.queue_lane, "vm")
        self.assertEqual(c.overview, {"min": 50, "max": 60})
        self.assertEqual(c.layout, {"overview_width": 3, "track_width": 3, "band_min": 1})
        self.assertTrue(c.kinds["user-decision"]["owner_gate"])
        self.assertEqual(c.kinds["user-decision"]["shape"], "hexagon")
        self.assertFalse(c.kinds["defect-verdict"]["rated"])
        self.assertFalse(c.kinds["defect-verdict"]["blocks"])
        self.assertEqual(len(c.tracks), 11)

    def test_check_prints_the_ok_line_ci_greps_for(self):
        r = run("check.py")
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertRegex(r.stdout, r"(?m)^check: OK")

    def test_every_edge_of_the_real_diagrams_goes_down(self):
        # the skill's suite checks this on its fixtures; here it is the committed raptorpath pages
        pages = [ROOT / "docs" / "roadmap.md", *sorted((ROOT / "docs" / "roadmap" / "tracks").glob("*.md"))]
        seen = 0
        for p in pages:
            for block in test_tree.tree_blocks(p.read_text(encoding="utf-8")):
                seen += 1
                self.assertEqual(test_tree.backward_edges(block), [], p.name)
        self.assertEqual(seen, 12)  # the overview and 11 tracks


if __name__ == "__main__":
    unittest.main()
