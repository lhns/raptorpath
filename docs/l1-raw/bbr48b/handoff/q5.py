import sys,glob,os,re,statistics as st
D=sys.argv[1]
# per-leg anchor bdp (sym), btlbw (sym/s), rtp_us, bbr_rs qbw (B/s), qrt_us, tx rate (dgr/s)
for cell in ['c1s-400','c1d-400','c2-100','c8-100']:
  for arm in ['BASE','CAND']:
    acc={}
    for f in sorted(glob.glob(f'{D}/{cell}@def-{arm}-*-c.log')):
        prev=None; per={}
        for ln in open(f,encoding='utf8',errors='replace'):
            if ln.startswith('[QCC]'):
                for m in re.finditer(r'p(\d):qm=(\w) qrt_us=(\d+) qsr_us=(\d+) qbw=(\d+)',ln):
                    per.setdefault(int(m[1]),{}).setdefault('qbw',[]).append(int(m[5])); per[int(m[1])].setdefault('qrt',[]).append(int(m[3]))
            if not ln.startswith('[DIAG]'): continue
            t=float(re.search(r't=([\d.]+)s',ln).group(1))
            if t<5: continue
            for m in re.finditer(r' p(\d):infl=\d+/sinfl=\d+/bdp(\d+).*? btlbw=(\d+).*?rtp_us=(\d+)',ln):
                d=per.setdefault(int(m[1]),{}); d.setdefault('bdp',[]).append(int(m[2])); d.setdefault('bw',[]).append(int(m[3])); d.setdefault('rtp',[]).append(int(m[4]))
            tx={int(m[1]):int(m[2]) for m in re.finditer(r'dgq(\d)\[hand=\d+ tx=(\d+)',ln)}
            if prev and t-prev[0]>0:
                for k,v in tx.items():
                    if k in prev[1]: per.setdefault(k,{}).setdefault('txr',[]).append((v-prev[1][k])/(t-prev[0]))
            prev=(t,tx)
        for k,d in per.items():
            a=acc.setdefault(k,{})
            for kk,v in d.items():
                if v: a.setdefault(kk,[]).append(st.median(v))
    for k in sorted(acc):
        a=acc[k]; m=lambda kk: st.median(a[kk]) if kk in a else float('nan')
        txr=m('txr'); true_bdp=txr*m('rtp')/1e6
        s=f"{cell} {arm} p{k}: anchor btlbw {m('bw'):.0f}/s vs tx {txr:.0f}/s (x{m('bw')/txr:.1f}); anchor bdp {m('bdp'):.0f} vs tx*rtp {true_bdp:.0f} (x{m('bdp')/true_bdp:.1f}); rtp {m('rtp')/1000:.2f} ms"
        if 'qbw' in a: s+=f"; bbr_rs qbw {m('qbw'):.0f} B/s (= {m('qbw')/txr:.0f} B per tx dgr); qrt {m('qrt')/1000:.2f} ms"
        print(s)
