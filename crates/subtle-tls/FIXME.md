# subtle-tls known issues

Audit date: 2026-02-16. 30 findings: 4 critical, 7 high, 9 medium, 10 low.

## Critical

### C-1: No basicConstraints (CA) validation — chain forgery
`cert.rs` `verify_chain_signatures` — Intermediate certificates are not checked
for `basicConstraints: CA=TRUE`. A leaf certificate could be used as an
intermediate to sign arbitrary certificates. `pathLenConstraint` is also not
enforced. RFC 5280 §4.2.1.9.

### C-2: No keyUsage / extendedKeyUsage checks
`cert.rs` `verify_chain_signatures` — No validation that intermediates have
`keyCertSign` set or that leaf certificates include `id-kp-serverAuth` (OID
1.3.6.1.5.5.7.3.1). A code-signing or email cert would be accepted as a TLS
server cert. RFC 5280 §4.2.1.3, §4.2.1.12.

### C-3: PKCS#1 v1.5 accepted in TLS 1.3 CertificateVerify
`cert.rs:931-934` — Signature algorithms 0x0401, 0x0501, 0x0601 (RSASSA-PKCS1-
v1_5) are accepted for CertificateVerify. RFC 8446 §4.4.3 states these "MUST NOT
be used" in TLS 1.3 CertificateVerify — only RSA-PSS is allowed. These are only
valid for signatures within certificate chains.

### C-4: Intermediate certificate validity period never checked
`cert.rs` `verify_chain` — `verify_validity` is only called on the leaf cert.
Expired intermediates are silently accepted. RFC 5280 §6.1.3 requires validity
checking on every certificate in the path.

## High

### H-1: No nameConstraints validation
`cert.rs` `verify_chain_signatures` — The `nameConstraints` extension is
entirely ignored. An intermediate constrained to `.example.com` would be
accepted issuing certs for any domain. RFC 5280 §4.2.1.10.

### H-2: RSA-PSS hash hardcoded to SHA-256
`cert.rs:516-528` — The RSA-PSS OID (1.2.840.113549.1.1.10) branch always uses
SHA-256 instead of parsing the hash from the AlgorithmIdentifier parameters.
Certs signed with RSA-PSS/SHA-384 or SHA-512 will fail verification.

### H-3: IP address SAN matching broken
`cert.rs:118-126` — X.509 SAN IP addresses are raw bytes (4 for IPv4, 16 for
IPv6), not UTF-8 strings. `std::str::from_utf8(ip_bytes)` will almost never
succeed. Not a problem for Tor (hostname-based) but broken for direct-IP.

### H-4: Cross-signed root skips its own signature verification
`cert.rs:386-431` `try_cross_signed_root` — The cross-signed cert is matched to
a trusted root by subject string only. Its own signature is never verified. The
penultimate cert is verified against the self-signed root from the trust store,
but the cross-signed cert's authenticity is never established.

### H-5: Trust store uses string-based DN comparison
`trust_store.rs:188-299` — All root matching functions compare Distinguished
Names via `.to_string()`. RFC 5280 §7.1 requires complex matching rules
(whitespace normalization, case-insensitive, ASN.1 encoding differences). This
can cause both false negatives (connection failures) and false positives
(security issue).

### H-6: `unsafe impl Send` for TlsStream
`stream.rs:69` — `TlsStream` contains JS objects (`CryptoKey`) which are `!Send`.
The `unsafe impl Send` is justified by WASM being single-threaded, but becomes
unsound if wasm-threads (SharedArrayBuffer) is ever enabled.

### H-7: ServerHello cipher suite not validated against ClientHello
`handshake.rs:350-353` — The server's chosen cipher suite is accepted without
checking it was offered in the ClientHello. RFC 8446 §4.1.3 requires this
check. A rogue server selecting 0x1302 (AES-256-GCM with SHA-384) would cause
incorrect key derivation since the code uses SHA-256 throughout.

## Medium

### M-1: No certificate revocation checking (CRL/OCSP)
No CRL downloads, OCSP queries, or OCSP stapling support. Revoked certificates
are accepted until they naturally expire.

