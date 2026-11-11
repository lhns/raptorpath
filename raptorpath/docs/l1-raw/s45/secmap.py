import re,sys
st=open(sys.argv[1],encoding='utf-8').read().split('\n')
gates=[l.strip() for l in open(sys.argv[2])]
sec='0';secs=[]
for i,l in enumerate(st,1):
    m=re.match(r'^##+ (\d+b?)\.',l)
    if m: sec=m.group(1)
    secs.append(sec)
for g in gates:
    pat=re.compile(re.escape(g)+r'(?![A-Z0-9_])')
    hits={}
    for i,l in enumerate(st,1):
        if pat.search(l): hits.setdefault(secs[i-1],[]).append(i)
    print(g, ' '.join(f"§{s}:{v[0]}({len(v)})" for s,v in hits.items()))
