#!/usr/bin/env python3
"""Measure how much of a repository's git history the nodes file accounts for.

Mechanical only, no judgement. Every commit on every ref and tag (author date before --before,
default: all history) is matched against the nodes by:
  sha     short or full sha in a node's text
  branch  branch or tag name: branches that carry the commit and not the main ref, the branch
          named in the subject of the merge commit that brought it in (git's "Merge branch 'x'",
          "Merge remote-tracking branch 'origin/x'", GitHub's "Merge pull request #N from
          owner/x", and config history.merge_subject), and tags that point at it
  <name>  each config history.weak_refs entry (for example a decision-record or PR number) in the
          commit message vs the same pattern in node text. Weak: one number is shared by many
          commits, so weak matches inflate coverage and are reported separately.
Text searched per node: evidence, folded, notes, verdict_quote, headline.
Buckets, strongest first:
  node    matched through a field other than folded
  folded  matched only through a folded item
  none    no match
A commit also gets a STRONG bucket: the strongest bucket reached through a sha or branch match only.
The summary leads with the strong table; the all-matches table follows.

Also reports nodes that the main page does not show (overview false and not a forward state).

Usage:
  python history_gap.py [--config PATH] --out DIR [--before YYYY-MM-DD]
Git runs with -C <directory of the config>. Writes DIR/history_gap_commits.csv (bucket counts weak matches; strong_bucket does not) and
DIR/history_gap_summary.md. The output is not meant to be committed.
"""
import argparse
import collections
import csv
import datetime
import re
import subprocess
import sys
from pathlib import Path

import yaml

sys.path.insert(0, str(Path(__file__).resolve().parent))
import build  # noqa: E402
import tree_config  # noqa: E402

FORWARD_STATES = build.FORWARD_STATES
DIRECT_FIELDS = ("evidence", "notes", "verdict_quote", "headline")
NAME_TEXT_FIELDS = ("notes", "verdict_quote", "headline")
STRONG = ("sha", "branch")
BUCKETS = ("node", "folded")
DEFAULT_MAIN = ("origin/main", "main", "master")
MAIN_NAMES = ("main", "master")  # branches never reported as a branch name

# a merge commit subject -> the merged branch
MERGE_SUBJECTS = [
    re.compile(r"^Merge (?:remote-tracking )?branch '([^']+)'"),
    re.compile(r"^Merge pull request #\d+ from \S+?/(\S+)"),
]


def git(repo, *args):
    r = subprocess.run(["git", "-C", str(repo), "-c", "core.quotepath=off", *args], capture_output=True,
                       encoding="utf-8", errors="replace")
    return r.stdout


def git_lines(repo, *args):
    with subprocess.Popen(["git", "-C", str(repo), "-c", "core.quotepath=off", *args], stdout=subprocess.PIPE,
                          stderr=subprocess.DEVNULL, encoding="utf-8", errors="replace") as p:
        for line in p.stdout:
            yield line.rstrip("\n")


def flat(v):
    if v is None:
        return ""
    if isinstance(v, (list, tuple)):
        return "\n".join(flat(x) for x in v)
    if isinstance(v, dict):
        return "\n".join(flat(x) for x in v.values())
    return str(v)


def items(v):
    if v is None:
        return []
    return [str(x) for x in v] if isinstance(v, (list, tuple)) else [str(v)]


SHA_RE = re.compile(r"(?<![0-9a-zA-Z])[0-9a-f]{7,40}(?![0-9a-zA-Z])")
# "commit <sha>" is a sha even when its prefix is all digits or all letters, which the bare-token
# heuristic below (needs a digit and a letter a-f) cannot tell from a number or a word
COMMIT_SHA_RE = re.compile(r"(?<![0-9a-zA-Z])commits? ([0-9a-f]{7,40})(?![0-9a-zA-Z])", re.I)
NAME_RE = re.compile(r"^(?:branch|tag)\s+(.+)$")


