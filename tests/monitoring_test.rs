//! ADR-0013: Monitoring and observability tests.

use raptorpath::monitor::stats::SharedStats;
use std::sync::atomic::Ordering;

#[test]
fn test_stats_initialization() {
    let stats = SharedStats::new();
    assert_eq!(stats.blocks.encoded.load(Ordering::Relaxed), 0);
    assert_eq!(stats.blocks.decoded_ok.load(Ordering::Relaxed), 0);
    assert_eq!(stats.blocks.decoded_fail.load(Ordering::Relaxed), 0);
    assert!(stats.paths.read().is_empty());
}

#[test]
fn test_add_paths() {
    let stats = SharedStats::new();
    stats.add_path(0);
    stats.add_path(1);
    stats.add_path(2);

    assert_eq!(stats.paths.read().len(), 3);
    assert!(stats.path(0).is_some());
    assert!(stats.path(1).is_some());
    assert!(stats.path(2).is_some());
    assert!(stats.path(99).is_none());
}

#[test]
fn test_path_stats_atomic_updates() {
    let stats = SharedStats::new();
    stats.add_path(0);

    let path = stats.path(0).unwrap();
    path.loss_rate_e6.store(100_000, Ordering::Relaxed); // 10%
    path.rtt_us.store(20_000, Ordering::Relaxed); // 20ms
    path.throughput_bps.store(50_000_000, Ordering::Relaxed); // 50 Mbps
    path.cwnd.store(150, Ordering::Relaxed);
    path.in_flight.store(42, Ordering::Relaxed);
    path.symbols_sent.fetch_add(1000, Ordering::Relaxed);
    path.symbols_received.fetch_add(900, Ordering::Relaxed);

    let snap = path.snapshot();
    assert!((snap.loss_rate - 0.1).abs() < 1e-6);
    assert!((snap.rtt_ms - 20.0).abs() < 0.01);
    assert!((snap.throughput_mbps - 50.0).abs() < 0.01);
    assert_eq!(snap.cwnd, 150);
    assert_eq!(snap.in_flight, 42);
    assert_eq!(snap.symbols_sent, 1000);
    assert_eq!(snap.symbols_received, 900);
}

#[test]
fn test_fec_stats() {
    let stats = SharedStats::new();
    stats.fec.total_source_symbols.store(10000, Ordering::Relaxed);
    stats.fec.total_repair_symbols.store(800, Ordering::Relaxed);
    stats.fec.target_tail_loss_bits.store(1e-5_f64.to_bits(), Ordering::Relaxed);
    stats.fec.actual_failure_rate_bits.store(1e-6_f64.to_bits(), Ordering::Relaxed);
    stats.fec.pi_correction_e3.store(1200, Ordering::Relaxed); // 1.2

    let snap = stats.snapshot();
    assert!((snap.fec.overhead_ratio - 0.08).abs() < 1e-6);
    assert!((snap.fec.target_tail_loss - 1e-5).abs() < 1e-10);
    assert!((snap.fec.actual_failure_rate - 1e-6).abs() < 1e-10);
    assert!((snap.fec.pi_correction - 1.2).abs() < 0.01);
}

#[test]
fn test_block_stats() {
    let stats = SharedStats::new();
    stats.blocks.encoded.fetch_add(100, Ordering::Relaxed);
    stats.blocks.decoded_ok.fetch_add(98, Ordering::Relaxed);
    stats.blocks.decoded_fail.fetch_add(2, Ordering::Relaxed);
    stats.blocks.pending.store(5, Ordering::Relaxed);

    let snap = stats.snapshot();
    assert_eq!(snap.blocks.encoded, 100);
    assert_eq!(snap.blocks.decoded_ok, 98);
    assert_eq!(snap.blocks.decoded_fail, 2);
    assert_eq!(snap.blocks.pending, 5);
}

#[test]
fn test_snapshot_json_serialization() {
    let stats = SharedStats::new();
    stats.add_path(0);
    stats.blocks.encoded.store(50, Ordering::Relaxed);
    stats.fec.total_source_symbols.store(2500, Ordering::Relaxed);
    stats.fec.total_repair_symbols.store(250, Ordering::Relaxed);

    let snap = stats.snapshot();
    let json = serde_json::to_string_pretty(&snap).unwrap();

    // Verify it's valid JSON with expected fields
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(parsed.get("uptime_secs").is_some());
    assert!(parsed.get("paths").unwrap().as_array().unwrap().len() == 1);
    assert_eq!(parsed["blocks"]["encoded"], 50);
    assert!((parsed["fec"]["overhead_ratio"].as_f64().unwrap() - 0.1).abs() < 1e-6);
}

#[test]
fn test_uptime_increases() {
    let stats = SharedStats::new();
    std::thread::sleep(std::time::Duration::from_millis(10));
    let snap = stats.snapshot();
    assert!(snap.uptime_secs > 0.0, "uptime should be positive");
}

#[test]
fn test_concurrent_path_updates() {
    use std::sync::Arc;

    let stats = Arc::new(SharedStats::new());
    stats.add_path(0);

    let path = stats.path(0).unwrap();

    // Simulate concurrent updates from multiple tasks
    let handles: Vec<_> = (0..10)
        .map(|_| {
            let p = path.clone();
            std::thread::spawn(move || {
                for _ in 0..100 {
                    p.symbols_sent.fetch_add(1, Ordering::Relaxed);
                    p.symbols_received.fetch_add(1, Ordering::Relaxed);
                }
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    assert_eq!(path.symbols_sent.load(Ordering::Relaxed), 1000);
    assert_eq!(path.symbols_received.load(Ordering::Relaxed), 1000);
}

#[tokio::test]
async fn test_http_endpoint() {
    use std::sync::Arc;

    let stats = Arc::new(SharedStats::new());
    stats.add_path(0);
    stats.add_path(1);
    stats.blocks.encoded.store(42, Ordering::Relaxed);

    // Start HTTP server on a random port
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let http_stats = stats.clone();
    tokio::spawn(async move {
        let app = axum::Router::new()
            .route("/status", axum::routing::get({
                let stats = http_stats.clone();
                move || {
                    let stats = stats.clone();
                    async move { axum::Json(stats.snapshot()) }
                }
            }));
        axum::serve(listener, app).await.unwrap();
    });

    // Give server a moment to start
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Fetch status
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let request = format!("GET /status HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).await.unwrap();

    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();

    // Should contain valid JSON body
    let body = response.split("\r\n\r\n").nth(1).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(body).unwrap();
    assert_eq!(parsed["blocks"]["encoded"], 42);
    assert_eq!(parsed["paths"].as_array().unwrap().len(), 2);
}
