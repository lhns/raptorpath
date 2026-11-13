#!/usr/bin/env python3
"""Audit what past development is missing from docs/roadmap/nodes.yaml.

Extraction only: lists candidates (ADRs, status sections, archive tags,
unmerged branches, uncovered commit clusters, paper section-10 rows) that no
node cites, with facts. Grouping is the owner's call.

Writes docs/roadmap/history/missing-candidates.{md,yaml}. Git output is
streamed line by line; nothing like `git log --all -p` is held in memory.

Run from anywhere inside the repo:  python tools/roadmap/history_audit.py
"""
import collections
import math
import os
import re
import subprocess
import sys

import yaml

ROOT = subprocess.run(["git", "rev-parse", "--show-toplevel"], capture_output=True,
                      text=True, encoding="utf-8").stdout.strip()
os.chdir(ROOT)
NODES = "docs/roadmap/nodes.yaml"
OUT_MD = "docs/roadmap/history/missing-candidates.md"
OUT_YAML = "docs/roadmap/history/missing-candidates.yaml"
MAIN_REF = "github/main"
STRICT_FIELDS = ("evidence", "notes", "title")
BROAD_FIELDS = STRICT_FIELDS + ("folded", "verdict_quote", "headline", "plain",
                                "id", "replaced_by", "order_reason")


def git_lines(*args):
    """Stream git stdout line by line."""
    p = subprocess.Popen(["git", "-c", "core.quotepath=off", *args], stdout=subprocess.PIPE,
                         stderr=subprocess.DEVNULL, encoding="utf-8", errors="replace")
    for line in p.stdout:
        yield line.rstrip("\n")
    p.wait()


def git_out(*args):
    r = subprocess.run(["git", "-c", "core.quotepath=off", *args], capture_output=True,
                       encoding="utf-8", errors="replace")
    return r.stdout


def flat(v):
    if v is None:
        return ""
    if isinstance(v, (list, tuple)):
        return "\n".join(flat(x) for x in v)
    if isinstance(v, dict):
        return "\n".join(flat(x) for x in v.values())
    return str(v)


# ---------------------------------------------------------------- nodes
data = yaml.safe_load(open(NODES, encoding="utf-8"))
NODE_LIST = data["nodes"]
STRICT = "\n".join(flat(n.get(f)) for n in NODE_LIST for f in STRICT_FIELDS)
BROAD = "\n".join(flat(n.get(f)) for n in NODE_LIST for f in BROAD_FIELDS)
HEX = lambda text: {t.lower() for t in re.findall(r"\b[0-9a-f]{7,40}\b", text)}
STRICT_HEX, BROAD_HEX = HEX(STRICT), HEX(BROAD)


def hex_cited(sha, pool):
    s = sha.lower()
    return any(h.startswith(s[:7]) or s.startswith(h) for h in pool)


def adr_numbers(text):
    """ADR numbers cited: 'ADR-0033', 'ADR 0033', 'ADR-33', '0033-slug', comma/slash/and lists after ADR(s).
    Ranges ('0052..0067', '0024-0030') are NOT expanded: nodes use them as date-placeholder notes."""
    nums = set()
    for m in re.finditer(r"\bADRs?[-\s]*(0*\d+(?:\s*(?:,|/|&|and|\+)\s*(?:ADR[-\s]*)?0*\d+)*)(?!\.\.)", text, re.I):
        nums.update(int(t) for t in re.findall(r"\d+", m.group(1)))
    for m in re.finditer(r"\b(\d{4})-[a-z][\w-]*", text):
        nums.add(int(m.group(1)))
    return nums


STRICT_ADR, BROAD_ADR = adr_numbers(STRICT), adr_numbers(BROAD)


