"""Stage 3e identity sample, shared by test_toy2.Stage3e (test 1) and check_ident3e.py.  The builders take the modules as arguments so
that the same configurations can be built against the old code (git archive of the base commit) and the new one.  Only fields that
exist on the base commit are used."""
import hashlib
import pickle

SMALL = dict(n_total=6000, dead=(0, 0.1, 0.1))


def cfgs(toy2, batch3d, batch3c):
    """name -> Cfg, one per sender family (S5, C1, C3, today's Realtime) plus a call cell and an outage-free link cell."""
    out = {}

    def small(c, **kw):
        for k, v in dict(SMALL, **kw).items():
            setattr(c, k, v)
        c.want_trace = True
        return c

    out["S5.CL@b"] = small(batch3d.s5_cfg("CL@b", 0, 0.4, 0.0))
    out["S5.OV@b"] = small(batch3d.s5_cfg("OV@b", 1, 0.4, 0.0), dead=None, n_total=6000)
    c = batch3d.s5_cfg("K-L10", 1, 0.12, 0.0)
    c.want_trace = True
    out["S5.K-L10"] = c
    out["C1.CL@b"] = small(batch3d.cmp_cfg("CL@b", "C1", 0))
    out["C3.OV@b"] = small(batch3d.cmp_cfg("OV@b", "C3", 1), dead=None, n_total=6000)
    c = batch3d.cmp_cfg("K-I2", "C1", 0)
    c.want_trace = True
    out["C1.K-I2"] = c
    lc, d = batch3d.base("CL@b")
    d.update(pause_B=10 ** 9)
    out["Ert.CL@b"] = small(toy2.Cfg(cell=lc, sender="S1", recv="R0e", seed=0, delta=50.0, rho=1.0, check_p1=False, d_arm="EWMA",
                                     budget="resid", floor_send=False, evict_window=200, r0e_max=500, **d))
    return out


def digest(r):
    """key -> sha256 of its repr; the gauge dict one level down (new gauges are allowed, so they are compared by name)."""
    out = {}
    for k, v in r.items():
        if k == "gauges":
            out[k] = {g: hashlib.sha256(repr(x).encode()).hexdigest() for g, x in v.items()}
        elif k.startswith("_"):
            out[k] = hashlib.sha256(pickle.dumps(list(v))).hexdigest()
        else:
            out[k] = hashlib.sha256(repr(v).encode()).hexdigest()
    return out
