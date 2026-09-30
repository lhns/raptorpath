//! The window sender's phase functions: blocks of `run_window_sender`'s loop
//! body as ordinary functions, the `emit_source` pattern. The loop locals a
//! phase owns are the fields of that phase's state struct; the shared handles
//! and the resolve-once policy ride in a borrowed context struct. Each call
//! site sits exactly where its block runs in the loop.

use super::*;
use super::sender_policy::SenderPolicy;

/// The dynamic store-cap refresh's state: every loop local the refresh block
/// wrote. Other loop phases READ `dyn_store_cap` (the flow-control gate) and
/// the DIAG report reads the pool-anchor and K_i gauges.
pub(crate) struct StoreCapState {
    /// Pool-anchor law state (`RWM_POOL_ANCHOR`, DIAG): whether the N ≥ 2
    /// honest send-anchor pool computed the cap at the last refresh, and its
    /// Σ before clamping (the cap gauge decides; the btlbw gauge may stay
    /// inflated since the cwnd feed is untouched).
    pub pa_engaged: bool,
    pub pa_sum: f64,
    /// Three-term law state (`RWM_THREE_TERM`): `Some((window, slack, span))`
    /// at the last refresh where the law engaged, `None` where it did not. The
    /// mechanism gauge (`docs/measurement-discipline.md` rule 15): a battery
    /// reads the three terms separately, and the span term's N = 1 zero is
    /// observable.
    pub tt_terms_diag: Option<(f64, f64, f64)>,
    pub tt_print_us: u64,
    /// path → windowed-min echo-ratio state (K_i), fed at the dyn-cap
    /// refresh cadence; ~10 s window = two 5 s half-buckets
    /// (`PERCAP_K_HALF_WINDOW_US`, shared by every honest-cap consumer).
    pub percap_k: std::collections::HashMap<u32, EchoRatioMin>,
    /// Throttled cache of the dynamic cap (recomputed off the scheduler lock at
    /// most every 5 ms; the pipe/BDP move far slower than the select loop).
    pub dyn_store_cap: usize,
    pub dyn_cap_refresh_us: u64,
    /// `sf=` gauge print cadence. A standalone INFO line, not part of the
    /// [DIAG] assembly: the population it reports is the store-cap phase's own
    /// instrument.
    pub sf_print_us: u64,
}

impl StoreCapState {
    pub(crate) fn new(pol: &SenderPolicy) -> Self {
        Self {
            pa_engaged: false,
            pa_sum: 0.0,
            tt_terms_diag: None,
            tt_print_us: 0,
            percap_k: std::collections::HashMap::new(),
            dyn_store_cap: pol.store_boot_cap.min(pol.store_max),
            dyn_cap_refresh_us: 0,
            sf_print_us: 0,
        }
    }
}

/// What the store-cap refresh borrows from `run_window_sender`.
pub(crate) struct StoreCapCtx<'a> {
    pub pol: &'a SenderPolicy,
    pub gates: &'a crate::gates::RuntimeGates,
    pub scheduler: &'a Arc<parking_lot::Mutex<Scheduler>>,
    pub copa_feed: &'a Option<Arc<CopaFeed>>,
    pub sumcap: &'a mut SumCapGauge,
    pub dcap: &'a mut DeltaCapGauge,
    pub ccap: &'a mut SenderTeardownGauges,
}

