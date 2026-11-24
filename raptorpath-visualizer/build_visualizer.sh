#!/bin/bash
# Build the interactive visualizer (and the three-knobs proposal page) with embedded WASM.
# Produces a single self-contained HTML file that works from file://.
#
# Usage: bash raptorpath-visualizer/build_visualizer.sh

set -euo pipefail
cd "$(dirname "$0")/.."

echo "Building WASM..."
wasm-pack build raptorpath-wasm --target web --out-dir ../raptorpath-visualizer/wasm 2>&1 | tail -3

# ADR-0077: the core crate must stay wasm-clean and pass its deny list.
echo "Checking raptorpath-core on wasm32 + deny list (ADR-0077)..."
cargo check -p raptorpath-core --target wasm32-unknown-unknown
cargo test -p raptorpath-core --test deny_list
echo "Base64-encoding WASM binary..."
WASM_B64=$(base64 -w0 raptorpath-visualizer/wasm/raptorpath_wasm_bg.wasm)
echo "  WASM size: $(wc -c < raptorpath-visualizer/wasm/raptorpath_wasm_bg.wasm) bytes"
echo "  Base64 size: ${#WASM_B64} chars"

echo "Generating inline visualizer..."

TMPFILE=$(mktemp)
cat > "$TMPFILE" <<PYEOF
import re

PAGES = ['interactive-visualizer', 'knobs-visualizer']

# Read glue JS, strip export keywords but keep function names
glue = open('raptorpath-visualizer/wasm/raptorpath_wasm.js', encoding='utf-8').read()
glue = glue.replace('export class ', 'class ')
glue = glue.replace('export function ', 'function ')
glue = re.sub(r'^export \{.*$', '', glue, flags=re.MULTILINE)
glue = re.sub(r'^/\* @ts-self-types.*$', '', glue, flags=re.MULTILINE)
# Remove async functions that use import.meta / multi-line strings
glue = re.sub(r'async function __wbg_load\(.*?^}', '', glue, flags=re.DOTALL|re.MULTILINE)
glue = re.sub(r'async function __wbg_init\(.*?^}', '', glue, flags=re.DOTALL|re.MULTILINE)

wasm_b64 = "${WASM_B64}"

