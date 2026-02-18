# tor-async-compat

async_trait wrapper that uses `?Send` on WASM.

## Overview

This is a proc-macro crate that provides an `#[async_trait]` attribute
which automatically uses `?Send` bounds when compiling for WASM targets.
On native platforms, it behaves like the standard `#[async_trait]` macro.

This allows crates to use a single `#[async_trait]` annotation that works
correctly on both native and WASM without conditional compilation.

## Usage

```rust
use tor_async_compat::async_trait;

#[async_trait]
trait MyTrait {
    async fn do_something(&self) -> Result<(), Error>;
}
```

On native targets this expands to `#[async_trait]` (with `Send` bounds).
On WASM targets this expands to `#[async_trait(?Send)]`.

## Why `?Send` is safe on WASM

WASM (wasm32-unknown-unknown) is single-threaded — there are no threads
to send futures between. The `Send` bound that `#[async_trait]` normally
requires exists to allow futures to be transferred across threads in
multi-threaded runtimes like tokio. Since WASM has no threads, this
bound is unnecessary and only serves to prevent compilation when a future
captures non-`Send` types (e.g. JS values from `web-sys` / `wasm-bindgen`
which are inherently `!Send`).

Dropping the `Send` requirement on WASM lets us hold JS-side objects
(WebRTC connections, WebSocket handles, DOM references) across `.await`
points without wrapper hacks, while keeping the stricter `Send` bound on
native targets where thread safety still matters.
