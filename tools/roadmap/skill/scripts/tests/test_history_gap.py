"""Tests for history_gap.py: the matching rules (unit) and a run over a tiny git repository (end to end).

Run from the repository root: python -m unittest discover -s skills/tech-tree/scripts/tests
"""
import csv
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

import yaml

HERE = Path(__file__).resolve().parent
SCRIPTS = HERE.parent
sys.path.insert(0, str(SCRIPTS))
sys.path.insert(0, str(HERE))
import history_gap as hg  # noqa: E402
import tree_config  # noqa: E402
from gitfixture import GitRepo  # noqa: E402

WEAK = {"dec": r"DEC[- ]?0*(\d{1,4})", "pr": r"#(\d+)\b"}


def node(i, **kw):
    n = {"id": i, "evidence": [], "notes": "", "verdict_quote": "", "headline": "", "folded": []}
    n.update(kw)
    return n


def commit(sha, subject="s", body=""):
    return {"sha": sha, "subject": subject, "body": body}


class ShaTokens(unittest.TestCase):
    def test_commit_prefix_with_only_digits_or_only_letters_is_a_sha(self):
        toks = hg.tokens("commit 34449977, commit cbbbfdbd and commit 0a55b65c")
        for s in ("34449977", "cbbbfdbd", "0a55b65c"):
            self.assertIn(("sha", s), toks)

    def test_bare_digit_run_is_still_not_a_sha(self):
        self.assertFalse([t for t in hg.tokens("measured 34449977 packets") if t[0] == "sha"])

    def test_folded_all_digit_sha_matches_its_commit(self):
        idx = hg.Index([node("api-a", folded=["Pace all traffic (2026-07-12, commit 34449977)"])])
        b, ids, vias, strong = hg.classify(commit("3444997717566"), {}, idx)
        self.assertEqual((b, ids, vias, strong), ("folded", ["api-a"], ["sha"], "folded"))


class WeakRefs(unittest.TestCase):
    def test_weak_refs_come_from_the_config_and_normalize_numbers(self):
        refs = hg.compile_refs(WEAK)
        toks = hg.tokens("see DEC-0069 and PR #42, DEC 7", refs)
        self.assertEqual({t for t in toks if t[0] != "sha"}, {("dec", "69"), ("dec", "7"), ("pr", "42")})
        self.assertEqual(hg.tokens("DEC-0069", {}), set())  # no config, no weak refs

    def test_a_pattern_with_text_keys_lowercases(self):
        toks = hg.tokens("closes JIRA-12", hg.compile_refs({"ticket": r"(JIRA-\d+)"}))
        self.assertIn(("ticket", "jira-12"), toks)


class StrongBucket(unittest.TestCase):
    def test_weak_node_match_does_not_hide_strong_folded_match(self):
        # the commit names DEC 5 (weak, matches api-a's evidence) and is folded by sha in api-b
        idx = hg.Index([node("api-a", evidence=["DEC-5"]), node("api-b", folded=["commit 773ca1c3"])], WEAK)
        b, ids, vias, strong = hg.classify(commit("773ca1c3aa", "DEC-5: baseline"), {}, idx)
        self.assertEqual((b, vias), ("node", ["dec"]))
        self.assertEqual(strong, "folded")

    def test_weak_only_match_is_strong_none(self):
        idx = hg.Index([node("api-a", evidence=["DEC-0069"])], WEAK)
        b, _, vias, strong = hg.classify(commit("abcdef1234", "per DEC-0069"), {}, idx)
        self.assertEqual((b, vias, strong), ("node", ["dec"], "none"))

    def test_without_configured_refs_a_weak_mention_matches_nothing(self):
        idx = hg.Index([node("api-a", evidence=["DEC-0069"])])
        self.assertEqual(hg.classify(commit("abcdef1234", "per DEC-0069"), {}, idx)[0], "none")

    def test_strong_node_match_wins(self):
        idx = hg.Index([node("api-a", evidence=["commit abcdef12"]), node("api-b", folded=["commit abcdef12"])])
        self.assertEqual(hg.classify(commit("abcdef1234"), {}, idx)[3], "node")

    def test_branch_names_match_whole_items_only(self):
        idx = hg.Index([node("api-a", folded=["branch feature-x"])])
        self.assertEqual(hg.classify(commit("1" * 12), {"1" * 12: {"feature-x"}}, idx)[:3], ("folded", ["api-a"], ["branch"]))
        self.assertEqual(hg.classify(commit("1" * 12), {"1" * 12: {"feature-y"}}, idx)[0], "none")


