import sys, os, math
TOY = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, TOY); os.chdir(TOY)
import numpy as np
import confirm13q as Q, confirm13 as K, confirm3e as C
cell = sys.argv[1]
jobs = Q.q_jobs() if cell == Q.Q else K.m_jobs()
r0 = C.recs(jobs[0])[0]
print(sorted(r0["gauges"].keys()))
print(sorted(k for k in r0.keys()))
print(sorted(r0.get("rep3e", {}).keys()))