def status_cites(text):
    """(strong, weak) section ids: 'status §4' / 'status 4' vs a bare '§4' not prefixed by 'paper'."""
    strong, weak = set(), set()
    for m in re.finditer(r"\bstatus(?:\.md)?\s*(?:§|section\s+)?\s*(\d+[a-z]?)\b", text, re.I):
        strong.add(m.group(1).lower())
    for m in re.finditer(r"(?<!paper )(?<!paper)(?<!Paper )(?<!diag36 )§\s*(\d+[a-z]?)\b", text):
        weak.add(m.group(1).lower())
    # lists after a strong cite: 'status §4/§5', '§41b, §41c'
    for m in re.finditer(r"\bstatus(?:\.md)?\s*§[\d\w]+((?:\s*(?:,|/|&|and)\s*§?\s*\d+[a-z]?)+)", text, re.I):
        for t in re.findall(r"\d+[a-z]?", m.group(1)):
            strong.add(t.lower())
    return strong, weak


STRICT_ST, BROAD_ST = status_cites(STRICT), status_cites(BROAD)

# ---- title-word matching to existing nodes
STOP = set("raptorpath message mode the and for with from that this into over under after before when what which are not but its via per "
           "off all any one two new old use used using only more less than then also each both such".split())


def tokens(s):
    out = set()
    for w in re.findall(r"[A-Za-z][A-Za-z0-9]+", s.lower().replace("_", " ")):
        if len(w) >= 4 and w not in STOP:
            out.add(w[:-1] if w.endswith("s") and len(w) > 4 else w)
    return out


NODE_TOK = {n["id"]: tokens(n["title"] + " " + n["id"].replace("-", " ")) for n in NODE_LIST}
DF = collections.Counter(t for ts in NODE_TOK.values() for t in ts)
NN = len(NODE_LIST)


def possible_match(text):
    """Best node by shared title words weighted by rarity. Returns 'node-id (w1, w2)' or ''."""
    tk = tokens(text)
    best, score, shared = None, 0.0, []
    for nid, nt in NODE_TOK.items():
        sh = [t for t in tk & nt if DF[t] <= 0.15 * NN]
        sc = sum(math.log(NN / DF[t]) for t in sh)
        if sc > score and (len(sh) >= 2 or (len(sh) == 1 and DF[sh[0]] <= 3)):
            best, score, shared = nid, sc, sh
    return f"{best} ({', '.join(sorted(shared))})" if best else ""


def month_of(d):
    return d[:7]


# ---------------------------------------------------------------- 1. ADRs
def collect_adrs():
    ev = collections.defaultdict(list)  # num -> [(date, status, path, sha)]
    cur = None
    for line in git_lines("log", "--all", "--name-status", "-M", "--format=C\t%H\t%cs", "--", "*adr/*.md"):
        if line.startswith("C\t"):
            _, sha, d = line.split("\t")
            cur = (sha, d)
        elif line and cur:
            parts = line.split("\t")
            st, paths = parts[0][0], parts[1:]
            path = paths[-1]
            m = re.match(r"(\d{3,4})-.*\.md$", os.path.basename(path))
            if not m:
                continue
            ev[int(m.group(1))].append((cur[1], st, path, cur[0], paths[0]))
    head_files = {int(re.match(r"(\d{3,4})-", os.path.basename(p)).group(1)): p
                  for p in git_lines("ls-tree", "-r", "--name-only", "HEAD")
                  if re.search(r"adr/\d{3,4}-.*\.md$", p)}
    rows = []
    for num, evs in sorted(ev.items()):
        evs.sort()
        added = min(e[0] for e in evs if e[1] in "AR")
        dels = [e for e in evs if e[1] == "D"]
        in_head = num in head_files
        deleted = None if in_head else (max(e[0] for e in dels) if dels else None)
        # last existing version: HEAD file, else the parent of the last deletion, else last non-D event
        text, slug_path, src = "", None, ""
        if in_head:
            slug_path, ref = head_files[num], "HEAD"
        elif dels:
            d = sorted(dels)[-1]
            slug_path, ref = d[2], d[3] + "^"
        else:
            e = [x for x in evs if x[1] != "D"][-1]
            slug_path, ref = e[2], e[3]
        text = git_out("show", f"{ref}:{slug_path}")
        if not text and not in_head:  # deletion on a side branch whose parent differs; try any earlier event
            for e in reversed(evs):
                if e[1] != "D":
                    text = git_out("show", f"{e[3]}:{e[2]}")
                    if text:
                        break
        title, status, summary = adr_facts(text, slug_path)
        paths = sorted({e[2] for e in evs})
        rows.append({
            "num": num, "title": title, "added": added, "deleted": deleted, "on_main": in_head,
            "status": status, "summary": summary, "paths": paths,
            "cited": num in STRICT_ADR,
            "cited_other_fields": (num in BROAD_ADR) and num not in STRICT_ADR,
            "possible_match": "" if num in STRICT_ADR else possible_match(title + " " + slug_path.replace("-", " ")),
        })
    return rows