class MergeSubjects(unittest.TestCase):
    def test_git_and_github_subjects(self):
        self.assertEqual(hg.merge_branch("Merge branch 'feature-x'"), "feature-x")
        self.assertEqual(hg.merge_branch("Merge branch 'feature-x' into main"), "feature-x")
        self.assertEqual(hg.merge_branch("Merge remote-tracking branch 'origin/hotfix-1'"), "hotfix-1")
        self.assertEqual(hg.merge_branch("Merge pull request #12 from acme/feat/Shiny"), "feat/shiny")
        self.assertIsNone(hg.merge_branch("Update the readme"))

    def test_configured_subject(self):
        self.assertEqual(hg.merge_branch("land: topic-9 (done)", r"^land: (\S+)"), "topic-9")
        self.assertIsNone(hg.merge_branch("land: topic-9"))


class EndToEnd(unittest.TestCase):
    """One repository, built once (git is slow on some machines); the tests only read it."""

    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.TemporaryDirectory()
        cls.dir = Path(cls.tmp.name)
        cls.repo_dir = cls.dir / "repo"
        r = cls.repo = GitRepo(cls.repo_dir)
        cls.c1 = r.commit("Initial import", "2026-01-10")
        cls.c2 = r.commit("Add the parser", "2026-01-20")
        r.branch("feature-x")
        cls.f1 = r.commit("Work on x", "2026-02-01")
        cls.f2 = r.commit("More work on x", "2026-02-02")
        r.checkout("main")
        cls.c3 = r.commit("Unrelated change", "2026-02-05")
        r.tag("v1.0")
        cls.m1 = r.merge("feature-x", "Merge branch 'feature-x'", "2026-02-10")
        r.branch("gh-branch")
        cls.g1 = r.commit("Work from a pull request", "2026-02-20")
        r.checkout("main")
        cls.m2 = r.merge("gh-branch", "Merge pull request #7 from acme/gh-branch", "2026-02-22")
        r.git("branch", "-q", "-d", "gh-branch")
        r.branch("wip-spike")
        cls.u1 = r.commit("Spike that was never merged", "2026-03-01")
        r.checkout("main")
        cls.w1 = r.commit("Fix flaky test (#42)", "2026-03-10")
        cls.config = {"nodes": "nodes.yaml", "page": "roadmap.md", "tracks": {"core": {"name": "core"}},
                      "kinds": {"change": {}}, "lanes": ["none"], "history": {"weak_refs": {"pr": r"#(\d+)\b"}}}
        (cls.repo_dir / "tech-tree.yaml").write_text(yaml.safe_dump(cls.config, sort_keys=False), encoding="utf-8")
        (cls.repo_dir / "nodes.yaml").write_text(yaml.safe_dump({"nodes": [
            {"id": "core-a", "overview": True, "state": "done", "evidence": [f"commit {cls.c2[:8]}"]},
            {"id": "core-b", "overview": True, "state": "done", "folded": ["branch feature-x"]},
            {"id": "core-c", "overview": True, "state": "done", "evidence": ["tag v1.0"]},
            {"id": "core-d", "overview": False, "state": "done", "start": "2026-03-10", "notes": "reviewed in PR #42"},
            {"id": "core-e", "overview": True, "state": "done", "folded": ["branch gh-branch"]},
        ]}), encoding="utf-8")
        cls.cfg = tree_config.load(cls.repo_dir / "tech-tree.yaml")

    @classmethod
    def tearDownClass(cls):
        cls.tmp.cleanup()

    def rows(self, before=None):
        rows, nodes, main_ref = hg.analyse(self.cfg, before)
        self.assertEqual(main_ref, "main")  # default: the first existing of origin/main, main, master
        return {c["sha"]: c for c in rows}

    def test_buckets_strong_and_all(self):
        by = self.rows()
        self.assertEqual(len(by), 10)
        expect = {  # sha: (bucket, strong bucket, node ids, vias)
            self.c1: ("none", "none", [], []),
            self.c2: ("node", "node", ["core-a"], ["sha"]),
            self.f1: ("folded", "folded", ["core-b"], ["branch"]),
            self.f2: ("folded", "folded", ["core-b"], ["branch"]),
            self.m1: ("folded", "folded", ["core-b"], ["branch"]),
            self.c3: ("node", "node", ["core-c"], ["branch"]),  # a tag points at it
            self.g1: ("folded", "folded", ["core-e"], ["branch"]),  # named only by the merge subject
            self.m2: ("folded", "folded", ["core-e"], ["branch"]),
            self.u1: ("none", "none", [], []),
            self.w1: ("node", "none", ["core-d"], ["pr"]),  # weak only: counts in "all", not in "strong"
        }
        for sha, (bucket, strong, ids, vias) in expect.items():
            c = by[sha]
            self.assertEqual((c["bucket"], c["strong_bucket"], c["ids"], c["vias"]), (bucket, strong, ids, vias), c["subject"])
        self.assertEqual(sum(1 for c in by.values() if c["strong_bucket"] == "none"), 3)  # c1, u1, w1
        self.assertEqual(sum(1 for c in by.values() if c["bucket"] == "none"), 2)  # c1, u1

    def test_refs_column_names_branches_and_tags(self):
        by = self.rows()
        self.assertIn("feature-x", by[self.f1]["refs"])
        self.assertIn("wip-spike", by[self.u1]["refs"])
        self.assertIn("v1.0", by[self.c3]["refs"])
        self.assertEqual(by[self.c1]["refs"], ["main"])

    def test_before_limits_history(self):
        self.assertEqual({c["subject"] for c in self.rows("2026-02-04").values()},
                         {"Initial import", "Add the parser", "Work on x", "More work on x"})

    def test_cli_writes_csv_and_summary_with_the_strong_table_first(self):
        out = self.dir / "out"
        p = subprocess.run([sys.executable, str(SCRIPTS / "history_gap.py"), "--config", str(self.repo_dir / "tech-tree.yaml"),
                            "--out", str(out)], capture_output=True, text=True, encoding="utf-8", cwd=self.dir)
        self.assertEqual(p.returncode, 0, p.stderr)
        summary = (out / "history_gap_summary.md").read_text(encoding="utf-8")
        self.assertEqual(summary.splitlines()[0], "# History gap (all history, 10 commits, main ref main)")
        self.assertLess(summary.index("## Strong matches"), summary.index("## All matches"))
        self.assertIn("3 of 10 commits have no strong match", summary)
        self.assertIn("shown separately because one number is shared by many commits", summary)
        self.assertIn("## Nodes not on the main page", summary)
        self.assertIn("overview false and not a forward state".lower(), summary.lower())
        self.assertEqual(p.stdout.strip(), summary.strip())
        with open(out / "history_gap_commits.csv", encoding="utf-8", newline="") as f:
            rows = list(csv.DictReader(f))
        self.assertEqual(len(rows), 10)
        self.assertEqual(list(rows[0]), ["sha", "date", "refs", "subject", "bucket", "strong_bucket", "node_ids", "via"])
        weak = next(r for r in rows if r["subject"].startswith("Fix flaky"))
        self.assertEqual((weak["bucket"], weak["strong_bucket"], weak["via"]), ("node", "none", "pr"))
        self.assertEqual(weak["sha"], self.w1[:10])

    def test_before_option_and_bad_main_ref(self):
        out = self.dir / "out"
        p = subprocess.run([sys.executable, str(SCRIPTS / "history_gap.py"), "--config", str(self.repo_dir / "tech-tree.yaml"),
                            "--out", str(out), "--before", "2026-01-15"], capture_output=True, text=True, encoding="utf-8")
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertIn("before 2026-01-15, 1 commits", p.stdout)
        raw = {**self.config, "history": {"main": "no-such-branch"}}
        (self.repo_dir / "badmain.yaml").write_text(yaml.safe_dump(raw), encoding="utf-8")
        p = subprocess.run([sys.executable, str(SCRIPTS / "history_gap.py"), "--config", str(self.repo_dir / "badmain.yaml"),
                            "--out", str(out)], capture_output=True, text=True, encoding="utf-8")
        self.assertEqual(p.returncode, 2)
        self.assertIn("no-such-branch", p.stderr)

    def test_out_is_required(self):
        p = subprocess.run([sys.executable, str(SCRIPTS / "history_gap.py"), "--config", str(self.repo_dir / "tech-tree.yaml")],
                           capture_output=True, text=True, encoding="utf-8")
        self.assertEqual(p.returncode, 2)


if __name__ == "__main__":
    unittest.main()
