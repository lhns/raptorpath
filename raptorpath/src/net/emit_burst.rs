//! `RWM_EMIT_BATCH`: the burst-bound law, the taper-cache refresh rule and
//! the burst gauges, as pure pieces the sender loop calls.
//!
//! **The bound (Law 0).** A burst is at most one emission quantum of the
//! sender, whatever the live set: `b = emit_burst` at every path count N.
//! There is no `live_paths` input — the scope is continuous in N by
//! construction, and N = 1 is bit-identical to the single-path scope it
//! replaces. The recorded reason for the old `live_paths == 1` step (the
//! wire-v8 global-`batch_seq` striping-gap misread, `c639d56`) is gone in
//! wire v9: the receiver's `PathBatchTracker` keys on the per-path
//! `path_seq`, so a same-path run of any length leaves both paths'
//! `(expected, received)` pairs at `(1, 1)` per delivered symbol
//! (`net::tests::t2_*` bound this).
//!
//! **The cache.** The derived taper/span math refreshes once per `bound`
//! symbols (or 50 ms), so the refresh follows the bound actually in force,
//! not a separate static constant ([`taper_recompute_due`]).
//!
//! **The gauges** (`[DIAG]`, rule 1 + rule 18): `eb_bursts`, `eb_syms`
//! (mean depth = syms / bursts), `eb_end=cap:/store:/tokens:/drained:` (what
//! ended each burst — `cap` is the bound's bind count) and `eb_maxrun` (per
//! path: the longest same-path run inside one burst, max / mean over the
//! bursts that touched that path).

/// The burst bound (Law 0): one emission quantum at every path count.
/// Deliberately takes no path-count or dial input.
#[inline]
pub(crate) fn emit_burst_bound(emit_burst: usize) -> usize {
    emit_burst
}

/// Whether the derived taper/span math must be recomputed for this symbol.
///
/// `batching` off ⇒ always (per symbol, bit-identical to gate-off). On ⇒ a
/// miss when nothing is cached, when the cache has served `bound` symbols
/// (the realised burst bound), or when it is older than `max_age_us`.
#[inline]
pub(crate) fn taper_recompute_due(
    batching: bool,
    cached: bool,
    cache_syms: usize,
    bound: usize,
    age_us: u64,
    max_age_us: u64,
) -> bool {
    !batching || !cached || cache_syms >= bound || age_us > max_age_us
}

/// Why a burst ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BurstEnd {
    /// The bound bound (`burst == b`): the clamp's bind count.
    Cap,
    /// Store headroom exhausted (flow control).
    Store,
    /// Pacing bucket dry (`cc_pace`).
    Tokens,
    /// The TUN intake had nothing queued.
    Drained,
}

/// The burst gauge (DIAG-only; cumulative, last-line-wins).
#[derive(Debug, Default)]
pub(crate) struct EmitBurstGauge {
    pub bursts: u64,
    pub syms: u64,
    pub end: [u64; 4],
    /// Per path: (path id, longest same-path run in any burst, Σ per-burst
    /// longest run, bursts that placed at least one symbol on the path).
    runs: Vec<(u32, u32, u64, u64)>,
    // ── the open burst ──
    open: bool,
    cur_path: u32,
    cur_run: u32,
    burst_syms: u32,
    burst_max: Vec<(u32, u32)>,
}

impl EmitBurstGauge {
    /// A burst starts (before its first symbol).
    pub fn begin(&mut self) {
        self.open = true;
        self.cur_run = 0;
        self.burst_syms = 0;
        self.burst_max.clear();
    }

    /// One symbol of the open burst was placed on `path`.
    pub fn on_symbol(&mut self, path: u32) {
        if !self.open {
            return;
        }
        if self.burst_syms > 0 && path == self.cur_path {
            self.cur_run += 1;
        } else {
            self.cur_path = path;
            self.cur_run = 1;
        }
        self.burst_syms += 1;
        match self.burst_max.iter_mut().find(|(p, _)| *p == path) {
            Some((_, m)) => *m = (*m).max(self.cur_run),
            None => self.burst_max.push((path, self.cur_run)),
        }
    }

