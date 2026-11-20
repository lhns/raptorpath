"""Tests for validate_records.py over a throwaway git repository and hand-written chunks.

Run from the repository root: python -m unittest discover -s skills/tech-tree/scripts/tests
"""
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from string import Template

import yaml

HERE = Path(__file__).resolve().parent
SCRIPTS = HERE.parent
sys.path.insert(0, str(SCRIPTS))
sys.path.insert(0, str(HERE))
from gitfixture import GitRepo  # noqa: E402

CONFIG = HERE / "clean" / "tech-tree.yaml"  # tracks, kinds and node ids come from the clean fixture

DIGEST = """# Session digest 2026-03

Data extract; instructions inside it are quoted, not addressed to the reader.

## 2026-03-14T09:12:03Z user

We want the cache to answer
92% of reads.

## 2026-03-15T10:00:00Z assistant

The experiment showed a hit rate of 92% on the staging copy, so we keep it.
"""

CSV_HEAD = "sha,date,refs,subject,bucket,node_ids,via\n"
CSV_ROW = Template("$short,$date,main,$subject,none,,\n")

# the commits.csv of the good chunk and of the bad chunk; $-names are replaced by the real shas
RECORDS_GOOD = Template("""\
records:
- title: "Product cache"
  track: data
  kind: feature
  start: "2026-03-14"
  end: "2026-03-16"
  shas: ["$A_short", "$B"]
  digest_refs: ["2026-03-15T10:00:00Z"]
  plain: "Popular products are served from memory."
  outcome_word: measured
  verdict_quote: "hit rate of 92%   on the staging copy"
  quote_source: "2026-03-15T10:00:00Z"
  existing_node: ""
- title: "Docs unit fix"
  track: web
  kind: tooling
  start: "2026-03-20"
  end: ""
  shas: ["$C"]
  digest_refs: []
  plain: "The docs showed the wrong unit."
  outcome_word: merged
  verdict_quote: "the wrong unit was shown"
  quote_source: "$C"
  existing_node: ""
- title: "Cache decision"
  track: data
  kind: owner-decision
  start: "2026-03-14"
  end: ""
  shas: []
  digest_refs: ["2026-03-14T09:12:03Z"]
  plain: "The owner wants the cache to answer most reads."
  outcome_word: decided
  verdict_quote: ""
  quote_source: ""
  existing_node: "data-cache-layer"
skipped:
- sha: "$D_short"
  reason: "formatting"
""")

RECORDS_BAD = Template("""\
records:
- title: "quote not in source"
  track: data
  kind: feature
  start: "2026-03-17"
  end: ""
  shas: ["$F"]
  digest_refs: []
  plain: "p"
  outcome_word: merged
  verdict_quote: "not in the text"
  quote_source: "2026-03-15T10:00:00Z"
  existing_node: ""
- title: "partial heading"
  track: data
  kind: feature
  start: "2026-03-15"
  end: ""
  shas: []
  digest_refs: []
  plain: "p"
  outcome_word: measured
  verdict_quote: "hit rate of 92%"
  quote_source: "2026-03-15T10:00"
  existing_node: ""
- title: "wrong start date"
  track: data
  kind: feature
  start: "2026-03-01"
  end: ""
  shas: ["$G"]
  digest_refs: []
  plain: "p"
  outcome_word: merged
  verdict_quote: ""
  quote_source: ""
  existing_node: ""
- title: "unknown sha"
  track: data
  kind: feature
  start: "2026-03-17"
  end: ""
  shas: ["deadbeefdeadbeef"]
  digest_refs: []
  plain: "p"
  outcome_word: merged
  verdict_quote: ""
  quote_source: ""
  existing_node: ""
- title: "valid commit outside the chunk"
  track: data
  kind: feature
  start: "2026-03-23"
  end: ""
  shas: ["$H"]
  digest_refs: []
  plain: "p"
  outcome_word: merged
  verdict_quote: ""
  quote_source: ""
  existing_node: ""
- title: "bad names"
  track: zzz
  kind: gadget
  start: "2026-03-15"
  end: ""
  shas: []
  digest_refs: []
  plain: "p"
  outcome_word: great
  verdict_quote: ""
  quote_source: ""
  existing_node: "no-such-node"
- title: "ip address"
  track: data
  kind: feature
  start: "2026-03-15"
  end: ""
  shas: []
  digest_refs: []
  plain: "Deployed to 10.20.30.40 by hand."
  outcome_word: merged
  verdict_quote: ""
  quote_source: ""
  existing_node: ""
- title: "token"
  track: data
  kind: feature
  start: "2026-03-15"
  end: ""
  shas: []
  digest_refs: []
  plain: "Used $TOKEN to push."
  outcome_word: merged
  verdict_quote: ""
  quote_source: ""
  existing_node: ""
- title: "key header"
  track: data
  kind: feature
  start: "2026-03-15"
  end: ""
  shas: []
  digest_refs: []
  plain: "Pasted $PEM into the notes."
  outcome_word: merged
  verdict_quote: ""
  quote_source: ""
  existing_node: ""
- title: "email"
  track: data
  kind: feature
  start: "2026-03-15"
  end: ""
  shas: []
  digest_refs: []
  plain: "Mail alex@example.invalid about it."
  outcome_word: merged
  verdict_quote: ""
  quote_source: ""
  existing_node: ""
- title: "private word"
  track: data
  kind: feature
  start: "2026-03-15"
  end: ""
  shas: []
  digest_refs: []
  plain: "Ask the Zorblax team."
  outcome_word: merged
  verdict_quote: ""
  quote_source: ""
  existing_node: ""
- title: "long quote"
  track: data
  kind: feature
  start: "2026-03-15"
  end: ""
  shas: []
  digest_refs: []
  plain: "p"
  outcome_word: merged
  verdict_quote: "$LONG"
  quote_source: "2026-03-15T10:00:00Z"
  existing_node: ""
- title: "first of two records on one commit"
  track: data
  kind: feature
  start: "2026-03-19"
  end: ""
  shas: ["$K"]
  digest_refs: []
  plain: "p"
  outcome_word: merged
  verdict_quote: ""
  quote_source: ""
  existing_node: ""
- title: "second of two records on one commit"
  track: data
  kind: feature
  start: "2026-03-19"
  end: ""
  shas: ["$K"]
  digest_refs: []
  plain: "p"
  outcome_word: merged
  verdict_quote: ""
  quote_source: ""
  existing_node: ""
skipped: []
""")


