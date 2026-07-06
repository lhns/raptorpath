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
  check(
    "2-path symmetric aggregation ~2x",
    dual.get_cum_decoded() === dual.get_num_source() && factor > 1.7 && factor <= 2.1,
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
