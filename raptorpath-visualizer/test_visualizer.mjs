// Headless test of the BUILT single-file visualizer.
//
// Extracts the embedded engine block (wasm + glue + SimWrapper) from
// interactive-visualizer.html and runs real transfers through it in Node —
// the same bytes a browser executes. Run by build_visualizer.sh after
// generation; a failure fails the build. The deployed artifact remains a
// single self-contained HTML file: these tests are build-time gates.
//
// Usage: node raptorpath-visualizer/test_visualizer.mjs

import { readFileSync } from "node:fs";

const html = readFileSync(
  new URL("./interactive-visualizer.html", import.meta.url),
  "utf8"
);

const start = html.indexOf("// WASM MODULE");
const end = html.indexOf("// UI");
if (start < 0 || end < 0 || end <= start) {
  throw new Error("build markers not found in interactive-visualizer.html");
}
const engine = html.slice(start, end);

// Evaluate the engine block and harvest the symbols under test.
const factory = new Function(
  engine +
    `
  return {
    Simulation,
    compute_r_star_continuous,
    controller_rate,
    burst_variance_factor,
    r_saturation,
    soft_saturate,
    saturation_pressure,
    p_fec_exact,
    compute_r_star_exact,
    p_lost,
    zeta_of_delta,
    span_horizon_b,
    span_deadline_d,
    span_width_a_star,
    pipeline_depth_m_star,
    trailing_offset_delta,
    shed_budget_residual,
    sim_tail_target_of_delta,
  };
`
);
const api = factory();

let failures = 0;
function check(name, cond, detail = "") {
  if (cond) {
    console.log(`PASS  ${name}${detail ? "  (" + detail + ")" : ""}`);
  } else {
    console.error(`FAIL  ${name}${detail ? "  (" + detail + ")" : ""}`);
    failures++;
  }
}

function runSim(hint, { fixedR, delta, rho } = {}) {
  const sim = new api.Simulation(0.05, 0.5, 50, 64, hint, fixedR, delta, rho);
  let ticks = 0;
  while (!sim.is_finished() && ticks < 20000) {
    sim.step();
    ticks++;
  }
  return {
    ticks,
    finished: sim.is_finished(),
    decoded: sim.get_cum_decoded(),
    givenUp: typeof sim.get_given_up === "function" ? sim.get_given_up() : 0,
    reliability:
      typeof sim.get_reliability === "function" ? sim.get_reliability() : 1,
    numSource: sim.get_num_source(),
    fec: sim.get_total_fec(),
    arq: sim.get_total_arq(),
    overhead: sim.get_overhead(),
    latAvg: sim.get_lat_avg(),
    latP99: sim.get_lat_percentile(0.99),
    jitter: sim.get_jitter(),
  };
}

// --- 1. Every hint completes the transfer (rho = 100%) ---
const auto = runSim("auto");
check("auto completes fully", auto.finished && auto.decoded === auto.numSource,
  `decoded=${auto.decoded}/${auto.numSource} in ${auto.ticks} ticks`);

const bulk = runSim("bulk");
check("bulk completes fully", bulk.finished && bulk.decoded === bulk.numSource,
  `decoded=${bulk.decoded}/${bulk.numSource} in ${bulk.ticks} ticks`);

const rt = runSim("realtime");
check("realtime completes fully", rt.finished && rt.decoded === rt.numSource,
  `decoded=${rt.decoded}/${rt.numSource} in ${rt.ticks} ticks`);

// --- 2. Bulk (throughput target) finishes BEFORE Realtime ---
check("bulk completes faster than realtime", bulk.ticks < rt.ticks,
  `bulk=${bulk.ticks} ticks, realtime=${rt.ticks} ticks`);

// --- 3. Hint semantics: Bulk mostly ARQ, Realtime FEC-heavy ---
check("bulk sends less FEC than realtime", bulk.fec < rt.fec,
  `bulk fec=${bulk.fec}, realtime fec=${rt.fec}`);

