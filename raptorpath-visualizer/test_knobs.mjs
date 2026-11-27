// Headless test of the BUILT three-knobs page's ENGINE block (proposal page).
//
// Extracts the embedded engine block (wasm + glue + SimWrapper) from
// knobs-visualizer.html and runs it in Node: the same bytes a browser
// executes. Run by build_visualizer.sh; a failure fails the build.
// Plan: docs/derivations/visualizer-three-knobs-plan.md section 5.4 item 10.
//
// What it gates:
//   1. the knob_* law exports at the named points T in {0.4, 2}, H = 0.02,
//      nudged +-2 %: no flip away from the threshold, release_at linear,
//      and the infinity cases (an IEEE value in the TESTS; the page script
//      never classifies it);
//   2. multipath_knobs at the inert knobs equals multipath, on eight
//      fingerprint fields, for the bulk / auto / realtime aliases; and
//      `new SimWrapper(params)` (main's page call) is that same run;
//   3. one stream cell: share_T and share_any at T = 0.4 and 0.4 * 1.02
//      within the derived bound of plan 5.4 item 5, with a positive control;
//   4. the constructor throws on an out-of-domain knob.
//
// Usage: node raptorpath-visualizer/test_knobs.mjs

import { readFileSync } from "node:fs";

const html = readFileSync(
  new URL("./knobs-visualizer.html", import.meta.url),
  "utf8"
);

const start = html.indexOf("// WASM MODULE");
const end = html.indexOf("// UI");
if (start < 0 || end < 0 || end <= start) {
  throw new Error("build markers not found in knobs-visualizer.html");
}
const engine = html.slice(start, end);

const factory = new Function(
  engine +
    `
  return {
    Simulation, SimWrapper, MAIN_KNOBS, BASELINE_CACHE,
    knob_law_revision, knob_intake_expired, knob_resend_admissible,
    knob_within_lifetime, knob_dequeue_expired, knob_protected,
    knob_trigger_delay, knob_one_way_fwd, knob_release_at, knob_ack_timeout,
  };
`
);
const api = factory();
const INF = Infinity;

let failures = 0;
function check(name, cond, detail = "") {
  if (cond) {
    console.log(`PASS  ${name}${detail ? "  (" + detail + ")" : ""}`);
  } else {
    console.error(`FAIL  ${name}${detail ? "  (" + detail + ")" : ""}`);
    failures++;
  }
}
const near = (a, b, tol = 1e-12) => Math.abs(a - b) <= tol;

// ---- 1. the law exports ------------------------------------------------
check("knob_law_revision names a revision",
  typeof api.knob_law_revision() === "string" && api.knob_law_revision().length > 0,
  api.knob_law_revision());

const O = 0.025; // a one-way term o-hat (RTT 50 ms)
for (const T of [0.4, 2]) {
  const ns = [0.5, 0.9, 0.98, 0.99, 1.01, 1.02, 1.5, 10]; // multiples of the threshold

  // L1: expired iff age >= T.
  {
    let ok = true, bad = "";
    for (const m of ns) {
      const want = m >= 1;
      if (api.knob_intake_expired(T, T * m) !== want) { ok = false; bad = `x${m}`; }
    }
    if (api.knob_intake_expired(T, T) !== true) { ok = false; bad = "exactly T"; }
    check(`L1 intake_expired T=${T}: flips only at age = T (+-2 % and beyond)`, ok, bad);
  }
  // L2: admissible iff age + o < T; threshold age = T - o.
  {
    const thr = T - O;
    let ok = true, bad = "";
    for (const m of ns) {
      const want = !(m >= 1);
      if (api.knob_resend_admissible(T, thr * m, O) !== want) { ok = false; bad = `x${m}`; }
    }
    if (api.knob_resend_admissible(T, thr, O) !== false) { ok = false; bad = "exactly T-o"; }
    check(`L2 resend_admissible T=${T}: flips only at age = T - o`, ok, bad);
  }
  // L3: within iff age <= T (a readout).
  {
    let ok = true, bad = "";
    for (const m of ns) {
      const want = m <= 1;
      if (api.knob_within_lifetime(T, T * m) !== want) { ok = false; bad = `x${m}`; }
    }
    if (api.knob_within_lifetime(T, T) !== true) { ok = false; bad = "exactly T"; }
    check(`L3 within_lifetime T=${T}: flips only at age = T`, ok, bad);
  }
  // Dequeue: expired iff age + m >= T; threshold age = T - m.
  {
    const M = 0.3, thr = T - M;
    let ok = true, bad = "";
    for (const m of ns) {
      const want = m >= 1;
      if (api.knob_dequeue_expired(T, thr * m, M) !== want) { ok = false; bad = `x${m}`; }
    }
    check(`dequeue_expired T=${T}: flips only at age = T - R`, ok, bad);
  }
  // protected: max(now, last + d) - h + o < T.
  {
    // now-dominated: last = h = 0, d = 0.05; the threshold now = T - o.
    const thrNow = T - O;
    let ok = true, bad = "";
    for (const m of ns) {
      const want = !(m >= 1);
      if (api.knob_protected(T, thrNow * m, 0, 0.05, 0, O) !== want) { ok = false; bad = `now x${m}`; }
    }
    // trigger-dominated: now = 0; the threshold d = T - o.
    for (const m of ns) {
      const want = !(m >= 1);
      if (api.knob_protected(T, 0, 0, thrNow * m, 0, O) !== want) { ok = false; bad = `d x${m}`; }
    }
    check(`protected T=${T}: flips only at max(now, last+d) - h + o = T`, ok, bad);
  }
}

