// Headless test of the BUILT three-knobs page's UI LAYER (stub DOM).
//
// test_knobs.mjs gates the ENGINE block; this file gates the WIRING above
// it, after test_visualizer_ui.mjs: the dial -> constructor routing, the
// named-point buttons, the readouts, the cartoon's floor pass and the
// page-script deny check. Plan: docs/derivations/visualizer-three-knobs-plan.md
// section 5.4 item 11. Run by build_visualizer.sh; a failure fails the build.
//
// Usage: node raptorpath-visualizer/test_knobs_ui.mjs

import { readFileSync } from "node:fs";

const html = readFileSync(
  new URL("./knobs-visualizer.html", import.meta.url),
  "utf8"
);

// ---- stub DOM (as test_visualizer_ui.mjs) -------------------------------
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
  return {
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
  resizeFns: [],
  addEventListener(ev, fn) { if (ev === "resize") this.resizeFns.push(fn); },
  devicePixelRatio: 1,
  innerWidth: 1400, innerHeight: 900,
};

// ---- run the whole script block (engine + UI) ----------------------------
const m = html.match(/<script>([\s\S]*)<\/script>/);
const fn = new Function(
  "document", "window", "requestAnimationFrame", "cancelAnimationFrame", "performance",
  m[1] +
    "\n;return { resetSim, updateLabels, updateReadouts, getParams, spanLaw, spanStep, drawSpanViz, updateSpanQuants, spanViz, KN_ROWS, drawUnitStrip, COLORS, sim: () => sim, pinned: () => pinned, drawDelayChart, CLASS_COLORS, drawBudgetBars, drawQueueStrip };"
);
const perfStub = { now: () => 0 };
const api = fn(documentStub, windowStub, cb => { rafQueue.push(cb); return rafQueue.length; }, () => {}, perfStub);

let failures = 0;
function check(name, cond, detail = "") {
  if (cond) console.log(`PASS  ${name}${detail ? "  (" + detail + ")" : ""}`);
  else { console.error(`FAIL  ${name}  ${detail}`); failures++; }
}

// ---- independent slider -> value maps (the plan's section 4.1; written
// out here so a wrong map in the page cannot pass by agreeing with itself) --
const mapT = u => 0.4 * (u / (1 - u)) ** 2;      // u = 1: 1/0 = Infinity
const mapH = u => 0.02 * (u / (1 - u)) ** 2;     // u = 0: 0; u = 1: Infinity
const mapLoad = u => u / (1 - u);                // u = 1: Infinity
const C = 4 * (1 - 0.05) * 1000;                 // default path: cap 4, eps 5 %
// exact at the ends (Infinity, 0); the page's x*x and the test's ** differ in the last bit
const relNear = (a, b, tol = 1e-9) => a === b || Math.abs(a - b) <= tol * Math.abs(b);
const eq = (a, b) => relNear(a, b, 1e-12);
const near = (a, b) => Math.abs(a - b) <= 1e-12;
const set = (id, v) => { getEl(id).value = String(v); };
const get = id => parseFloat(getEl(id).value);
const SLIDERS = ["sl-t", "sl-h", "sl-dprice", "sl-load", "sl-outstart", "sl-outlen",
                 "sl-eps", "sl-q", "sl-rtt", "sl-cap", "sl-w", "sl-budget"];
const snap = () => Object.fromEntries(SLIDERS.map(id => [id, getEl(id).value]));

// ---- 1. init ---------------------------------------------------------------
check("init: sim constructed", api.sim() !== null);
check("init: law revision printed in the footer",
  getEl("kn-rev").textContent === api.sim().knobLawRevision && getEl("kn-rev").textContent.length > 1,
  getEl("kn-rev").textContent);
check("init: the page says it is a proposal", /PROPOSAL/.test(html) && /<title>[^<]*PROPOSAL/.test(html));

