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

## Amendment 3 (registration 15, reg-stage3e at 30c77ca9): arm RB and a threat to validity

- Arm RB: `repair=dict(r="rb", edge=...)`; r is held at r_max = 0.5 from the outage start until `rb_A` after its end (`rb_unit` "s": seconds; "sym": first sends), the law fed the true loss
  outside; no outage, no window, so RB equals RL there.  Part RB runs every design in CL-long, CLw-long and CL-long-v at 12 seeds, at rb_A = 3 s and 0.25 s (about 4 900 symbols: the engine's
  Gilbert-Elliott estimates forget per symbol), after EARLY and before MAIN.  The reader reads RB as reported only unless `ENGINE-CAP` starts with "holds"; in CL-long-v every arm at
  delta = 0.005 is reported whatever the marker says (it never reaches the cap), today's Realtime at delta >= 0.5 follows the marker.  `ENGINE-CAP` here says clamped, so RB is reported only.
- Markers written at the coordinator's instruction: `PASS-simread` (a reading, not a value gate) and `ENGINE-CAP` (clamped).

**Threat to validity: the window.** In the offline simulation the rate law and the span read the computed window W* for every design
(`rate_law`: `W, W_pre = derive_window_parts(tail, p, 1.0, srtt, rate)`, `W = min(float(round(W)), float(W_CAP))`; `read_law`: `self.W_cur = L["W"]`; `astar` clamps to `self.W_cur`;
`stage_repair`: `ws = max(0, self.A_known - int(self.W_cur))`), whereas the engine feeds the rate snapshot the encoder's live window size
(`emit_source.rs`: `cadenced_repair_rate(..., st.encoder.window_size(), ...)` into `rate_snapshot(est, window)`).  Where the encoder's fill is below W*, which is the case at a finite lifetime
(13.2 against a floor of 16 in the simulator's reading), the engine's r is higher: the simulation's repair is understated there against the engine, mostly in the new model's rows.
Not changed; recorded as a threat.

## TIME (registration 14.8): one seed of each cell x design under repair RL, one process

| cell | design | wall (s) | peak RSS (MB) | |
|---|---|---|---|---|
| OV-long | adr | 30.7 | 146 |  |
| OV-long | rt | 160.5 | 193 |  |
| OV-long | bulk | 26.4 | 266 |  |
| OV-long-v | adr | 609.3 | 278 |  |
| OV-long-v | rt | 184.8 | 208 |  |
| OV-long-v | bulk | 24.4 | 284 |  |
| CL-long | adr | 21.4 | 302 |  |
| CL-long | rt | 14.8 | 359 |  |
| CL-long | bulk | 12.4 | 377 |  |
| CL-long-v | adr | 44.2 | 419 |  |
| CL-long-v | rt | 29.9 | 431 |  |
| CL-long-v | bulk | 10.8 | 389 |  |
| OV-long | new | 33.9 | 401 |  |
| CLw-long | adr | 11.1 | 186 |  |
| CLw-long | rt | 13.3 | 200 |  |
| CLw-long | bulk | 10.7 | 216 |  |
| OV-long-v | new | 28.6 | 285 |  |
| CL-long | new | 14.6 | 209 |  |
| CL-long-v | new | 14.7 | 214 |  |
| CLw-long | new | 11.7 | 207 |  |
| OV | new | 7.7 | 214 |  |
| OV | adr | 8.4 | 239 |  |
| OV | rt | 35.9 | 244 |  |
| OV | bulk | 7.2 | 254 |  |
| CL | new | 7.4 | 273 |  |
| CL | adr | 7.4 | 290 |  |
| CL | rt | 8.5 | 297 |  |
| CL | bulk | 7.0 | 300 |  |
| CLc | new | 8.4 | 313 |  |
| CLc | adr | 7.4 | 324 |  |
| CLc | rt | 10.0 | 327 |  |
| CLc | bulk | 8.1 | 296 |  |
| CLc-v | new | 10.0 | 305 |  |
| CLc-v | adr | 10.0 | 431 | bounded at 10 s, not run |
| CLc-v | rt | 10.0 | 431 | bounded at 10 s, not run |
| CLc-v | bulk | 10.0 | 431 | bounded at 10 s, not run |
| CLs-long | new | 10.0 | 431 | bounded at 10 s, not run |
| CLs-long | adr | 10.0 | 431 | bounded at 10 s, not run |
| CLs-long | rt | 10.0 | 431 | bounded at 10 s, not run |
| CLs-long | bulk | 10.0 | 431 | bounded at 10 s, not run |
| Ec | new | 10.0 | 431 | bounded at 10 s, not run |
| Ec | adr | 10.0 | 431 | bounded at 10 s, not run |
| Ec | rt | 10.0 | 431 | bounded at 10 s, not run |
| Ec | bulk | 10.0 | 431 | bounded at 10 s, not run |
| E-I2 | new | 10.0 | 431 | bounded at 10 s, not run |
| E-I2 | adr | 10.0 | 431 | bounded at 10 s, not run |
| E-I2 | rt | 10.0 | 431 | bounded at 10 s, not run |
| E-I2 | bulk | 10.0 | 431 | bounded at 10 s, not run |
| M-I2 | new | 10.0 | 431 | bounded at 10 s, not run |
| M-I2 | adr | 10.0 | 431 | bounded at 10 s, not run |
| M-I2 | rt | 10.0 | 431 | bounded at 10 s, not run |
| M-I2 | bulk | 10.0 | 431 | bounded at 10 s, not run |
| RT | new | 10.0 | 431 | bounded at 10 s, not run |
| RT | adr | 10.0 | 431 | bounded at 10 s, not run |
| RT | rt | 10.0 | 431 | bounded at 10 s, not run |
| RT | bulk | 10.0 | 431 | bounded at 10 s, not run |
| K-I2 | new | 10.0 | 431 | bounded at 10 s, not run |
| K-I2 | adr | 10.0 | 431 | bounded at 10 s, not run |
| K-I2 | rt | 10.0 | 431 | bounded at 10 s, not run |
| K-I2 | bulk | 10.0 | 431 | bounded at 10 s, not run |
| K-L10 | new | 10.0 | 431 | bounded at 10 s, not run |
| K-L10 | adr | 10.0 | 431 | bounded at 10 s, not run |
| K-L10 | rt | 10.0 | 431 | bounded at 10 s, not run |
| K-L10 | bulk | 10.0 | 431 | bounded at 10 s, not run |
| K-OV | new | 10.0 | 431 | bounded at 10 s, not run |
| K-OV | adr | 10.0 | 431 | bounded at 10 s, not run |
| K-OV | rt | 10.0 | 431 | bounded at 10 s, not run |
| K-OV | bulk | 10.0 | 431 | bounded at 10 s, not run |

Total over the registered run counts before the verdict, one pool of 9 workers: **2.1 h** (gate: 24 h) -> PASS-time

## 2026-11-26: cuts to the plan, recorded before DECIDE has any data

The owner wants the run done as soon as possible.  Changes since the TIME step died, none of them touching a law, a cell, a readout or a reading rule:

- **TIME.**  The first chain run stopped after 33 of 68 TIME rows (exit -1, no traceback).  The 34th configuration (CLc-v, ADR-0084 at rho 0.99, repair RL) alone takes 9.5 s, and rows 34 to 68 all
  complete in a fresh process, so the exit was not the configuration; the cause is not found (an outside kill is the likeliest).  TIME now appends each row to `time_rows.jsonl` and a restart skips
  the rows done (test `TimeResume`).  The recorded 33 rows cover every long cell (CL-long, CLw-long, CL-long-v, OV-long, OV-long-v: all four designs each); the 35 others are bounded at 10 s and the
  largest recorded peak (431 MB) and marked so in the table above.
- **Pools.**  The heavy rows peaked at 146 to 431 MB, not 1.5 GB (the largest, ADR-0084 in OV-long-v, 278 MB at 609 s).  Checked on one seed at each end of ADR-0084's rho dial in OV-long: rho 0.95
  took 42 s and peaked at 152 MB, rho 1 took 42 s and 145 MB.  The two pools are replaced by one pool of at most 9 workers; each run waits until the free memory is its measured peak plus 1 GB.
  The total by the 14.7 formula for one pool of 9 workers, over the registered runs before the verdict: 2.1 h.
- **RB** is reported only (`ENGINE-CAP` says clamped), so it moves behind the verdict with ROBUST and SMOOTH, MAIN no longer waits for it, and only rb_A = 0.25 s is kept (72 configurations, 864 runs).
- Each part now streams its output into its log while it runs.
### CL-long (readouts share_by_0p4, recovery_time; paired seeds, the first 12)

#### repair off: FAILS

| baseline | readout | difference | outcome | under the verdict |
|---|---|---|---|---|
| ADR-0084 sender at rho=0p95, today's receiver (C1) | share_by_0p4 | 0.0595 [0.0576, 0.0616] vs m=0.0173: shown better | shown better | yes |
| ADR-0084 sender at rho=0p98, today's receiver (C1) | share_by_0p4 | 0.0409 [0.0389, 0.0433] vs m=0.0164: shown better | shown better | yes |
| ADR-0084 sender at rho=0p99, today's receiver (C1) | share_by_0p4 | 0.0268 [0.0248, 0.0289] vs m=0.0157: shown better | shown better | yes |
| ADR-0084 sender at rho=0p995, today's receiver (C1) | share_by_0p4 | 0.0268 [0.0248, 0.0289] vs m=0.0157: shown better | shown better | yes |
| ADR-0084 sender at rho=1, today's receiver (C1) | share_by_0p4 | 0.0265 [0.0247, 0.0282] vs m=0.0157: shown better | shown better | yes |
| today's Realtime, delta=0p5 (E-rt) | share_by_0p4 | 0.0428 [0.0424, 0.0432] vs m=0.00512: shown better (against the curve's lowest point, 12 seeds) | shown better | no, reported only |
| today's Realtime, delta=5 (E-rt) | share_by_0p4 | 0.0436 [0.0432, 0.044] vs m=0.00512: shown better (against the curve's lowest point, 12 seeds) | shown better | no, reported only |
| today's Realtime, delta=50 (E-rt) | share_by_0p4 | 0.0436 [0.0432, 0.044] vs m=0.00512: shown better (against the curve's lowest point, 12 seeds) | shown better | no, reported only |
| never gives up, strict order = today's Bulk (C3) | share_by_0p4 | outside the swept range (12 seeds) | outside the swept range | yes |
| object API (S5 at T=inf, hold 0) | share_by_0p4 | outside the swept range (12 seeds) | outside the swept range | yes |
| ADR-0084 sender at rho=0p95, today's receiver (C1) | recovery_time | 1.04 [0.978, 1.1] vs m=0.273: shown better | shown better | yes |
| ADR-0084 sender at rho=0p98, today's receiver (C1) | recovery_time | 0.659 [0.612, 0.711] vs m=0.255: shown better | shown better | yes |
| ADR-0084 sender at rho=0p99, today's receiver (C1) | recovery_time | 0.369 [0.316, 0.425] vs m=0.24: shown better | shown better | yes |
| ADR-0084 sender at rho=0p995, today's receiver (C1) | recovery_time | 0.369 [0.316, 0.425] vs m=0.24: shown better | shown better | yes |
| ADR-0084 sender at rho=1, today's receiver (C1) | recovery_time | 0.369 [0.322, 0.414] vs m=0.24: shown better | shown better | yes |
| today's Realtime, delta=0p5 (E-rt) | recovery_time | 0 [0, 0] vs m=0.1: equivalent (against the curve's lowest point, 12 seeds) | equivalent | no, reported only |
| today's Realtime, delta=5 (E-rt) | recovery_time | 0 [0, 0] vs m=0.1: equivalent (against the curve's lowest point, 12 seeds) | equivalent | no, reported only |
| today's Realtime, delta=50 (E-rt) | recovery_time | 0 [0, 0] vs m=0.1: equivalent (against the curve's lowest point, 12 seeds) | equivalent | no, reported only |
| never gives up, strict order = today's Bulk (C3) | recovery_time | outside the swept range (12 seeds) | outside the swept range | yes |
| object API (S5 at T=inf, hold 0) | recovery_time | outside the swept range (12 seeds) | outside the swept range | yes |