// trigger_delay: the branch is on the seq's state, never on a knob.
check("trigger_delay: resent waits the cooldown c",
  api.knob_trigger_delay(true, 0.1, 0.05, 0.2) === 0.1);
check("trigger_delay: never-resent takes min(srtt, p_reach)",
  api.knob_trigger_delay(false, 0.1, 0.05, 0.2) === 0.05 &&
  api.knob_trigger_delay(false, 0.1, 0.5, 0.2) === 0.2);
check("one_way_fwd = srtt - rtprop/2 (RTT/2 with no queue)",
  api.knob_one_way_fwd(0.05, 0.05) === 0.025 &&
  near(api.knob_one_way_fwd(0.08, 0.05), 0.055));
check("ack_timeout = srtt + max(4 rttvar, g) + ack_delay",
  near(api.knob_ack_timeout(0.05, 0.01, 0.001, 0), 0.09) &&
  near(api.knob_ack_timeout(0.05, 0.0001, 0.001, 0.01), 0.061));

// release_at at H = 0.02, +-2 %: linear while the hold term binds, and
// continuous (1-Lipschitz) across the crossing with the floor term.
{
  const H0 = 0.02, tDet = 1.0;
  const rel = (H, tFloor, Hf) => api.knob_release_at(tDet, H, tFloor, Hf);
  // hold binds (floor term far away): release = t_detect + H exactly linear
  const a = rel(H0 * 0.98, 5, 0.01), b = rel(H0, 5, 0.01), c = rel(H0 * 1.02, 5, 0.01);
  check("release_at: linear in H while the hold term binds",
    near(b - a, H0 * 0.02) && near(c - b, H0 * 0.02) && near(b, tDet + H0));
  // the crossing sits AT H = 0.02 (t_floor + H_f = 1.02)
  const x = [0.98, 1, 1.02].map(m => rel(H0 * m, 1.01, 0.01));
  check("release_at: continuous across the hold/floor crossing at H = 0.02",
    near(x[1], 1.02) && Math.abs(x[2] - x[1]) <= 0.02 * H0 + 1e-12 &&
    Math.abs(x[1] - x[0]) <= 0.02 * H0 + 1e-12 && x[2] >= x[1] && x[1] >= x[0],
    x.map(v => v.toFixed(6)).join(" "));
  // floor binds: independent of H
  check("release_at: H-independent while the floor term binds",
    rel(H0 * 0.98, 1.001, 0.005) === rel(H0 * 1.02, 1.001, 0.005));
}

// Infinity cases (IEEE; the TESTS may compare against Infinity, the page
// script may not).
check("inf: intake_expired(inf, 1e9) is false", api.knob_intake_expired(INF, 1e9) === false);
check("inf: resend_admissible(inf, 1e9, 1e9) is true", api.knob_resend_admissible(INF, 1e9, 1e9) === true);
check("inf: within_lifetime(inf, 1e9) is true", api.knob_within_lifetime(INF, 1e9) === true);
check("inf: dequeue_expired(inf, 1e9, 1e9) is false", api.knob_dequeue_expired(INF, 1e9, 1e9) === false);
check("inf: protected(inf, ...) is true", api.knob_protected(INF, 1e9, 1e9, 1e9, 0, 1e9) === true);
check("inf: release_at(t, inf, inf, 0) is inf", api.knob_release_at(1, INF, INF, 0) === INF);
check("inf: release_at(t, inf, floor, Hf) = floor + Hf (the floor term releases)",
  api.knob_release_at(1, INF, 1.5, 0.01) === 1.5 + 0.01);
check("inf: release_at(t, 0, inf, inf) = t (H = 0 releases at detection)",
  api.knob_release_at(1, 0, INF, INF) === 1);

