//! Linux TUN implementation using the `tun` crate.

use super::{TunConfig, TunInterface};
use bytes::Bytes;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tracing::info;

pub async fn create_tun(config: TunConfig) -> anyhow::Result<TunInterface> {
    let mut tun_config = tun::Configuration::default();
    tun_config
        .name(&config.name)
        .address(config.address)
        .netmask(config.netmask)
        .mtu(config.mtu as u16)
        .up();

    let dev = tun::create_as_async(&tun_config)?;
    let (mut reader, mut writer) = tokio::io::split(dev);

    info!(name = %config.name, "TUN interface created");

    // ADR-0011: larger channel capacities to reduce stalls under load
    let (os_tx, rx) = mpsc::channel::<Bytes>(4096);
    let (tx, mut inject_rx) = mpsc::channel::<Bytes>(4096);

    // Read loop: OS → raptorpath
    tokio::spawn(async move {
        let mut buf = vec![0u8; config.mtu as usize + 64];
        loop {
            match reader.read(&mut buf).await {
                Ok(n) if n > 0 => {
                    let packet = Bytes::copy_from_slice(&buf[..n]);
                    if os_tx.send(packet).await.is_err() {
                        break;
                    }
                }
                Ok(_) => break,
                Err(e) => {
                    tracing::error!(?e, "TUN read error");
                    break;
                }
            }
        }
    });

    // Write loop: raptorpath → OS
    tokio::spawn(async move {
        // A single bad inner packet must NEVER tear down the tunnel. When the
        // window/packing FEC path occasionally delivers a mis-framed packet,
        // the kernel rejects the write with EINVAL; the old code `break`ed on
        // that, dropping inject_rx, which closed the receiver's inject channel
        // and shut the whole tunnel down. The peer's path liveness then timed
        // out ~6s later (L1: this killed rp-realtime streams). Drop malformed
        // packets and continue; only give up after a run of consecutive write
        // failures, which signals the device itself is gone.
        const MAX_CONSECUTIVE_WRITE_ERRORS: u32 = 64;
        let mut consecutive_errors: u32 = 0;
        while let Some(packet) = inject_rx.recv().await {
            if !super::looks_like_ip(&packet) {
                tracing::debug!(len = packet.len(), "dropping non-IP TUN packet");
                continue;
            }
            match writer.write_all(&packet).await {
                Ok(()) => consecutive_errors = 0,
                Err(e) => {
                    consecutive_errors += 1;
                    tracing::warn!(?e, len = packet.len(), consecutive_errors,
                        "TUN write error — dropping packet");
                    if consecutive_errors >= MAX_CONSECUTIVE_WRITE_ERRORS {
                        tracing::error!("too many consecutive TUN write errors — device gone");
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
