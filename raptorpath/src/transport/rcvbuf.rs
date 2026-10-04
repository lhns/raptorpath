//! The endpoint UDP socket's kernel receive buffer (`SO_RCVBUF`): the
//! request, the read-back, the `[RCVBUF]` echo, and the per-socket kernel
//! drop counter (`SO_MEMINFO[SK_MEMINFO_DROPS]`).
//!
//! **Why.** quinn binds its UDP socket without touching `SO_RCVBUF`
//! (quinn 0.11.9 `Endpoint::server`/`client`; quinn-udp only *offers*
//! `set_recv_buffer_size`), so every raptorpath endpoint got the host's
//! `net.core.rmem_default` — 212 992 B on stock Linux, ≈ 1.5 ms of arrivals
//! at 1 Gbit/s line rate. A receiver drain pause longer than that (a tokio
//! worker busy with another task, a vCPU wait) overflowed it: at the c1
//! cells the kernel dropped 17–182 GRO skbs (≈ ×9 datagrams each) per run,
//! and the engine read them as channel erasure (status.md §6, V4 finding 1;
//! fec-arq-model.md §8.5 "The kernel receive buffer").
//!
//! **The request** is a formula, not a tuning knob (CLAUDE.md formula-first):
//!
//! ```text
//! B_req = R_line × T_pause × k_truesize / 2
//!       = 125 000 000 B/s × 0.032 s × 2 / 2 = 4 000 000 B
//! ```
//!
//! - `R_line` = 125 MB/s: the fastest line rate the engine is measured on
//!   (the 1 Gbit/s c1 cells). Arrival trains reach the socket at line rate
//!   whatever the mean goodput, so the line rate, not the goodput, sizes it.
//! - `T_pause` = 32 ms: the drain-pause tolerance. **Bounded, not derived**:
//!   pauses longer than the default buffer's 1.5–3.3 ms window occurred at
//!   c1s (the drops), and pauses past ≈ 14 ms never occurred at the
//!   100 Mbit/s cells (zero drops in 224 rows with a 212 992 B buffer
//!   covering ≈ 14–17 ms there). 32 ms is ≥ 2× that upper bound.
//! - `k_truesize` = 2: the buffer is charged by `skb->truesize`, not payload.
//!   2 is the overhead the kernel's own convention assumes when it stores
//!   `sk_rcvbuf = 2 × min(req, rmem_max)` (`sock_setsockopt`, SO_RCVBUF:
//!   "double it to allow for struct sk_buff overhead"); the `/2` is that
//!   doubling, so the granted truesize budget is `R_line × T_pause × k`.
//!
//! The request is the same at every dial position (it is not a function of
//! δ or ρ), so the no-mode-switch invariant is not involved.
//!
//! **The clamp and its gauge.** The kernel clamps the request to
//! `net.core.rmem_max` (4 194 304 B on the benchmark VM's kernel, 212 992 B
//! on older distributions). A grant below `2 × B_req` is the clamp binding:
//! the echo reports it as `clamped=1`, and as root the engine then retries
//! with `SO_RCVBUFFORCE` (which ignores `rmem_max`) and reports which option
//! applied (`via=`). The echo is printed once per socket at bind
//! (measurement-discipline rule 1: it proves the request reached the socket).
//!
//! **The drop counter** is an instrument only: `rxdrop<path>=` on the
//! receiver's `[CTLD]` line. Its unit is the kernel's `sk_drops`, which
//! counts dropped *skbs* — with quinn-udp's `UDP_GRO` on, one skb is a whole
//! sender GSO superpacket (≈ 9 datagrams at c1s) — the same unit as the
//! netns `RcvbufErrors` counter (`ss -uamn` `d` equalled `RcvbufErrors` on
//! every measured run). It is not subtracted from the loss feed here.

use std::io;
use std::net::{SocketAddr, UdpSocket};

/// Line rate the request is sized for, bytes/s (1 Gbit/s: the c1 cells).
pub const RCVBUF_R_LINE_BYTES_PER_S: u64 = 125_000_000;
/// Drain-pause tolerance, ms (bounded, see the module doc).
pub const RCVBUF_T_PAUSE_MS: u64 = 32;
/// Truesize overhead factor the kernel's doubling convention absorbs.
pub const RCVBUF_K_TRUESIZE: u64 = 2;

/// `B_req = R_line × T_pause × k_truesize / 2` (module doc), in bytes.
pub const fn rcvbuf_request_bytes(r_line_bytes_per_s: u64, t_pause_ms: u64, k_truesize: u64) -> u64 {
    r_line_bytes_per_s * t_pause_ms / 1000 * k_truesize / 2
}

/// The requested `SO_RCVBUF`, bytes: 4 000 000.
pub const RCVBUF_REQUEST: usize =
    rcvbuf_request_bytes(RCVBUF_R_LINE_BYTES_PER_S, RCVBUF_T_PAUSE_MS, RCVBUF_K_TRUESIZE) as usize;