// ---- 2. identity at the inert knobs -------------------------------------
const CELL1 = { eps: [0.05], q: [0.5], rtt: [50], cap: [4] };
const CELL2 = { eps: [0.026, 0.048], q: [0.5, 0.5], rtt: [10, 40], cap: [5, 1] };
const u32 = a => new Uint32Array(a);
const f64 = a => new Float64Array(a);

function fingerprint(s) {
  return [s.get_tick(), s.get_total_src(), s.get_total_fec(), s.get_total_arq(),
          s.get_total_lost(), s.get_cum_decoded(), s.get_cum_sent(), s.get_cum_arrived()];
}
function runToEnd(s, cap = 20000) {
  let n = 0;
  while (!s.is_finished() && n < cap) { s.step(); n++; }
  return s.is_finished();
}
function mainRun(cell, hint) {
  return api.Simulation.multipath(f64(cell.eps), f64(cell.q), u32(cell.rtt), u32(cell.cap),
    64, hint, undefined, undefined, undefined);
}
function inertRun(cell, hint) {
  const n = cell.eps.length;
  return api.Simulation.multipath_knobs(f64(cell.eps), f64(cell.q), u32(cell.rtt), u32(cell.cap),
    64, hint, undefined, undefined, undefined,
    INF, INF, INF, 2000, f64(Array(n).fill(INF)), f64(Array(n).fill(0)));
}
for (const [cname, cell] of [["1 path", CELL1], ["2 paths", CELL2]]) {
  for (const hint of ["bulk", "auto", "realtime"]) {
    const a = mainRun(cell, hint), b = inertRun(cell, hint);
    const fa = runToEnd(a), fb = runToEnd(b);
    const A = fingerprint(a), B = fingerprint(b);
    check(`identity: multipath_knobs(inert) == multipath, ${cname}, ${hint}`,
      fa && fb && A.every((v, i) => v === B[i]),
      `ticks ${A[0]}/${B[0]}, src ${A[1]}, fec ${A[2]}, arq ${A[3]}`);
  }
}
{
  // main's page call: new SimWrapper(params) goes through MAIN_KNOBS.
  const params = { paths: [{ eps: 0.05, q: 0.5, rttMs: 50, cap: 4 }], W: 64, hint: "auto",
                   fixedR: undefined, customDelta: undefined, customRho: undefined };
  const w = new api.SimWrapper(params);
  while (!w.finished) w.step();
  const ref = mainRun(CELL1, "auto"); runToEnd(ref);
  const A = fingerprint(w.inner), B = fingerprint(ref);
  check("identity: new SimWrapper(params) (main's call) == multipath",
    A.every((v, i) => v === B[i]) && w.knobs === api.MAIN_KNOBS,
    `ticks ${A[0]}`);
  check("main's knobs are the inert constant",
    w.lifetimeS === INF && w.holdS === INF && w.sourceRate === INF);
  const k = { lifetimeS: 0.4, holdS: 0.02, sourceRate: 1900, numSource: 2000,
              outageStartS: 0.3, outageLenS: 0.2 };
  const w2 = api.SimWrapper.withKnobs({ ...params, hint: "continuum", customDelta: 50, customRho: null }, k);
  check("withKnobs: knobs reach the wasm constructor",
    w2.lifetimeS === 0.4 && w2.holdS === 0.02 && w2.sourceRate === 1900 && w2.inner.get_rho() === 1);
}

// The single-path baseline memo is keyed on the knobs too (2 paths run the
// hidden baselines; a key that ignored T would hand a T = 2 run the T = 0.4
// baseline).
{
  const base = { paths: [{ eps: 0.026, q: 0.5, rttMs: 10, cap: 5 }, { eps: 0.048, q: 0.5, rttMs: 40, cap: 1 }],
                 W: 64, hint: "continuum", customDelta: 50, customRho: null };
  const knobs = T => ({ lifetimeS: T, holdS: 0, sourceRate: INF, numSource: 2000, outageStartS: INF, outageLenS: 0 });
  const ready = w => new Promise((res, rej) => {
    if (w.baselineReady) return res();
    const t = setTimeout(() => rej(new Error("baseline timeout")), 60000);
    w.onBaselineReady = () => { clearTimeout(t); res(); };
  });
  const n0 = api.BASELINE_CACHE.size;
  const w1 = api.SimWrapper.withKnobs(base, knobs(0.4));
  const w2 = api.SimWrapper.withKnobs(base, knobs(2));
  await ready(w1); await ready(w2);
  const n1 = api.BASELINE_CACHE.size;
  const w3 = api.SimWrapper.withKnobs(base, knobs(0.4));
  check("baseline memo: runs differing only in T get two keys", n1 - n0 === 2, `${n1 - n0} new keys`);
  check("baseline memo: identical knobs hit the cache", w3.baselineReady && api.BASELINE_CACHE.size === n1 &&
    w3.bestSingleMeasured === w1.bestSingleMeasured && w1.bestSingleMeasured > 0,
    `baseline ${w1.bestSingleMeasured.toFixed(3)} sym/tick`);
}