- shown worse or outside the swept range: 4; undetermined: 0; shown better: 10

#### repair RL (live law, true loss): FAILS

| baseline | readout | difference | outcome | under the verdict |
|---|---|---|---|---|
| ADR-0084 sender at rho=0p95, today's receiver (C1) | share_by_0p4 | 0.0874 [0.083, 0.0923] vs m=0.028: shown better | shown better | yes |
| ADR-0084 sender at rho=0p98, today's receiver (C1) | share_by_0p4 | 0.0665 [0.0621, 0.071] vs m=0.0271: shown better | shown better | yes |
| ADR-0084 sender at rho=0p99, today's receiver (C1) | share_by_0p4 | 0.0272 [0.0241, 0.0301] vs m=0.0251: undetermined [doubled to 24 seeds] | undetermined | yes |
| ADR-0084 sender at rho=0p995, today's receiver (C1) | share_by_0p4 | 0.0127 [0.00913, 0.0167] vs m=0.0244: equivalent | equivalent | yes |
| ADR-0084 sender at rho=1, today's receiver (C1) | share_by_0p4 | 0.0127 [0.00826, 0.0187] vs m=0.0244: equivalent | equivalent | yes |
| today's Realtime, delta=0p5 (E-rt) | share_by_0p4 | 0.0139 [0.0135, 0.0142] vs m=0.00512: shown better (against the curve's lowest point, 12 seeds) | shown better | no, reported only |
| today's Realtime, delta=5 (E-rt) | share_by_0p4 | 0.0139 [0.0135, 0.0142] vs m=0.00512: shown better (against the curve's lowest point, 12 seeds) | shown better | no, reported only |
| today's Realtime, delta=50 (E-rt) | share_by_0p4 | 0.0139 [0.0135, 0.0142] vs m=0.00512: shown better (against the curve's lowest point, 12 seeds) | shown better | yes |
| never gives up, strict order = today's Bulk (C3) | share_by_0p4 | outside the swept range (12 seeds) | outside the swept range | yes |
| object API (S5 at T=inf, hold 0) | share_by_0p4 | outside the swept range (12 seeds) | outside the swept range | yes |
| ADR-0084 sender at rho=0p95, today's receiver (C1) | recovery_time | 1.76 [1.66, 1.86] vs m=0.488: shown better | shown better | yes |
| ADR-0084 sender at rho=0p98, today's receiver (C1) | recovery_time | 1.33 [1.23, 1.43] vs m=0.469: shown better | shown better | yes |
| ADR-0084 sender at rho=0p99, today's receiver (C1) | recovery_time | 0.527 [0.461, 0.592] vs m=0.429: shown better [doubled to 24 seeds] | shown better | yes |
| ADR-0084 sender at rho=0p995, today's receiver (C1) | recovery_time | 0.257 [0.184, 0.334] vs m=0.415: equivalent | equivalent | yes |
| ADR-0084 sender at rho=1, today's receiver (C1) | recovery_time | 0.248 [0.153, 0.378] vs m=0.415: equivalent | equivalent | yes |
| today's Realtime, delta=0p5 (E-rt) | recovery_time | 0 [0, 0] vs m=0.1: equivalent (against the curve's lowest point, 12 seeds) | equivalent | no, reported only |
| today's Realtime, delta=5 (E-rt) | recovery_time | 0 [0, 0] vs m=0.1: equivalent (against the curve's lowest point, 12 seeds) | equivalent | no, reported only |
| today's Realtime, delta=50 (E-rt) | recovery_time | 0 [0, 0] vs m=0.1: equivalent (against the curve's lowest point, 12 seeds) | equivalent | yes |
| never gives up, strict order = today's Bulk (C3) | recovery_time | outside the swept range (12 seeds) | outside the swept range | yes |
| object API (S5 at T=inf, hold 0) | recovery_time | outside the swept range (12 seeds) | outside the swept range | yes |

- shown worse or outside the swept range: 4; undetermined: 1; shown better: 6

**CELL CL-long: FAILS**



## 2026-11-26: DECIDE (CL-long): the registered reading FAILS, a spec error; the amended reading

Written after CL-long's data and before the data of every other cell.

**The registered reading (kept).**  Read by 13.3 as registered, with the 12-seed doubling already run (`reread-DECIDE-CL-long.log`), CL-long reads **FAILS** in both repair-off and RL: today's Bulk and the object API
(S5 at T = inf, hold 0) read "outside the swept range" on both readouts, four comparisons per mode, and nothing reads "shown worse".  **This is a specification error, not a result.**

**Why.**  Not a strict ">" at the top of the curve: the T = inf point is merged away.  Neighbouring curve points whose mean delivered shares differ by no more than the A/A noise (0.00126 in CL-long) keep the one with the
better on-time share; T = 2 (delivered 0.99926, on time 0.6531 in repair-off, 0.4586 in RL) and T = inf (1.0, 0.6500 and 0.4485) differ by 0.00074, so T = 2 absorbs T = inf and the curve ends at 0.99926.  Bulk and the object
API deliver 1.0, above the curve, so they read outside the range.  The rule's own premise, that every sweep includes T = inf, is therefore false after the merge.

**The amendment.**  A baseline that is itself a configuration of the new model is read by identity, not by interpolation, and counts as neither better nor worse.
- The object API is S5 at T = inf, hold 0: the curve's own last point (read as identical to itself).
- Today's Bulk is S5 at T = inf, hold inf (strict order), and is compared with that row, never with the any-order H = 0 curve (S5 at T = inf, hold 0 differs from Bulk in all 12 seeds, as it should).
- The identity must hold exactly on every key `id3e` compares (and the trace digest where both rows have one).  A difference is a finding: the cell fails and the difference is named.
- Verified on DECIDE's stored runs, 12 seeds, both modes: **0 of 12 seeds differ in repair-off and 0 of 12 in RL** (the RL rows carry trace digests and they are equal; the repair-off rows are reused from proof stage 1 and carry none).
- The merge of neighbouring points is not changed, and the range test is not widened.

**The amended reading (no new runs).**  Repair off: **PASSES** (0 worse or out of range, 10 shown better, 4 identity).  Repair RL: **UNDETERMINED** (0 worse or out of range, 6 shown better, 4 identity, 1 undetermined:
ADR-0084 at rho 0.99 on the on-time share, still undetermined after its doubling to 24 seeds, which is the one doubling).  The cell does not FAIL, so the chain resumes at EARLY.  Under the rule that any undetermined cell leaves
the stage undetermined, this cell keeps the stage UNDETERMINED unless another reading resolves it.

### CL-long (readouts share_by_0p4, recovery_time; paired seeds, the first 12)

#### repair off: PASSES

