//! The diagnostic readout writer: one line, one write.
//!
//! Every `[TAG] …` readout the engine prints (`[DIAG]`, `[ETA]`, `[LAT]`,
//! `[CHI]`, …) is scraped line by line by the L1 parsers, and the L1
//! harnesses capture the engine with both streams on one file
//! (`… >log 2>&1`). Two writers share that file: the readouts (stderr) and
//! the `tracing` subscriber (stdout, [`init_tracing`]).
//!
//! `eprintln!` is not a line discipline. Stderr is unbuffered, so
//! `eprintln!("{l}")` is TWO `write(2)`s — the formatted text, then the
//! `"\n"` — and a `tracing` record written between them lands on the
//! readout's line: `[LAT] … final=1` immediately followed by
//! `2026-…Z  INFO … cleaning up TUN interface`, and an empty line after it
//! (status §3.1 item 6). The stderr lock serialises only the process's own
//! stderr writers; stdout's record does not take it.
//!
//! [`emit_line`] formats the whole line, newline included, into one buffer
//! and hands it to the locked stderr in one `write_all` — one `write(2)` —
//! and the subscriber writes each record as one buffer too, so the two can
//! interleave only BETWEEN lines, never inside one. `readout!` has
//! `eprintln!`'s syntax; every readout site uses it, and
//! `no_eprintln_in_engine_source` keeps it that way.
//!
//! A failed write is dropped rather than panicking (unlike `eprintln!`):
//! several readouts are emitted from destructors at teardown, where a panic
//! would abort.

use std::io::Write;

/// Write one complete readout line to stderr in a single write.
pub fn emit_line(line: &str) {
    let mut buf = String::with_capacity(line.len() + 1);
    buf.push_str(line);
    buf.push('\n');
    let _ = std::io::stderr().lock().write_all(buf.as_bytes());
}

/// `eprintln!`'s syntax, one write per line (see the module header).
#[macro_export]
macro_rules! readout {
    ($($arg:tt)*) => {
        $crate::readout::emit_line(&::std::format!($($arg)*))
    };
}

/// The process's `tracing` subscriber: `RUST_LOG` when set, else
/// `raptorpath=info`; records to stdout, one buffer per record. Called once
/// from `main`; public so the newline-discipline test runs the binary's own
/// writer configuration and not a copy of it.
pub fn init_tracing() {
    // RUST_LOG wins when set; the info default applies only otherwise
    // (an added directive at equal specificity would override the env one
    // and silently ignore RUST_LOG=raptorpath=debug).
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("raptorpath=info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

#[cfg(test)]
mod tests {
    /// The gate that keeps the discipline: no engine source file prints a
    /// readout with `eprintln!`/`eprint!` (two writes, see the module
    /// header). Test modules (`tests.rs`, `tests/`) are exempt — their output
    /// is the test harness's, not a readout.
    #[test]
    fn no_eprintln_in_engine_source() {
        fn walk(dir: &std::path::Path, hits: &mut Vec<String>) {
            for e in std::fs::read_dir(dir).expect("read src dir").flatten() {
                let p = e.path();
                if p.is_dir() {
                    if p.file_name().is_some_and(|n| n == "tests") {
                        continue;
                    }
                    walk(&p, hits);
                } else if p.extension().is_some_and(|x| x == "rs")
                    && !p.file_name().is_some_and(|n| n == "tests.rs")
                {
                    let src = std::fs::read_to_string(&p).unwrap_or_default();
                    for (i, l) in src.lines().enumerate() {
                        let code = l.split("//").next().unwrap_or("");
                        if code.contains(concat!("eprint", "ln!(")) || code.contains(concat!("eprint", "!(")) {
                            hits.push(format!("{}:{}: {}", p.display(), i + 1, l.trim()));
                        }
                    }
                }
            }
        }
        let mut hits = Vec::new();
        walk(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut hits);
        assert!(
            hits.is_empty(),
            "readouts must go through `crate::readout!` (one write per line), \
             not eprintln!/eprint! (two writes — a tracing record can land \
             between them):\n{}",
            hits.join("\n")
        );
    }
}
