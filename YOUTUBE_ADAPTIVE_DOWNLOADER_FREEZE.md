# YouTube Adaptive Downloader Architecture Freeze

**Document**: `YOUTUBE_ADAPTIVE_DOWNLOADER_FREEZE.md`  
**Milestone**: Downloader Speed Optimization & Adaptive Connection Recovery Freeze  
**Status**: **FROZEN / PRODUCTION READY**  
**Effective Date**: 2026-10-27  
**Authoritative Implementation**: [`src-tauri/src/engine/parallel_downloader.rs`](file:///D:/Antigravity/FreeFetch/src-tauri/src/engine/parallel_downloader.rs)

---

## 1. Subsystem Freeze Declaration

The YouTube Adaptive Parallel Downloader subsystem is formally frozen.

> **No further performance tuning should modify this subsystem unless a future benchmark demonstrates a material production defect or regression.**

This freeze encompasses:
1. **`RangeWorkQueue`**: Shared work-stealing queue coordinating concurrent worker range claims.
2. **YouTube 2 MiB Dynamic Sub-Ranges**: Uniform 2,097,152-byte chunk partitioning for YouTube streams.
3. **`StallWatchdog`**: Rolling throughput monitor evaluating per-worker transfer health.
4. **Stall Recovery**: Bounded socket termination and remainder re-requesting.
5. **Fresh-Connection Recovery**: Immediate re-dispatch over a clean TCP socket to reset CDN connection pacing.
6. **Positional Writes**: Thread-safe Windows `SetFilePointerEx` + `WriteFile` (`seek_write`) file operations.
7. **`.part.meta` Consistency**: Crash-resilient range tracking and atomic state serialization.
8. **Retry Limits**: Hard bounded stall recovery budget (`MAX_STALL_RECOVERIES_PER_RANGE = 3`).
9. **Cancellation & Pause Safety**: Clean watcher propagation and immediate unclaim on interruption.

---

## 2. Production Invariants

### Invariant 1: Range Partitioning Separation
- **YouTube Media Streams**: Media URLs hosted on `googlevideo.com` or `youtube.com` MUST be partitioned into uniform 2 MiB sub-ranges (`YOUTUBE_SUBRANGE_SIZE = 2 * 1024 * 1024`).
- **Non-YouTube Streams**: Standard CDNs (Cloudflare, direct servers) MUST preserve monolithic partitions (`total_bytes.div_ceil(concurrency)`) to avoid unnecessary connection churn where single connections sustain 18+ MB/s.

### Invariant 2: Shared Range Work-Stealing
- Workers claim sub-ranges dynamically from `RangeWorkQueue`.
- No sub-range is permanently bound to a worker. If a worker finishes or stalls, work items are claimed cooperatively.
- Once a sub-range completes, it is atomically marked `is_finished = true` and saved to `.part.meta`.

### Invariant 3: Stall Watchdog Criteria
A worker connection is classified as stalled if and only if:
- Body streaming is active.
- Rolling throughput over a 1.0-second window is strictly `< 100 KB/s` (`DEFAULT_STALL_THRESHOLD_BYTES_PER_SEC`).
- This condition persists continuously for `> 3.0 seconds` (`DEFAULT_STALL_DURATION`).
- The remaining unwritten bytes in the sub-range exceed `64 KB` (`STALL_MIN_REMAINING_BYTES`).
- Stall recovery count for that sub-range is strictly `< 3` (`MAX_STALL_RECOVERIES_PER_RANGE`).

### Invariant 4: Positional File I/O Soundness
- Positional writes MUST continue using `PositionalFileWriter::write_at(offset, buf)` (`seek_write`).
- No shared mutable file cursors or intermediate segment stitching files are permitted.
- Bytes downloaded before a stall recovery MUST be preserved. The recovery request begins strictly at `range.start_byte + range.downloaded_bytes`.
- Zero duplicate bytes and zero gaps.

### Invariant 5: Crash Resiliency & Metadata Integrity
- The `.part.meta` file records exact range boundaries and written bytes.
- On pause or restart, completed ranges are preserved; incomplete ranges resume from their last recorded written offset.
- Upon 100% download completion, the file is synced (`sync_all`), `.part.meta` is deleted, and the `.part` file is promoted to the destination.

### Invariant 6: Security & Process Model
- FreeFetch maintains direct-vector subprocess execution.
- No shell interpreters (`cmd.exe`, `powershell.exe`) are invoked.
- Absolute path traversal checks and Windows reserved name sanitization are strictly preserved.
- No authentication bypasses, Botguard integrations, or PO-token minting scripts are permitted in this subsystem.

---

## 3. Configuration Constants Reference

```rust
pub const DEFAULT_STALL_THRESHOLD_BYTES_PER_SEC: u64 = 100 * 1024; // 100 KB/s
pub const DEFAULT_STALL_DURATION: Duration = Duration::from_secs(3); // 3 seconds
pub const MAX_STALL_RECOVERIES_PER_RANGE: usize = 3;
pub const STALL_MIN_REMAINING_BYTES: u64 = 64 * 1024; // 64 KB
pub const YOUTUBE_SUBRANGE_SIZE: u64 = 2 * 1024 * 1024; // 2 MiB
```

---

## 4. Verification Evidence

- **Unit & Integration Tests**: 173/173 PASS ([`tests/youtube_adaptive_tests.rs`](file:///D:/Antigravity/FreeFetch/src-tauri/tests/youtube_adaptive_tests.rs))
- **Frontend Tests**: 35/35 PASS
- **Clippy**: Clean with `-D warnings`
- **Formatting**: `cargo fmt --check` clean
- **Release Build**: `freefetch.exe` production binary verified