def compile_refs(weak_refs):
    """{name: regex text with one group} -> {name: compiled}."""
    return {name: re.compile(rx, re.I) for name, rx in (weak_refs or {}).items()}


def has_digit(s):
    return any(c.isdigit() for c in s)


def ref_key(s):
    """Normal form of a weak ref's captured text: numbers without leading zeros, else lowercase."""
    s = s.strip()
    return str(int(s)) if s.isdigit() else s.lower()


def tokens(text, refs=None, item_names=()):
    """(kind, key) tokens of free text. Names come only from whole items (branch X / tag X)."""
    out = set()
    for m in SHA_RE.findall(text):
        if has_digit(m) and any(c in "abcdef" for c in m):
            out.add(("sha", m))
    for m in COMMIT_SHA_RE.findall(text):
        out.add(("sha", m.lower()))
    for name, rx in (refs or {}).items():
        for m in rx.finditer(text):
            out.add((name, ref_key(m.group(1))))
    for it in item_names:
        m = NAME_RE.match(it.strip())
        if m:
            out.add(("branch", norm_name(m.group(1))))
    return out


def norm_name(n):
    n = n.strip().strip("`'\"").rstrip(":,;.")
    for p in ("refs/heads/", "refs/tags/", "refs/remotes/", "origin/", "upstream/"):
        if n.startswith(p):
            n = n[len(p):]
    return n.lower()


# ---------------------------------------------------------------- nodes
class Index:
    """token -> {(node id, bucket)}; bucket is node|folded."""

    def __init__(self, nodes, weak_refs=None):
        self.refs = compile_refs(weak_refs)
        self.index = collections.defaultdict(set)
        for n in nodes:
            nid = n["id"]
            for f in DIRECT_FIELDS:
                for t in tokens(flat(n.get(f)), self.refs, items(n.get(f))):
                    self.index[t].add((nid, "node"))
            for t in tokens(flat(n.get("folded")), self.refs, items(n.get("folded"))):
                self.index[t].add((nid, "folded"))
        self.shas_known = {t[1] for t in self.index if t[0] == "sha"}
        # prose mentions of branch names in notes/verdict_quote/headline (names >= 8 chars, word-bounded)
        self.prose = {n["id"]: "\n".join(flat(n.get(f)) for f in NAME_TEXT_FIELDS).lower() for n in nodes}

    def sha_nodes(self, sha):
        hits = set()
        for tok in self.shas_known:
            if sha.startswith(tok):
                hits |= self.index[("sha", tok)]
        return hits


# ---------------------------------------------------------------- history
def load_commits(repo, before):
    fmt = "%H%x1f%aI%x1f%s%x1f%b%x1e"
    raw = git(repo, "log", "--all", "--format=" + fmt)
    commits = []
    for rec in raw.split("\x1e"):
        rec = rec.strip("\n")
        if not rec:
            continue
        sha, date, subj, body = rec.split("\x1f", 3)
        if not before or date[:10] < before:
            commits.append({"sha": sha, "date": date[:10], "subject": subj, "body": body})
    return commits


def find_main_ref(repo, configured=None):
    """The configured main ref, else the first of origin/main, main, master that exists; None if none does."""
    for ref in ([configured] if configured else list(DEFAULT_MAIN)):
        if git(repo, "rev-parse", "--verify", "-q", ref + "^{commit}").strip():
            return ref
    return None


def merge_branch(subject, extra=None):
    """The branch name a merge commit subject names, or None."""
    for rx in MERGE_SUBJECTS + ([re.compile(extra)] if extra else []):
        m = rx.search(subject)
        if m:
            return norm_name(m.group(1))
    return None


