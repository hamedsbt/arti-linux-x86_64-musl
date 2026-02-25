# tor-snowflake

Snowflake pluggable transport for Arti.

## Overview

This crate implements a Snowflake pluggable transport client that works
on both native platforms and WASM. It enables Tor connections through
WebRTC-based Snowflake proxies, providing censorship circumvention.

## Architecture

The crate provides:

- **Snowflake broker client** — negotiates WebRTC connections via the Snowflake broker
- **WebRTC data channel transport** — communicates with Snowflake proxies over WebRTC
- **KCP reliable transport** — provides reliable delivery over the unreliable WebRTC data channel
- **TLS layer** — wraps the KCP stream in TLS for the Tor protocol

## Platform Differences

| Component | Native | WASM |
|-----------|--------|------|
| WebRTC | tokio-tungstenite | web-sys RtcPeerConnection |
| TLS | rustls (ring) | rustls (RustCrypto) |
| Async runtime | tokio | wasm-bindgen-futures |
