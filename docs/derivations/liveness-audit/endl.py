# usage: endl.py OUT DIR:ARM,ARM ...  last line per tag(+site) per file; median/min/max per field across files
import sys, re, os, glob, statistics as st
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from plib import parse, ANSI
from collections import defaultdict
out = open(sys.argv[1], 'w')
for spec in sys.argv[2:]:
    d, arms = spec.rsplit(':', 1)
    for arm in arms.split(','):
        for side in 'cs':
            files = sorted(glob.glob(os.path.join(d, '*@def-%s-s*-%s.log' % (arm, side))))
            acc = defaultdict(lambda: defaultdict(list))
            for f in files:
                last = {}
                for line in open(f, errors='replace'):
                    line = ANSI.sub('', line).rstrip('\n')
                    if not line.startswith('[') or line.startswith('[DIAG]') or line.startswith('[SPAN]'): continue
                    tag = line.split(']')[0] + ']'
                    m = re.search(r'site=(\w+)', line); 
                    if m: tag += m.group(1)
                    m = re.search(r'phase=(\w+)', line)
                    if m: tag += m.group(1)
                    last[tag] = line
                for tag, line in last.items():
                    for k, v in parse(line).items(): acc[tag][k].append(v)
            out.write('=== %s %s side=%s files=%d\n' % (d[-30:], arm, side, len(files)))
            for tag in sorted(acc):
                items = []
                for k in acc[tag]:
                    vs = acc[tag][k]
                    if len(set(vs)) == 1: items.append('%s=%g' % (k, vs[0]))
                    else: items.append('%s=%g[%g-%g]' % (k, st.median(vs), min(vs), max(vs)))
                out.write('  %s %s\n' % (tag, ' '.join(items)))
out.close()
