//! L0 micro-bench of the generation coding machine ALONE (decode-CPU ceiling,
//! JOB 1). No networking, no tokio — pure single-thread CPU attribution of the
//! `GenerationEncoder`/`GenerationDecoder` pair on L1-shaped parameters
//! (G=384, S=1200 bulk symbols, r=0.03, ε≈2.6 % GE-class loss).
//!
//! `#[ignore]` — measurement instrument, not a CI gate.
//!
//! ```text
//! cargo test --test gen_decode_bench --release -- --ignored --nocapture
//! ```
//!
//! Env knobs:
//!   RWM_B_GENS  generations per scenario (default 30)
//!   RWM_B_G     generation size G (default 384)
//!   RWM_B_S     symbol size S (default 1200 = bulk profile)
//!   RWM_B_EPS   iid loss rate on the wire stream (default 0.026 ≈ c2)
//!   RWM_B_R     proactive overhead r (default 0.03)
//!   RWM_B_LATE  fraction of surviving sources arriving AFTER the repairs
//!               (reorder/lateness → the #59 injection path; default 0.10)
//!   RWM_B_IMPL  new | ref | both (default both)
//!
//! Buckets (decode-side attribution):
//!   src      source symbol, its generation has NO matrix yet (unit delivery)
//!   src+mat  source symbol AFTER its generation's matrix exists (#59 injection)
//!   rep0     FIRST repair of a generation (slot creation + first elimination)
//!   rep      subsequent repairs (dense-row elimination)

use std::time::Instant;

use raptorpath::fec::reference::RefGenerationDecoder;
use raptorpath::fec::{GenerationDecoder, GenerationEncoder, WindowDecoder, WindowEncoder, WireSymbol};

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name).ok().and_then(|s| s.parse().ok()).unwrap_or(default)
}
fn env_f64(name: &str, default: f64) -> f64 {
    std::env::var(name).ok().and_then(|s| s.parse().ok()).unwrap_or(default)
}

fn payload(seq: u64, s: usize) -> Vec<u8> {
    (0..s).map(|j| (seq as u8).wrapping_mul(31).wrapping_add((j as u8).wrapping_mul(7))).collect()
}

/// SplitMix64 (same generator the codebase uses for coefficients).
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }
    fn chance(&mut self, p: f64) -> bool {
        (self.next() as f64 / u64::MAX as f64) < p
    }
}

/// One wire event of a pre-built trace: the symbol plus its attribution key.
struct Ev {
    sym: WireSymbol,
    anchor: u64, // generation anchor (u64::MAX for sources — derived from seq)
    is_repair: bool,
}

struct Trace {
    events: Vec<Ev>,
    n_source_wire: usize,
    n_repair_wire: usize,
    total_source: u64, // all source seqs that must come out
    enc_wall_us: u64,  // encoder CPU spent building the coded symbols
    enc_coded: u64,
}

