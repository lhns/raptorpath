#!/bin/bash
# The reference manifest of plan 56's suite reuse (status 56): the sha256 of every blob under raptorpath/src and raptorpath/tests at a git revision, as
# `<hash>  <path>` lines in C-locale path order -- the same lines loopdiag_run_all.sh's engine_manifest() produces from a checkout.
#   bash ref_manifest.sh 7fd92dd5 > ref56-manifest.sha256     (cand54: the tree that passed section 54's suites)
#   bash ref_manifest.sh HEAD | cmp - ref56-manifest.sha256   (does this tree equal the reference?)
set -euo pipefail
rev="${1:?usage: ref_manifest.sh <git revision>}"
cd "$(git rev-parse --show-toplevel)"      # ls-tree pathspecs are relative to the working directory
git ls-tree -r --name-only "$rev" -- raptorpath/src raptorpath/tests | LC_ALL=C sort | while IFS= read -r f; do
  printf '%s  %s\n' "$(git show "$rev:$f" | sha256sum | cut -d' ' -f1)" "$f"
done
