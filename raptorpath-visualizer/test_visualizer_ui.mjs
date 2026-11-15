// Headless test of the BUILT visualizer's UI LAYER (stub DOM).
//
// test_visualizer.mjs gates the ENGINE block (wasm + SimWrapper); this file
// gates the WIRING above it — the dial → hint → sim routing, readout
// rendering, the span-machine law panel, the ρ contract paths, and UI
// responsiveness. Engine tests and UI tests can each stay green while the
// wire between them is broken (e.g. the Bulk preset not routing to the
// engine's late-is-fine law), so the routing itself is gated here (the
// visualizer-scale instance of MEASUREMENT DISCIPLINE rule 1: prove the
// mechanism under test actually executes). Run by
// build_visualizer.sh after the engine gates; a failure fails the build.
//
// Usage: node raptorpath-visualizer/test_visualizer_ui.mjs

import { readFileSync } from "node:fs";

const html = readFileSync(
  new URL("./interactive-visualizer.html", import.meta.url),
  "utf8"
);

// ---- stub DOM ----------------------------------------------------------
const ids = new Set();
for (const m of html.matchAll(/id="([^"]+)"/g)) ids.add(m[1]);

const attrs = {};
for (const m of html.matchAll(/<(input|select|button|canvas)\b[^>]*id="([^"]+)"[^>]*>/g)) {
  const t = m[0];
  attrs[m[2]] = {
    tag: m[1],
    value: (t.match(/value="([^"]+)"/) || [])[1],
    min: (t.match(/min="([^"]+)"/) || [])[1],
    max: (t.match(/max="([^"]+)"/) || [])[1],
  };
}
for (const m of html.matchAll(/<select\b[^>]*id="([^"]+)"[^>]*>([\s\S]*?)<\/select>/g)) {
  const sel = (m[2].match(/<option value="([^"]*)" selected>/) ||
               m[2].match(/<option value="([^"]*)">/) || [])[1];
  attrs[m[1]] = { tag: "select", value: sel !== undefined ? sel : "" };
}

function ctx2d() {
  const noop = () => {};
  return new Proxy({}, {
    get: (_, k) => (k === "measureText" ? () => ({ width: 10 }) : (typeof k === "string" ? noop : undefined)),
    set: () => true,
  });
}

const listeners = {};
function makeEl(id) {
  const a = attrs[id] || {};
  const el = {
    id,
    value: a.value !== undefined ? a.value : "0",
    min: a.min, max: a.max,
    textContent: "", innerHTML: "",
    style: {},
    classList: { toggle: () => {}, add: () => {}, remove: () => {}, contains: () => false },
    rows: [{}],
    insertRow() {
      const row = { cells: [], insertCell() { const c = { textContent: "" }; row.cells.push(c); return c; } };
      this.rows.push(row);
      return row;
    },
    deleteRow(i) { this.rows.splice(i, 1); },
    appendChild: () => {},
    setAttribute: () => {},
    getAttribute: () => "tip",
    getBoundingClientRect: () => ({ left: 0, top: 0, right: 100, bottom: 20, width: 100, height: 20 }),
    addEventListener(ev, fn) { (listeners[id] = listeners[id] || {})[ev] = fn; },
    getContext: ctx2d,
    clientWidth: 900, clientHeight: 200, width: 900, height: 200,
    options: [], selectedIndex: 0,
  };
  return el;
}
const els = {};
function getEl(id) {
  if (!els[id]) {
    if (!ids.has(id)) throw new Error("getElementById('" + id + "') — id NOT IN HTML");
    els[id] = makeEl(id);
  }
  return els[id];
}

let rafQueue = [];
const documentStub = {
  getElementById: getEl,
  createElement: () => makeEl("_dyn" + Math.random()),
  body: { appendChild: () => {} },
  addEventListener: () => {},
};
const windowStub = {
  addEventListener: () => {},
  devicePixelRatio: 1,
  innerWidth: 1400, innerHeight: 900,
};