/// Build a SYSTEMATIC-mode trace: raw source rides the wire (minus ε loss, a
/// `late` fraction reordered to after the repairs), proactive repair
/// ceil(G·r) per generation, deficit top-up until every hole is coverable.
/// `fill_frac > 0` additionally emits filling-generation (FILL_FLAG) repairs
/// mid-fill, exercising the (anchor, G) early-matrix + injection path hard.
fn build_systematic_trace(
    n_gens: usize,
    g: usize,
    s: usize,
    eps: f64,
    r: f64,
    late: f64,
    fill_frac: f64,
    seed: u64,
) -> Trace {
    let mut enc = GenerationEncoder::new_systematic(s as u16, g, n_gens.max(2), r);
    let mut rng = Rng(seed);
    let mut events: Vec<Ev> = Vec::new();
    let mut enc_wall_us = 0u64;
    let mut enc_coded = 0u64;

    for gen in 0..n_gens as u64 {
        let anchor = gen * g as u64;
        let mut on_time: Vec<Ev> = Vec::new();
        let mut late_src: Vec<Ev> = Vec::new();
        let mut fills: Vec<Ev> = Vec::new();
        let mut holes = 0usize;
        for i in 0..g as u64 {
            let seq = anchor + i;
            let sym = enc.add_source(&payload(seq, s));
            // Mid-fill FILL_FLAG repairs (the present-at-stall pacer).
            if fill_frac > 0.0 && rng.chance(fill_frac) && enc.wants_filling_coding() {
                let t = Instant::now();
                let rep = enc.generate_repair_filling();
                enc_wall_us += t.elapsed().as_micros() as u64;
                enc_coded += 1;
                if !rng.chance(eps) {
                    fills.push(Ev { sym: rep, anchor, is_repair: true });
                }
            }
            if rng.chance(eps) {
                holes += 1; // lost on the wire
            } else if rng.chance(late) {
                late_src.push(Ev { sym, anchor, is_repair: false });
            } else {
                on_time.push(Ev { sym, anchor, is_repair: false });
            }
        }
        // Proactive repair at seal: drain the remaining per-generation budget.
        let mut reps: Vec<Ev> = Vec::new();
        loop {
            if !enc.wants_coding() {
                break;
            }
            let t = Instant::now();
            let rep = enc.generate_repair();
            enc_wall_us += t.elapsed().as_micros() as u64;
            enc_coded += 1;
            if !rng.chance(eps) {
                reps.push(Ev { sym: rep, anchor, is_repair: true });
            }
        }
        // Deficit top-up (the reactive round): enough surviving FULL-WIDTH
        // repair DoF to cover every hole, +2 margin against coefficient
        // dependence.  FILL_FLAG repairs only span their emission-time prefix,
        // so they do not count toward covering arbitrary holes.
        let surviving = reps.len();
        if holes + 2 > surviving {
            for _ in 0..(holes + 2 - surviving) {
                let t = Instant::now();
                let rep = enc.generate_repair_for(anchor).expect("sealed generation retained");
                enc_wall_us += t.elapsed().as_micros() as u64;
                enc_coded += 1;
                reps.push(Ev { sym: rep, anchor, is_repair: true });
            }
        }
        // Arrival order: fills interleave the fill (already positioned), then
        // on-time source, then the sealed repairs, then the LATE source (the
        // injection path), then nothing else (deficit reps ride with reps).
        events.extend(fills);
        events.extend(on_time);
        events.extend(reps);
        events.extend(late_src);
    }
    let n_source_wire = events.iter().filter(|e| !e.is_repair).count();
    let n_repair_wire = events.len() - n_source_wire;
    Trace {
        events,
        n_source_wire,
        n_repair_wire,
        total_source: (n_gens * g) as u64,
        enc_wall_us,
        enc_coded,
    }
}

/// Build a CODED-ONLY trace (the current L1 gen-arm wire): NO raw source —
/// every wire symbol is a dense combination; budget ceil(G·(1+r)) per
/// generation, ε loss, deficit top-up to full rank (+margin).
fn build_coded_only_trace(n_gens: usize, g: usize, s: usize, eps: f64, r: f64, seed: u64) -> Trace {
    let mut enc = GenerationEncoder::new(s as u16, g, n_gens.max(2), r);
    let mut rng = Rng(seed);
    let mut events: Vec<Ev> = Vec::new();
    let mut enc_wall_us = 0u64;
    let mut enc_coded = 0u64;

    for gen in 0..n_gens as u64 {
        let anchor = gen * g as u64;
        for i in 0..g as u64 {
            enc.add_source(&payload(anchor + i, s));
        }
        let mut survived = 0usize;
        loop {
            if !enc.wants_coding() {
                break;
            }
            let t = Instant::now();
            let rep = enc.generate_repair();
            enc_wall_us += t.elapsed().as_micros() as u64;
            enc_coded += 1;
            if !rng.chance(eps) {
                events.push(Ev { sym: rep, anchor, is_repair: true });
                survived += 1;
            }
        }
        // Deficit top-up to G + 2 margin (dependent rows are possible).
        while survived < g + 2 {
            let t = Instant::now();
            let rep = enc.generate_repair_for(anchor).expect("sealed generation retained");
            enc_wall_us += t.elapsed().as_micros() as u64;
            enc_coded += 1;
            events.push(Ev { sym: rep, anchor, is_repair: true });
            survived += 1;
        }
    }
    let n = events.len();
    Trace {
        events,
        n_source_wire: 0,
        n_repair_wire: n,
        total_source: (n_gens * g) as u64,
        enc_wall_us,
        enc_coded,
    }
}

#[derive(Default, Clone, Copy)]
struct Bucket {
    n: u64,
    us: u64,
}

fn run_decode<D: WindowDecoder>(dec: &mut D, trace: &Trace) -> (u64, u64, [Bucket; 4]) {
    // buckets: 0=src 1=src+mat 2=rep0 3=rep
    let mut buckets = [Bucket::default(); 4];
    let mut has_matrix: std::collections::HashSet<u64> = Default::default();
    let mut delivered = 0u64;
    let t_all = Instant::now();
    for ev in &trace.events {
        let b = if ev.is_repair {
            if has_matrix.insert(ev.anchor) {
                2
            } else {
                3
            }
        } else if has_matrix.contains(&ev.anchor) {
            1
        } else {
            0
        };
        let t = Instant::now();
        let out = dec.add_symbol(&ev.sym);
        let dt = t.elapsed().as_micros() as u64;
        buckets[b].n += 1;
        buckets[b].us += dt;
        delivered += out.len() as u64;
    }
    let total_us = t_all.elapsed().as_micros() as u64;
    (delivered, total_us, buckets)
}

