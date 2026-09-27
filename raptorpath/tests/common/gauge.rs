//! Shared readers for the engine's `[TAG] key=value …` gauge lines.
//!
//! Include with `#[path = "common/gauge.rs"] mod gauge;`.
//!
//! **Every numeric reader applies [`numeric_prefix`]** — a process has two
//! writers (stdout, stderr) and a concurrent write can land inside a gauge
//! line's LAST field (the `alpha_override_reachability` lesson). Reading
//! `42` out of `42[GATES] …` is right; failing to parse it was a flake.
#![allow(dead_code)]

/// The raw text after `key` in the first whitespace token starting with it.
pub fn raw_field<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    line.split_whitespace().find_map(|t| t.strip_prefix(key))
}

/// The leading numeric part of `v` (digits, `.`, sign, exponent).
pub fn numeric_prefix(v: &str) -> &str {
    let end = v
        .find(|c: char| {
            !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+' || c == 'e' || c == 'E')
        })
        .unwrap_or(v.len());
    &v[..end]
}

/// The numeric value text of `key` (numeric prefix applied); panics if the
/// key is absent — a missing field is the old-engine / dead-gauge failure.
pub fn field<'a>(line: &'a str, key: &str) -> &'a str {
    numeric_prefix(
        raw_field(line, key).unwrap_or_else(|| panic!("`{key}` missing from gauge line: {line}")),
    )
}

/// The full (non-numeric) token value of `key`, e.g. a name or a list.
pub fn str_field<'a>(line: &'a str, key: &str) -> &'a str {
    raw_field(line, key).unwrap_or_else(|| panic!("`{key}` missing from gauge line: {line}"))
}

pub fn u64_field(line: &str, key: &str) -> u64 {
    let v = field(line, key);
    v.parse()
        .unwrap_or_else(|e| panic!("`{key}` value `{v}` does not parse: {e} in {line}"))
}

pub fn f64_field(line: &str, key: &str) -> f64 {
    let v = field(line, key);
    v.parse()
        .unwrap_or_else(|e| panic!("`{key}` value `{v}` does not parse: {e} in {line}"))
}

/// A slot that may be `-` (not yet measured): `-` → `None`.
pub fn opt_field(line: &str, key: &str) -> Option<u64> {
    let v = field(line, key);
    (v != "-").then(|| {
        v.parse()
            .unwrap_or_else(|e| panic!("`{key}` value `{v}` does not parse: {e} in {line}"))
    })
}

/// [`opt_field`] for real-valued slots.
pub fn opt_f64_field(line: &str, key: &str) -> Option<f64> {
    let v = field(line, key);
    (v != "-").then(|| {
        v.parse()
            .unwrap_or_else(|e| panic!("`{key}` value `{v}` does not parse: {e} in {line}"))
    })
}

/// `final=1` as a whole token — the exit-flush marker.
pub fn is_final(line: &str) -> bool {
    line.split_whitespace().any(|t| t == "final=1")
}

/// The LAST line carrying `tag` — plain arrival order, no preference.
/// Use this (or [`require`]) wherever a test asserts something ABOUT the
/// last line (e.g. "the last line is the exit flush").
pub fn last_line<'a>(log: &'a str, tag: &str) -> Option<&'a str> {
    log.lines().rev().find(|l| l.contains(tag))
}

/// The line to read a TOTAL off: the last `final=1` line carrying `tag` if
/// the process flushed one (the exit flush holds the whole transfer), else
/// the last line carrying it.
pub fn last_with<'a>(log: &'a str, tag: &str) -> Option<&'a str> {
    let mut lines = log.lines().rev().filter(|l| l.contains(tag));
    let last = lines.clone().next();
    lines.find(|l| is_final(l)).or(last)
}

/// [`last_line`], panicking with `what` and the log when the tag is absent.
pub fn require<'a>(log: &'a str, tag: &str, what: &str) -> &'a str {
    last_line(log, tag).unwrap_or_else(|| panic!("no `{tag}` line — {what}\n--- log ---\n{log}"))
}

/// The maximum `key<n>` over EVERY token of the log — for interval counters
/// (`[DIAG] cod=`) that must never be read off the last line alone.
pub fn max_u64_token(log: &str, key: &str) -> u64 {
    log.split_whitespace()
        .filter_map(|t| t.strip_prefix(key))
        .filter_map(|v| numeric_prefix(v).parse::<u64>().ok())
        .max()
        .unwrap_or(0)
}

/// Split a per-path gauge line into its `pN:` slots: each slot is the
/// `pN` head (colon replaced by a space) followed by its tokens.
pub fn slots(line: &str) -> Vec<String> {
    line.split_whitespace().fold(Vec::new(), |mut acc: Vec<String>, t| {
        let head = t.split_once(':').is_some_and(|(h, _)| {
            h.len() > 1 && h.starts_with('p') && h[1..].chars().all(|c| c.is_ascii_digit())
        });
        if head {
            acc.push(t.replacen(':', " ", 1));
        } else if let Some(last) = acc.last_mut() {
            last.push(' ');
            last.push_str(t);
        }
        acc
    })
}