/// Which socket option produced the final grant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RcvbufVia {
    /// Plain `SO_RCVBUF` (unprivileged, clamped to `rmem_max`).
    SoRcvbuf,
    /// `SO_RCVBUFFORCE` (root; ignores `rmem_max`). Linux only.
    SoRcvbufForce,
}

impl RcvbufVia {
    pub fn as_str(self) -> &'static str {
        match self {
            RcvbufVia::SoRcvbuf => "SO_RCVBUF",
            RcvbufVia::SoRcvbufForce => "SO_RCVBUFFORCE",
        }
    }
}

/// What one endpoint socket asked for and what the kernel granted (read
/// back with `getsockopt(SO_RCVBUF)` after the request).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RcvbufGrant {
    pub requested: usize,
    pub granted: usize,
    pub via: RcvbufVia,
}

impl RcvbufGrant {
    /// The clamp's bind gauge: the kernel granted less than the doubled
    /// request, i.e. `rmem_max` (or the platform's equivalent) bound.
    pub fn clamped(&self) -> bool {
        self.granted < self.requested.saturating_mul(2)
    }
}

/// The `[RCVBUF]` echo, one line per endpoint socket.
pub fn echo_line(path_id: u32, role: &str, addr: SocketAddr, g: &RcvbufGrant) -> String {
    format!(
        "[RCVBUF] p{path_id} role={role} bind={addr} req={} granted={} via={} clamped={}",
        g.requested,
        g.granted,
        g.via.as_str(),
        u8::from(g.clamped())
    )
}

/// Bind a UDP socket for a quinn endpoint with `SO_RCVBUF = requested`,
/// read the grant back, and (Linux, root, clamped) retry with
/// `SO_RCVBUFFORCE`. An IPv6 address is made dual-stack exactly as
/// `quinn::Endpoint::client` does (best effort), so the client port keeps
/// its behavior.
pub fn bind_udp_with_rcvbuf(addr: SocketAddr, requested: usize) -> io::Result<(UdpSocket, RcvbufGrant)> {
    use socket2::{Domain, Protocol, Socket, Type};
    let sock = Socket::new(Domain::for_address(addr), Type::DGRAM, Some(Protocol::UDP))?;
    if addr.is_ipv6() {
        let _ = sock.set_only_v6(false);
    }
    // A refused request is not fatal: the read-back below reports what the
    // socket has, and the echo makes the shortfall visible.
    let _ = sock.set_recv_buffer_size(requested);
    sock.bind(&addr.into())?;
    let std_sock: UdpSocket = sock.into();
    let mut grant = RcvbufGrant {
        requested,
        granted: recv_buffer_size(&std_sock)?,
        via: RcvbufVia::SoRcvbuf,
    };
    if grant.clamped() && is_root() && force_recv_buffer_size(&std_sock, requested) {
        grant.granted = recv_buffer_size(&std_sock)?;
        grant.via = RcvbufVia::SoRcvbufForce;
    }
    Ok((std_sock, grant))
}

/// `getsockopt(SO_RCVBUF)`: the kernel's stored size (on Linux the doubled
/// value).
pub fn recv_buffer_size(sock: &UdpSocket) -> io::Result<usize> {
    socket2::SockRef::from(sock).recv_buffer_size()
}

#[cfg(target_os = "linux")]
fn is_root() -> bool {
    // SAFETY: geteuid has no preconditions (same call as preflight.rs).
    unsafe { libc::geteuid() == 0 }
}

#[cfg(not(target_os = "linux"))]
fn is_root() -> bool {
    false
}

/// `setsockopt(SO_RCVBUFFORCE)`; true when the kernel accepted it.
#[cfg(target_os = "linux")]
fn force_recv_buffer_size(sock: &UdpSocket, requested: usize) -> bool {
    use std::os::fd::AsRawFd;
    let v: libc::c_int = requested.min(libc::c_int::MAX as usize) as libc::c_int;
    // SAFETY: the fd is owned by `sock` and open for the call's duration;
    // the option value is a c_int we own, with its exact size passed.
    let rc = unsafe {
        libc::setsockopt(
            sock.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_RCVBUFFORCE,
            &v as *const libc::c_int as *const libc::c_void,
            std::mem::size_of::<libc::c_int>() as libc::socklen_t,
        )
    };
    rc == 0
}

#[cfg(not(target_os = "linux"))]
fn force_recv_buffer_size(_sock: &UdpSocket, _requested: usize) -> bool {
    false
}