/// Plain-reliable delay-based window cap (paper §6), refreshed off the
/// scheduler lock at most every 5 ms.
pub(crate) fn refresh_store_cap(scs: &mut StoreCapState, ctx: StoreCapCtx<'_>) {
    let StoreCapCtx { pol, gates, scheduler, copa_feed, sumcap, dcap, ccap } = ctx;
    if pol.plain_dyn_cap {
        let dnow = now_us();
        if dnow.saturating_sub(scs.dyn_cap_refresh_us) >= 5_000 {
            scs.dyn_cap_refresh_us = dnow;
            // `sf=` readout every ~2 s under RWM_DIAG — the saturation-filter
            // population at the refresh instants: how often the placement
            // filter would have emptied the Σ (the `active_paths()` trap,
            // closed by plan 2b — the Σ now reads `channel_paths`).
            if gates.diag && dnow.saturating_sub(scs.sf_print_us) >= 2_000_000 {
                scs.sf_print_us = dnow;
                let (t, lv, ac, sh, ze) = store_cap_sf_gauge();
                info!(
                    ticks = t,
                    live_sum = lv,
                    active_sum = ac,
                    short_ticks = sh,
                    zero_ticks = ze,
                    "[SF] store-cap saturation filter: active_paths() vs live_paths() at the dyn-cap refresh"
                );
            }
            // Σ-cwnd store law only when the feed owns the operating point
            // (Copa-sole); the sampling-only feed (RWM_PLAIN_RS) keeps the
            // anchor-sum law, fed honest samples.
            if copa_feed.as_ref().is_some_and(|f| f.owns_cc()) {
                // Copa owns the operating point (ADR-0062), so the outstanding
                // window is keyed to Σ cwnd (the probe state), not the BtlBw
                // anchor. With the honest send-interval sampler a 2×anchor cap
                // is circular: samples can never read above the store-capped
                // delivered rate, so the anchor could never grow toward the
                // pipe. cwnd escapes the loop because Copa probes it upward
                // independent of the cap; gain×cwnd keeps ~1 cwnd of recovery
                // runway buffered above the substrate window (quinn enforces
                // cwnd on the wire).
                // live_paths(), not active_paths(): the latter filters by spare
                // capacity (available() > 0), and a cwnd-saturated path — the
                // normal state of a wire-bound sender — would make cwnd_sum read
                // 0, collapsing the cap to the boot value and whiplashing the
                // TUN gate.
                let (cwnd_sum, n_live): (f64, usize) = {
                    let sched = scheduler.lock();
                    let live = sched.live_paths();
                    let cs: f64 = live
                        .iter()
                        .filter_map(|id| sched.path(*id).map(|p| p.cwnd as f64))
                        .sum();
                    (cs, live.len().max(1))
                };
                scs.pa_engaged = false; // Copa-sole owns the store law (Σcwnd)
                scs.dyn_store_cap = if let Some(cap) = pooled_store_cap(
                    pol.store_paths_on,
                    pol.sum_cap,
                    pol.delta_cap,
                    pol.delta_b,
                    n_live,
                    cwnd_sum,
                    pol.store_bdp_gain,
                    pol.store_cap_floor,
                    pol.store_path_pool,
                ) {
                    // `RWM_SUM_CAP` reaches the Copa-sole seat too: the base
                    // here is Σ cwnd rather than Σ anchor — still a sum over
                    // paths, so still already linear in the count. Leaving
                    // this seat on the `×N` would make the gate's meaning
                    // depend on the CC family (ADR-0064).
                    sumcap.record(pol.sum_cap, n_live, cwnd_sum, pol.store_bdp_gain, cap);
                    dcap.record(n_live, cwnd_sum, pol.store_bdp_gain, cap);
                    cap
                } else if cwnd_sum > 0.0 {
                    ((pol.store_bdp_gain * cwnd_sum).ceil() as usize)
                        .clamp(pol.store_cap_floor, pol.store_max)
                } else {
                    pol.store_boot_cap.min(pol.store_max)
                };
            } else {
                // The three-term limit (`RWM_THREE_TERM`): the law's inputs
                // over live paths — the set every honest-cap consumer reads,
                // and the set whose RTprop spread is the span term's
                // argument. There is no path-count predicate here or in the
                // law: at N = 1 the spread is zero and the span term
                // vanishes by arithmetic. Rate source: the per-path
                // delivered-rate anchor (`btlbw_sym_per_s`), the same source
                // the Σ-anchor base reads, so the A/B isolates the law.
                let tt_slots: Vec<Option<ThreeTermPath>> = if pol.three_term_on {
                    let sched = scheduler.lock();
                    sched
                        .live_paths()
                        .iter()
                        .map(|id| {
                            sched.path(*id).map(|p| ThreeTermPath {
                                id: *id,
                                rate: p.btlbw_sym_per_s(),
                                srtt: p.srtt(),
                                rtprop: p.min_rtt(),
                                k_raw: p.k_raw(),
                            })
                        })
                        .collect()
                } else {
                    Vec::new()
                };
                let tt_terms = three_term_terms(&mut scs.percap_k, &tt_slots, dnow);
                // Alongside the Σanchor base, accumulate the honest per-path
                // cap sum Σ anchor_i·(K_i+gain−1) when the honest sampler is
                // live (see `honest_store_cap`; K_i observed here at the
                // refresh cadence). hsum = 0.0 whenever honest_cap_on is
                // false, and the expressions below then run unchanged.
                let (bdp, hsum, n_live): (f64, f64, usize) = {
                    let sched = scheduler.lock();
                    let live = sched.live_paths();
                    let n = live.len().max(1);
                    // The path set is the channel's membership
                    // (`channel_paths` = `live_paths()`), the set `n_live` is
                    // counted from: `active_paths()` (`available() > 0`) is
                    // the placement filter, and a wire-bound sender is
                    // cwnd-saturated by definition, so it would drop the paths
                    // carrying the transfer (Σ = 0 → the boot-cap cliff).
                    // The `[SF]` gauge still counts how often the placement
                    // filter is short of membership at this refresh.
                    let act = sched.active_paths();
                    store_cap_sf_record(live.len(), act.len());
                    // Warm-anchor slots for the honest per-path cap, in
                    // path-set order — collected here, evaluated once by
                    // `honest_cap_terms` below (the law lives there).
                    let (bdp, slots) = store_cap_pool_inputs(&sched, pol.honest_cap_on);
                    let terms =
                        honest_cap_terms(&mut scs.percap_k, &slots, dnow, pol.store_bdp_gain);
                    // hsum = 0.0 whenever honest_cap_on is false, and the
                    // expressions below run unchanged.
                    let hsum: f64 = if pol.honest_cap_on {
                        terms.iter().flatten().sum()
                    } else {
                        0.0
                    };
                    (bdp, hsum, n)
                };
                // Pool-anchor honest dual-store law (`RWM_POOL_ANCHOR`):
                // per-live-path honest caps on the send-interval anchor.
                // Collected only at N ≥ 2 (the N = 1 code path, including the
                // percap_k maps, is untouched); a cold send anchor or RTprop
                // yields a None term, so capw_store_cap's all-warm requirement
                // falls back to the configured chain below. live_paths(), not
                // active_paths(): the cwnd-saturation filter must not drop a
                // saturated path's earned share.
                let pa_terms: Vec<Option<f64>> = if pol.pool_anchor_on && n_live >= 2 {
                    // Rate source: the send-interval anchor (the ratcheted
                    // send mean). Path set: live_paths().
                    let slots: Vec<Option<HonestCapPath>> = {
                        let sched = scheduler.lock();
                        sched
                            .live_paths()
                            .iter()
                            .map(|id| {
                                sched.path(*id).and_then(|p| {
                                    // Cold send anchor or cold RTprop → no
                                    // slot at all, before feeding the clock
                                    // tracker.
                                    let sr = p.send_rate_anchor().filter(|r| *r > 0.0)?;
                                    let rtp = p
                                        .min_rtt()
                                        .map(|d| d.as_secs_f64())
                                        .filter(|r| *r > 0.0)?;
                                    Some(HonestCapPath {
                                        id: *id,
                                        anchor: Some(sr * rtp),
                                        rate: Some(sr),
                                        srtt: p.srtt(),
                                        rtprop: p.min_rtt(),
                                        k_raw: p.k_raw(),
                                    })
                                })
                            })
                            .collect()
                    };
                    honest_cap_terms(&mut scs.percap_k, &slots, dnow, pol.store_bdp_gain)
                } else {
                    Vec::new()
                };
                scs.pa_engaged = false;
                scs.tt_terms_diag = None;
                scs.dyn_store_cap = if let Some((cap, w, sl, sp)) = three_term_store_cap(
                    pol.three_term_on,
                    &tt_terms,
                    pol.contract_rho,
                    pol.delta_b,
                    pol.store_cap_floor,
                ) {
                    // The law under test takes precedence over every
                    // pooled fallback, exactly as `capw_store_cap` does
                    // for its own arm. Warm-up (any live path cold) ⇒
                    // `None` ⇒ the configured chain below runs verbatim.
                    scs.tt_terms_diag = Some((w, sl, sp));
                    cap
                } else if pol.honest_cap_on && hsum > 0.0 {
                    // Honest law: the Σ is already per-path-composed (each
                    // term carries its own K_i and runway), so no gain×
                    // multiplier here. Ceilings: the store latch at N = 1,
                    // the N×knee pool when the path-scaled pool is configured.
                    let ceiling = if pol.store_paths_on && n_live >= 2 {
                        n_live.saturating_mul(pol.store_path_pool).max(pol.store_cap_floor)
                    } else {
                        pol.store_max
                    };
                    (hsum.ceil() as usize).clamp(pol.store_cap_floor, ceiling)
                } else if let Some(cap) = capw_store_cap(
                    pol.pool_anchor_on,
                    &pa_terms,
                    pol.store_cap_floor,
                    pol.store_path_pool,
                ) {
                    // Pool-anchor law engaged (RWM_POOL_ANCHOR, N ≥ 2, all
                    // send anchors warm): Σ honest per-path caps on the
                    // burst-immune send-interval rate, clamped [floor, N·knee].
                    // The explicit experiment arm (RWM_PLAIN_RS+RWM_HONEST_CAP)
                    // takes precedence above.
                    scs.pa_engaged = true;
                    scs.pa_sum = pa_terms.iter().flatten().sum();
                    cap
                } else if let Some(cap) = pooled_store_cap(
                    pol.store_paths_on,
                    pol.sum_cap,
                    pol.delta_cap,
                    pol.delta_b,
                    n_live,
                    bdp,
                    pol.store_bdp_gain,
                    pol.store_cap_floor,
                    pol.store_path_pool,
                ) {
                    // The shipped seat: every default dual cell's cap is
                    // decided here. `pol.sum_cap` and `pol.delta_cap` are
                    // values inside one expression, not second laws — see
                    // `pooled_store_cap`.
                    sumcap.record(pol.sum_cap, n_live, bdp, pol.store_bdp_gain, cap);
                    dcap.record(n_live, bdp, pol.store_bdp_gain, cap);
                    cap
                } else if bdp > 0.0 {
                    ((pol.store_bdp_gain * bdp).ceil() as usize).clamp(pol.store_cap_floor, pol.store_max)
                } else {
                    pol.store_boot_cap.min(pol.store_max)
                };
                // `[CCAP]` bind fractions, taken at the refresh that computed
                // the cap. The unclamped law is `window + slack + span`,
                // recorded separately from its bounds
                // (`docs/measurement-discipline.md` rule 17).
                if pol.composed_cap {
                    ccap.refreshes += 1;
                    ccap.cap_sum += scs.dyn_store_cap as f64;
                    if let Some((w, sl, sp)) = scs.tt_terms_diag {
                        ccap.engaged += 1;
                        // TERM 3's geometry, folded in at the same refresh
                        // that counted the engagement. `span_forms` returns
                        // `Some` on exactly the ticks `three_term_store_cap`
                        // did, so the means and `eng=` share one denominator.
                        if let Some(sf) = span_forms(&tt_terms) {
                            debug_assert!(
                                (sf.shipped - sp).abs() <= 1e-9 * sp.abs().max(1.0),
                                "the [CCAP] span gauge and the law's TERM 3 \
                                 disagree — two computations of one geometry"
                            );
                            ccap.record_span(sf);
                        }
                        let unclamped = (w + sl + sp).ceil();
                        if unclamped >= WIN_STORE_MAX as f64 {
                            ccap.at_mem += 1;
                        }
                        if unclamped <= pol.store_cap_floor as f64 {
                            ccap.at_floor += 1;
                        }
                    }
                }
            }
            // `[3T]` readout — the three-term law's mechanism-liveness echo
            // (`docs/measurement-discipline.md` rule 15). It prints whenever
            // the gate is configured, so "configured but never engaged"
            // (all-cold anchors) is distinct from "engaged": `eng=0` with a
            // live `[GATES] RWM_THREE_TERM=1` is a warm-up failure, not a null
            // result. `span` must read 0.0 at every single-path cell.
            if pol.three_term_on && dnow.saturating_sub(scs.tt_print_us) >= 2_000_000 {
                scs.tt_print_us = dnow;
                let (w, sl, sp) = scs.tt_terms_diag.unwrap_or((0.0, 0.0, 0.0));
                info!(
                    eng = scs.tt_terms_diag.is_some() as u8,
                    cap = scs.dyn_store_cap,
                    window = w,
                    slack = sl,
                    span = sp,
                    rho = pol.contract_rho,
                    b = pol.delta_b,
                    "[3T] three-term outstanding limit: window + slack + span (RWM_THREE_TERM)"
                );
            }
        }
    }
}

/// What the paced GENERATION coded-emission block (proactive fill, the
/// deficit-driven recovery round-robin and the bootstrap/pacing clocks)
/// borrows from `run_window_sender`. `cell` fields are the loop's Copy
/// scalars: copied in at entry, written back at the single exit.
pub(crate) struct GenEmitCtx<'a> {
    pub pol: &'a SenderPolicy,
    pub generation: bool,
    pub tx_paused: bool,
    pub cwnd_full: bool,
    pub gp_rate_max: f64,
    pub cc_rate_cached: f64,
    pub scheduler: &'a Arc<parking_lot::Mutex<Scheduler>>,
    pub transport: &'a Arc<QuicTransport>,
    pub stats: &'a Arc<SharedStats>,
    pub batch_counter: &'a BatchCounter,
    pub window_ack_seq: &'a Arc<AtomicU64>,
    pub st: &'a mut SenderState,
    pub dg: &'a mut DiagState,
    pub gen_want: &'a mut BTreeMap<u64, u64>,
    pub gen_emitted: &'a mut std::collections::HashMap<u64, u64>,
    pub gen_recover_at: &'a mut std::collections::HashMap<u64, u64>,
    pub gen_coded_total: &'a mut u64,
    pub gen_rate_ewma: &'a mut f64,
    pub gen_rate_sample_us: &'a mut u64,
    pub gen_rate_sample_ack: &'a mut u64,
    pub gen_tokens: &'a mut f64,
    pub gen_tok_last_us: &'a mut u64,
    pub proactive_coded_total: &'a mut u64,
    pub recovery_coded_total: &'a mut u64,
}