def adr_facts(text, path):
    title = ""
    lines = text.splitlines()
    for l in lines:
        if l.startswith("# "):
            title = re.sub(r"^#\s*(ADR[-\s]*\d+\s*[:\u2014\u2013.-]*\s*)?", "", l).strip()
            break
    if not title:
        title = re.sub(r"^\d+-|\.md$", "", os.path.basename(path)).replace("-", " ")
    status = ""
    for i, l in enumerate(lines):
        m = re.match(r"^(?:#+\s*|[-*]\s*)?\**Status\**(?::\**|\s*$)\s*(.*)$", l, re.I)
        if m:
            s = m.group(1).strip(" *")
            if not s:  # '## Status' heading: take next non-blank line
                s = next((x.strip(" *") for x in lines[i + 1:] if x.strip()), "")
            status = s
    summary = ""
    for sec in ("context", "decision", "problem", "background"):
        for i, l in enumerate(lines):
            if re.match(rf"^#+\s*{sec}\b", l, re.I):
                body = []
                for x in lines[i + 1:]:
                    if x.startswith("#"):
                        break
                    if x.strip():
                        body.append(x.strip())
                    elif body:
                        break
                summary = " ".join(body)
                break
        if summary:
            break
    if not summary:  # first paragraph after the title
        body = []
        for l in lines[1:]:
            if l.startswith("#") or re.match(r"^\s*(\*\*)?Status\**\s*:", l, re.I):
                if body:
                    break
                continue
            if l.strip():
                body.append(l.strip())
            elif body:
                break
        summary = " ".join(body)
    summary = re.sub(r"\s+", " ", re.sub(r"[`*]", "", summary))
    m = re.match(r"(.{40,260}?[.!?])(\s|$)", summary)
    return title, status[:140], (m.group(1) if m else summary[:240])


# ---------------------------------------------------------------- 2. status sections
def collect_status():
    sec = {}  # id -> dict
    cur = None
    for line in git_lines("log", "--all", "-p", "--format=C\t%H\t%cs", "--",
                          "*status.md"):
        if line.startswith("C\t"):
            _, sha, d = line.split("\t")
            cur = (sha, d)
        elif line.startswith("+## ") and cur:
            m = re.match(r"\+## (\d+[a-z]?)\.\s+(.*)", line)
            if not m:
                continue
            sid, head = m.group(1).lower(), m.group(2).strip()
            s = sec.setdefault(sid, {"id": sid, "first": cur[1], "first_sha": cur[0][:10],
                                     "heading": head, "variants": {head}})
            s["variants"].add(head)
            if cur[1] < s["first"]:
                s["first"], s["first_sha"], s["heading"] = cur[1], cur[0][:10], head
    main_ids = set()
    for l in git_lines("show", "HEAD:docs/status.md"):
        m = re.match(r"## (\d+[a-z]?)\.\s", l)
        if m:
            main_ids.add(m.group(1).lower())
    rows = []
    for sid, s in sec.items():
        strong = sid in STRICT_ST[0]
        weak = sid in STRICT_ST[1]
        kind = "status-prefixed" if strong else ("bare section sign" if weak else "")
        rows.append({
            "section": sid, "heading": s["heading"][:160], "first_seen": s["first"], "first_sha": s["first_sha"],
            "heading_variants": len(s["variants"]), "on_main": sid in main_ids,
            "cited": bool(kind), "cited_how": kind,
            "cited_other_fields": (not kind) and (sid in BROAD_ST[0] or sid in BROAD_ST[1]),
            "possible_match": "" if kind else possible_match(s["heading"]),
        })
    rows.sort(key=lambda r: (int(re.match(r"\d+", r["section"]).group()), r["section"]))
    return rows