// ---- 2. routing: one hint, rho = null, the knobs reach the model ------------
function routeAt(label, expectT, expectH, expectLoad) {
  let err = "";
  try {
    api.resetSim();
    const s = api.sim();
    for (let i = 0; i < 100; i++) s.step();
    api.updateReadouts();
    api.updateLabels();
    const p = api.getParams();
    const okHint = s.hint === "continuum" && p.hint === "continuum";
    const okRho = p.customRho === null && s.inner.get_rho() === 1;
    const okT = eq(s.inner.get_lifetime_s(), expectT);
    const okH = eq(s.inner.get_hold_s(), expectH);
    const okL = eq(s.inner.get_source_rate(), expectLoad * C);
    if (!(okHint && okRho && okT && okH && okL))
      err = `hint ${s.hint} rho ${p.customRho}/${s.inner.get_rho()} T ${s.inner.get_lifetime_s()} (want ${expectT}) H ${s.inner.get_hold_s()} (want ${expectH}) lambda ${s.inner.get_source_rate()} (want ${expectLoad * C})`;
  } catch (e) { err = "THROWS " + e; }
  check(label, err === "", err);
}
const dflt = snap();
const reset = () => { for (const [k, v] of Object.entries(dflt)) set(k, v); };
{
  // every position of each dial (both ends included), the others at the default
  for (const u of [0.1, 0.2, 0.5, 0.6910, 0.9, 0.99, 0.999, 1]) {
    reset(); set("sl-t", u);
    routeAt(`T slider u=${u}: one hint, rho null, lifetime routed`, mapT(u), mapH(get("sl-h")), mapLoad(get("sl-load")));
  }
  for (const u of [0, 0.01, 0.1, 0.5, 0.9, 0.99, 1]) {
    reset(); set("sl-h", u);
    routeAt(`H slider u=${u}: one hint, rho null, hold routed`, mapT(get("sl-t")), mapH(u), mapLoad(get("sl-load")));
  }
  for (const lg of [-2.30103, -1.5, -0.30103, 0.7, 1.69897]) {
    reset(); set("sl-dprice", lg);
    routeAt(`delta slider 10^${lg}: one hint, rho null`, mapT(get("sl-t")), mapH(get("sl-h")), mapLoad(get("sl-load")));
  }
  for (const u of [0.05, 0.1, 1 / 3, 0.545, 0.9, 0.99, 1]) {
    reset(); set("sl-load", u);
    routeAt(`load slider u=${u}: one hint, rho null, source rate routed`, mapT(get("sl-t")), mapH(get("sl-h")), mapLoad(u));
  }
  // the dials together (a mode switch would show in a corner)
  let corners = 0, bad = "";
  for (const tu of [0.1, 0.5, 1]) for (const hu of [0, 0.5, 1]) for (const lu of [0.05, 1 / 3, 1]) {
    reset(); set("sl-t", tu); set("sl-h", hu); set("sl-load", lu);
    api.resetSim();
    const s = api.sim();
    for (let i = 0; i < 80; i++) s.step();
    api.updateReadouts();
    corners++;
    if (s.hint !== "continuum" || api.getParams().customRho !== null ||
        !eq(s.inner.get_lifetime_s(), mapT(tu)) || !eq(s.inner.get_hold_s(), mapH(hu)) ||
        !eq(s.inner.get_source_rate(), mapLoad(lu) * C)) bad = `T u=${tu} H u=${hu} load u=${lu}`;
  }
  check("routing at the 27 corners of the T x H x load dials", bad === "", bad || `${corners} corners`);
  // an outage reaches the model: a hold of 0.2 s at 0.3 s kills sends in it
  reset(); set("sl-outstart", 0.3); set("sl-outlen", 0.5); set("sl-load", 1 / 3);
  api.resetSim();
  check("outage sliders route (start/length reach the run)",
    api.getParams().outageStartS === 0.3 && api.getParams().outageLenS === 0.5);
  // the release/input listeners of the new sliders are wired (the sweep above
  // calls resetSim directly, so a broken `change` listener would pass it)
  for (const [id, u, label] of [["sl-t", 0.5, "v-t"], ["sl-h", 0.5, "v-h"], ["sl-load", 0.5, "v-load"],
                                ["sl-outstart", 0.9, "v-outstart"], ["sl-outlen", 0.4, "v-outlen"]]) {
    reset(); api.resetSim();
    const before = api.sim(), text0 = getEl(label).textContent;
    set(id, u);
    listeners[id].input();
    const liveLabel = getEl(label).textContent !== text0;
    check(`${id}: input updates its label live`, liveLabel, `${text0} -> ${getEl(label).textContent}`);
    check(`${id}: input does not rebuild the run`, api.sim() === before);
    listeners[id].change();
    const p = api.getParams(), s2 = api.sim();
    check(`${id}: change rebuilds the run with the new value`,
      s2 !== before && eq(s2.inner.get_lifetime_s(), mapT(get("sl-t"))) &&
      eq(s2.inner.get_hold_s(), mapH(get("sl-h"))) &&
      eq(s2.inner.get_source_rate(), mapLoad(get("sl-load")) * C) &&
      p.outageStartS === get("sl-outstart") && p.outageLenS === get("sl-outlen"));
  }
  reset();
}