/// The paced generation coded-emission phase: proactive fill, the proactive
/// pacer, and the deficit-driven recovery round-robin. Returns whether any
/// coded symbol flowed.
pub(crate) fn emit_generation_coded(ctx: GenEmitCtx<'_>) -> bool {
    let GenEmitCtx {
        pol,
        generation,
        tx_paused,
        cwnd_full,
        gp_rate_max,
        cc_rate_cached,
        scheduler,
        transport,
        stats,
        batch_counter,
        window_ack_seq,
        st,
        dg,
        gen_want,
        gen_emitted,
        gen_recover_at,
        gen_coded_total: gen_coded_total_cell,
        gen_rate_ewma: gen_rate_ewma_cell,
        gen_rate_sample_us: gen_rate_sample_us_cell,
        gen_rate_sample_ack: gen_rate_sample_ack_cell,
        gen_tokens: gen_tokens_cell,
        gen_tok_last_us: gen_tok_last_us_cell,
        proactive_coded_total: proactive_coded_total_cell,
        recovery_coded_total: recovery_coded_total_cell,
    } = ctx;
    let mut gen_coded_total = *gen_coded_total_cell;
    let mut gen_rate_ewma = *gen_rate_ewma_cell;
    let mut gen_rate_sample_us = *gen_rate_sample_us_cell;
    let mut gen_rate_sample_ack = *gen_rate_sample_ack_cell;
    let mut gen_tokens = *gen_tokens_cell;
    let mut gen_tok_last_us = *gen_tok_last_us_cell;
    let mut proactive_coded_total = *proactive_coded_total_cell;
    let mut recovery_coded_total = *recovery_coded_total_cell;
    let mut gd_flow = false;
    if generation && st.encoder.window_size() > 0 {
        let now = now_us();
        // Object tail: intake is idle (not just paused by backpressure — no
        // new source for a few RTTs while the pipe has room). Let the final
        // partial generation recover; a mid-stream backpressure pause is NOT
        // idle (tx_paused), so this never floods a still-filling generation.
        st.encoder.set_intake_idle(!tx_paused && now.saturating_sub(st.gen_last_source_us) > 30_000);
        // Under `RWM_OOO_RETAIN`, advance the proactive-coding floor to follow
        // the send frontier (the last `pipeline` sealed generations), decoupled
        // from the stalled in-order retention floor. The send frontier runs
        // `ooo_gens` ahead of a stalled generation; without this the coder
        // would keep re-coding the stalled generation and never provision the
        // fresh ones, which would then need reactive recovery.
        if pol.ooo_retain {
            let (_, newest) = st.encoder.window_span();
            let code_anchor =
                newest.saturating_sub((pol.pipeline as u64) * (pol.gen_size as u64));
            st.encoder.set_code_base(code_anchor);
        }
        // Ack-clocked window flow control. Emit coded symbols up to
        //   total_coded ≤ delivered·(1+r) + W_inflight
        // where `delivered` = cumulative ack (decoded source symbols) and
        // W_inflight is the in-flight coded allowance (≈ BDP + one
        // generation). The delivered·(1+r) term is the steady coded budget to
        // reconstruct what has been delivered (r covers loss + the MDS
        // margin); W_inflight is the burst the pipe may hold ahead of the
        // decode frontier. This self-clocks to the link goodput and bounds the
        // QUIC datagram buffer, so over-emission cannot strand fresh coded
        // behind stale coded. `RWM_GEN_R` (overhead r) and `RWM_GEN_INFLIGHT`
        // tune it.
        let ack_now = window_ack_seq.load(Ordering::Relaxed);
        // The encoder caps per-generation emission to ceil(len·(1+r)), so
        // this window is never spent producing low-rank symbols over a
        // still-filling generation. Loosening the clock to the sent frontier
        // reintroduces datagram bufferbloat and serializes symmetric
        // aggregation. The small-G frontier-advance deadlock is closed instead
        // by the receiver seeding a wedged generation's width so the deficit
        // loop — which is ack-clock-independent — always funds the frontier
        // hole. `RWM_CODED_SRC` offers the sent-frontier clock as an opt-in.
        // Under OOO retention and under gen_pipe the proactive budget is
        // clocked on the sent frontier: the cumulative ack is stalled on a
        // hole while the send frontier runs ahead, and the intake cap (M*·G)
        // plus the per-generation ceil(len·(1+r)) budgets already bound the
        // outstanding coded.
        let target = if pol.coded_src_clock || pol.ooo_retain || pol.gen_pipe {
            let (_, wend) = st.encoder.window_span();
            (wend as f64) * (1.0 + pol.gen_repair_floor) + pol.gen_inflight_window
        } else {
            (ack_now as f64) * (1.0 + pol.gen_repair_floor) + pol.gen_inflight_window
        };
        // Clock the pacing rate to the delivered goodput (ack rate): sample
        // the ack advance over a ~20 ms window into an EWMA, and pace at a
        // headroom multiple of that, clamped to [floor, ceiling], so coded
        // emission does not outrun the receiver's decode and the datagram
        // intake is not overrun. Before the first sample the floor primes the
        // first generation.
        {
            let dt = now.saturating_sub(gen_rate_sample_us);
            if dt >= 20_000 {
                let dack = ack_now.saturating_sub(gen_rate_sample_ack) as f64;
                let inst = dack / (dt as f64 / 1_000_000.0);
                gen_rate_ewma = if gen_rate_ewma <= 0.0 {
                    inst
                } else {
                    0.7 * gen_rate_ewma + 0.3 * inst
                };
                gen_rate_sample_us = now;
                gen_rate_sample_ack = ack_now;
            }
        }
        // Under CC-rate pacing the coded bucket shares the source's small
        // headroom (a 1.5× overshoot itself overruns the datagram path).
        // Under gen_pipe the pace is anchored to the windowed-max delivered
        // rate: the decode-clocked EWMA decays toward the floor between
        // generation acks, throttling emission exactly while the pipe is
        // waiting. Headroom 1.25 (the BBR probe gain — the wire must fund
        // (1+r)/(1−ε) ≈ 1.08× the delivered rate plus ramp margin) instead
        // of 1.5, whose overshoot bursts drop.
        let eff_factor = if pol.cc_pace {
            pol.cc_pace_headroom
        } else if pol.gen_pipe {
            1.25
        } else {
            1.5
        };
        // Under cc_pace clock coded emission on the frontier-independent CC
        // rate (max with the goodput EWMA) so a stalled in-order ack does not
        // starve coded emission below the link.
        let eff_base = if pol.cc_pace {
            gen_rate_ewma.max(cc_rate_cached)
        } else if pol.gen_pipe {
            gen_rate_ewma.max(gp_rate_max)
        } else {
            gen_rate_ewma
        };
        let eff_rate = (eff_base * eff_factor).clamp(pol.gen_rate_floor, pol.gen_rate);
        dg.diag_eff_rate = eff_rate;
        // Refill the pacing token bucket (capped at a small burst). Under
        // cc_pace the cap is ≈ a few ms of link rate (not 64) so a caught-up
        // bucket can't release a large coded burst onto the datagram path.
        let tok_dt = now.saturating_sub(gen_tok_last_us);
        gen_tok_last_us = now;
        let gen_tok_cap = if pol.cc_pace { (eff_rate * 0.004).clamp(8.0, 64.0) } else { 64.0 };
        gen_tokens = (gen_tokens + eff_rate * (tok_dt as f64 / 1_000_000.0)).min(gen_tok_cap);
        let burst_cap = if pol.cc_pace { 64u32 } else { 256u32 };
        let mut emitted = 0u32;
        while !pol.proactive_pacer
            && (gen_coded_total as f64) < target
            && emitted < burst_cap
            && gen_tokens >= 1.0
            && !cwnd_full
            && st.encoder.wants_coding()
        {
            let path = {
                let sched = scheduler.lock();
                if pol.xpath_repair {
                    sched.place_repair_spare_path().unwrap_or(0)
                } else {
                    sched.place_symbol(true, &[]).unwrap_or(0)
                }
            };
            gen_coded_total += 1;
            emitted += 1;
            gd_flow = true;
            gen_tokens -= 1.0;
            let sym = st.encoder.generate_repair();
            // Count this proactive emission toward the per-generation
            // in-flight accounting so the deficit loop never double-sends
            // what proactive already covered.
            if sym.data.len() >= 8 {
                let anchor = u64::from_le_bytes(sym.data[0..8].try_into().unwrap());
                *gen_emitted.entry(anchor).or_insert(0) += 1;
                if pol.diag_on {
                    st.gl.entry(anchor).or_insert((0, 0, 0)).2 = now_us();
                }
            }
            proactive_coded_total += 1;
            let seqs = batch_counter.next(path);
            let batch = SymbolBatch::new(vec![sym], now_us(), seqs, path);
            if let Err(e) = transport.send_symbols(path, batch) {
                warn!(path, ?e, "failed to send generation coded symbol");
            }
            {
                let mut sched = scheduler.lock();
                if let Some(p) = sched.path_mut(path) {
                    p.charge_in_flight(1);
                }
            }
            if let Some(ps) = stats.path(path) {
                ps.symbols_sent.fetch_add(1, Ordering::Relaxed);
            }
            stats.fec.total_repair_symbols.fetch_add(1, Ordering::Relaxed);
        }

        // Proactive pacer (`RWM_PROACTIVE_PACER`) — present-at-stall. Emit
        // filling-generation proactive repair on the generation grid, paced by
        // the same CC token bucket but without the ack-clock `target` gate
        // (the cumulative ack is stalled exactly when the frontier needs
        // repair) and without any source-availability gate (this block runs
        // every loop iteration, including tx_paused wakeups). Bounded by each
        // generation's ceil(len·r) budget (wants_filling_coding turns false at
        // budget), the CC rate (gen_tokens) and congestion (cwnd_full).
        // Supersedes the sealed batched proactive path above; the reactive
        // deficit below remains the fallback.
        if pol.proactive_pacer {
            let mut fill_emitted = 0u32;
            while fill_emitted < burst_cap
                && gen_tokens >= 1.0
                && !cwnd_full
                && st.encoder.wants_filling_coding()
            {
                let path = {
                    let sched = scheduler.lock();
                    if pol.xpath_repair {
                        sched.place_repair_spare_path().unwrap_or(0)
                    } else {
                        sched.place_symbol(true, &[]).unwrap_or(0)
                    }
                };
                let sym = st.encoder.generate_repair_filling();
                fill_emitted += 1;
                gen_tokens -= 1.0;
                // Count against per-generation in-flight accounting so the
                // deficit loop never double-sends what proactive covered.
                if sym.data.len() >= 8 {
                    let anchor = u64::from_le_bytes(sym.data[0..8].try_into().unwrap());
                    *gen_emitted.entry(anchor).or_insert(0) += 1;
                }
                proactive_coded_total += 1;
                let seqs = batch_counter.next(path);
                let batch = SymbolBatch::new(vec![sym], now_us(), seqs, path);
                if let Err(e) = transport.send_symbols(path, batch) {
                    warn!(path, ?e, "failed to send filling-generation repair");
                }
                {
                    let mut sched = scheduler.lock();
                    if let Some(p) = sched.path_mut(path) {
                        p.charge_in_flight(1);
                    }
                }
                if let Some(ps) = stats.path(path) {
                    ps.symbols_sent.fetch_add(1, Ordering::Relaxed);
                }
                stats.fec.total_repair_symbols.fetch_add(1, Ordering::Relaxed);
            }
        }

        // Deficit-driven recovery emission. Emit the residual coded symbols
        // each stalled frontier generation still needs, paced by the same
        // token bucket, round-robin so no generation starves — but not gated
        // by the ack-clocked `target`: the cumulative ack is stalled exactly
        // when the frontier generation needs recovery, so gating recovery on
        // the ack is a deadlock. The receiver's per-generation deficit bounds
        // the total (we send only the residual it reports, minus what is
        // already in flight — tracked in gen_want), so bypassing the ack
        // clock cannot flood.
        if !pol.no_reactive && !gen_want.is_empty() {
            let rec_burst = 256u32;
            let mut rec_emitted = 0u32;
            'recover: loop {
                // Reactive stops at the shared link budget (gen_tokens) and,
                // with `RWM_REACT_CAP`, is non-exempt from the in-flight cap
                // (cwnd_full): exempting it floods the pipe. The frontier is
                // funded instead by (a) the win backstop keeping the send
                // frontier within a few generations of the in-order frontier,
                // so the stranded generation is recent, and (b) recovery
                // running each iteration as the in-flight budget expires on
                // the RTT timescale.
                if gen_tokens < 1.0 || rec_emitted >= rec_burst
                    || (pol.react_cap_on && cwnd_full) {
                    break;
                }
                let now_r = now_us();
                let anchors: Vec<u64> = gen_want.keys().copied().collect();
                if anchors.is_empty() {
                    break;
                }
                let mut progressed = false;
                for a in anchors {
                    if gen_tokens < 1.0 || rec_emitted >= rec_burst
                        || (pol.react_cap_on && cwnd_full) {
                        break 'recover;
                    }
                    let want = gen_want.get(&a).copied().unwrap_or(0);
                    if want == 0 {
                        gen_want.remove(&a);
                        continue;
                    }
                    let sym = match st.encoder.generate_repair_for(a) {
                        Some(s) => s,
                        None => {
                            // Generation no longer retained/sealed (decoded
                            // and advanced, or not yet sealed) — drop its want.
                            gen_want.remove(&a);
                            continue;
                        }
                    };
                    // Deficit recovery funds a frontier generation whose hole
                    // is the long pole — most often a source lost on the slow
                    // path. Place it by the ∝-goodput placement law (softmax
                    // over marginal cost, paper §5.7), which biases the
                    // covering repair toward the fast path proportionally
                    // without starving a symmetric second path (hard argmax
                    // concentration serializes symmetric aggregation).
                    let path = {
                        let sched = scheduler.lock();
                        if pol.xpath_repair {
                            sched.place_repair_spare_path().unwrap_or(0)
                        } else {
                            sched.place_symbol(true, &[]).unwrap_or(0)
                        }
                    };
                    *gen_emitted.entry(a).or_insert(0) += 1;
                    recovery_coded_total += 1;
                    gd_flow = true;
                    if pol.diag_on {
                        st.gl.entry(a).or_insert((0, 0, 0)).2 = now_us();
                    }
                    let nw = want - 1;
                    if nw == 0 {
                        gen_want.remove(&a);
                    } else {
                        gen_want.insert(a, nw);
                    }
                    gen_tokens -= 1.0;
                    if pol.react_cap_on {
                        // Stamp this generation's recovery time so the spacing
                        // check above holds off further recovery for ~1 SRTT.
                        gen_recover_at.insert(a, now_r);
                    }
                    rec_emitted += 1;
                    progressed = true;
                    let seqs = batch_counter.next(path);
                    let batch = SymbolBatch::new(vec![sym], now_us(), seqs, path);
                    if let Err(e) = transport.send_symbols(path, batch) {
                        warn!(path, ?e, "failed to send generation recovery symbol");
                    }
                    {
                        let mut sched = scheduler.lock();
                        if let Some(p) = sched.path_mut(path) {
                            p.charge_in_flight(1);
                        }
                    }
                    if let Some(ps) = stats.path(path) {
                        ps.symbols_sent.fetch_add(1, Ordering::Relaxed);
                    }
                    stats.fec.total_repair_symbols.fetch_add(1, Ordering::Relaxed);
                }
                if !progressed {
                    break;
                }
            }
        }
    }
    *gen_coded_total_cell = gen_coded_total;
    *gen_rate_ewma_cell = gen_rate_ewma;
    *gen_rate_sample_us_cell = gen_rate_sample_us;
    *gen_rate_sample_ack_cell = gen_rate_sample_ack;
    *gen_tokens_cell = gen_tokens;
    *gen_tok_last_us_cell = gen_tok_last_us;
    *proactive_coded_total_cell = proactive_coded_total;
    *recovery_coded_total_cell = recovery_coded_total;
    gd_flow
}

