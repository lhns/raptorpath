// Headless test of the BUILT visualizer's UI LAYER (stub DOM).
//
// test_visualizer.mjs gates the ENGINE block (wasm + SimWrapper); this file
// gates the WIRING above it — the dial → hint → sim routing, readout
// rendering, the span-machine law panel, the ρ contract paths, and UI
// responsiveness. It exists because of a measured coverage hole (2026-07-28):
// the Bulk preset silently stopped routing to the engine's late-is-fine law
// while both endpoints' own tests stayed green — the wire between them was
// untested (the visualizer-scale instance of MEASUREMENT DISCIPLINE rule 1:
// prove the mechanism under test actually executes). Run by
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
  "document", "window", "requestAnimationFrame", "cancelAnimationFrame",
  m[1] +
    "\n;return { resetSim, updateLabels, updateReadouts, drawRstarChart, spanLaw, spanStep, drawSpanViz, updateSpanQuants, WALLS, sim: () => sim };"
);
const api = fn(documentStub, windowStub, cb => { rafQueue.push(cb); return rafQueue.length; }, () => {});

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
    `A*=${law.aStar.toFixed(1)} M*=${law.mStar} Δ=${law.delta} r*=${law.rStar.toFixed(3)} retain=${law.retain}`);
}

// Preset buttons = the protocol hints, wired to the dial.
check("hint preset buttons wired",
  listeners["btn-d-bulk"] && listeners["btn-d-bulk"].click &&
  listeners["btn-d-auto"].click && listeners["btn-d-rt"].click);
listeners["btn-d-rt"].click();
check("Realtime preset sets the dial",
  Math.abs(parseFloat(getEl("sl-dprice").value) - Math.log10(50)) < 1e-6);

// ROUTING (the gap this file exists for) — THE NO-MODE-SWITCH INVARIANT:
// ONE hint string at EVERY dial position (presets, between them, and with
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
check("morph: Bulk end RETAIN + r* = 0, Realtime end r* > 0",
  lawBulk.retain && lawBulk.rStar === 0 && !lawRt.retain && lawRt.rStar > 0,
  `rt r*=${lawRt.rStar.toFixed(3)}`);
check("morph: deadline D shrinks toward Realtime",
  lawRt.D < lawBulk.D, `rt D=${lawRt.D.toFixed(0)}ms, bulk D=${lawBulk.D.toFixed(0)}ms`);

// Span cartoon runs at both ends; the §16.20.8 stall transients make the
// shed law's decision points reachable (shed within an explicit ρ < 1
// budget at the realtime end — at ρ = 1 they are rare BY DESIGN, §16.26).
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
  for (let i = 0; i < 4000; i++) api.spanStep(law, 1.2);
  api.updateSpanQuants(law);
  const counters = getEl("span-counters").innerHTML;
  check("shed law reachable: sheds counted at rt-end with ρ < 1 (stall transients)",
    /shed (\d+)/.test(counters) && parseInt(counters.match(/shed (\d+)/)[1]) > 0,
    counters.replace(/<[^>]*>/g, "").slice(0, 100));
}

// ρ dial: contract morphs; the sim honors §6.1 T_cut give-up.
getEl("sel-npaths").value = "1";
listeners["sel-npaths"].change();
getEl("sl-eps").value = "0.10"; getEl("sl-q").value = "0.3"; getEl("sl-rtt").value = "80";
listeners["btn-d-bulk"].click();
getEl("sl-rhoc").value = "0.95";
listeners["sl-rhoc"].input();
check("ρ dial: contract line shows T_cut give-up",
  getEl("delta-derived").innerHTML.includes("T_cut give-up"),
  getEl("delta-derived").innerHTML.replace(/<[^>]*>/g, ""));
// ρ must COMPOSE with the Bulk price — touching ρ may not change the law
// (the hidden-mode-switch-keyed-on-ρ defect, user-caught 2026-07-28).
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
check("ρ dial: span budget = 1−ρ when the dial is below 1",
  Math.abs(lawRho.budget - 0.05) < 1e-9 && !lawRho.retain && lawRho.userRho === 0.95,
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

// 2-path topology: RESPONSIVENESS regression gate (the 10-second hang:
// the §16.6 baseline runs were synchronous full transfers with ~W²
// decode cost; they are now chunked+cached). The change handler itself
// must return promptly.
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
