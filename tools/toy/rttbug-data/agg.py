import json, math, collections
d = json.load(open('rttbug-out/cmp.json'))
arms = collections.OrderedDict()
for k, v in d.items():
    n, s = k.split('|'); arms.setdefault(n, []).append(v)
M = [("share_any", 100, "%", 3), ("share_T", 100, "%", 3), ("recovery_time", 1, " s", 2), ("srtt", 1000, " ms", 3), ("o_hat_end", 1000, " ms", 3), ("dh_p50", 1000, " ms", 3)]
def num(x): return isinstance(x, (int, float)) and not isinstance(x, bool) and not math.isnan(x) and not math.isinf(x)
out = []
out.append("### Affected samples per arm (12 seeds, unfixed run; bad = RTT sample on a seq that had not arrived when the report was made)\n")
out.append("| cell.arm | seeds | bad / samples | seeds with bad > 0 | seeds where any readout differs after the fix |")
out.append("|---|---|---|---|---|")
for n, vs in arms.items():
    bad = sum(v['cnt']['rfc_bad'] + v['cnt']['p_bad'] for v in vs); tot = sum(v['cnt']['rfc_total'] + v['cnt']['p_total'] for v in vs)
    sb = sum(1 for v in vs if v['cnt']['rfc_bad'] + v['cnt']['p_bad'] > 0)
    diff = sum(1 for v in vs if any(v['base'].get(m[0]) != v['fix'].get(m[0]) for m in M))
    out.append("| %s | %d | %d / %d | %d | %d |" % (n, len(vs), bad, tot, sb, diff))
for m, sc, u, p in M:
    out.append("\n### %s (mean over seeds, unfixed -> fixed; paired delta mean [min, max]; units %s)\n" % (m, (u.strip() or "fraction x100 = %")))
    out.append("| cell.arm | unfixed | fixed | delta mean [min, max] | n numeric |")
    out.append("|---|---|---|---|---|")
    for n, vs in arms.items():
        pr = [(v['base'].get(m), v['fix'].get(m)) for v in vs]
        pr = [(a * sc, b * sc) for a, b in pr if num(a) and num(b)]
        if not pr:
            out.append("| %s | n/a | n/a | n/a | 0 of %d |" % (n, len(vs))); continue
        a = sum(x for x, _ in pr) / len(pr); b = sum(y for _, y in pr) / len(pr); dl = [y - x for x, y in pr]
        out.append("| %s | %.*f | %.*f | %+.*f [%+.*f, %+.*f] | %d of %d |" % (n, p, a, p, b, p, b - a, p, min(dl), p, max(dl), len(pr), len(vs)))
open('agg.md', 'w').write("\n".join(out))
print("\n".join(out))