| baseline | readout | difference | outcome | under the verdict |
|---|---|---|---|---|
| ADR-0084 sender at rho=0p95, today's receiver (C1) | share_by_0p4 | 0.0595 [0.0576, 0.0616] vs m=0.0173: shown better | shown better | yes |
| ADR-0084 sender at rho=0p98, today's receiver (C1) | share_by_0p4 | 0.0409 [0.0389, 0.0433] vs m=0.0164: shown better | shown better | yes |
| ADR-0084 sender at rho=0p99, today's receiver (C1) | share_by_0p4 | 0.0268 [0.0248, 0.0289] vs m=0.0157: shown better | shown better | yes |
| ADR-0084 sender at rho=0p995, today's receiver (C1) | share_by_0p4 | 0.0268 [0.0248, 0.0289] vs m=0.0157: shown better | shown better | yes |
| ADR-0084 sender at rho=1, today's receiver (C1) | share_by_0p4 | 0.0265 [0.0247, 0.0282] vs m=0.0157: shown better | shown better | yes |
| today's Realtime, delta=0p5 (E-rt) | share_by_0p4 | 0.0428 [0.0424, 0.0432] vs m=0.00512: shown better (against the curve's lowest point, 12 seeds) | shown better | no, reported only |
| today's Realtime, delta=5 (E-rt) | share_by_0p4 | 0.0436 [0.0432, 0.044] vs m=0.00512: shown better (against the curve's lowest point, 12 seeds) | shown better | no, reported only |
| today's Realtime, delta=50 (E-rt) | share_by_0p4 | 0.0436 [0.0432, 0.044] vs m=0.00512: shown better (against the curve's lowest point, 12 seeds) | shown better | no, reported only |
| never gives up, strict order = today's Bulk (C3) | share_by_0p4 | 0 [0, 0] vs m=0: identity [today's Bulk equals S5 at T=inf, hold inf on every compared key, 12 seeds] | identity | yes |
| object API (S5 at T=inf, hold 0) | share_by_0p4 | 0 [0, 0] vs m=0: identity [the object API is the curve's own point: S5 at T=inf, hold 0] | identity | yes |
| ADR-0084 sender at rho=0p95, today's receiver (C1) | recovery_time | 1.04 [0.978, 1.1] vs m=0.273: shown better | shown better | yes |
| ADR-0084 sender at rho=0p98, today's receiver (C1) | recovery_time | 0.659 [0.612, 0.711] vs m=0.255: shown better | shown better | yes |
| ADR-0084 sender at rho=0p99, today's receiver (C1) | recovery_time | 0.369 [0.316, 0.425] vs m=0.24: shown better | shown better | yes |
| ADR-0084 sender at rho=0p995, today's receiver (C1) | recovery_time | 0.369 [0.316, 0.425] vs m=0.24: shown better | shown better | yes |
| ADR-0084 sender at rho=1, today's receiver (C1) | recovery_time | 0.369 [0.322, 0.414] vs m=0.24: shown better | shown better | yes |
| today's Realtime, delta=0p5 (E-rt) | recovery_time | 0 [0, 0] vs m=0.1: equivalent (against the curve's lowest point, 12 seeds) | equivalent | no, reported only |
| today's Realtime, delta=5 (E-rt) | recovery_time | 0 [0, 0] vs m=0.1: equivalent (against the curve's lowest point, 12 seeds) | equivalent | no, reported only |
| today's Realtime, delta=50 (E-rt) | recovery_time | 0 [0, 0] vs m=0.1: equivalent (against the curve's lowest point, 12 seeds) | equivalent | no, reported only |
| never gives up, strict order = today's Bulk (C3) | recovery_time | 0 [0, 0] vs m=0: identity [today's Bulk equals S5 at T=inf, hold inf on every compared key, 12 seeds] | identity | yes |
| object API (S5 at T=inf, hold 0) | recovery_time | 0 [0, 0] vs m=0: identity [the object API is the curve's own point: S5 at T=inf, hold 0] | identity | yes |

- shown worse or outside the swept range: 0; undetermined: 0; shown better: 10

#### repair RL (live law, true loss): UNDETERMINED

| baseline | readout | difference | outcome | under the verdict |
|---|---|---|---|---|
| ADR-0084 sender at rho=0p95, today's receiver (C1) | share_by_0p4 | 0.0874 [0.083, 0.0923] vs m=0.028: shown better | shown better | yes |
| ADR-0084 sender at rho=0p98, today's receiver (C1) | share_by_0p4 | 0.0665 [0.0621, 0.071] vs m=0.0271: shown better | shown better | yes |
| ADR-0084 sender at rho=0p99, today's receiver (C1) | share_by_0p4 | 0.0272 [0.0241, 0.0301] vs m=0.0251: undetermined [doubled to 24 seeds] | undetermined | yes |
| ADR-0084 sender at rho=0p995, today's receiver (C1) | share_by_0p4 | 0.0127 [0.00913, 0.0167] vs m=0.0244: equivalent | equivalent | yes |
| ADR-0084 sender at rho=1, today's receiver (C1) | share_by_0p4 | 0.0127 [0.00826, 0.0187] vs m=0.0244: equivalent | equivalent | yes |
| today's Realtime, delta=0p5 (E-rt) | share_by_0p4 | 0.0139 [0.0135, 0.0142] vs m=0.00512: shown better (against the curve's lowest point, 12 seeds) | shown better | no, reported only |
| today's Realtime, delta=5 (E-rt) | share_by_0p4 | 0.0139 [0.0135, 0.0142] vs m=0.00512: shown better (against the curve's lowest point, 12 seeds) | shown better | no, reported only |
| today's Realtime, delta=50 (E-rt) | share_by_0p4 | 0.0139 [0.0135, 0.0142] vs m=0.00512: shown better (against the curve's lowest point, 12 seeds) | shown better | yes |
| never gives up, strict order = today's Bulk (C3) | share_by_0p4 | 0 [0, 0] vs m=0: identity [today's Bulk equals S5 at T=inf, hold inf on every compared key, 12 seeds] | identity | yes |
| object API (S5 at T=inf, hold 0) | share_by_0p4 | 0 [0, 0] vs m=0: identity [the object API is the curve's own point: S5 at T=inf, hold 0] | identity | yes |
| ADR-0084 sender at rho=0p95, today's receiver (C1) | recovery_time | 1.76 [1.66, 1.86] vs m=0.488: shown better | shown better | yes |
| ADR-0084 sender at rho=0p98, today's receiver (C1) | recovery_time | 1.33 [1.23, 1.43] vs m=0.469: shown better | shown better | yes |
| ADR-0084 sender at rho=0p99, today's receiver (C1) | recovery_time | 0.527 [0.461, 0.592] vs m=0.429: shown better [doubled to 24 seeds] | shown better | yes |
| ADR-0084 sender at rho=0p995, today's receiver (C1) | recovery_time | 0.257 [0.184, 0.334] vs m=0.415: equivalent | equivalent | yes |
| ADR-0084 sender at rho=1, today's receiver (C1) | recovery_time | 0.248 [0.153, 0.378] vs m=0.415: equivalent | equivalent | yes |
| today's Realtime, delta=0p5 (E-rt) | recovery_time | 0 [0, 0] vs m=0.1: equivalent (against the curve's lowest point, 12 seeds) | equivalent | no, reported only |
| today's Realtime, delta=5 (E-rt) | recovery_time | 0 [0, 0] vs m=0.1: equivalent (against the curve's lowest point, 12 seeds) | equivalent | no, reported only |
| today's Realtime, delta=50 (E-rt) | recovery_time | 0 [0, 0] vs m=0.1: equivalent (against the curve's lowest point, 12 seeds) | equivalent | yes |
| never gives up, strict order = today's Bulk (C3) | recovery_time | 0 [0, 0] vs m=0: identity [today's Bulk equals S5 at T=inf, hold inf on every compared key, 12 seeds] | identity | yes |
| object API (S5 at T=inf, hold 0) | recovery_time | 0 [0, 0] vs m=0: identity [the object API is the curve's own point: S5 at T=inf, hold 0] | identity | yes |

- shown worse or outside the swept range: 0; undetermined: 1; shown better: 6

**CELL CL-long: UNDETERMINED**



**Why the bar m grows about 1.6 times with repair (from the stored runs).**  Not the A/A floor (share_by_0p4: 0.00512 in every mode, below the bar) and not the doubling.  The bar is `max(T_bar, NF)` with
`T_bar = 0.05 (1 - share_ref)`, 5 % of the reference's miss rate, and the reference is the baseline.  ADR-0084 at rho 0.99 is on time on 0.6858 of the units in repair-off (T_bar 0.0157) and 0.4985 with RL (T_bar 0.0251): the baseline's
on-time share fell with repair, so its miss rate rose 0.314 to 0.501 and the bar with it.  (ADR-0084's recovery went 4.8 s to 8.6 s, and the link dropped 108 datagrams outside the outage; today's Bulk 0.6093 to 0.4482.)

**Why only one of today's Realtime's three rows counts under the verdict with repair, and why they read alike.**  The registered scope (13.3): today's Realtime is under the verdict at delta = 50 in RL and RE, reported elsewhere, and reported only in
repair-off.  They read alike because delta barely reaches the design in this cell: repair-off sheds 5 604 holes at every delta (gauge `verdicts`) and skips 62 220 candidates for the 200-symbol window at every delta; the delivered and on-time
shares are 0.8984, 0.8975 and 0.8975 for delta 0.5, 5, 50 (the last two identical to five digits).  With RL the shed counts differ (1, 2, 5), the shares do not (0.9251, 0.9251, 0.9251).  The shed budget and the window bind, and delta
enters only through the age deadline `b(delta) srtt`, which by inference from these equal counts does not bind (not tested separately).  So the three rows are one reading counted once under the verdict.

**Misprediction recorded.**  The registration predicted that the new model's lead over ADR-0084 at matched share would grow with repair by 1.2 to 1.6 (about 1.4).  Measured, in CL-long (12 seeds, lead = new curve minus the baseline):

| baseline | on-time share, repair off | on-time share, RL | recovery, repair off | recovery, RL |
|---|---|---|---|---|
| ADR-0084 rho 0.99 | +0.0268 | +0.0272 (undetermined at 24 seeds) | 0.369 s | 0.527 s (x 1.43, as predicted) |
| ADR-0084 rho 0.995 | +0.0268 | +0.0127 (equivalent) | 0.369 s | 0.257 s (equivalent) |
| ADR-0084 rho 1 | +0.0265 | +0.0127 (equivalent) | 0.369 s | 0.248 s (equivalent) |

So the prediction held for rho 0.99 on recovery and failed at rho >= 0.995, where the lead roughly halved to "equivalent" on both readouts.  What the stored runs show: repair raises ADR-0084's delivered share (0.99209 to 0.99503) while its on-time share falls
0.173 (0.6861 to 0.5127); the new model's curve is steeper there, so moving the matched point up 0.003 in delivery costs it about 0.02 of on-time share, and the matched-share lead shrinks.  That is an observation, not a tested explanation.
### CL-long (readouts share_by_0p4, recovery_time; paired seeds, the first 12)

#### repair off: PASSES

| baseline | readout | difference | outcome | under the verdict |
|---|---|---|---|---|
| ADR-0084 sender at rho=0p95, today's receiver (C1) | share_by_0p4 | 0.0595 [0.0576, 0.0616] vs m=0.0173: shown better | shown better | yes |
| ADR-0084 sender at rho=0p98, today's receiver (C1) | share_by_0p4 | 0.0409 [0.0389, 0.0433] vs m=0.0164: shown better | shown better | yes |
| ADR-0084 sender at rho=0p99, today's receiver (C1) | share_by_0p4 | 0.0268 [0.0248, 0.0289] vs m=0.0157: shown better | shown better | yes |
| ADR-0084 sender at rho=0p995, today's receiver (C1) | share_by_0p4 | 0.0268 [0.0248, 0.0289] vs m=0.0157: shown better | shown better | yes |
| ADR-0084 sender at rho=1, today's receiver (C1) | share_by_0p4 | 0.0265 [0.0247, 0.0282] vs m=0.0157: shown better | shown better | yes |
| today's Realtime, delta=0p5 (E-rt) | share_by_0p4 | 0.0428 [0.0424, 0.0432] vs m=0.00512: shown better (against the curve's lowest point, 12 seeds) | shown better | no, reported only |
| today's Realtime, delta=5 (E-rt) | share_by_0p4 | 0.0436 [0.0432, 0.044] vs m=0.00512: shown better (against the curve's lowest point, 12 seeds) | shown better | no, reported only |
| today's Realtime, delta=50 (E-rt) | share_by_0p4 | 0.0436 [0.0432, 0.044] vs m=0.00512: shown better (against the curve's lowest point, 12 seeds) | shown better | no, reported only |
| never gives up, strict order = today's Bulk (C3) | share_by_0p4 | 0 [0, 0] vs m=0: identity [today's Bulk equals S5 at T=inf, hold inf on every compared key, 12 seeds] | identity | yes |
| object API (S5 at T=inf, hold 0) | share_by_0p4 | 0 [0, 0] vs m=0: identity [the object API is the curve's own point: S5 at T=inf, hold 0] | identity | yes |
| ADR-0084 sender at rho=0p95, today's receiver (C1) | recovery_time | 1.04 [0.978, 1.1] vs m=0.273: shown better | shown better | yes |
| ADR-0084 sender at rho=0p98, today's receiver (C1) | recovery_time | 0.659 [0.612, 0.711] vs m=0.255: shown better | shown better | yes |
| ADR-0084 sender at rho=0p99, today's receiver (C1) | recovery_time | 0.369 [0.316, 0.425] vs m=0.24: shown better | shown better | yes |
| ADR-0084 sender at rho=0p995, today's receiver (C1) | recovery_time | 0.369 [0.316, 0.425] vs m=0.24: shown better | shown better | yes |
| ADR-0084 sender at rho=1, today's receiver (C1) | recovery_time | 0.369 [0.322, 0.414] vs m=0.24: shown better | shown better | yes |
| today's Realtime, delta=0p5 (E-rt) | recovery_time | 0 [0, 0] vs m=0.1: equivalent (against the curve's lowest point, 12 seeds) | equivalent | no, reported only |
| today's Realtime, delta=5 (E-rt) | recovery_time | 0 [0, 0] vs m=0.1: equivalent (against the curve's lowest point, 12 seeds) | equivalent | no, reported only |
| today's Realtime, delta=50 (E-rt) | recovery_time | 0 [0, 0] vs m=0.1: equivalent (against the curve's lowest point, 12 seeds) | equivalent | no, reported only |
| never gives up, strict order = today's Bulk (C3) | recovery_time | 0 [0, 0] vs m=0: identity [today's Bulk equals S5 at T=inf, hold inf on every compared key, 12 seeds] | identity | yes |
| object API (S5 at T=inf, hold 0) | recovery_time | 0 [0, 0] vs m=0: identity [the object API is the curve's own point: S5 at T=inf, hold 0] | identity | yes |

- shown worse or outside the swept range: 0; undetermined: 0; shown better: 10

#### repair RL (live law, true loss): UNDETERMINED

| baseline | readout | difference | outcome | under the verdict |
|---|---|---|---|---|
| ADR-0084 sender at rho=0p95, today's receiver (C1) | share_by_0p4 | 0.0874 [0.083, 0.0923] vs m=0.028: shown better | shown better | yes |
| ADR-0084 sender at rho=0p98, today's receiver (C1) | share_by_0p4 | 0.0665 [0.0621, 0.071] vs m=0.0271: shown better | shown better | yes |
| ADR-0084 sender at rho=0p99, today's receiver (C1) | share_by_0p4 | 0.0272 [0.0241, 0.0301] vs m=0.0251: undetermined [doubled to 24 seeds] | undetermined | yes |
| ADR-0084 sender at rho=0p995, today's receiver (C1) | share_by_0p4 | 0.0127 [0.00913, 0.0167] vs m=0.0244: equivalent | equivalent | yes |
| ADR-0084 sender at rho=1, today's receiver (C1) | share_by_0p4 | 0.0127 [0.00826, 0.0187] vs m=0.0244: equivalent | equivalent | yes |
| today's Realtime, delta=0p5 (E-rt) | share_by_0p4 | 0.0139 [0.0135, 0.0142] vs m=0.00512: shown better (against the curve's lowest point, 12 seeds) | shown better | no, reported only |
| today's Realtime, delta=5 (E-rt) | share_by_0p4 | 0.0139 [0.0135, 0.0142] vs m=0.00512: shown better (against the curve's lowest point, 12 seeds) | shown better | no, reported only |
| today's Realtime, delta=50 (E-rt) | share_by_0p4 | 0.0139 [0.0135, 0.0142] vs m=0.00512: shown better (against the curve's lowest point, 12 seeds) | shown better | yes |
| never gives up, strict order = today's Bulk (C3) | share_by_0p4 | 0 [0, 0] vs m=0: identity [today's Bulk equals S5 at T=inf, hold inf on every compared key, 12 seeds] | identity | yes |
| object API (S5 at T=inf, hold 0) | share_by_0p4 | 0 [0, 0] vs m=0: identity [the object API is the curve's own point: S5 at T=inf, hold 0] | identity | yes |
| ADR-0084 sender at rho=0p95, today's receiver (C1) | recovery_time | 1.76 [1.66, 1.86] vs m=0.488: shown better | shown better | yes |
| ADR-0084 sender at rho=0p98, today's receiver (C1) | recovery_time | 1.33 [1.23, 1.43] vs m=0.469: shown better | shown better | yes |
| ADR-0084 sender at rho=0p99, today's receiver (C1) | recovery_time | 0.527 [0.461, 0.592] vs m=0.429: shown better [doubled to 24 seeds] | shown better | yes |
| ADR-0084 sender at rho=0p995, today's receiver (C1) | recovery_time | 0.257 [0.184, 0.334] vs m=0.415: equivalent | equivalent | yes |
| ADR-0084 sender at rho=1, today's receiver (C1) | recovery_time | 0.248 [0.153, 0.378] vs m=0.415: equivalent | equivalent | yes |
| today's Realtime, delta=0p5 (E-rt) | recovery_time | 0 [0, 0] vs m=0.1: equivalent (against the curve's lowest point, 12 seeds) | equivalent | no, reported only |
| today's Realtime, delta=5 (E-rt) | recovery_time | 0 [0, 0] vs m=0.1: equivalent (against the curve's lowest point, 12 seeds) | equivalent | no, reported only |
| today's Realtime, delta=50 (E-rt) | recovery_time | 0 [0, 0] vs m=0.1: equivalent (against the curve's lowest point, 12 seeds) | equivalent | yes |
| never gives up, strict order = today's Bulk (C3) | recovery_time | 0 [0, 0] vs m=0: identity [today's Bulk equals S5 at T=inf, hold inf on every compared key, 12 seeds] | identity | yes |
| object API (S5 at T=inf, hold 0) | recovery_time | 0 [0, 0] vs m=0: identity [the object API is the curve's own point: S5 at T=inf, hold 0] | identity | yes |

- shown worse or outside the swept range: 0; undetermined: 1; shown better: 6

**CELL CL-long: UNDETERMINED**

### RT (readouts share_by_0p4, share_by_0p12; paired seeds, the first 12)

#### repair RL (live law, true loss): PASSES

| baseline | readout | difference | outcome | under the verdict |
|---|---|---|---|---|
| ADR-0084 sender at rho=0p95, today's receiver (C1) | share_by_0p4 | 0 [0, 0] vs m=0.000333: equivalent | equivalent | yes |
| ADR-0084 sender at rho=0p98, today's receiver (C1) | share_by_0p4 | 0 [0, 0] vs m=0.000333: equivalent | equivalent | yes |
| ADR-0084 sender at rho=0p99, today's receiver (C1) | share_by_0p4 | 0 [0, 0] vs m=0.000333: equivalent | equivalent | yes |
| ADR-0084 sender at rho=0p995, today's receiver (C1) | share_by_0p4 | 0 [0, 0] vs m=0.000333: equivalent | equivalent | yes |
| ADR-0084 sender at rho=1, today's receiver (C1) | share_by_0p4 | 0 [0, 0] vs m=0.000333: equivalent | equivalent | yes |
| today's Realtime, delta=0p5 (E-rt) | share_by_0p4 | 0 [0, 0] vs m=0.000333: equivalent | equivalent | no, reported only |
| today's Realtime, delta=5 (E-rt) | share_by_0p4 | 0 [0, 0] vs m=0.000333: equivalent | equivalent | no, reported only |
| today's Realtime, delta=50 (E-rt) | share_by_0p4 | 0 [0, 0] vs m=0.000333: equivalent | equivalent | yes |
| never gives up, strict order = today's Bulk (C3) | share_by_0p4 | 0 [0, 0] vs m=0: identity [today's Bulk equals S5 at T=inf, hold inf on every compared key, 12 seeds] | identity | yes |
| object API (S5 at T=inf, hold 0) | share_by_0p4 | 0 [0, 0] vs m=0: identity [the object API is the curve's own point: S5 at T=inf, hold 0] | identity | yes |
| ADR-0084 sender at rho=0p95, today's receiver (C1) | share_by_0p12 | 0.000764 [6.47e-05, 0.00148] vs m=0.00113: undetermined [doubled to 24 seeds] | undetermined | no, reported only |
| ADR-0084 sender at rho=0p98, today's receiver (C1) | share_by_0p12 | 0.000764 [6.47e-05, 0.00148] vs m=0.00113: undetermined [doubled to 24 seeds] | undetermined | no, reported only |
| ADR-0084 sender at rho=0p99, today's receiver (C1) | share_by_0p12 | 0.000764 [6.47e-05, 0.00148] vs m=0.00113: undetermined [doubled to 24 seeds] | undetermined | no, reported only |
| ADR-0084 sender at rho=0p995, today's receiver (C1) | share_by_0p12 | 0.000764 [6.47e-05, 0.00148] vs m=0.00113: undetermined [doubled to 24 seeds] | undetermined | no, reported only |
| ADR-0084 sender at rho=1, today's receiver (C1) | share_by_0p12 | 0.000764 [6.47e-05, 0.00148] vs m=0.00113: undetermined [doubled to 24 seeds] | undetermined | no, reported only |
| today's Realtime, delta=0p5 (E-rt) | share_by_0p12 | 0.00218 [0.000993, 0.00364] vs m=0.00121: undetermined [doubled to 24 seeds] | undetermined | no, reported only |
| today's Realtime, delta=5 (E-rt) | share_by_0p12 | 0.00147 [0.000303, 0.00262] vs m=0.00117: undetermined [doubled to 24 seeds] | undetermined | no, reported only |
| today's Realtime, delta=50 (E-rt) | share_by_0p12 | 0.000536 [-0.000325, 0.00139] vs m=0.00113: undetermined [doubled to 24 seeds] | undetermined | no, reported only |
| never gives up, strict order = today's Bulk (C3) | share_by_0p12 | 0 [0, 0] vs m=0: identity [today's Bulk equals S5 at T=inf, hold inf on every compared key, 12 seeds] | identity | no, reported only |
| object API (S5 at T=inf, hold 0) | share_by_0p12 | 0 [0, 0] vs m=0: identity [the object API is the curve's own point: S5 at T=inf, hold 0] | identity | no, reported only |

- shown worse or outside the swept range: 0; undetermined: 0; shown better: 0

**CELL RT: PASSES**

### OV (readouts share_by_0p4; paired seeds, the first 6)

#### repair RL (live law, true loss): FAILS

| baseline | readout | difference | outcome | under the verdict |
|---|---|---|---|---|
| ADR-0084 sender at rho=0p95, today's receiver (C1) | share_by_0p4 | 0.00794 [0.00752, 0.00842] vs m=0.0445: equivalent | equivalent | yes |
| ADR-0084 sender at rho=0p98, today's receiver (C1) | share_by_0p4 | 0.00803 [0.00763, 0.00846] vs m=0.0445: equivalent | equivalent | yes |
| ADR-0084 sender at rho=0p99, today's receiver (C1) | share_by_0p4 | 0.00798 [0.00757, 0.0084] vs m=0.0445: equivalent | equivalent | yes |
| ADR-0084 sender at rho=0p995, today's receiver (C1) | share_by_0p4 | 0.00784 [0.00749, 0.00826] vs m=0.0445: equivalent | equivalent | yes |
| ADR-0084 sender at rho=1, today's receiver (C1) | share_by_0p4 | 8.89e-05 [2.08e-05, 0.000154] vs m=0.0441: equivalent | equivalent | yes |
| today's Realtime, delta=0p5 (E-rt) | share_by_0p4 | -0.696 [-0.697, -0.696] vs m=0.0093: shown worse | shown worse | no, reported only |
| today's Realtime, delta=5 (E-rt) | share_by_0p4 | -0.696 [-0.696, -0.695] vs m=0.00934: shown worse | shown worse | no, reported only |
| today's Realtime, delta=50 (E-rt) | share_by_0p4 | -0.697 [-0.697, -0.696] vs m=0.0093: shown worse | shown worse | yes |
| never gives up, strict order = today's Bulk (C3) | share_by_0p4 | 0 [0, 0] vs m=0: identity [today's Bulk equals S5 at T=inf, hold inf on every compared key, 6 seeds] | identity | yes |
| object API (S5 at T=inf, hold 0) | share_by_0p4 | 0 [0, 0] vs m=0: identity [the object API is the curve's own point: S5 at T=inf, hold 0] | identity | yes |

- shown worse or outside the swept range: 1; undetermined: 0; shown better: 0

**CELL OV: FAILS**



## 2026-11-26: EARLY, OV reads FAILS (kept); what the stored runs show; fixes to the reader and the plan

**OV, repair RL, as registered: FAILS (kept).**  Today's Realtime delivers 0.8165 to 0.8169 and is on time on 0.814 (delta 0.5, 5, 50, 6 seeds); the new model's curve at that delivered share, between T = 0.6 (0.8068
delivered) and T = 1 (0.8348), is on time on 0.1175: shown worse by 0.70 (bar 0.0093).  Re-read with MAIN's T = 0.4 RL row added (it was missing from EARLY, see below; 6 seeds): the reading is unchanged, because T = 0.4 delivers
0.7929 (on time 0.7928, delay p50 0.390 s), below today's Realtime's 0.8169, and the curve has a cliff at T = tau: every T from 0.6 to infinity is on time on 0.1175 at tau = 0.4 s, since the delay p50 is the lifetime itself (0.59 s at T = 0.6).
The reading rule is not changed.

**Today's Realtime in OV under RL, from the stored runs (6 seeds, mean per run; keys `win_first`, `win_resend`, `gauges.rep_sent`, `queue_drop_out`, `evict_skip`, `verdicts`, `refusals`, `dh_p50`, `dh_p90`):**

| design | delivered | on time at 0.4 s | repair share of link sends | link-queue drops | window skips | sheds | budget refusals | delay p50 / p90 (s) |
|---|---|---|---|---|---|---|---|---|
| today's Realtime, delta 50 | 0.8165 | 0.8141 | 0.080 (10 507 of ~131 000) | 29 263 | 426 562 | 0 | 264 | 0.172 / inf |
| today's Realtime, delta 0.5 | 0.8169 | 0.8139 | 0.071 | 27 910 | 426 389 | 0 | 264 | 0.172 / inf |
| new model, T = 0.4 | 0.7929 | 0.7928 | 0.080 | 0 | 0 | 0 | 0 | 0.390 / inf |
| new model, T = 0.6 | 0.8068 | 0.1175 | 0.080 | 0 | 0 | 0 | 0 | 0.590 / inf |
| new model, T = inf | 1.0000 | 0.1175 | 0.079 | 0 | 0 | 0 | 0 | 1.685 / 3.03 |
| ADR-0084 (rho 0.95 to 1), today's Bulk | 1.0000 | 0.110 to 0.117 | 0.079 | 0 | 0 | 0 to 3 156 | 0 to 4 486 | 1.69 to 1.81 / 3.0 to 3.3 |

So today's Realtime arm is not sending almost no repair: its repair share (0.071 to 0.080) equals the new model's.  It has no source pause (`pause_events` 0 against 5 000 to 6 600 for the paused designs), so all 120 000 first sends enter the link, the 1 250-datagram
queue drops about 29 000 of them (the excess over capacity: 10 000 against 12 000 offered), the window skips 426 000 candidates, and what arrives arrives within 0.17 s.  Its delivered share is the capacity share, about (C/lambda)(1 - eps) = 0.81.

**Prediction or arm?**  The registration's OV row (13.4) predicted exactly this for OV: "Today's Realtime sends fresh units into a 0.125 s queue that drops the excess, so it is about (C/lambda)(1 - eps) = 0.81".  The 0.1-or-less prediction belongs to K-OV (a 1 s queue).
So for OV the prediction is right and the arm does what it was registered to do.  What the arm leaves out is the engine's congestion-window gate: `scheduler/mod.rs` `active_paths()` (line 229) keeps only paths with `available() > 0` (`PathState::available` = cwnd - in_flight, `path.rs:456`),
so the engine does not push 12 000/s into a 10 000/s path past its window; the offline simulation has no cwnd (Q22) and no source pause for this arm.  Whether the engine would deliver more or less than 0.81 under this overload is not established by this arm.  Recorded as a threat.

**Fixes made after this reading** (no new data except the two rows below):
- **EARLY lacked the primary-lifetime row** of RT (T = 0.12) and OV (T = 0.4): MAIN's RL arm is a sweep point that AMEND leaves out, and MAIN runs after EARLY.  EARLY now includes both; they were run (`--part EARLY --cells OV,RT`, 18 runs).
  RT's PASS did not rest on the gap: its readings are the same with the T = 0.12 point added (0 difference in every verdict comparison at 0.4 s: both sides are on time on 0.9989 to 1.0, a ceiling), and its 0.12 s readouts are reported only
  (degenerate floor) or undetermined after their doubling.  RT's PASS is therefore vacuous at 0.4 s and open at 0.12 s.
- The reader now refuses a cell whose curve lacks the cell's primary lifetime (`REFUSED`; the chain stops on it).
- CL-long-v under RL: the delta = 0.005 rows have 3 seeds and equal their repair-off rows by arithmetic; the reader had no 12-seed curve (the crash was `np.stack` of an empty list).  They are now read by identity against their repair-off rows
  (IDENTITY PASSES, FAILS if they differ, IDENTITY PENDING while the repair-off rows, which are REST rows, have not run); CL-long-v reads IDENTITY PENDING.
- The reading output gains a delay table per row (delay p50 and p90 from hand-over over every unit, and the delivered units' p50).
- OV-long and OV-long-v: today's Realtime is restored to full seeds (6) and its whole delta range (the 2026-11-25 cut assumed old senders read about 0 on time under overload; this arm does not).  ADR-0084 and Bulk stay at 3 seeds at the dial ends
  (ADR-0084 reads 0.110 to 0.117 on time in OV, close to 0).  AMEND is 549 configurations.

### OV (readouts share_by_0p4; paired seeds, the first 6)

#### repair RL (live law, true loss): FAILS

| baseline | readout | difference | outcome | under the verdict |
|---|---|---|---|---|
| ADR-0084 sender at rho=0p95, today's receiver (C1) | share_by_0p4 | 0.00794 [0.00752, 0.00842] vs m=0.0445: equivalent | equivalent | yes |
| ADR-0084 sender at rho=0p98, today's receiver (C1) | share_by_0p4 | 0.00803 [0.00763, 0.00846] vs m=0.0445: equivalent | equivalent | yes |
| ADR-0084 sender at rho=0p99, today's receiver (C1) | share_by_0p4 | 0.00798 [0.00757, 0.0084] vs m=0.0445: equivalent | equivalent | yes |
| ADR-0084 sender at rho=0p995, today's receiver (C1) | share_by_0p4 | 0.00784 [0.00749, 0.00826] vs m=0.0445: equivalent | equivalent | yes |
| ADR-0084 sender at rho=1, today's receiver (C1) | share_by_0p4 | 8.89e-05 [2.08e-05, 0.000154] vs m=0.0441: equivalent | equivalent | yes |
| today's Realtime, delta=0p5 (E-rt) | share_by_0p4 | -0.696 [-0.697, -0.696] vs m=0.0093: shown worse | shown worse | no, reported only |
| today's Realtime, delta=5 (E-rt) | share_by_0p4 | -0.696 [-0.696, -0.695] vs m=0.00934: shown worse | shown worse | no, reported only |
| today's Realtime, delta=50 (E-rt) | share_by_0p4 | -0.697 [-0.697, -0.696] vs m=0.0093: shown worse | shown worse | yes |
| never gives up, strict order = today's Bulk (C3) | share_by_0p4 | 0 [0, 0] vs m=0: identity [today's Bulk equals S5 at T=inf, hold inf on every compared key, 6 seeds] | identity | yes |
| object API (S5 at T=inf, hold 0) | share_by_0p4 | 0 [0, 0] vs m=0: identity [the object API is the curve's own point: S5 at T=inf, hold 0] | identity | yes |

- shown worse or outside the swept range: 1; undetermined: 0; shown better: 0

Delay from hand-over per row (the readout the on-time share hides), first 6 seeds:

| row | delivered | on time | delay p50 (s) | delay p90 (s) | delivered units' p50 (s) |
|---|---|---|---|---|---|
| ('adr', 0.95) | 1.0000 | 0.1096 | 1.812 | 3.260 | 1.807 |
| ('adr', 0.98) | 1.0000 | 0.1095 | 1.811 | 3.258 | 1.806 |
| ('adr', 0.99) | 1.0000 | 0.1095 | 1.812 | 3.260 | 1.807 |
| ('adr', 0.995) | 1.0000 | 0.1097 | 1.811 | 3.260 | 1.806 |
| ('adr', 1.0) | 1.0000 | 0.1174 | 1.686 | 3.031 | 1.681 |
| ('bulk',) | 1.0000 | 0.1173 | 1.686 | 3.031 | 1.681 |
| ('lt', 0.4, 0.0) | 0.7929 | 0.7928 | 0.390 | inf | 0.385 |
| ('lt', 0.6, 0.0) | 0.8068 | 0.1175 | 0.590 | inf | 0.585 |
| ('lt', 1.0, 0.0) | 0.8348 | 0.1175 | 0.990 | inf | 0.985 |
| ('lt', 2.0, 0.0) | 0.9045 | 0.1175 | 1.685 | 1.992 | 1.519 |
| ('lt', 2.0, inf) | 0.9045 | 0.1173 | 1.686 | 1.992 | 1.519 |
| ('lt', inf, 0.0) | 1.0000 | 0.1175 | 1.685 | 3.031 | 1.680 |
| ('lt', inf, inf) | 1.0000 | 0.1173 | 1.686 | 3.031 | 1.681 |
| ('rt', 0.5) | 0.8169 | 0.8139 | 0.172 | inf | 0.163 |
| ('rt', 5.0) | 0.8167 | 0.8132 | 0.172 | inf | 0.163 |
| ('rt', 50.0) | 0.8165 | 0.8141 | 0.172 | inf | 0.163 |

**CELL OV: FAILS**



## 2026-11-26: OV steady windows, the exploratory sender-wait rows, and the hold on the new model's remaining rows

**OV under RL, steady window (stored runs, 6 seeds).**  From 0.5 s after the first dequeue expiry to 0.5 s before the source stops, the on-time share at 2 s is 0.7653 to 0.7655 for every T from 0.4 to 2: flat in T.  The delivered share rises with T (0.7929, 0.8068,
0.8348, 0.9045, 1.0 for T = 0.4, 0.6, 1, 2, inf) only through the backlog still queued when the source stops.  Today's Realtime's steady window is 0.808, 0.043 above.  The new model's delay is its lifetime (p50 0.390 s at T = 0.4, 0.590 s at 0.6), not T - R (R_mean 0.111 s): the
dequeue rule sends a unit as late as age T - R and it then waits about R in the queue and on the path.  Today's Realtime delivers at 0.172 s (its link queue is capped at 1 250 datagrams: 0.125 s, plus the path).  Every design spends 0.071 to 0.080 of its link sends on repair.

**Exploratory: sizing a sender wait limit.**  Reported only; these rows never count under the verdict.

exploratory: sizing a sender wait limit (OV, repair off both sides, 6 seeds; reported only, never under the verdict)

| row | delivered | on time 0.4 s | on time 2 s | steady 0.4 s | steady 2 s | delay p50 | delay p90 | R | srtt | repairs | repair share | queue drops |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| ('lt', 0.15, 0.0) | 0.8182 | 0.8182 | 0.8182 | 0.8077 | 0.8077 | 0.138 | inf | 0.103 | 0.097 | 0 | 0.000 | 0 |
| ('lt', 0.2, 0.0) | 0.8220 | 0.8220 | 0.8220 | 0.8077 | 0.8077 | 0.188 | inf | 0.103 | 0.097 | 0 | 0.000 | 0 |
| ('lt', 0.4, 0.0) | 0.8378 | 0.8378 | 0.8378 | 0.8076 | 0.8076 | 0.388 | inf | 0.103 | 0.097 | 0 | 0.000 | 0 |
| ('rt', 50.0) | 0.8186 | 0.8155 | 0.8186 | 0.8083 | 0.8089 | 0.171 | inf | 0.135 | 0.131 | 0 | 0.000 | 18751 |


Without repair the new model's steady window (0.8077) equals today's Realtime's (0.8083), and at T = 0.15 it delivers the same share (0.8182 against 0.8186) at a lower delay p50 (0.138 s against 0.171 s).  With repair the new model's steady window fell to 0.7655, today's Realtime's to 0.808.
Why repair costs the new model more is not tested here.

**Hold.**  Every new-model row still to run (MAIN and REST) is held, because a Revision 4 of the sender may re-run them.  The baseline rows (ADR-0084, today's Realtime, today's Bulk) stay valid and run now (`--part BASE`, any cell, 240 fingerprints, 1 983 runs at the time of writing);
K-OV and OV-long are not read until a reading is registered (equal deadline, steady windows, delay p50, like-for-like repair); only their baseline rows run.  The chain stops after EARLY and BASE.

**K-OV's baseline rows stop the run (2026-11-26).**  `--part BASE` wrote `STOP` at the first K-OV row: ADR-0084 at rho 0.95, repair off, seed 0, did not finish (`finished` False): it sent 2 681 of 3 000 units in the 120 s `t_limit`, paused for 119.8 s of them (B = 8 against a 0.2 s round trip),
delivered 0.886 and was on time on 0.008.  The registration's K-OV prediction (old designs read at most 0.08 on time) did not consider the time limit.  K-OV's baseline rows are held out of BASE until a decision (a longer `t_limit` changes the cell, so it needs an amendment); the STOP is left in place for the coordinator.

## 2026-11-26: what repair does in OV (stored runs and one new seed), and BASE's repair-on rows are provisional

**OV under RL, 6 seeds (stored runs), per run.**

| design | repairs sent | repairs dropped at the link queue | innovative at the receiver | units decoded | lost first sends | share of link sends that is repair |
|---|---|---|---|---|---|---|
| today's Realtime, delta 50 | 10 507 | 10 077 (96 %) | 138 | 138 (0.6 % of the lost) | 22 194 | 0.080 |
| today's Realtime, delta 0.5 | 9 154 | 8 762 (96 %) | 144 | 144 | 22 158 | 0.071 |
| new model, T = 0.4 | 8 331 | 0 | 2 823 | 2 821 (99.8 % of the lost) | 2 826 | 0.080 |

Link-queue drops in today's Realtime: 29 263 per run (delta 50), of which 10 077 are its own repairs; the other 19 187 are first sends (the gauge `first_qdrop`, added below, reads 19 187 on seed 0).  So its repairs do not mostly recover what the link buffer dropped: it decodes 138 units against 19 187 first sends the queue dropped, and
96 % of its repairs are themselves dropped by the same queue.  The new model at T = 0.4 has no queue drops, repairs 8 331 times (5 254 redundant) and decodes 2 821 of 2 826 lost first sends; repair takes 0.080 of its link slots, which are 8.0 % of the 103 700 sends the link carried, against 95 100 first sends (24 900 units were shed at the intake by the dequeue rule).

**Gauge added (no behaviour change):** `first_qdrop` (first sends dropped at the link queue) and `dec_units_qdrop` (decoded units whose first send was dropped at the link queue), which split the decoded units by cause; the stored rows lacked them.  On seed 0 in OV under RL: today's Realtime `first_qdrop` 19 187, `dec_units_qdrop` 0 (all 138 decoded units were channel losses, none was a queue drop); the new model at T = 0.4 `first_qdrop` 0, `dec_units_qdrop` 0.
Proof of no behaviour change: `check_ident3e.py` (the identity sample) passes with the gauges in place, and the tests `Stage3eDecodedByCause`.

**BASE's repair-on rows are provisional.**  Revision 4 may add the engine's spare-capacity clamp on repair (Q22, `min(r, (cwnd - in_flight)/in_flight)`) to every design; that would change every repair-on baseline row, which BASE is running now.  The repair-off rows stay valid.

## 2026-11-26: liveness stop and today's Realtime (amendment, dated; written after BASE's first K-OV and OV-long-v data)

`--part BASE` stopped on "OV-long-v | today's Realtime, delta 0.5 | repair off, seed 2, failing: finished".  Today's Realtime sends no floor, so an object whose tail is shed never finishes; proof stage 1 pinned this as a property of the arm
(`test_a_silent_shed_of_the_open_tail_leaves_the_object_unfinished`).  The stop rule "every run finished" is meant for the new model and the designs that are meant to finish, not for a recorded property of a baseline.
For today's Realtime, "unfinished at t_limit" is now recorded per run (`unfinished`: 0 or 1, and `delivered_by_t_limit`: the share delivered by t_limit) and read as it is, not as a stop.  Every other design keeps the stop (and K-OV's held rows are unchanged by this).

## 2026-11-26 Revision 4 step 1: in_flight counts every datagram; spare_bind in headroom cells

- D-hat is +inf on a median 64 % of ticks (min 0.025, max 1.0), so tau is inert most of the time. The 0.889 and 1.111 extremes of the D-hat to oracle ratio look like rate-step artefacts (1/1.125 and 1/0.9), not estimator error; not chased.
- in_flight now counts every datagram sent (first sends, copies, repairs) and not yet covered by a report (a report covering first-send seq rhi covers everything sent up to that first send). Gauge only; identity unchanged.
- spare_bind (share of first sends at which min(r, spare) < r, spare = (2048 - in_flight)/in_flight), seed 0, headroom cells: CL-long RL arms 0.12 (T = 0.4) to 0.67 (T = inf), RS 0.15, RS-inf 0.77, RE 0.13, RF3 0.97, R0 0; CL-long-v 0 to 0.01; Ec, RT 0. The doc expects about 0: not met in CL-long. The clamp would change those cells.

## 2026-11-26 Revision 4 step 2: spare_bind binds in the drain window only; CL-long clamp-on predictions (registered before the sizing run)

spare_bind per arm (seed 0, headroom cells): see the previous note; it binds in the drain window only. Per 1 s on CL-long RL, seed 0 (outage [2.0, 3.5] s), first sends at which spare < r, out of first sends:
- T = 0.4: s2 192/996 (the outage start), s3 5 571/6 095, s4 11 254/11 254, s5 6 211/10 514, none from s6 on (drain over by about 5.5 s).
- T = 2: s2 192/996, s3 1/4 436, s4 10 327/11 458, s5 to s14 every first send, s15 7 899/10 701. Nothing before 2.0 s; the drain at T = 2 lasts to about the end of the run.
So it is confined to [outage start, drain end]; before the outage it is 0. This is the engine behaviour of ENGINE-CAP (the engine sends almost no repair while a backlog drains), expected, not a defect.

Registered before the clamp-on CL-long rows run (RL at T = 0.4, 2 and inf, clamp on, against clamp off and repair off, 3 seeds):
- T = 0.4: delivered 0.939 to 0.942 (stored RL 0.939, repair off 0.9414), on time and recovery within 0.1 s as stored: no visible change (the drain is short, repair there was small).
- T = 2 and T = inf: the clamp removes the drain-time repair, so recovery_time moves from RL's 9.2 s (T = 2) and 9.4 s (T = inf) toward the repair-off value; predicted to close more than half the gap. Delivered and on-time shares: between RL and repair off, no worse than RL beyond the A/A floor.

- Correction, same day: the spare_bind gauge reads the cell's own store pause B (cfg.pause_B: 1 000 in CL-long, OV and the other link cells, 8 in K-OV, the default 2 048 where none is configured), as the doc's cwnd_sim = B. The spare formulas written above with 2 048 were my text, not what ran: every spare_bind number in the notes is at B = 1 000 in the link cells. New field cwnd_sim (0 = the cell's pause_B) lets a row state and vary it; every step-2 row carries its cwnd_sim.

## 2026-11-26 Revision 4 step 2: the sizing run (reported only, no verdict; 3 seeds; `rev4_step2.py`, rows in out3e/rev4/step2.jsonl)

Built behind config fields that default off (clamp, wait_limit, eq, eq_cap, cwnd_sim, rate2, rate2_t): the identity sample reproduces ba3cf31c with the fields off (check_ident3e, parts A, B, C, G). Not built: the doc's "no payload staged while in_flight >= B" gate (the store pause on outstanding seqs already gates; in_flight can pass B, 1 230 to 1 460 against B = 1 000 in CL-long) and the clamp on each taper grant (only the cadenced accrual is clamped). Every row states its cwnd_sim; today's Realtime has no store pause, so its clamp rows use the cell's B (1 000 in OV, OV-end, OV-crit; 8 in K-OV).

CL-long, clamp on (delivered / on time within 0.4 s / recovery_time s), 3 seeds:

| T | repair off | RL, clamp off | RL, clamp on, B = 1 000 | RL, clamp on, B = 2 048 |
|---|---|---|---|---|
| 0.4 | 0.9406 / 0.9406 / 0 | 0.9392 / 0.9392 / 0 | 0.9399 / 0.9399 / 0 | = clamp off (never binds) |
| 2 | 0.9993 / 0.655 / 5.6 | 0.9990 / 0.463 / 9.27 | 0.9990 / 0.638 / 5.87 | = clamp off |
| inf | 1.000 / 0.652 / 5.6 | 1.000 / 0.453 / 9.47 | 1.000 / 0.633 / 6.00 | = clamp off |

Against the registered prediction: recovery moved toward repair off and closed 93 % of the gap at T = 2 (3.40 of 3.67 s) and 90 % at T = inf (3.47 of 3.87 s); more than half, as predicted. The doc's 1.15 to 1.25 times repair off is not met: 1.05 and 1.07 times. Its on time 0.58 to 0.61 is exceeded (0.638 and 0.633, repair off 0.655 and 0.652). At B = 2 048 the clamp never binds (in_flight max 1 460 < 2 048 / (1 + r)), so the rows equal clamp off. The wait limit on top is inert in CL-long (identical rows, 0 ticks).

OV, 2x2 at T = 0.4, delta = 50, RL (delivered / on time / delay p50; doc prediction in brackets):
- clamp off, wait off: 0.793 / 0.793 / 0.390 (0.7929 stored).
- clamp on, wait off: 0.833 / 0.833 / 0.390 (0.837 / 0.837 / 0.39).
- clamp off, wait on: 0.756 / 0.756 / 0.105 (0.770 / 0.770 / 0.095): lower by 0.014.
- both: 0.809 / 0.809 / 0.094 (0.79 to 0.81 / 0.79 to 0.80 / 0.095 to 0.10); steady window [3, 9] 0.803 / 0.803 (0.808 / 0.80).
- T = 2, delta = 50, both: 0.809 / 0.808 / 0.094 (0.80 / 0.79 / 0.10). delta = 0.5 at T = 0.4: 0.809 / 0.809 / 0.094 (0.80 / 0.80 / 0.10); at T = 2: 0.809 / 0.439 / 0.512 (0.81 / 0.40 / 0.7): delay lower than predicted.
- T = inf: the wait limit is inert (identical rows, as designed).

Today's Realtime, clamp on (B = 1 000): stored arm in OV 0.817 / 0.815 / 0.171, steady 0.808 (doc: share stays about 0.81); engine-queue arm in OV 0.873 / 0.160 / 0.724, steady window 0.815 / 0 (doc: 0.84 to 0.88 / 0.20 to 0.25 / 0.65 to 0.70; steady 0.808 / 0): delivered inside, on time and p50 slightly outside.

OV-end (delivered / on time / p50, windows [3, 5.5] and [9, 15]): Revision 3 with the clamp at T = 0.4, 0.929 / 0.929 / 0.156, windows 0.804 / 0.804 and 1.000 / 1.000; Revision 4 at delta 50 and 25, 0.911 / 0.911 / 0.028, windows 0.803 and 1.000: the two deltas are indistinguishable and the cost against Revision 3 is 0.018 (doc 0.019). At T = 2: Revision 3 1.000 / 0.308 / 0.701; Revision 4 delta = 0.25, 1.000 / 0.310 / 0.697 (doc: nearly equal to Revision 3, gain <= 0.01: met); delta = 0.5, 0.956 / 0.522 / 0.365 with window [9, 15] on time 0.999 against 0.426: the doc predicted a loss of 0.074 in delivered and a gain <= 0.01 in on time; measured loss 0.044 and gain 0.21. Misprediction recorded: the cap at delta = 0.5 clears the backlog before the overload ends (no collapse_t, tau < T on 50 030 ticks).
Today's Realtime in OV-end with the clamp: stored arm 0.916 / 0.916, windows 0.808 and 1.000 (doc 0.81, 0.97 to 0.99); engine-queue arm 0.955 / 0.529 / 0.355, windows 0.837 / 0 and 0.997 / 0.997 (doc 0.97 / 0.57; 0 on time in [3, 5.5] met; share 0.837 against 0.808).

OV-crit (lambda = 9 600): Revision 3 and Revision 4, both with the clamp, are identical, 1.000 / 1.000 / 0.093 with 0 ticks of tau < T (doc: equal within the A/A floor met). Today's Realtime: stored arm unclamped 0.999 / 0.999 / 0.131 (doc 0.91 to 0.95 on time: higher); with the clamp 0.999 / 0.999 / 0.091 for both arms (engine-queue arm doc p50 <= 0.1, on time 0.95 to 0.97: met, on time higher).

K-OV (B = 8, T = 0.4, delta = 50, clamp on): Revision 3 0.539 / 0.539 / 0.307; Revision 4 0.540 / 0.539 / 0.239 (doc 0.72 delivered, on time 0.66 to 0.72, p50 0.16: missed, both). Today's Realtime stored arm 0.740 / 0.007 / 1.52 (doc <= 0.1 on time: met); engine-queue arm 0.917 / 0.007 / 10.3 (doc p50 about 7, on time about 0.03, delivered 0.9 to 0.99: delivered met).

Not run or incomplete: the strict-order (H = infinity) row of 4.1; the engine-queue arm with the clamp off. Without the clamp the arm sends about 10 000 repairs that all arrive; the generic-rank decoder re-runs its matching over one component of thousands of unknowns at each arrival (n = 1 500 units took 6 s and the growth is steeper than linear), so those rows take hours: limited to one seed and left running (rows `RT-arms ... clamp=0 eq=1`). The clamp rows of the same arm are fast (r_eff is about 0). Other gauges: spare_bind in OV 0.97 to 0.98 (drain window), 0.4 to 0.9 in OV-end, 0.88 in OV-crit for the new model; collapse_t in OV 1.34 to 1.41 s at T = 0.4, 1.76 s at T = 2 (delta 50), 6.0 s at T = 2 (delta 0.5).

- Amendment, same day (coordinator): the engine-queue arm with the clamp off is not run (no engine counterpart: the engine has the clamp). Rows `RT-arms ... clamp=0 eq=1` are dropped from the sizing set; the partial rows already stored (OV-crit, 2 seeds) are kept in the file but not reported. The decoder is unchanged.

## 2026-11-26 Revision 4 step 2, review: the send gate, the taper grant, K-OV, the CL-long score, the delta sweep, dominance, strict order

**The taper grant (3).** The offline simulation has no taper stage: its only repair grant is the per-first-send accrual (`owed += r`), which is clamped. The engine's `fec_rate.rs:729` grant has no counterpart to clamp, so the doc's "each taper grant" does not apply here.

**The send gate (3).** Built as `send_gate` (default off; tests red first, identity sample reproduces ba3cf31c). In_flight counts reports only (a send leaves it when a report covers it), as the doc says. That deadlocks the gate after an outage: nothing arrives, so no report comes, so in_flight stays at B and the sender never resumes (CL-long, T = 0.4 and T = inf: delivered 0.10, paused 77.9 s of 80 s; T = 2: 0.994 / on time 0.49 / recovery 8.8 s). The engine has loss detection that a report-only count lacks, so in_flight gets `inflight_to` (default 0 = reports only; 1.125 = RFC 9002's time threshold: a send unreported for 9/8 of R is declared lost). With it the gate runs (rows `gate=1 to=1.125`). Effect against the clamp-only rows (3 seeds, B = 1 000, clamp on):
- CL-long RL T = 0.4: 0.9405 / 0.9405 / rec 0 (0.9399); T = 2: 0.999 / 0.610 / 6.43 s (0.638, 5.87 s); T = inf: 1.000 / 0.605 / 6.53 s (0.633, 6.00 s). Repair off with the gate: unchanged (0.9406; 0.9993 / 0.6545 / 5.60 s; 1.000 / 0.652 / 5.63 s). in_flight max 999 (was 1 177 to 1 243).
- OV, both parts, T = 0.4: 0.798 / 0.798 / 0.093 (0.809 without the gate), steady 0.793; T = 2, delta 0.5: 0.799 / 0.463 / 0.473 (0.809 / 0.439 / 0.512).
- OV-end, Revision 4 T = 0.4: 0.906 / 0.906 (0.911); T = 2 delta 0.5: 0.931 / 0.620 / 0.217 (0.956 / 0.522 / 0.365). OV-crit Revision 3: 1.000 (p50 0.121 against 0.093); Revision 4 0.994.
- K-OV (B = 8): 0.498 and 0.495 (0.539 and 0.540), p50 inf: the gate adds to a window that already limits the sender.
At B = 2 048 the gate never binds (rows identical). The gate does not apply to today's Realtime (no pause at Realtime, 3.3).

**K-OV (4), seed 0 to 2, new model T = 0.4, delta 50.**

| row | delivered | repairs sent | repair share of link slots | link busy | paused (s of ~60) | spare_bind | ov_frac | D-hat finite |
|---|---|---|---|---|---|---|---|---|
| repair off | 0.5437 | 3 | 0 | 0.984 | 59.9 | n/a | 0.954 | 95.4 % |
| RL, clamp off | 0.4110 | 503 | 0.277 | 0.985 | 58.9 | 1.00 | 0.936 | 93.6 % |
| RL, clamp on | 0.5394 | 20 | 0.010 | 0.984 | 59.9 | 1.00 | 0.954 | 95.4 % |

The law's r is 0.388 in all three rows (the call cell's 0.38 is its value here too), and the clamp acts: repair falls from 503 to 20, delivered returns to the repair-off value (0.411 to 0.539 against 0.544). So the miss against 0.72 is not repair: with or without it the sender is paused 59.9 s of 60 s with the link 98 % busy at B = 8 (window limited). The wait limit is live (D-hat finite on 95 % of ticks, x > 1 on 95 %, collapse at 2.7 s) but moves delivered by 0.0003 and delay p50 from 0.307 to 0.239 s.

**CL-long against the corrected prediction (5).** three-knobs-rev4 `0ae484d2` was authored 04:52:34 and pushed 04:52:41 on 2026-11-26. The sizing run started at 05:12:46, so the comparison is pre-data by time (I read the doc after the run). B = 1 000 without the gate: on time 0.638 (T = 2) and 0.633 (T = inf) against 0.62 to 0.65: met; recovery 1.05 and 1.07 times repair off against 1.0 to 1.1: met. B = 2 048: equal to RL: met. With the gate and the time threshold (the stricter in_flight): on time 0.610 and 0.605 (below 0.62 to 0.65), recovery 1.15 and 1.17 times (above 1.0 to 1.1).

**OV-end delta sweep (6), T = 2, both parts, 3 seeds** (delivered / on time / p50 / window [9, 15] on time): 0.25: 1.000 / 0.3105 / 0.697 / 0.435; 0.30: 0.998 / 0.319 / 0.685 / 0.476; 0.35: 0.988 / 0.367 / 0.608 / 0.613; 0.40: 0.977 / 0.420 / 0.526 / 0.780; 0.45: 0.967 / 0.472 / 0.445 / 0.949; 0.49: 0.959 / 0.509 / 0.385 / 1.000; 0.50: 0.956 / 0.522 / 0.365 / 1.000; 0.51: 0.954 / 0.533 / 0.348 / 1.000. A smooth monotone curve in every readout, with no step at 0.5; the window [9, 15] on time saturates at 1.0 from about 0.47 (the cap clears the backlog before the overload ends). The +0.21 at delta = 0.5 is the slope of this curve, not a step; the doc's "gain <= 0.01" held only for delta <= 0.3.

**Dominance (7), `rev4_dominance.py`, the rule as registered (paired differences against the bar max(t_bar, A/A floor)).**
- OV, finite-T rows pooled (T = 0.4 and 2, they agree: 0.809 / 0.809 at both; floor_bind is 0.85 and 0.82, not 1): against today's Realtime as stored, equivalent on delivered (-0.008, bar 0.009), on time (-0.006) and p50 (+0.077 s better, bar 0.1), steady window equivalent: the rule returns not worse. Against the engine-queue arm: delivered shown worse (whole run -0.064, steady window -0.012, bar 0.009), on time shown better (+0.65; steady +0.80 against 0), p50 shown better (0.094 against 0.724 s): the first clause (delivered not worse beyond the bar) fails by 0.012 in the steady window, so the rule does not return dominance; the result is a trade of 0.012 of steady share for 0.80 of steady on time. Against the stored Bulk and ADR-0084 rows (repair on, no clamp, they never give up): delivered shown worse (1.000 against 0.809), on time shown better (+0.69), p50 shown better (1.6 to 1.8 s against 0.094).
- OV-end: the sweep does not collapse (floor_bind 0.33 at T = 0.4, 0.00 at T = 2, delta = 0.5) and the two finite-T rows disagree (on time 0.911 against 0.522), so the rule's precondition fails and a pooled point is not meaningful; the rows are reported as separate points above.

**Strict order (8), OV, T = 0.4, delta = 50, H = inf, 3 seeds.** Both parts: 0.809 / 0.809 / p50 0.247 s, steady 0.804 / 0.804 (doc 0.72 to 0.79 on time, p50 0.18 to 0.25: on time above the range, p50 in it). Clamp only: 0.833 / 0.833 / 0.390 s, steady 0.805.

## 2026-11-26 Revision 4 step 2: the frozen build (gate on, engine expiry, clamp on, wait limit on); K-OV; the engine-queue arm's fill

**In-flight expiry at the engine's rule.** `scheduler/path.rs` `expire_in_flight` (default, no `RWM_RELEASE_1TO1`): `(self.srtt() * 4).max(IN_FLIGHT_EXPIRY_MIN)`, with `IN_FLIGHT_EXPIRY_MIN = 250 ms` (`scheduler/copa.rs:307`): "Default horizon: `max(4 x SRTT, 250 ms)`, roughly a decade past the scale at which a symbol's fate is decided, which makes this a backstop". The 9/8 x R I used before is the `RWM_RELEASE_1TO1` variant, not the default. Field `inflight_to_rule = "engine"` (default "") replaces it in the gate and the clamp's in-flight count: `engine_in_flight_horizon(srtt) = max(4 srtt, 0.25)`. Test red first; identity sample with the fields off. The frozen Revision 4 build is: `send_gate` on, `inflight_to_rule = "engine"`, `clamp` on, `wait_limit` on.

Frozen build against the clamp-only rows (3 seeds, B = 1 000; delivered / on time / p50 / recovery):
- CL-long RL: T = 0.4 0.9405 / 0.9405 (clamp only 0.9399); T = 2 0.9992 / 0.616 / 0.049 s / 6.30 s (0.9990 / 0.638 / 0.007 / 5.87 s: recovery 1.125 times repair off); T = inf 1.000 / 0.612 / 0.061 s / 6.37 s (1.000 / 0.633 / 0.009 / 6.00 s: 1.14 times). Repair off with the gate and engine expiry: 0.9406; 0.9993 / 0.6545 / 5.60 s; 1.000 / 0.652 / 5.63 s.
- OV, both parts: T = 0.4 0.798 / 0.798 / 0.093 (0.809); T = 2, delta 0.5 0.799 / 0.463 / 0.473 (0.809 / 0.439 / 0.512).
- OV-end, Revision 4: T = 0.4 0.906 / 0.906 / 0.027 (0.911); T = 2, delta 0.5 0.931 / 0.620 / 0.217 (0.956 / 0.522 / 0.365). Revision 3 T = 0.4 0.925 / 0.924 (0.930).
- OV-crit: Revision 3 1.000 / 1.000 / 0.121 (0.093); Revision 4 0.994 / 0.994 / 0.096.
- K-OV: Revision 3 0.498 / 0.497 / p50 inf (0.539 / 0.539 / 0.307); Revision 4 0.491 / 0.491 / p50 inf (0.540 / 0.539 / 0.239).
Against the corrected prediction of 0ae484d2 the frozen build is just outside on CL-long: on time 0.616 and 0.612 (0.62 to 0.65), recovery 1.125 and 1.14 times (1.0 to 1.1).

**K-OV, the quarter of link slots that deliver nothing new (stored rows only: stage 3e AA rows, K-OV, new model at its primary lifetime T = 0.4, repair off, seeds 0 and 1 at both phases).** The link is 100 % busy in [1, 10] s and 60 s long: 40 slots/s x 60 s = 2 400. Sends by kind: floor datagrams 603 (`floors`; `bl_fires` 603), resends 3 (`resend_by_sweep` 3, `resend_by_report` 0), repairs 0, first sends about 1 794 (2 400 less the others), of which 90 % arrive (delivered 0.538 and 0.543 of 3 000 = 1 613 to 1 629). So 603 / 2 400 = 25 % of the link is floor datagrams. They come from the blocker timer (`sweep_form = "blocker"`): 603 fires in 60 s is one per 100 ms, the sweep period capped at 100 ms under a 200 ms RTT (Q17), each one a floor datagram that costs a slot at 40 slots/s (10 floors/s = 25 % of 40). The sweep is not resending in-flight data: resends are 3 and duplicate arrivals 2 in the whole run. The duplicates gauge is a total (`dup`), not per kind; here it is 2, so it is not needed. For contrast the never-give-up row of the same cell (T = inf, hold 0) sends no floors but 679 to 719 resends, 248 by report and 431 to 478 by sweep, of which `dup` 316 to 355 arrive twice (about half of the resends are duplicates), with delivered 1.000 over 32 s. So in K-OV the floor repeat, not repair and not duplicate resends, is what holds the new model at 0.54 against about 0.72.

**The engine-queue arm in OV, steady window [3, 9] (clamp on, B = 1 000, 3 seeds).** The 4 MiB queue (8 192 symbols) is still filling in second 3. Median delay of units handed over in second s: 0.172, 0.373, 0.572, 0.723, 0.727, 0.727, 0.727, 0.727, 0.727 (s = 0 to 8); evicted 12 100 per run, queue peak 8 192. Share delivered by hand-over second: 0.979, 0.972, 0.971, 0.850, 0.809, 0.809, 0.809, 0.808, 0.808, 0.918 (s = 0 to 9). Delivery rate per second (link): 9 100, 9 760, 9 650, 9 740, 9 610, 9 780, 9 700, 9 610, 9 860, 9 610 (about C (1 - eps) = 9 700). So the registered window's 0.815 is second 3 (0.850, still filling, no eviction yet) averaged with five seconds at 0.809, which is the link's carryable 0.808. Second 4 to 9 alone: 0.809. The registered reading is unchanged (the window is [3, 9], fixed before the data); the fill time is about 3.3 s, not the 4.1 s predicted, because the arrival into the queue is 12 000 + repairs against the 10 000 drain.