// --- 3b. Latency metrics show the latency/throughput trade ---
const ONE_WAY = 25; // rtt 50ms in runSim
check("avg latency >= one-way propagation",
  bulk.latAvg >= ONE_WAY && rt.latAvg >= ONE_WAY,
  `bulk=${bulk.latAvg.toFixed(1)}ms, rt=${rt.latAvg.toFixed(1)}ms`);
check("realtime p99 latency beats bulk", rt.latP99 < bulk.latP99,
  `rt=${rt.latP99.toFixed(1)}ms, bulk=${bulk.latP99.toFixed(1)}ms`);
check("jitter finite and non-negative",
  Number.isFinite(rt.jitter) && rt.jitter >= 0,
  `rt jitter=${rt.jitter.toFixed(2)}ms`);

// --- 4. Custom triangle mode: rho < 1 gives up cleanly ---
const custom = runSim("custom", { delta: 0.05, rho: 0.95 });
check(
  "custom rho: decoded + given_up == num_source",
  custom.decoded + custom.givenUp === custom.numSource,
  `decoded=${custom.decoded}, givenUp=${custom.givenUp}`
);
check("custom rho: reliability >= 0.90", custom.reliability >= 0.9,
  `reliability=${custom.reliability.toFixed(4)}`);

// --- 5. Fixed-r mode pins the rate ---
const fixed = runSim("fixed", { fixedR: 0.15 });
check("fixed-r completes fully", fixed.finished && fixed.decoded === fixed.numSource,
  `overhead=${fixed.overhead.toFixed(1)}%`);

// --- 6. Continuous r* glides to 0 as delta -> eps (paper 8.4) ---
const rTight = api.compute_r_star_continuous(0.025, 2.9, 64, 1e-6);
const rLoose = api.compute_r_star_continuous(0.025, 2.9, 64, 0.03);
check("continuous r*: tight delta -> positive rate", rTight > 0.05,
  `r(1e-6)=${rTight.toFixed(3)}`);
check("continuous r*: delta >= eps -> 0 (pure ARQ)", rLoose === 0,
  `r(0.03)=${rLoose}`);

// --- 7. Shared controller + saturation are callable and sane ---
// (args: ..., tail_target, bulk_late_is_fine, completion_exposure (P6 chi),
//  saturation_cap, max_overhead)
const rate = api.controller_rate(
  0.09, 5.1, 3.3, 64, 400, 0.2, 5e-4, 0.004, 1e-7, false, 0, true, 0.5
);
const rsat = api.r_saturation(0.09, 5.1, 64, 0.2, 5e-4);
check("controller_rate finite and capped", rate > 0 && rate <= rsat + 1e-12,
  `rate=${rate.toFixed(3)}, r_sat=${rsat.toFixed(3)}`);

// --- 7b. Soft saturation + pressure (paper 14.21.1) ---
// soft_saturate never exceeds min(rate, r_sat); pressure is 0.5 at r_sat and
// monotone. This is the continuous cap the CAP badge now reflects.
check("soft_saturate never exceeds r_sat",
  api.soft_saturate(5.0, rsat) <= rsat + 1e-12
    && api.soft_saturate(0.02, rsat) <= 0.02 + 1e-9,
  `soft(5,rsat)=${api.soft_saturate(5.0, rsat).toFixed(3)}`);
check("saturation_pressure = 0.5 at r_sat",
  Math.abs(api.saturation_pressure(rsat, rsat) - 0.5) < 1e-9);
check("saturation_pressure monotone (low < high)",
  api.saturation_pressure(0.05, rsat) < api.saturation_pressure(0.9, rsat)
    && api.saturation_pressure(0.9, rsat) <= 1.0);
// The live Simulation exposes the pressure accessor in [0,1].
{
  const s = new api.Simulation(0.09, 0.5, 200, 64, "realtime", undefined, undefined, undefined);
  for (let i = 0; i < 300 && !s.is_finished(); i++) s.step();
  const p = s.get_saturation_pressure();
  check("Simulation.get_saturation_pressure in [0,1]",
    Number.isFinite(p) && p >= 0 && p <= 1, `pressure=${p.toFixed(3)}`);
}