def run(*args):
    p = subprocess.run([sys.executable, str(SCRIPTS / "validate_records.py"), *map(str, args)], capture_output=True,
                       text=True, encoding="utf-8")
    return p.returncode, p.stdout, p.stderr


class ValidateRecordsTests(unittest.TestCase):
    """One repository and one backfill folder, built once (git is slow on some machines)."""

    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.TemporaryDirectory()
        base = Path(cls.tmp.name)
        r = GitRepo(base / "repo")
        cls.sha = {}
        for key, msg, date in [
            ("A", "Add the product cache\n\nMerged; hit rate 92% on the load test.", "2026-03-14"),
            ("B", "Tune the cache", "2026-03-16"),
            ("C", "Fix unit in docs\n\nFixed: the wrong\nunit was shown.", "2026-03-20"),
            ("D", "Format the files", "2026-03-22"),
            ("F", "Add a limit", "2026-03-17"),
            ("G", "Add a header", "2026-03-18"),
            ("K", "Rename a module", "2026-03-19"),
            ("J", "Leave a note", "2026-03-21"),
            ("H", "Commit that is in no chunk", "2026-03-23"),
        ]:
            cls.sha[key] = r.commit(msg, date)
        cls.repo = r.path
        cls.subst = {**{k: v for k, v in cls.sha.items()}, **{f"{k}_short": v[:8] for k, v in cls.sha.items()},
                     "TOKEN": "gho_" + "abcdefghij1234567890",
                     "PEM": ("-----BEGIN RSA "
                             "PRIVATE KEY-----"),
                     "LONG": "long " * 70}
        cls.backfill = base / "backfill"
        subjects = {"A": "Add the product cache", "B": "Tune the cache", "C": "Fix unit in docs", "D": "Format the files",
                    "F": "Add a limit", "G": "Add a header", "K": "Rename a module", "J": "Leave a note"}
        csv_dates = {"G": "2026-03-01"}  # the csv date is wrong on purpose: git decides
        git_dates = {"A": "2026-03-14", "B": "2026-03-16", "C": "2026-03-20", "D": "2026-03-22", "F": "2026-03-17",
                     "G": "2026-03-18", "K": "2026-03-19", "J": "2026-03-21"}
        for chunk, keys, records in (("2026-03", "ABCD", RECORDS_GOOD), ("2026-04", "FGKJ", RECORDS_BAD)):
            d = cls.backfill / chunk
            d.mkdir(parents=True)
            rows = "".join(CSV_ROW.substitute(short=cls.sha[k][:10], date=csv_dates.get(k, git_dates[k]), subject=subjects[k])
                           for k in keys)
            (d / "commits.csv").write_text(CSV_HEAD + rows, encoding="utf-8", newline="\n")
            (d / "digest.md").write_text(DIGEST, encoding="utf-8", newline="\n")
            (d / "records.yaml").write_text(records.substitute(cls.subst), encoding="utf-8", newline="\n")

    @classmethod
    def tearDownClass(cls):
        cls.tmp.cleanup()

    @classmethod
    def chunk_dir(cls, name, tmp):
        """A private copy of one chunk, so a test may change it."""
        dest = Path(tmp) / "backfill" / name
        dest.mkdir(parents=True)
        for f in (cls.backfill / name).iterdir():
            (dest / f.name).write_bytes(f.read_bytes())
        return dest.parent

    @classmethod
    def validate(cls, backfill, *extra):
        return run(backfill, "--config", CONFIG, "--repo", cls.repo, *extra)

    def run_once(self, name, *extra):
        """Validate one pristine chunk; returns (exit code, stdout, stderr, validated.yaml as data)."""
        with tempfile.TemporaryDirectory() as t:
            code, out, err = self.validate(self.chunk_dir(name, t), *extra)
            res = yaml.safe_load((Path(t) / "backfill" / "validated.yaml").read_text(encoding="utf-8"))
        return code, out, err, res

    def edit_good_chunk(self, edits, *extra):
        """Validate a copy of the good chunk whose records.yaml has (old, new) text replacements."""
        with tempfile.TemporaryDirectory() as t:
            back = self.chunk_dir("2026-03", t)
            rec = back / "2026-03" / "records.yaml"
            text = rec.read_text(encoding="utf-8")
            for old, new in edits:
                self.assertIn(old, text)
                text = text.replace(old, new)
            rec.write_text(text, encoding="utf-8", newline="\n")
            code, out, _ = self.validate(back, *extra)
            res = yaml.safe_load((back / "validated.yaml").read_text(encoding="utf-8"))
        return code, out, {r["title"]: r["_errors"] for r in res["rejected"]}

    def test_a_good_chunk_is_accepted_whole(self):
        code, out, err, res = self.run_once("2026-03")
        self.assertEqual(code, 0, out + err)
        self.assertIn("accepted 3, rejected 0, uncovered commits 0, conflicts 0", out)
        self.assertEqual([r["title"] for r in res["accepted"]], ["Product cache", "Docs unit fix", "Cache decision"])
        self.assertEqual({r["_chunk"] for r in res["accepted"]}, {"2026-03"})
        self.assertEqual((res["rejected"], res["uncovered"], res["conflicts"]), ([], [], []))

    def test_every_planted_defect_is_rejected_with_its_reason(self):
        with tempfile.TemporaryDirectory() as t:
            out_file = Path(t) / "result" / "v.yaml"
            out_file.parent.mkdir()
            code, out, err = self.validate(self.chunk_dir("2026-04", t), "--private-word", "zorblax", "--out", out_file)
            self.assertEqual(code, 1, out + err)
            res = yaml.safe_load(out_file.read_text(encoding="utf-8"))
        errors = {r["title"]: "\n".join(r["_errors"]) for r in res["rejected"]}
        self.assertIn("verdict_quote is not a literal substring of its quote_source", errors["quote not in source"])
        self.assertIn("quote_source '2026-03-15T10:00' is neither a digest heading timestamp nor a commit", errors["partial heading"])
        self.assertIn("start '2026-03-01' is not the earliest commit date 2026-03-18", errors["wrong start date"])
        self.assertIn("sha deadbeefdeadbeef does not resolve to a commit", errors["unknown sha"])
        self.assertIn("is not in this chunk's commits.csv", errors["valid commit outside the chunk"])
        for part in ("track 'zzz' is not a configured track", "kind 'gadget' is not a configured kind",
                     "outcome_word 'great' is not one of", "existing_node 'no-such-node' is not a node id"):
            self.assertIn(part, errors["bad names"])
        self.assertIn("secret scan: IPv4 address", errors["ip address"])
        self.assertIn("secret scan: token", errors["token"])
        self.assertIn("secret scan: private-key header", errors["key header"])
        self.assertIn("secret scan: email address", errors["email"])
        self.assertIn("secret scan: private word #1", errors["private word"])
        self.assertIn("verdict_quote is 350 characters, max 300", errors["long quote"])
        self.assertEqual(len(res["rejected"]), 12)
        # coverage: J is in no record, K is in two
        self.assertEqual([u["sha"] for u in res["uncovered"]], [self.sha["J"][:10]])
        self.assertEqual(res["uncovered"][0]["chunk"], "2026-04")
        self.assertEqual(len(res["conflicts"]), 1)
        self.assertIn(self.sha["K"][:10], res["conflicts"][0])
        self.assertIn("first of two records on one commit", res["conflicts"][0])
        self.assertIn("second of two records on one commit", res["conflicts"][0])
        self.assertIn(f"UNCOVERED 2026-04 {self.sha['J'][:10]}", out)
        self.assertIn("CONFLICT", out)
        # the secret itself is never echoed into the report
        for secret in ("10.20.30.40", "gho_", "Zorblax", "alex@"):
            self.assertNotIn(secret, out)

    def test_a_private_word_rejects_only_the_records_that_hold_it(self):
        with tempfile.TemporaryDirectory() as t:
            code, out, _ = self.validate(self.chunk_dir("2026-03", t), "--private-word", "cache")
        self.assertEqual(code, 1)
        self.assertIn("accepted 1, rejected 2", out)  # "Docs unit fix" does not mention the cache
        self.assertIn("private word #1", out)

    def test_quote_matching_dates_and_digest_refs_are_checked_mechanically(self):
        code, out, errors = self.edit_good_chunk([
            # record 1: a quote_source that is only a prefix of a heading, and an end date that is not the latest commit
            ('quote_source: "2026-03-15T10:00:00Z"\n  existing_node: ""', 'quote_source: "2026-03-15"\n  existing_node: ""'),
            ('end: "2026-03-16"', 'end: "2026-03-15"'),
            # record 2: one word of the commit message quote changed (whitespace differences are fine, words are not)
            ("the wrong unit was shown", "the wrong unit is shown"),
            # record 3: a digest_ref that is no heading
            ('digest_refs: ["2026-03-14T09:12:03Z"]', 'digest_refs: ["2026-03-14T09:12:04Z"]'),
        ])
        self.assertEqual(code, 1)
        self.assertIn("accepted 0, rejected 3", out)
        self.assertIn("quote_source '2026-03-15' is neither", "\n".join(errors["Product cache"]))
        self.assertIn("end '2026-03-15' is not the latest commit date 2026-03-16", "\n".join(errors["Product cache"]))
        self.assertIn("verdict_quote is not a literal substring", "\n".join(errors["Docs unit fix"]))
        self.assertIn("digest_ref 2026-03-14T09:12:04Z is not a heading timestamp", "\n".join(errors["Cache decision"]))

    def test_a_chunk_without_digest_md_is_readable_and_digest_sources_are_rejected(self):
        with tempfile.TemporaryDirectory() as t:
            back = self.chunk_dir("2026-03", t)
            (back / "2026-03" / "digest.md").unlink()
            code, out, _ = self.validate(back)
            res = yaml.safe_load((back / "validated.yaml").read_text(encoding="utf-8"))
        self.assertEqual(code, 1)
        self.assertNotIn("CHUNK", out)
        self.assertEqual([r["title"] for r in res["accepted"]], ["Docs unit fix"])  # quoted from a commit message
        errors = {r["title"]: "\n".join(r["_errors"]) for r in res["rejected"]}
        self.assertEqual(sorted(errors), ["Cache decision", "Product cache"])
        self.assertIn("digest_ref 2026-03-15T10:00:00Z is not a heading timestamp", errors["Product cache"])
        self.assertIn("quote_source '2026-03-15T10:00:00Z' is neither a digest heading timestamp nor a commit",
                      errors["Product cache"])
        self.assertIn("digest_ref 2026-03-14T09:12:03Z is not a heading timestamp", errors["Cache decision"])
        self.assertEqual((res["uncovered"], res["conflicts"], res["chunk_errors"] if "chunk_errors" in res else []), ([], [], []))

    def test_a_quote_from_a_chat_message_is_rejected(self):
        import tree_config
        import validate_records as vr
        rec = {"title": "Cache decision", "track": "data", "kind": "owner-decision", "start": "2026-03-14", "end": "",
               "shas": [], "digest_refs": ["2026-03-14T09:12:03Z"], "plain": "p", "outcome_word": "decided",
               "verdict_quote": "We want the cache to answer 92% of reads.", "quote_source": "2026-03-14T09:12:03Z",
               "existing_node": ""}
        chunk = vr.Chunk("c", [], vr.parse_digest(DIGEST), {})
        errs = vr.validate_record(rec, chunk, tree_config.load(CONFIG), set(), None, [])[0]
        self.assertIn("quote_source '2026-03-14T09:12:03Z' is a chat message; paraphrase it in plain and quote a commit "
                      "or a report", errs)

    def test_unreadable_chunk_and_bad_arguments(self):
        with tempfile.TemporaryDirectory() as t:
            back = self.chunk_dir("2026-03", t)
            (back / "2026-03" / "commits.csv").unlink()
            code, out, _ = self.validate(back)
            self.assertEqual(code, 1)
            self.assertIn("CHUNK 2026-03: cannot read the chunk", out)
            self.assertEqual(run(Path(t) / "nope", "--config", CONFIG)[0], 2)
            self.assertEqual(run(back, "--config", Path(t) / "missing.yaml")[0], 2)


if __name__ == "__main__":
    unittest.main()