// ---- 3. continuity in T on one stream cell ------------------------------
// 1 path (eps 0.05, q 0.5, rtt 50, cap 4), W = 64, continuum delta = 50,
// lambda = 0.5 C with C = 4 * 0.95 * 1000, 2000 units. RTT 50 ms puts the
// resend arrival ages t_k = k * 50 ms + 25 ms + 1 tick (k = 7: 0.376 s,
// k = 8: 0.426 s) at least 24 ms from 0.4 s, so T = 0.4 sits clear of a
// physical step (plan 5.4 item 5, cell placement).
const C = 4 * (1 - 0.05) * 1000;
function streamShares(T, outageStart, outageLen) {
  const s = api.Simulation.multipath_knobs(f64([0.05]), f64([0.5]), u32([50]), u32([4]),
    64, "continuum", undefined, 50, null,
    T, 0, 0.5 * C, 2000, f64([outageStart]), f64([outageLen]));
  if (!runToEnd(s)) throw new Error(`stream cell T=${T} did not finish`);
  return { t: s.get_share_t(), any: s.get_share_any(), l1: s.get_l1_viol(), cv: s.get_commit_viol() };
}
const SIDE = [0.90, 0.95, 1.05, 1.10], GRID = [0.98, 0.99, 1.0, 1.01, 1.02];
function bound(T0, outageStart, outageLen, label) {
  const at = m => streamShares(T0 * m, outageStart, outageLen);
  const side = SIDE.map(at), grid = GRID.map(at);
  let allOk = true; const lines = [];
  for (const key of ["t", "any"]) {
    const sd = side.map(r => r[key]), g = grid.map(r => r[key]);
    // tolerance per 1 % step: 1.5 * max side slope * 0.01 T + one unit's quantum
    const sl = Math.max(Math.abs(sd[1] - sd[0]), Math.abs(sd[3] - sd[2])) / (0.05 * T0);
    const tol = 1.5 * sl * 0.01 * T0 + 1 / 2000;
    // the same check for the real series and the planted one: every
    // neighbouring pair within tol, and 0.4 -> 0.4 * 1.02 (two 1 % steps)
    // within 2 tol
    const within = series =>
      [1, 2, 3, 4].every(i => Math.abs(series[i] - series[i - 1]) <= tol) &&
      Math.abs(series[4] - series[2]) <= 2 * tol;
    const pairs = within(g), span = pairs;
    const planted = g.slice(); planted[2] += 5 * tol;   // positive control
    check(`share_${key} continuous in T at ${T0} s (${label})`, pairs,
      `grid ${g.map(v => v.toFixed(4)).join(" ")}, tol ${tol.toFixed(5)}`);
    check(`share_${key} planted step of 5 tol fails the same bound (${label})`, !within(planted));
    allOk = allOk && pairs && span;
  }
  const viol = [...side, ...grid].reduce((s, r) => s + r.l1 + r.cv, 0);
  check(`l1_viol and commit_viol read 0 across the T grid (${label})`, viol === 0, `${viol}`);
  return allOk;
}
// Without an outage this cell delivers everything inside 0.4 s (no power;
// reported), so the same bound is also run with a 0.5 s outage at 0.3 s,
// where T binds.
bound(0.4, INF, 0, "stream, no outage");
bound(0.4, 0.3, 0.5, "stream, outage 0.3 s + 0.5 s");

// ---- 4. out-of-domain knobs throw ---------------------------------------
function throwsOn(T, H, rate = INF) {
  try {
    api.Simulation.multipath_knobs(f64([0.05]), f64([0.5]), u32([50]), u32([4]),
      64, "continuum", undefined, 50, null, T, H, rate, 2000, f64([INF]), f64([0]));
    return false;
  } catch (e) { return true; }
}
check("constructor throws for T = 0", throwsOn(0, INF));
check("constructor throws for T = NaN", throwsOn(NaN, INF));
check("constructor throws for T < 0", throwsOn(-1, INF));
check("constructor throws for H = -1", throwsOn(INF, -1));
check("constructor throws for H = NaN", throwsOn(INF, NaN));
check("constructor throws for lambda = 0", throwsOn(INF, INF, 0));
check("constructor accepts the ends T = inf, H = 0 and H = inf", !throwsOn(INF, 0) && !throwsOn(INF, INF));

if (failures > 0) {
  console.error(`\n${failures} check(s) FAILED`);
  process.exit(1);
}
console.log("\nAll three-knobs engine checks passed.");