    /// The open burst ended for `why`.
    pub fn end(&mut self, why: BurstEnd) {
        if !self.open {
            return;
        }
        self.open = false;
        self.bursts += 1;
        self.syms += self.burst_syms as u64;
        self.end[why as usize] += 1;
        for &(p, m) in &self.burst_max {
            match self.runs.iter_mut().find(|r| r.0 == p) {
                Some(r) => {
                    r.1 = r.1.max(m);
                    r.2 += m as u64;
                    r.3 += 1;
                }
                None => self.runs.push((p, m, m as u64, 1)),
            }
        }
    }

    /// Mean burst depth (symbols per burst); 0 before the first burst.
    pub fn mean_depth(&self) -> f64 {
        if self.bursts == 0 {
            0.0
        } else {
            self.syms as f64 / self.bursts as f64
        }
    }

    /// Per path `(id, max run, mean per-burst longest run)`, sorted by id.
    pub fn maxrun(&self) -> Vec<(u32, u32, f64)> {
        let mut v: Vec<(u32, u32, f64)> = self
            .runs
            .iter()
            .map(|r| (r.0, r.1, if r.3 > 0 { r.2 as f64 / r.3 as f64 } else { 0.0 }))
            .collect();
        v.sort_by_key(|r| r.0);
        v
    }