// ---- 3. named points write their triple and nothing else ---------------------
{
  const TRIPLES = {
    "btn-p-bulk":       { t: INFINITY(), d: 0.005, h: INFINITY() },
    "btn-p-auto":       { t: INFINITY(), d: 0.5,   h: INFINITY() },
    "btn-p-rt-object":  { t: INFINITY(), d: 50,    h: 0 },
    "btn-p-rt-call":    { t: 0.4,        d: 50,    h: 0 },
    "btn-p-object-api": { t: INFINITY(), d: null,  h: 0 },   // delta kept
    "btn-p-video":      { t: 2,          d: 0.005, h: 0 },
  };
  function INFINITY() { return Infinity; }
  for (const [id, want] of Object.entries(TRIPLES)) {
    check(`${id}: wired`, !!(listeners[id] && listeners[id].click));
    // start from a state that differs from every triple everywhere
    reset();
    set("sl-t", 0.3); set("sl-h", 0.3); set("sl-dprice", 0.3);
    set("sl-load", 0.7); set("sl-outstart", 0.5); set("sl-outlen", 0.4);
    set("sl-eps", 0.1); set("sl-q", 0.3); set("sl-rtt", 80); set("sl-cap", 3); set("sl-w", 100);
    const before = snap();
    listeners[id].click();
    const after = snap();
    const T = mapT(get("sl-t")), H = mapH(get("sl-h")), d = Math.pow(10, get("sl-dprice"));
    const okT = relNear(T, want.t), okH = relNear(H, want.h);
    const okD = want.d === null ? after["sl-dprice"] === before["sl-dprice"] : relNear(d, want.d);
    const written = want.d === null ? ["sl-t", "sl-h"] : ["sl-t", "sl-h", "sl-dprice"];
    const untouched = SLIDERS.filter(s => !written.includes(s)).every(s => after[s] === before[s]);
    check(`${id}: writes (T, delta, H) = (${want.t}, ${want.d === null ? "kept" : want.d}, ${want.h}) and nothing else`,
      okT && okH && okD && untouched,
      `T ${T}, delta ${d}, H ${H}, untouched ${untouched}`);
    const s = api.sim();
    check(`${id}: the run it starts carries the triple`,
      s.inner.get_lifetime_s() === T && s.inner.get_hold_s() === H && s.hint === "continuum");
  }
  // the buttons never read the workload: the same triple at every load
  let same = true;
  for (const lu of [0.05, 1 / 3, 1]) {
    reset(); set("sl-load", lu);
    listeners["btn-p-rt-call"].click();
    if (mapT(get("sl-t")) !== 0.4 || mapH(get("sl-h")) !== 0) same = false;
  }
  check("presets never read the workload (Realtime, call: T = 0.4 at every load)", same);
  // workload buttons write only the load slider
  const WL = { "btn-w-object": Infinity, "btn-w-stream": 0.5, "btn-w-overload": 1.2 };
  for (const [id, want] of Object.entries(WL)) {
    reset(); set("sl-t", 0.3); set("sl-h", 0.3); set("sl-dprice", 0.3);
    const before = snap();
    listeners[id].click();
    const after = snap();
    const only = SLIDERS.filter(s => s !== "sl-load").every(s => after[s] === before[s]);
    check(`${id}: writes lambda/C = ${want} and nothing else`,
      relNear(mapLoad(get("sl-load")), want) && only, `lambda/C ${mapLoad(get("sl-load"))}`);
    check(`${id}: the run carries lambda = ${want} C`, eq(api.sim().inner.get_source_rate(), want * C));
  }
  reset();
}