// ---- run the whole script block (engine + UI) --------------------------
const m = html.match(/<script>([\s\S]*)<\/script>/);
const fn = new Function(
  "document", "window", "requestAnimationFrame", "cancelAnimationFrame", "performance",
  m[1] +
    "\n;return { resetSim, updateLabels, updateReadouts, drawRstarChart, spanLaw, spanStep, drawSpanViz, updateSpanQuants, WALLS, sim: () => sim, animate, setRunning: v => { running = v; } };"
);
// Stub clock for the frame-budget gate: every read advances `clockStep` ms.
let clockNow = 0, clockStep = 0;
const perfStub = { now: () => (clockNow += clockStep) };
const api = fn(documentStub, windowStub, cb => { rafQueue.push(cb); return rafQueue.length; }, () => {}, perfStub);

let failures = 0;
function check(name, cond, detail = "") {
  if (cond) console.log(`PASS  ${name}${detail ? "  (" + detail + ")" : ""}`);
  else { console.error(`FAIL  ${name}  ${detail}`); failures++; }
}
const sleep = ms => new Promise(r => setTimeout(r, ms));

// ---- checks ------------------------------------------------------------
check("init: sim constructed", api.sim() !== null);

// The δ dial across the continuum: law sane, sim steps at every position.
for (const lg of [-2.30103, -1.5, -0.30103, 0.7, 1.69897]) {
  getEl("sl-dprice").value = String(lg);
  api.updateLabels();
  api.drawRstarChart();
  api.resetSim();
  const s = api.sim();
  for (let i = 0; i < 120; i++) s.step();
  api.updateReadouts();
  const law = api.spanLaw();
  check("δ=10^" + lg.toFixed(2) + ": law sane + sim steps",
    isFinite(law.aStar) && law.aStar >= 1 && law.mStar >= 2 && law.mStar <= 32 &&
    law.delta >= 1 && law.budget >= 0,
    `A*=${law.aStar.toFixed(1)} M*=${law.mStar} Δ=${law.delta} r*=${law.rStar.toFixed(3)} budget=${law.budget}`);
}

// Preset buttons = the protocol hints, wired to the dial.
check("hint preset buttons wired",
  listeners["btn-d-bulk"] && listeners["btn-d-bulk"].click &&
  listeners["btn-d-auto"].click && listeners["btn-d-rt"].click);
listeners["btn-d-rt"].click();
check("Realtime preset sets the dial",
  Math.abs(parseFloat(getEl("sl-dprice").value) - Math.log10(50)) < 1e-6);

// Routing (the gap this file exists for) — the no-mode-switch invariant:
// one hint string at every dial position (presets, between them, and with
// ρ < 1). Any position constructing a different hint is a mode switch.
{
  const positions = [-2.30103, -1.5, -0.30103, 0.7, 1.69897];
  let oneHint = true, seen = "";
  for (const lg of positions) {
    getEl("sl-dprice").value = String(lg);
    api.resetSim();
    if (api.sim().hint !== "continuum") { oneHint = false; seen = api.sim().hint + " @ 10^" + lg; }
  }
  getEl("sl-rhoc").value = "0.95";
  api.resetSim();
  if (api.sim().hint !== "continuum") { oneHint = false; seen = "ρ<1 flips hint"; }
  getEl("sl-rhoc").value = "1.0";
  check("NO-MODE-SWITCH: one hint ('continuum') at every dial position incl. ρ<1",
    oneHint, seen || "5 positions + ρ<1");
}
// The Bulk end must reach late-is-fine THROUGH the UI path: settled
// mid-stream r ~ 0 at ε = 5% (window excludes estimator warm-up).
listeners["btn-d-bulk"].click();
{
  const s = api.sim();
  let maxR = 0;
  for (let i = 0; i < 300; i++) { s.step(); if (i >= 100) maxR = Math.max(maxR, s.rLive); }
  check("Bulk end: settled mid-stream r ~ 0 through the UI path",
    maxR < 1e-3, `settled max r=${maxR}`);
}

