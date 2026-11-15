//! Source deny-list for the sans-IO core (docs/derivations/shared-core-design.md
//! section 3, "Enforcement"; ADR-0076 phase 5.1). Walks `raptorpath-core/src`
//! and fails on any token that would bring I/O, a clock read, ambient state
//! or a protocol hint into the core.
//!
//! The walk asserts it visited files, and a self-test proves the scanner
//! flags every form in a planted fixture, so the gate cannot pass vacuously.

use std::fs;
use std::path::{Path, PathBuf};

/// Substring tokens.
const SUBSTR: &[&str] = &[
    "tokio",
    "quinn",
    "std::time::Instant",
    "SystemTime",
    ".elapsed()",
    "std::env",
    "std::thread",
    "std::net",
    "tracing",
    "println!",
    "eprintln!",
    "ProtocolHint",
    "RWM_",
    "thread_local",
    "lazy_static",
];

/// Whole-identifier tokens (so `operand` and `rand_u` do not trip them).
const IDENTS: &[&str] = &["rand", "Instant", "OnceLock", "LazyLock", "Mutex", "RwLock"];

/// Identifier prefixes (`AtomicU64`, `AtomicBool`, ...).
const IDENT_PREFIXES: &[&str] = &["Atomic"];

/// Modules that must not be named inside a `std::{...}` import group.
const STD_GROUP_BANNED: &[&str] = &["env", "thread", "net"];

fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in fs::read_dir(dir).expect("read_dir") {
        let p = e.expect("entry").path();
        if p.is_dir() {
            rs_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

fn idents(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).filter(|s| !s.is_empty())
}

/// The forbidden forms found in one file's text.
fn scan_text(text: &str) -> Vec<String> {
    let mut bad = Vec::new();
    for t in SUBSTR {
        if text.contains(t) {
            bad.push((*t).to_string());
        }
    }
    for id in idents(text) {
        if IDENTS.contains(&id) || IDENT_PREFIXES.iter().any(|p| id.starts_with(p)) {
            bad.push(id.to_string());
        }
    }
    // `use std::{env, thread};` hides the module behind a group.
    let mut rest = text;
    while let Some(i) = rest.find("std::{") {
        rest = &rest[i + "std::{".len()..];
        let body = &rest[..rest.find('}').unwrap_or(rest.len())];
        for id in idents(body) {
            if STD_GROUP_BANNED.contains(&id) {
                bad.push(format!("std::{{{id}}}"));
            }
        }
    }
    // Item-level statics (a lifetime `'static` is not one).
    for l in text.lines() {
        let t = l.trim_start();
        let t = t.strip_prefix("pub ").or_else(|| t.strip_prefix("pub(crate) ")).unwrap_or(t);
        if t.starts_with("static ") {
            bad.push("static item".to_string());
        }
    }
    bad
}

/// Returns (files visited, violations as "file: form").
fn scan(dir: &Path) -> (usize, Vec<String>) {
    let mut files = Vec::new();
    rs_files(dir, &mut files);
    files.sort();
    let mut bad = Vec::new();
    for f in &files {
        let text = fs::read_to_string(f).expect("read");
        for b in scan_text(&text) {
            bad.push(format!("{}: {b}", f.display()));
        }
    }
    (files.len(), bad)
}

#[test]
fn core_source_has_no_forbidden_tokens() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let (n, bad) = scan(&src);
    assert!(n >= 1, "walk found no .rs file under {}", src.display());
    assert!(bad.is_empty(), "forbidden tokens in raptorpath-core/src:\n{}", bad.join("\n"));
}

#[test]
fn the_walk_is_not_vacuous() {
    // (planted source, the form the scanner must report). Each lives in its
    // own file of a nested fixture directory, so recursion is exercised too.
    let planted: &[(&str, &str)] = &[
        ("use tokio::io;", "tokio"),
        ("use quinn::Endpoint;", "quinn"),
        ("use std::time::Instant;", "std::time::Instant"),
        ("use std::time::{Duration, Instant};", "Instant"),
        ("fn f() { let t = Instant::now(); }", "Instant"),
        ("fn f() { SystemTime::now(); }", "SystemTime"),
        ("fn f(t: u8) { t.elapsed(); }", ".elapsed()"),
        ("use std::env;", "std::env"),
        ("use std::{env, thread};", "std::{env}"),
        ("use std::{fmt, thread};", "std::{thread}"),
        ("use std::thread;", "std::thread"),
        ("use std::net::UdpSocket;", "std::net"),
        ("use rand::Rng;", "rand"),
        ("use tracing::info;", "tracing"),
        ("fn f() { println!(); }", "println!"),
        ("fn f() { eprintln!(); }", "eprintln!"),
        ("enum ProtocolHint {}", "ProtocolHint"),
        ("const G: &str = \"RWM_X\";", "RWM_"),
        ("static X: u8 = 0;", "static item"),
        ("pub static X: u8 = 0;", "static item"),
        ("static X: AtomicU64 = AtomicU64::new(0);", "AtomicU64"),
        ("static X: OnceLock<u8> = OnceLock::new();", "OnceLock"),
        ("struct S(Mutex<u8>);", "Mutex"),
        ("struct S(RwLock<u8>);", "RwLock"),
        ("thread_local! { }", "thread_local"),
    ];
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("deny_list_fixture");
    let _ = fs::remove_dir_all(&root);
    let nested = root.join("sub");
    fs::create_dir_all(&nested).unwrap();
    for (i, (code, _)) in planted.iter().enumerate() {
        fs::write(nested.join(format!("f{i}.rs")), format!("{code}\n")).unwrap();
    }
    // Clean code must not be flagged: look-alike identifiers, a lifetime.
    fs::write(
        nested.join("clean.rs"),
        "fn operand(x: &'static str) { let rand_u = 1; let instants = 2; let _ = (x, rand_u, instants); }\n\
         use std::{fmt, time::Duration};\n",
    )
    .unwrap();
    let (n, bad) = scan(&root);
    assert_eq!(n, planted.len() + 1, "walk must recurse and visit every fixture file");
    for (i, (code, form)) in planted.iter().enumerate() {
        // `f1.rs: ` must not match `f11.rs: `: anchor on the path separator.
        let file = format!("f{i}.rs: ");
        assert!(
            bad.iter().any(|b| b.contains(&format!("{}{file}", std::path::MAIN_SEPARATOR)) && b.ends_with(form)),
            "planted {code:?} not flagged as {form:?}: {bad:?}"
        );
    }
    assert!(!bad.iter().any(|b| b.contains("clean.rs")), "clean fixture was flagged: {bad:?}");
    let _ = fs::remove_dir_all(&root);
}
