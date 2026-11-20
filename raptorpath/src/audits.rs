//! Static source-walk audits (test detection plan, S2a and S2b). `#[cfg(test)]`
//! only: nothing here reaches the engine binary.
//!
//! **S2a, production-caller audit.** Every `pub` / `pub(..)` fn defined outside
//! test code needs a call site outside test code (`#[cfg(test)]` items,
//! `#[cfg(any(test, feature = "test-seams"))]` items, `*tests.rs` files, files
//! declared by a `#[cfg(test)] mod x;`), or it must be listed with a reason in
//! [`ALLOWLIST`], or sit in [`PENDING_TRIAGE`] (no reason yet: a human decides
//! whether the fn is dead, a missing wiring, or fine).
//!
//! **S2b, ignored-parameter audit.** rustc never warns on a `_`-prefixed name,
//! so `fn f(_x: T)` hides a parameter the caller fills in and the callee
//! drops. Flags every `_name` parameter that the body never reads and that at
//! least one call site fills with a non-constant argument.
//!
//! Both are text-based (no `syn`: no new dependency). Limits, so nobody reads a
//! pass as a proof:
//! * Name matching, not type resolution. A method is "called" when some
//!   `.name(` appears, whatever the receiver, so a same-named method on another
//!   type gives a false pass (`new`, `len`, `get` are always "called").
//!   Associated fns are matched on `Type::name` / `Self::name` only.
//! * A fn called only by another uncalled fn counts as called (no transitive
//!   deadness). A fn reached only through a re-export alias
//!   (`use X as Y; Y::f()`) is reported uncalled: a false positive.
//! * Callers in `tests/`, `benches/`, other crates and the `test-seams`
//!   feature do not count, by design.
//! * Macro-generated fns, trait-impl fns (not `pub`) and fns named inside
//!   `macro_rules!` bodies are not seen.
//!
//! The scan is a pure function of `(path, source)` pairs, so the fixtures at
//! the bottom prove it flags a planted defect and passes a planted control.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

// ---------------------------------------------------------------- lists

/// S2a: uncalled pub fns with a decided reason. Key: `path::[Type::]fn`.
/// Mechanical categories (main entry point, serde `default = "fn"` paths) are
/// computed, not listed. Empty until the coordinator triages.
const ALLOWLIST: &[(&str, &str)] = &[];

/// S2a: uncalled pub fns awaiting triage. No reason on purpose.
const PENDING_TRIAGE: &[&str] = &[
    "control/anchor.rs::SendRateAnchor::mean_rate",
    "control/estimator.rs::LossCredit::held",
    "control/estimator.rs::LossEstimator::bocd_updates",
    "control/estimator.rs::LossEstimator::is_in_burst",
    "control/estimator.rs::LossEstimator::loss_rate_mean",
    "control/estimator.rs::LossEstimator::record_counts",
    "control/estimator.rs::LossEstimator::start_cadence_clock_at",
    "control/estimator.rs::LossEstimator::update_rx_loss",
    "control/fec_rate.rs::BudgetAllocator::proactive_rate",
    "control/fec_rate.rs::BudgetAllocator::total_budget",
    "control/fec_rate.rs::FecRateController::compute_repair_count",
    "control/fec_rate.rs::FecRateController::compute_repair_rate_capped",
    "control/fec_rate.rs::FecRateController::diagnostics",
    "control/fec_rate.rs::FecRateController::feedback_update",
    "control/fec_rate.rs::FecRateController::set_bulk_pure_arq",
    "control/fec_rate.rs::FecRateController::set_saturation_cap",
    "control/fec_rate.rs::FecRateController::set_tail_provision",
    "control/fec_rate.rs::RepairRateCache::get_or_eval",
    "fec/traits.rs::FecBackend::repair_wire_overhead",
    "net/ackdiag.rs::AckCadenceGauge::last_recon",
    "net/ackdiag.rs::AckCadenceGauge::totals",
    "net/channel_set.rs::channel_worst_loss_rate",
    "net/control_msg.rs::repair_request_ignored",
    "net/eta.rs::SenderEta::frontier_eta_us",
    "net/eta.rs::SenderEta::wire_gauges",
    "net/eta.rs::Tlag::ring_len",
    "net/framing.rs::SymbolPacker::should_flush",
    "net/framing.rs::read_packets",
    "net/lat.rs::LatGauge::n_deliv",
    "net/late.rs::RankGauge::is_fed",
    "net/recovery_clock.rs::qnative_recovery_round_us",
    "net/recovery_laws.rs::pooled_recovery_srtt_of",
    "net/recv_block.rs::RecvDiagBlock::final_done",
    "net/rttdump.rs::RttDump::totals",
    "net/sack.rs::SackMemo::intervals",
    "net/sack.rs::compute_gap_ranges",
    "net/sack.rs::sack_release_mark",
    "net/seq_bits.rs::SeqBits::refused",
    "net/shed.rs::CompletionFeed::drain_bps",
    "net/snapver.rs::SnapOrder::newest",
    "net/store_cap.rs::path_scaled_store_cap",
    "net/store_cap.rs::store_cap_sf_reset",
    "net/succ.rs::SuccGauge::det_n",
    "net/succ.rs::SuccGauge::hist",
    "net/succ.rs::SuccGauge::observe_high",
    "net/succ.rs::SuccGauge::orig_frac",
    "net/succ.rs::SuccGauge::over_n",
    "net/succ.rs::SuccGauge::resolve",
    "net/succ.rs::SuccGauge::rtx_n",
    "net/succ.rs::SuccGauge::sp_n",
    "net/succ.rs::SuccGauge::xp_n",
    "runtime_obs.rs::armed",
    "scheduler/mod.rs::Scheduler::cold_place",
    "scheduler/mod.rs::Scheduler::on_loss",
    "scheduler/mod.rs::Scheduler::place_wdiv_derived",
    "scheduler/mod.rs::Scheduler::schedule",
    "scheduler/mod.rs::Scheduler::set_cold_place",
    "scheduler/mod.rs::Scheduler::set_place_wdiv_derived",
    "scheduler/mod.rs::Scheduler::set_protocol_hint",
    "scheduler/mod.rs::lcw_reset",
    "scheduler/path.rs::PathState::loss_clamp_witness",
    "scheduler/path.rs::PathState::sender_truth_release_batch",
    "task_obs.rs::RxProbe::counters",
    "transport/io_owner.rs::first_identity_violation",
    "transport/io_owner.rs::identity_counts",
    "transport/quic.rs::QuicTransport::cc_window_bytes",
    "transport/quic.rs::QuicTransport::rcvbuf_grant",
    "transport/quic.rs::QuicTransport::rx_socket_rcvbuf",
    "tun/mod.rs::TunInterface::write_packet",
];