# ---------------------------------------------------------------- 3. tags
def collect_tags():
    rows = []
    fmt = "%(refname:short)\t%(objecttype)\t%(creatordate:short)\t%(contents:subject)\t%(objectname)\t%(*objectname)"
    for line in git_lines("for-each-ref", "refs/tags/archive", f"--format={fmt}"):
        name, typ, d, subj, obj, deref = (line.split("\t") + [""] * 6)[:6]
        commit = deref or obj
        if typ == "commit":
            subj = git_out("log", "-1", "--format=%s", commit).strip()
        bare = name.split("/", 1)[1]
        full = re.search(re.escape(name), STRICT) is not None
        barec = re.search(r"(?<![\w])" + re.escape(bare) + r"(?![\w])", STRICT) is not None
        sha = hex_cited(commit, STRICT_HEX)
        how = "full tag name" if full else ("bare name" if barec else ("commit sha" if sha else ""))
        broad = (re.search(re.escape(bare), BROAD) is not None) or hex_cited(commit, BROAD_HEX)
        rows.append({"tag": name, "type": "annotated" if typ == "tag" else "lightweight", "date": d,
                     "subject": subj[:160], "commit": commit[:10], "cited": bool(how), "cited_how": how,
                     "cited_other_fields": broad and not how,
                     "possible_match": "" if how else possible_match(bare)})
    rows.sort(key=lambda r: r["date"])
    return rows


# ---------------------------------------------------------------- 4. branches
def collect_branches():
    rows = []
    for ref in git_lines("branch", "-r", "--no-merged", MAIN_REF, "--format=%(refname:short)"):
        if not ref.startswith("github/") or ref.endswith("/HEAD"):
            continue
        name = ref[len("github/"):]
        commits = [l.split("\t", 2) for l in git_lines("log", f"{MAIN_REF}..{ref}", "--format=%H\t%cs\t%s")]
        if not commits:
            continue
        first, last = commits[-1], commits[0]
        cited_name = re.search(r"(?<![\w/-])" + re.escape(name) + r"(?![\w-])", STRICT) is not None
        cited_sha = [c[0][:8] for c in commits if hex_cited(c[0], STRICT_HEX)]
        how = "branch name" if cited_name else ("commit sha" if cited_sha else "")
        broad = re.search(re.escape(name), BROAD) is not None or any(hex_cited(c[0], BROAD_HEX) for c in commits)
        rows.append({"branch": name, "last_date": last[1], "last_subject": last[2][:150], "unique_commits": len(commits),
                     "first_date": first[1], "cited": bool(how), "cited_how": how,
                     "cited_other_fields": broad and not how,
                     "possible_match": "" if how else possible_match(name + " " + last[2])})
    rows.sort(key=lambda r: r["last_date"])
    return rows


# ---------------------------------------------------------------- 0/5. months, commit clusters
def node_months():
    c = collections.Counter(month_of(n["start"]) for n in NODE_LIST if n.get("start"))
    return c, sum(1 for n in NODE_LIST if not n.get("start"))


def collect_commits():
    out = []
    for l in git_lines("log", "--all", "--format=%H\t%cs\t%s"):
        sha, d, s = l.split("\t", 2)
        out.append((sha, d, s))
    return out


GENERIC = {"merge", "status", "docs", "doc", "fix", "test", "tests", "feat", "chore", "refactor", "wip", "tools",
           "add", "update", "harness", "paper", "roadmap", "revert", "bench", "perf", "build"}


