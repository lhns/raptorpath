# usage: count.py liveness-audit.md   class counts of the section 1 table (all seats, the Auto and the Bulk columns)
import sys, re, collections
COLS = ['c2 B', 'c2 A', 'c2 R', 'c8 B', 'c8 A', 'c8 R', 'E']
def cls(c):
    c = c.strip()
    if c.startswith('PIN('): return 'PIN'
    return c.replace('\*', '+').replace('*', '')
lines = [l for l in open(sys.argv[1], encoding='utf-8') if re.match(r'\| [A-Z]\d', l)]
tot = collections.Counter(); per = [collections.Counter() for _ in COLS]; eng = 0
for l in lines:
    cells = [cls(c) for c in l.split('|')[2:9]]
    if all(c == 'n/a' for c in cells): continue
    eng += 1
    for i, c in enumerate(cells): tot[c] += 1; per[i][c] += 1
print('table lines %d, engine lines %d, cells %d' % (len(lines), eng, eng * 7))
print('all', dict(tot))
for i, n in enumerate(COLS): print(n, dict(per[i]))
