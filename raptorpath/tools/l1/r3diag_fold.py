#!/usr/bin/env python3
"""§26 (docs/status.md): the receiver-rooted breakdown of a server
`perf script -F tid,sym` (dwarf call graphs), read off the VM.

For every sample whose stack contains the receiver task's poll
(`run_receiver`), report: the receiver's share of all samples; its direct
callees (the frame just below `run_receiver`, self when none); and, for
the samples under `UnifiedDecoder::add_symbol`, the leaf frame's class
(gf256 arithmetic, allocator, memmove/memset, BTree, hash, other).

    zcat P-c1d.script.gz | python3 r3diag_fold.py
"""
import collections
import re
import sys

CLASSES = [
    ("gf256", re.compile(r"gf256|mul_acc|gf_mul")),
    ("alloc", re.compile(r"malloc|free|_int_|sysmalloc|consolidate|realloc|mprotect|brk|calloc|cfree")),
    ("memcpy", re.compile(r"memmove|memcpy|memset")),
    ("btree", re.compile(r"btree|BTree")),
    ("hash", re.compile(r"hash|Hash|sip|Sip")),
]


def leaf_class(sym):
    for name, rx in CLASSES:
        if rx.search(sym):
            return name
    return "other"


def samples(lines):
    cur = []
    for line in lines:
        line = line.rstrip("\n")
        if not line.strip():
            if cur:
                yield cur
            cur = []
            continue
        if line.startswith("\t") or line.startswith(" " * 8):
            cur.append(line.strip())
        else:
            if cur:
                yield cur
            cur = []
    if cur:
        yield cur


def short(sym):
    sym = re.sub(r"\+0x[0-9a-f]+$", "", sym)
    sym = re.sub(r"^[0-9a-f]+ ", "", sym)
    return sym[:110]


def main():
    total = 0
    rx = 0
    callee = collections.Counter()
    leaf_rx = collections.Counter()
    dec = collections.Counter()
    dec_leaf = collections.Counter()
    for frames in samples(sys.stdin):
        total += 1
        syms = [short(f) for f in frames]  # leaf first
        idx = next((i for i, s in enumerate(syms) if "run_receiver" in s), None)
        if idx is None:
            continue
        rx += 1
        callee[syms[idx - 1] if idx > 0 else "(self)"] += 1
        leaf_rx[leaf_class(syms[0])] += 1
        if any("add_symbol" in s for s in syms[:idx]):
            c = leaf_class(syms[0])
            dec[c] += 1
            dec_leaf[syms[0]] += 1
    print(f"samples={total} receiver={rx} receiver_share={rx / max(total, 1):.3f}")
    print("-- receiver leaf classes (share of receiver samples)")
    for k, v in leaf_rx.most_common():
        print(f"  {k:8s} {v:6d} {v / max(rx, 1):.3f}")
    print("-- receiver direct callees (share of receiver samples)")
    for k, v in callee.most_common(30):
        print(f"  {v:6d} {v / max(rx, 1):.3f} {k}")
    nd = sum(dec.values())
    print(f"-- under add_symbol: {nd} samples ({nd / max(rx, 1):.3f} of receiver); leaf classes:")
    for k, v in dec.most_common():
        print(f"  {k:8s} {v:6d} {v / max(nd, 1):.3f}")
    print("-- under add_symbol: top leaves")
    for k, v in dec_leaf.most_common(20):
        print(f"  {v:6d} {v / max(nd, 1):.3f} {k}")


if __name__ == "__main__":
    main()
