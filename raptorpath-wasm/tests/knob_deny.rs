//! Source deny test for the three-knob model (plan
//! docs/derivations/visualizer-three-knobs-plan.md §5.4 item 8; CLAUDE.md
//! NO-MODE-SWITCH). ∞ is a value, never a classified case, and the lifetime
//! T and the hold H meet a literal or another knob only inside the law
//! functions of `src/knobs.rs`, pinned here by name.
//!
//! Scanned: the non-test code of `src/lib.rs`, `src/knobs.rs`,
//! `src/consumer.rs` and `src/link.rs` (comments and string literals
//! removed). Flagged:
//! - `is_infinite` / `is_finite` anywhere;
//! - `INFINITY` on a line that compares;
//! - a knob identifier on a line that compares or clamps (`<`, `>`, `<=`,
//!   `>=`, `==`, `!=`, `.min(`, `.max(`, `.clamp(`), outside the argument
//!   list of a `knobs::` call (passing T or H to a law is the allowed form);
//! - in `src/link.rs` (the forward leg, addendum 1 A4), any knob identifier
//!   or law call at all: the leg is a pure function of its own parameters.
//!
//! The scan asserts it visited every file, and a planted-fixture self-test
//! proves each form is flagged, so the gate cannot pass vacuously.

use std::fs;
use std::path::Path;

/// The law functions of `knobs.rs`: the only bodies allowed to compare T or H.
const PINNED: &[&str] = &[
    "intake_expired", "resend_admissible", "within_lifetime", "dequeue_expired", "protected",
    "trigger_delay", "one_way_fwd", "release_at", "released_by_hold", "ack_timeout", "validate_knobs",
];

/// Identifiers that carry the lifetime or the hold anywhere in the crate.
const KNOB_IDENTS: &[&str] = &[
    "lifetime", "lifetime_s", "t_life", "t_lifetime", "hold", "hold_s", "h_hold", "knob_t", "knob_h",
];

/// In `knobs.rs` the law parameters are named `t` and `h`.
const KNOBS_RS_PARAMS: &[&str] = &["t", "h"];

const FILES: &[&str] = &["src/lib.rs", "src/knobs.rs", "src/consumer.rs", "src/link.rs"];

/// Remove `//` comments and the contents of string literals (which may span
/// lines); newlines are kept so line numbers stay meaningful.
fn strip_comments_and_strings(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let (mut in_str, mut in_comment) = (false, false);
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\n' {
            in_comment = false;
            out.push('\n');
        } else if in_comment {
        } else if in_str {
            if c == '\\' {
                i += 1; // skip the escaped char
            } else if c == '"' {
                in_str = false;
                out.push('"');
            }
        } else if c == '/' && chars.get(i + 1) == Some(&'/') {
            in_comment = true;
        } else if c == '"' {
            in_str = true;
            out.push('"');
        } else {
            out.push(c);
        }
        i += 1;
    }
    out
}

/// Byte range of the brace block that starts at the first `{` at or after `from`.
fn block_end(text: &str, from: usize) -> usize {
    let open = from + text[from..].find('{').expect("block");
    let mut depth = 0i32;
    for (i, c) in text[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return open + i + 1;
                }
            }
            _ => {}
        }
    }
    text.len()
}

/// Drop `#[cfg(test)] mod NAME { ... }` blocks (tests may compare against ∞).
fn strip_test_modules(text: &str) -> String {
    let mut s = text.to_string();
    loop {
        let Some(at) = s.find("#[cfg(test)]") else { break };
        let rest = s[at + "#[cfg(test)]".len()..].trim_start();
        let is_block_mod = rest.starts_with("mod ")
            && rest.find('{').is_some_and(|b| rest.find(';').is_none_or(|sc| b < sc));
        if is_block_mod {
            let end = block_end(&s, at);
            s.replace_range(at..end, "");
        } else {
            // a test-only field or `mod x;` line: keep it, but hide the marker.
            s.replace_range(at..at + "#[cfg(test)]".len(), "");
        }
    }
    s
}

/// Drop the bodies of the pinned law functions.
fn strip_pinned(text: &str) -> String {
    let mut s = text.to_string();
    for name in PINNED {
        let pat = format!("fn {name}(");
        if let Some(at) = s.find(&pat) {
            let end = block_end(&s, at);
            s.replace_range(at..end, "");
        }
    }
    s
}

/// Drop the argument lists of calls into the law module: passing T or H to
/// a pinned law is the allowed form, comparing them is not.
fn strip_law_calls(line: &str) -> String {
    let mut out = String::new();
    let mut rest = line;
    while let Some(at) = rest.find("knobs::") {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        let Some(open) = tail.find('(') else {
            out.push_str(tail);
            rest = "";
            break;
        };
        let mut depth = 0i32;
        let mut end = tail.len();
        for (i, c) in tail[open..].char_indices() {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = open + i + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        out.push_str("law()");
        rest = &tail[end..];
    }
    out.push_str(rest);
    out
}

fn idents(line: &str) -> impl Iterator<Item = &str> {
    line.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).filter(|s| !s.is_empty())
}

/// Whether a code line compares or clamps.
fn compares(line: &str) -> bool {
    if ["==", "!=", "<=", ">=", ".min(", ".max(", ".clamp("].iter().any(|op| line.contains(op)) {
        return true;
    }
    let b = line.as_bytes();
    (1..b.len().saturating_sub(1)).any(|i| {
        (b[i] == b'<' || b[i] == b'>') && b[i - 1].is_ascii_whitespace() && b[i + 1].is_ascii_whitespace()
    })
}