/// `SO_MEMINFO` (include/uapi/asm-generic/socket.h: 55) and its drops index
/// `SK_MEMINFO_DROPS` (include/uapi/linux/sock_diag.h: 8, of
/// `SK_MEMINFO_VARS` = 9 u32s). Both checked in the benchmark VM's kernel
/// headers; defined here because the libc crate does not export them on
/// every target.
#[cfg(target_os = "linux")]
const SO_MEMINFO: libc::c_int = 55;
#[cfg(target_os = "linux")]
const SK_MEMINFO_DROPS: usize = 8;
#[cfg(target_os = "linux")]
const SK_MEMINFO_VARS: usize = 9;

/// The socket's kernel drop count (`sk_drops`; unit: skbs — see the module
/// doc). `None` where the platform has no such counter or the call fails;
/// never a substituted zero.
#[cfg(target_os = "linux")]
pub fn socket_drops(sock: &UdpSocket) -> Option<u64> {
    use std::os::fd::AsRawFd;
    let mut buf = [0u32; SK_MEMINFO_VARS];
    let mut len = std::mem::size_of_val(&buf) as libc::socklen_t;
    // SAFETY: the fd is owned by `sock`; `buf` is a u32 array we own whose
    // byte size is passed in `len`, and the kernel writes at most `len`.
    let rc = unsafe {
        libc::getsockopt(
            sock.as_raw_fd(),
            libc::SOL_SOCKET,
            SO_MEMINFO,
            buf.as_mut_ptr() as *mut libc::c_void,
            &mut len,
        )
    };
    if rc != 0 || (len as usize) < (SK_MEMINFO_DROPS + 1) * 4 {
        return None;
    }
    Some(u64::from(buf[SK_MEMINFO_DROPS]))
}

#[cfg(not(target_os = "linux"))]
pub fn socket_drops(_sock: &UdpSocket) -> Option<u64> {
    None
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// The constant is the formula's value, and the formula has the stated
    /// degree in each input (linear in R_line, T_pause and k; halved by the
    /// kernel's doubling) — measurement-discipline rule 17.
    #[test]
    fn the_request_is_the_formula() {
        assert_eq!(RCVBUF_REQUEST, 4_000_000);
        let b = |r, t, k| rcvbuf_request_bytes(r, t, k);
        let base = b(125_000_000, 32, 2);
        for n in 1..=8u64 {
            assert_eq!(b(125_000_000 * n, 32, 2), base * n, "linear in R_line");
            assert_eq!(b(125_000_000, 32 * n, 2), base * n, "linear in T_pause");
            assert_eq!(b(125_000_000, 32, 2 * n), base * n, "linear in k_truesize");
        }
        // Inside the unprivileged cap of a 4 MiB rmem_max (the VM's kernel).
        assert!(RCVBUF_REQUEST <= 4_194_304);
    }

    #[test]
    fn clamped_is_the_doubling_shortfall() {
        let g = |granted| RcvbufGrant { requested: 4_000_000, granted, via: RcvbufVia::SoRcvbuf };
        assert!(!g(8_000_000).clamped());
        assert!(g(7_999_999).clamped(), "rmem_max between req/2 and req binds");
        assert!(g(425_984).clamped(), "the old 212992 rmem_max binds");
    }

    #[test]
    fn the_echo_line_names_every_field() {
        let g = RcvbufGrant { requested: 4_000_000, granted: 8_000_000, via: RcvbufVia::SoRcvbuf };
        let a: SocketAddr = "10.77.0.2:7000".parse().unwrap();
        assert_eq!(
            echo_line(0, "server", a, &g),
            "[RCVBUF] p0 role=server bind=10.77.0.2:7000 req=4000000 granted=8000000 via=SO_RCVBUF clamped=0"
        );
    }

    /// The socket this module binds carries the request: the granted size,
    /// read back from the kernel, reaches the floor the host permits —
    /// `2 × min(req, rmem_max)` unprivileged on Linux (the kernel doubles),
    /// `2 × req` as root (SO_RCVBUFFORCE). A socket bound without the
    /// request reads `rmem_default` (212 992) and fails this.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_bound_socket_reads_back_the_floor() {
        let (s, g) = bind_udp_with_rcvbuf("127.0.0.1:0".parse().unwrap(), RCVBUF_REQUEST).unwrap();
        let floor = linux_floor(RCVBUF_REQUEST);
        assert_eq!(recv_buffer_size(&s).unwrap(), g.granted, "grant is the live read-back");
        assert!(g.granted >= floor, "granted {} < floor {floor}", g.granted);
        assert_eq!(socket_drops(&s), Some(0), "SO_MEMINFO reads a fresh socket's drops");
    }

    /// `2 × min(req, rmem_max)`, or `2 × req` as root.
    #[cfg(target_os = "linux")]
    pub(crate) fn linux_floor(req: usize) -> usize {
        if is_root() {
            return 2 * req;
        }
        let rmem_max: usize = std::fs::read_to_string("/proc/sys/net/core/rmem_max")
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .expect("/proc/sys/net/core/rmem_max readable");
        2 * req.min(rmem_max)
    }
}