const pfx = api.p_fec_exact(0.013, 0.5, 0.1, 64);
check("exact P_fec sane", pfx > 0.9 && pfx <= 1.0, `p_fec_exact=${pfx.toFixed(4)}`);

// --- 7c. THE UNIFIED SPAN LAW (paper §16.20.3, §16.26, §12.4; ADR-0064) —
// formula fidelity against the paper's stated formulas, ≥3 spot values per
// formula, hand-computed. These are the quantities the centerpiece panel
// animates; a drift here is a lie on screen and fails the build.
function near(a, b, rel = 1e-9) {
  return Math.abs(a - b) <= rel * Math.max(1, Math.abs(b));
}
// §12.4: δ(hint) = 0.5/ζ ⇒ ζ = 0.5/δ; the three preset anchors.
check("§12.4 ζ(δ) at the three presets",
  near(api.zeta_of_delta(50), 0.01) &&
  near(api.zeta_of_delta(0.5), 1) &&
  near(api.zeta_of_delta(0.005), 100),
  `ζ(50)=${api.zeta_of_delta(50)}, ζ(0.5)=${api.zeta_of_delta(0.5)}, ζ(0.005)=${api.zeta_of_delta(0.005)}`);
// §16.26: b(hint) = ½/1/2 at Realtime/Auto/Bulk; D = min(b·RTprop, 2·RTprop).
check("§16.26 b(δ) anchors ½/1/2",
  near(api.span_horizon_b(50), 0.5) &&
  near(api.span_horizon_b(0.5), 1) &&
  near(api.span_horizon_b(0.005), 2),
  `b(50)=${api.span_horizon_b(50)}, b(0.5)=${api.span_horizon_b(0.5)}, b(0.005)=${api.span_horizon_b(0.005)}`);
check("§16.26 D(δ) = min(b·RTprop, 2·RTprop) at RTprop=100ms",
  near(api.span_deadline_d(50, 0.1), 0.05) &&
  near(api.span_deadline_d(0.5, 0.1), 0.1) &&
  near(api.span_deadline_d(0.005, 0.1), 0.2) &&
  near(api.span_deadline_d(1e-6, 0.1), 0.2), // 2·RTprop cap holds below Bulk
  `D=[${api.span_deadline_d(50,0.1)}, ${api.span_deadline_d(0.5,0.1)}, ${api.span_deadline_d(0.005,0.1)}]`);
// §16.20.3: A* = clamp(rate·D, 1, W) — incl. the paper's 200 sym/s × 20 ms
// voice example (= 4).
check("§16.20.3 A* = clamp(rate·D, 1, W)",
  near(api.span_width_a_star(200, 0.02, 512), 4) &&
  near(api.span_width_a_star(1000, 0.2, 64), 64) &&   // W clamp
  near(api.span_width_a_star(10, 0.002, 512), 1),     // floor 1
  `A*=[${api.span_width_a_star(200,0.02,512)}, ${api.span_width_a_star(1000,0.2,64)}, ${api.span_width_a_star(10,0.002,512)}]`);
// §16.20.3/§16.20.5: M* = ceil(rate·2·RTprop/A*_q)+1, clamped [2, 32].
check("§16.20.3 M* = ceil(rate·2RTprop/A*q)+1 clamp [2,32]",
  near(api.pipeline_depth_m_star(5000, 0.1, 128), 9) && // ceil(1000/128)+1
  near(api.pipeline_depth_m_star(200, 0.01, 4), 2) &&   // floor (depth inert)
  near(api.pipeline_depth_m_star(1e6, 0.1, 1), 32),     // memory ceiling
  `M*=[${api.pipeline_depth_m_star(5000,0.1,128)}, ${api.pipeline_depth_m_star(200,0.01,4)}, ${api.pipeline_depth_m_star(1e6,0.1,1)}]`);
// §16.20.3/ADR-0064: Δ = clamp(⌈rate·J⌉, 1, 64).
check("§16.20.3 Δ = clamp(⌈rate·J⌉, 1, 64)",
  near(api.trailing_offset_delta(200, 0.005), 1) &&
  near(api.trailing_offset_delta(5000, 0.003), 15) &&
  near(api.trailing_offset_delta(1e6, 1), 64) &&
  near(api.trailing_offset_delta(10, 0), 1),
  `Δ=[${api.trailing_offset_delta(200,0.005)}, ${api.trailing_offset_delta(5000,0.003)}, ${api.trailing_offset_delta(1e6,1)}]`);
