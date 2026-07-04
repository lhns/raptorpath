//! Windows TUN implementation using the wintun driver.
//!
//! Requires the wintun.dll driver to be present. Download from:
//! https://www.wintun.net/

use super::{TunConfig, TunInterface};
use bytes::Bytes;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::info;

pub async fn create_tun(config: TunConfig) -> anyhow::Result<TunInterface> {
    // Load wintun.dll
    let wintun = unsafe { wintun::load()? };

    // Create adapter
    let adapter = match wintun::Adapter::open(&wintun, &config.name) {
        Ok(a) => a,
        Err(_) => wintun::Adapter::create(&wintun, &config.name, "RaptorPath", None)?,
    };

    // Set IP address using netsh (wintun doesn't do IP config)
    let addr_str = config.address.to_string();
    let mask_str = config.netmask.to_string();
    let output = tokio::process::Command::new("netsh")
        .args([
            "interface",
            "ip",
            "set",
            "address",
            &config.name,
            "static",
            &addr_str,
            &mask_str,
        ])
        .output()
        .await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "Failed to configure TUN interface '{}': {}. \
             Make sure you are running as Administrator.",
            config.name,
            stderr.trim()
        );
    }

    // Start session
    let session = Arc::new(adapter.start_session(wintun::MAX_RING_CAPACITY)?);
    let session_read = Arc::clone(&session);
    let session_write = Arc::clone(&session);

    info!(name = %config.name, "WinTUN interface created");

    // ADR-0011: larger channel capacities to reduce stalls under load
    let (os_tx, rx) = mpsc::channel::<Bytes>(4096);
    let (tx, mut inject_rx) = mpsc::channel::<Bytes>(4096);

    let mtu = config.mtu;

    // Read loop: OS → raptorpath (blocking, run on dedicated thread)
    tokio::task::spawn_blocking(move || {
        loop {
            match session_read.receive_blocking() {
                Ok(packet) => {
                    let data = Bytes::copy_from_slice(packet.bytes());
                    if os_tx.blocking_send(data).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    tracing::error!(?e, "WinTUN read error");
                    break;
                }
            }
        }
    });

    // Write loop: raptorpath → OS
    tokio::spawn(async move {
        // A single bad inner packet must NEVER tear down the tunnel (see the
        // Linux writer for the full rationale — one malformed FEC-delivered
        // packet used to kill the whole tunnel). Drop malformed packets and
        // continue; only give up after a run of consecutive write failures,
        // which signals the device itself is gone.
        const MAX_CONSECUTIVE_WRITE_ERRORS: u32 = 64;
        let mut consecutive_errors: u32 = 0;
        while let Some(packet) = inject_rx.recv().await {
            if !super::looks_like_ip(&packet) {
                tracing::debug!(len = packet.len(), "dropping non-IP TUN packet");
                continue;
            }
            match session_write.allocate_send_packet(packet.len() as u16) {
                Ok(mut send_packet) => {
                    send_packet.bytes_mut().copy_from_slice(&packet);
                    session_write.send_packet(send_packet);
                    consecutive_errors = 0;
                }
                Err(e) => {
                    consecutive_errors += 1;
                    tracing::warn!(?e, len = packet.len(), consecutive_errors,
                        "WinTUN write error — dropping packet");
                    if consecutive_errors >= MAX_CONSECUTIVE_WRITE_ERRORS {
                        tracing::error!("too many consecutive WinTUN write errors — device gone");
                        break;
                    }
                }
            }
        }
    });

    Ok(TunInterface {
        name: config.name,
        rx,
        tx,
    })
}
