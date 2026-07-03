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

const pfx = api.p_fec_exact(0.013, 0.5, 0.1, 64);
check("exact P_fec sane", pfx > 0.9 && pfx <= 1.0, `p_fec_exact=${pfx.toFixed(4)}`);

if (failures > 0) {
  console.error(`\n${failures} check(s) FAILED`);
  process.exit(1);
}
console.log("\nAll visualizer engine checks passed.");
