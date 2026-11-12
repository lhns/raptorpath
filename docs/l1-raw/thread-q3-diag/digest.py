"""Digest the Q3 diagnostic session run dir: python -I digest.py RUN_DIR"""
import sys, os, re, glob, statistics as st

RUN = sys.argv[1]


def field(line, key):
    m = re.search(r'(?:^|\s)' + re.escape(key) + r'=([^\s|]+)', line)
    return m.group(1) if m else None


def xfer_lines(path, tag):
    out = []
    try:
        for l in open(path, encoding='utf-8', errors='replace'):
            l = re.sub(r'\x1b\[[0-9;]*m', '', l).strip()
            i = l.find(tag)
            if i >= 0 and 'phase=xfer' in l:
                out.append(l[i:])
    except FileNotFoundError:
        pass
    return out


def mbps(path):
    try:
        s = open(path, encoding='utf-8', errors='replace').read()
    except FileNotFoundError:
        return None
    m = re.findall(r'"mbps":\s*([0-9.]+)', s)
    return float(m[-1]) if m else None


def stat_summary(path):
    """Per 100 ms interval: per-CPU busy fraction (non-idle, non-iowait) and
    softirq; keep intervals where the system is >= 1.5 cores busy (the
    transfer); report median over them of: total busy cores, max-vCPU busy,
    max-vCPU softirq, idle cores, steal."""
    snaps = []
    cur = None
    try:
        for l in open(path):
            p = l.split()
            if not p:
                continue
            if p[0] == 'T':
                cur = {'t': float(p[1]), 'cpus': {}}
                snaps.append(cur)
            elif p[0].startswith('cpu') and p[0] != 'cpu' and cur is not None:
                cur['cpus'][p[0]] = list(map(int, p[1:9]))
    except FileNotFoundError:
        return None
    rows = []
    for a, b in zip(snaps, snaps[1:]):
        per = []
        for c in b['cpus']:
            if c not in a['cpus']:
                continue
            d = [y - x for x, y in zip(a['cpus'][c], b['cpus'][c])]
            tot = sum(d)
            if tot <= 0:
                continue
            user, nice, sys_, idle, iow, irq, sirq, steal = d
            per.append(((tot - idle - iow) / tot, sirq / tot, steal / tot, (user + nice) / tot, sys_ / tot))
        if per:
            rows.append(per)
    act = [r for r in rows if sum(x[0] for x in r) >= 1.5]
    if not act:
        return None
    med = lambda xs: st.median(xs)
    return {
        'n': len(act),
        'busy_cores': med([sum(x[0] for x in r) for r in act]),
        'max_cpu_busy': med([max(x[0] for x in r) for r in act]),
        'p90_max_cpu_busy': sorted(max(x[0] for x in r) for r in act)[int(0.9 * (len(act) - 1))],
        'sirq_cores': med([sum(x[1] for x in r) for r in act]),
        'max_cpu_sirq': med([max(x[1] for x in r) for r in act]),
        'steal_cores': med([sum(x[2] for x in r) for r in act]),
        'user_cores': med([sum(x[3] for x in r) for r in act]),
        'sys_cores': med([sum(x[4] for x in r) for r in act]),
        'ncpu': len(act[0]),
    }


for drv in sorted(glob.glob(os.path.join(RUN, '*-drv.out'))):
    tag = os.path.basename(drv)[:-8]
    base = os.path.join(RUN, tag)
    print('=' * 100)
    print(tag, 'mbps', mbps(base + '-c.log'))
    ss = stat_summary(base + '-stat.txt')
    if ss:
        print('  VM  ' + ' '.join(f'{k}={v:.3f}' if isinstance(v, float) else f'{k}={v}' for k, v in ss.items()))
    for side, f in (('cli', base + '-c.log'), ('srv', base + '-s.log')):
        for l in xfer_lines(f, '[TASK]'):
            print(f'  {side} TASK {field(l,"task"):18s} cores={field(l,"cores")} busy={field(l,"busy")} '
                  f'polls={field(l,"polls")} cpu/poll={field(l,"cpu_per_poll_us")} '
                  f'asl_vol={field(l,"asl_vol_us")} asl_inv={field(l,"asl_inv_us")} asl_none={field(l,"asl_none_us")} '
                  f'vcsw={field(l,"vcsw")} ivcsw={field(l,"ivcsw")} wall_s={field(l,"wall_s")}')
        for l in xfer_lines(f, '[QSOCK]'):
            print(f'  {side} QSOCK {field(l,"sock")} send_calls={field(l,"send_calls")} gso={field(l,"gso")} '
                  f'us/call={field(l,"send_us_per_call")} send_frac={field(l,"send_frac")} recv_calls={field(l,"recv_calls")} '
                  f'recv_dg={field(l,"recv_dg")} recv_frac={field(l,"recv_frac")} wb={field(l,"send_wb")}')
        for l in xfer_lines(f, '[IOWN]'):
            print(f'  {side} IOWN p{field(l,"path")} asleep_frac={field(l,"asleep_frac")} q_wall_us={field(l,"q_wall_us")} '
                  f'snd={field(l,"qk_snd")} rd={field(l,"qk_rd")} pub={field(l,"qk_pub")} '
                  f'rx_full={field(l,"rx_full")}/{field(l,"rx_full_us")}us ack_full={field(l,"ack_full")}/{field(l,"ack_full_us")}us '
                  f'fl={field(l,"fl_sends")}/{field(l,"fl_full")}/{field(l,"fl_full_us")}us tx_dg={field(l,"tx_dg")} rx_dg={field(l,"rx_dg")} ack_dg={field(l,"ack_dg")} wall_s={field(l,"wall_s")}')
        for l in xfer_lines(f, '[THR] sum'):
            print(f'  {side} THRsum cores={field(l,"cores")} busy_s={field(l,"busy_s")} wall_s={field(l,"wall_s")}')
