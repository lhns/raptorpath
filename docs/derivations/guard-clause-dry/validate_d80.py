#!/usr/bin/env python3
"""Validation of resolution.py: the guard's own session_resolution (every clause, the ledger's real row values) on a ledger, rtp clauses printed as json
for comparison with the (C) entries of resolution.py's output.  usage: python -I validate_d80.py <ld.log> <out.json>"""
import json
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "..", "tools", "l1"))
import place38_parse as pp  # noqa: E402
import place41b_score as ps  # noqa: E402

if __name__ == "__main__":
    rows, _ = pp.rows_of([sys.argv[1]])
    rows, _ = pp.apply_replacements(rows)
    sr = ps.session_resolution(rows, seed=0)
    json.dump({"%s|C|%s" % (c, n.replace("rtp_floor_", "")): v["d80"] for (c, n), v in sr.items() if n.startswith("rtp_floor")}, open(sys.argv[2], "w"), indent=1)
