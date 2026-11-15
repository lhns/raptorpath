# POST HOC. Tail retransmits by ORIGINAL placement path ([C8CONV-S] ro= deltas) and frontier-stall ms by blocking-hole owner
# path (st=) over the tail (from the C8CONV-S line nearest the last source [DIAG] line to the last one), vs the source split sp=.
import os, re, sys, statistics as st
D = sys.argv[1]; acc = {}
for f in sorted(os.listdir(D)):
    if not (f.startswith("c8-100@") and f.endswith("-c.log")): continue
    arm = re.search(r"def-(\w+)-s", f).group(1)
    lines = open(os.path.join(D, f), errors="replace").read().splitlines()
    di = [i for i, l in enumerate(lines) if l.startswith("[DIAG]") and not re.search(r"src=0sym", l)]
    ise = di[-1]
    cv = [i for i, l in enumerate(lines) if l.startswith("[C8CONV-S]")]
    a = max(i for i in cv if i <= ise); b = cv[-1]
    g = lambda l: {int(p): (int(sp), int(ro), int(ms)) for p, sp, ro, ms in re.findall(r"p(\d+):sp=(\d+) ro=(\d+) st=(\d+)ms", l)}
    A, B = g(lines[a]), g(lines[b])
    ro = {p: B[p][1] - A[p][1] for p in B}; stl = {p: B[p][2] - A[p][2] for p in B}
    acc.setdefault(arm, []).append((ro[0] / max(1, ro[0] + ro[1]), stl[0], stl[1], B[0][0] / (B[0][0] + B[1][0]), ro[0] + ro[1]))
for arm, v in acc.items():
    print(f"{arm}: tail retx whose ORIGINAL was on leg0 = {st.mean(x[0] for x in v):.2f} (n/row {st.mean(x[4] for x in v):.0f}); leg0 share of source = {st.mean(x[3] for x in v):.2f}; tail frontier-stall ms by owner leg0/leg1 = {st.mean(x[1] for x in v):.0f}/{st.mean(x[2] for x in v):.0f}")
