# YouTube Adaptive Connection Recovery & Dynamic Sub-Range Architecture

**Status**: Phase 1 Design Specification  
**Target Systems**: `src-tauri/src/engine/parallel_downloader.rs`, `src-tauri/src/engine/chunk_downloader.rs`  
**Applies To**: YouTube media streams (`googlevideo.com`, `youtube.com`)  
**Preserves**: Frozen extraction pipeline, direct-vector execution, `.part` positional writing, `.part.meta` crash consistency, IPC contracts, cancellation/pause invariants.

---

## 1. Problem Statement & Background

Empirical benchmarks in `YOUTUBE_CLIENT_THROUGHPUT_EXPERIMENT.md` demonstrated that:
1. Single raw HTTP Range requests of 2 MiB over a fresh connection achieve **~0.33 MB/s**.
2. Static, monolithic range partitioning (e.g., 6 workers × 13.5 MB static partitions) rapidly decays to **~0.06 MB/s** aggregate sustained throughput (~10 KB/s per worker).
3. The root cause is Google CDN's (`googlevideo.com`) aggressive connection-level TCP window throttling (`initcwndbps=648375`). While initial burst windows allow ~300 KB/s, long-lived connections suffer connection pacing and throughput collapse.
4. Static reverse engineering of 4K Video Downloader+ revealed that it counters this decay using dynamic sub-range allocation combined with an active stall monitor (`DownloadTrackerWorkerNode`) that aborts choked TCP sockets and issues fresh Range requests from the `last valid range` byte offset.

---

## 2. Core Architecture

```text
               +-------------------------------------------------+
               |              Total Stream File                   |
               | (e.g., 80 MB divided into 2 MiB sub-ranges)      |
               +-------------------------------------------------+
                                       |
                                       v
               +-------------------------------------------------+
               |          Shared Range Work Queue                |
               | [Sub-range 0] [Sub-range 1] ... [Sub-range 39]  |
               +-------------------------------------------------+
                         /             |             \
                        /              |              \
                       v               v               v
               +---------------+---------------+---------------+
               |   Worker 1    |   Worker 2    |   Worker 3... |
               | (Claims next) | (Claims next) | (Claims next) |
               +---------------+---------------+---------------+
                       |               |               |
                       v               v               v
            [Stall Watchdog]    [Stall Watchdog] [Stall Watchdog]
            - Rolling speed     - Rolling speed  - Rolling speed
            - Timer > 3s        - Timer > 3s     - Timer > 3s
                       | (If stalled < 100 KB/s)
                       v
            [Abort HTTP Stream]
            - Drop socket
            - Calculate remaining unwritten range
            - Re-queue remainder
            - Claim fresh TCP connection
            - Resume positional write
```

---

## 3. Detailed Design

### A. Dynamic Sub-Range Model (YouTube-Specific)

1. **Sub-Range Sizing**:
   - For YouTube media streams (`googlevideo.com` or `youtube.com`), total file length is divided into uniform **2 MiB** (`2,097,152` bytes) sub-ranges.
   - For non-YouTube streams (e.g. Cloudflare, direct servers), the existing monolithic partition scheme (`total_bytes.div_ceil(concurrency)`) is preserved to prevent unnecessary connection churn where CDNs sustain 18+ MB/s.
2. **Work-Stealing Queue**:
   - Instead of 1 fixed range per worker, a shared `RangeQueue` manages all sub-ranges.
   - Any worker that finishes its assigned sub-range immediately claims the next available unfinished sub-range.
   - Faster workers download more ranges; slower workers do not bottleneck total download completion.
3. **Resumption & Crash Recovery**:
   - Sub-ranges are tracked in `PartMetadata.ranges`.
   - If FreeFetch restarts or resumes from `.part.meta`, all sub-ranges with `is_finished == true` are skipped.
   - Unfinished sub-ranges are re-queued from `start_byte + downloaded_bytes`.

### B. Reactive Stall Detection Watchdog

