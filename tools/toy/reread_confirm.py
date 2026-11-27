"""Re-reads finished cells of the confirmation from the stored runs (no runs, no markers, no STOP): python reread_confirm.py CELL [CELL...]  -> prints the cell's reading."""
import json
import sys

import confirm3e as C

nf = json.load(open(C.nf_path()))
for cell in sys.argv[1:]:
    jobs = C.jobs_of(cell)
    lines, undet = C.read_cell(cell, jobs, nf)
    print("\n".join(lines))
    print("undetermined arms (a doubling would run them):", sorted(undet))