// §16.26: 1−ρ = ε̂·(1−P_fec) — bounds, ε̂=0 zero, monotone-decreasing in r.
{
  const b0 = api.shed_budget_residual(0.0, 0.1, 64, 2.0);
  const bLo = api.shed_budget_residual(0.05, 0.0, 64, 2.9);
  const bHi = api.shed_budget_residual(0.05, 0.30, 64, 2.9);
  check("§16.26 shed budget 1−ρ = ε̂·(1−P_fec)",
    b0 === 0 && bLo > 0 && bLo <= 0.05 + 1e-12 && bHi < bLo && bHi < 0.01,
    `budget(ε̂=0)=${b0}, budget(r=0)=${bLo.toFixed(4)}, budget(r=.3)=${bHi.toExponential(2)}`);
}
// The δ-continuum tail-target mapping: the three §12.4 anchors, exact.
check("δ-continuum tail targets at the presets (50→1e-7, 0.5→1e-5, 0.005→0.05)",
  near(api.sim_tail_target_of_delta(50), 1e-7, 1e-9) &&
  near(api.sim_tail_target_of_delta(0.5), 1e-5, 1e-9) &&
  near(api.sim_tail_target_of_delta(0.005), 0.05, 1e-9),
  `tails=[${api.sim_tail_target_of_delta(50).toExponential(2)}, ${api.sim_tail_target_of_delta(0.5).toExponential(2)}, ${api.sim_tail_target_of_delta(0.005).toExponential(2)}]`);

// --- 7d. The δ continuum drives the SIM continuously (the UI path:
// 'custom' + derived tail strictly BETWEEN presets; AT the Bulk preset
// the engine's late-is-fine 'bulk' law verbatim): the Realtime end pays
// FEC, the Bulk end is pure ARQ and completes faster.
{
  const rtEnd = runSim("custom", { delta: api.sim_tail_target_of_delta(50), rho: 1.0 });
  const bulkEnd = runSim("bulk"); // the UI's Bulk-preset path (§14.26)
  check("δ continuum: both ends deliver fully",
    rtEnd.decoded === rtEnd.numSource && bulkEnd.decoded === bulkEnd.numSource,
    `rt=${rtEnd.decoded}, bulk=${bulkEnd.decoded}`);
  check("δ continuum: Realtime end sends more FEC than Bulk end",
    rtEnd.fec > bulkEnd.fec,
    `fec rt-end=${rtEnd.fec}, bulk-end=${bulkEnd.fec}`);
  check("δ continuum: Bulk end completes faster",
    bulkEnd.ticks < rtEnd.ticks,
    `bulk-end=${bulkEnd.ticks} ticks, rt-end=${rtEnd.ticks} ticks`);
  // The ρ dial stays a thing (the triangle's second corner): the UI path
  // with ρ < 1 gives up cleanly via §6.1 T_cut toward the declared target.
  const lossyRho = new api.Simulation(
    0.10, 0.3, 80, 64, "custom",
    undefined, api.sim_tail_target_of_delta(0.5), 0.95);
  let t = 0;
  while (!lossyRho.is_finished() && t++ < 20000) lossyRho.step();
  check("ρ dial on the continuum: decoded + given_up == num_source, reliability ≥ 0.90",
    lossyRho.get_cum_decoded() + lossyRho.get_given_up() === lossyRho.get_num_source() &&
    lossyRho.get_reliability() >= 0.90,
    `decoded=${lossyRho.get_cum_decoded()}, givenUp=${lossyRho.get_given_up()}, rel=${lossyRho.get_reliability().toFixed(4)}`);
}

