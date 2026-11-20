#!/usr/bin/env python3
"""Expected-failure job: run the #[ignore]d defect tests and require that every one FAILS.

A defect test is a test whose ignore reason starts with a roadmap node id, e.g.
    #[ignore = "rec-evict-blocker-unresolved: at rho < 1 ..."]
It is found by grepping the sources, not from a list. The job passes when every such
test fails, and fails when one passes (the fix landed, or the test rotted) or did not
run at all (build error, name not found).

  expected_failure.py [--root REPO] [--list]    --list prints the tests found and exits
"""
import argparse
import os
import pathlib
import re
import subprocess
import sys
import tomllib

IGNORE = re.compile(r'#\[ignore\s*=\s*"([a-z0-9]+(?:-[a-z0-9]+)+):\s')
FN = re.compile(r"\bfn\s+(\w+)")
MSG = "a defect test now passes: fix landed or the test rotted; un-ignore it or fix it"


def node_ids(root):
    """Roadmap node ids, or None when the tree has no docs/roadmap/nodes.yaml (a test fixture)."""
    f = root / "docs/roadmap/nodes.yaml"
    return set(re.findall(r"^- id: (\S+)", f.read_text(encoding="utf-8"), re.M)) if f.is_file() else None


def scan(root):
    """-> [{id, file, fn, crate, kind, target, features}] for every defect-ignored test.
    A hyphenated prefix that is not a roadmap node is an error: a typo must not drop the test."""
    found = []
    nodes = node_ids(root)
    for crate in sorted(p.parent for p in root.glob("*/Cargo.toml")):
        meta = tomllib.loads((crate / "Cargo.toml").read_text(encoding="utf-8"))
        pkg = meta["package"]["name"]
        req = {t["name"]: t.get("required-features", []) for t in meta.get("test", [])}
        for sub in ("src", "tests"):
            for f in sorted((crate / sub).rglob("*.rs")) if (crate / sub).is_dir() else []:
                lines = f.read_text(encoding="utf-8", errors="replace").splitlines()
                for i, l in enumerate(lines):
                    m = IGNORE.search(l)
                    if not m:
                        continue
                    fn = next((FN.search(x).group(1) for x in lines[i:i + 8] if FN.search(x)), None)
                    rel = f.relative_to(root).as_posix()
                    if nodes is not None and m.group(1) not in nodes:
                        raise SystemExit(f"{rel}:{i + 1}: ignore reason starts with '{m.group(1)}:', which is not a node id in docs/roadmap/nodes.yaml")
                    if fn is None:
                        raise SystemExit(f"{rel}:{i + 1}: defect ignore with no fn within 8 lines")
                    top = sub == "tests" and f.parent == crate / "tests"
                    found.append({"id": m.group(1), "file": rel, "fn": fn, "crate": pkg,
                                  "kind": "test" if top else "lib", "target": f.stem if top else None,
                                  "features": req.get(f.stem, []) if top else []})
    return found


def command(t):
    c = ["cargo", "test", "-p", t["crate"]]
    if t["features"]:
        c += ["--features", ",".join(t["features"])]
    c += ["--test", t["target"]] if t["kind"] == "test" else ["--lib"]
    return c + ["--", "--ignored", "--exact", t["fn"]] if t["kind"] == "test" else c + ["--", "--ignored", t["fn"]]


def env_for(t):
    env = dict(os.environ)
    if "sim" in t["features"]:  # seeded select! order; never in .cargo/config.toml
        env["RUSTFLAGS"] = (env.get("RUSTFLAGS", "") + " --cfg tokio_unstable").strip()
    return env


def verdict(t, output):
    """-> (ok, text). ok only if the test ran and FAILED."""
    m = [x for x in re.findall(r"^test (\S+) \.\.\. (ok|FAILED|ignored)\s*$", output, re.M)
         if x[0] == t["fn"] or x[0].endswith("::" + t["fn"])]
    if not m:
        return False, f"{t['id']}: {t['fn']} did not run (build error or name not found)"
    if any(r == "ok" for _, r in m):
        return False, f"{t['id']}: {t['fn']} PASSED: {MSG}"
    if any(r == "ignored" for _, r in m):
        return False, f"{t['id']}: {t['fn']} was still ignored (the filter or flags are wrong)"
    return True, f"{t['id']}: {t['fn']} failed, as expected"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", default=str(pathlib.Path(__file__).resolve().parents[2]))
    ap.add_argument("--list", action="store_true")
    a = ap.parse_args()
    root = pathlib.Path(a.root)
    tests = scan(root)
    print(f"defect tests found ({len(tests)}):")
    for t in tests:
        print(f"  {t['id']}  {t['file']}::{t['fn']}  $ {' '.join(command(t))}")
    if a.list:
        return 0
    bad = 0
    for t in tests:
        r = subprocess.run(command(t), cwd=root, env=env_for(t), capture_output=True, text=True, errors="replace", timeout=3000)
        ok, text = verdict(t, r.stdout + r.stderr)
        print(("ok    " if ok else "BAD   ") + text, flush=True)
        if not ok:
            bad += 1
            print("\n".join((r.stdout + r.stderr).splitlines()[-15:]))
    if bad:
        print(f"expected-failure job: {bad} of {len(tests)} defect tests are not failing as expected")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
