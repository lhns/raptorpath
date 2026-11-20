#!/usr/bin/env python3
"""Measure how much pre-November history docs/roadmap/nodes.yaml accounts for.

Mechanical only, no judgement. Every commit on every ref and tag dated before
--before (default 2026-11-01, author date) is matched against the nodes by:
  sha        short or full sha in a node's text
  branch     branch or tag name: branches that carry the commit and not main,
             the "merge: NAME - ..." subject of the main merge that brought
             it in, and tags that point at it
  status     a status section number (§N) in the commit message vs "status §N"
  adr        an ADR number in the commit message vs "ADR-NNNN"
Text searched per node: evidence, folded, notes, verdict_quote, headline.
Buckets, strongest first:
  node     matched through a field other than folded
  folded   matched only through a folded item
  dropped  matched only through the top-level dropped list
  none     no match
Each row also records how it matched ("via", e.g. sha, branch, status, adr);
status and adr matches are weak (a section or ADR number is shared by many
commits), so the summary also counts rows with a strong match (sha or branch).

Also reports nodes that the main page does not show (overview false and not a
forward state, see build.py r_overview / r_plain), and ADR numbers without a
file in docs/adr, with what git history says about them.

Usage (from inside the repo):
  python tools/roadmap/history_gap.py --out <dir>
Writes <dir>/history_gap_commits.csv and <dir>/history_gap_summary.md.
Output is not meant to be committed.
"""
import argparse
import collections
import csv
import os
import re
import subprocess
import sys
import tempfile

import yaml

NODES = "docs/roadmap/nodes.yaml"
FORWARD_STATES = ("in-progress", "next", "blocked", "parked")  # same as build.py
DIRECT_FIELDS = ("evidence", "notes", "verdict_quote", "headline")
STRONG = ("sha", "branch")


def git(*args):
    r = subprocess.run(["git", "-c", "core.quotepath=off", *args], capture_output=True,
                       encoding="utf-8", errors="replace")
    return r.stdout


def git_lines(*args):
    p = subprocess.Popen(["git", "-c", "core.quotepath=off", *args], stdout=subprocess.PIPE,
                         stderr=subprocess.DEVNULL, encoding="utf-8", errors="replace")
    for line in p.stdout:
        yield line.rstrip("\n")
    p.wait()


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
STATUS_RE = re.compile(r"(?:status\s*)?§\s?(\d+)")
ADR_RE = re.compile(r"ADR[- ]?0*(\d{1,4})", re.I)
NAME_RE = re.compile(r"^(?:branch|tag)\s+(.+)$")


def has_digit(s):
    return any(c.isdigit() for c in s)


def tokens(text, item_names=()):
    """(kind, key) tokens of free text. Names come only from whole items (branch X / tag X)."""
    out = set()
    for m in SHA_RE.findall(text):
        if has_digit(m) and any(c in "abcdef" for c in m):
            out.add(("sha", m))
    for m in COMMIT_SHA_RE.findall(text):
        out.add(("sha", m.lower()))
    for m in STATUS_RE.finditer(text):
        # "paper §10" is not a status section
        pre = text[max(0, m.start() - 8):m.start()].lower()
        if "paper" in pre:
            continue
        out.add(("status", m.group(1)))
    for m in ADR_RE.finditer(text):
        out.add(("adr", str(int(m.group(1)))))
    for it in item_names:
        m = NAME_RE.match(it.strip())
        if m:
            out.add(("branch", norm_name(m.group(1))))
    return out


def norm_name(n):
    n = n.strip().strip("`'\"").rstrip(":,;.")
    for p in ("github/", "origin/", "refs/heads/", "refs/tags/", "refs/remotes/"):
        if n.startswith(p):
            n = n[len(p):]
    return n.lower()


# ---------------------------------------------------------------- nodes
NAME_TEXT_FIELDS = ("notes", "verdict_quote", "headline")


class Index:
    """token -> {(node id, bucket)}; bucket is node|folded|dropped."""

    def __init__(self, nodes, dropped=()):
        self.index = collections.defaultdict(set)
        for n in nodes:
            nid = n["id"]
            for f in DIRECT_FIELDS:
                for t in tokens(flat(n.get(f)), items(n.get(f))):
                    self.index[t].add((nid, "node"))
            for t in tokens(flat(n.get("folded")), items(n.get("folded"))):
                self.index[t].add((nid, "folded"))
        for d in dropped:
            for t in tokens(flat(d), [str(d.get("src", "")), ]):
                self.index[t].add(("dropped:" + str(d.get("src", ""))[:40], "dropped"))
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
BEFORE = None


