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
