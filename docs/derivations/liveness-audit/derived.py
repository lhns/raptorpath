# usage: derived.py DIR:ARM,ARM ...   per-tick derived shares from sender [DIAG]/[SPAN]
import sys, os, glob, re
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from plib import parse, ANSI
for spec in sys.argv[1:]:
    d, arms = spec.rsplit(':', 1)
    for arm in arms.split(','):
        files = sorted(glob.glob(os.path.join(d, '*@def-%s-s*-c.log' % arm)))
        c = dict(span=0, astar_eq_w=0, astar_ge_w=0, rr05=0, rr0=0, diag=0, cw_max=0, cw_eq10k=0, rtt_gt20=0, nb10=0, p1infl9k=0)
        fin = []
        for f in files:
            lastd = None
            for line in open(f, errors='replace'):
                line = ANSI.sub('', line)
                if line.startswith('[SPAN]'):
                    p = parse(line)
                    if p.get('t', 0) < 2: continue
                    c['span'] += 1
                    a, w = p.get('a_star'), p.get('wsize')
                    if a is not None and w is not None:
                        c['astar_eq_w'] += a == w; c['astar_ge_w'] += a >= w
                    c['rr05'] += p.get('rr') == 0.5; c['rr0'] += p.get('rr') == 0
                elif line.startswith('[DIAG]'):
                    p = parse(line)
                    if p.get('t', 0) < 2 or p.get('src', 0) <= 0: continue
                    c['diag'] += 1; lastd = p
                    np_ = p.get('np', 1)
                    c['cw_max'] += p.get('cwnd') == 10000 * np_
                    c['cw_eq10k'] += p.get('cwnd') == 10000
                    c['rtt_gt20'] += p.get('rtt', 0) > 20
                    c['nb10'] += p.get('nbud') == 10
                    c['p1infl9k'] += p.get('p1.infl.0', 0) > 9000
            if lastd: fin.append(lastd)
        sh = lambda k, den: '%.3f' % (c[k] / c[den]) if c[den] else '-'
        def fm(k):
            v = sorted(x.get(k, float('nan')) for x in fin); return '%g' % v[len(v)//2] if v else '-'
        cap = [x.get('eb_end.0', 0) / max(1, x.get('eb_bursts', 1)) for x in fin]
        store = [x.get('eb_end.1', 0) / max(1, x.get('eb_bursts', 1)) for x in fin]
        pt = ['%g/%g' % (x.get('mpr.plost', 0), x.get('mpr.taper', 0)) for x in fin[:2]]
        print('%-6s %s rows=%d span=%d a*=W %s a*>=W %s rr=.5 %s rr=0 %s | diag=%d cwnd=MAX*np %s cwnd=10k %s rtt>20ms %s nbud=10 %s p1infl>9k %s | eb cap %.4f store %.3f | plost/taper %s fast %s sweeps %s gapdrop %s retx %s srel %s/%s cod %s' % (
            d[-34:-25], arm, len(files), c['span'], sh('astar_eq_w', 'span'), sh('astar_ge_w', 'span'), sh('rr05', 'span'), sh('rr0', 'span'),
            c['diag'], sh('cw_max', 'diag'), sh('cw_eq10k', 'diag'), sh('rtt_gt20', 'diag'), sh('nb10', 'diag'), sh('p1infl9k', 'diag'),
            sorted(cap)[len(cap)//2] if cap else -1, sorted(store)[len(store)//2] if store else -1, ' '.join(pt), fm('mpr.fast'), fm('sweeps'), fm('gapdrop'), fm('retx'), fm('srel.0'), fm('srel.1'), fm('cum.1')))
