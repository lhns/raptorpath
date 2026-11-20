#!/usr/bin/env python3
"""Unit tests for history_gap.py matching. Run: python tools/roadmap/test_history_gap.py"""
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import history_gap as hg  # noqa: E402


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
        idx = hg.Index([node("rec-a", folded=["Pace all traffic (2026-07-12, commit 34449977)"])])
        b, ids, vias, strong = hg.classify(commit("3444997717566"), {}, idx)
        self.assertEqual((b, ids, vias, strong), ("folded", ["rec-a"], ["sha"], "folded"))


class StrongBucket(unittest.TestCase):
    def test_weak_node_match_does_not_hide_strong_folded_match(self):
        # the commit names status §5 (weak, matches rec-a's evidence) and is folded by sha in rec-b
        idx = hg.Index([node("rec-a", evidence=["status §5"]), node("rec-b", folded=["commit 773ca1c3"])])
        b, ids, vias, strong = hg.classify(commit("773ca1c3aa", "status §5: baseline"), {}, idx)
        self.assertEqual((b, vias), ("node", ["status"]))
        self.assertEqual(strong, "folded")

    def test_weak_only_match_is_strong_none(self):
        idx = hg.Index([node("rec-a", evidence=["ADR-0069"])])
        self.assertEqual(hg.classify(commit("abcdef1234", "per ADR-0069"), {}, idx)[3], "none")

    def test_strong_node_match_wins(self):
        idx = hg.Index([node("rec-a", evidence=["commit abcdef12"]), node("rec-b", folded=["commit abcdef12"])])
        self.assertEqual(hg.classify(commit("abcdef1234"), {}, idx)[3], "node")


if __name__ == "__main__":
    unittest.main()
