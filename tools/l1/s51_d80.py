#!/usr/bin/env python3
"""Status 51 (s51-design.md section 6 item 5, corrected by s51-evict-design.md section 9.2 item 51-5): the pre-registration d80 table of score51's statistic, recomputed offline on the historical 600 MB rows.

    s51_d80.py [--draws N] [--nperm N] [--cells c2-100,c8-100] [--out tools/l1/d80_51.json] [--seed N] [--rows]

The statistic is s51_score's: the two-sided rank-sum min-p family of 4 clauses, FAIL at a preset iff the family p <= 0.05/3, T1 at 6 v 6 and T2 at 6 v 12 (the ratio of the registered design). The method is the
section 4 bootstrap: A and B resampled with replacement from the pool of one cell, the shift applied to B's one clause (goodput x (1 - d), the others x (1 + d)), the grid 1/2/3/5/8/12/20/30/50 %, the null the
scorer's own (exact enumeration at 6 v 6 = 924 relabellings; `nperm` random relabellings at 6 v 12), d80 = the smallest grid shift caught in >= 80 % of `draws`.

Pools (rows with all four clauses): the section 48b BASE rows at c2 / c8 (goodput and CPU from the ledger, the in-order delay from guard-clause-dry/rowvals-bbr48b.json), the section 53 c8 BASE + BASES rows
(the in-order delay recomputed from the kept client logs), and the rows built at or after 87c2135d (the snapshot-order law): a row's seat binary is read from its launch's BINSHA.txt and classified with
`git merge-base --is-ancestor 87c2135d <commit>`; only SHIPPED-DEFAULT arms count as post-87c2135d rows (a lever / gauge / pool arm is not the baseline). The report prints how many rows qualify per era.
The JSON (--out) is machine-readable: {cell: {"T1": {clause: d80}, "T2": {clause: d80}, "n_rows": n, "eras": {...}, "draws": N}}; s51_score reads it for its 2x resolution check."""
import json
import os
import subprocess
import sys
import tarfile
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import s51_score as sc  # noqa: E402

REPO = os.path.normpath(os.path.join(HERE, "..", ".."))
RAW = os.path.join(REPO, "docs", "l1-raw")
POST_LAW = "87c2135d"
C2, C8 = "c2-100", "c8-100"


def is_ancestor(commit, of=POST_LAW):
    """True iff `of` is an ancestor of `commit` (the row's binary was built at or after the snapshot-order law). None when git cannot tell."""
    try:
        return subprocess.run(["git", "-C", REPO, "merge-base", "--is-ancestor", of, commit], capture_output=True).returncode == 0
    except OSError:
        return None


def commits(binsha_path):
    """{'bin': sha, 'bin-base': sha} of a launch's BINSHA.txt ('<sha256>  <path>  commit=<sha> [(BASE)]')."""
    out = {}
    try:
        for ln in open(binsha_path, encoding="utf-8", errors="replace"):
            if "commit=" not in ln:
                continue
            seat = "bin-base" if "/bin-base/" in ln else "bin"
            out[seat] = ln.split("commit=")[1].split()[0]
    except OSError:
        pass
    return out


def per_gb(sec, nbytes):
    return None if (sec is None or not nbytes) else sec / (nbytes / 1e9)


def rows_of(path):
    import place38_parse as pp
    return pp.rows_of([path])[0] if os.path.exists(path) else []


def tgz_delay(tgz, row):
    """The in-order delay mean (ms) of a kept row from its client log inside the launch's diag tarball, or None."""
    import threadp1_parse as t1
    name = f"{row['cell']}@{row.get('store', 'def')}-{row['arm']}-s{row['seed']}-r{row['rep']}-a1-c.log"
    try:
        with tarfile.open(tgz) as tf:
            m = [x for x in tf.getmembers() if x.name.endswith("/" + name)]
            if not m:
                return None
            cli = tf.extractfile(m[0]).read().decode("utf-8", errors="replace").splitlines()
        st = t1.inorder_delay_stats(cli)
        return st["mean"] * 1e3 if st else None
    except (OSError, tarfile.TarError):
        return None