1. **Configurable Constants**:
   ```rust
   pub const DEFAULT_STALL_THRESHOLD_BYTES_PER_SEC: u64 = 100 * 1024; // 100 KB/s
   pub const DEFAULT_STALL_DURATION: Duration = Duration::from_secs(3); // 3 seconds
   pub const MAX_STALL_RECOVERIES_PER_RANGE: usize = 3;
   pub const STALL_RECOVERY_MIN_REMAINING_BYTES: u64 = 64 * 1024; // 64 KB
   ```
2. **Classification Invariants**:
   A worker is classified as **stalled** if and only if ALL conditions hold:
   - The HTTP response headers have been received and body streaming is active.
   - Rolling/instantaneous throughput over a 1-second sampling window is strictly below `STALL_THRESHOLD_BYTES_PER_SEC` (or zero bytes received).
   - This low-throughput state persists continuously for longer than `STALL_DURATION`.
   - The remaining unwritten bytes in the sub-range exceed `STALL_RECOVERY_MIN_REMAINING_BYTES` (avoiding premature abortion when a range is naturally finishing its last few kilobytes).
   - Cancellation has not been signaled (`cancel_rx`).
   - Pause has not been requested.
3. **Watchdog Sampling Algorithm**:
   - Each worker runs a bounded `tokio::select!` block between `stream.next()` and a periodic watchdog timer (or chunk timestamp diff).
   - If chunks stop arriving or throughput fails to meet 100 KB/s for 3 consecutive seconds, a `StallDetected` signal is triggered.

### C. Connection Recovery & Re-queueing Flow

1. **Immediate Socket Teardown**:
   - The worker immediately drops the active response body stream, closing the stalled TCP connection.
2. **Exact Range Math**:
   - `written_bytes = range.downloaded_bytes`
   - `next_offset = range.start_byte + written_bytes`
   - `unwritten_bytes = range.end_byte - next_offset + 1`
3. **Re-queue / Fresh Connection**:
   - If `stall_recoveries < MAX_STALL_RECOVERIES_PER_RANGE`:
     - Range is updated with `start_byte = next_offset` (or range offset adjusted).
     - Worker immediately opens a fresh HTTP Range request: `Range: bytes={next_offset}-{range.end_byte}`.
     - YouTube CDN grants a new `initcwndbps` burst window on the fresh TCP socket.
4. **Positional Write Soundness**:
   - Writing continues via `PositionalFileWriter::write_at(next_offset, buf)`.
   - No file cursor movement; zero byte duplication; zero risk of file truncation or corruption.

### D. Retry Limits & Fault Tolerance

1. **Recovery Budget**:
   - A single sub-range is permitted at most `MAX_STALL_RECOVERIES_PER_RANGE = 3` stall recoveries.
   - If a sub-range stalls 3 times in succession, the stall watchdog stops killing the connection and allows the normal stream to continue or fall back to the existing exponential backoff retry handler.
2. **Preservation of Existing Error Handling**:
   - `HTTP 403 / 410`: Handled by `url_refresher` callback to refresh the signed media URL.
   - `HTTP 429 / 503`: Handled by existing exponential backoff (`tokio::time::sleep(delay)`).
   - `HTTP 416`: Handled as completion.

### E. Cancellation & Pause Safety

1. **Cancel Propagation**:
   - `cancel_rx` is checked before claiming any sub-range and on every stream chunk iteration.
   - Aborting a stalled connection immediately yields `Err(FreeFetchError::Cancelled)` if cancellation was requested.
2. **Pause / Shutdown**:
   - On pause, current sub-ranges write their latest `downloaded_bytes` to `.part.meta` before exiting cleanly.
   - When resumed, remaining bytes are loaded from `.part.meta` and queued without loss.

---

## 4. Invariant Checklist

- [x] Positional file writes (`seek_write`) unchanged.
- [x] Security model intact (no shell execution, path canonicalization preserved).
- [x] Non-YouTube downloads unaffected.
- [x] `.part.meta` updated atomically per completed sub-range.
- [x] Zero duplicate bytes or file corruption.
