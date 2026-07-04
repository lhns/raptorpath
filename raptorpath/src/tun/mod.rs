//! TUN/TAP virtual network interface.
//!
//! Creates a virtual network interface that captures IP packets from the OS.
//! Packets written to the TUN device appear as if received from the network;
//! packets sent by the OS to the TUN subnet are captured for encoding and
//! multipath transmission.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "windows")]
mod windows;

use bytes::Bytes;
use tokio::sync::mpsc;

/// Platform-agnostic TUN interface.
pub struct TunInterface {
    /// Name of the interface
    pub name: String,
    /// Receiver for packets from the OS (to be sent over multipath)
    rx: mpsc::Receiver<Bytes>,
    /// Sender for packets to inject into the OS (received from multipath)
    pub tx: mpsc::Sender<Bytes>,
}

/// Configuration for the TUN interface.
pub struct TunConfig {
    pub name: String,
    pub address: std::net::IpAddr,
    pub netmask: std::net::IpAddr,
    pub mtu: u16,
}

/// App-side handle for a memory-backed TUN (see [`TunInterface::memory`]).
///
/// Lets an in-process application feed "packets" into the multipath engine
/// (as if the OS emitted them) and receive packets the engine delivers (as
/// if injecting into the OS). Used by `raptorpath perf` to drive objects
/// over the real transport without a kernel TUN or an inner TCP stack —
/// the apples-to-apples comparison against native QUIC perf tools.
pub struct MemTun {
    /// Packets the app wants the engine to send (app → engine).
    pub feed: mpsc::Sender<Bytes>,
    /// Packets the engine delivered (engine → app).
    pub delivered: mpsc::Receiver<Bytes>,
}

impl TunInterface {
    /// Create and configure a TUN interface.
    #[cfg(target_os = "linux")]
    pub async fn create(config: TunConfig) -> anyhow::Result<Self> {
        linux::create_tun(config).await
    }

    #[cfg(target_os = "windows")]
    pub async fn create(config: TunConfig) -> anyhow::Result<Self> {
        windows::create_tun(config).await
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    pub async fn create(_config: TunConfig) -> anyhow::Result<Self> {
        anyhow::bail!("TUN interface not supported on this platform")
    }

    /// Create a memory-backed TUN: no kernel device, no routing/DNS. The
    /// engine treats it exactly like an OS TUN (opaque `Bytes` packets in
    /// and out); the returned [`MemTun`] is the app-side of both channels.
    pub fn memory(_mtu: u16) -> (Self, MemTun) {
        // app → engine (the engine reads these via read_packet)
        let (feed_tx, feed_rx) = mpsc::channel(8192);
        // engine → app (the engine injects via self.tx)
        let (deliver_tx, deliver_rx) = mpsc::channel(8192);
        let tun = TunInterface {
            name: "mem".to_string(),
            rx: feed_rx,
            tx: deliver_tx,
        };
        (
            tun,
            MemTun {
                feed: feed_tx,
                delivered: deliver_rx,
            },
        )
    }

    /// Read a packet from the TUN device (packet sent by the OS).
    pub async fn read_packet(&mut self) -> Option<Bytes> {
        self.rx.recv().await
    }

    /// Write a packet to the TUN device (inject into the OS network stack).
    pub async fn write_packet(&self, data: Bytes) -> anyhow::Result<()> {
        self.tx
            .send(data)
            .await
            .map_err(|_| anyhow::anyhow!("TUN write channel closed"))
    }
}

impl Drop for TunInterface {
    fn drop(&mut self) {
        tracing::info!(name = %self.name, "cleaning up TUN interface");
    }
}
