//! Source deny-list for the sans-IO core (docs/derivations/shared-core-design.md
//! section 3, "Enforcement"; ADR-0076 phase 5.1). Walks `raptorpath-core/src`
//! and fails on any token that would bring I/O, a clock read, ambient state
//! or a protocol hint into the core.
//!
//! The walk asserts it visited files, and a self-test proves the scanner
//! flags every token in a planted fixture, so the gate cannot pass vacuously.

use std::fs;
use std::path::{Path, PathBuf};

/// Substring tokens.
const SUBSTR: &[&str] = &[
    "tokio",
    "quinn",
    "std::time::Instant",
    "Instant::now",
    "SystemTime",
    ".elapsed()",
    "std::env",
    "std::thread",
    "std::net",
    "tracing",
    "println!",
    "eprintln!",
    "ProtocolHint",
];

/// Whole-identifier tokens (so `operand` and `rand_u` do not trip it).
const IDENTS: &[&str] = &["rand"];

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

/// Returns (files visited, violations as "file: token").
fn scan(dir: &Path) -> (usize, Vec<String>) {
    let mut files = Vec::new();
    rs_files(dir, &mut files);
    files.sort();
    let mut bad = Vec::new();
    for f in &files {
        let text = fs::read_to_string(f).expect("read");
        for t in SUBSTR {
            if text.contains(t) {
                bad.push(format!("{}: {t}", f.display()));
            }
        }
        for id in text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
            if IDENTS.contains(&id) {
                bad.push(format!("{}: {id}", f.display()));
            }
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
    // Every token, planted in a nested fixture file, must be flagged.
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("deny_list_fixture");
    let _ = fs::remove_dir_all(&root);
    let nested = root.join("sub");
    fs::create_dir_all(&nested).unwrap();
    let mut planted: Vec<&str> = SUBSTR.to_vec();
    planted.extend_from_slice(IDENTS);
    for (i, t) in planted.iter().enumerate() {
        fs::write(nested.join(format!("f{i}.rs")), format!("fn x() {{ let _ = \"{t}\"; }}\n")).unwrap();
    }
    fs::write(nested.join("clean.rs"), "fn operand() { let rand_u = 1; let _ = rand_u; }\n").unwrap();
    let (n, bad) = scan(&root);
    assert_eq!(n, planted.len() + 1, "walk must recurse and visit every fixture file");
    for t in &planted {
        assert!(bad.iter().any(|b| b.ends_with(&format!(": {t}"))), "planted token {t:?} not flagged: {bad:?}");
    }
    assert!(!bad.iter().any(|b| b.contains("clean.rs")), "clean fixture was flagged: {bad:?}");
    let _ = fs::remove_dir_all(&root);
}
