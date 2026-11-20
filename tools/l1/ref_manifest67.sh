#!/bin/bash
# The reference manifest of plan 67's suite reuse (status 67, registration 6.1: "the suite-reuse HIT check against session 0"), for ONE seat tree at a git revision: the sha256 of every blob loopdiag_p67.sh's src_manifest67 reads --
# the workspace Cargo.toml / Cargo.lock, every top-level crate's Cargo.toml, raptorpath/src, raptorpath/tests, raptorpath-math / -core / -wasm src, gf256/src, vendor/ AND tools/ci/flake_rule.py (ADR-0080's rule: run_all and
# suites_run.sh read it, so a changed rule must not reuse results produced under the old one) -- as `<hash>  <path>` lines in C-locale path order.
#   bash ref_manifest67.sh vmb-s1 > s1-manifest.sha256       (the tree whose suites ran in session 0)
#   bash ref_manifest67.sh HEAD | cmp - s1-manifest.sha256   (does this tree equal the reference?)
# Session 0 records one per seat (LD_REF_MANIFEST_S1 / LD_REF_MANIFEST_S2).
set -euo pipefail
rev="${1:?usage: ref_manifest67.sh <git revision>}"
cd "$(git rev-parse --show-toplevel)"
git ls-tree -r --name-only "$rev" | grep -E '^(Cargo\.toml|Cargo\.lock|[^/]+/Cargo\.toml|raptorpath/src/|raptorpath/tests/|raptorpath-math/src/|raptorpath-core/src/|raptorpath-wasm/src/|gf256/src/|vendor/|tools/ci/flake_rule\.py$)' | LC_ALL=C sort -u | while IFS= read -r f; do
  printf '%s  %s\n' "$(git show "$rev:$f" | sha256sum | cut -d' ' -f1)" "$f"
done
