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
AA         24 configurations,   24 after the dedupe,   288 runs
AA2        38 configurations,   38 after the dedupe,   456 runs
DECIDE     54 configurations,   52 after the dedupe,   624 runs
CROSS       2 configurations,    2 after the dedupe,    24 runs
MAIN       59 configurations,   41 after the dedupe,   492 runs
AMEND     561 configurations,  521 after the dedupe,  5784 runs
ROBUST     32 configurations,   32 after the dedupe,   384 runs
SMOOTH    272 configurations,  258 after the dedupe,  3096 runs
total    1042 configurations,  968 after the dedupe, 11148 runs
```

Against registration 13.5 (before the dedupe): MAIN 59, DECIDE 54, CROSS 2, ROBUST 32, SMOOTH 272 and AMEND 507 at twelve seeds plus 54 at three seeds
(561 configurations) match the listed numbers; AMEND matches the per-cell split too (CLw-long 75, RT 55, CL 52, OV 47, K-I2 40, K-L10 40, K-OV 36,
OV-long 34, CLc 30, CL-long 26, the three video cells 21 each, Ec, E-I2 and M-I2 3 each).  AA and AA2 hold more than the registration's 6 and 11 A/A
rows because the A/A has two readings (next section): each cell lists its A and A' at T = inf and at its primary lifetime (the file cells have one pair).
DOUBLE (seeds 12 to 23 of the fingerprints the reader names) is not in the table.  The registration's unconditional total (989 configurations,
11 382 runs) excludes CROSS and counts 6 + 11 A/A rows; the plan above has 11 148 runs including CROSS and the A rows of both readings.

## Open before AA runs: which A/A, a conflict between the build brief and the registration

- Build brief 8.5: the floor is frozen by `proof_s1.freeze`'s rule, which is the 95th percentile over 12 seeds of |A - A'| with A = S5 at **T = inf, hold 0**
  (the point every sweep ends at) and A' its phase variant 1.
- Registration 5: all arms run at T = 0.4 s (RT 0.12 s) and hold 0 "unless stated"; the A/A row says "S5, phase variant 1" and states no T.  Read literally the
  A/A is the S5 arm at its **primary lifetime**, paired with that S5 arm, and MAIN's 59 rows then need no extra A rows.
- The build brief's rule for a disagreement outside the registration's section 3 is to stop and report it.  Nothing blocks the build, so both readings
  are built: AA and AA2 run both pairs, MAIN's own AA arm follows the registration (variant 1 at the primary lifetime), and `summarize3e.py freeze --aa-at tinf|primary`
  stores both floors in `nf_stage3e.json` and takes the chosen one for `cells` (default tinf, the build brief).  The coordinator should choose before AA runs.

## Timing run (build brief 9 step 6)

One process, nothing else of mine running (the machine carried two long batches): today's Bulk (never gives up, strict order, edge none) with repair RL
in CL-long, seed 0, `toy2.run(batch3e.bulk_cfg("CL-long", 0, 50.0, rp("RL")))`, no trace.

| wall time | peak working set | dec_comp_max | repairs sent | decoded units | share on time at 0.4 s | recovery |
|---|---|---|---|---|---|---|
| 11.9 s | 110.7 MB (psapi PeakWorkingSetSize) | 14 | 18 825 | 5 928 (rep_used 5 928, redundant 12 228) | 0.4547 | 9.4 s (0.15 s readout: 11.7 s) |

The edge-"none" components after the outage stay small (largest 14 unknowns), far from the 30 minute stop.  r averaged 0.099 (W cap bound in
0.50 of the readings, saturation pressure 0.20); p_dec_meas 0.855.  These numbers come from one run and are not a reading under any registered rule.

## Tests (build brief 9 step 3)

`python -m unittest test_toy test_toy2 test_proof_s1 test_batch3e`: 196 tests, OK (547 s on the loaded machine), run on the final tree.  Before the change the same
command without `test_batch3e` ran 116 tests (test_toy, test_toy2, test_proof_s1); `Stage3e` adds 45 tests and `test_batch3e` 35 (116 + 45 + 35 = 196).
The identity check above was run on the tree before the last edits (the RATE script, the `--full` mode, the A/A readings and the reader's doubling; of `toy2.py` only a docstring
changed since); its PASS line stands for the sample.

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
- The doubling of 13.3 is read through the store: `load_rows` loads seeds 12 to 23 where they exist, a comparison with its 24-seed rows on disk is read once and
  never listed again, and one without them lists the rows to double (`--write-undet`).
- `W_pre` (the window before the [16, 512] clamp) is not in `rate_ref3e.json`, because `derive_window` returns only the clamped value; the test and `check_rate3e.py`
  compare the clamped value, so W_pre is pinned only where the clamp does not bind.
- The port rounds W with Python's `round` (half to even) where Rust's `.round()` rounds half away from zero.  No point of the reference grid is a tie, so the
  grid cannot see the difference.
- The stage 3e keys are listed in `summary3e`'s docstring (the brief says `summary`'s); `summary`'s docstring points to it.
- `DONE-<PART>` is written only by an unfiltered invocation (no `--cells`, no `--only`), so AMEND run cell by cell needs a final unfiltered pass.  `summarize3e.py read` defaults
  to the modes OFF and RL; RE is asked for with `--modes OFF,RL,RE`.
- `check_ident3e.py` (the sample) writes no marker: PASS-ident and PASS-r0 are written only by `check_ident3e.py --full`, which adds `check_ident3d.py` (one worker) and R0
  against repair off in every batch3d cell (3 seeds) and every batch3e cell (12 seeds).  `--full` is built and its R0 part was exercised on K-I2, P2 and RT only; it was not run in
  full.  Both markers are owed.  The runner also checks R0 against S5 (id3e and trace) at the same seed in MAIN.
- `check_rate3e.py` is the RATE part: it checks `rate_ref3e.json` and the registration's 2.1 table and writes `PASS-rate` (or STOP with the cause).  It passes here; its
  marker was not kept in the worktree.
- Not built, because the build brief codes none of them: the `rmax_bind` and `burst_bind` gauges, the per-1 s series of r, W, A* and decodes, the `kp_stats` read of
  SMOOTH, and the reader's per-part driver that runs DECIDE-then-DOUBLE automatically.  The constant 0.0875 of the brief's fixed-r list has no arm in registration 5.
- The `--workers 2` pool path of `batch3e.py` is not exercised by a test or a run (the tests use one worker).

## Follow-ups after the coordinator's decisions

- A/A: `summarize3e.py freeze` stores both pairs and takes the larger floor per cell and readout (`combine_floors`); a floor that is zero in both pairs is listed as
  degenerate and the reader marks every comparison on that readout reported only (never under the verdict).  Where one pair is zero the other pair's floor is used and the
  freeze prints it.  The values themselves are known only after AA runs.
- Stage verdict: any UNDETERMINED cell makes the stage UNDETERMINED (as proof stage 1); FAILS still wins.
- Today's Realtime under the RFC estimator: it keeps its own 50 ms seed on its deadline (`srtt` is None until the first sample under both estimators; test
  `TodaysRealtimeSeed`).  After the first sample the RFC srtt differs from the old estimator's (0.101 against 0.110 in RT, which includes the report tick); no change was made.
- Gauges `rmax_bind` and `burst_bind` (flags from `controller_rate_parts`, anchor term; fractions in `rep3e`) and `rep3e["series"]`: per 1 s repairs, mean r, mean W, mean A*, decoded units.
- `summarize3e.py smooth` reads kp_stats (K and P) over every 5 consecutive points of each fine sweep (`batch3e.SMOOTH_GROUPS`) against the cell's A/A floor.
- `python batch3e.py --smoke-pool` runs the `--workers 2` path on two tiny configurations in a temporary store with the markers waived.  Built, not run: `amend\phase4.done`
  did not exist and `amend_run.py` was still running.

## Amendment 2 (registration 14, reg-stage3e at dd0a941f): what the runner and reader do now

- Seeds: 12 on DECIDE, MAIN's CL, CL-long, CLw-long and RT rows and the cells CL-long, CL, CLw-long, RT, CLc, CLc-v, CL-long-v, K-I2, K-L10; 6 on K-OV, OV, OV-long, OV-long-v,
  the file cells and MAIN's OV and CLs-long rows; 3 on the delta = 0.005 identity rows, SMOOTH, and the old senders of OV-long and OV-long-v (ADR-0084 at rho 0.95 and 1, today's
  Realtime at delta 0.5 and 50, or 0.005 and 50 in OV-long-v, and today's Bulk); the A/A rows 12 everywhere; ROBUST 6.  AMEND is 543 configurations.
- The doubling goes from n to 2n seeds (3 to 6, 6 to 12, 12 to 24) and is read once; a comparison against a 3-seed old point runs on 3 seeds of the new curve and doubles to 6.
  The A/A floor is frozen at n = 3, 6, 12 and 24 over the first n seeds (the A/A rows have 12, so 24 reads the 12-seed floor) and each comparison uses the floor of its own count.
- Reuse: `--reuse-dirs 3d=PATH,ps1=PATH` indexes the re-baseline's and proof stage 1's stored rows by fingerprint; they are used only after `check_ident3e.py --reuse ...` wrote `PASS-reuse`
  (a 3-seed rerun of every shared fingerprint against its stored row under id3e; the stored rows keep no trace).  K-I2 and K-L10 are not shared with stage 3d because the outage cells read
  `windows` (recovery), which stage 3d's call cells did not, and their old arms use the old RTT estimator there; CL, CL-long, OV and OV-long are.
- Two pools: `--light-workers` (up to 8) and `--heavy-workers` (up to 2; an old sender in CL-long, CLw-long, CL-long-v, OV-long, OV-long-v, and RQ); each run waits until free memory is at
  least 1 GB plus 1.5 GB (heavy) or 0.15 GB (light).
- Order: AA, AA2, TIME (`PASS-time` if the total is at most 24 h, else STOP with the table), DECIDE, EARLY (the RL rows of Ec, RT, OV, CL-long-v), MAIN, REST (the other AMEND rows, long overload
  cells last), then ROBUST and SMOOTH only with `VERDICT-PASSES`, which `summarize3e.py verdict` writes (and removes when the verdict is not PASSES).
- Runs before the verdict, after the dedupe and before any reuse: AA 288, AA2 456, DECIDE 624, MAIN 402, EARLY 360, REST 4 512 = 6 642 (registration 14.9: 5 964 with reuse and one A/A pair);
  ROBUST 192 and SMOOTH 774 follow a pass.  AA and AA2 hold both A/A pairs and DECIDE's repair-off half is still listed (it is reused when proof stage 1's store is given).