/// S2b: ignored `_param`s with a decided reason. Key: `path::[Type::]fn::param`.
const PARAM_ALLOWLIST: &[(&str, &str)] = &[];

/// S2b: ignored `_param`s awaiting triage. No reason on purpose.
const PARAM_PENDING_TRIAGE: &[&str] = &[
    "control/fec_rate.rs::FecRateController::feedback_update_window::_repairs_fed",
    "control/fec_rate.rs::FecRateController::feedback_update_window::_repairs_useful",
    "control/fec_rate.rs::FecRateController::new_with_toggles::_enable_pi_feedback",
    "net/mod.rs::create_window_encoder::_backend",
    "runtime_obs.rs::read_threads::_with_delay",
    "transport/bbr_rs.rs::RsBandwidthEstimation::end_acks::_app_limited",
    "transport/bbr_rs.rs::RsBandwidthEstimation::end_acks::_current_round",
    "transport/bbr_rs.rs::RsBandwidthEstimation::on_sent::_bytes",
    "transport/rcvbuf.rs::force_recv_buffer_size::_requested",
    "transport/rcvbuf.rs::force_recv_buffer_size::_sock",
    "transport/rcvbuf.rs::socket_drops::_sock",
    "tun/mod.rs::TunInterface::create::_config",
];

// ---------------------------------------------------------------- text helpers

