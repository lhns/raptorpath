import sys,glob,os,statistics as st
sys.path.insert(0,os.path.dirname(__file__))
from p import parse
D=sys.argv[1]
print('cell arm | lines | share fl>=cw (gate+clamp at 0) | share spare<0.1 | median spare | median fl | median cw | median sum(B) | median sum(qcwnd)/1233 | median store win/cap | share paused>=50')
for cell in ['c1s-400','c1d-400','c2-100','c8-100']:
    for arm in ['BASE','CAND']:
        L=[];rowsp=[]
        for f in sorted(glob.glob(f'{D}/{cell}@def-{arm}-*-c.log')):
            rows=[r for r in parse(f) if r['t']>=1.0 and r['p']]
            # drop tail: lines after source end approximated by store win < 0.5 cap? keep all
            L+=rows
        n=len(L)
        g=sum(1 for r in L if r['fl']>=r['cw'])/n
        sp=[(r['cw']-r['fl'])/r['fl'] if r['fl']>0 else float('inf') for r in L]
        c=sum(1 for s in sp if s<0.1)/n
        sB=[sum(p.get('B',0) for p in r['p'].values()) for r in L]
        sq=[sum(p.get('qcwnd',0) for p in r['p'].values())/1233 for r in L]
        pa=sum(1 for r in L if r['paused']>=50)/n
        fin=[s for s in sp if s!=float('inf')]
        print(f"{cell} {arm} | {n} | {g:.4f} | {c:.4f} | {st.median(fin):.1f} | {st.median([r['fl'] for r in L]):.0f} | {st.median([r['cw'] for r in L]):.0f} | {st.median(sB):.0f} | {st.median(sq):.0f} | {st.median([r['win'] for r in L]):.0f}/{st.median([r['cap'] for r in L]):.0f} | {pa:.3f}")
