# Potential Improvements

Issues discovered during code review that are not included in the current
changes. These may be addressed in future work.

## 1. Streaming/incremental directory downloads

**File:** `crates/tor-dirmgr/src/bootstrap.rs`

**Problem:** The upstream `download_attempt` function fetches all directory
documents in parallel via `fetch_multiple`, collects all responses into a Vec,
then processes them sequentially. This means storage isn't persisted until all
downloads are complete — if the client is interrupted mid-download, no partial
progress is saved.

**Proposed improvement:** Process responses incrementally as they arrive using
`futures::stream::buffer_unordered`, persisting each to storage before waiting
for the remaining downloads. This enables:
- Faster perceived bootstrap (progress updates during download)
- Partial persistence (restart resumes from where it left off)
- Better WASM responsiveness (can yield between processing responses)

**Why not included:** The previous implementation split `download_attempt` into
`#[cfg(test)]` (batch) and `#[cfg(not(test))]` (streaming) code paths. This
meant the production streaming path had zero test coverage — tests exercised
only the batch path. The `process_download_response` extraction was good but the
cfg split was problematic.

**Future approach:** Implement streaming downloads in a way that the test
infrastructure can also exercise the streaming code path, perhaps by making the
response source (canned vs network) configurable without cfg-gating the
processing logic.

## 2. WASM yield in load_and_apply_documents

**File:** `crates/tor-dirmgr/src/bootstrap.rs`

**Problem:** `load_and_apply_documents` processes microdescriptor chunks
synchronously. On WASM (single-threaded), parsing thousands of microdescriptors
blocks the UI thread.

**Proposed improvement:** Make the function async and yield to the event loop
between chunks via `sleep(Duration::ZERO)` on WASM. This was previously
implemented with a `#[cfg(target_arch = "wasm32")]` yield point.

**Why not included:** This is a WASM-specific optimization that belongs in the
WASM support branch, not in the basic time/async compatibility layer. It should
be re-added when the full WASM changes are applied.

## 3. Remove commented-out .get_mut() in GetMicrodescsState

**File:** `crates/tor-dirmgr/src/state.rs:1118`

**What:** There is a commented-out `//.get_mut()` call in `add_from_download`
between `.lock()` and `.expect()`. It appears to be leftover from a refactor
where the storage lock type changed.

**Why not included:** Removing dead comments in upstream code is low-value churn.
Could be cleaned up in a broader code quality pass.
