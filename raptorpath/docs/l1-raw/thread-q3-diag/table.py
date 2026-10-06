import sys, re
cur = None
rows = []
for l in open(sys.argv[1], encoding='utf-8'):
    l = l.rstrip('\n')
    m = re.match(r'^([A-E]-\S+) mbps (\S+)', l)
    if m:
        cur = {'tag': m.group(1), 'mbps': m.group(2)}
        rows.append(cur)
        continue
    if cur is None:
        continue
    def f(k):
        m = re.search(r'(?:^|\s)' + re.escape(k) + r'=(\S+)', l)
        return m.group(1) if m else '-'
    if l.strip().startswith('VM'):
        cur['vm'] = f'busy={f("busy_cores")} maxcpu={f("max_cpu_busy")} p90max={f("p90_max_cpu_busy")} sirq={f("sirq_cores")}'
    m = re.match(r'\s+(cli|srv) TASK (\S+)', l)
    if m:
        cur.setdefault('t', {}).setdefault(m.group(1) + ':' + re.sub(r'-(client|server)', '', m.group(2)), []).append(
            f'{f("cores")}/{f("busy")}')
    m = re.match(r'\s+(cli|srv) IOWN (p\d)', l)
    if m and f('wall_s') != '0.000':
        cur.setdefault('io', []).append(f'{m.group(1)}{m.group(2)} asl={f("asleep_frac")} fl={f("fl")}')
    m = re.match(r'\s+(cli|srv) QSOCK', l)
    if m and f('send_calls') != '0':
        cur.setdefault('qs', []).append(f'{m.group(1)} gso={f("gso")} sfrac={f("send_frac")}')
keys = ['srv:receiver', 'cli:sender', 'cli:owner-p0', 'cli:qconn-p0', 'cli:qconn-p1', 'srv:qconn-p0', 'srv:qconn-p1', 'srv:owner-p0', 'cli:perf', 'srv:perf']
for r in rows:
    t = r.get('t', {})
    print(r['tag'], r['mbps'], '|', r.get('vm', ''))
    print('    ' + ' '.join(f'{k}={",".join(t[k])}' for k in keys if k in t))
    print('    ' + '; '.join(r.get('io', [])) + ' | ' + '; '.join(r.get('qs', [])))