def ref_membership(repo, main_ref, merge_subject=None):
    """commit -> set of branch/tag names that carry it and the main ref does not, plus the names of the merges
    into the main ref (a merge's own name goes to the merge commit and every commit it brought in)."""
    names = collections.defaultdict(set)
    skip = set(MAIN_NAMES) | {norm_name(main_ref)}
    refs = git(repo, "for-each-ref", "--format=%(refname)", "refs/heads", "refs/remotes", "refs/tags").split()
    for r in refs:
        if r.startswith("refs/remotes/"):
            if r.endswith("/HEAD"):
                continue
            short = norm_name(r.split("/", 3)[3])
        else:
            short = norm_name(r.split("/", 2)[2])
        if short in skip:
            continue
        for c in git_lines(repo, "rev-list", r, "--not", main_ref):
            names[c].add(short)
        if r.startswith("refs/tags/"):
            tgt = git(repo, "rev-parse", r + "^{commit}").strip()
            if tgt:
                names[tgt].add(short)
    for line in git_lines(repo, "log", "--first-parent", "--merges", "--format=%H%x1f%P%x1f%s", main_ref):
        h, parents, subj = line.split("\x1f", 2)
        ps = parents.split()
        if len(ps) < 2:
            continue
        nm = merge_branch(subj, merge_subject)
        if not nm:
            continue
        names[h].add(nm)
        for c in git_lines(repo, "rev-list", ps[1], "--not", ps[0]):
            names[c].add(nm)
    return names


def classify(c, names, idx):
    """(bucket, ids, vias, strong_bucket). bucket is the strongest bucket with any match; strong_bucket the
    strongest bucket with a sha or branch match, so a weak node match cannot hide a strong folded one."""
    text = c["subject"] + "\n" + c["body"]
    toks = {t for t in tokens(text, idx.refs) if t[0] != "sha"}
    toks.add(("sha", c["sha"]))
    for nm in names.get(c["sha"], ()):
        toks.add(("branch", nm))
    hits = collections.defaultdict(set)  # bucket -> {(nid, via)}
    for nid, b in idx.sha_nodes(c["sha"]):
        hits[b].add((nid, "sha"))
    for t in toks:
        if t[0] == "sha":
            continue
        for nid, b in idx.index.get(t, ()):
            hits[b].add((nid, t[0]))
    # branch names in node prose (strict: long names only, word-bounded)
    for nm in names.get(c["sha"], ()):
        if len(nm) >= 8:
            pat = re.compile(r"(?<![\w-])" + re.escape(nm) + r"(?![\w-])")
            for nid, pr in idx.prose.items():
                if pat.search(pr):
                    hits["node"].add((nid, "branch"))
    strong = next((b for b in BUCKETS if any(v in STRONG for _, v in hits[b])), "none")
    for b in BUCKETS:
        if hits[b]:
            vias = sorted({v for _, v in hits[b]})
            ids = sorted({i for i, _ in hits[b]})
            return b, ids, vias, strong
    return "none", [], [], strong


def analyse(cfg, before=None):
    """(rows, nodes, main_ref): every commit with its bucket, strong bucket, node ids, vias and refs."""
    repo = cfg.root
    nodes = build.load_nodes(cfg.nodes_path)
    hist = cfg.history
    main_ref = find_main_ref(repo, hist.get("main"))
    if not main_ref:
        raise ValueError("no main ref found (set history.main in the config)" if not hist.get("main")
                         else f"ref '{hist['main']}' does not exist")
    mainset = set(git(repo, "rev-list", main_ref).split())
    idx = Index(nodes, hist.get("weak_refs"))
    names = ref_membership(repo, main_ref, hist.get("merge_subject"))
    src = {}
    for line in git_lines(repo, "log", "--all", "--source", "--format=%H\t%S"):
        h, _, s = line.partition("\t")
        src[h] = norm_name(s)
    rows = []
    for c in load_commits(repo, before):
        b, ids, vias, sb = classify(c, names, idx)
        c.update(bucket=b, ids=ids, vias=vias, strong_bucket=sb,
                 refs=sorted(names.get(c["sha"], ())) or (["main"] if c["sha"] in mainset else [src.get(c["sha"], "")]))
        rows.append(c)
    return rows, nodes, main_ref


