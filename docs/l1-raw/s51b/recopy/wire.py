import sys,re,glob,os,statistics as st
for d in sys.argv[1:]:
    R={}
    for f in sorted(glob.glob(os.path.join(d,"*-drv.out"))):
        arm=re.search(r"@def-([A-Z0-9]+)-",f).group(1); t=open(f,errors="replace").read()
        sent=[int(x) for x in re.findall(r"QDISC cli\d: .*?Sent (\d+) bytes",t)]
        s=float(re.search(r'"mean_s":([0-9.]+)',t).group(1)); mb=float(re.search(r'"mean_mbps":([0-9.]+)',t).group(1))
        R.setdefault(arm,[]).append((sum(sent)*8/s/1e6, mb, sum(sent)/600e6))
    print(d.split('/')[-4])
    for a,v in R.items(): print(f" {a} wire_Mbit/s={st.median([x[0] for x in v]):.1f} good={st.median([x[1] for x in v]):.1f} wire/payload={st.median([x[2] for x in v]):.3f}")