def cluster_keys(subject):
    keys = []
    m = re.match(r"^(\w+)\(([^)]+)\)!?:", subject)
    if m:
        keys.append(m.group(2).lower())
    for f in re.findall(r"RWM_[A-Z0-9_]+", subject):
        keys.append(f.lower())
    pre = re.split(r"\s[-\u2014\u2013]\s|:\s|:$", subject, maxsplit=1)[0]
    pre = re.sub(r"[\u00a7#]\s*\w+|\b\d[\w.]*\b|\(.*?\)", " ", pre)
    words = [w for w in re.findall(r"[A-Za-z][\w'-]*", pre.lower())]
    if words:
        if words[0] in GENERIC:
            rest = re.split(r":\s*", subject, maxsplit=1)
            tail = rest[1] if len(rest) == 2 else ""
            tw = [w for w in re.findall(r"[A-Za-z][\w'-]*", re.sub(r"\b\d[\w.]*\b", " ", tail.lower()))
                  if w not in STOP]
            if tw:
                keys.append(f"{words[0]}: {' '.join(tw[:2])}")
        else:
            keys.append(" ".join(words[:3]))
    return keys


def collect_clusters(commits, months):
    per_month = collections.defaultdict(lambda: collections.defaultdict(list))
    for sha, d, s in commits:
        if month_of(d) in months:
            for k in set(cluster_keys(s)):
                per_month[month_of(d)][k].append((sha, d, s))
    ranges = [(n["id"], n["start"], n.get("end") or n["start"], tokens(n["title"] + " " + n["id"].replace("-", " ")))
              for n in NODE_LIST if n.get("start")]
    rows = []
    for mo, cl in sorted(per_month.items()):
        for key, cs in cl.items():
            if len(cs) < 3:
                continue
            kw = tokens(key.replace(":", " "))
            matching = [r for r in ranges if kw & r[3]] if kw else []
            unc = [c for c in cs if not any(r[1] <= c[1] <= r[2] for r in matching)]
            if len(unc) < 3:
                continue
            rows.append({"month": mo, "cluster": key, "commits": len(cs), "uncovered": len(unc),
                         "first": min(c[1] for c in unc), "last": max(c[1] for c in unc),
                         "examples": [f"{c[0][:8]} {c[2][:100]}" for c in unc[:3]],
                         "possible_match": ", ".join(sorted({r[0] for r in matching})[:4]) if matching else ""})
    rows.sort(key=lambda r: (r["month"], -r["uncovered"]))
    return rows


# ---------------------------------------------------------------- 6. paper section 10
def collect_paper():
    rows, in10, group = [], False, ""
    for l in git_lines("show", "HEAD:docs/fec-arq-model.md"):
        if l.startswith("## "):
            in10 = l.startswith("## 10.")
            continue
        if not in10 or not l.startswith("|") or l.startswith("|---") or l.startswith("| mechanism"):
            continue
        cells = [c.strip() for c in re.split(r"(?<!\\)\|", l.strip().strip("|"))]
        if len(cells) < 4:
            continue
        name = cells[0]
        if all(not c for c in cells[1:]):
            group = name.strip("*")
            continue
        flags = re.findall(r"RWM_[A-Z0-9_]+", name)
        hashes = re.findall(r"`([0-9a-f]{7,40})`", cells[2])
        clean = re.sub(r"[`*]", "", name)
        norm = lambda s: re.sub(r"[^a-z0-9]+", " ", s.lower()).strip()
        nclean = norm(clean)
        by_name = len(nclean) > 8 and nclean in norm(BROAD)
        by_name_strict = len(nclean) > 8 and nclean in norm(STRICT)
        by_flag = [f for f in flags if f in BROAD]
        by_flag_strict = [f for f in flags if f in STRICT]
        by_hash = [h for h in hashes if hex_cited(h, BROAD_HEX)]
        by_hash_strict = [h for h in hashes if hex_cited(h, STRICT_HEX)]
        how = ("name " if by_name_strict else "") + ("flag " if by_flag_strict else "")
        how_broad = ("name " if by_name else "") + ("flag " if by_flag else "")
        weak = bool(by_hash) and not how_broad
        rows.append({"group": group, "mechanism": clean[:150], "why": re.sub(r"[`*]", "", cells[1])[:200],
                     "commit": ", ".join(hashes) or cells[2].strip("`"), "status": re.sub(r"[`*]", "", cells[3])[:100],
                     "cited": bool(how.strip()), "cited_how": how.strip(),
                     "cited_other_fields": bool(how_broad.strip()) and not how.strip(),
                     "cited_commit_only": weak,
                     "possible_match": "" if (how_broad.strip() or weak) else possible_match(clean)})
    return rows