def load_pools(include_rows=False):
    """-> {cell: {"rows": [(vector, era, source, arm)], ...}}. era = 'pre' | 'post' (relative to 87c2135d); a row whose seat commit is unknown is 'unclassified' and excluded from 'post'."""
    out = {C2: [], C8: []}
    rv_path = os.path.join(REPO, "docs", "derivations", "guard-clause-dry", "rowvals-bbr48b.json")
    rowvals = json.load(open(rv_path)) if os.path.exists(rv_path) else {}
    # section 48b
    cm = commits(os.path.join(RAW, "bbr48b", "run", "BINSHA.txt"))
    for r in rows_of(os.path.join(RAW, "bbr48b", "run", "bbr48", "ld.log")):
        if r["status"] != "LIVE" or r.get("dnf") or r["arm"] != "BASE" or r["cell"] not in out:
            continue
        key = f"{r['cell']}|BASE|{r['seed']}|{r['rep']}"
        d = ((rowvals.get(key) or {}).get("B") or {}).get("mean")
        b = r.get("bytes") or 600_000_000
        v = [r.get("mbps"), None if d is None else d * 1e3, per_gb(r.get("cpu_cli"), b), per_gb(r.get("cpu_srv"), b)]
        out[r["cell"]].append((v, _era(cm.get("bin-base")), "s48b BASE", "BASE"))
    # section 53 (c8 BASE + BASES; the delay is recomputed from the kept logs)
    cm = commits(os.path.join(RAW, "s53", "BINSHA.txt"))
    tgz = os.path.join(RAW, "s53", "rep53-diag-ld.tgz")
    for r in rows_of(os.path.join(RAW, "s53", "rep53-ld.log")):
        if r["status"] != "LIVE" or r.get("dnf") or r["arm"] not in ("BASE", "BASES") or r["cell"] != C8:
            continue
        t = r.get("t53") or {}
        v = [r.get("mbps"), tgz_delay(tgz, r) if os.path.exists(tgz) else None, t.get("cpu_cli_s_per_gb"), t.get("cpu_srv_s_per_gb")]
        out[C8].append((v, _era(cm.get("bin-base")), "s53 " + r["arm"], r["arm"]))
    # section 54 (c8 600 MB, the shipped-default pool 2048 on the BASE seat = cand54; L* = lever B, not shipped)
    cm = commits(os.path.join(RAW, "s54", "54b-BINSHA.txt"))
    for r in rows_of(os.path.join(RAW, "s54", "rep54b-ld.log")):
        if r["status"] != "LIVE" or r.get("dnf") or r["arm"] not in ("B2048", "L2048") or r["cell"] != C8:
            continue
        t = r.get("t54") or {}
        seat = "bin-base" if r["arm"].startswith("B") else "bin"
        v = [r.get("mbps"), None if t.get("inorder_mean_run") is None else t["inorder_mean_run"] * 1e3, t.get("cpu_cli_s_per_gb"), t.get("cpu_srv_s_per_gb")]
        shipped = r["arm"] == "B2048"
        out[C8].append((v, _era(cm.get(seat)) if shipped else "excluded-not-shipped-default", "s54 " + r["arm"], r["arm"]))
    return out


def _era(commit):
    if not commit:
        return "unclassified"
    a = is_ancestor(commit)
    return "unclassified" if a is None else ("post" if a else "pre")


def usable(rows):
    """The pool of complete clause vectors: pre + post eras (the post rows are the shipped-default ones after the snapshot-order law)."""
    return [v for v, era, _, _ in rows if era in ("pre", "post") and all(x is not None for x in v)]


def main(argv):
    def arg(name, default):
        return argv[argv.index(name) + 1] if name in argv else default
    draws, nperm, seed = int(arg("--draws", 200)), int(arg("--nperm", 400)), int(arg("--seed", 51))
    cells = arg("--cells", f"{C2},{C8}").split(",")
    out_path = arg("--out", None)
    pools = load_pools()
    res = {}
    t0 = time.time()
    print(f"D80 PRE-REGISTRATION TABLE (score51's statistic: two-sided rank-sum min-p, family of {len(sc.CLAUSES)}, FAIL iff p <= {sc.THRESH:.4f}; {draws} bootstrap draws, exact 924 relabellings at 6 v 6, {nperm} at 6 v 12, grid {', '.join('%g' % (100 * g) for g in sc.D80_GRID)} %)")
    for cell in cells:
        rows = pools[cell]
        eras = {}
        for _, era, src, _ in rows:
            eras[f"{era}:{src}"] = eras.get(f"{era}:{src}", 0) + 1
        pool = usable(rows)
        post = sum(1 for v, era, _, _ in rows if era == "post" and all(x is not None for x in v))
        print(f"== {cell}: {len(pool)} usable rows; eras {eras}; rows built at or after {POST_LAW} and shipped-default: {post}" + (" (FEW: the snapshot-order era is thinly sampled; the in-session A/A governs resolution)" if post < 6 else ""))
        if len(pool) < 6:
            print("   too few rows: not computed")
            continue
        t1 = sc.d80(pool, 6, 6, draws=draws, nperm=nperm, seed=seed)
        t2 = sc.d80(pool, 6, 12, draws=draws, nperm=nperm, seed=seed + 1)
        res[cell] = {"T1": t1, "T2": t2, "n_rows": len(pool), "eras": eras, "post_rows": post, "draws": draws, "nperm": nperm}
        for nm, d in (("T1 6 v 6", t1), ("T2 6 v 12", t2)):
            print(f"   {nm}: " + " ".join(f"{c} {'>50 %' if d[c] is None else '%g %%' % (100 * d[c])}" for c in sc.CLAUSES))
    print(f"({time.time() - t0:.0f} s)")
    if out_path:
        with open(out_path, "w", encoding="utf-8", newline="\n") as fh:
            json.dump(res, fh, indent=1, sort_keys=True)
            fh.write("\n")
        print("wrote", out_path)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