// Morph with Auto-derived W*(δ): Realtime = fresh spans inside W*;
// Bulk = spans pinned at the (small, derived) window with a DEEPER M*.
listeners["btn-d-rt"].click();
const lawRt = api.spanLaw();
listeners["btn-d-bulk"].click();
const lawBulk = api.spanLaw();
check("morph: Realtime fresh spans (A* < W*), Bulk pinned at W* with deeper M*",
  lawRt.aStar < lawRt.p.W && lawBulk.aStar >= lawBulk.p.W && lawBulk.mStar > lawRt.mStar,
  `rt A*=${lawRt.aStar.toFixed(0)}/W*=${lawRt.p.W} M*=${lawRt.mStar}; bulk A*=${lawBulk.aStar.toFixed(0)}/W*=${lawBulk.p.W} M*=${lawBulk.mStar}`);
check("morph: ρ = 1 budget = 0 at BOTH ends (RETAIN is the ρ = 1 point), Bulk r* = 0, Realtime r* > 0",
  lawBulk.budget === 0 && lawRt.budget === 0 && lawBulk.rStar === 0 && lawRt.rStar > 0,
  `rt r*=${lawRt.rStar.toFixed(3)}, budgets ${lawBulk.budget}/${lawRt.budget}`);
check("morph: deadline D shrinks toward Realtime",
  lawRt.D < lawBulk.D, `rt D=${lawRt.D.toFixed(0)}ms, bulk D=${lawBulk.D.toFixed(0)}ms`);

// Span cartoon runs at both ends; the stall transients make the shed
// law's decision points reachable (shed within an explicit ρ < 1 budget at
// the realtime end — at ρ = 1 they are rare by design, §5.6).
for (const preset of ["btn-d-bulk", "btn-d-rt"]) {
  listeners[preset].click();
  const law = api.spanLaw();
  for (let i = 0; i < 600; i++) api.spanStep(law, 1.2);
  api.drawSpanViz(law);
  api.updateSpanQuants(law);
}
{
  getEl("sl-eps").value = "0.10";
  getEl("sl-rhoc").value = "0.95";
  listeners["btn-d-rt"].click();
  const law = api.spanLaw();
  check("shed cell (ε 10 %, ρ .95, Realtime): budget = min(1−ρ, R)",
    law.budget === Math.min(1 - 0.95, law.derived) && law.derived < 0.05,
    `budget=${law.budget}, R=${law.derived}`);
  for (let i = 0; i < 4000; i++) api.spanStep(law, 1.2);
  api.updateSpanQuants(law);
  const counters = getEl("span-counters").innerHTML;
  check("shed law reachable: sheds counted at rt-end with ρ < 1 (stall transients)",
    /shed (\d+)/.test(counters) && parseInt(counters.match(/shed (\d+)/)[1]) > 0,
    counters.replace(/<[^>]*>/g, "").slice(0, 100));
}