// ---- 4. readouts: infinity at the ends, text for NaN, pin ----------------------
{
  reset(); set("sl-t", 1); set("sl-h", 1); set("sl-load", 1);
  listeners["sl-t"].input();
  check("T = inf renders as an infinity sign", getEl("v-t").textContent.includes("∞"), getEl("v-t").textContent);
  check("H = inf renders as an infinity sign", getEl("v-h").textContent.includes("∞"), getEl("v-h").textContent);
  check("lambda = inf renders as an infinity sign", getEl("v-load").textContent.includes("∞"), getEl("v-load").textContent);
  set("sl-t", 0.5); set("sl-h", 0); set("sl-load", 1 / 3); listeners["sl-t"].input();
  check("finite ends render as numbers",
    getEl("v-t").textContent.startsWith("0.400") && getEl("v-h").textContent.startsWith("0.000") &&
    getEl("v-load").textContent.startsWith("0.50"),
    `${getEl("v-t").textContent} | ${getEl("v-h").textContent} | ${getEl("v-load").textContent}`);
  const row = id => api.KN_ROWS.find(r => r[0] === id)[1];
  check("delay quantile past the delivered share renders as inf",
    row("dq99")({ delayQuantile: () => Infinity }).includes("∞"), row("dq99")({ delayQuantile: () => Infinity }));
  check("recovery NaN renders 'not within the run'",
    row("recovery")({ recoveryTime: NaN }) === "not within the run", row("recovery")({ recoveryTime: NaN }));
  check("recovery finite renders seconds", row("recovery")({ recoveryTime: 0.25 }) === "0.250 s");
  check("strict stall NaN renders 'none'", row("stall")({ strictStallBeyondOutage: NaN }) === "none");
  check("every readout row has a live and a pinned cell in the HTML",
    api.KN_ROWS.every(([id]) => ids.has("kr-" + id) && ids.has("kp-" + id)), `${api.KN_ROWS.length} rows`);

  // a finite-T stream run with an outage: l1_viol and commit_viol read 0
  reset(); set("sl-t", 0.5); set("sl-h", 0); set("sl-load", 1 / 3);
  set("sl-outstart", 0.3); set("sl-outlen", 0.5);
  api.resetSim();
  const s = api.sim();
  let n = 0;
  while (!s.finished && n < 20000) { s.step(); n++; }
  api.updateReadouts();
  check("run finished (liveness)", s.finished, `${n} ticks`);
  check("l1_viol reads 0", getEl("kr-l1viol").textContent === "0", getEl("kr-l1viol").textContent);
  check("commit_viol reads 0", getEl("kr-commitviol").textContent === "0", getEl("kr-commitviol").textContent);
  check("share readouts render percentages",
    /%$/.test(getEl("kr-share-any").textContent) && /%$/.test(getEl("kr-share-t").textContent),
    `${getEl("kr-share-any").textContent} / ${getEl("kr-share-t").textContent}`);
  check("bind gauges render", /^\d+$/.test(getEl("kr-rel-hold").textContent) && /^\d+$/.test(getEl("kr-rel-floor").textContent));

  // pin: a display snapshot beside the live column
  listeners["btn-pin"].click();
  const pinnedNow = api.KN_ROWS.every(([id]) => getEl("kp-" + id).textContent === getEl("kr-" + id).textContent);
  check("pin: the pinned column equals the live one at the moment of pinning", pinnedNow);
  set("sl-t", 1); set("sl-load", 1); api.resetSim();
  for (let i = 0; i < 60; i++) api.sim().step();
  api.updateReadouts();
  check("pin: the live column moves on, the pinned one stays",
    getEl("kp-units").textContent === `${s.unitsOut} / 2000` && getEl("kr-units").textContent !== getEl("kp-units").textContent,
    `${getEl("kp-units").textContent} vs ${getEl("kr-units").textContent}`);
  check("pin: caption names the pinned run", getEl("kn-pin-caption").textContent.includes("pinned: T = 0.400"),
    getEl("kn-pin-caption").textContent);
  listeners["btn-unpin"].click();
  check("unpin clears the column", getEl("kp-units").textContent === "–" && api.pinned() === null);
  reset();
}

// ---- 4b. delivery by unit: out-of-order has its own colour ------------------
{
  function strip(hu) {
    reset(); set("sl-t", 1); set("sl-h", hu); set("sl-load", 1 / 3); set("sl-eps", 0.1);
    api.resetSim();
    const s = api.sim();
    let n = 0;
    while (!s.finished && n < 20000) { s.step(); n++; }
    const fills = new Set();
    const cv = getEl("unit-canvas"), orig = cv.getContext;
    cv.getContext = () => new Proxy({}, {
      get: (_, k) => (typeof k === "string" ? () => {} : undefined),
      set: (_, k, v) => { if (k === "fillStyle") fills.add(v); return true; },
    });
    api.drawUnitStrip();
    cv.getContext = orig;
    api.updateReadouts();
    return { s, fills };
  }
  const open = strip(0);
  const nOoo = open.s.oooUnits.reduce((a, b) => a + b, 0);
  check("H = 0 stream run has out-of-order units (precondition)", nOoo > 0, `${nOoo} units`);
  check("the out-of-order colour is drawn when oooUnits has a 1", open.fills.has(api.COLORS.ooo));
  check("out-of-order share readout is a percentage", /%$/.test(getEl("kr-ooo-share").textContent) &&
    !getEl("kr-ooo-share").textContent.startsWith("0.0"), getEl("kr-ooo-share").textContent);
  const strict = strip(1);
  check("H = inf: no out-of-order unit, colour not drawn",
    strict.s.oooUnits.every(b => b === 0) && !strict.fills.has(api.COLORS.ooo) &&
    getEl("kr-ooo-share").textContent.startsWith("0.0"), getEl("kr-ooo-share").textContent);
  check("on-time colour is drawn too", open.fills.has(api.COLORS.onTime));
  reset();
}