fn is_ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// `src` with comments and string/char literal contents overwritten by spaces
/// (newlines kept, quotes kept), byte-for-byte the same length.
fn blank(src: &str) -> String {
    let b = src.as_bytes();
    let n = b.len();
    let mut o = b.to_vec();
    let wipe = |o: &mut Vec<u8>, a: usize, z: usize| {
        for byte in o.iter_mut().take(z).skip(a) {
            if *byte != b'\n' {
                *byte = b' ';
            }
        }
    };
    let mut i = 0;
    while i < n {
        match b[i] {
            b'/' if i + 1 < n && b[i + 1] == b'/' => {
                let s = i;
                while i < n && b[i] != b'\n' {
                    i += 1;
                }
                wipe(&mut o, s, i);
            }
            b'/' if i + 1 < n && b[i + 1] == b'*' => {
                let s = i;
                let mut depth = 0;
                while i < n {
                    if i + 1 < n && b[i] == b'/' && b[i + 1] == b'*' {
                        depth += 1;
                        i += 2;
                    } else if i + 1 < n && b[i] == b'*' && b[i + 1] == b'/' {
                        depth -= 1;
                        i += 2;
                        if depth == 0 {
                            break;
                        }
                    } else {
                        i += 1;
                    }
                }
                wipe(&mut o, s, i);
            }
            b'r' if (i == 0 || !is_ident(b[i - 1]) || (b[i - 1] == b'b' && (i < 2 || !is_ident(b[i - 2]))))
                && raw_string_open(b, i).is_some() =>
            {
                let (hashes, body) = raw_string_open(b, i).unwrap();
                let mut j = body;
                let end;
                loop {
                    if j >= n {
                        end = n;
                        break;
                    }
                    if b[j] == b'"' && (0..hashes).all(|k| j + 1 + k < n && b[j + 1 + k] == b'#') {
                        end = j;
                        break;
                    }
                    j += 1;
                }
                wipe(&mut o, body, end);
                i = (end + 1 + hashes).min(n);
            }
            b'"' => {
                let s = i + 1;
                i += 1;
                while i < n && b[i] != b'"' {
                    if b[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
                wipe(&mut o, s, i.min(n));
                i += 1;
            }
            b'\'' => {
                if i + 1 < n && b[i + 1] == b'\\' {
                    let s = i + 1;
                    i += 2;
                    while i < n && b[i] != b'\'' {
                        i += 1;
                    }
                    wipe(&mut o, s, i.min(n));
                    i += 1;
                } else if i + 2 < n && b[i + 2] == b'\'' {
                    wipe(&mut o, i + 1, i + 2);
                    i += 3;
                } else if i + 1 < n && b[i + 1] >= 0x80 {
                    let mut j = i + 2;
                    while j < n && j < i + 6 && b[j] != b'\'' {
                        j += 1;
                    }
                    wipe(&mut o, i + 1, j.min(n));
                    i = j + 1;
                } else {
                    i += 1; // a lifetime
                }
            }
            _ => i += 1,
        }
    }
    String::from_utf8(o).expect("blanking keeps UTF-8 valid")
}

/// `r"`, `r#"`, `r##"` ... at `i`: (hash count, index of the first body byte).
fn raw_string_open(b: &[u8], i: usize) -> Option<(usize, usize)> {
    let mut j = i + 1;
    let mut h = 0;
    while j < b.len() && b[j] == b'#' {
        h += 1;
        j += 1;
    }
    (j < b.len() && b[j] == b'"').then_some((h, j + 1))
}

/// Index of the closer matching the opener at `open` (`(`, `[`, `{`).
fn match_close(s: &[u8], open: usize) -> Option<usize> {
    let (o, c) = match s[open] {
        b'(' => (b'(', b')'),
        b'[' => (b'[', b']'),
        b'{' => (b'{', b'}'),
        _ => return None,
    };
    let mut d = 0;
    for (k, &ch) in s.iter().enumerate().skip(open) {
        if ch == o {
            d += 1;
        } else if ch == c {
            d -= 1;
            if d == 0 {
                return Some(k);
            }
        }
    }
    None
}

fn skip_ws(s: &[u8], mut i: usize) -> usize {
    while i < s.len() && s[i].is_ascii_whitespace() {
        i += 1;
    }
    i
}

/// Index just past the last non-whitespace byte before `i` (0 if none).
fn back_ws(s: &[u8], mut i: usize) -> usize {
    while i > 0 && s[i - 1].is_ascii_whitespace() {
        i -= 1;
    }
    i
}

/// The identifier ending at `end` (exclusive), if any.
fn word_before(s: &[u8], end: usize) -> Option<(usize, usize)> {
    let mut st = end;
    while st > 0 && is_ident(s[st - 1]) {
        st -= 1;
    }
    (st < end).then_some((st, end))
}

/// Every identifier token: (start, end).
fn tokens(s: &[u8]) -> Vec<(usize, usize)> {
    let mut v = Vec::new();
    let mut i = 0;
    while i < s.len() {
        if is_ident(s[i]) && !s[i].is_ascii_digit() {
            let st = i;
            while i < s.len() && is_ident(s[i]) {
                i += 1;
            }
            v.push((st, i));
        } else if s[i].is_ascii_digit() {
            while i < s.len() && is_ident(s[i]) {
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    v
}

/// Split `s` at top-level commas (depth over `()[]{}<>`, `->` and `=>` skipped).
fn split_top(s: &str) -> Vec<String> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut d = 0i32;
    let mut st = 0;
    for i in 0..b.len() {
        match b[i] {
            b'(' | b'[' | b'{' | b'<' => d += 1,
            b')' | b']' | b'}' => d -= 1,
            b'>' if i > 0 && (b[i - 1] == b'-' || b[i - 1] == b'=') => {}
            b'>' => d -= 1,
            b',' if d == 0 => {
                out.push(s[st..i].to_string());
                st = i + 1;
            }
            _ => {}
        }
    }
    if !s[st..].trim().is_empty() {
        out.push(s[st..].to_string());
    }
    out
}

// ---------------------------------------------------------------- test-code stripping

/// Whether an attribute (`#[...]`, original text) gates its item to test code.
fn is_test_gate(attr: &str) -> bool {
    let a = attr.trim();
    if a == "#[test]" || a == "#[tokio::test]" || a.starts_with("#[tokio::test(") {
        return true;
    }
    if !a.starts_with("#[cfg(") || a.contains("not(") {
        return false;
    }
    if a.contains("\"test-seams\"") {
        return true;
    }
    let b = a.as_bytes();
    tokens(b).iter().any(|&(s, e)| &a[s..e] == "test")
}

/// Blank test-gated items out of `clean` (in place). Returns the names of
/// `mod x;` declarations that were gated (their files are test files).
fn strip_test_items(clean: &mut String, orig: &str) -> Vec<String> {
    let mut mods = Vec::new();
    let bytes = clean.clone().into_bytes();
    let mut out = bytes.clone();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'#' && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            let close = match match_close(&bytes, i + 1) {
                Some(c) => c,
                None => break,
            };
            if is_test_gate(&orig[i..=close]) {
                // The item: further attributes, then up to `{..}` or `;`.
                let mut j = close + 1;
                loop {
                    j = skip_ws(&bytes, j);
                    if j + 1 < bytes.len() && bytes[j] == b'#' && bytes[j + 1] == b'[' {
                        match match_close(&bytes, j + 1) {
                            Some(c) => j = c + 1,
                            None => break,
                        }
                    } else {
                        break;
                    }
                }
                let header_start = j;
                let mut d = 0i32;
                let mut end = bytes.len();
                let mut k = j;
                while k < bytes.len() {
                    match bytes[k] {
                        b'(' | b'[' => d += 1,
                        b')' | b']' => d -= 1,
                        b'{' if d == 0 => {
                            end = match_close(&bytes, k).map_or(bytes.len(), |c| c + 1);
                            break;
                        }
                        b';' if d == 0 => {
                            end = k + 1;
                            break;
                        }
                        _ => {}
                    }
                    k += 1;
                }
                let header = &clean[header_start..end.min(clean.len())];
                if !header.contains('{') {
                    // `mod name;`
                    let toks = tokens(header.as_bytes());
                    for w in toks.windows(2) {
                        if &header[w[0].0..w[0].1] == "mod" {
                            mods.push(header[w[1].0..w[1].1].to_string());
                        }
                    }
                }
                for byte in out.iter_mut().take(end).skip(i) {
                    if *byte != b'\n' {
                        *byte = b' ';
                    }
                }
                i = end;
                continue;
            }
            i = close + 1;
            continue;
        }
        i += 1;
    }
    *clean = String::from_utf8(out).expect("UTF-8");
    mods
}

/// `mod x;` declarations of any kind in `clean` (for children of test files).
fn all_mod_decls(clean: &str) -> Vec<String> {
    let b = clean.as_bytes();
    let toks = tokens(b);
    let mut v = Vec::new();
    for w in toks.windows(2) {
        if &clean[w[0].0..w[0].1] == "mod" {
            let after = skip_ws(b, w[1].1);
            if after < b.len() && b[after] == b';' {
                v.push(clean[w[1].0..w[1].1].to_string());
            }
        }
    }
    v
}

fn child_paths(file: &str, name: &str) -> Vec<String> {
    let (dir, base) = match file.rsplit_once('/') {
        Some((d, f)) => (d.to_string(), f.to_string()),
        None => (String::new(), file.to_string()),
    };
    let sub = if matches!(base.as_str(), "mod.rs" | "lib.rs" | "main.rs") {
        dir
    } else {
        let stem = base.trim_end_matches(".rs");
        if dir.is_empty() { stem.to_string() } else { format!("{dir}/{stem}") }
    };
    let join = |x: &str| if sub.is_empty() { x.to_string() } else { format!("{sub}/{x}") };
    vec![join(&format!("{name}.rs")), join(&format!("{name}/mod.rs"))]
}

// ---------------------------------------------------------------- definitions

#[derive(Clone, Debug, PartialEq)]
enum Ctx {
    Free,
    Inherent(String),
    TraitImpl,
    TraitDef,
}

#[derive(Clone, Debug)]
struct FnDef {
    file: usize,
    name: String,
    name_pos: usize,
    is_pub: bool,
    ctx: Ctx,
    has_self: bool,
    /// (param name if a plain ident, is_self)
    params: Vec<(Option<String>, bool)>,
    body: Option<(usize, usize)>,
}

impl FnDef {
    fn key(&self, path: &str) -> String {
        match &self.ctx {
            Ctx::Inherent(t) => format!("{path}::{t}::{}", self.name),
            _ => format!("{path}::{}", self.name),
        }
    }
}

struct Block {
    start: usize,
    end: usize,
    ctx: Ctx,
}

/// `impl` and `trait` blocks of a (cleaned) file.
fn blocks(s: &str) -> Vec<Block> {
    let b = s.as_bytes();
    let mut v = Vec::new();
    for (st, en) in tokens(b) {
        let w = &s[st..en];
        if w != "impl" && w != "trait" {
            continue;
        }
        let p = back_ws(b, st);
        let item_pos = p == 0
            || matches!(b[p - 1], b'}' | b';' | b'{' | b']')
            || word_before(b, p).is_some_and(|(a, z)| matches!(&s[a..z], "unsafe" | "pub" | "auto"))
            || b[p - 1] == b')';
        if !item_pos {
            continue;
        }
        // Header runs to the first `{` outside parens/brackets.
        let mut d = 0i32;
        let mut k = en;
        let mut open = None;
        while k < b.len() {
            match b[k] {
                b'(' | b'[' => d += 1,
                b')' | b']' => d -= 1,
                b'{' if d == 0 => {
                    open = Some(k);
                    break;
                }
                b';' if d == 0 => break,
                _ => {}
            }
            k += 1;
        }
        let Some(open) = open else { continue };
        let Some(close) = match_close(b, open) else { continue };
        let header = &s[en..open];
        let ctx = if w == "trait" {
            Ctx::TraitDef
        } else if has_for(header) {
            Ctx::TraitImpl
        } else {
            Ctx::Inherent(impl_type(header))
        };
        v.push(Block { start: open, end: close, ctx });
    }
    v
}

/// A `for` (not the `for<'a>` of a higher-ranked bound) in an impl header.
fn has_for(header: &str) -> bool {
    let b = header.as_bytes();
    tokens(b).iter().any(|&(st, en)| {
        &header[st..en] == "for" && {
            let nxt = skip_ws(b, en);
            !(nxt < b.len() && b[nxt] == b'<')
        }
    })
}

/// The last path segment (before any `<`) of the type in an inherent impl header.
fn impl_type(header: &str) -> String {
    let b = header.as_bytes();
    let mut i = skip_ws(b, 0);
    if i < b.len() && b[i] == b'<' {
        let mut d = 0;
        while i < b.len() {
            match b[i] {
                b'<' => d += 1,
                b'>' if i > 0 && b[i - 1] != b'-' => {
                    d -= 1;
                    if d == 0 {
                        i += 1;
                        break;
                    }
                }
                _ => {}
            }
            i += 1;
        }
    }
    let rest = &header[i.min(header.len())..];
    let rb = rest.as_bytes();
    let mut last = String::new();
    let mut k = 0;
    while k < rb.len() {
        if is_ident(rb[k]) {
            let st = k;
            while k < rb.len() && is_ident(rb[k]) {
                k += 1;
            }
            let w = &rest[st..k];
            if !matches!(w, "dyn" | "mut" | "where") {
                last = w.to_string();
            }
            if w == "where" {
                break;
            }
            // A generic argument list ends the path.
            if k < rb.len() && rb[k] == b'<' {
                break;
            }
            if k + 1 < rb.len() && rb[k] == b':' && rb[k + 1] == b':' {
                k += 2;
                continue;
            }
            if !last.is_empty() && !matches!(w, "dyn" | "mut") {
                break;
            }
        } else {
            k += 1;
        }
    }
    last
}

/// Every fn definition of a cleaned file.
fn defs(file: usize, s: &str) -> Vec<FnDef> {
    let b = s.as_bytes();
    let blks = blocks(s);
    let mut out = Vec::new();
    for (st, en) in tokens(b) {
        if &s[st..en] != "fn" {
            continue;
        }
        let ns = skip_ws(b, en);
        if ns >= b.len() || !is_ident(b[ns]) || b[ns].is_ascii_digit() {
            continue; // `fn(` pointer type
        }
        let mut ne = ns;
        while ne < b.len() && is_ident(b[ne]) {
            ne += 1;
        }
        let name = s[ns..ne].to_string();
        // Modifiers and visibility, walking backwards.
        let mut is_pub = false;
        let mut p = back_ws(b, st);
        loop {
            if p == 0 {
                break;
            }
            if b[p - 1] == b'"' {
                // extern "C": the string was blanked; skip it and its opener.
                let mut q = p - 1;
                while q > 0 && b[q - 1] != b'"' {
                    q -= 1;
                }
                p = back_ws(b, q.saturating_sub(1));
                continue;
            }
            if b[p - 1] == b')' {
                let mut q = p - 1;
                let mut d = 0;
                loop {
                    if b[q] == b')' {
                        d += 1;
                    } else if b[q] == b'(' {
                        d -= 1;
                        if d == 0 {
                            break;
                        }
                    }
                    if q == 0 {
                        break;
                    }
                    q -= 1;
                }
                let before = back_ws(b, q);
                if let Some((a, z)) = word_before(b, before) {
                    if &s[a..z] == "pub" {
                        is_pub = true;
                    }
                }
                break;
            }
            match word_before(b, p) {
                Some((a, z)) => {
                    let w = &s[a..z];
                    match w {
                        "pub" => {
                            is_pub = true;
                            break;
                        }
                        "const" | "async" | "unsafe" | "extern" | "default" => p = back_ws(b, a),
                        _ => break,
                    }
                }
                None => break,
            }
        }
        // Generics, parameters.
        let mut k = ne;
        if k < b.len() && b[k] == b'<' {
            let mut d = 0;
            while k < b.len() {
                match b[k] {
                    b'<' => d += 1,
                    b'>' if b[k - 1] != b'-' => {
                        d -= 1;
                        if d == 0 {
                            k += 1;
                            break;
                        }
                    }
                    _ => {}
                }
                k += 1;
            }
        }
        let po = skip_ws(b, k);
        if po >= b.len() || b[po] != b'(' {
            continue;
        }
        let Some(pc) = match_close(b, po) else { continue };
        let mut params = Vec::new();
        let mut has_self = false;
        for (idx, raw) in split_top(&s[po + 1..pc]).iter().enumerate() {
            let t = raw.trim();
            let head = t.split(':').next().unwrap_or("").trim();
            let head_clean = head.trim_start_matches('&').trim_start_matches("mut ").trim();
            let head_clean = head_clean.trim_start_matches('\'').trim();
            let is_self = idx == 0
                && (t == "self"
                    || t == "mut self"
                    || t.starts_with("&self")
                    || t.starts_with("& self")
                    || t.starts_with("&mut self")
                    || t.starts_with("&'")
                        && t.split_whitespace().any(|w| w == "self")
                    || t.starts_with("self:")
                    || t.starts_with("mut self:"));
            if is_self {
                has_self = true;
            }
            let ident = !head_clean.is_empty() && head_clean.bytes().all(is_ident) && t.contains(':');
            params.push((ident.then(|| head_clean.to_string()), is_self));
        }
        // Body or `;`.
        let mut d = 0i32;
        let mut body = None;
        let mut q = pc + 1;
        while q < b.len() {
            match b[q] {
                b'(' | b'[' => d += 1,
                b')' | b']' => d -= 1,
                b'{' if d == 0 => {
                    body = match_close(b, q).map(|c| (q, c));
                    break;
                }
                b';' if d == 0 => break,
                _ => {}
            }
            q += 1;
        }
        // Innermost enclosing impl/trait block, unless inside another fn body
        // (a nested fn is free).
        let ctx = blks
            .iter()
            .filter(|bl| bl.start < st && st < bl.end)
            .min_by_key(|bl| bl.end - bl.start)
            .map(|bl| bl.ctx.clone())
            .unwrap_or(Ctx::Free);
        out.push(FnDef { file, name, name_pos: ns, is_pub, ctx, has_self, params, body });
    }
    out
}

// ---------------------------------------------------------------- occurrences

#[derive(Clone, Copy, PartialEq, Debug)]
enum Prefix {
    Dot,
    /// `Qualifier::name` (qualifier text range, may be empty for `>::`).
    Path,
    Decl,
    Bare,
}

struct Occ {
    file: usize,
    pos: usize,
    end: usize,
    prefix: Prefix,
    qualifier: String,
}

fn classify(s: &str, st: usize, en: usize) -> (Prefix, String) {
    let b = s.as_bytes();
    let p = back_ws(b, st);
    if p >= 2 && b[p - 1] == b':' && b[p - 2] == b':' {
        let q = back_ws(b, p - 2);
        let qual = word_before(b, q).map(|(a, z)| s[a..z].to_string()).unwrap_or_default();
        return (Prefix::Path, qual);
    }
    if p >= 1 && b[p - 1] == b'.' && !(p >= 2 && b[p - 2] == b'.') {
        return (Prefix::Dot, String::new());
    }
    if let Some((a, z)) = word_before(b, p) {
        if &s[a..z] == "fn" {
            return (Prefix::Decl, String::new());
        }
    }
    let _ = en;
    (Prefix::Bare, String::new())
}

/// Next non-space byte after `en`, and whether it opens a call (`(` or `::<..>(`).
fn call_open(s: &str, en: usize) -> Option<usize> {
    let b = s.as_bytes();
    let mut i = skip_ws(b, en);
    if i + 2 < b.len() && b[i] == b':' && b[i + 1] == b':' && b[skip_ws(b, i + 2)] == b'<' {
        let mut k = skip_ws(b, i + 2);
        let mut d = 0;
        while k < b.len() {
            match b[k] {
                b'<' => d += 1,
                b'>' if b[k - 1] != b'-' => {
                    d -= 1;
                    if d == 0 {
                        k += 1;
                        break;
                    }
                }
                _ => {}
            }
            k += 1;
        }
        i = skip_ws(b, k);
    }
    (i < b.len() && b[i] == b'(').then_some(i)
}

/// Does `occ` look like a use of `def`? Name matching only; see the module limits.
fn qualifies(def: &FnDef, occ: &Occ, text: &str) -> bool {
    let b = text.as_bytes();
    let called = call_open(text, occ.end).is_some();
    match (&def.ctx, def.has_self) {
        (Ctx::Free, _) => match occ.prefix {
            Prefix::Bare => {
                if called {
                    return true;
                }
                // A function value: `map(name)`, `= name;`, `, name,`.
                let p = back_ws(b, occ.pos);
                let n = skip_ws(b, occ.end);
                p > 0
                    && matches!(b[p - 1], b'(' | b',' | b'=')
                    && n < b.len()
                    && matches!(b[n], b')' | b',' | b';')
            }
            Prefix::Path => true,
            _ => false,
        },
        (Ctx::Inherent(t), true) => match occ.prefix {
            Prefix::Dot => called,
            Prefix::Path => occ.qualifier == *t || occ.qualifier == "Self" || occ.qualifier.is_empty(),
            _ => false,
        },
        (Ctx::Inherent(t), false) => {
            occ.prefix == Prefix::Path
                && (occ.qualifier == *t || occ.qualifier == "Self" || occ.qualifier.is_empty())
        }
        _ => false,
    }
}

// ---------------------------------------------------------------- argument constness

/// Whether a call argument is a compile-time constant (literal, unit variant,
/// `CONST`, `None`, ...). Anything else is "non-constant".
fn is_const_arg(a: &str) -> bool {
    let mut t = a.trim();
    loop {
        if let Some(r) = t.strip_prefix("&mut ") {
            t = r.trim();
        } else if let Some(r) = t.strip_prefix('&') {
            t = r.trim();
        } else {
            break;
        }
    }
    if t.is_empty() || t == "()" || t == "[]" || t == "true" || t == "false" || t == "None" {
        return true;
    }
    let f = t.as_bytes()[0];
    if f.is_ascii_digit() || f == b'"' || f == b'\'' || (f == b'-' && t.len() > 1 && t.as_bytes()[1].is_ascii_digit()) {
        return true;
    }
    if t.starts_with("b\"") || t.starts_with("Default::default()") {
        return true;
    }
    if let Some(inner) = t.strip_prefix("Some(").and_then(|r| r.strip_suffix(')')) {
        return is_const_arg(inner);
    }
    for pre in ["Duration::from_secs(", "Duration::from_millis(", "Duration::from_micros(", "Duration::from_nanos(", "std::time::Duration::from_secs(", "std::time::Duration::from_millis("] {
        if let Some(inner) = t.strip_prefix(pre).and_then(|r| r.strip_suffix(')')) {
            return is_const_arg(inner);
        }
    }
    // A bare path: `A::B::NAME` / `Enum::Variant` / `NAME`.
    if t.bytes().all(|c| is_ident(c) || c == b':') {
        let last = t.rsplit("::").next().unwrap_or(t);
        return last.chars().next().is_some_and(|c| c.is_ascii_uppercase());
    }
    false
}

// ---------------------------------------------------------------- the scan

#[derive(Default, Debug)]
struct Scan {
    /// S2a: pub fns without a production caller, minus the mechanical categories.
    uncalled: BTreeSet<String>,
    /// S2b: `_param`s a body never reads that some caller fills non-constantly.
    ignored_params: BTreeSet<String>,
    /// Mechanical-category counts, printed so a reviewer sees what was auto-allowed.
    categories: BTreeMap<&'static str, usize>,
    pub_defs: usize,
    files: usize,
    test_files: usize,
}

fn is_test_file_name(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.ends_with("tests.rs") || path.split('/').any(|c| c == "tests")
}

fn scan(files: &[(String, String)]) -> Scan {
    let mut out = Scan { files: files.len(), ..Default::default() };
    let paths: Vec<&str> = files.iter().map(|(p, _)| p.as_str()).collect();
    let idx: HashMap<&str, usize> = paths.iter().enumerate().map(|(i, p)| (*p, i)).collect();

    let mut clean: Vec<String> = files.iter().map(|(_, t)| blank(t)).collect();
    let mut is_test: Vec<bool> = files
        .iter()
        .enumerate()
        .map(|(i, (p, _))| is_test_file_name(p) || clean[i].contains("#![cfg(test)]"))
        .collect();

    // Strip gated items; collect gated `mod x;` files; fixpoint through test files' children.
    let mut gated_mods: Vec<Vec<String>> = Vec::new();
    for (i, (_, t)) in files.iter().enumerate() {
        gated_mods.push(strip_test_items(&mut clean[i], t));
    }
    let raw_mods: Vec<Vec<String>> = files.iter().map(|(_, t)| all_mod_decls(&blank(t))).collect();
    loop {
        let mut changed = false;
        for i in 0..files.len() {
            let kids: &Vec<String> = if is_test[i] { &raw_mods[i] } else { &gated_mods[i] };
            for name in kids {
                for cand in child_paths(paths[i], name) {
                    if let Some(&j) = idx.get(cand.as_str()) {
                        if !is_test[j] {
                            is_test[j] = true;
                            changed = true;
                        }
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }
    out.test_files = is_test.iter().filter(|t| **t).count();

    // serde `default = "path"` / `with = "path"` names are callers the text cannot see.
    let mut serde_names: HashSet<String> = HashSet::new();
    for (i, (_, t)) in files.iter().enumerate() {
        if is_test[i] {
            continue;
        }
        let mut from = 0;
        while let Some(off) = t[from..].find("serde(") {
            let st = from + off;
            let Some(c) = match_close(t.as_bytes(), st + 5) else { break };
            let seg = &t[st..=c];
            let mut parts = seg.split('"');
            parts.next();
            while let Some(lit) = parts.next() {
                if let Some(last) = lit.rsplit("::").next() {
                    serde_names.insert(last.to_string());
                }
                parts.next();
            }
            from = c;
        }
    }

    // Production definitions and occurrences.
    let mut all_defs: Vec<FnDef> = Vec::new();
    let mut occs: HashMap<String, Vec<Occ>> = HashMap::new();
    for (i, text) in clean.iter().enumerate() {
        if is_test[i] {
            continue;
        }
        all_defs.extend(defs(i, text));
        for (st, en) in tokens(text.as_bytes()) {
            let (prefix, qualifier) = classify(text, st, en);
            occs.entry(text[st..en].to_string())
                .or_default()
                .push(Occ { file: i, pos: st, end: en, prefix, qualifier });
        }
    }

    for d in &all_defs {
        let path = paths[d.file];
        let key = d.key(path);
        let in_trait = matches!(d.ctx, Ctx::TraitImpl | Ctx::TraitDef);
        let callers: Vec<&Occ> = occs
            .get(&d.name)
            .into_iter()
            .flatten()
            .filter(|o| {
                let own = o.file == d.file && d.body.is_some_and(|(a, z)| a < o.pos && o.pos < z);
                !own && !(o.file == d.file && o.pos == d.name_pos) && qualifies(d, o, &clean[o.file])
            })
            .collect();

        // S2a
        if d.is_pub && !in_trait {
            out.pub_defs += 1;
            if !callers.is_empty() {
                // called
            } else if path == "main.rs" {
                *out.categories.entry("main entry point").or_default() += 1;
            } else if serde_names.contains(&d.name) {
                *out.categories.entry("serde path").or_default() += 1;
            } else {
                out.uncalled.insert(key.clone());
            }
        }

        // S2b
        let Some((bs, be)) = d.body else { continue };
        for (pi, (pname, is_self)) in d.params.iter().enumerate() {
            let Some(pn) = pname else { continue };
            if *is_self || !pn.starts_with('_') || pn.trim_start_matches('_').is_empty() {
                continue;
            }
            let body_text = &clean[d.file][bs..be];
            let used = tokens(body_text.as_bytes()).iter().any(|&(a, z)| &body_text[a..z] == pn);
            if used {
                continue;
            }
            if in_trait {
                *out.categories.entry("trait signature (_param)").or_default() += 1;
                continue;
            }
            if path == "main.rs" {
                *out.categories.entry("main entry point (_param)").or_default() += 1;
                continue;
            }
            // Position among the call arguments (self is not passed with `.f(..)`).
            let mut nonconst = false;
            for o in &callers {
                let text = &clean[o.file];
                let Some(open) = call_open(text, o.end) else { continue };
                let Some(close) = match_close(text.as_bytes(), open) else { continue };
                let args = split_top(&text[open + 1..close]);
                // `.f(..)` omits the receiver; `Type::f(self_arg, ..)` passes it first.
                let want = pi - usize::from(d.has_self && o.prefix == Prefix::Dot);
                if let Some(a) = args.get(want) {
                    if !is_const_arg(a) {
                        nonconst = true;
                    }
                }
            }
            if nonconst {
                out.ignored_params.insert(format!("{key}::{pn}"));
            }
        }
    }
    out
}

fn load_src() -> Vec<(String, String)> {
    fn walk(dir: &std::path::Path, root: &std::path::Path, out: &mut Vec<(String, String)>) {
        let mut entries: Vec<_> = std::fs::read_dir(dir).expect("read src dir").flatten().collect();
        entries.sort_by_key(|e| e.path());
        for e in entries {
            let p = e.path();
            if p.is_dir() {
                walk(&p, root, out);
            } else if p.extension().is_some_and(|x| x == "rs") {
                let rel = p.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
                out.push((rel, std::fs::read_to_string(&p).expect("read source")));
            }
        }
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut v = Vec::new();
    walk(&root, &root, &mut v);
    assert!(v.len() > 50, "the source walk found only {} files", v.len());
    v
}

// ---------------------------------------------------------------- tests

fn check_lists(
    what: &str,
    found: &BTreeSet<String>,
    allow: &[(&str, &str)],
    pending: &[&str],
) {
    let listed: BTreeSet<&str> = allow.iter().map(|(k, _)| *k).chain(pending.iter().copied()).collect();
    let new: Vec<&String> = found.iter().filter(|k| !listed.contains(k.as_str())).collect();
    assert!(
        new.is_empty(),
        "{what}: {} not allowlisted and not in PENDING_TRIAGE:\n  {}",
        new.len(),
        new.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("\n  ")
    );
    let stale: Vec<&&str> = listed.iter().filter(|k| !found.contains(**k)).collect();
    assert!(
        stale.is_empty(),
        "{what}: listed but no longer flagged (fixed or renamed: delete the entry):\n  {}",
        stale.iter().map(|s| **s).collect::<Vec<_>>().join("\n  ")
    );
    for (k, why) in allow {
        assert!(!why.trim().is_empty(), "{what}: allowlist entry {k} has no reason");
    }
}

#[test]
fn every_pub_fn_has_a_production_caller_or_a_listed_reason() {
    let s = scan(&load_src());
    eprintln!(
        "[S2a] {} files ({} test), {} pub fns, auto-allowed {:?}, uncalled {}",
        s.files, s.test_files, s.pub_defs, s.categories, s.uncalled.len()
    );
    assert!(s.pub_defs > 200, "the scan saw only {} pub fns: the walker is broken", s.pub_defs);
    check_lists("S2a uncalled pub fns", &s.uncalled, ALLOWLIST, PENDING_TRIAGE);
}

#[test]
fn no_ignored_parameter_receives_a_live_value() {
    let s = scan(&load_src());
    eprintln!("[S2b] auto-allowed {:?}, flagged {}", s.categories, s.ignored_params.len());
    check_lists("S2b ignored `_param`s", &s.ignored_params, PARAM_ALLOWLIST, PARAM_PENDING_TRIAGE);
}

// -- the scanner itself: a planted defect is flagged, a planted control is not.

fn fixture(files: &[(&str, &str)]) -> Scan {
    scan(&files.iter().map(|(p, t)| (p.to_string(), t.to_string())).collect::<Vec<_>>())
}

#[test]
fn scanner_flags_a_pub_fn_only_tests_call_and_passes_a_called_one() {
    let s = fixture(&[
        (
            "lib.rs",
            "pub struct C { x: f64 }\n\
             impl C {\n    pub fn live(&self) -> f64 { self.x }\n    pub fn dead(&self) -> f64 { self.x }\n    pub fn make() -> C { C { x: 1.0 } }\n}\n\
             pub fn free_live() {}\npub fn free_dead() {}\n\
             pub fn driver() { let c = C::make(); c.live(); free_live(); }\n\
             #[cfg(test)]\nmod t { use super::*; #[test] fn a() { C::make().dead(); free_dead(); } }\n",
        ),
        ("other_tests.rs", "fn x() { crate::free_dead(); }\n"),
        ("main.rs", "fn main() { lib::driver(); }\n"),
    ]);
    assert_eq!(
        s.uncalled.iter().map(String::as_str).collect::<Vec<_>>(),
        vec!["lib.rs::C::dead", "lib.rs::free_dead"],
        "RED: the planted uncalled fns must be the flagged set exactly"
    );
}

#[test]
fn scanner_treats_a_field_read_as_no_call_and_a_cfg_test_mod_file_as_test_code() {
    let s = fixture(&[
        ("lib.rs", "pub struct R { proactive_rate: f64 }\nimpl R { pub fn proactive_rate(&self) -> f64 { self.proactive_rate } }\n#[cfg(test)]\nmod kid;\npub fn go(r: &R) -> f64 { r.proactive_rate }\n"),
        ("kid.rs", "fn t(r: &crate::R) { r.proactive_rate(); }\n"),
    ]);
    assert!(
        s.uncalled.contains("lib.rs::R::proactive_rate"),
        "a field read `r.name` and a call in a `#[cfg(test)] mod` file are not production calls: {:?}",
        s.uncalled
    );
}

#[test]
fn scanner_flags_an_ignored_param_with_a_live_arg_and_not_a_constant_one_or_a_trait_sig() {
    let s = fixture(&[(
        "lib.rs",
        "pub struct C;\n\
         impl C {\n    pub fn new_with(a: f64, _toggle: bool) -> C { let _ = a; C }\n    fn only_const(_k: u32) {}\n    fn used(_n: u32) -> u32 { _n }\n}\n\
         pub trait T { fn f(&self, _x: u32); }\nimpl T for C { fn f(&self, _x: u32) {} }\n\
         pub fn drive(flag: bool, n: u32) { C::new_with(1.0, flag); C::only_const(3); C::used(n); C.f(n); }\n",
    )]);
    assert_eq!(
        s.ignored_params.iter().map(String::as_str).collect::<Vec<_>>(),
        vec!["lib.rs::C::new_with::_toggle"],
        "RED/GREEN: only the live-arg ignored param is flagged; categories {:?}",
        s.categories
    );
}
