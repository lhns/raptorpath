import re, glob, os, collections

base = r'C:\Users\pierr\Documents\claude\raptorpath\.claude\worktrees\agent-afd78f5bf23ca54a2\raptorpath\docs\l1-raw'
pat = re.compile(r'CTLDLINE (\S+) rep=(\d+) site=(cli|srv) \[CTLD\](.*)')
pair = re.compile(r'p(\d+) tx=(\d+) rx=(\d+)')

# last (cumulative) reading per (file, arm, rep, site, path)
last = {}
for f in sorted(glob.glob(os.path.join(base, '*.log'))):
    for ln in open(f, encoding='utf-8', errors='replace'):
        m = pat.search(ln)
        if not m:
            continue
        arm, rep, site, rest = m.group(1), m.group(2), m.group(3), m.group(4)
        for p, tx, rx in pair.findall(rest):
            last[(os.path.basename(f), arm, rep, site, p)] = (int(tx), int(rx))

# join cli/srv on the same (file, arm, rep, path)
rows = collections.defaultdict(dict)
for (f, arm, rep, site, p), v in last.items():
    rows[(f, arm, rep, p)][site] = v

agg = collections.defaultdict(lambda: [0, 0, 0])  # cell -> [cli_tx, srv_rx, n]
for k, v in sorted(rows.items()):
    if 'cli' not in v or 'srv' not in v:
        continue
    f, arm, rep, p = k
    cli_tx = v['cli'][0]
    srv_rx = v['srv'][1]
    cell = arm.split('-')[0]
    a = agg[cell]
    a[0] += cli_tx
    a[1] += srv_rx
    a[2] += 1

print('%-8s %14s %14s %10s %8s' % ('cell', 'cli_tx(frames)', 'srv_rx(frames)', 'lost_frac', 'legs'))
for cell in sorted(agg):
    tx, rx, n = agg[cell]
    if tx == 0:
        continue
    print('%-8s %14d %14d %10.6f %8d' % (cell, tx, rx, (tx - rx) / tx, n))