// --- 7d2. The Bulk preset runs the engine's late-is-fine law (§14.26):
// pure ARQ mid-stream at ε = 5% (r = 0 identically, cold start included),
// the χ completion glide intact at the stream tail; at ε = 10% (p̂ above
// the 0.05 tail budget) the glide actually EMITS tail FEC.
{
  const b5 = new api.Simulation(0.05, 0.5, 50, 64, "bulk", undefined, undefined, undefined);
  let midR = 0, t = 0;
  for (; t < 300; t++) { b5.step(); midR = Math.max(midR, b5.get_r_star()); }
  while (!b5.is_finished() && t++ < 20000) b5.step();
  check("Bulk preset: mid-stream r = 0 at ε=5% (late is fine, §14.26)",
    midR === 0, `max mid-stream r=${midR}`);
  check("Bulk preset: χ completion glide fired by end of stream",
    b5.get_completion_exposure() > 0.95,
    `χ=${b5.get_completion_exposure().toFixed(3)}`);
  check("Bulk preset: delivers fully, ~zero FEC at ε=5%",
    b5.get_cum_decoded() === b5.get_num_source() && b5.get_total_fec() <= 5,
    `fec=${b5.get_total_fec()}, decoded=${b5.get_cum_decoded()}`);
  const b10 = new api.Simulation(0.10, 0.5, 50, 64, "bulk", undefined, undefined, undefined);
  t = 0;
  while (!b10.is_finished() && t++ < 20000) b10.step();
  check("Bulk preset: tail glide emits FEC once p̂ exceeds the 0.05 budget (ε=10%)",
    b10.get_total_fec() > 0 && b10.get_cum_decoded() === b10.get_num_source(),
    `fec=${b10.get_total_fec()}`);
}

// --- 7e. Retention store (walls #7/#9): SACK-clocked occupancy within the
// path-scaled pool and ≤ the frontier-clocked counterfactual; the pool
// BINDS when cap·RTT outgrows it.
{
  const s = new api.Simulation(0.05, 0.5, 50, 64, "auto", undefined, undefined, undefined);
  let okInv = true, sawOcc = false;
  while (!s.is_finished() && s.get_tick() < 20000) {
    s.step();
    if (s.get_store_occupancy() > s.get_store_occupancy_frontier()) okInv = false;
    if (s.get_store_occupancy() > 100) sawOcc = true;
  }
  check("store: SACK-clocked ≤ frontier-clocked counterfactual, gauge live",
    okInv && sawOcc && s.get_pool_cap() === 512 && s.get_pool_stalls() === 0,
    `cap=${s.get_pool_cap()}, stalls=${s.get_pool_stalls()}`);
  const big = api.Simulation.multipath(
    [0.02], [0.5], new Uint32Array([200]), new Uint32Array([8]),
    64, "auto", undefined, undefined, undefined);
  let t = 0;
  while (!big.is_finished() && t++ < 20000) big.step();
  check("store: pool binds when cap·RTT ≫ pool (wall #7), transfer still completes",
    big.get_pool_stalls() > 0 && big.get_cum_decoded() === big.get_num_source(),
    `stalls=${big.get_pool_stalls()}`);
}

// --- 7f. Per-path recovery clocks (wall #8, §16.24): heterogeneous RTTs
// hold slow-path holes past the aggregate clock (phantom retx avoided);
// homogeneous paths produce none.
{
  const het = api.Simulation.multipath(
    [0.026, 0.048], [0.5, 0.5], new Uint32Array([10, 40]), new Uint32Array([5, 1]),
    64, "bulk", undefined, undefined, undefined);
  let t = 0;
  while (!het.is_finished() && t++ < 20000) het.step();
  const homo = api.Simulation.multipath(
    [0.05, 0.05], [0.5, 0.5], new Uint32Array([50, 50]), new Uint32Array([4, 4]),
    64, "bulk", undefined, undefined, undefined);
  t = 0;
  while (!homo.is_finished() && t++ < 20000) homo.step();
  check("per-path clocks: phantoms avoided on heterogeneous, none on homogeneous",
    het.get_phantom_avoided() > 0 && homo.get_phantom_avoided() === 0,
    `het avoided=${het.get_phantom_avoided()}, homo=${homo.get_phantom_avoided()}`);
}

