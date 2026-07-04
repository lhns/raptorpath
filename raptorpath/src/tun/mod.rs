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

/// Cheap sanity check that `packet` is a plausible IP datagram before we hand
/// it to the kernel TUN. A TUN in IFF_TUN mode expects a raw IP packet whose
/// first nibble is the IP version; garbage (e.g. a mis-decoded or mis-framed
/// FEC symbol) written verbatim makes the kernel reject the write with EINVAL.
/// Dropping obviously-malformed packets keeps that garbage out of the write
/// path — see the write loops, where a raw write error must NOT be fatal.
pub(crate) fn looks_like_ip(packet: &[u8]) -> bool {
    match packet.first().map(|b| b >> 4) {
        Some(4) => packet.len() >= 20, // minimum IPv4 header
        Some(6) => packet.len() >= 40, // fixed IPv6 header
        _ => false,
    }
}

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

#[cfg(test)]
mod tests {
    use super::looks_like_ip;

    #[test]
    fn accepts_minimal_ipv4() {
        let mut pkt = vec![0u8; 20];
        pkt[0] = 0x45; // version 4, IHL 5
        assert!(looks_like_ip(&pkt));
    }

    #[test]
    fn accepts_minimal_ipv6() {
        let mut pkt = vec![0u8; 40];
        pkt[0] = 0x60; // version 6
        assert!(looks_like_ip(&pkt));
    }

    #[test]
    fn rejects_empty() {
        assert!(!looks_like_ip(&[]));
    }

    #[test]
    fn rejects_wrong_version_nibble() {
        // A mis-decoded FEC symbol whose first byte is not an IP version.
        let pkt = vec![0x00u8; 64];
        assert!(!looks_like_ip(&pkt));
        let pkt = vec![0xFFu8; 64];
        assert!(!looks_like_ip(&pkt));
    }

    #[test]
    fn rejects_ip_version_but_too_short() {
        // Correct version nibble but truncated below the header — exactly the
        // garbage that made the kernel return EINVAL and kill the tunnel.
        let short_v4 = vec![0x45u8; 8];
        assert!(!looks_like_ip(&short_v4));
        let short_v6 = vec![0x60u8; 20];
        assert!(!looks_like_ip(&short_v6));
    }
}