/// What the NACK/gap repair loop (drain the receiver's gap reports and
/// answer each hole with a targeted retransmit or a coded repair, under
/// the cooldown, budget, recovery-suppression and hold-down laws) borrows
/// from `run_window_sender`. `cell` fields are the loop's Copy scalars:
/// copied in at entry, written back at the single exit.
pub(crate) struct ServeGapsCtx<'a> {
    pub pol: &'a SenderPolicy,
    pub reliable: bool,
    pub now_repair_us: u64,
    pub cached_max_repairs: u64,
    pub scheduler: &'a Arc<parking_lot::Mutex<Scheduler>>,
    pub fec_controller: &'a Arc<parking_lot::Mutex<FecRateController>>,
    pub transport: &'a Arc<QuicTransport>,
    pub stats: &'a Arc<SharedStats>,
    pub batch_counter: &'a BatchCounter,
    pub copa_feed: &'a Option<Arc<CopaFeed>>,
    pub mpd_pf_floor: &'a std::cell::Cell<u64>,
    pub mpd_pf_clock: &'a std::cell::Cell<u64>,
    pub mpd_pf_sum: &'a std::cell::Cell<u64>,
    pub nack_rx: &'a mut tokio::sync::mpsc::Receiver<(FireCause, u32, Vec<(u64, u64)>)>,
    pub st: &'a mut SenderState,
    pub dg: &'a mut DiagState,
    pub rack_echo: &'a mut RackClockGauge,
    pub hold_echo: &'a mut HoldDownGauge,
    pub mp_delivered: &'a mut std::collections::HashMap<u32, Vec<u64>>,
    pub pending_gaps: &'a mut Option<(FireCause, u32, Vec<(u64, u64)>)>,
    pub cached_nack_budget: &'a mut u64,
    pub nack_repairs_this_period: &'a mut u64,
    pub mp_evid_max: &'a mut u64,
}