### M-2: Handshake state machine does not enforce message ordering
`stream.rs:264-390` — Uses independent boolean flags but never validates that
messages arrive in the required order (EncryptedExtensions → Certificate →
CertificateVerify → Finished). Out-of-order messages are accepted.

### M-3: EncryptedExtensions not parsed or validated
`stream.rs:343-347` — The EncryptedExtensions body is entirely ignored. ALPN
negotiation result is never confirmed. Extensions not offered in ClientHello
are not rejected. RFC 8446 §4.3.1.

### M-4: ECDSA signature coord size derived from hash, not curve
`cert.rs:749-751`, `cert.rs:1025-1027` — DER-to-raw ECDSA conversion uses hash
name to determine coord byte length. P-384/SHA-256 would produce 64 bytes when
96 are expected. The correct curve is already extracted in
`get_ec_curve_from_key` but not threaded through.

### M-5: No HelloRetryRequest handling or downgrade detection
`stream.rs:207-254`, `handshake.rs:328-421` — HRR (ServerHello with special
random) is treated as a normal ServerHello, causing protocol confusion. TLS
1.2/1.1 downgrade sentinels in `server_random` last 8 bytes are never checked.
RFC 8446 §4.1.3, §4.1.4.

### M-6: Unknown handshake message types silently accepted
`stream.rs:376-379` — Unknown types are logged, added to transcript, and
continued. RFC 8446 does not permit unknown types during the main handshake;
this should abort the connection.

### M-7: `skip_verification` as public struct field
`lib.rs:64-72` — Easy to set accidentally. Should use a builder pattern or
explicit danger method (like rustls `danger_accept_invalid_certs()`).

### M-8: ServerHello session ID length not bounds-checked
`handshake.rs:342-344` — `pos` is advanced by `1 + session_id_len` without
checking bounds first. The later check catches it, but the code relies on
accidental correctness rather than explicit validation.

### M-9: Handshake message buffer has no size limit
`stream.rs:306` — `handshake_buffer` grows without limit. A malicious server
could send fragmented records causing unbounded memory growth.
`MAX_HANDSHAKE_MESSAGE_SIZE` is defined but never enforced.

## Low

### L-1: X25519 all-zero shared secret not rejected
`crypto.rs:200-221` — RFC 7748 §6.1 requires aborting if the shared secret is
all zeros (small-subgroup attack). `x25519-dalek` does not reject this.

### L-2: EC curve defaults to P-256 on parse failure
`cert.rs:592-596` — Falls back to P-256 with a warning instead of returning an
error. Could cause silent wrong-curve verification.

### L-3: Certificate list length uses saturating arithmetic
`handshake.rs:725` — `saturating_add` masks overflow from malicious `list_len`.
Should explicitly check `pos + list_len > data.len()`.

### L-4: No TLS alert sent on handshake failure
`stream.rs` — Fatal errors return `Err` without sending a TLS alert to the
server. RFC 8446 §6 requires appropriate fatal alerts.

### L-5: No maximum certificate chain depth
`cert.rs:211-326` — Chain building loop has no depth limit. A server sending
hundreds of certs causes O(n) parsing and verification. Most implementations
cap at 10-20.

### L-6: Secret key material not zeroized on drop
`handshake.rs:68-97`, `record.rs:31-35`, `stream.rs:61-64` — Cryptographic
secrets stored in `Vec<u8>` are not zeroized when dropped. Should use the
`zeroize` crate with `ZeroizeOnDrop`.

### L-7: Empty legacy session ID in ClientHello
`handshake.rs:136` — RFC 8446 §4.1.2 recommends a 32-byte random session ID
for middlebox compatibility (Appendix D.4). An empty ID may cause failures
through certain TLS-inspecting middleboxes.

### L-8: Excessive info-level logging of key derivation
`handshake.rs:425-496`, `crypto.rs:201-219` — Key derivation steps logged at
`info!` level. Should be `debug!` or `trace!` to avoid persisting timing
information in production logs.

### L-9: `unwrap()` on `ordered_chain.last()`
`cert.rs:223` — Relies on implicit invariant. Should use `.expect()` or
propagate an error for defense-in-depth.

### L-10: `supported_versions` extension not required in ServerHello
`handshake.rs:381-391` — Checked if present but not required. RFC 8446 §4.1.3
mandates it for TLS 1.3 servers.