// ---- 4c. addendum 1 A1: the delay-dot chart draws the model's classes ---------
// Routing witness: the dots drawn per colour equal the model's class counts, and
// while drawing the page calls nothing on the model but take_unit_events.
{
  reset(); set("sl-t", 0.5); set("sl-h", 0); set("sl-load", 1 / 3); set("sl-eps", 0.1);
  set("sl-outstart", 0.3); set("sl-outlen", 0.5);
  api.resetSim();
  const s = api.sim(), real = s.inner;
  const calls = {};
  const counting = new Proxy(real, {
    get(t, k) {
      const v = t[k];
      return typeof v === "function" ? (...a) => { calls[k] = (calls[k] || 0) + 1; return v.apply(t, a); } : v;
    },
  });
  let n = 0, frames = 0;
  while (!s.finished && n < 20000) {
    s.step(); n++;
    if (n % 50 === 0) { s.inner = counting; api.drawDelayChart(); api.drawUnitStrip(); s.inner = real; frames++; }
  }
  const fills = {};
  let style = null;
  const cv = getEl("delay-canvas"), orig = cv.getContext;
  cv.getContext = () => new Proxy({}, {
    get: (_, k) => (k === "fillRect" ? () => { fills[style] = (fills[style] || 0) + 1; }
      : k === "measureText" ? () => ({ width: 10 }) : (typeof k === "string" ? () => {} : undefined)),
    set: (_, k, v) => { if (k === "fillStyle") style = v; return true; },
  });
  s.inner = counting;
  api.drawDelayChart();
  s.inner = real; cv.getContext = orig;
  const counts = Array.from(real.get_class_counts());
  const want = {};
  for (let c = 1; c < 6; c++) want[api.CLASS_COLORS[c]] = (want[api.CLASS_COLORS[c]] || 0) + counts[c];
  for (const k of Object.keys(want)) if (want[k] === 0) delete want[k];
  check("A1 precondition: the run ends with on-time, out-of-order and undelivered units",
    s.finished && counts[1] > 0 && counts[3] > 0 && counts[4] > 0, `classes ${counts} after ${n} ticks`);
  check("A1: drawn dots per colour equal the model's class counts (routing witness)",
    JSON.stringify(Object.entries(fills).sort()) === JSON.stringify(Object.entries(want).sort()),
    `drawn ${JSON.stringify(fills)} model ${JSON.stringify(want)}`);
  const names = Object.keys(calls);
  check("A1: while drawing, the page calls only take_unit_events on the model",
    names.length === 1 && names[0] === "take_unit_events" && calls.take_unit_events > frames,
    JSON.stringify(calls));
  const st = s.unitStore, final = Array.from(real.get_unit_classes());
  check("A1: the page's store (events only) equals the model's final classes",
    final.every((c, u) => st.cls[u] === c), `${final.length} units`);
  reset();
}

// ---- 4d. addendum 1 A5: the resend budget dial, its bars and headline ----------
// and A6 option (b): the forward leg stays at its default (no page control).
{
  const mapBudget = u => 10 * (u / (1 - u)) ** 2;    // u = 1: Infinity
  for (const u of [0, 0.2, 0.5, 0.8, 0.99, 1]) {
    reset(); set("sl-budget", u);
    let err = "";
    try {
      api.resetSim();
      const s = api.sim(), p = api.getParams();
      for (let i = 0; i < 60; i++) s.step();
      api.updateReadouts();
      if (!(relNear(s.inner.get_resend_limit(), mapBudget(u)) && s.hint === "continuum" && p.customRho === null))
        err = `limit ${s.inner.get_resend_limit()} want ${mapBudget(u)}`;
      for (let i = 0; i < s.numPaths; i++)
        if (!(s.inner.get_link_service_s(i) === 0 && s.inner.get_link_queue_s(i) === Infinity &&
              s.inner.get_link_prop_s(i) === 0)) err += ` path ${i} link not at the default`;
    } catch (e) { err = "THROWS " + e; }
    check(`budget slider u=${u}: limit routed, one hint, rho null, default leg`, err === "", err);
  }
  check("budget: the unlimited end renders as an infinity sign",
    (reset(), set("sl-budget", 1), listeners["sl-budget"].input(), getEl("v-budget").textContent.includes("∞")),
    getEl("v-budget").textContent);
  reset(); set("sl-t", 0.3); set("sl-h", 0.3); set("sl-load", 0.7);
  const before = snap();
  listeners["btn-budget-engine"].click();
  const after = snap();
  check("engine budget button writes 10 per 5 ms and nothing else",
    SLIDERS.filter(k => k !== "sl-budget").every(k => after[k] === before[k]) &&
    relNear(api.sim().inner.get_resend_limit(), 10),
    `limit ${api.sim().inner.get_resend_limit()}`);
  // a lossy stream with an outage at a small budget: the bars are the model's ring
  reset(); set("sl-budget", 0.25); set("sl-eps", 0.1); set("sl-load", 1 / 3);
  set("sl-outstart", 0.3); set("sl-outlen", 0.4);
  api.resetSim();
  const s = api.sim();
  let n = 0;
  while (!s.finished && n < 20000) { s.step(); n++; }
  const rects = [];
  const cv = getEl("budget-canvas"), orig = cv.getContext;
  cv.getContext = () => new Proxy({}, {
    get: (_, k) => (k === "fillRect" ? (x, y, w, h) => rects.push(h) : (typeof k === "string" ? () => {} : undefined)),
    set: () => true,
  });
  api.drawBudgetBars();
  cv.getContext = orig;
  const ring = Array.from(s.inner.get_budget_used());
  const scale = rects[ring.indexOf(Math.max(...ring))] / Math.max(...ring);
  check("budget bars: one bar per window, heights proportional to the model's ring",
    rects.length === ring.length && ring.length > 10 && ring.every((u, i) => Math.abs(rects[i] - u * scale) < 1e-9),
    `${rects.length} bars, ${ring.length} windows`);
  check("budget: no window exceeds the limit (precondition: the budget binds)",
    ring.every(u => u <= s.inner.get_resend_limit()) && s.inner.get_resend_braked_units() > 0,
    `limit ${s.inner.get_resend_limit().toFixed(3)}, held back ${s.inner.get_resend_braked_units()}`);
  const head = getEl("budget-headline").textContent;
  check("budget headline shows the held-back seqs, not the refusals first",
    head.startsWith("seqs held back: " + s.inner.get_resend_braked_units() + " "), head);
  api.updateReadouts();
  check("budget readouts: held-back seqs and refusals",
    getEl("kr-braked-units").textContent === String(s.inner.get_resend_braked_units()) &&
    getEl("kr-braked-attempts").textContent === String(s.inner.get_resend_braked_attempts()));
  reset();
}

