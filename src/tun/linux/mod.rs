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

    let (os_tx, rx) = mpsc::channel::<Bytes>(256);
    let (tx, mut inject_rx) = mpsc::channel::<Bytes>(256);

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
        while let Some(packet) = inject_rx.recv().await {
            if let Err(e) = writer.write_all(&packet).await {
                tracing::error!(?e, "TUN write error");
                break;
            }
        }
    });

    Ok(TunInterface {
        name: config.name,
        rx,
        tx,
    })
}