/// The NACK/gap repair phase: drain the receiver's gap reports and answer
/// each hole under the cooldown, budget, suppression and hold-down laws.
pub(crate) fn serve_gaps(ctx: ServeGapsCtx<'_>) {
    let ServeGapsCtx {
        pol,
        reliable,
        now_repair_us,
        cached_max_repairs,
        scheduler,
        fec_controller,
        transport,
        stats,
        batch_counter,
        copa_feed,
        mpd_pf_floor,
        mpd_pf_clock,
        mpd_pf_sum,
        nack_rx,
        st,
        dg,
        rack_echo,
        hold_echo,
        mp_delivered,
        pending_gaps,
        cached_nack_budget: cached_nack_budget_cell,
        nack_repairs_this_period: nack_repairs_this_period_cell,
        mp_evid_max: mp_evid_max_cell,
    } = ctx;
    let mut cached_nack_budget = *cached_nack_budget_cell;
    let mut nack_repairs_this_period = *nack_repairs_this_period_cell;
    let mut mp_evid_max = *mp_evid_max_cell;
    loop {
        let (gap_cause, ack_path, gaps) = match pending_gaps.take() {
            Some(g) => g,
            None => match nack_rx.try_recv() {
                Ok(g) => {
                    // A gap report is a state snapshot of the receiver's
                    // current holes (frontier + inverted SACK), not a delta,
                    // so a queued backlog is stale by construction and only
                    // the newest snapshot needs processing. Under the mp law
                    // holes legitimately outlive their reports (suppressed
                    // until a loss channel fires), so the gap-ack cadence
                    // queues snapshots faster than they change; coalescing
                    // removes that walk tax. With the gate off every report
                    // is processed. The newest snapshot keeps its `[FCAUSE]`
                    // cause tag, so fires are attributed to the report that
                    // drove them.
                    let mut g = g;
                    if pol.recov_mp_law {
                        while let Ok(n) = nack_rx.try_recv() {
                            g = n;
                            if pol.diag_on {
                                dg.mpd_coalesced += 1;
                            }
                        }
                    }
                    g
                }
                Err(_) => break,
            },
        };
        if cached_max_repairs == 0 || cached_nack_budget == 0 {
            // Fully suppressed or budget exhausted — drain NACK queue
            dg.diag_gaps_dropped += 1;
            continue;
        }

        // SRTT drives the per-seq retransmit cooldown and the age gate.
        // RWM_RECOV_MP additionally snapshots per-path smoothed clocks
        // (Copa srtt + estimator EWMA) for the per-flight hole law, and
        // the live path count (N = 1 ⇒ the law is inert).
        // Tuple is (copa/estimator srtt, estimator EWMA rtt).
        let mut mp_clocks: std::collections::HashMap<u32, (u64, u64)> =
            std::collections::HashMap::new();
        let mut mp_n_paths: usize = 1;
        let srtt_us = {
            let sched = scheduler.lock();
            // The law's N and clock snapshot range over the channel's
            // membership (`recovery_clock_paths`): a cwnd-saturated path
            // (available() == 0) must not collapse the law to the N = 1
            // bypass mid-transfer.
            let ids = recovery_clock_paths(&sched);
            if pol.recov_mp_law || pol.recov_sp || pol.diag_on {
                mp_n_paths = ids.len();
                for id in &ids {
                    if let Some(p) = sched.path(*id) {
                        mp_clocks.insert(
                            *id,
                            (
                                p.srtt().as_micros() as u64,
                                p.estimator.rtt().as_micros() as u64,
                            ),
                        );
                    }
                }
            }
            // The pooled clock reads the same set.
            let pooled: Vec<u64> = ids
                .iter()
                .filter_map(|id| sched.path(*id))
                .map(|p| p.estimator.rtt().as_micros() as u64)
                .collect();
            pooled_recovery_srtt_us(&pooled)
        };
        // The per-seq retransmit cooldown, floored at the fixed floor.
        let retx_cooldown_us = retx_cooldown_us(srtt_us, NACK_RETX_COOLDOWN_FLOOR_US);
        // The per-flight law threshold for a path (falls back to the
        // pooled cooldown clock when the path has no snapshot).
        let mp_thr_of = |mp_clocks: &std::collections::HashMap<u32, (u64, u64)>,
                         p: u32|
         -> u64 {
            match mp_clocks.get(&p) {
                Some(&(srtt, ewma)) => {
                    // The kGranularity analog: the fixed cooldown floor.
                    let floor = NACK_RETX_COOLDOWN_FLOOR_US;
                    let (thr, floor_won) = mp_time_threshold_split(srtt, ewma, floor);
                    if pol.diag_on {
                        if floor_won {
                            mpd_pf_floor.set(mpd_pf_floor.get() + 1);
                        } else {
                            mpd_pf_clock.set(mpd_pf_clock.get() + 1);
                        }
                        mpd_pf_sum.set(mpd_pf_sum.get().saturating_add(floor));
                    }
                    thr
                }
                None => retx_cooldown_us,
            }
        };
        // The same 9/8 threshold as `mp_thr_of`, without its `pf=` counter
        // side effect: a read-only audit gauge may not move an existing
        // gauge's reading, and `mp_thr_of` bumps `mpd_pf_*` on every call.
        let mp_thr_pure = |mp_clocks: &std::collections::HashMap<u32, (u64, u64)>,
                           p: u32|
         -> u64 {
            match mp_clocks.get(&p) {
                Some(&(srtt, ewma)) => {
                    mp_time_threshold_split(srtt, ewma, NACK_RETX_COOLDOWN_FLOOR_US).0
                }
                None => retx_cooldown_us,
            }
        };
        // `max(srtt, ewma)/2` per path — the closure classifier's only
        // input, and the same half-RTT the age gate uses.
        let half_srtt_of = |p: u32| -> u64 {
            match mp_clocks.get(&p) {
                Some(&(srtt, ewma)) => srtt.max(ewma) / 2,
                None => srtt_us / 2,
            }
        };

        let (win_start, win_end) = st.encoder.window_span();
        let mut retransmitted: u64 = 0;
        let mut nacked_count: u64 = 0;
        if pol.diag_on {
            dg.mpd_gap_reports += 1;
        }

        // Packet-threshold evidence ingestion (RFC 9002 §6.1.1 per
        // path): fold this report's implied delivered intervals into the
        // per-path sorted evidence lists. Monotone watermark ⇒ each seq
        // ingested at most once over the transfer.
        if pol.recov_mp_law && mp_n_paths > 1 {
            for (lo, hi) in mp_delivered_intervals(&gaps) {
                let start = lo.max(mp_evid_max + 1);
                if start > hi {
                    continue;
                }
                for (&q, &pj) in st.source_path_map.range(start..=hi) {
                    mp_delivered.entry(pj).or_default().push(q);
                }
                mp_evid_max = mp_evid_max.max(hi);
            }
        }

        // The hold-down resolution signal (paper §7.4). Every stamped hole
        // this report no longer lists, inside the span this report covers, is
        // one the receiver has — feed its outstanding time. Placed here,
        // before the loop below stamps this batch's own seqs, or every hole
        // would be resolved by the report that first announced it.
        hold_echo.on_report(
            &gaps,
            now_repair_us,
            &st.shed_seqs,
            ack_path,
            &half_srtt_of,
        );
        'gaps: for &(gap_start, gap_end) in &gaps {
            // EVICT: only the coding window can serve a gap — older
            // seqs are gone. RETAIN: the sent-data store serves ANY
            // un-acked seq (targeted ARQ for holes that aged out of
            // the FEC horizon), so gaps are not clamped to the window.
            let (clamped_start, clamped_end) = if reliable {
                (gap_start, gap_end)
            } else {
                (gap_start.max(win_start), gap_end.min(win_end))
            };
            if clamped_start > clamped_end {
                continue;
            }
            nacked_count += clamped_end - clamped_start + 1;

            for seq in clamped_start..=clamped_end {
                if retransmitted >= cached_max_repairs || cached_nack_budget == 0 {
                    break 'gaps;
                }
                if pol.diag_on {
                    dg.mpd_gap_seqs += 1;
                }
                // Stamp the first report of this hole (paper §7.4). Placed at
                // the loop head so the origin is "when the sender was first
                // told", not "when the sender got as far as deciding" — a
                // hole suppressed by cooldown was still outstanding.
                // Also record the seq's live flight at its first report — the
                // last retransmit if any, else the original send — against
                // the RFC 9002 §6.1.2 threshold that would judge it. Read
                // before every suppression `continue`, so a hole the cooldown
                // or the shed skips is measured too.
                let ha_flight: Option<(u64, u32)> = st
                    .nack_retx_at
                    .get(&seq)
                    .copied()
                    .or_else(|| {
                        st.retransmit_buffer.get(&seq).map(|&(t, _, p)| (t, p))
                    });
                hold_echo.on_reported(
                    seq,
                    st.source_path_map.get(&seq).copied().unwrap_or(st.last_source_path),
                    now_repair_us,
                    matches!(gap_cause, FireCause::GapData | FireCause::GapRefresh),
                    ha_flight.map(|(t, p)| {
                        (
                            now_repair_us.saturating_sub(t),
                            mp_thr_pure(&mp_clocks, p),
                        )
                    }),
                );
                // δ-honest shed (paper §5.6): a hole already shed is never
                // served again (the receiver's own δ-horizon passes it); a
                // past-deadline hole is shed within the ρ budget — a
                // retransmit fired at age > D(δ) lands after the receiver's
                // give-up, pure waste that serializes the stream.
                // Budget-refused holes fall through to ARQ (serialize: ρ
                // wins).
                if pol.shed_on {
                    if st.shed_seqs.contains(&seq) {
                        continue;
                    }
                    if let Some(&(send_time_us, _, _)) = st.retransmit_buffer.get(&seq) {
                        let age = now_repair_us.saturating_sub(send_time_us);
                        if shed_allowed(
                            age,
                            st.shed_deadline_us_live,
                            st.shed_total,
                            stats.fec.total_source_symbols.load(Ordering::Relaxed),
                            st.shed_budget_frac,
                        ) {
                            st.retransmit_buffer.remove(&seq);
                            st.nack_retx_at.remove(&seq);
                            st.shed_seqs.insert(seq);
                            st.shed_total += 1;
                            continue;
                        }
                        if st.shed_deadline_us_live > 0 && age > st.shed_deadline_us_live {
                            st.shed_denied += 1;
                        }
                    }
                }
                // Per-seq cooldown: repeated gap acks for the same
                // hole must not resend more than once per SRTT.
                if let Some(&(last, _)) = st.nack_retx_at.get(&seq) {
                    if !cooldown_elapsed(now_repair_us, last, retx_cooldown_us) {
                        if pol.diag_on {
                            dg.mpd_supp_cool += 1;
                        }
                        continue;
                    }
                }
                // The seq's live flight: the last retransmit if any (it
                // inherits the in-flight clock of its own path), else the
                // original send.
                let mp_flight: Option<(u64, u32)> = st.nack_retx_at
                    .get(&seq)
                    .copied()
                    .or_else(|| {
                        st.retransmit_buffer.get(&seq).map(|&(t, _, p)| (t, p))
                    });
                if pol.recov_mp_law && mp_n_paths > 1 {
                    // The skew-aware hole law (ADR-0059) — RFC 9002 loss detection
                    // generalized per path, BOTH channels:
                    //  §6.1.1 packet threshold (fast, honest): the
                    //   ORIGINAL flight on path j is lost once ≥3 later
                    //   path-j symbols are delivered (same-path FIFO
                    //   evidence — scheduler-created cross-path gaps
                    //   cannot trigger it). Retransmitted seqs are
                    //   excluded (wire order ≠ seq order for them).
                    //  §6.1.2 time threshold (safety net): a gap whose
                    //   LIVE flight is younger than 9/8× its own path's
                    //   smoothed RTT is a gap the scheduler created,
                    //   not a hole. Suppression-only; the receiver's
                    //   hole-refresh re-advertises until a channel
                    //   fires, so real holes still recover.
                    let time_ripe = match mp_flight {
                        Some((t, p)) => mp_hole_ripe(
                            mp_n_paths,
                            now_repair_us,
                            Some(t),
                            mp_thr_of(&mp_clocks, p),
                        ),
                        None => true,
                    };
                    let mut fast = false;
                    if !time_ripe && !st.nack_retx_at.contains_key(&seq) {
                        let orig = st.source_path_map.get(&seq).copied();
                        fast = orig
                            .and_then(|j| mp_delivered.get(&j))
                            .is_some_and(|v| mp_fast_lost(v, seq));
                    }
                    if !time_ripe && !fast {
                        if pol.diag_on {
                            dg.mpd_supp_law += 1;
                        }
                        continue;
                    }
                    if fast && pol.diag_on {
                        dg.mpd_fired_fast += 1;
                    }
                } else if pol.recov_sp && mp_n_paths <= 1 {
                    // `RWM_RECOV_SP`: the same §6.1.2 time threshold applied
                    // at N = 1 — a gap seq whose live flight (last retransmit,
                    // else the original) is younger than 9/8×max(smoothed
                    // clocks) is merely late/queued, not lost. Time channel
                    // only (see the gate's decl note); suppression-only — the
                    // hole-refresh re-advertises.
                    let time_ripe = time_threshold_ripe(
                        now_repair_us,
                        mp_flight.map(|(t, _)| t),
                        mp_flight
                            .map(|(_, p)| mp_thr_of(&mp_clocks, p))
                            .unwrap_or(0),
                    );
                    if !time_ripe {
                        if pol.diag_on {
                            dg.mpd_supp_law += 1;
                        }
                        continue;
                    }
                } else {
                    // Age gate: cross-path/jitter skew can report a seq that
                    // is merely late, not lost — only repair symbols old
                    // enough that an in-flight copy would already have been
                    // sacked.
                    if let Some(&(send_time_us, _, _)) = st.retransmit_buffer.get(&seq) {
                        if !legacy_age_ripe(now_repair_us, send_time_us, srtt_us) {
                            if pol.diag_on {
                                dg.mpd_supp_age += 1;
                            }
                            continue;
                        }
                    }
                }
                // Cross-path: avoid the path that originally carried this
                // symbol. On the reliable seat the targeted retransmit is
                // placed by the law with a ρ_fate penalty on the original
                // path (paper §5.7) — the continuous form of
                // select_repair_path_avoiding.
                let original_path = st.source_path_map.get(&seq).copied().unwrap_or(st.last_source_path);
                let nack_path = {
                    let sched = scheduler.lock();
                    if reliable {
                        sched.place_symbol(true, &[original_path]).unwrap_or(st.last_source_path)
                    } else {
                        select_repair_path_avoiding(&sched, original_path, st.last_source_path)
                    }
                };

                // Exact source retransmission first — reliable mode
                // serves from the sent-data store (survives window
                // eviction; a stale gap for an already-acked seq has
                // nothing to serve and is skipped) — else fall back
                // to the encoder window, then to a fungible repair.
                let sym = if reliable {
                    match st.sent_store.get(&seq) {
                        Some(s) => s.clone(),
                        // Not in the store ⇒ already acked (removal is
                        // by ack only): the receiver has it; skip.
                        None => {
                            if pol.diag_on {
                                dg.mpd_stale += 1;
                            }
                            continue;
                        }
                    }
                } else {
                    st.encoder.get_source(seq).unwrap_or_else(|| st.encoder.generate_repair())
                };

                // The hold-down gate (`RWM_HOLDDOWN_Q`, paper §7.4). The sender
                // does not answer a reported hole until it has been outstanding
                // for at least `T(q) = W_q(1−q)` on the path its original flew
                // — an order statistic on the hole-resolution stream. Placed
                // after every other suppression `continue` and before
                // `record_fire_cause`, so `[FCAUSE] n` keeps meaning "fires
                // that reached the wire" and `[HOLD] sup=` is the only place
                // the difference is accounted. With the gate absent
                // `should_hold` is always false.
                //
                // `[FCAUSE]` then classifies this fire by the producer of the
                // gap batch that drove it. It sits outside the `mp_flight`
                // guard that `record_fire` sits inside, so `[FCAUSE] n` can
                // exceed `[RACK] fired`; the difference is printed as
                // `unattr=`.
                //
                // False-alarm validation (paper §7.1), fed on every arm
                // (ungated by RWM_DIAG): a fire whose target flight is
                // younger than its own per-path law threshold is data that was
                // going to arrive anyway — the spurious-by-law class, the
                // measurable stand-in for RACK's DSACK-detected spurious
                // recovery, scored against RFC 8985 §6.2.4's budget.
                //
                // DIAG attribution below: live-flight age vs the per-path law
                // threshold (young = the law would have suppressed it),
                // per-flight-path and per-retx-path emission counts.
                if hold_echo.should_hold(seq, now_repair_us) {
                    continue;
                }
                rack_echo.record_fire_cause(gap_cause);
                if let Some((t, p)) = mp_flight {
                    let age = now_repair_us.saturating_sub(t);
                    rack_echo.record_fire(age < mp_thr_of(&mp_clocks, p));
                }
                if pol.diag_on {
                    if let Some((t, p)) = mp_flight {
                        let age = now_repair_us.saturating_sub(t);
                        let thr = mp_thr_of(&mp_clocks, p);
                        if age < thr {
                            dg.mpd_fired_young += 1;
                        } else {
                            dg.mpd_fired_ripe += 1;
                        }
                        dg.mpd_age_ms_sum += age as f64 / 1000.0;
                        *dg.mpd_fired_flight.entry(p).or_insert(0) += 1;
                    } else {
                        dg.mpd_fired_ripe += 1;
                    }
                    *dg.mpd_fired_on.entry(nack_path).or_insert(0) += 1;
                    // Retransmit attributed to the seq's original placement
                    // path (slow-placed symbols re-served on the fast path).
                    *dg.c8c_retx_orig.entry(original_path).or_insert(0) += 1;
                }

                let seqs = batch_counter.next(nack_path);
                let batch = SymbolBatch::new(vec![sym], now_us(), seqs, nack_path);
                let sent = match transport.send_symbols(nack_path, batch) {
                    Ok(()) => true,
                    Err(e) => {
                        warn!(nack_path, ?e, "failed to send NACK retransmission");
                        false
                    }
                };
                debug!(seq, nack_path, "SACK-gap retransmit");
                // `RWM_CHARGE_RECOVERY` (default off): bypass channel 1 of 2.
                // Without it this symbol reaches the link with no in-flight
                // charge, no pacer debit and no `symbols_sent` increment,
                // where every other channel meters all three at the handoff
                // (see `send_arq_repair_batch`). Charging cannot deadlock
                // recovery: this site reads neither `available()` nor
                // `cwnd_full`; it is budgeted by `cached_nack_budget`. The
                // pacer debit may go negative, as the block-ARQ repair's does.
                if crate::scheduler::charge_recovery_active() {
                    {
                        let mut sched = scheduler.lock();
                        if let Some(p) = sched.path_mut(nack_path) {
                            p.charge_in_flight(1);
                            p.consume_pace_tokens(1);
                        }
                    }
                    if let Some(ps) = stats.path(nack_path) {
                        ps.symbols_sent.fetch_add(1, Ordering::Relaxed);
                    }
                }
                // A retransmit re-commits the seq to its new path and
                // re-snapshots the rate sample, so the eventual ack is
                // attributed to the path that delivered it with a truthful
                // send-interval (a paused feed is an absent feed).
                if let Some(feed) = copa_feed.as_ref() {
                    feed.on_sent(seq, nack_path);
                    let mut sched = scheduler.lock();
                    if let Some(p) = sched.path_mut(nack_path) {
                        p.on_src_sent(seq, false);
                    }
                }
                // The retransmit inherits the in-flight state: the next
                // hole decision for this seq clocks THIS flight on ITS
                // path (closes the re-NACK-while-flying feedback).
                st.nack_retx_at.insert(seq, (now_repair_us, nack_path));
                // A copy of this seq has reached the wire. Stamped here and
                // nowhere else, so the classifier's "was a retransmit ever
                // emitted" is answered by the emission itself.
                hold_echo.on_retx(seq, now_repair_us, nack_path);
                stats.fec.record_correction(CorrectionKind::SourceCopy, sent);
                nack_repairs_this_period += 1;
                cached_nack_budget = cached_nack_budget.saturating_sub(1);
                dg.diag_retx += 1;
                retransmitted += 1;
            }
        }

        // Repair margin: extra repairs proportional to loss rate
        if retransmitted > 0 {
            let current_loss = {
                let sched = scheduler.lock();
                recovery_clock_paths(&sched)
                    .iter()
                    .filter_map(|id| sched.path(*id))
                    .map(|p| p.estimator.loss_rate())
                    .fold(0.0f64, f64::max)
            };
            let margin = (retransmitted as f64 * current_loss).ceil() as u64;
            // Place the extra repair margin by the law (paper §5.7): fungible
            // repairs cover the whole window → fate over the window's source
            // paths. Single path ⇒ that path.
            let margin_path = {
                let sched = scheduler.lock();
                if reliable {
                    let covered = window_source_paths(&*st.encoder, &st.source_path_map);
                    sched.place_symbol(true, &covered).unwrap_or(st.last_source_path)
                } else {
                    select_repair_path(&sched, st.last_source_path)
                }
            };
            for _ in 0..margin {
                if st.encoder.window_size() == 0 {
                    break;
                }
                let repair_sym = st.encoder.generate_repair();
                let seqs = batch_counter.next(margin_path);
                let batch = SymbolBatch::new(vec![repair_sym], now_us(), seqs, margin_path);
                let sent = match transport.send_symbols(margin_path, batch) {
                    Ok(()) => true,
                    Err(e) => {
                        warn!(margin_path, ?e, "failed to send NACK repair margin");
                        false
                    }
                };
                // `RWM_CHARGE_RECOVERY`: bypass channel 2 of 2 — same three
                // meters as the SACK-gap retransmit above.
                if crate::scheduler::charge_recovery_active() {
                    {
                        let mut sched = scheduler.lock();
                        if let Some(p) = sched.path_mut(margin_path) {
                            p.charge_in_flight(1);
                            p.consume_pace_tokens(1);
                        }
                    }
                    if let Some(ps) = stats.path(margin_path) {
                        ps.symbols_sent.fetch_add(1, Ordering::Relaxed);
                    }
                }
                stats.fec.record_correction(CorrectionKind::Coded, sent);
                nack_repairs_this_period += 1;
                cached_nack_budget = cached_nack_budget.saturating_sub(1);
            }
        }

        // Reduce repair_debt — NACK'd symbols are handled reactively now
        // The cadenced rate (Stage 1c; evaluated outside the locks on a miss).
        let repair_rate = super::emit_source::cadenced_repair_rate(
            &mut st.rate_cache,
            fec_controller,
            scheduler,
            st.encoder.window_size(),
            now_us(),
        );
        let debt_reduction = nacked_count as f64 * repair_rate;
        st.repair_debt = (st.repair_debt - debt_reduction).max(0.0);
    }
    *cached_nack_budget_cell = cached_nack_budget;
    *nack_repairs_this_period_cell = nack_repairs_this_period;
    *mp_evid_max_cell = mp_evid_max;
}

