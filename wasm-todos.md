# Wasm TODOs

## Required

- [ ] Code review tor-time
- [ ] Merge tor-time
- [ ] Code review tor-async-compat
- [ ] Merge tor-async-compat
- [x] Fix pre-existing tests (`cargo test`) (see `scripts/check.sh`)
- [ ] Automated testing
- [x] Fix warnings, clippy
- [ ] Check/clean ai code
- [x] Make tor-js API same/similar to https://www.npmjs.com/package/tor-js
  - [x] Use standard `Response` incl streaming
  - [x] Fix awkward initialization
  - [x] Make initialization portable (don't use wasm file)
  - [x] Fix `TorClient` async new
  - [x] Logging
  - [x] Remove rust inmemory storage
  - [x] FileSystem storage
  - [x] IndexedDb storage
  - [x] In-memory storage
  - [x] Platform-dependent (browser/nodejs) default storage
- [ ] Fix slow bootstrap (via non-tor sources if needed)
- Sync with arti main branch
  - [x] 206e629
  - [x] 9306eec
  - [x] 5c13837
  - [x] 606f3ab63
  - [ ] (Probably needed again later)
- [x] Fix/deprioritize TODO/FIXMEs added
- [ ] Implement/fix missing storage locking in JS
- [x] Provide tiny module variant via hash-checked download
  - jsdelivr/unpkg/githubusercontent
- [x] WebRTC demo
- [ ] Fix: Repeated "Unable to select a guard relay" issues when changing bridges (sometimes?)
  - Sometimes it recovers (can complete request), other times it gets stuck
- [x] Fix sourcemaps
- [x] Check npm pack content
- [ ] Add singleton entry points
- [ ] README for npm package
- [ ] Check incremental build logic in tor-js (rerun-if-changed prevents ordinary cache invalidation? intent was to build only more often, never skip)
- [ ] Confirm trace logging works in js
- [ ] Fix wasm-base64 (should be self-contained, but requires `<script type="importmap">` in showcase index.html)
- [ ] Fix slow js storage startup (loading '000s items into memory - fix with parallelism or chunked storage of microdescs)
- [ ] Code review
- [ ] Merge
- [ ] Publish on npm

## Nice to Have

- [ ] API extensions
  - [ ] Isolated clients (share network caching)
  - [ ] WebSocket
  - [ ] Regular sockets
- [ ] Make wasm small
- [ ] Fix performance issue(s) affecting normal bootstrap
  - [ ] Microdesc stalls when batch size or parallelism is higher
  - [ ] Downloads unblocked by ping loop
- [ ] Fix tor-fetch.js slow exit (prints response and hangs for a long time)