// ---- The give-up law, option (ii): budget = min(1 − ρ, R), no branch ----
// (docs/derivations/giveup-law-model.md). R = the §5.6 residual over A*;
// ρ = 1 is the retain point (budget 0 at every δ), with no `retain` flag.
{
  getEl("sel-npaths").value = "1";
  listeners["sel-npaths"].change();
  getEl("sl-q").value = "0.5"; getEl("sl-rtt").value = "50";
  const lawAt = (dprice, eps, rho) => {
    getEl("sl-dprice").value = String(Math.log10(dprice));
    getEl("sl-eps").value = String(eps);
    getEl("sl-rhoc").value = String(rho);
    return api.spanLaw();
  };
  const PRESETS = [["bulk", 0.005], ["auto", 0.5], ["realtime", 50]];

  // Test 1: anchors.
  {
    let ok = true, bad = "";
    for (const [name, dp] of PRESETS) for (const eps of [0.025, 0.05, 0.10]) {
      const one = lawAt(dp, eps, 1.0);
      if (one.budget !== 0) { ok = false; bad = `${name} ε=${eps}: budget(ρ=1)=${one.budget}`; }
      for (const rho of [0.9, 0.95, 0.99, 0.999]) {
        const l = lawAt(dp, eps, rho);
        const want = (1 - rho) >= l.derived ? l.derived : (1 - rho);
        if (l.budget !== want) { ok = false; bad = `${name} ε=${eps} ρ=${rho}: ${l.budget} != ${want}`; }
      }
    }
    check("give-up law: anchors (ρ=1 → 0; = R when 1−ρ ≥ R; = 1−ρ when 1−ρ < R)", ok, bad || "3 presets × 3 ε × 5 ρ");
  }

  // Test 2a: monotone non-increasing in ρ, including ρ → 1.
  {
    let ok = true, bad = "";
    for (const [name, dp] of PRESETS) for (const eps of [0.05, 0.10]) {
      let prev = Infinity;
      const rhos = [];
      for (let k = 0; k <= 100; k++) rhos.push(0.9 + 0.001 * k);
      rhos.push(1 - 1e-6, 1 - 1e-9, 1.0);
      rhos.sort((a, b) => a - b);
      for (const rho of rhos) {
        const b = lawAt(dp, eps, rho).budget;
        if (b > prev + 1e-15) { ok = false; bad = `${name} ε=${eps}: rose at ρ=${rho}`; }
        prev = b;
      }
    }
    check("give-up law: budget non-increasing in ρ over [0.9, 1] incl. ρ → 1", ok, bad);
  }

  // Test 2b: continuity at ±2 % nudges in ρ (of the slack 1−ρ) and in δ at
  // every preset; the preset nudges never reach W22's step.
  {
    let ok = true, bad = "";
    for (const [name, dp] of PRESETS) for (const eps of [0.05, 0.10]) for (const rho of [1.0, 0.99, 0.95]) {
      const lo = lawAt(dp / 1.02, eps, rho), hi = lawAt(dp * 1.02, eps, rho);
      const e = lo.p.aggEps;
      if (Math.abs(hi.budget - lo.budget) > 0.1 * e) {
        ok = false; bad = `${name} ε=${eps} ρ=${rho}: δ nudge ${lo.budget} → ${hi.budget}`;
      }
      const rl = lawAt(dp, eps, 1 - (1 - rho) * 0.98).budget;
      const rh = lawAt(dp, eps, 1 - (1 - rho) * 1.02).budget;
      if (Math.abs(rh - rl) > 0.04 * (1 - rho) + 1e-15) {
        ok = false; bad = `${name} ε=${eps} ρ=${rho}: ρ nudge ${rl} → ${rh}`;
      }
    }
    check("give-up law: continuous under ±2 % nudges of ρ and δ at every preset", ok, bad || "3 presets × 2 ε × 3 ρ");
  }

  // Test 2c: dial sweep δ = 0.005·1.02^k up to 50. Every neighbour pair has
  // |Δbudget| ≤ 0.05·ε̂, except the pair that straddles r* = ε/(1−ε) (W22's
  // p_fec_normal step); there the jump is bounded by R's own jump (min is
  // 1-Lipschitz), and only that pair is excluded.
  {
    let ok = true, bad = "", excluded = 0, pairs = 0, worstRel = 0;
    for (const eps of [0.025, 0.05, 0.10]) for (const rho of [1.0, 0.9999, 0.95]) {
      let prev = null;
      for (let k = 0; 0.005 * Math.pow(1.02, k) <= 50; k++) {
        const l = lawAt(0.005 * Math.pow(1.02, k), eps, rho);
        if (prev) {
          pairs++;
          const e = l.p.aggEps, floor = e / (1 - e);
          const straddle = (prev.rStar <= floor) !== (l.rStar <= floor);
          const db = Math.abs(l.budget - prev.budget);
          if (straddle) {
            excluded++;
            if (db > Math.abs(l.derived - prev.derived) + 1e-15) {
              ok = false; bad = `excluded jump ${db} > R's own ${Math.abs(l.derived - prev.derived)}`;
            }
          } else {
            worstRel = Math.max(worstRel, db / e);
            if (db > 0.05 * e) { ok = false; bad = `ε=${eps} ρ=${rho} k=${k}: Δbudget ${db}`; }
          }
        }
        prev = l;
      }
    }
    check("give-up law: δ dial sweep continuous (only the r=ε/(1−ε) pair excluded, bounded by R's jump)",
      ok, bad || `${pairs} pairs, ${excluded} excluded, worst |Δbudget|/ε̂ = ${worstRel.toFixed(4)}`);
  }

  // Test 8: bind-fraction gauge (FORMULA-FIRST: the min is a clamp) —
  // REPORTED, not pinned: the share of the 100 slider positions
  // ρ = 0.900 .. 0.999 where 1−ρ binds against R, per preset and ε.
  {
    const rows = [];
    for (const [name, dp] of PRESETS) for (const eps of [0.05, 0.10]) {
      let rhoBinds = 0;
      for (let k = 0; k < 100; k++) {
        const l = lawAt(dp, eps, 0.9 + 0.001 * k);
        if (1 - l.userRho < l.derived) rhoBinds++;
      }
      rows.push(`${name} ε=${eps}: 1−ρ binds ${rhoBinds}/100, R binds ${100 - rhoBinds}/100`);
    }
    check("give-up law: bind-fraction gauge reported", rows.length === 6, rows.join(" | "));
  }

  // Which side of the min binds is shown (no retain flag, no ρ<1 ternary).
  {
    const l = lawAt(0.5, 0.05, 0.95);
    api.updateSpanQuants(l);
    const t = getEl("q-budget").textContent;
    const l1 = lawAt(0.5, 0.05, 1.0);
    api.updateSpanQuants(l1);
    const t1 = getEl("q-budget").textContent;
    check("budget text names the binding side of min(1−ρ, R)",
      t.includes("R binds") && t1.includes("1−ρ binds") && !/retain/i.test(t + t1),
      `ρ.95: "${t}"; ρ1: "${t1}"`);
  }
  getEl("sl-eps").value = "0.05"; getEl("sl-rhoc").value = "1.0";
  listeners["btn-d-auto"].click();
}