def load_commits(before):
    fmt = "%H%x1f%aI%x1f%s%x1f%b%x1e"
    raw = git("log", "--all", "--format=" + fmt)
    commits = []
    for rec in raw.split("\x1e"):
        rec = rec.strip("\n")
        if not rec:
            continue
        sha, date, subj, body = rec.split("\x1f", 3)
        if date[:10] < before:
            commits.append({"sha": sha, "date": date[:10], "subject": subj, "body": body})
    return commits


def main_set():
    ref = "github/main" if git("rev-parse", "--verify", "-q", "github/main").strip() else "main"
    return ref, set(git("rev-list", ref).split())


def ref_membership(main_ref, mainset):
    """commit -> set of branch/tag names that carry it and main does not; tag -> commit; merge-subject names."""
    names = collections.defaultdict(set)
    refs = git("for-each-ref", "--format=%(refname)", "refs/heads", "refs/remotes", "refs/tags").split()
    for r in refs:
        short = norm_name(r.split("/", 2)[2] if r.startswith("refs/") else r)
        if r.startswith("refs/remotes/") and r.endswith("/HEAD"):
            continue
        if r.startswith("refs/remotes/"):
            short = norm_name(r.split("/", 3)[3])
        if short in ("main", "master"):
            continue
        for c in git_lines("rev-list", r, "--not", main_ref):
            names[c].add(short)
        if r.startswith("refs/tags/"):
            tgt = git("rev-parse", r + "^{commit}").strip()
            if tgt:
                names[tgt].add(short)
    # main merges: every commit a merge brought in carries the merge's branch name
    for line in git_lines("log", "--first-parent", "--merges", "--format=%H%x1f%P%x1f%s", main_ref):
        h, parents, subj = line.split("\x1f", 2)
        ps = parents.split()
        if len(ps) < 2:
            continue
        m = re.match(r"merge:\s*(.+?)(?:\s+-\s+|\s+into main\b|$)", subj, re.I) or \
            re.match(r"Merge (?:branch |remote-tracking branch )?'?([^'\s]+)", subj)
        if not m:
            continue
        nm = norm_name(m.group(1))
        names[h].add(nm)
        for c in git_lines("rev-list", ps[1], "--not", ps[0]):
            names[c].add(nm)
    return names


def classify(c, names, idx):
    """(bucket, ids, vias, strong_bucket). bucket is the strongest bucket with any match; strong_bucket the
    strongest bucket with a sha or branch match, so a weak node match cannot hide a strong folded one."""
    text = c["subject"] + "\n" + c["body"]
    toks = {t for t in tokens(text) if t[0] != "sha"}
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
    strong = next((b for b in ("node", "folded", "dropped") if any(v in STRONG for _, v in hits[b])), "none")
    for b in ("node", "folded", "dropped"):
        if hits[b]:
            vias = sorted({v for _, v in hits[b]})
            ids = sorted({i for i, _ in hits[b]})
            return b, ids, vias, strong
    return "none", [], [], strong


