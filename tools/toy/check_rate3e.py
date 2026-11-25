"""Part RATE (registration 9 step 5): the offline simulation's rate law against raptorpath-math.

Every entry of rate_ref3e.json (r to 1e-9, W exactly), the A&S ports and the registration's 2.1 table (to 1e-5).  A pass writes out3e/PASS-rate; any
difference writes out3e/STOP with the cause and exits 1.

    python check_rate3e.py
"""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import toy2  # noqa: E402

OUT = os.path.join(HERE, "out3e")
TABLE = [(0.03, 0.0115, 10000.0, 115, 0.10687), (0.03, 0.0115, 12000.0, 138, 0.09982), (0.03, 0.0115, 8000.0, 92, 0.11566),
         (0.03, 0.010, 10000.0, 100, 0.11234), (0.03, 0.100, 11250.0, 200, 0.08754), (0.03, 0.098, 9000.0, 200, 0.08754),
         (0.03, 0.0115, 1000.0, 16, 0.22952), (0.10, 0.101, 50.0, 16, 0.38355),
         (0.0344, 0.0115, 10000.0, 115, 0.11566), (0.05, 0.100, 11250.0, 200, 0.11609), (0.1, 0.100, 11250.0, 200, 0.19542),
         (0.3, 0.100, 11250.0, 200, 0.50000), (0.0344, 0.098, 9000.0, 200, 0.09313), (0.0344, 0.0115, 8000.0, 92, 0.12543),
         (0.135, 0.101, 50.0, 16, 0.42470)]                                    # registration 2.1 and Appendix A: (eps, srtt, rate, W, r) at delta = 50


def check():
    """The list of differences (empty = pass)."""
    with open(os.path.join(HERE, "rate_ref3e.json")) as f:
        ref = json.load(f)
    bad = []
    for e in ref["grid"]:
        L = toy2.rate_law(e["eps"], e["srtt"], e["rate"], e["delta"])
        if L["W"] != e["W"]:
            bad.append("W %s != %s at %s" % (L["W"], e["W"], e))
        if abs(L["r"] - e["r"]) > 1e-9:
            bad.append("r %.12g != %.12g at %s" % (L["r"], e["r"], e))
        if abs(min(max(L["W_pre"], 16.0), 512.0) - e["W_clamped"]) > 1e-9:
            bad.append("W (before rounding, after the clamp) differs at %s" % e)
        if abs(L["r_sat"] - e["r_sat"]) > 1e-9:
            bad.append("r_sat differs at %s" % e)
    for z, v in ref["survival"]:
        if abs(toy2.normal_survival_as(z) - v) > 1e-12:
            bad.append("normal_survival at %g" % z)
    for p, v in ref["quantile"]:
        if abs(toy2.normal_quantile_as(p) - v) > 1e-9:
            bad.append("normal_quantile at %g" % p)
    for r, e, w, s2, v in ref["p_fec"]:
        if abs(toy2.p_fec_normal_as(r, e, w, s2) - v) > 1e-12:
            bad.append("p_fec_normal at %s" % [r, e, w, s2])
    for eps, srtt, rate, W, r in TABLE:
        L = toy2.rate_law(eps, srtt, rate, 50.0)
        if L["W"] != W or abs(L["r"] - r) > 1e-5:
            bad.append("the 2.1 table at %s: W %s r %.6g" % ((eps, srtt, rate), L["W"], L["r"]))
    return bad


def main():
    bad = check()
    os.makedirs(OUT, exist_ok=True)
    if bad:
        with open(os.path.join(OUT, "STOP"), "w") as f:
            f.write("part RATE: %d differences, first: %s\n" % (len(bad), bad[0]))
        print("RATE CHECK: FAIL (%d differences)" % len(bad), bad[:3])
        return 1
    open(os.path.join(OUT, "PASS-rate"), "w").close()
    print("RATE CHECK: PASS (rate_ref3e.json entries to 1e-9 in r and exact in W, the A&S ports, the registration's 2.1 table to 1e-5)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
