"""Server-side control: the server runs the owner's republish timer but no sender-side gate decision.
dCPU_srv (G - R medians) against the server's owner_timer_wakes, per within-session cell.
Usage: python -I srv.py <l1-raw dir>"""
import statistics as st, sys
import os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from rows import rows
R = rows()
def med(led, cell, arm, k):
    v = [r[k] for r in R if r["ledger"] == led and r["cell"] == cell and r["arm"] == arm and r.get(k) is not None]
    return st.median(v) if v else 0.0
def rng(led, cell, arm, k):
    v = sorted(r[k] for r in R if r["ledger"] == led and r["cell"] == cell and r["arm"] == arm)
    return f"{v[0]:.2f}-{v[-1]:.2f}"
print("cell   cpu_srv R (range)   G (range)    dSrv   otw_srv   dSrv/otw_us | cpu_cli R range, G range")
for led, cell in [("68","K2"),("68","K8"),("68r","PC"),("68r","LR"),("68s","BU"),("68","A2")]:
    d = med(led,cell,"G","cpu_srv") - med(led,cell,"R","cpu_srv")
    o = med(led,cell,"G","otw_srv")
    print(f"{led:4}{cell}  {med(led,cell,'R','cpu_srv'):6.2f} ({rng(led,cell,'R','cpu_srv')})  {med(led,cell,'G','cpu_srv'):6.2f} ({rng(led,cell,'G','cpu_srv')})  {d:5.2f}  {o:7.0f}  {d/o*1e6 if o else float('nan'):6.1f} | {rng(led,cell,'R','cpu_cli')}, {rng(led,cell,'G','cpu_cli')}")
# reopen transitions (closed->open) on G rows, client
for led, cell in [("68","K2"),("68","K8"),("68r","PC"),("68r","LR"),("68s","BU")]:
    print(led, cell, "G reopens", med(led,cell,"G","reopens"), "otw", med(led,cell,"G","otw"), "R wk_ack", med(led,cell,"R","wk_ack"), "G wk_ack", med(led,cell,"G","wk_ack"), "R iters", med(led,cell,"R","iters"), "G iters", med(led,cell,"G","iters"))
