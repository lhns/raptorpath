#!/usr/bin/env python3
"""Mechanically validate the records an extractor wrote (format: extractor-brief.md).

BACKFILL_DIR holds one subfolder per chunk; a subfolder with a records.yaml is a chunk and also
needs commits.csv; digest.md is optional (without it no digest entry exists). A record is accepted or rejected whole, never repaired:
  - track and kind are in the config, outcome_word is in the fixed list, existing_node (if set)
    is an id in the nodes file;
  - every sha resolves to a commit (git rev-parse --verify <sha>^{commit}) and is in the chunk's
    commits.csv;
  - start is the earliest and end the latest author date (git, %as) of the record's commits (or "");
  - verdict_quote (whitespace-normalized) is a literal substring of its quote_source: a digest entry
    whose heading timestamp equals quote_source exactly, or the full message of the commit
    quote_source resolves to; at most 300 characters;
  - the record holds no secret (IPv4 address, email, token prefix, private-key header, --private-word).
Coverage: each commits.csv row is in exactly one record or in `skipped`. Commits in two places are
reported as conflicts, commits in none as uncovered.

Writes validated.yaml ({accepted, rejected (each with _errors), uncovered, conflicts}) and prints a
summary. Exit 1 if anything is rejected, uncovered or in conflict.

Usage:
  python validate_records.py BACKFILL_DIR [--config PATH] [--repo DIR] [--private-word W ...] [--out FILE]
"""
import argparse
import collections
import csv
import re
import subprocess
import sys
from pathlib import Path

import yaml

sys.path.insert(0, str(Path(__file__).resolve().parent))
import build  # noqa: E402
import tree_config  # noqa: E402

OUTCOME_WORDS = ["merged", "refuted", "parked", "abandoned", "measured", "decided", "unknown"]
MAX_QUOTE = 300
SHA_RE = re.compile(r"^[0-9a-f]{7,40}$")
DIGEST_HEAD_RE = re.compile(r"(?m)^## (\S+)[ \t]*(\S*)[^\n]*\n")
CHAT_KINDS = ("user",)  # typed chat lines: paraphrased in `plain`, never quoted as a verdict

IPV4 = re.compile(r"\b(?:\d{1,3}\.){3}\d{1,3}\b")
EMAIL = re.compile(r"[\w.+-]+@[\w-]+(?:\.[\w-]+)+")
TOKEN = re.compile(r"\b(?:ghp|gho|ghs|ghu|github_pat|sk|xox[bp])[-_][A-Za-z0-9_-]{10,}|\bAKIA[0-9A-Z]{16}\b")
# a key block header, built from parts so that no source line holds both words
PEM_HEAD = re.compile(
    "-----BEGIN [A-Z ]*"
    "PRIVATE KEY-----")
SECRET_PATTERNS = [("IPv4 address", IPV4), ("email address", EMAIL), ("token", TOKEN), ("private-key header", PEM_HEAD)]


def norm(s):
    return " ".join(str(s).split())


class Repo:
    """Cached read-only git lookups."""

    def __init__(self, path):
        self.path = path
        self._full, self._info = {}, {}

    def _git(self, *args):
        return subprocess.run(["git", "-C", str(self.path), *args], capture_output=True, encoding="utf-8", errors="replace")

    def full(self, sha):
        """The full sha a commit-ish resolves to, or None."""
        if sha not in self._full:
            r = self._git("rev-parse", "--verify", "-q", f"{sha}^{{commit}}")
            self._full[sha] = r.stdout.strip() if r.returncode == 0 and r.stdout.strip() else None
        return self._full[sha]

    def info(self, full):
        """(author date YYYY-MM-DD, full message) of a resolved commit."""
        if full not in self._info:
            out = self._git("show", "-s", "--format=%as%x1f%B", full).stdout
            date, _, msg = out.partition("")
            self._info[full] = (date.strip(), msg)
        return self._info[full]


def parse_digest(text):
    """{heading timestamp: [entry text, ...]}; the timestamp is the first word after '## '."""
    entries = collections.defaultdict(list)
    heads = list(DIGEST_HEAD_RE.finditer(text))
    for i, m in enumerate(heads):
        end = heads[i + 1].start() if i + 1 < len(heads) else len(text)
        entries[m.group(1)].append((m.group(2), text[m.end():end]))
    return entries


def flatten(v):
    if isinstance(v, dict):
        return [s for x in v.values() for s in flatten(x)]
    if isinstance(v, (list, tuple)):
        return [s for x in v for s in flatten(x)]
    return [] if v is None else [str(v)]


def secret_findings(record, private_words):
    """Names of the kinds of secret found in a record (never the secret itself)."""
    text = "\n".join(flatten(record))
    found = [name for name, rx in SECRET_PATTERNS if rx.search(text)]
    low = text.lower()
    found += [f"private word #{i}" for i, w in enumerate(private_words, 1) if w and w.lower() in low]
    return found


