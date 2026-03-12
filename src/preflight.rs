//! Pre-flight environment validation.
//!
//! Checks that the environment meets all requirements before starting,
//! providing clear, actionable error messages.

use std::fmt;
use std::net::SocketAddr;

/// Result of a single preflight check.
#[derive(Debug)]
pub struct CheckResult {
    pub name: &'static str,
    pub status: CheckStatus,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CheckStatus {
    Pass,
    Warn,
    Fail,
}

impl fmt::Display for CheckResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let tag = match self.status {
            CheckStatus::Pass => "[PASS]",
            CheckStatus::Warn => "[WARN]",
            CheckStatus::Fail => "[FAIL]",
        };
        write!(f, "{} {} — {}", tag, self.name, self.message)
    }
}

/// Run all preflight checks.
pub fn run_checks(
    bind_addrs: &[SocketAddr],
    is_server: bool,
    status_addr: Option<SocketAddr>,
) -> Vec<CheckResult> {
    let mut results = vec![];

    results.push(check_privileges());
    results.push(check_tun_driver());

    for addr in bind_addrs {
        results.push(check_bind_address(*addr));
    }

    if let Some(addr) = status_addr {
        results.push(check_bind_address_named(addr, "Status endpoint"));
    }

    if is_server && bind_addrs.is_empty() {
        results.push(CheckResult {
            name: "Bind addresses",
            status: CheckStatus::Fail,
            message: "Server mode requires at least one --bind address.".into(),
        });
    }

    results
}

/// Check for administrator/root privileges.
fn check_privileges() -> CheckResult {
    #[cfg(target_os = "windows")]
    {
        // Try to open a registry key that requires admin
        use std::process::Command;
        let output = Command::new("net").arg("session").output();
        match output {
            Ok(o) if o.status.success() => CheckResult {
                name: "Privileges",
                status: CheckStatus::Pass,
                message: "Running as Administrator.".into(),
            },
            _ => CheckResult {
                name: "Privileges",
                status: CheckStatus::Fail,
                message: "Not running as Administrator. Right-click the terminal and select 'Run as Administrator'.".into(),
            },
        }
    }

    #[cfg(target_os = "linux")]
    {
        let euid = unsafe { libc::geteuid() };
        if euid == 0 {
            CheckResult {
                name: "Privileges",
                status: CheckStatus::Pass,
                message: "Running as root.".into(),
            }
        } else {
            CheckResult {
                name: "Privileges",
                status: CheckStatus::Fail,
                message: "Not running as root. Run with: sudo raptorpath ...".into(),
            }
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        CheckResult {
            name: "Privileges",
            status: CheckStatus::Warn,
            message: "Cannot check privileges on this platform.".into(),
        }
    }
}

/// Check that TUN driver is available.
fn check_tun_driver() -> CheckResult {
    #[cfg(target_os = "windows")]
    {
        // Check if wintun.dll can be found
        let dll_check = unsafe { wintun::load() };
        match dll_check {
            Ok(_) => CheckResult {
                name: "TUN driver",
                status: CheckStatus::Pass,
                message: "wintun.dll loaded successfully.".into(),
            },
            Err(e) => CheckResult {
                name: "TUN driver",
                status: CheckStatus::Fail,
                message: format!(
                    "wintun.dll not found: {e}. Download from https://www.wintun.net/ \
                     and place wintun.dll next to the raptorpath executable or in your PATH."
                ),
            },
        }
    }

    #[cfg(target_os = "linux")]
    {
        if std::path::Path::new("/dev/net/tun").exists() {
            CheckResult {
                name: "TUN driver",
                status: CheckStatus::Pass,
                message: "/dev/net/tun is available.".into(),
            }
        } else {
            CheckResult {
                name: "TUN driver",
                status: CheckStatus::Fail,
                message: "/dev/net/tun not found. Load the tun kernel module: sudo modprobe tun".into(),
            }
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        CheckResult {
            name: "TUN driver",
            status: CheckStatus::Warn,
            message: "TUN driver check not implemented for this platform.".into(),
        }
    }
}

/// Check that a bind address is available.
fn check_bind_address(addr: SocketAddr) -> CheckResult {
    check_bind_address_named(addr, "Bind address")
}

fn check_bind_address_named(addr: SocketAddr, label: &str) -> CheckResult {
    match std::net::UdpSocket::bind(addr) {
        Ok(_) => CheckResult {
            name: "Port availability",
            status: CheckStatus::Pass,
            message: format!("{label} {addr} is available."),
        },
        Err(e) => CheckResult {
            name: "Port availability",
            status: CheckStatus::Fail,
            message: format!(
                "{label} {addr} is not available: {e}. \
                 Check if another program is using this port."
            ),
        },
    }
}

/// Print all check results and return whether any failed.
pub fn print_and_check(results: &[CheckResult]) -> bool {
    let mut has_failure = false;
    for r in results {
        match r.status {
            CheckStatus::Pass => println!("  {r}"),
            CheckStatus::Warn => {
                println!("  {r}");
            }
            CheckStatus::Fail => {
                println!("  {r}");
                has_failure = true;
            }
        }
    }
    has_failure
}