// ---- 4e. addendum 1 A4 under A6 option (b): the default leg draws no queue ----
// The strip draws the model's per-tick samples (teed here from the one drain)
// and nothing else; with the default leg every sample, drop and busy time is 0.
{
  reset(); set("sl-t", 0.5); set("sl-h", 0); set("sl-load", 1 / 3); set("sl-eps", 0.1);
  set("sl-outstart", 0.3); set("sl-outlen", 0.4);
  api.resetSim();
  const s = api.sim(), real = s.inner, teed = [];
  const take = real.take_queue_samples.bind(real);
  real.take_queue_samples = () => { const v = take(); teed.push(...v); return v; };
  const heights = [];
  const cv = getEl("queue-strip"), orig = cv.getContext;
  cv.getContext = () => new Proxy({}, {
    get: (_, k) => (k === "fillRect" ? (x, y, w, h) => heights.push(h) : (typeof k === "string" ? () => {} : undefined)),
    set: () => true,
  });
  let n = 0;
  while (!s.finished && n < 20000) { s.step(); n++; if (n % 40 === 0) api.drawQueueStrip(); }
  heights.length = 0;   // each frame redraws every sample: check the last frame
  api.drawQueueStrip();
  cv.getContext = orig;
  api.updateReadouts();
  const paths = s.numPaths;
  check("A4 default leg: the queue strip drew one sample per tick and path, the model's own",
    teed.length === s.tick * paths && heights.length === teed.length && s.queueRows.flat().length === teed.length,
    `${heights.length} drawn, ${teed.length} sampled, ${s.tick} ticks x ${paths} paths`);
  check("A4 default leg: every drawn queue sample is 0",
    teed.every(v => v === 0) && heights.every(v => v === 0));
  let drops = 0, busy = 0;
  for (let i = 0; i < paths; i++) { drops += real.get_path_qdrop(i); busy += real.get_path_link_use(i); }
  check("A4 default leg: zero drops and zero link use, shown as 0",
    drops === 0 && busy === 0 && getEl("queue-note").textContent.startsWith("queue drops: 0 ") &&
    getEl("kr-qdrops").textContent === Array(paths).fill("0 / 0").join(" · "),
    `${getEl("queue-note").textContent} | ${getEl("kr-qdrops").textContent}`);
  check("A4 readouts: one-way delay per path renders (rtt/2 with the default leg)",
    getEl("kr-owd").textContent === "25.0 / 25.0 / 25.0 ms", getEl("kr-owd").textContent);
  delete real.take_queue_samples;
  reset();
}

