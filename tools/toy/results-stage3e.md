# Stage 3e results (offline simulation)

Registration: `docs/derivations/three-knobs-stage3e-registration.md` on `reg-stage3e` at c7021312 (Amendment 1, section 13).
Build brief: `docs/derivations/three-knobs-stage3e-build.md`, same commit.  Code base: `proof-s1` at b2addffb, branch `toy-stage3e`.
Nothing below has been run beyond the unit tests, the identity checks and the one timing run named in the build brief.

## Rate-law reference program

`rate_ref3e.json` holds the values of `raptorpath-math` (Appendix A's composition, extended to the grid eps in {0.01, 0.03, 0.0344, 0.05,
0.1, 0.135, 0.3} x srtt in {0.0115, 0.05, 0.1, 0.3} x rate in {50, 1000, 8000, 10000, 11250} x delta in {0.005, 0.5, 5, 50}, plus the A&S
survival and quantile values and a few `p_fec_normal` points).  It was produced by this scratch crate (outside the repository, with
`raptorpath-math` as a path dependency, base revision of `proof-s1`):

```rust
use raptorpath_math as m;

fn r(p: f64, srtt: f64, rate: f64, delta: f64) -> (f64, f64, f64, f64) {
    let t_tail = (1e-5 * m::zeta_of_delta(delta)).clamp(1e-9, 0.1);
    let w = m::derive_window(t_tail, p, 1.0, srtt, rate, 0.0).round().min(200.0);
    let mk = |tail: f64, late: bool| m::RateInputs {
        p_upper: p, sigma2: 1.0, mean_burst: 1.0, mass: m::MassStats::default(), tail_provision: true,
        window: w, t_symbols: (srtt * rate).max(1.0), srtt, t_sym: 1.0 / rate, codec_overhead: 0.004,
        tail_target: tail, bulk_late_is_fine: late, completion_exposure: 0.0, inner_feedback: 0.0,
        saturation_cap: true, max_overhead: 0.5,
    };
    let beta = m::bulkness_of_delta(delta);
    let a = m::controller_rate(&mk(t_tail, false));
    let l = m::controller_rate(&mk((1e-5f64 * 100.0).clamp(1e-9, 0.1), true));
    (w, (1.0 - beta) * a + beta * l, m::r_saturation(p, 1.0, w, srtt, 1.0 / rate), beta)
}

fn main() {
    let eps = [0.01, 0.03, 0.0344, 0.05, 0.1, 0.135, 0.3];
    let srtt = [0.0115, 0.05, 0.1, 0.3];
    let rate = [50.0, 1000.0, 8000.0, 10000.0, 11250.0];
    let delta = [0.005, 0.5, 5.0, 50.0];
    println!("{{");
    println!("\"grid\": [");
    let mut first = true;
    for &e in &eps { for &s in &srtt { for &rt in &rate { for &d in &delta {
        let (w, rr, rsat, beta) = r(e, s, rt, d);
        let t_tail = (1e-5 * m::zeta_of_delta(d)).clamp(1e-9, 0.1);
        let wr = m::derive_window(t_tail, e, 1.0, s, rt, 0.0);
        if !first { println!(","); }
        first = false;
        print!("  {{\"eps\": {:e}, \"srtt\": {:e}, \"rate\": {:e}, \"delta\": {:e}, \"W\": {}, \"W_clamped\": {:e}, \"r\": {:e}, \"r_sat\": {:e}, \"beta\": {:e}}}",
               e, s, rt, d, w, wr, rr, rsat, beta);
    }}}}
    println!("\n],");
    println!("\"survival\": [");
    let zs = [-9.0, -3.0, -1.0, 0.0, 0.5, 2.0, 4.5, 9.0];
    for (i, &z) in zs.iter().enumerate() {
        print!("  [{:e}, {:e}]{}", z, m::normal_survival(z), if i + 1 < zs.len() { ",\n" } else { "\n" });
    }
    println!("],");
    println!("\"quantile\": [");
    let ps = [1e-12, 1e-7, 0.05, 0.5, 0.95, 1.0 - 1e-7];
    for (i, &p) in ps.iter().enumerate() {
        print!("  [{:e}, {:e}]{}", p, m::normal_quantile(p), if i + 1 < ps.len() { ",\n" } else { "\n" });
    }
    println!("],");
    println!("\"p_fec\": [");
    let pf = [(0.1, 0.03, 100.0, 1.0), (0.3835, 0.1, 16.0, 1.0), (0.2, 0.03, 100.0, 1.0), (0.05, 0.03, 50.0, 1.0), (0.5, 0.135, 200.0, 1.0)];
    for (i, &(rr, e, w, s2)) in pf.iter().enumerate() {
        print!("  [{:e}, {:e}, {:e}, {:e}, {:e}]{}", rr, e, w, s2, m::p_fec_normal(rr, e, w, s2), if i + 1 < pf.len() { ",\n" } else { "\n" });
    }
    println!("]");
    println!("}}");
}
```

