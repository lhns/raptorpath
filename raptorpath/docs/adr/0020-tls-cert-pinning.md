# ADR-0020: Optional TLS Certificate Pinning

## Status: Resolved

## Context

RaptorPath uses QUIC for transport, which requires TLS 1.3. For development
and self-hosted deployments, the server generates a self-signed certificate
on each startup. The client currently skips certificate verification entirely
(`SkipCertVerification`), which is acceptable for testing but provides no
authentication in production — any MITM can impersonate the server.

Full PKI (CA-signed certificates, OCSP, CRL) is overkill for a point-to-point
tunnel where both endpoints are controlled by the same operator.

## Decision

Add optional **certificate pinning** via SHA-256 fingerprint comparison.

### How It Works

1. **Server startup**: Generates a self-signed cert (as before) and logs
   the SHA-256 fingerprint to stdout:
   ```
   Server certificate fingerprint (SHA-256): ab12cd34...
   ```

2. **Client configuration**: The operator saves the server's certificate
   (DER or PEM format) and passes it via `--pin-cert <path>` or the
   `pin_cert` config field.

3. **Verification**: `PinnedCertVerifier` computes SHA-256 of the server's
   presented certificate and compares it against the expected hash loaded
   from the pinned file. Mismatch → connection refused.

4. **No pin specified**: Falls back to `SkipCertVerification` (current
   behavior) for backwards compatibility and development use.

### Implementation

- `PinnedCertVerifier` implements `rustls::client::danger::ServerCertVerifier`
- `load_pinned_cert_hash()` reads DER files directly or extracts DER from PEM
- `sha256_fingerprint()` computes SHA-256 over the raw DER bytes
- Config wired through `RaptorpathConfig.pin_cert` → `PeerConfig.pin_cert`
  → `QuicTransport::new()`

### Dependencies Added

- `sha2` — SHA-256 computation
- `hex` — Fingerprint display
- `pem` — PEM file parsing

## Alternatives Considered

1. **Pre-shared key (PSK)**: Simpler but doesn't compose with QUIC's TLS 1.3
   handshake. Would require a custom QUIC extension or a separate auth layer.

2. **TOFU (Trust On First Use)**: Auto-pin on first connection, like SSH.
   More convenient but vulnerable to MITM on the first connection. Could be
   added later as an optional mode.

3. **Full PKI with CA**: Proper CA-signed certs with revocation. The right
   answer for large deployments but too heavy for the typical 1-server,
   1-client tunnel use case.

## Consequences

- Production deployments can authenticate the server without a PKI
- Zero UX change for development (no pin = same as before)
- Server prints fingerprint on startup for easy copy-paste to client config
- Supports both DER and PEM certificate formats
- Future: could extend to mutual TLS (client cert pinning) with similar approach