class Chunk:
    def __init__(self, name, rows, digest, data):
        self.name, self.rows, self.digest, self.data = name, rows, digest, data
        self.lengths = sorted({len(r["sha"]) for r in rows}, reverse=True)
        self.by_prefix = {r["sha"]: r for r in rows}

    def row_of(self, full):
        """The commits.csv row a full sha belongs to, or None."""
        for n in self.lengths:
            if full[:n] in self.by_prefix:
                return self.by_prefix[full[:n]]
        return None


def validate_record(rec, chunk, cfg, node_ids, repo, private_words):
    """(errors, [full shas]) for one record."""
    if not isinstance(rec, dict):
        return ["record is not a mapping"], []
    errs = []
    if rec.get("track") not in cfg.tracks:
        errs.append(f"track {rec.get('track')!r} is not a configured track")
    if rec.get("kind") not in cfg.kinds:
        errs.append(f"kind {rec.get('kind')!r} is not a configured kind")
    if rec.get("outcome_word") not in OUTCOME_WORDS:
        errs.append(f"outcome_word {rec.get('outcome_word')!r} is not one of {OUTCOME_WORDS}")
    existing = rec.get("existing_node") or ""
    if existing and existing not in node_ids:
        errs.append(f"existing_node {existing!r} is not a node id")

    fulls = []
    shas = rec.get("shas") or []
    if not isinstance(shas, list):
        errs.append("shas must be a list")
        shas = []
    for s in shas:
        s = str(s)
        full = repo.full(s) if SHA_RE.match(s) else None
        if not full:
            errs.append(f"sha {s} does not resolve to a commit")
        elif not chunk.row_of(full):
            errs.append(f"sha {s} is not in this chunk's commits.csv")
        else:
            fulls.append(full)

    start, end = norm(rec.get("start") or ""), norm(rec.get("end") or "")
    refs = rec.get("digest_refs") or []
    if not isinstance(refs, list):
        errs.append("digest_refs must be a list")
        refs = []
    for r in refs:
        if str(r) not in chunk.digest:
            errs.append(f"digest_ref {r} is not a heading timestamp in digest.md")
    if fulls and len(fulls) == len(shas):
        dates = sorted(repo.info(f)[0] for f in fulls)
        if start != dates[0]:
            errs.append(f"start {start!r} is not the earliest commit date {dates[0]}")
        if end and end != dates[-1]:
            errs.append(f"end {end!r} is not the latest commit date {dates[-1]}")
    elif not shas:
        if not re.fullmatch(r"\d{4}-\d{2}-\d{2}", start):
            errs.append(f"start {start!r} is not a YYYY-MM-DD date")
        if end and not re.fullmatch(r"\d{4}-\d{2}-\d{2}", end):
            errs.append(f"end {end!r} is not a YYYY-MM-DD date")

    quote = rec.get("verdict_quote") or ""
    if quote:
        if len(str(quote)) > MAX_QUOTE:
            errs.append(f"verdict_quote is {len(str(quote))} characters, max {MAX_QUOTE}")
        src = str(rec.get("quote_source") or "")
        if src in chunk.digest:
            haystacks = [norm(t) for k, t in chunk.digest[src] if k not in CHAT_KINDS]
            if not haystacks:
                errs.append(f"quote_source {src!r} is a chat message; paraphrase it in plain and quote a commit or a report")
        else:
            full = repo.full(src) if SHA_RE.match(src) else None
            haystacks = [norm(repo.info(full)[1])] if full else []
            if not haystacks:
                errs.append(f"quote_source {src!r} is neither a digest heading timestamp nor a commit")
        if haystacks and not any(norm(quote) in h for h in haystacks):
            errs.append("verdict_quote is not a literal substring of its quote_source")
    errs += [f"secret scan: {f}" for f in secret_findings(rec, private_words)]
    return errs, fulls


def load_chunk(d):
    """(Chunk, None) or (None, error message)."""
    try:
        with open(d / "commits.csv", encoding="utf-8", newline="") as f:
            rows = list(csv.DictReader(f))
        dg = d / "digest.md"  # optional: a project without session transcripts has none
        digest = parse_digest(dg.read_text(encoding="utf-8")) if dg.exists() else {}
        data = yaml.safe_load((d / "records.yaml").read_text(encoding="utf-8")) or {}
    except (OSError, yaml.YAMLError, UnicodeDecodeError) as e:
        return None, f"{d.name}: cannot read the chunk: {e}"
    if not isinstance(data, dict) or not all(isinstance(r.get("sha"), str) and r["sha"] for r in rows):
        return None, f"{d.name}: records.yaml must be a mapping and commits.csv needs a sha column"
    return Chunk(d.name, rows, digest, data), None