/// The forbidden forms in one file's text.
fn scan_text(text: &str, is_knobs_rs: bool) -> Vec<String> {
    let mut code = strip_test_modules(&strip_comments_and_strings(text));
    if is_knobs_rs {
        code = strip_pinned(&code);
    }
    let mut bad = Vec::new();
    for (n, line) in code.lines().enumerate() {
        let ids: Vec<&str> = idents(line).collect();
        if ids.iter().any(|i| *i == "is_infinite" || *i == "is_finite") {
            bad.push(format!("line {}: classifies infinity: {}", n + 1, line.trim()));
        }
        if !compares(line) {
            continue;
        }
        if ids.iter().any(|i| i.contains("INFINITY")) {
            bad.push(format!("line {}: compares against INFINITY: {}", n + 1, line.trim()));
        }
        let knob = |i: &str| KNOB_IDENTS.contains(&i) || (is_knobs_rs && KNOBS_RS_PARAMS.contains(&i));
        let outside_calls = strip_law_calls(line);
        if idents(&outside_calls).any(knob) {
            bad.push(format!("line {}: compares a knob: {}", n + 1, line.trim()));
        }
    }
    bad
}

/// `src/link.rs` names no knob and calls no law (addendum 1 A4).
fn scan_link(text: &str) -> Vec<String> {
    let code = strip_test_modules(&strip_comments_and_strings(text));
    code.lines()
        .enumerate()
        .filter(|(_, l)| idents(l).any(|i| KNOB_IDENTS.contains(&i) || i == "knobs"))
        .map(|(n, l)| format!("line {}: a knob in the forward leg: {}", n + 1, l.trim()))
        .collect()
}

#[test]
fn knob_source_has_no_mode_switch_forms() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut visited = 0;
    let mut bad = Vec::new();
    for f in FILES {
        let p = root.join(f);
        let text = fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
        visited += 1;
        for b in scan_text(&text, f.ends_with("knobs.rs")) {
            bad.push(format!("{f}: {b}"));
        }
        if f.ends_with("link.rs") {
            for b in scan_link(&text) {
                bad.push(format!("{f}: {b}"));
            }
        }
    }
    assert_eq!(visited, FILES.len());
    assert!(bad.is_empty(), "NO-MODE-SWITCH forms in the knob model:\n{}", bad.join("\n"));
}

#[test]
fn the_scan_is_not_vacuous() {
    // (planted source, as lib.rs / consumer.rs code)
    let planted_lib: &[&str] = &[
        "fn a(x: f64) -> bool { x.is_infinite() }",
        "fn a(x: f64) -> bool { !x.is_finite() }",
        "fn a(x: f64) -> bool { x == f64::INFINITY }",
        "fn a(&self) -> bool { self.lifetime_s < 1.0 }",
        "fn a(&self) -> bool { self.hold_s >= 0.02 }",
        "fn a(&self) -> bool { self.lifetime_s > self.hold_s }",
        "fn a(&self) -> f64 { self.hold_s.min(1.0) }",
        "fn a(&self, now: f64) -> bool { now + self.lifetime <= now }",
        "fn a(&self) -> bool { knobs::intake_expired(1.0, 0.5) && self.hold_s > 0.0 }",
    ];
    for p in planted_lib {
        assert!(!scan_text(p, false).is_empty(), "scanner missed: {p}");
    }
    // In knobs.rs, a comparison on `t` outside a pinned law is flagged ...
    assert!(!scan_text("pub fn sneaky(t: f64) -> bool { t < 0.4 }", true).is_empty());
    // ... and inside a pinned law it is allowed.
    assert!(scan_text("pub fn intake_expired(t: f64, age: f64) -> bool {\n    age >= t\n}", true).is_empty());
    // Comments, strings and test modules are not code.
    let clean = "// if lifetime_s < 1.0 { mode }\n\
                 fn w(s: u64, n: u64) { while s < n && knobs::intake_expired(self.lifetime_s, f(s)) {} }\n\
                 fn f() -> &'static str { \"hold_s == 0\" }\n\
                 #[cfg(test)]\nmod tests {\n    fn t() { assert!(x.is_finite() && x == f64::INFINITY); }\n}\n\
                 fn g(lifetime_s: f64) -> f64 { lifetime_s + 1.0 }\n\
                 fn v() -> Vec<f64> { Vec::new() }\n";
    let got = scan_text(clean, false);
    assert!(got.is_empty(), "false positives: {got:?}");
    // The forward leg may not name a knob or call a law, even uncompared.
    for p in [
        "pub fn leg(t: f64, lifetime_s: f64) -> f64 { t + lifetime_s }",
        "pub fn leg(t: f64) -> bool { knobs::intake_expired(t, 0.0) }",
        "pub fn leg(hold: f64) -> f64 { hold }",
    ] {
        assert!(!scan_link(p).is_empty(), "link scanner missed: {p}");
    }
    let leg = "pub fn leg_send(free: &mut f64, t: f64, sv: f64) -> f64 { t.max(*free) + sv }
               // the lifetime is not read here
";
    assert!(scan_link(leg).is_empty(), "link false positive");
}
