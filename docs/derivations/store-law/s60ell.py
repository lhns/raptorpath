"""s60: does the row's lifetime echo minimum l rise with the static pool W? Row-level Spearman rank correlation of l
against W, from the per-row tables of s60boot.out (s56, 600 MB / 5 GB rows) and s60p27.out (p27, 100 MB rows), with
an exact permutation p-value over the W labels (one-sided, rho > 0; 20 000 shuffles, fixed seed).

Usage: python -I s60ell.py s60boot.out s60p27.out
"""
import random
import re
import sys


def ranks(v):
    o = sorted(range(len(v)), key=lambda i: v[i])
    r = [0.0] * len(v)
    i = 0
    while i < len(o):
        j = i
        while j + 1 < len(o) and v[o[j + 1]] == v[o[i]]:
            j += 1
        for k in range(i, j + 1):
            r[o[k]] = (i + j) / 2.0
        i = j + 1
    return r


def spearman(x, y):
    rx, ry = ranks(x), ranks(y)
    n = len(x)
    mx, my = sum(rx) / n, sum(ry) / n
    num = sum((a - mx) * (b - my) for a, b in zip(rx, ry))
    den = (sum((a - mx) ** 2 for a in rx) * sum((b - my) ** 2 for b in ry)) ** 0.5
    return num / den


def report(name, pts):
    w = [p[0] for p in pts]
    l = [p[1] for p in pts]
    rho = spearman(w, l)
    rnd = random.Random(60)
    hits = 0
    for _ in range(20000):
        s = w[:]
        rnd.shuffle(s)
        hits += spearman(s, l) >= rho
    print('%s: rows %d  Spearman rho(l, W) %+.3f  one-sided permutation p %.4f' % (name, len(pts), rho, hits / 20000))


rows = {}
for L in open(sys.argv[1]):
    m = re.match(r'(c\S+) P(\d+) p(\d) \| l ([\d. ]+) \| bootq', L)
    if m:
        cell, P, leg = m.group(1), int(m.group(2)), int(m.group(3))
        W = 2 * P if cell.startswith('c8') else P
        rows.setdefault(('s56', cell, leg), []).extend((W, float(x)) for x in m.group(4).split())
for L in open(sys.argv[2]):
    m = re.match(r'(c\S+) (\d+) \| b0p0 .* \| l0 ([\d. ]+)$', L.strip())
    if m:
        rows.setdefault(('p27', m.group(1), 0), []).extend((int(m.group(2)), float(x)) for x in m.group(3).split())
for k in sorted(rows):
    report('%s %s leg %d' % k, rows[k])
