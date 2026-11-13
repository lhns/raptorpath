"""Check law-ledger.yaml and regenerate the generated blocks of law-ledger.md.

Usage: python render.py [--check]   (paths are relative to this file)
The prose in law-ledger.md is hand-written; only the text between
<!-- BEGIN:x --> and <!-- END:x --> markers is replaced.
"""
import collections, pathlib, re, sys
import yaml

HERE = pathlib.Path(__file__).resolve().parent
YAML = HERE.parent / "law-ledger.yaml"
MD = HERE.parent / "law-ledger.md"
REQUIRED = ["id", "name", "verdicts", "priority", "depends_on", "evidence"]
TAGS = {"OK", "DIVERGENT", "UNDERIVED", "PINNED", "DEGENERATE", "DISHONEST-INPUT", "MODE-SWITCH"}
COLS = [("loc", "1. location"), ("formula", "2. formula (code)"), ("paper", "3. paper"),
        ("derivation", "4. derivation"), ("regime", "5. measured in regime?"),
        ("clamp", "6. clamp bind"), ("inputs", "7. inputs honest?"),
        ("modeswitch", "8. NO-MODE-SWITCH"), ("verdicts", "9. verdict"),
        ("fix", "10. fix class, priority")]


def check(rows):
    ids = [r["id"] for r in rows]
    dup = [i for i, n in collections.Counter(ids).items() if n > 1]
    assert not dup, f"duplicate ids {dup}"
    known = set(ids)
    for r in rows:
        for k in REQUIRED:
            assert k in r, f"{r.get('id')}: missing {k}"
        for k in ("depends_on", "ships_with"):
            for d in r.get(k, []):
                assert d in known, f"{r['id']}: {k} {d} unknown"
        for v in r["verdicts"]:
            assert v.split("(")[0] in TAGS, f"{r['id']}: bad verdict {v}"
        assert r["priority"] in {"P0", "P1", "P2", "P3"}, r["id"]
    ranks = sorted(r.get("rank", 0) for r in rows)
    assert ranks == list(range(1, len(rows) + 1)), "ranks must be 1..N, each once"


def distribution(rows):
    c = collections.Counter(v.split("(")[0] for r in rows for v in r["verdicts"])
    ok_only = [r for r in rows if r["verdicts"] == ["OK"]]
    ok_unverified = [r["id"] for r in ok_only if r.get("measured") != "yes"]
    pri = collections.Counter(r["priority"] for r in rows)
    lines = [f"- rows: {len(rows)}"]
    lines.append("- verdict tags (a row may carry several): " + ", ".join(
        f"{t} {c[t]}" for t in ["OK", "DIVERGENT", "UNDERIVED", "PINNED", "DEGENERATE",
                                "DISHONEST-INPUT", "MODE-SWITCH"]))
    lines.append(f"- rows that are OK only: {len(ok_only)}, of which not measured in regime "
                 f"(OK but unverified): {len(ok_unverified)} ({', '.join(ok_unverified)})")
    lines.append("- priorities: " + ", ".join(f"{p} {pri[p]}" for p in ["P0", "P1", "P2", "P3"]))
    return "\n".join(lines)


def esc(s):
    return str(s).replace("|", "\\|").replace("\n", " ")


def summary(rows):
    out = ["| id | law | verdicts | priority | rank | depends on |", "|---|---|---|---|---|---|"]
    for r in sorted(rows, key=lambda r: r["rank"]):
        out.append(f"| [{r['id']}](#{r['id'].lower()}) | {esc(r['name'])} | {esc(', '.join(r['verdicts']))} | "
                   f"{r['priority']} | {r.get('rank') or '-'} | {esc(', '.join(r['depends_on'])) or '-'} |")
    return "\n".join(out)


def full(rows):
    out, area = [], None
    for r in rows:
        if r.get("area") != area:
            area = r.get("area")
            out.append(f"\n### Area: {area}\n")
        out.append(f"#### {r['id']}\n\n**{r['name']}** ({r.get('reach', '')})\n")
        for key, label in COLS:
            if key == "verdicts":
                val = f"**{', '.join(r['verdicts'])}**: {r['evidence']}"
            elif key == "fix":
                val = f"{r.get('fix_class', '-')}; **{r['priority']}**, rank {r.get('rank') or '-'}"
                if r["depends_on"]:
                    val += f"; depends on {', '.join(r['depends_on'])}"
                if r.get("ships_with"):
                    val += f"; ships with {', '.join(r['ships_with'])}"
            elif key == "formula":
                val = f"\n\n  ```text\n  {r[key]}\n  ```\n"
            else:
                val = r.get(key, "-")
            out.append(f"- *{label}:* {val}")
        out.append(f"- *flags:* measured {r.get('measured', '-')}, gauge {r.get('gauge', '-')}\n")
    return "\n".join(out)


def splice(md, name, body):
    pat = re.compile(rf"(<!-- BEGIN:{name} -->\n).*?(<!-- END:{name} -->)", re.S)
    assert pat.search(md), f"marker {name} missing"
    return pat.sub(lambda m: m.group(1) + body + "\n" + m.group(2), md)


def main():
    rows = yaml.safe_load(YAML.read_text(encoding="utf-8"))["rows"]
    for r in rows:  # YAML 1.1 reads bare yes/no as booleans
        for k in ("measured", "gauge"):
            if isinstance(r.get(k), bool):
                r[k] = "yes" if r[k] else "no"
    check(rows)
    md = MD.read_text(encoding="utf-8")
    new = md
    for name, body in [("dist", distribution(rows)), ("summary", summary(rows)), ("rows", full(rows))]:
        new = splice(new, name, body)
    if "--check" in sys.argv:
        assert new == md, "law-ledger.md is stale: run render.py"
        print(f"OK: {len(rows)} rows, md in sync")
    else:
        MD.write_text(new, encoding="utf-8")
        print(f"rendered {len(rows)} rows")
    print(distribution(rows))


if __name__ == "__main__":
    main()