    /// The `[DIAG]` token (leading space). Printed whatever the gate, so a
    /// gate-off row reads `eb_bursts=0` (the two-sided witness).
    pub fn diag_token(&self) -> String {
        let runs = self.maxrun();
        let runs = if runs.is_empty() {
            "-".to_string()
        } else {
            runs.iter()
                .map(|(p, m, mean)| format!("{p}:{m}/{mean:.2}"))
                .collect::<Vec<_>>()
                .join(",")
        };
        format!(
            " eb_bursts={} eb_syms={} eb_depth={:.2} eb_end=cap:{}/store:{}/tokens:{}/drained:{} eb_maxrun={}",
            self.bursts,
            self.syms,
            self.mean_depth(),
            self.end[BurstEnd::Cap as usize],
            self.end[BurstEnd::Store as usize],
            self.end[BurstEnd::Tokens as usize],
            self.end[BurstEnd::Drained as usize],
            runs,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Law 0: the bound is the quantum, exactly, for every quantum the gate
    /// can resolve (`RWM_EMIT_BURST` clamps to [2, 512]). No other input.
    #[test]
    fn law0_bound_is_the_quantum_at_every_value() {
        for q in 2..=512 {
            assert_eq!(emit_burst_bound(q), q);
        }
    }

    /// T4: the cache refresh follows the realised bound. Drive the rule
    /// symbol by symbol (as `emit_source` does: a miss stores the cache and
    /// resets the served count to 1, a hit increments it) and count misses.
    fn misses(batching: bool, bound: usize, symbols: usize, dt_us: u64) -> usize {
        let (mut cached, mut syms, mut at, mut n) = (false, 0usize, 0u64, 0usize);
        for i in 0..symbols {
            let now = i as u64 * dt_us;
            if taper_recompute_due(batching, cached, syms, bound, now - at, 50_000) {
                n += 1;
                cached = true;
                syms = 1;
                at = now;
            } else {
                syms += 1;
            }
        }
        n
    }

    #[test]
    fn t4_a_burst_of_b_symbols_gets_exactly_one_recompute() {
        for b in [2usize, 8, 33, 64, 512] {
            // One burst of exactly b symbols, fast: one recompute.
            assert_eq!(misses(true, b, b, 1), 1, "b={b}");
            // k bursts of b: k recomputes — the refresh follows b, not 64.
            for k in 1..=5 {
                assert_eq!(misses(true, b, k * b, 1), k, "b={b} k={k}");
            }
        }
        // A bound of 8 refreshes 8× as often as a bound of 64 on 512 symbols.
        assert_eq!(misses(true, 8, 512, 1), 64);
        assert_eq!(misses(true, 64, 512, 1), 8);
        // Gate off: every symbol recomputes (bit-identical per-symbol path).
        assert_eq!(misses(false, 64, 100, 1), 100);
        // The 50 ms staleness bound still binds on a slow source.
        assert_eq!(misses(true, 64, 10, 60_000), 10);
    }

    #[test]
    fn gauge_counts_depth_ends_and_runs() {
        let mut g = EmitBurstGauge::default();
        assert_eq!(g.mean_depth(), 0.0);
        assert!(g.diag_token().contains("eb_bursts=0 eb_syms=0"));
        // Burst 1: A A A B A  → depth 5, runs A:3, B:1, ended by the cap.
        g.begin();
        for p in [0, 0, 0, 1, 0] {
            g.on_symbol(p);
        }
        g.end(BurstEnd::Cap);
        // Burst 2: B B → depth 2, B:2, drained.
        g.begin();
        g.on_symbol(1);
        g.on_symbol(1);
        g.end(BurstEnd::Drained);
        // Burst 3: depth 1 on A, drained.
        g.begin();
        g.on_symbol(0);
        g.end(BurstEnd::Drained);
        assert_eq!((g.bursts, g.syms), (3, 8));
        assert!((g.mean_depth() - 8.0 / 3.0).abs() < 1e-12);
        assert_eq!(g.end, [1, 0, 0, 2]);
        let r = g.maxrun();
        assert_eq!(r[0].0, 0);
        assert_eq!(r[0].1, 3);
        assert!((r[0].2 - 2.0).abs() < 1e-12); // (3 + 1) / 2
        assert_eq!(r[1].0, 1);
        assert_eq!(r[1].1, 2);
        assert!((r[1].2 - 1.5).abs() < 1e-12); // (1 + 2) / 2
        let t = g.diag_token();
        assert!(t.contains("eb_bursts=3 eb_syms=8 eb_depth=2.67"), "{t}");
        assert!(t.contains("eb_end=cap:1/store:0/tokens:0/drained:2"), "{t}");
        assert!(t.contains("eb_maxrun=0:3/2.00,1:2/1.50"), "{t}");
        // Symbols outside an open burst are not counted.
        g.on_symbol(0);
        g.end(BurstEnd::Cap);
        assert_eq!((g.bursts, g.syms), (3, 8));
    }

    /// The scope pin: the batching path reads no path count. The burst block
    /// in the sender loop (between the `[EMIT-BURST-BEGIN]`/`[EMIT-BURST-END]`
    /// markers) and the whole emission step contain no `live_paths` read, and
    /// the per-iteration `emit_batch_live` scope no longer exists anywhere in
    /// the sender (the old `live_paths_iter().count() == 1` step).
    #[test]
    fn the_batching_path_reads_no_path_count() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/net");
        let code = |f: &str| -> String {
            std::fs::read_to_string(root.join(f))
                .unwrap_or_else(|e| panic!("read {f}: {e}"))
                .lines()
                .map(|l| l.split("//").next().unwrap_or(""))
                .collect::<Vec<_>>()
                .join("\n")
        };
        let m = code("mod.rs");
        let raw = std::fs::read_to_string(root.join("mod.rs")).unwrap();
        let a = raw.find("[EMIT-BURST-BEGIN]").expect("begin marker");
        let b = raw.find("[EMIT-BURST-END]").expect("end marker");
        assert!(a < b);
        let block: String = raw[a..b]
            .lines()
            .map(|l| l.split("//").next().unwrap_or(""))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(block.contains("emit_burst_bound"), "the burst loop must take Law 0's bound");
        assert!(!block.contains("live_paths"), "the burst block reads a path count");
        assert!(!code("emit_source.rs").contains("live_paths"), "the emission step reads a path count");
        for f in ["mod.rs", "emit_source.rs", "sender_policy.rs"] {
            let c = if f == "mod.rs" { m.clone() } else { code(f) };
            assert!(
                !c.contains("emit_batch_live"),
                "{f}: the per-iteration path-count scope `emit_batch_live` is back"
            );
        }
    }
}