# ---------------------------------------------------------------- report
def esc(s):
    return str(s).replace("|", "\\|").replace("\n", " ")


def table(headers, rows):
    out = ["| " + " | ".join(headers) + " |", "|" + "|".join("---" for _ in headers) + "|"]
    out += ["| " + " | ".join(esc(c) for c in r) + " |" for r in rows]
    return "\n".join(out)


def yn(r):
    if r["cited"]:
        return "yes" + (f" ({r['cited_how']})" if r.get("cited_how") else "")
    return "broad-only" if r.get("cited_other_fields") else "NO"


def main():
    commits = collect_commits()
    nmo, nodate = node_months()
    cmo = collections.Counter(month_of(d) for _, d, _ in commits)
    months = sorted(set(nmo) | set(cmo))
    month_rows = [{"month": m, "nodes": nmo.get(m, 0), "commits": cmo.get(m, 0),
                   "commits_per_node": round(cmo[m] / nmo[m], 1) if nmo.get(m) else None} for m in months]
    heavy = [r["month"] for r in month_rows if r["commits"] and (r["nodes"] == 0 or r["commits_per_node"] > 15)]
    # fill empty months between (May-June) for display
    adrs, status, tags = collect_adrs(), collect_status(), collect_tags()
    branches, paper = collect_branches(), collect_paper()
    clusters = collect_clusters(commits, set(heavy))

    def miss(rows):
        return [r for r in rows if not r["cited"]]

    summary = {
        "nodes_total": len(NODE_LIST), "nodes_without_start": nodate, "commits_all_refs": len(commits),
        "adrs": {"total": len(adrs), "uncited_strict": len(miss(adrs)),
                 "uncited_even_broad": sum(1 for r in adrs if not r["cited"] and not r["cited_other_fields"]),
                 "uncited_and_deleted": sum(1 for r in miss(adrs) if r["deleted"])},
        "status_sections": {"total": len(status), "uncited_strict": len(miss(status)),
                            "uncited_even_broad": sum(1 for r in status if not r["cited"] and not r["cited_other_fields"]),
                            "uncited_and_gone_from_main": sum(1 for r in miss(status) if not r["on_main"]),
                            "cited_only_by_bare_section_sign": sum(1 for r in status if r["cited_how"] == "bare section sign")},
        "archive_tags": {"total": len(tags), "uncited_strict": len(miss(tags)),
                         "uncited_even_broad": sum(1 for r in tags if not r["cited"] and not r["cited_other_fields"])},
        "unmerged_branches": {"total": len(branches), "uncited_strict": len(miss(branches)),
                              "uncited_even_broad": sum(1 for r in branches if not r["cited"] and not r["cited_other_fields"])},
        "uncovered_commit_clusters": {"total": len(clusters), "months_examined": heavy,
                                      "uncovered_commits_in_clusters": sum(r["uncovered"] for r in clusters)},
        "paper_s10_rows": {"total": len(paper), "uncited_strict": len(miss(paper)),
                           "uncited_even_broad": sum(1 for r in paper if not r["cited"] and not r["cited_other_fields"]),
                           "cited_only_by_commit_hash": sum(1 for r in paper if r["cited_commit_only"])},
    }
    result = {"generated_from": {"nodes": NODES, "main_ref": MAIN_REF, "head": git_out("rev-parse", "HEAD").strip()},
              "summary": summary, "months": month_rows, "adrs": adrs, "status_sections": status,
              "archive_tags": tags, "unmerged_branches": branches, "commit_clusters": clusters,
              "paper_s10_rows": paper}
    os.makedirs(os.path.dirname(OUT_YAML), exist_ok=True)
    with open(OUT_YAML, "w", encoding="utf-8", newline="\n") as f:
        yaml.safe_dump(result, f, allow_unicode=True, sort_keys=False, width=200)

    S = summary
    md = ["# Missing-from-the-tech-tree candidates", "",
          "Extraction only: facts about past work that no node cites. Grouping is the owner's call. "
          "Generated by `tools/roadmap/history_audit.py` from `docs/roadmap/nodes.yaml`; machine-readable twin: "
          "`missing-candidates.yaml`.", "",
          "**How to read the cited columns.** `NO` = no node cites it in `evidence`, `notes` or `title` (the strict test, "
          "text-normalised). `broad-only` = not in those fields but present in `folded`, `verdict_quote`, `headline`, "
          "`plain`, `id`, `replaced_by` or `order_reason`. `yes (...)` = cited, with how. A node may also cover an item "
          "without citing it; `possible match` gives the node whose title words overlap most (rarity-weighted, heuristic; "
          "items are kept either way).", "",
          "## Summary", "",
          table(["list", "total", "uncited (strict)", "uncited even in broad fields", "note"], [
              ["1 ADRs (ever existed, any ref)", S["adrs"]["total"], S["adrs"]["uncited_strict"],
               S["adrs"]["uncited_even_broad"], f"{S['adrs']['uncited_and_deleted']} of the uncited are deleted"],
              ["2 status sections", S["status_sections"]["total"], S["status_sections"]["uncited_strict"],
               S["status_sections"]["uncited_even_broad"],
               f"{S['status_sections']['uncited_and_gone_from_main']} uncited are gone from main; "
               f"{S['status_sections']['cited_only_by_bare_section_sign']} count as cited only by a bare section sign"],
              ["3 archive tags", S["archive_tags"]["total"], S["archive_tags"]["uncited_strict"],
               S["archive_tags"]["uncited_even_broad"], ""],
              ["4 unmerged remote branches (github)", S["unmerged_branches"]["total"], S["unmerged_branches"]["uncited_strict"],
               S["unmerged_branches"]["uncited_even_broad"], ""],
              ["5 commit-scope clusters with no node (heuristic)", S["uncovered_commit_clusters"]["total"],
               S["uncovered_commit_clusters"]["total"], "-",
               f"{S['uncovered_commit_clusters']['uncovered_commits_in_clusters']} commit-memberships; "
               f"months {', '.join(heavy)}"],
              ["6 paper section 10 rows", S["paper_s10_rows"]["total"], S["paper_s10_rows"]["uncited_strict"],
               S["paper_s10_rows"]["uncited_even_broad"],
               f"{S['paper_s10_rows']['cited_only_by_commit_hash']} more are cited only via their commit hash"],
          ]), "",
          f"Totals: {S['nodes_total']} nodes ({S['nodes_without_start']} without a start date), "
          f"{S['commits_all_refs']} commits on all refs. Main = `{MAIN_REF}` for branch merge status; HEAD = "
          f"`{result['generated_from']['head'][:10]}` for \"on main\".", "",
          "## Before-numbers: nodes per start month vs commits per month", "",
          table(["month", "nodes (by start)", "commits (all refs)", "commits per node"],
                [[r["month"], r["nodes"], r["commits"], "-" if r["commits_per_node"] is None else r["commits_per_node"]]
                 for r in month_rows]), "",
          f"Months with commits per node over 15 (or no nodes at all), used for list 5: {', '.join(heavy) or 'none'}.", ""]

    md += ["## 1. ADRs", "",
           "Every `NNNN-*.md` that ever existed under an `adr/` directory in any ref. Title = first heading of its last "
           "existing version; status = its last `Status` line; summary = first sentence of Context (else Decision).", "",
           table(["ADR", "title", "added", "deleted", "last status", "summary", "cited", "possible match"],
                 [[f"{r['num']:04d}", r["title"], r["added"], r["deleted"] or ("on main" if r["on_main"] else "?"),
                   r["status"], r["summary"], yn(r), r["possible_match"]] for r in adrs if not r["cited"]]),
           "", "Reconciliation with the owner's list of 35: ADRs 0029 and 0037 are counted as cited here because node "
           "`rec-early-arq` names them inside the list `ADR-0003/5/7/29/37/42-47` (a node note; lists with `/` or `,` are "
           "expanded, ranges like `42-47` are not). ADR 0046 shows `broad-only`: it is named only in a `folded` entry of "
           "`meas-c8-aggregation-arc`. The strict-uncited count therefore differs from 35 by those three.", "",
           f"Cited ADRs omitted from the table above: {sum(1 for r in adrs if r['cited'])} (full list in the YAML).", ""]

    md += ["## 2. Status sections", "",
           "Every `## <n>.` heading ever added to a `status.md` (any path, any ref), first-seen date = earliest commit "
           "that added it. Numbers 1-3 on main are the current-state sections of `docs/status.md`, not the old sections "
           "with the same number (heading_variants shows renames or collisions). `cited` = `status §n` / `status n` "
           "(strong) or a bare `§n` not preceded by `paper` (weak, shown in brackets).", "",
           table(["section", "heading", "first seen", "variants", "on main", "cited", "possible match"],
                 [[r["section"], r["heading"][:110], r["first_seen"], r["heading_variants"],
                   "yes" if r["on_main"] else "no", yn(r), r["possible_match"]] for r in status if not r["cited"]]),
           "", f"All {sum(1 for r in status if r['cited'])} of {len(status)} sections are cited by some node; the table is empty "
           "for that reason. Sections cited only by a bare section sign (weak) are marked in the YAML (`cited_how`).", ""]

    md += ["## 3. Archive tags", "",
           table(["tag", "type", "date", "subject", "commit", "cited", "possible match"],
                 [[r["tag"], r["type"], r["date"], r["subject"], r["commit"], yn(r), r["possible_match"]] for r in tags]), ""]

    md += ["## 4. Unmerged remote branches (github, not merged into github/main)", "",
           table(["branch", "last commit", "unique commits", "last subject", "cited", "possible match"],
                 [[r["branch"], r["last_date"], r["unique_commits"], r["last_subject"], yn(r), r["possible_match"]]
                  for r in branches]), ""]

    md += ["## 5. Commit clusters with no node (HEURISTIC)", "",
           "Heuristic, not a verdict. Commit subjects in the months above are keyed by conventional scope `type(scope):`, "
           "any `RWM_*` flag named in the subject, or the leading words before the first `:` / dash (for generic leads "
           "like `merge`, `status`, `docs` the first two words after the colon). A cluster is listed when at least 3 of "
           "its commits fall outside the `[start, end]` range of every node whose title or id shares a word with the "
           "key. Clusters can overlap (one commit, several keys); subjects are free-form so some keys are noise.", "",
           table(["month", "cluster key", "commits", "uncovered", "uncovered span", "examples", "nodes with a matching word"],
                 [[r["month"], r["cluster"], r["commits"], r["uncovered"], f"{r['first']} to {r['last']}",
                   "; ".join(r["examples"]), r["possible_match"]] for r in clusters]), ""]

    md += ["## 6. Paper section 10 rows (docs/fec-arq-model.md)", "",
           "Rows of the archived-arm / verdict table at HEAD. `cited` = a node names the mechanism or one of its `RWM_*` "
           "flags (commit-hash-only matches are flagged separately in the last column; a commit often measures several rows).", "",
           table(["group", "mechanism", "status", "commit", "cited", "hash only", "possible match"],
                 [[r["group"], r["mechanism"], r["status"], r["commit"], yn(r), "yes" if r["cited_commit_only"] else "",
                   r["possible_match"]] for r in paper if not r["cited"]]), "",
           f"Cited rows omitted: {sum(1 for r in paper if r['cited'])} (full list in the YAML).", ""]

    with open(OUT_MD, "w", encoding="utf-8", newline="\n") as f:
        f.write("\n".join(md))
    print(yaml.safe_dump(summary, allow_unicode=True, sort_keys=False))


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8")
    main()