// ---- 5. the cartoon: the floor pass through the wasm export -------------------
{
  function cartoon(u) {
    reset(); set("sl-t", u); set("sl-dprice", 1.69897);
    api.spanViz.holes = []; api.spanViz.fx = []; api.spanViz.passed = 0; api.spanViz.t = 0;
    api.spanViz.sent = 0; api.spanViz.nextStall = 500; api.spanViz.stallUntil = 0;
    const law = api.spanLaw();
    for (let i = 0; i < 6000; i++) api.spanStep(law, 1.2);
    api.drawSpanViz(law);
    api.updateSpanQuants(law);
    return law;
  }
  const inf = cartoon(1);
  check("cartoon at T = inf: nothing is ever passed by the floor", api.spanViz.passed === 0, `passed ${api.spanViz.passed}`);
  check("cartoon at T = inf: the T - o marker reads infinity",
    inf.tMarkS === Infinity && getEl("q-tmarker").textContent.includes("∞"), getEl("q-tmarker").textContent);
  // a units bug (ms handed to the wasm law as s) keeps both ends above and
  // still passes them: the middle must read "no hole passed, some delivered"
  const mid = cartoon(0.5);
  check("cartoon at T = 0.4 s, RTT 50 ms: no hole is passed, holes are delivered",
    api.spanViz.passed === 0 && api.spanViz.delivered > 0 && near(mid.tMarkS, 0.4 - 0.025),
    `passed ${api.spanViz.passed}, delivered ${api.spanViz.delivered}, T-o ${mid.tMarkS}`);
  const small = cartoon(0.1);
  check("cartoon at T ~ 5 ms: holes are passed by the floor and counted",
    api.spanViz.passed > 0 && getEl("span-counters").innerHTML.includes("passed by the floor"),
    `passed ${api.spanViz.passed}`);
  check("cartoon: o-hat = RTT/2 through knob_one_way_fwd", Math.abs(small.oHatS - small.rttS / 2) < 1e-12,
    `o ${small.oHatS} s, rtt ${small.rttS} s`);
  check("cartoon: the marker is a number at a finite T",
    getEl("q-tmarker").textContent.includes("ms") && !getEl("q-tmarker").textContent.includes("∞"),
    getEl("q-tmarker").textContent);
  reset();
}