// ρ dial: contract morphs; the sim honors §3.5 T_cut give-up.
getEl("sel-npaths").value = "1";
listeners["sel-npaths"].change();
getEl("sl-eps").value = "0.10"; getEl("sl-q").value = "0.3"; getEl("sl-rtt").value = "80";
listeners["btn-d-bulk"].click();
getEl("sl-rhoc").value = "0.95";
listeners["sl-rhoc"].input();
check("ρ dial: contract line shows the min(1−ρ, R) give-up",
  getEl("delta-derived").innerHTML.includes("give-up min(1−ρ, R)"),
  getEl("delta-derived").innerHTML.replace(/<[^>]*>/g, ""));
// ρ must compose with the Bulk price — touching ρ may not change the law
// (a law change keyed on ρ is a hidden mode switch).
listeners["sl-rhoc"].change(); // resetSim at Bulk preset with ρ = 0.95
{
  const s = api.sim();
  check("Bulk + ρ<1: same one hint, ρ composes (no mode flip)",
    s.hint === "continuum");
  let maxR = 0;
  for (let i = 0; i < 300; i++) { s.step(); if (i >= 100) maxR = Math.max(maxR, s.rLive); }
  check("Bulk + ρ<1: settled mid-stream still pure ARQ through the UI path",
    maxR < 1e-2, `settled max r=${maxR}`);
}
const lawRho = api.spanLaw();
check("ρ dial: span budget = 1−ρ when 1−ρ < R (Bulk, ε 10 %)",
  Math.abs(lawRho.budget - 0.05) < 1e-9 && lawRho.userRho === 0.95,
  `budget=${lawRho.budget}`);
listeners["sl-rhoc"].change(); // resetSim with ρ = 0.95
{
  const s = api.sim();
  for (let i = 0; i < 2500 && !s.finished; i++) s.step();
  api.updateReadouts();
  check("ρ dial: reliability readout visible",
    getEl("rho-readout").style.display === "" &&
    getEl("rho-readout").innerHTML.includes("given up"),
    getEl("rho-readout").innerHTML.replace(/<[^>]*>/g, "").slice(0, 90));
}
getEl("sl-rhoc").value = "1.0";
listeners["sl-rhoc"].input();
check("ρ dial back to 1: ρ = 1 contract restored",
  getEl("delta-derived").innerHTML.includes("ρ = 1"),
  getEl("delta-derived").innerHTML.replace(/<[^>]*>/g, ""));