def validate_chunk(chunk, cfg, node_ids, repo, private_words):
    """(accepted, rejected, uncovered, conflicts) of one chunk."""
    accepted, rejected = [], []
    owners = collections.defaultdict(list)  # csv sha -> labels of the records/skips that hold the commit
    for i, rec in enumerate((chunk.data.get("records") or []), 1):
        errs, fulls = validate_record(rec, chunk, cfg, node_ids, repo, private_words)
        label = f"record {i} ({norm(rec.get('title', ''))[:40]})" if isinstance(rec, dict) else f"record {i}"
        for f in fulls:
            row = chunk.row_of(f)
            if label not in owners[row["sha"]]:
                owners[row["sha"]].append(label)
        item = dict(rec) if isinstance(rec, dict) else {"record": rec}
        item["_chunk"] = chunk.name
        if errs:
            item["_errors"] = errs
            rejected.append(item)
        else:
            accepted.append(item)
    for j, sk in enumerate((chunk.data.get("skipped") or []), 1):
        s = str(sk.get("sha", "")) if isinstance(sk, dict) else ""
        row = next((r for r in chunk.rows if s and (r["sha"].startswith(s) or s.startswith(r["sha"]))), None)
        if row:
            owners[row["sha"]].append(f"skipped entry {j}")
    uncovered = [{"chunk": chunk.name, "sha": r["sha"], "subject": r.get("subject", "")}
                 for r in chunk.rows if not owners.get(r["sha"])]
    conflicts = [f"{chunk.name}: commit {sha} is in {len(who)} places: {', '.join(who)}"
                 for sha, who in owners.items() if len(who) > 1]
    return accepted, rejected, uncovered, conflicts


def run(backfill, cfg, repo_path, private_words):
    """The result mapping written to validated.yaml (plus `chunk_errors`)."""
    nodes = build.load_nodes(cfg.nodes_path)
    node_ids = {n["id"] for n in nodes if isinstance(n, dict)}
    repo = Repo(repo_path)
    res = {"accepted": [], "rejected": [], "uncovered": [], "conflicts": [], "chunk_errors": []}
    for d in sorted(p for p in Path(backfill).iterdir() if p.is_dir() and (p / "records.yaml").exists()):
        chunk, err = load_chunk(d)
        if err:
            res["chunk_errors"].append(err)
            continue
        a, r, u, c = validate_chunk(chunk, cfg, node_ids, repo, private_words)
        res["accepted"] += a
        res["rejected"] += r
        res["uncovered"] += u
        res["conflicts"] += c
    return res


def summary(res):
    lines = [f"accepted {len(res['accepted'])}, rejected {len(res['rejected'])}, "
             f"uncovered commits {len(res['uncovered'])}, conflicts {len(res['conflicts'])}, "
             f"unreadable chunks {len(res['chunk_errors'])}"]
    reasons = collections.Counter(e.split(" ")[0] + " " + e.split(" ")[1] if " " in e else e
                                  for r in res["rejected"] for e in r["_errors"])
    if reasons:
        lines.append("rejection reasons: " + "; ".join(f"{k} x{v}" for k, v in reasons.most_common()))
    for r in res["rejected"]:
        lines.append(f"REJECTED {r['_chunk']} | {norm(r.get('title', ''))[:50]} | " + " | ".join(r["_errors"]))
    lines += [f"UNCOVERED {u['chunk']} {u['sha']} {u['subject'][:60]}" for u in res["uncovered"]]
    lines += [f"CONFLICT {c}" for c in res["conflicts"]]
    lines += [f"CHUNK {c}" for c in res["chunk_errors"]]
    return lines


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("backfill_dir", type=Path)
    tree_config.add_config_arg(ap)
    ap.add_argument("--repo", type=Path, help="the git repository (default: the config's directory)")
    ap.add_argument("--private-word", action="append", default=[], help="a word no record may contain (repeatable)")
    ap.add_argument("--out", type=Path, help="where to write validated.yaml (default: BACKFILL_DIR/validated.yaml)")
    a = ap.parse_args(argv)
    tree_config.utf8_stdio()
    cfg = tree_config.load_for_cli(a.config, "validate_records")
    if cfg is None:
        return 2
    if not a.backfill_dir.is_dir():
        print(f"validate_records: {a.backfill_dir} is not a directory", file=sys.stderr)
        return 2
    try:
        res = run(a.backfill_dir, cfg, a.repo or cfg.root, a.private_word)
    except (OSError, ValueError, yaml.YAMLError) as e:
        print(f"validate_records: {e}", file=sys.stderr)
        return 2
    out = a.out or a.backfill_dir / "validated.yaml"
    out.write_text(yaml.safe_dump(res, allow_unicode=True, sort_keys=False), encoding="utf-8", newline="\n")
    print("\n".join(summary(res)))
    return 1 if res["rejected"] or res["uncovered"] or res["conflicts"] or res["chunk_errors"] else 0


if __name__ == "__main__":
    sys.exit(main())