def adr_report():
    present = {}
    for f in os.listdir("docs/adr"):
        m = re.match(r"(\d{4})-", f)
        if m:
            present[int(m.group(1))] = f
    ever = collections.defaultdict(set)
    for line in git_lines("log", "--all", "--name-only", "--format=", "--", ":(glob)**/adr/*"):
        m = re.search(r"(?:^|/)(\d{4})-[^/]*$", line)
        if m:
            ever[int(m.group(1))].add(line)
    deleted = set()
    for line in git_lines("log", "--all", "--diff-filter=D", "--name-only", "--format=", "--", ":(glob)**/adr/*"):
        m = re.search(r"(?:^|/)(\d{4})-[^/]*$", line)
        if m:
            deleted.add(int(m.group(1)))
    renamed = set()
    for line in git_lines("log", "--all", "-M", "--diff-filter=R", "--name-status", "--format=", "--", ":(glob)**/adr/*"):
        parts = line.split("\t")
        if len(parts) == 3:
            for p in parts[1:]:
                m = re.search(r"(?:^|/)(\d{4})-[^/]*$", p)
                if m:
                    renamed.add(int(m.group(1)))
    mention = collections.Counter()
    for sub in git_lines("log", "--all", "--format=%s%n%b"):
        for m in ADR_RE.finditer(sub):
            mention[int(m.group(1))] += 1
    rows = []
    for n in range(1, 84):
        if n in present:
            continue
        rows.append((n, n in ever, n in deleted, n in renamed, mention.get(n, 0)))
    return present, rows


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--before", default="2026-11-01")
    ap.add_argument("--nodes", default=NODES, help="nodes.yaml to measure (relative to the repo root)")
    ap.add_argument("--out", default=os.path.join(tempfile.gettempdir(), "history_gap"))
    a = ap.parse_args()
    os.makedirs(a.out, exist_ok=True)
    os.chdir(subprocess.run(["git", "rev-parse", "--show-toplevel"], capture_output=True,
                            text=True, encoding="utf-8").stdout.strip())
    data = yaml.safe_load(open(a.nodes, encoding="utf-8"))
    nodes = data["nodes"]
    idx = Index(nodes, data.get("dropped") or [])

    main_ref, mainset = main_set()
    commits = load_commits(a.before)
    names = ref_membership(main_ref, mainset)
    src = {}
    for line in git_lines("log", "--all", "--source", "--format=%H\t%S"):
        h, _, s = line.partition("\t")
        src[h] = norm_name(s)

    rows = []
    for c in commits:
        b, ids, vias, sb = classify(c, names, idx)
        c.update(bucket=b, ids=ids, vias=vias, strong_bucket=sb,
                 refs=sorted(names.get(c["sha"], ())) or (["main"] if c["sha"] in mainset else [src.get(c["sha"], "")]))
        rows.append(c)

    with open(os.path.join(a.out, "history_gap_commits.csv"), "w", newline="", encoding="utf-8") as fh:
        w = csv.writer(fh)
        w.writerow(["sha", "date", "refs", "subject", "bucket", "node_ids", "via"])
        for c in sorted(rows, key=lambda r: r["date"]):
            w.writerow([c["sha"][:10], c["date"], ";".join(c["refs"])[:200], c["subject"],
                        c["bucket"], ";".join(c["ids"][:3]), ";".join(c["vias"])])

    months = sorted({c["date"][:7] for c in rows})
    tab = {m: collections.Counter() for m in months}
    strong = {m: collections.Counter() for m in months}
    for c in rows:
        tab[c["date"][:7]][c["bucket"]] += 1
        strong[c["date"][:7]][c["strong_bucket"]] += 1

    # hidden nodes
    hidden = [n for n in nodes if not n.get("overview") and n.get("state") not in FORWARD_STATES]
    nodiagram = [n for n in nodes if n.get("diagram") is False]
    undated = [n for n in nodes if not n.get("start")]
    present, missing = adr_report()

    L = []
    L.append(f"# History gap (commits before {a.before}, {len(rows)} commits, ref {main_ref})\n")
    L.append("## Buckets per month (all match kinds)\n")
    L.append("| month | commits | node | folded | dropped | none | none % |")
    L.append("|---|---|---|---|---|---|---|")
    for m in months:
        t = tab[m]
        n = sum(t.values())
        L.append(f"| {m} | {n} | {t['node']} | {t['folded']} | {t['dropped']} | {t['none']} | {100 * t['none'] // max(n, 1)} |")
    tot = sum(tab.values(), collections.Counter())
    L.append(f"| all | {len(rows)} | {tot['node']} | {tot['folded']} | {tot['dropped']} | {tot['none']} | {100 * tot['none'] // max(len(rows), 1)} |")
    L.append("\n## Buckets per month (strong matches only: sha or branch/tag name)\n")
    L.append("| month | commits | node | folded | dropped | none |")
    L.append("|---|---|---|---|---|---|")
    for m in months:
        t = strong[m]
        L.append(f"| {m} | {sum(t.values())} | {t['node']} | {t['folded']} | {t['dropped']} | {t['none']} |")
    st = sum(strong.values(), collections.Counter())
    L.append(f"| all | {len(rows)} | {st['node']} | {st['folded']} | {st['dropped']} | {st['none']} |")
    L.append("\n## Nodes not on the main page\n")
    L.append(f"{len(nodes)} nodes. Overview false and not a forward state (not in the overview, plain-words or lane lists): {len(hidden)}. "
             f"`diagram: false` (also absent from the track trees): {len(nodiagram)}. No start date: {len(undated)}.")
    bym = collections.Counter((n.get("start") or "none")[:7] for n in hidden)
    L.append("Hidden by start month: " + ", ".join(f"{k}: {v}" for k, v in sorted(bym.items())))
    L.append("\n## ADR numbers 0001-0083 without a file in docs/adr\n")
    L.append(f"{len(present)} files; {len(missing)} numbers missing.\n")
    L.append("| ADR | ever a file in git | deleted | renamed | commits mentioning it |")
    L.append("|---|---|---|---|---|")
    for n, ev, de, rn, mc in missing:
        L.append(f"| {n:04d} | {'yes' if ev else 'no'} | {'yes' if de else 'no'} | {'yes' if rn else 'no'} | {mc} |")
    with open(os.path.join(a.out, "history_gap_summary.md"), "w", encoding="utf-8") as fh:
        fh.write("\n".join(L) + "\n")
    sys.stdout.write("\n".join(L) + "\n")


if __name__ == "__main__":
    main()