listeners["sl-rhoc"].change();

// 2-path topology: responsiveness regression gate. The single-path
// baseline runs are full transfers with ~W² decode cost, so they must run
// chunked and cached off the change handler, which itself must return
// promptly.
getEl("sl-eps").value = "0.05"; getEl("sl-q").value = "0.5"; getEl("sl-rtt").value = "50";
listeners["btn-d-auto"].click();
getEl("sel-npaths").value = "2";
listeners["sel-npaths"].change();
{
  const t0 = Date.now();
  getEl("sel-preset").value = "c8";
  listeners["sel-preset"].change();
  const dt = Date.now() - t0;
  check("topology change returns promptly (< 800 ms; was seconds when sync)",
    dt < 800, `${dt}ms`);
  const s = api.sim();
  for (let i = 0; i < 400; i++) s.step();
  api.updateReadouts();
  check("2-path c8: store gauge rendered",
    getEl("store-pct").textContent.includes("/ 1024"), getEl("store-pct").textContent);
  check("2-path c8: per-path clock note rendered",
    getEl("mp-clocks").innerHTML.includes("per-path recovery clocks"));
  check("2-path c8: store note carries both release laws",
    getEl("store-note").innerHTML.includes("SACK-clocked") &&
    getEl("store-note").innerHTML.includes("frontier-clocked"));
  // The async baseline lands and the aggregation readout completes.
  let waited = 0;
  while (!s.baselineReady && waited < 30000) { await sleep(50); waited += 50; }
  api.updateReadouts();
  check("§16.6 baseline lands async; aggregation factor rendered",
    s.baselineReady && getEl("mp-agg").innerHTML.includes("aggregation factor"),
    `after ~${waited}ms`);
  // The memo: an immediate topology re-flip must hit the cache.
  getEl("sel-preset").value = "c7";
  listeners["sel-preset"].change();
  getEl("sel-preset").value = "c8";
  const t1 = Date.now();
  listeners["sel-preset"].change();
  let w2 = 0;
  while (!api.sim().baselineReady && w2 < 5000) { await sleep(10); w2 += 10; }
  check("baseline memo: repeated topology flip resolves from cache (< 1 s)",
    api.sim().baselineReady && (Date.now() - t1) < 1000, `${Date.now() - t1}ms`);
}


// Frame budget: animate() stops stepping once ~12 ms is spent (the last
// decile's steps cost 50+ ms each), but always makes progress, and does the
// full speed's worth of steps when the clock says steps are cheap.
{
  getEl("sel-speed").value = "25";
  api.resetSim();
  const s0 = api.sim();
  const tick = () => s0.tick;
  const run = (step) => {
    clockStep = step;
    const t = tick();
    api.setRunning(true);
    api.animate();
    api.setRunning(false);
    return tick() - t;
  };
  const cheap = run(0.001);
  const slow = run(5);
  const glacial = run(100);
  clockStep = 0;
  check("animate: cheap steps -> full speed steps (25)", cheap >= 20, `${cheap} ticks`);
  check("animate: 5 ms/step -> budget stops the frame early (<= 4 steps)",
    slow >= 1 && slow <= 4, `${slow} ticks`);
  check("animate: step slower than the budget -> still exactly 1 step", glacial === 1, `${glacial}`);
  rafQueue = [];
}

// Wall chain intact.
check("wall chain: 9 walls", api.WALLS.length === 9);
for (let i = 0; i < 9; i++) listeners["wall-next"].click();
check("wall chain wraps", getEl("wall-idx").textContent === "1 / 9");

// Drain pending animation frames without error.
for (let i = 0; i < 8 && rafQueue.length; i++) {
  const q = rafQueue; rafQueue = [];
  for (const cb of q) cb(i * 16);
}
check("rAF frames drain without error", true);

if (failures > 0) {
  console.error(`\n${failures} UI check(s) FAILED`);
  process.exit(1);
}
console.log("\nAll visualizer UI checks passed.");