# The init block includes: base64 wasm, glue code, initSync call, and SimWrapper.
# The glue functions (burst_variance_factor, etc.) call the raw 'wasm' object internally.
# The UI code calls these glue functions directly (they are global after inlining).
init_block = f'''// =========================================================================
// WASM MODULE - real Rust math compiled from raptorpath-math (embedded)
// =========================================================================
const WASM_B64 = "{wasm_b64}";
const wasmBytes = Uint8Array.from(atob(WASM_B64), c => c.charCodeAt(0));

// --- Inlined wasm-bindgen glue (functions call raw 'wasm' object internally) ---
{glue}
// --- End inlined glue ---

initSync({{ module: wasmBytes }});
''' + '''
// CamelCase wrappers for three-var solvers (UI uses camelCase, glue uses snake_case)
function solveRFromDeltaRho(eps,q,W,s2,delta,rho) {
  const a = solve_r_from_delta_rho(eps,q,W,s2,delta,rho);
  return {r:a[0], delta:a[1], rho:a[2], tCut:a[3]};
}
function solveDeltaFromRRho(eps,q,W,s2,r,rho) {
  const a = solve_delta_from_r_rho(eps,q,W,s2,r,rho);
  return {r:a[0], delta:a[1], rho:a[2], tCut:a[3]};
}
function solveRhoFromRDelta(eps,q,W,s2,r,delta) {
  const a = solve_rho_from_r_delta(eps,q,W,s2,r,delta);
  return {r:a[0], delta:a[1], rho:a[2], tCut:a[3]};
}

// --- Simulation wrapper ---
// The inner Simulation runs the SAME rate-controller code as the production
// transport (shared raptorpath-math::controller_rate), fed by an honest
// RTT-delayed estimator (BOCD p-hat, GE sigma2-hat). Multipath (§5.5,
// RWM at L0): params.paths = [{eps,q,rttMs,cap}, ...] builds N independent
// GE channels pouring into ONE shared reliable window; a single-entry array
// (or legacy flat params) reproduces the classic single-path sim exactly.
const SMOOTH_WINDOW = 6;

// Single-path baseline memo: the hidden single-path baseline runs are pure
// functions of (path cfgs, hint, W, r/δ/ρ params) — cached so repeated
// Resets and topology flips never recompute them.
const BASELINE_CACHE = new Map();

// The knob values main's page runs at: every knob inert (T = H = lambda =
// infinity, no outage). IEEE arithmetic makes the three-knob laws inert there
// (plan 3.1), so new SimWrapper(params) is main's model bit for bit.
const MAIN_KNOBS = {
  lifetimeS: Infinity, holdS: Infinity, sourceRate: Infinity,
  numSource: 2000, outageStartS: Infinity, outageLenS: 0,
};
// The forward leg and the resend budget main's page runs at (addendum 1 A2,
// A5): no bottleneck (service 0 s), no queue limit, no propagation split, no
// budget. Values, not absent fields: the leg is the send instant there.
const MAIN_LINK = { serviceS: 0, queueS: Infinity, propS: 0, resendLimit: Infinity };

// The model's unit class codes (CLASS_* in raptorpath-wasm, addendum 1 A1):
// 0 pending, 1 on time, 2 late, 3 out of order, 4 undelivered, 5 expired.
const UNIT_CLASS_DELIVERED = [false, true, true, true, false, false];

class SimWrapper {
  // ONE construction path: the second argument defaults to MAIN_KNOBS, and
  // the knobs page passes its own through SimWrapper.withKnobs. Nothing
  // here inspects a knob value.
  constructor(params, knobs = MAIN_KNOBS, link = MAIN_LINK) {
    const fixedR = params.fixedR;
    // Accept both shapes: params.paths array (new) or flat single-path
    // params (compat shim for any old caller).
    const paths = params.paths && params.paths.length
      ? params.paths
      : [{ eps: params.eps, q: params.q, rttMs: params.rttMs, cap: 4 }];
    this.knobs = knobs;
    this.link = link;
    this.inner = SimWrapper._build(paths, params, fixedR, knobs, link);
    this.rateHistory = [];
    // Per-path channel-state traces (the strip shows the selected path).
    this.channelStates = paths.map(() => []);
    // Truth-side aggregates (capacity-weighted — what the shared window
    // rides; equals the single path's values for N = 1).
    const totalCap = paths.reduce((s, c) => s + c.cap, 0);
    this.eps = paths.reduce((s, c) => s + c.cap / totalCap * c.eps, 0);
    this.q   = paths.reduce((s, c) => s + c.cap / totalCap * c.q, 0);
    this.hint = params.hint;
    this.sigma2True = this.inner.get_sigma2();
    this.srcDoneTick = null;  // tick of the last source symbol (tail FEC starts here)
    // Unit classes (addendum 1 A1): each unit's latest (hand-over s, delay s,
    // class code), filled only by pullUnitEvents from the model's events.
    const nu = this.inner.get_num_source();
    this.unitStore = { n: nu, h: new Float64Array(nu), delay: new Float64Array(nu).fill(Infinity),
                       cls: new Uint8Array(nu), hMax: 0, dMax: 0 };
    // Single-path baseline (§5.5), measured-vs-measured: run each path alone through
    // the same engine (same hint, same W, that path's own channel) and take
    // the best completion goodput. The aggregation factor compares the
    // multipath run against this — what a single-path transfer would
    // actually achieve, not the loss-free theoretical ceiling (which no
    // real run reaches: overhead, ramp and drain are paid on both sides).
    //
    // Async, chunked and cached: these are full hidden transfers, one per
    // path, with ~W²-scaling decode cost — run synchronously they would
    // freeze the page for seconds per topology change. They run in ~40 ms
    // slices off the construction path; baselineReady flips when done
    // (the UI shows “measuring…” until then), and results are memoized per
    // config.
    this.bestSingleMeasured = 0;
    this.baselineReady = paths.length <= 1;
    this.onBaselineReady = null;
    this._dead = false;
    if (paths.length > 1) {
      // String(): JSON.stringify would turn Infinity into null.
      const key = JSON.stringify([paths, params.hint, params.W,
                                  fixedR, params.customDelta, params.customRho])
        + '|' + Object.values(knobs).map(String).join(',')
        + '|' + Object.values(link).map(String).join(',');
      const hit = BASELINE_CACHE.get(key);
      if (hit !== undefined) {
        this.bestSingleMeasured = hit;
        this.baselineReady = true;
      } else {
        this._measureBaseline(paths, params, fixedR, key, knobs, link);
      }
    }
  }
  // The one wasm constructor call, for the run and for each baseline path.
  // The outage window applies to every path it builds.
  static _build(paths, params, fixedR, knobs, link) {
    return Simulation.multipath_link(
      paths.map(c => c.eps), paths.map(c => c.q),
      new Uint32Array(paths.map(c => c.rttMs)),
      new Uint32Array(paths.map(c => c.cap)),
      params.W, params.hint, fixedR, params.customDelta, params.customRho,
      knobs.lifetimeS, knobs.holdS, knobs.sourceRate, knobs.numSource,
      new Float64Array(paths.map(() => knobs.outageStartS)),
      new Float64Array(paths.map(() => knobs.outageLenS)),
      new Float64Array(paths.map(() => link.serviceS)),
      new Float64Array(paths.map(() => link.queueS)),
      new Float64Array(paths.map(() => link.propS)),
      link.resendLimit);
  }
  static withKnobs(params, knobs, link = MAIN_LINK) { return new SimWrapper(params, knobs, link); }
  _measureBaseline(paths, params, fixedR, key, knobs, link) {
    const sims = paths.map(c => SimWrapper._build([c], params, fixedR, knobs, link));
    const wrapper = this;
    let idx = 0, guard = 0, best = 0;
    function slice() {
      if (wrapper._dead) { // superseded by a newer Reset: abandon cleanly
        for (const s of sims) if (s.free) s.free();
        return;
      }
      const t0 = Date.now();
      while (idx < sims.length) {
        const s = sims[idx];
        while (!s.is_finished() && guard < 25000) {
          s.step(); guard++;
          if (Date.now() - t0 > 40) { setTimeout(slice, 0); return; }
        }
        const gp = s.get_num_source() / Math.max(s.get_tick(), 1);
        if (gp > best) best = gp;
        if (s.free) s.free();
        idx++; guard = 0;
      }
      BASELINE_CACHE.set(key, best);
      wrapper.bestSingleMeasured = best;
      wrapper.baselineReady = true;
      if (wrapper.onBaselineReady) wrapper.onBaselineReady();
    }
    setTimeout(slice, 0);
  }
  // measured aggregate goodput vs the best MEASURED single-path run (>1 =
  // the §5.5 per-path-affine ceiling is broken in like-for-like terms)
  get aggregationFactorMeasured() {
    return this.bestSingleMeasured > 0 ? this.aggGoodput / this.bestSingleMeasured : 0;
  }
  // --- Retention store + recovery clocks (walls #7/#8/#9; §6.1, §7.1, §6.3) ---
  get poolCap()                { return this.inner.get_pool_cap(); }
  get storeOccupancy()         { return this.inner.get_store_occupancy(); }
  get storeOccupancyFrontier() { return this.inner.get_store_occupancy_frontier(); }
  get poolStalls()             { return this.inner.get_pool_stalls(); }
  get frontier()               { return this.inner.get_frontier(); }
  get phantomHeld()            { return this.inner.get_phantom_held(); }
  get phantomAvoided()         { return this.inner.get_phantom_avoided(); }
  // --- Multipath accessors (§5.5) ---
  get numPaths() { return this.inner.get_num_paths(); }
  pathCapacity(i)   { return this.inner.get_path_capacity(i); }
  pathRttMs(i)      { return this.inner.get_path_rtt_ms(i); }
  pathEpsHat(i)     { return this.inner.get_path_eps_hat(i); }
  pathGoodputEst(i) { return this.inner.get_path_goodput_est(i); }
  pathGoodputTrue(i){ return this.inner.get_path_goodput_true(i); }
  pathSrc(i)  { return this.inner.get_path_src(i); }
  pathFec(i)  { return this.inner.get_path_fec(i); }
  pathArq(i)  { return this.inner.get_path_arq(i); }
  pathLost(i) { return this.inner.get_path_lost(i); }
  // aggregate delivery goodput (decoded/tick) vs the best single path's
  // true goodput — the §5.5 readout (>1 breaks the resequencing ceiling)
  get aggGoodput()        { return this.inner.get_agg_goodput(); }
  get bestSingleGoodput() { return this.inner.get_best_single_goodput(); }
  get aggregationFactor() { return this.inner.get_aggregation_factor(); }
  get sumGoodputTrue() {
    let s = 0;
    for (let i = 0; i < this.numPaths; i++) s += this.pathGoodputTrue(i);
    return s;
  }
  get finished() { return this.inner.is_finished(); }
  // live controller output vs closed-form reference at TRUE channel params
  get rLive()    { return this.inner.get_r_star(); }
  get rStarRef() { return this.inner.get_r_star_auto(); }
  // what the controller sees (estimator state)
  get pUpper()    { return this.inner.get_p_upper(); }
  get estLoss()   { return this.inner.get_estimated_loss(); }
  get sigma2Est() { return this.inner.get_sigma2_est(); }
  get deltaEff()  { return this.inner.get_delta_eff(); }
  get rSat()      { return this.inner.get_r_sat(); }
  // derived window W* (§4.8) from the live estimator state
  get derivedW()  { return this.inner.get_derived_w(); }

  // saturation pressure in [0,1] (§4.4), shown as the continuous
  // saturation-pressure gauge. 0 = cap slack, 0.5 = at r_sat, ->1 = held.
  get satPressure() { return this.inner.get_saturation_pressure(); }
  // custom triangle mode (rho < 1 enables T_cut give-up eviction)
  get givenUp()     { return this.inner.get_given_up(); }
  get reliability() { return this.inner.get_reliability(); }
  get rho()         { return this.inner.get_rho(); }
  // overhead decomposition: floor is the channel's fault, excess is the protocol's.
  // nominal floor = eps/(1-eps); realized floor = lost/arrived over THIS run's
  // actual sends (a finite run samples only a prefix of the calibrated channel).
  get overheadFloor()         { return this.inner.get_overhead_floor(); }
  get overheadFloorRealized() { return this.inner.get_overhead_floor_realized(); }
  get excessOverhead()        { return this.inner.get_excess_overhead(); }
  // --- Three-knob readouts (proposal; inert on main's page) ---
  get lifetimeS()   { return this.inner.get_lifetime_s(); }
  get holdS()       { return this.inner.get_hold_s(); }
  get sourceRate()  { return this.inner.get_source_rate(); }
  get intakeExpired()    { return this.inner.get_intake_expired(); }
  get intakeExpiredDeq() { return this.inner.get_intake_expired_deq(); }
  get unitsOut()    { return this.inner.get_units_out(); }
  get fAnn()        { return this.inner.get_f_ann(); }
  get aKnown()      { return this.inner.get_a_known(); }
  get fRx()         { return this.inner.get_f_rx(); }
  get passedUndelivered() { return this.inner.get_passed_undelivered(); }
  get commitViol()  { return this.inner.get_commit_viol(); }
  get l1Viol()      { return this.inner.get_l1_viol(); }
  get skipBelowFloor() { return this.inner.get_skip_below_floor(); }
  get floors()      { return this.inner.get_floors(); }
  get floorCarried() { return this.inner.get_floor_carried(); }
  get spanCutByT()  { return this.inner.get_span_cut_by_t(); }
  get shareAny()    { return this.inner.get_share_any(); }
  get shareT()      { return this.inner.get_share_t(); }
  get shareByRef()  { return this.inner.get_share_by_ref(); }
  delayQuantile(p)  { return this.inner.get_delay_quantile(p); }
  get ooo()         { return this.inner.get_ooo(); }
  get oooShare()    { return this.inner.get_ooo_share(); }
  get oooUnits()    { return this.inner.get_ooo_units(); }
  get unitDelays()  { return this.inner.get_unit_delays(); }
  get deliveredUnits() { return this.inner.get_delivered_units(); }
  // The ONE drain of the model's class events (addendum 1 A1): append them to
  // the store and return it; a superseding event overwrites its unit. dMax
  // spans the delivered classes only (an undelivered unit has no delay).
  pullUnitEvents() {
    const ev = this.inner.take_unit_events(), st = this.unitStore;
    for (let i = 0; i < ev.length; i += 4) {
      const u = ev[i], c = ev[i + 3];
      st.h[u] = ev[i + 1]; st.delay[u] = ev[i + 2]; st.cls[u] = c;
      st.hMax = Math.max(st.hMax, ev[i + 1]);
      if (UNIT_CLASS_DELIVERED[c]) st.dMax = Math.max(st.dMax, ev[i + 2]);
    }
    return st;
  }
  get unitClasses() { return this.inner.get_unit_classes(); }
  get classCounts() { return this.inner.get_class_counts(); }
  get lateShare()   { return this.inner.get_late_share(); }
  get oooLate()     { return this.inner.get_ooo_late(); }
  get lateFills()   { return this.inner.get_late_fills(); }
  // --- The forward leg (addendum 1 A2): routing witnesses and gauges ---
  linkServiceS(i) { return this.inner.get_link_service_s(i); }
  linkQueueS(i)   { return this.inner.get_link_queue_s(i); }
  linkPropS(i)    { return this.inner.get_link_prop_s(i); }
  pathQdrop(i)    { return this.inner.get_path_qdrop(i); }
  get resendLimit() { return this.inner.get_resend_limit(); }
  // --- The resend budget (addendum 1 A5) ---
  get budgetUsed()  { return this.inner.get_budget_used(); }
  get resendBrakedUnits()    { return this.inner.get_resend_braked_units(); }
  get resendBrakedAttempts() { return this.inner.get_resend_braked_attempts(); }
  get dup()         { return this.inner.get_dup(); }
  get inFlight()    { return this.inner.get_in_flight(); }
  get knowledgeLeadMeanMs() { return this.inner.get_knowledge_lead_mean_ms(); }
  get knowledgeLeadMaxMs()  { return this.inner.get_knowledge_lead_max_ms(); }
  get relByHold()   { return this.inner.get_rel_by_hold(); }
  get relByFloor()  { return this.inner.get_rel_by_floor(); }
  get releasedHoles() { return this.inner.get_released_holes(); }
  get recoveryTime() { return this.inner.get_recovery_time(); }
  get strictStallBeyondOutage() { return this.inner.get_strict_stall_beyond_outage(); }
  get knobLawRevision() { return this.inner.get_knob_law_revision(); }
  get totalSrc() { return this.inner.get_total_src(); }
  get totalFec() { return this.inner.get_total_fec(); }
  get totalArq() { return this.inner.get_total_arq(); }
  get totalLost() { return this.inner.get_total_lost(); }
  get cumDecoded() { return this.inner.get_cum_decoded(); }
  get tick() { return this.inner.get_tick(); }
  // delivery latency (ms): send -> decode + one-way propagation; given-up excluded
  get latLast() { return this.inner.get_lat_last(); }
  get latAvg()  { return this.inner.get_lat_avg(); }
  get latP50()  { return this.inner.get_lat_percentile(0.5); }
  get latP99()  { return this.inner.get_lat_percentile(0.99); }
  get jitter()  { return this.inner.get_jitter(); }
  retxBufSize() { return this.inner.get_retx_buf_size(); }
  step() {
    if (this.finished) return;
    this.inner.step();
    for (let i = 0; i < this.channelStates.length; i++) {
      this.channelStates[i].push(this.inner.path_channel_is_good(i) ? "good" : "bad");
    }
    const lost = this.inner.get_lost();
    const pct = 100 / this.inner.get_num_source();
    this.rateHistory.push({
      src: this.inner.get_src(), fec: this.inner.get_fec(),
      arq: this.inner.get_arq(), lost,
      cumSent: this.inner.get_cum_sent() * pct,
      cumArrived: this.inner.get_cum_arrived() * pct,
      cumDecoded: this.inner.get_cum_decoded() * pct,
    });
    if (this.srcDoneTick === null && this.inner.get_total_src() >= this.inner.get_num_source()) {
      this.srcDoneTick = this.rateHistory.length - 1;
    }
  }
  smoothedRate(idx) {
    const w = SMOOTH_WINDOW;
    const start = Math.max(0, idx - w + 1), end = idx + 1;
    let s=0,f=0,a=0,l=0,n=0;
    for (let i=start; i<end && i<this.rateHistory.length; i++) {
      s+=this.rateHistory[i].src; f+=this.rateHistory[i].fec;
      a+=this.rateHistory[i].arq; l+=this.rateHistory[i].lost; n++;
    }
    const h = this.rateHistory[idx];
    return n>0 ? {src:s/n, fec:f/n, arq:a/n, lost:l/n,
                  cumSent:h.cumSent, cumArrived:h.cumArrived, cumDecoded:h.cumDecoded}
               : {src:0,fec:0,arq:0,lost:0,cumSent:0,cumArrived:0,cumDecoded:0};
  }
}

'''

