# POST HOC. Per arm and send leg: retransmits sent (fp/on 'on'), live-flight path at fire ('fp'),
# original leg ('ro'), truth loss per leg, receiver duplicate arrivals per arrival leg, and receiver
# frontier unblocks (>= 5 ms stalls) credited per arrival leg, split main / tail.
# Tail on the receiver = [C8CONV-R] lines with index >= t_src_end (lines are ~1/s from start; approximate).
import sys, statistics as st
sys.path.insert(0, sys.argv[1]); from parse import rows
D = sys.argv[2]
for cell in sys.argv[3].split(","):
    acc = {}
    for r in rows(D, cell):
        T = r["T"]; e = T[-1]
        ise = max(i for i, x in enumerate(T) if x["src"] > 0)
        tse = T[ise]["t"]; s = T[ise]
        R = r["R"]; k = min(len(R) - 1, int(tse)); Rt, Re = R[k], R[-1]
        tr = r["truth"]; h = r["hold"]
        d = dict(on0=e["on0"], on1=e["on1"], fp0=e["fp0"], fp1=e["fp1"], ro0=e.get("ro0", 0), ro1=e.get("ro1", 0),
                 sp0=e.get("sp0", 0), sp1=e.get("sp1", 0),
                 l0=tr[0][2], l1=tr[1][2], lost0=tr[0][1], lost1=tr[1][1], eg0=tr[0][0], eg1=tr[1][0],
                 dup0=Re.get("dup0", 0), dup1=Re.get("dup1", 0),
                 tdup0=Re.get("dup0", 0) - Rt.get("dup0", 0), tdup1=Re.get("dup1", 0) - Rt.get("dup1", 0),
                 ton0=e["on0"] - s["on0"], ton1=e["on1"] - s["on1"],
                 mun0=Rt.get("unb0", 0), mun1=Rt.get("unb1", 0), mums0=Rt.get("ubms0", 0), mums1=Rt.get("ubms1", 0),
                 tun0=Re.get("unb0", 0) - Rt.get("unb0", 0), tun1=Re.get("unb1", 0) - Rt.get("unb1", 0),
                 tums0=Re.get("ubms0", 0) - Rt.get("ubms0", 0), tums1=Re.get("ubms1", 0) - Rt.get("ubms1", 0),
                 F1o0=h.get(0, {}).get("hy_n", 0) + h.get(0, {}).get("cx_n", 0),
                 F1o1=h.get(1, {}).get("hy_n", 0) + h.get(1, {}).get("cx_n", 0),
                 cx0=h.get(0, {}).get("cx_p50_us", 0) / 1e3, cx1=h.get(1, {}).get("cx_p50_us", 0) / 1e3,
                 fires=e["retx"], tail=r["secs"] - tse)
        acc.setdefault(r["arm"], []).append(d)
    for arm, v in sorted(acc.items()):
        m = lambda k: st.mean(x[k] for x in v)
        print(f"== {cell} {arm} n={len(v)} tail={m('tail'):.2f}s fires/row={m('fires'):.0f}")
        print(f"  retx SENT on leg0/leg1: {m('on0'):.0f}/{m('on1'):.0f} (leg1 share {m('on1')/(m('on0')+m('on1')):.2f}); "
              f"by ORIGINAL leg0/leg1: {m('ro0'):.0f}/{m('ro1'):.0f}; live-flight leg at fire 0/1: {m('fp0'):.0f}/{m('fp1'):.0f}")
        print(f"  truth loss leg0/leg1: {m('l0'):.4f}/{m('l1'):.4f} (lost dgrams {m('lost0'):.0f}/{m('lost1'):.0f}; source placed {m('sp0'):.0f}/{m('sp1'):.0f})")
        exp = m('l0') * (m('sp0') + m('on0')) + m('l1') * (m('sp1') + m('on1'))
        print(f"  fires expected from loss alone (orig+retx lost, uniform per-leg rate) = {exp:.0f} vs fired {m('fires'):.0f} -> ratio {m('fires')/exp:.2f}")
        print(f"  expected retx LOST per send leg (rate x sent): leg0 {m('l0')*m('on0'):.0f} ({m('l0')*100:.1f}%), leg1 {m('l1')*m('on1'):.0f} ({m('l1')*100:.1f}%)")
        print(f"  rx dup arrivals per arrival leg, whole run 0/1: {m('dup0'):.0f}/{m('dup1'):.0f} (per retx sent there {m('dup0')/max(1,m('on0')):.3f}/{m('dup1')/max(1,m('on1')):.3f}); tail: {m('tdup0'):.0f}/{m('tdup1'):.0f} vs tail retx sent {m('ton0'):.0f}/{m('ton1'):.0f}")
        print(f"  rx frontier unblocks (>=5ms) credited to arrival leg, MAIN 0/1: n {m('mun0'):.0f}/{m('mun1'):.0f}, mean stall ended {m('mums0')/max(1,m('mun0')):.0f}/{m('mums1')/max(1,m('mun1')):.0f} ms, total {m('mums0')/1e3:.1f}/{m('mums1')/1e3:.1f} s")
        print(f"                                               TAIL 0/1: n {m('tun0'):.0f}/{m('tun1'):.0f}, mean stall ended {m('tums0')/max(1,m('tun0')):.0f}/{m('tums1')/max(1,m('tun1')):.0f} ms, total {m('tums0')/1e3:.2f}/{m('tums1')/1e3:.2f} s")
        print(f"  [HOLD] holes-with-retx resolved by report (pruned-at-frontier excluded) orig0/orig1: {m('F1o0'):.0f}/{m('F1o1'):.0f}; closed_retx p50 from first report {m('cx0'):.0f}/{m('cx1'):.0f} ms")
