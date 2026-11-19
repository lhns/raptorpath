# usage: live.py OUT DIR:ARM[,ARM] ...   per-tick stats of [DIAG]/[SPAN] (sender = -c.log) + end-of-run lines
import sys, os, glob
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from collections import Counter, defaultdict
from plib import parse, ANSI
def stats(vals):
    vals = [v for v in vals]
    if not vals: return None
    c = Counter(vals); mode, mc = c.most_common(1)[0]
    s = sorted(vals); n = len(s)
    q = lambda f: s[min(n - 1, int(f * n))]
    return (n, mode, mc / n, s[0], q(.1), q(.5), q(.9), s[-1], len(c))
out = open(sys.argv[1], "w")
for spec in sys.argv[2:]:
    d, arms = spec.rsplit(':', 1)
    for arm in arms.split(','):
        for side, tags in (('c', ('[DIAG]', '[SPAN]')), ('s', ('[DIAG]', '[SPAN]'))):
            files = sorted(glob.glob(os.path.join(d, '*@def-%s-s*-c.log' % arm if side == 'c' else '*@def-%s-s*-s.log' % arm)))
            per = defaultdict(list); endl = defaultdict(list)
            for f in files:
                for line in open(f, errors='replace'):
                    line = ANSI.sub('', line).rstrip('\n')
                    if not line.startswith('['): continue
                    tag = line.split(']')[0] + ']'
                    if tag in tags:
                        p = parse(line)
                        t = p.get('t', 0)
                        if t < 2: continue
                        if tag == '[DIAG]' and p.get('src', 0) <= 0: continue
                        for k, v in p.items(): per[tag + k].append(v)
                    else:
                        endl[tag].append(line)
            out.write('=== %s %s side=%s files=%d\n' % (d.split(os.sep)[-5] if False else d[-30:], arm, side, len(files)))
            for k in sorted(per):
                s = stats(per[k])
                out.write('  %-34s n=%d mode=%g share=%.3f min=%g p10=%g p50=%g p90=%g max=%g distinct=%d\n' % ((k,) + s))
            for tag in sorted(endl):
                ls = endl[tag]
                out.write('  END %s x%d last: %s\n' % (tag, len(ls), ls[-1][:600]))
out.close()