# Both pages get the SAME init block (plan 3.2): main's page script is
# untouched, the knobs page is built from its own source.
for name in PAGES:
    path = 'raptorpath-visualizer/' + name + '.html'
    html = open(path, encoding='utf-8').read()
    # Change module script to regular script
    html = html.replace('<script type="module">', '<script>')
    # Replace everything from "WASM MODULE" to just before "UI" section
    pattern = r'// =+\n// WASM MODULE.*?(?=// =+\n// UI)'
    # A callable replacement: init_block has backslashes that re.sub would
    # otherwise read as escapes.
    html, n = re.subn(pattern, lambda m: init_block, html, flags=re.DOTALL)
    assert n == 1, name + ': engine block markers not found'
    open(path, 'w', encoding='utf-8', newline='\n').write(html)
    print(name + f": HTML size: {len(html)} bytes")
PYEOF

python3 "$TMPFILE"
rm -f "$TMPFILE"

echo "Running engine tests against the built file..."
node raptorpath-visualizer/test_visualizer.mjs
echo "Engine tests passed."

echo "Running UI-layer tests (stub DOM) against the built file..."
node raptorpath-visualizer/test_visualizer_ui.mjs
echo "UI tests passed."

echo "Running knobs-page engine tests (three-knob laws, plan 5.4 item 10)..."
node raptorpath-visualizer/test_knobs.mjs
echo "Knobs engine tests passed."

echo "Running knobs-page UI tests (stub DOM, plan 5.4 item 11)..."
node raptorpath-visualizer/test_knobs_ui.mjs
echo "Knobs UI tests passed."

echo "Done! Visualizers updated: raptorpath-visualizer/interactive-visualizer.html"
echo "  and the three-knobs proposal page raptorpath-visualizer/knobs-visualizer.html"
echo "Open it directly in a browser (file:// works)."
