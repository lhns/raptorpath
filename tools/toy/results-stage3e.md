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

## Timing run (build brief 9 step 6)

One process, nothing else of mine running (the machine carried two long batches): today's Bulk (never gives up, strict order, edge none) with repair RL
in CL-long, seed 0, `toy2.run(batch3e.bulk_cfg("CL-long", 0, 50.0, rp("RL")))`, no trace.

| wall time | peak working set | dec_comp_max | repairs sent | decoded units | share on time at 0.4 s | recovery |
|---|---|---|---|---|---|---|
| 11.9 s | 110.7 MB (psapi PeakWorkingSetSize) | 14 | 18 825 | 5 928 (rep_used 5 928, redundant 12 228) | 0.4547 | 9.4 s (0.15 s readout: 11.7 s) |

The edge-"none" components after the outage stay small (largest 14 unknowns), far from the 30 minute stop.  r averaged 0.099 (W cap bound in
0.50 of the readings, saturation pressure 0.20); p_dec_meas 0.855.  These numbers come from one run and are not a reading under any registered rule.

## Tests (build brief 9 step 3)

`python -m unittest test_toy test_toy2 test_proof_s1 test_batch3e`: 193 tests, OK.  Before the change the same command without `test_batch3e` ran 116 tests
(test_toy, test_toy2, test_proof_s1); `Stage3e` adds 45 tests and `test_batch3e` 32 (116 + 45 + 32 = 193).
The identity check above was re-run on the final tree (same PASS line).

## Where the build brief was followed with a change, and what it left open

- Test 15, "owd_est rel equals fwd within 1e-9 with no queue": not attainable.  srtt carries the wait of the report for the next 2 ms tick, so `fwd` reads
  up to one tick more than `rel`.  The test asserts `rel` = RTprop/2 to 1e-9 and `fwd` within 2 ms of it.  With rev_q = 0.1 `fwd` reads about 0.1 above
  `rel` (the whole reverse queue enters srtt), not 0.05; the test asserts more than 0.09.
- Test 7, the worth gauge in OV with 3 000 units: the backlog cannot reach T - R in 0.25 s, so T = 0.4 cannot fire it.  The test runs T = 0.03, where o_hat
  passes T once the link queue builds; the assertion is unchanged (age edge > 0, worth edge 0).
- Test 5, the `ws` case: A* is clamped to W, so `ws` binds only with A_known = n_sent and W = A*; the constructed state says so.
- RT's `W_floor_bind_frac` is 0.998 and not 1.0: the readings before the first RTT sample see the 333 ms seed.  The test asserts above 0.9.
- The decoder's matching is the brief's recursion written with an explicit stack in the same visiting order, and the cascade is a work list in the recursion's order
  (a differential test against the recursive form on random systems, and a 1 500-deep chain that exceeds Python's recursion limit).
- A repair is written to the sender trace as `(t, lo, j, end, 3)` when tracing, so that the identity gates (T = inf and H = inf against today's Bulk, delta 0.005 against OFF,
  R0 against S5) see it.  Old runs have no such entries.
- Today's Bulk, ADR-0084 and today's Realtime arms run with `rtt_est = "rfc"` in every cell (the link cells had it already).  Without it the call cells would give the
  Bulk arm the old estimator, and the T = inf identity could not hold under repair, because srtt feeds r.
- Reading of 13.3: the stage verdict PASSES when no cell FAILS and one comparison reads shown better, so an UNDETERMINED cell does not block it.
- A/A: A = S5 at T = inf, hold 0, repair off in the cell, A' the same with phase variant 1 (every cell's sweep includes that point); the AA and AA2 parts hold both rows.
- `PASS-r0` is written by `check_ident3e.py` from its 23-configuration sample.  The registration's full Part ID (R0 against S5 in the 21 stage 3d cells at 3 seeds)
  is not run; the runner checks R0 against S5 id3e and trace at the same seed in MAIN instead.
- Not built, because the build brief codes none of them: the `rmax_bind` and `burst_bind` gauges, the per-1 s series of r, W, A* and decodes, the `kp_stats` read of
  SMOOTH, and the reader's per-part driver that runs DECIDE-then-DOUBLE automatically.  The constant 0.0875 of the brief's fixed-r list has no arm in registration 5.
- The `--workers 2` pool path of `batch3e.py` is not exercised by a test or a run (the tests use one worker).