/// What the cumulative-ack advance (slide the encoder window, release the
/// retention store and the per-seq ledgers, feed the rate controller and
/// the generation/request books) borrows from `run_window_sender`. `cell`
/// fields are the loop's Copy scalars: copied in at entry, written back at
/// the single exit.
pub(crate) struct AckAdvanceCtx<'a> {
    pub pol: &'a SenderPolicy,
    pub gates: &'a crate::gates::RuntimeGates,
    /// The peer's cumulative point, wire v9 count form: every seq below it
    /// is delivered; 0 = nothing delivered.
    pub next_expected: u64,
    pub generation: bool,
    pub reliable: bool,
    pub scheduler: &'a Arc<parking_lot::Mutex<Scheduler>>,
    pub fec_controller: &'a Arc<parking_lot::Mutex<FecRateController>>,
    pub completion_feed: &'a Option<Arc<CompletionFeed>>,
    pub mp_delivered: &'a mut std::collections::HashMap<u32, Vec<u64>>,
    pub st: &'a mut SenderState,
    pub dg: &'a mut DiagState,
    pub hold_echo: &'a mut HoldDownGauge,
    pub gen_want: &'a mut BTreeMap<u64, u64>,
    pub gen_emitted: &'a mut std::collections::HashMap<u64, u64>,
    pub gen_emitted_at_report: &'a mut std::collections::HashMap<u64, u64>,
    pub gen_recover_at: &'a mut std::collections::HashMap<u64, u64>,
    pub req_want: &'a mut BTreeMap<u64, (u16, u64)>,
    pub req_emitted: &'a mut std::collections::HashMap<u64, u64>,
    pub req_at_report: &'a mut std::collections::HashMap<u64, u64>,
    pub sack_released: &'a mut BTreeSet<u64>,
    /// The last `next_expected` this phase processed (count form, 0 at start).
    pub prev_ack: &'a mut u64,
    pub c8c_last_ack_adv_us: &'a mut u64,
    pub nack_repairs_this_period: &'a mut u64,
}