def month_table(rows, key, title):
    months = sorted({c["date"][:7] for c in rows})
    tab = {m: collections.Counter() for m in months}
    for c in rows:
        tab[c["date"][:7]][c[key]] += 1
    out = [title, "", "| month | commits | node | folded | none |", "|---|---|---|---|---|"]
    for m in months:
        t = tab[m]
        out.append(f"| {m} | {sum(t.values())} | {t['node']} | {t['folded']} | {t['none']} |")
    tot = sum(tab.values(), collections.Counter())
    out.append(f"| all | {len(rows)} | {tot['node']} | {tot['folded']} | {tot['none']} |")
    return out, tot


def summary(rows, nodes, main_ref, before):
    hidden = [n for n in nodes if not n.get("overview") and n.get("state") not in FORWARD_STATES]
    nodiagram = [n for n in nodes if n.get("diagram") is False]
    undated = [n for n in nodes if not n.get("start")]
    scope = f"before {before}, " if before else "all history, "
    L = [f"# History gap ({scope}{len(rows)} commits, main ref {main_ref})", ""]
    strong_lines, st = month_table(rows, "strong_bucket", "## Strong matches (sha or branch/tag name)")
    L += strong_lines
    L.append("")
    L.append(f"{st['none']} of {len(rows)} commits have no strong match: the nodes do not account for them by sha or branch.")
    L.append("")
    all_lines, _ = month_table(rows, "bucket", "## All matches (strong and weak)")
    L += all_lines
    L.append("")
    L.append("Weak matches (configured reference patterns such as decision-record or PR numbers) are shown "
             "separately because one number is shared by many commits, so they inflate coverage.")
    L.append("")
    L.append("## Nodes not on the main page")
    L.append("")
    L.append(f"{len(nodes)} nodes. Overview false and not a forward state (not in the overview, plain-words or lane lists): "
             f"{len(hidden)}. `diagram: false` (also absent from the track trees): {len(nodiagram)}. "
             f"No start date: {len(undated)}.")
    bym = collections.Counter((str(n.get("start") or "none"))[:7] for n in hidden)
    L.append("Hidden by start month: " + (", ".join(f"{k}: {v}" for k, v in sorted(bym.items())) or "none"))
    return L


def write_outputs(rows, nodes, main_ref, before, out):
    out = Path(out)
    out.mkdir(parents=True, exist_ok=True)
    with open(out / "history_gap_commits.csv", "w", newline="", encoding="utf-8") as fh:
        w = csv.writer(fh)
        w.writerow(["sha", "date", "refs", "subject", "bucket", "strong_bucket", "node_ids", "via"])
        for c in sorted(rows, key=lambda r: r["date"]):
            w.writerow([c["sha"][:10], c["date"], ";".join(c["refs"])[:200], c["subject"],
                        c["bucket"], c["strong_bucket"], ";".join(c["ids"][:3]), ";".join(c["vias"])])
    lines = summary(rows, nodes, main_ref, before)
    (out / "history_gap_summary.md").write_text("\n".join(lines) + "\n", encoding="utf-8", newline="\n")
    return lines


def _date_arg(s):
    try:
        datetime.date.fromisoformat(s)
    except ValueError:
        raise argparse.ArgumentTypeError(f"{s!r} is not a YYYY-MM-DD date") from None
    return s


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    tree_config.add_config_arg(ap)
    ap.add_argument("--out", required=True, type=Path, help="directory for the csv and the summary (outside the repo)")
    ap.add_argument("--before", type=_date_arg, help="only commits with an author date before this day (default: all)")
    a = ap.parse_args(argv)
    tree_config.utf8_stdio()
    cfg = tree_config.load_for_cli(a.config, "history_gap")
    if cfg is None:
        return 2
    try:
        rows, nodes, main_ref = analyse(cfg, a.before)
    except (OSError, ValueError, yaml.YAMLError) as e:
        print(f"history_gap: {e}", file=sys.stderr)
        return 2
    lines = write_outputs(rows, nodes, main_ref, a.before, a.out)
    print("\n".join(lines))
    return 0


if __name__ == "__main__":
    sys.exit(main())
