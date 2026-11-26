import sys, os, math
TOY = r"C:\Users\pierr\AppData\Local\Temp\claude\c92d18e023ce8566\28b06ad0-9458-4799-81e9-72731fe0b356\scratchpad\s3e\tools\toy"
sys.path.insert(0, TOY); os.chdir(TOY)
import numpy as np
import confirm13q as Q, confirm13 as K, confirm3e as C
cell = sys.argv[1]
jobs = Q.q_jobs() if cell == Q.Q else K.m_jobs()
r0 = C.recs(jobs[0])[0]
print(sorted(r0["gauges"].keys()))
print(sorted(k for k in r0.keys()))
print(sorted(r0.get("rep3e", {}).keys()))