## Identity check (build brief 9 step 4)

`python check_ident3e.py`, one process, alone on the machine (the machine ran two long batches elsewhere):

```text
base b2addffb; 16 configurations, seed 0; 2139 old keys compared (trace and per-seq arrays as digests); new keys/gauges (allowed): 36
IDENTITY CHECK (proof stage 1): PASS
IDENTITY CHECK (stage 3e): PASS (A: base b2addffb on the proof-s1 sample reproduced; B: R0 equals repair off on 23 configurations, 0 differences; C: RL at delta 0.005 equals off, 0 differences; id3e keys, sender traces and event counters; one process)
```

The sample of B is the proof-s1 sample (16) plus one configuration per sender family and cell shape of `ident3e_cfgs.py` (7).  Not run: the full
`check_ident3d.py` configuration set (several thousand runs, four workers); the proof-s1 sample spans the same code paths.

## Configurations and runs (build brief 8.3), after the fingerprint dedupe

`python batch3e.py --part CROSS --list` (the first nine lines; a fingerprint belongs to the first part in the order AA, AA2, DECIDE, CROSS, MAIN,
AMEND, ROBUST, SMOOTH that asks for it).  This is the count to report before any decision part runs:

```text
AA         12 configurations,   12 after the dedupe,   144 runs
AA2        22 configurations,   22 after the dedupe,   264 runs
DECIDE     54 configurations,   53 after the dedupe,   636 runs
CROSS       2 configurations,    2 after the dedupe,    24 runs
MAIN       59 configurations,   46 after the dedupe,   552 runs
AMEND     561 configurations,  529 after the dedupe,  5880 runs
ROBUST     32 configurations,   32 after the dedupe,   384 runs
SMOOTH    272 configurations,  258 after the dedupe,  3096 runs
total    1014 configurations,  954 after the dedupe, 10980 runs
```

Against registration 13.5 (before the dedupe): MAIN 59, DECIDE 54, CROSS 2, ROBUST 32, SMOOTH 272 and AMEND 507 at twelve seeds plus 54 at three seeds
(561 configurations) match the listed numbers; AMEND matches the per-cell split too (CLw-long 75, RT 55, CL 52, OV 47, K-I2 40, K-L10 40, K-OV 36,
OV-long 34, CLc 30, CL-long 26, the three video cells 21 each, Ec, E-I2 and M-I2 3 each).  AA and AA2 list each cell's A row (S5 at T=inf, hold 0,
repair off) beside its A' row (phase variant 1), so they hold 12 and 22 configurations where the registration counts 6 and 11 A/A rows.  DOUBLE
(seeds 12 to 23 of the fingerprints the reader names) is not in the table.  The registration's unconditional total (989 configurations, 11 382 runs)
excludes CROSS and the extra A rows; after the dedupe the plan above is 10 980 runs including CROSS and the A rows.