fn scenario<D: WindowDecoder>(name: &str, impl_name: &str, dec: &mut D, trace: &Trace, s: usize) {
    let (delivered, total_us, b) = run_decode(dec, trace);
    assert_eq!(
        delivered, trace.total_source,
        "{name}/{impl_name}: delivered {delivered} != total source {}",
        trace.total_source
    );
    let symps = delivered as f64 / (total_us as f64 / 1e6);
    let mbps = symps * s as f64 * 8.0 / 1e6;
    eprintln!(
        "{name:<22} {impl_name:<4} decode {total_ms:>8.1} ms  {symps:>9.0} sym/s ({mbps:>7.1} Mbit/s @S={s})  \
         wire src={ns} rep={nr}",
        total_ms = total_us as f64 / 1e3,
        ns = trace.n_source_wire,
        nr = trace.n_repair_wire,
    );
    let names = ["src", "src+mat", "rep0", "rep"];
    for (i, bu) in b.iter().enumerate() {
        if bu.n > 0 {
            eprintln!(
                "    {:<8} n={:>6}  total {:>9.1} ms  mean {:>8.1} us",
                names[i],
                bu.n,
                bu.us as f64 / 1e3,
                bu.us as f64 / bu.n as f64
            );
        }
    }
}

#[test]
#[ignore = "measurement instrument (decode-CPU ceiling JOB 1), not a CI gate"]
fn gen_decode_bench() {
    let n_gens = env_usize("RWM_B_GENS", 30);
    let g = env_usize("RWM_B_G", 384);
    let s = env_usize("RWM_B_S", 1200);
    let eps = env_f64("RWM_B_EPS", 0.026);
    let r = env_f64("RWM_B_R", 0.03);
    let late = env_f64("RWM_B_LATE", 0.10);
    let which = std::env::var("RWM_B_IMPL").unwrap_or_else(|_| "both".into());

    eprintln!(
        "--- gen_decode_bench: gens={n_gens} G={g} S={s} eps={eps} r={r} late={late} impl={which}"
    );

    // ── GF(256) kernel throughput (the SIMD floor everything divides by) ──
    {
        let fused = g + s;
        let mut rows: Vec<Vec<u8>> = (0..g).map(|i| payload(i as u64, fused)).collect();
        let src = payload(9999, fused);
        // warm
        for row in rows.iter_mut() {
            gf256::mul_acc_slice(7, &src, row);
        }
        let iters = 20_000usize;
        let t = Instant::now();
        for i in 0..iters {
            let row = &mut rows[i % g];
            gf256::mul_acc_slice(7, &src, row);
        }
        let dt = t.elapsed().as_secs_f64();
        let gbs = (iters * fused) as f64 / dt / 1e9;
        eprintln!(
            "kernel mul_acc_slice   len={fused}  {gbs:.2} GB/s  ({:.2} us/row)",
            dt / iters as f64 * 1e6
        );
    }

    // ── traces ──
    let seed = 42u64;
    let tr_sys = build_systematic_trace(n_gens, g, s, eps, r, late, 0.0, seed);
    let tr_sys_clean = build_systematic_trace(n_gens, g, s, 0.0, r, late, 0.0, seed);
    let tr_sys_inorder = build_systematic_trace(n_gens, g, s, 0.0, r, 0.0, 0.0, seed);
    let tr_fill = build_systematic_trace(n_gens, g, s, eps, r, late, 0.05, seed);
    let tr_coded = build_coded_only_trace(n_gens, g, s, eps, r, seed);

    for (name, tr) in [
        ("sys eps", &tr_sys),
        ("sys clean late", &tr_sys_clean),
        ("sys clean inorder", &tr_sys_inorder),
        ("sys fill eps", &tr_fill),
        ("coded-only eps", &tr_coded),
    ] {
        eprintln!(
            "[encoder] {name:<16} coded={:>6}  enc {:>8.1} ms  ({:>6.1} us/coded sym; {:.0} coded sym/s)",
            tr.enc_coded,
            tr.enc_wall_us as f64 / 1e3,
            tr.enc_wall_us as f64 / tr.enc_coded.max(1) as f64,
            tr.enc_coded as f64 / (tr.enc_wall_us as f64 / 1e6),
        );
        if which != "new" {
            let mut d = RefGenerationDecoder::new(s as u16);
            scenario(name, "ref", &mut d, tr, s);
        }
        if which != "ref" {
            let mut d = GenerationDecoder::new(s as u16);
            scenario(name, "new", &mut d, tr, s);
        }
    }
}
