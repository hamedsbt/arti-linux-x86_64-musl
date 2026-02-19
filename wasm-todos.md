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
- [ ] Make tor-js API same/similar to https://www.npmjs.com/package/tor-js
  - [x] Use standard `Response` incl streaming
  - [x] Fix awkward initialization
  - [ ] Make initialization portable (don't use wasm file)
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
  - [ ] (Probably needed again later)
- [ ] Fix/deprioritize TODO/FIXMEs added
- [ ] Provide tiny module variant via hash-checked download
- [ ] Code review
- [ ] Merge
- [ ] Publish on npm

## Nice to Have

- [ ] API extensions
  - [ ] Isolated clients (share network caching)
  - [ ] WebSocket
  - [ ] Regular sockets
- [ ] Try making npm module small
- [ ] Fix performance issue(s) affecting normal bootstrap
  - [ ] Microdesc stalls when batch size or parallelism is higher
  - [ ] Downloads unblocked by ping loop
- [ ] Fix tor-fetch.js slow exit (prints response and hangs for a long time)
