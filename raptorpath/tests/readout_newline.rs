//! Newline discipline between the diagnostic readouts and `tracing`
//! (status §3.1 item 6).
//!
//! Every L1 harness captures the engine with both streams on one file or
//! pipe (`… >log 2>&1`, `… 2>&1 | tee`), and the parsers read it line by
//! line. A `tracing` record (stdout) that lands inside a readout line
//! (stderr) — `[LAT] … final=1` followed on the same physical line by
//! `2026-…Z  INFO … cleaning up TUN interface` — corrupts the last token of
//! the readout and the record both. The claim: a readout is written as ONE
//! complete line, so the two writers interleave only between lines.
//!
//! The child is this test binary re-executed with both of its fds on one
//! file, running the binary's own subscriber configuration
//! (`readout::init_tracing`) and the engine's own readout writer
//! (`raptorpath::readout!`) from two threads at once, plus a `println!`
//! writer (the perf client's JSON rows share stdout with `tracing`).

use std::process::{Command, Stdio};

const CHILD: &str = "RP_READOUT_NEWLINE_CHILD";
const N: usize = 20_000;
const TAG: &str = "[LAT] site=receiver";

/// The child half: inert unless re-executed by the parent below.
#[test]
fn child_writers() {
    if std::env::var_os(CHILD).is_none() {
        return;
    }
    raptorpath::readout::init_tracing();
    let t = std::thread::spawn(|| {
        for i in 0..N {
            tracing::info!("cleaning up TUN interface {i}");
        }
    });
    let j = std::thread::spawn(|| {
        for i in 0..N {
            println!("{{\"run\": {i}, \"mbps\": 1.0}}");
        }
    });
    for i in 0..N {
        raptorpath::readout!("{TAG} n={i} p50=1 p99=2 final=1");
    }
    t.join().unwrap();
    j.join().unwrap();
}

#[test]
fn a_readout_never_shares_a_line_with_a_tracing_record() {
    let dir = std::env::temp_dir().join(format!("rp-readout-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("merged.log");
    let f = std::fs::File::create(&path).unwrap();
    let status = Command::new(std::env::current_exe().unwrap())
        // `--quiet`: libtest's own pretty format prints `test child_writers
        // ... ` with no newline before the test body runs, which would glue
        // onto the first line written here — the harness's partial line, not
        // the engine's. The terse format prints only whole lines around it.
        .args(["child_writers", "--exact", "--nocapture", "--test-threads=1", "--quiet"])
        .env(CHILD, "1")
        .env("RUST_LOG", "info")
        .stdout(Stdio::from(f.try_clone().unwrap()))
        .stderr(Stdio::from(f))
        .status()
        .unwrap();
    assert!(status.success(), "the child writer failed: {status:?}");
    let raw = std::fs::read(&path).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    let log = String::from_utf8_lossy(&raw);

    let mut readouts = 0usize;
    let mut records = 0usize;
    let mut glued: Vec<&str> = Vec::new();
    let (mut with_rec, mut with_json) = (0usize, 0usize);
    for l in log.lines() {
        let has_tag = l.contains(TAG);
        let has_rec = l.contains("cleaning up TUN interface");
        let has_json = l.contains("\"mbps\"");
        if has_tag {
            readouts += 1;
            // The readout is the whole line: the tag starts it, `final=1`
            // ends it, and nothing else rides on it.
            if !l.starts_with(TAG) || !l.ends_with("final=1") || has_rec || has_json {
                glued.push(l);
            }
            with_rec += usize::from(has_rec);
            with_json += usize::from(has_json);
        }
        if has_rec {
            records += 1;
        }
    }
    let shown: Vec<_> = glued.iter().take(5).collect();
    assert!(
        glued.is_empty(),
        "{} of {readouts} readout lines share their line with another writer \
         ({with_rec} with a tracing record, {with_json} with a println row; \
         first {}): {shown:#?}",
        glued.len(),
        shown.len()
    );
    assert_eq!(readouts, N, "a readout line was lost or split");
    assert_eq!(records, N, "a tracing record was lost or split");
}
