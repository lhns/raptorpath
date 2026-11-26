import sys,glob,os,statistics as st
sys.path.insert(0,os.path.dirname(__file__))
from p import parse
D=sys.argv[1]; cell=sys.argv[2] if len(sys.argv)>2 else 'c8-100'
bins=[(0,1),(1,4),(4,8),(8,16),(16,24),(24,32),(32,40),(40,60)]
for arm in ['BASE','CAND']:
    agg={b:{k:[] for k in ['B0','B1','fl','cw','win','cap','w0','w1','r0','r1','q0','q1','i0','i1']} for b in bins}
    for f in sorted(glob.glob(f'{D}/{cell}@def-{arm}-*-c.log')):
        rows=parse(f)
        for b in bins:
            sel=[r for r in rows if b[0]<=r['t']<b[1]]
            if not sel: continue
            for k,fn in [('B0',lambda r:r['p'][0].get('B')),('B1',lambda r:r['p'].get(1,{}).get('B')),('fl',lambda r:r['fl']),('cw',lambda r:r['cw']),('win',lambda r:r['win']),('cap',lambda r:r['cap']),
                         ('w0',lambda r:r['p'][0].get('wrtt')),('w1',lambda r:r['p'].get(1,{}).get('wrtt')),('r0',lambda r:r['p'][0].get('rtt')),('r1',lambda r:r['p'].get(1,{}).get('rtt')),
                         ('q0',lambda r:r['p'][0].get('qcwnd')),('q1',lambda r:r['p'].get(1,{}).get('qcwnd')),('i0',lambda r:r['p'][0].get('infl')),('i1',lambda r:r['p'].get(1,{}).get('infl'))]:
                v=[fn(r) for r in sel if fn(r) is not None]
                if v: agg[b][k].append(st.median(v))
    print('==',cell,arm,'(median over rows of row-median per bin)')
    for b in bins:
        a=agg[b]
        if not a['fl']: continue
        m=lambda k: (f"{st.median(a[k]):.0f}" if a[k] else '-')
        print(f"[{b[0]:>2},{b[1]:>2}) n={len(a['fl'])} win {m('win')}/{m('cap')} cw {m('cw')} fl {m('fl')} | B0 {m('B0')} infl0 {m('i0')} wrtt0 {m('w0')} rtt0 {m('r0')} qcw0 {m('q0')} | B1 {m('B1')} infl1 {m('i1')} wrtt1 {m('w1')} rtt1 {m('r1')} qcw1 {m('q1')}")
