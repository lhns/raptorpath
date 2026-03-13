# ADR-0012: Platform Setup Is Too Complex and Error-Prone

## Status
**Resolved** — TOML config file support, profile presets (home/datacenter), preflight checks with actionable errors, subcommands (run/check/status), netsh uses async tokio::process::Command.

## Context
Running raptorpath requires multiple manual steps that aren't documented, validated, or automated.

## Problems

### Windows
1. **wintun.dll not bundled**: user must download from wintun.net and place in PATH or working directory. Failure error is cryptic (`LoadLibrary failed`).
2. **Admin required**: creating TUN adapter and running `netsh` both need elevation. No check or helpful error.
3. **netsh is blocking**: `std::process::Command` in async context. Should use `tokio::process::Command`.
4. **No cleanup**: if the process crashes, the wintun adapter persists and may conflict on restart.

### Linux
1. **Root or CAP_NET_ADMIN required**: `tun::create_as_async()` fails with `EPERM`. Error message is the raw kernel error with no context.
2. **No privilege drop**: process runs as root for its entire lifetime. Should create TUN, then drop to unprivileged user.
3. **No systemd integration**: no service file, no `Type=notify`, no socket activation.

### Both platforms
1. **No pre-flight checks**: the binary should verify all requirements before starting (TUN driver available, admin rights, bind addresses reachable, peer reachable).
2. **CLI is expert-only**: `--target-tail-loss 1e-5` and `--max-fec-overhead 0.5` are meaningless to non-experts.
3. **No config file**: everything must be passed as CLI args. For persistent setups, this is painful.

## Decision Required

### Immediate
1. Bundle `wintun.dll` in the release artifacts (or auto-download on first run)
2. Add a `preflight_check()` that validates environment before starting
3. Add clear error messages: "This program requires administrator privileges. Right-click and Run as Administrator."
4. Use `tokio::process::Command` for netsh

### Short-term
1. Add `--config <path>` for TOML config file support
2. Add presets: `--profile home-wifi-lte`, `--profile datacenter-multipath`
3. Add `raptorpath check` subcommand that validates setup without starting

### Medium-term
1. Linux: drop privileges after TUN creation
2. Linux: provide systemd service file
3. Windows: provide installer that bundles wintun.dll
4. Both: auto-detect available network interfaces and suggest bind addresses

## Example improved UX
```bash
# Instead of:
raptorpath --bind 0.0.0.0:4433,0.0.0.0:4434 --peer 1.2.3.4:4433,1.2.3.4:4434 \
  --tun-name rpath0 --tun-addr 10.99.0.1/24 --target-tail-loss 1e-5

# This:
raptorpath connect 1.2.3.4 --profile home
# (auto-detects interfaces, creates TUN, applies sensible defaults)
```

## Consequences
- Lower barrier to entry
- Fewer support issues from misconfiguration
- Slightly more code complexity in setup phase