/// The newly acked span of one cumulative advance, wire v9 count form:
/// `Some((first, last))` inclusive when `next_expected` moved past
/// `prev_next`, i.e. seqs `prev_next ..= next_expected − 1` were delivered;
/// `None` otherwise. A first ack of `next_expected = 1` (only seq 0
/// delivered) is `Some((0, 0))` -- v8's `received_up_to = 0` could not be
/// told apart from "nothing delivered", so seq 0 was never acked or pruned.
pub(crate) fn cumulative_advance(prev_next: u64, next_expected: u64) -> Option<(u64, u64)> {
    (next_expected > prev_next).then(|| (prev_next, next_expected - 1))
}

/// The cumulative-ack advance phase.
pub(crate) fn on_ack_advance(ctx: AckAdvanceCtx<'_>) {
    let AckAdvanceCtx {
        pol,
        gates,
        next_expected,
        generation,
        reliable,
        scheduler,
        fec_controller,
        completion_feed,
        mp_delivered,
        st,
        dg,
        hold_echo,
        gen_want,
        gen_emitted,
        gen_emitted_at_report,
        gen_recover_at,
        req_want,
        req_emitted,
        req_at_report,
        mut sack_released,
        prev_ack: prev_ack_cell,
        c8c_last_ack_adv_us: c8c_last_ack_adv_us_cell,
        nack_repairs_this_period: nack_repairs_this_period_cell,
    } = ctx;
    let mut prev_ack = *prev_ack_cell;
    let mut c8c_last_ack_adv_us = *c8c_last_ack_adv_us_cell;
    let mut nack_repairs_this_period = *nack_repairs_this_period_cell;
    if let Some((first_acked, ack)) = cumulative_advance(prev_ack, next_expected) {
        // `ack` is the highest newly delivered seq (inclusive) -- the v8
        // meaning every prune below is written against: `split_off(&(ack +
        // 1))` = `split_off(&next_expected)`.
        //
        // DIAG: attribute the just-ended frontier stall (time since the
        // previous cumulative advance, ≥ 5 ms) to the original placement path
        // of the hole that was blocking (seq = `first_acked`, the old
        // cumulative point) — read before the cleanup below prunes
        // source_path_map to ack+1.
        if pol.diag_on {
            let nowa = now_us();
            if c8c_last_ack_adv_us > 0 {
                let dt_us = nowa.saturating_sub(c8c_last_ack_adv_us);
                if dt_us >= 5_000 {
                    if let Some(&owner) = st.source_path_map.get(&first_acked) {
                        *dg.c8c_stall_ms.entry(owner).or_insert(0) += dt_us / 1000;
                        *dg.c8c_stall_n.entry(owner).or_insert(0) += 1;
                    }
                }
            }
            c8c_last_ack_adv_us = nowa;
        }
        // Reduce repair_debt proportionally — ACK'd symbols no longer need proactive coverage
        let newly_acked = next_expected - prev_ack;
        // Compute the repair rate and the derived window target (paper §4.8)
        // from the worst (highest-loss) channel path, under a single lock
        // acquisition.
        let derived_window = {
            let mut ctrl = fec_controller.lock();
            let sched = scheduler.lock();
            let path_est = worst_eps_estimator(&sched);
            // χ, the completion exposure (`RWM_COMPLETION_EXPOSURE`, paper
            // §4.6). The perf client knows the remaining bytes of the object
            // it is feeding. Under the gate it publishes them into a
            // `CompletionFeed` and this site converts them to a time —
            // `T_rem = remaining / throughput` — and prices the exposure with
            // the math crate's `completion_exposure(T_rem, srtt, rttvar)`.
            // Without it χ ≡ 0 and the Bulk-end glide never runs.
            //
            // The engine's estimator exposes a smoothed RTT and no RTTVAR, so
            // `0.125·srtt` stands in for it: RFC 6298's steady-state relation
            // (β = ¼ updates settle near RTTVAR ≈ srtt/8 on a steady link). It
            // is the arm's own constant, not a derived quantity.
            // `completion_exposure` floors σ_ARQ at `srtt/4` regardless.
            //
            // Absent by default: this block is skipped and χ stays 0.
            if let (true, Some(feed), Some(est)) =
                (gates.completion_exposure, completion_feed.as_ref(), path_est)
            {
                let tput = est.throughput();
                let srtt = est.rtt().as_secs_f64();
                let chi = if tput > 0.0 && srtt > 0.0 {
                    let t_rem = feed.remaining_bytes() as f64 / tput;
                    raptorpath_math::completion_exposure(t_rem, srtt, 0.125 * srtt)
                } else {
                    0.0
                };
                ctrl.set_completion_exposure(chi);
                CHI.observe(chi);
            }
            path_est.and_then(|est| ctrl.derive_window(est))
        };
        // The cadenced rate (Stage 1c): read after χ is published above, and
        // evaluated outside the locks on a miss.
        let repair_rate = super::emit_source::cadenced_repair_rate(
            &mut st.rate_cache,
            fec_controller,
            scheduler,
            st.encoder.window_size(),
            now_us(),
        );
        let debt_reduction = newly_acked as f64 * repair_rate;
        st.repair_debt = (st.repair_debt - debt_reduction).max(0.0);

        // Keep the encoder window at the derived W* (paper §4.8), bounded by
        // the sender's hard ceiling; fall back to win_cap/2 when the
        // estimator has no throughput/RTT sample yet (cold start).
        // Generation mode advances by generation: the cumulative ack passes
        // a seq only when its whole generation has decoded and delivered
        // contiguously, so everything at or below `ack` is done — drop those
        // generations (advance gen-aligns internally). No W*-behind retention
        // (the coding target is the generation, not a sliding W).
        if generation {
            st.encoder.advance(ack + 1);
            // GLIFE: fold completed generations into the lifecycle sums
            // (fill = first-source→sealed, code = sealed→last-emit,
            // wait = last-emit→acked). RWM_DIAG only.
            if pol.diag_on {
                let now_g = now_us();
                let done: Vec<u64> = st.gl
                    .keys()
                    .copied()
                    .filter(|&a| a + pol.gen_size as u64 <= ack + 1)
                    .collect();
                for a in done {
                    if let Some((f, s, e)) = st.gl.remove(&a) {
                        if f > 0 && s >= f && e >= s {
                            dg.gl_sum.0 += s - f;
                            dg.gl_sum.1 += e - s;
                            dg.gl_sum.2 += now_g.saturating_sub(e);
                            dg.gl_sum.3 += 1;
                        }
                    }
                }
            }
            // Drop per-generation deficit bookkeeping for generations that
            // have now been fully delivered + dropped (anchors below the
            // retained window start). Keeps the maps bounded to the M
            // in-flight generations.
            let (win_start, _) = st.encoder.window_span();
            gen_want.retain(|&a, _| a >= win_start);
            gen_emitted.retain(|&a, _| a >= win_start);
            gen_emitted_at_report.retain(|&a, _| a >= win_start);
            gen_recover_at.retain(|&a, _| a >= win_start);
        } else {
            let keep_behind = derived_window
                .map(|w| w.clamp(16, pol.win_cap))
                .unwrap_or(pol.win_cap / 2) as u64;
            st.encoder.advance(ack.saturating_sub(keep_behind));
        }

        // The request bookkeeping's own prune (paper §7.6), the parallel of
        // the generation maps' `retain` above. A span anchor at or below the
        // cumulative ack is delivered — the receiver will never ask about it
        // again — so its want, its emission count and its in-flight baseline
        // are dead. Without this the three maps grow with the transfer rather
        // than with the outstanding window, because a report's anchor is the
        // receiver's first missing seq. Empty (and free) on a disarmed run.
        if !req_want.is_empty() || !req_emitted.is_empty() {
            req_want.retain(|&a, _| a > ack);
            req_emitted.retain(|&a, _| a > ack);
            req_at_report.retain(|&a, _| a > ack);
        }

        // Reset budget period counters on significant window advancement
        if newly_acked >= 10 {
            nack_repairs_this_period = 0;
            st.source_symbols_this_period = 0;
        }

        // Clean up source_path_map and retransmit buffer for ACKed/evicted
        // sequences. Reliable mode keeps path attribution for everything
        // still in the store (aged holes retransmit cross-path too).
        let (win_start, _) = st.encoder.window_span();
        let path_map_floor = if reliable { ack + 1 } else { win_start };
        st.source_path_map.retain(|&seq, _| seq >= path_map_floor);
        // Remove ACKed symbols from retransmit buffer (all seqs <= ack)
        st.retransmit_buffer = st.retransmit_buffer.split_off(&(ack + 1));
        // δ-honest shed set: pruned on the same cumulative twin (the
        // receiver's frontier passing a shed seq closes its story).
        if !st.shed_seqs.is_empty() {
            st.shed_seqs = st.shed_seqs.split_off(&(ack + 1));
        }
        // The sent-data store is drained by acks only — this is the whole
        // retention contract.
        st.sent_store = st.sent_store.split_off(&(ack + 1));
        // RWM_STORE_SACK_RELEASE: the released-mark set prunes on the same
        // cumulative twin — at/below the frontier the slot is fully freed
        // (payload dropped above, mark dropped here); the subset-of-sent_store
        // invariant is preserved. No-op when off.
        if !sack_released.is_empty() {
            sack_release_prune(&mut sack_released, next_expected);
        }
        // Cumulative release of the per-path accounts (the split_off twin;
        // seqs already SACK-released are gone from the account map, so no
        // double-release).
        if pol.percap_track {
            percap_release_cumulative(&mut st.percap_acct, &mut st.percap_out, next_expected);
        }
        // Hold-down (paper §7.4): drop the stamped holes the frontier has
        // passed. Prune only — the estimator is fed off the receiver's own gap
        // report (`on_report`), never off the cumulative frontier, because the
        // frontier cannot pass a hole until every earlier hole is filled and
        // that sample is head-of-line lag rather than this hole's resolution.
        hold_echo.on_retired(ack);
        // Drop NACK-retransmit cooldown entries for delivered seqs.
        st.nack_retx_at.retain(|&seq, _| seq > ack);
        // Drop packet-threshold evidence the frontier passed (counts are only
        // ever taken above a live gap, and gaps are above the frontier).
        if pol.recov_mp_law {
            for v in mp_delivered.values_mut() {
                let idx = v.partition_point(|&x| x <= ack);
                v.drain(..idx);
            }
        }
        // Update correction deficit: ACKed symbols no longer need coverage
        {
            let mut sched = scheduler.lock();
            sched.deficit.on_ack_cumulative(ack);
        }
        // Reset taper offset on window advancement (new correction cycle)
        st.taper_offset = 0;

        prev_ack = next_expected;
    }
    *prev_ack_cell = prev_ack;
    *c8c_last_ack_adv_us_cell = c8c_last_ack_adv_us;
    *nack_repairs_this_period_cell = nack_repairs_this_period;
}
