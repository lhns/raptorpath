import sys, json, dataclasses, itertools, numpy as np, multiprocessing as mp
sys.path.insert(0,'.')
import batch3d as D, toy2
INF=float("inf")
def go(a):
    T,H,hf,fr,seed=a
    cfg=dataclasses.replace(D.p2_cfg(seed,T,H,hf),floor_repeat=fr,elig="rack" if fr=="evidenced_overdue" else "")
    sm=toy2.Sim(cfg); r=sm.run()
    given=[s for s in range(sm.n_sent) if not sm.got[s] and not sm.dec_flag[s]]       # never received nor decoded: the sender gave them up (or lost them for good)
    orphan=sum(1 for s in given if s>=sm.F_rx)
    slow_orphans=sum(1 for s in given if sm.path_of[s]==1 and s>=sm.F_rx)
    g=r["gauges"]
    return dict(T=T,H=H,hf=hf,fr=fr,seed=seed,finished=r["finished"],dv=r["share_any"],ot=r["share_by_0p4"],p50=r["dh_p50"],floors=g["floors"],supp=g.get("rep_suppressed"),know=g.get("rep_fired_by_knowledge"),to=g.get("rep_fired_by_timeout"),viol=g.get("rep_supp_viol"),given=len(given),orphan=orphan,slow_orphans=slow_orphans,slow_given=sum(1 for s in given if sm.path_of[s]==1),t_last=sm.t_last)
if __name__=="__main__":
    tasks=[(T,H,hf,fr,k) for T in D.P2_T for H in (0.0,INF) for hf in ("est","0") for fr in ("","evidenced_overdue") for k in range(3)]
    with mp.Pool(9) as p: rows=p.map(go,tasks)
    json.dump(rows,open("out3e/rev4/p2_new.json","w"))
    keys=sorted({(r["T"],r["H"],r["hf"]) for r in rows},key=str)
    print("T H hf | floors off/on | delivered off/on | on time off/on | p50 off/on | finished on | given(slow) | orphans on | supp know timeout viol")
    for k in keys:
        off=[r for r in rows if (r["T"],r["H"],r["hf"])==k and r["fr"]==""]; on=[r for r in rows if (r["T"],r["H"],r["hf"])==k and r["fr"]]
        m=lambda rs,f: np.mean([r[f] or 0 for r in rs])
        print(k,"| %.0f/%.0f | %.4f/%.4f | %.4f/%.4f | %.3f/%.3f | %s%s | %.0f(%.0f) | %.0f slow %.0f | %.0f %.0f %.0f %.0f"%(m(off,"floors"),m(on,"floors"),m(off,"dv"),m(on,"dv"),m(off,"ot"),m(on,"ot"),m(off,"p50"),m(on,"p50"),all(r["finished"] for r in on),"" if all(r["finished"] for r in off) else " (off:not all)",m(on,"given"),m(on,"slow_given"),m(on,"orphan"),m(on,"slow_orphans"),m(on,"supp"),m(on,"know"),m(on,"to"),m(on,"viol")))
