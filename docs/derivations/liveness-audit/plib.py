# shared parser for live.py, endl.py and derived.py: one log line -> {key: float}
# usage: live.py OUT DIR:ARM[,ARM] ...   per-tick stats of [DIAG]/[SPAN] (sender = -c.log) + end-of-run lines
import sys, re, os, glob, statistics as st
from collections import Counter, defaultdict
ANSI = re.compile(r'\x1b\[[0-9;]*m')
TOK = re.compile(r'([A-Za-z_][\w.]*?)=([^\s\]\[]+)')
NUM = re.compile(r'^-?\d+(?:\.\d+)?(?:e-?\d+)?')
def parse(line):
    line = ANSI.sub('', line).replace('Some(', '').replace(')', '')
    out = {}
    # flatten bracket groups: mpr[...] wait[...] wake[...] dgq0[...]
    def grp(m):
        pre = m.group(1)
        for k, v in TOK.findall(m.group(2)):
            out[pre + '.' + k] = v
        return ' '
    line2 = re.sub(r'(\w+)\[([^\]]*)\]', grp, line)
    for k, v in TOK.findall(line2):
        out.setdefault(k, v)
    # p0:infl=.. style: tokens like p0:infl -> captured as 'infl' after 'p0:'; handle explicitly
    for m in re.finditer(r'(p\d):(\w+)=([^\s]+)', line):
        out[m.group(1) + '.' + m.group(2)] = m.group(3)
    res = {}
    for k, v in out.items():
        v = re.sub(r'sym/s$', '', v)
        parts = re.split(r'[/]', v)
        if len(parts) == 1:
            n = NUM.match(v)
            if n: res[k] = float(n.group(0))
        else:
            for i, p in enumerate(parts):
                n = NUM.match(p.split(':')[-1] if ':' in p and not p[0].isdigit() else p)
                if n: res[k + '.' + str(i)] = float(n.group(0))
    return res