// ---- 6. page-script deny check -------------------------------------------------
// Over the UI script block (from the UI marker to the end of the script): no
// isFinite and no comparison against Infinity anywhere, and no comparison on a
// line that names a T or H variable. T/H identifiers are listed here; the page
// names every T- or H-derived variable with one of these prefixes.
const T_H_NAME = /\b(lifetime\w*|hold\w*|tDial\w*|hDial\w*|tMark\w*|hMark\w*|slT|slH|T_SCALE|H_SCALE)\b/;
const CMP = /(<=|>=|===|!==|==|!=|<|>)/;
function stripCode(line) {
  return line
    .replace(/'(?:[^'\\]|\\.)*'|"(?:[^"\\]|\\.)*"|`(?:[^`\\]|\\.)*`/g, "''") // strings
    .replace(/\/\/.*$/, "")                                                    // line comment
    .replace(/\/\*.*?\*\//g, "")                                               // block comment
    .replace(/=>/g, "  ");                                                     // arrow
}
// Arguments passed into a knob_* law call are the allowed form (as in the
// crate's knob_deny.rs): drop each balanced knob_name(...) before the check.
function lawFree(line) {
  let out = '', rest = line, at;
  while ((at = rest.search(/\bknob_\w+\s*\(/)) >= 0) {
    out += rest.slice(0, at);
    let i = rest.indexOf('(', at), depth = 0;
    for (; i < rest.length; i++) {
      if (rest[i] === '(') depth++;
      else if (rest[i] === ')' && --depth === 0) { i++; break; }
    }
    out += 'law()';
    rest = rest.slice(i);
  }
  return out + rest;
}
function denyViolations(src) {
  const out = [];
  let visited = 0;
  src.split("\n").forEach((raw, i) => {
    const line = stripCode(raw);
    if (/\bisFinite\b|\bNumber\.isFinite\b/.test(line)) out.push(`${i + 1}: isFinite: ${raw.trim()}`);
    if (/(===?|!==?)\s*-?\s*Infinity\b|\bInfinity\s*(===?|!==?)/.test(line)) out.push(`${i + 1}: compare to Infinity: ${raw.trim()}`);
    // x - x === 0 (or any x - x) is an infinity test under another name.
    if (/\b([A-Za-z_$][\w$]*)\s*-\s*\1\b/.test(line)) out.push(`${i + 1}: x - x infinity idiom: ${raw.trim()}`);
    if (T_H_NAME.test(line)) {
      visited++;
      if (CMP.test(line)) out.push(`${i + 1}: comparison on a T/H variable: ${raw.trim()}`);
      // truthiness (H = 0 is falsy, exactly a preset) and clamps on T or H
      if (/\?|\|\||&&|!\s*[A-Za-z_$]|\bif\s*\(|\bMath\.(min|max)\b/.test(lawFree(line)) && T_H_NAME.test(lawFree(line)))
        out.push(`${i + 1}: truthiness or clamp on a T/H variable: ${raw.trim()}`);
    }
  });
  return { out, visited };
}
{
  // self-test on planted fixtures: every form must be flagged, clean lines not
  const planted = [
    "if (Number.isFinite(x)) go();",
    "if (isFinite(x)) go();",
    "if (x === Infinity) go();",
    "if (x == Infinity) go();",
    "if (x !== Infinity) go();",
    "if (Infinity === x) go();",
    "if (lifetimeS < 1) go();",
    "if (holdS >= 0) go();",
    "if (tDial == 1) go();",
    "if (hDial !== 0) go();",
    "const c = lifetimeS <= x;",
    "const c = x > holdS;",
    "if (lifetimeS != 0) go();",
    "if (slT.value === '1') go();",
    "const delivered = (d - d === 0);",
    "if (holdS) go();",
    "const x = holdS ? a : b;",
    "const x = lifetimeS || 1;",
    "const x = Math.min(lifetimeS, 2);",
    "const x = !holdS;",
  ];
  const clean = [
    "const a = lifetimeS - oHatS;",
    "x.replace('Infinity', '∞');",
    "// if (lifetimeS < 1) in a comment",
    "const f = (lifetimeS) => lifetimeS * 2;",
    "const s = 'lifetimeS < 1';",
    "const t = Infinity;",
    "const n = a < b;",
    "const c = knob_within_lifetime(lifetimeS, d) ? a : b;",
  ];
  const missed = planted.filter(l => denyViolations(l).out.length === 0);
  const falsePos = clean.filter(l => denyViolations(l).out.length > 0);
  check("deny check self-test: every planted form is flagged", missed.length === 0, missed.join(" | "));
  check("deny check self-test: clean lines are not flagged", falsePos.length === 0, falsePos.join(" | "));

  const ui = html.slice(html.indexOf("// UI"), html.lastIndexOf("</script>"));
  const { out, visited } = denyViolations(ui);
  check("deny check visited the page's T/H lines", visited > 15, `${visited} lines name a T or H variable`);
  check("page script: no isFinite, no comparison on Infinity, no comparison on a T or H variable",
    out.length === 0, out.join(" | "));
  // the engine block's own SimWrapper additions: no presence test on knobs
  const eng = html.slice(html.indexOf("// WASM MODULE"), html.indexOf("// UI"));
  const wrapper = eng.slice(eng.indexOf("class SimWrapper"));
  check("SimWrapper: one construction path (no `if (knobs)` selecting a constructor)",
    !/if\s*\(\s*!?\s*knobs\b/.test(wrapper) && /knobs = MAIN_KNOBS/.test(wrapper) &&
    (wrapper.match(/Simulation\.multipath\b/g) || []).length === 0 &&
    (wrapper.match(/Simulation\.multipath_link/g) || []).length === 1);
}

// ---- 6b. A1: the class drawings compare no delivery tick, delay or seq --------
{
  const DT_NAME = /\b(deliver\w*|seq\w*|tick\w*|delay\w*|decod\w*|ooo\w*|late\w*|onTime\w*)/i;
  const drawDeny = src => src.split("\n").map(stripCode)
    .filter(l => CMP.test(l) && DT_NAME.test(l));
  const planted = [
    "if (st.delay[u] > lim) x();",
    "const late = deliveredAt[u] > 0.4;",
    "if (seqA < seqB) y();",
    "const c = tick >= 5 ? a : b;",
    "const o = oooUnits[u] === 1;",
  ];
  const clean = [
    "ctx.fillRect(X(st.h[u]) - 1, CLASS_ON_STRIP[c] ? stripY : Y(st.delay[u]) - 1, 2, 2);",
    "for (let u = 0; u < st.n; u++) {",
    "// if (st.delay[u] > T) in a comment",
  ];
  check("A1 drawing deny self-test: planted forms flagged",
    planted.every(l => drawDeny(l).length === 1), planted.filter(l => drawDeny(l).length !== 1).join(" | "));
  check("A1 drawing deny self-test: clean lines pass",
    clean.every(l => drawDeny(l).length === 0), clean.filter(l => drawDeny(l).length).join(" | "));
  const ui = html.slice(html.indexOf("// UI"), html.lastIndexOf("</script>"));
  const a = ui.indexOf("function drawUnitStrip("), b = ui.indexOf("// --- channel strip");
  const body = ui.slice(a, b);
  check("A1 drawing deny: the class drawings were found", a > 0 && b > a && /function drawDelayChart\(/.test(body));
  const bad = drawDeny(body);
  check("A1 drawing deny: no delivery-tick, delay or seq comparison in the class drawings",
    bad.length === 0, bad.join(" | "));
}

if (failures > 0) {
  console.error(`\n${failures} UI check(s) FAILED`);
  process.exit(1);
}
console.log("\nAll three-knobs UI checks passed.");