// --- 8. Multipath (paper §16, Reliable Windowed Multipath at L0) ---
function runInner(sim) {
  let ticks = 0;
  while (!sim.is_finished() && ticks < 20000) {
    sim.step();
    ticks++;
  }
  return ticks;
}
// 8a. N = 1 via the multipath constructor must be IDENTICAL to the classic
// single-path constructor (the regression contract).
{
  const a = new api.Simulation(0.05, 0.5, 50, 64, "auto", undefined, undefined, undefined);
  const b = api.Simulation.multipath(
    [0.05], [0.5], new Uint32Array([50]), new Uint32Array([4]),
    64, "auto", undefined, undefined, undefined
  );
  const ta = runInner(a), tb = runInner(b);
  check(
    "multipath N=1 identical to single-path",
    ta === tb &&
      a.get_total_fec() === b.get_total_fec() &&
      a.get_total_arq() === b.get_total_arq() &&
      a.get_cum_decoded() === b.get_cum_decoded() &&
      b.get_num_paths() === 1,
    `ticks ${ta}/${tb}, fec ${a.get_total_fec()}/${b.get_total_fec()}`
  );
}
// 8b. Symmetric 2-path: one shared window over two equal paths completes
// ~2x faster than one path (order-statistic aggregation, §16.3), and the
// per-path accessors are live.
{
  const single = new api.Simulation(0.05, 0.5, 50, 64, "bulk", undefined, undefined, undefined);
  const dual = api.Simulation.multipath(
    [0.05, 0.05], [0.5, 0.5], new Uint32Array([50, 50]), new Uint32Array([4, 4]),
    64, "bulk", undefined, undefined, undefined
  );
  const ts = runInner(single), td = runInner(dual);
  const factor = ts / td;
  // Gate 1.6–2.1 (was 1.7): under the per-path RFC 9002 retransmit clock
  // (§16.24 model, 2026-07-28) a drain straggler honestly costs a full
  // own-RTT detection round, paid on the dual run's shorter total.
  check(
    "2-path symmetric aggregation ~2x",
    dual.get_cum_decoded() === dual.get_num_source() && factor > 1.6 && factor <= 2.1,
    `single=${ts} ticks, dual=${td} ticks, x${factor.toFixed(2)}`
  );
  const af = dual.get_aggregation_factor();
  check(
    "aggregation factor accessor > 1 (beats best single path)",
    Number.isFinite(af) && af > 1.0,
    `factor=x${af.toFixed(2)} (agg ${dual.get_agg_goodput().toFixed(2)} vs best ${dual.get_best_single_goodput().toFixed(2)} sym/tick)`
  );
  const s0 = dual.get_path_src(0), s1 = dual.get_path_src(1);
  check(
    "symmetric striping ~50/50",
    Math.abs(s0 / (s0 + s1) - 0.5) < 0.05,
    `src split ${s0}/${s1}`
  );
}
// 8c. Heterogeneous C8-like (§16.6 P1 at L0): the shared window over
// fast+slow must STRICTLY beat the fast path alone — the §16.2 ceiling of
// every per-path-affine in-order transport. Measured vs measured, same
// engine, same hint/W.
{
  const fastAlone = api.Simulation.multipath(
    [0.026], [0.5], new Uint32Array([10]), new Uint32Array([5]),
    64, "bulk", undefined, undefined, undefined
  );
  const rwm = api.Simulation.multipath(
    [0.026, 0.048], [0.5, 0.5], new Uint32Array([10, 40]), new Uint32Array([5, 1]),
    64, "bulk", undefined, undefined, undefined
  );
  const tf = runInner(fastAlone), tr = runInner(rwm);
  const factor = tf / tr;
  check(
    "heterogeneous C8: RWM strictly beats fast path alone (P1)",
    rwm.get_cum_decoded() === rwm.get_num_source() && factor > 1.0,
    `fast-alone=${tf} ticks, RWM=${tr} ticks, x${factor.toFixed(3)} (asymptote ~x1.20)`
  );
}

if (failures > 0) {
  console.error(`\n${failures} check(s) FAILED`);
  process.exit(1);
}
console.log("\nAll visualizer engine checks passed.");
