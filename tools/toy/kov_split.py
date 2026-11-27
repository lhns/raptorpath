import sys, json, numpy as np, multiprocessing as mp
sys.path.insert(0,'.')
import rev4_step2 as S, batch3e as B
def go(a): return S.one(a)
if __name__=="__main__":
    B.build()
    specs=[]
    def sp(lab,fr,wait,gate,rule):
        return dict(label=lab,group="floor",cell="K-OV",base="K-OV",design="new",mode="RL",T=0.4,delta=50.0,clamp=True,wait=wait,eq=False,cwnd=0.0,kw={},H=0.0,gate=gate,to=0.0,rule=rule,fr=fr)
    specs=[sp("R3 fr","evidenced",False,False,""),sp("R3 off","",False,False,""),sp("R4 no gate fr","evidenced",True,False,""),sp("R4 no gate off","",True,False,""),sp("R4 frozen fr","evidenced",True,True,"engine"),sp("R4 frozen off","",True,True,"engine")]
    with mp.Pool(9) as p:
        rows=p.map(go,[(s,k) for s in specs for k in range(3)])
    json.dump(rows,open("out3e/rev4/kov_split.json","w"))
    for lab in [s["label"] for s in specs]:
        rs=[r for r in rows if r["label"]==lab]
        m=lambda f: float(np.mean([f(r) for r in rs]))
        g=lambda k: m(lambda r: r["g"].get(k) or 0)
        slots=m(lambda r: r["n_sent"]+r["g"]["repair_sends"]+r["g"]["rep_sent"]+r["g"]["floors"])
        print("%-16s dv %.4f ot %.4f p50 %.3f | slots: first %.0f resends %.0f repairs %.0f floors %.0f total %.0f busy %.3f pause %.1f dup %.0f | fires: know %.0f timeout %.0f carrier young %.0f overdue %.0f none %.0f suppressed %.0f" % (lab,m(lambda r:r["share_any"]),m(lambda r:r["share_by_0p4"]),m(lambda r:r["dh_p50"]),m(lambda r:r["n_sent"]),g("repair_sends"),g("rep_sent"),g("floors"),slots,m(lambda r:r["link_busy_frac"]),m(lambda r:r["pause_time"]),g("dup"),g("rep_fired_by_knowledge"),g("rep_fired_by_timeout"),g("rep_fire_carrier_young"),g("rep_fire_carrier_overdue"),g("rep_fire_carrier_none"),g("rep_suppressed")))
