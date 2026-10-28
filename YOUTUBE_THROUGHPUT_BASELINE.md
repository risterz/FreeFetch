# YouTube Throughput Benchmark Baseline

**Document**: `YOUTUBE_THROUGHPUT_BASELINE.md`  
**Milestone**: Downloader Performance Baseline  
**Verification Date**: 2026-10-27  
**Test Media Target**: Rick Astley - Never Gonna Give You Up (`https://www.youtube.com/watch?v=dQw4w9WgXcQ`)

---

## 1. Verified Benchmark Metrics

The following metrics represent the authoritative production benchmark established for FreeFetch's parallel range downloader:

| Metric | Value |
| :--- | :--- |
| **Test Format** | YouTube Format 137 |
| **Resolution** | 1080p FHD (1920x1080) |
| **File Size** | 80,911,999 bytes (~77.16 MB) |
| **Active Workers** | 6 |
| **Sub-Range Chunk Size** | 2 MiB (2,097,152 bytes) |
| **Baseline Sustained Speed** | ~0.04–0.06 MB/s |
| **Adaptive Sustained Speed** | **~0.09 MB/s** |
| **Baseline 30s Volume** | 1.23 MB (1,291,950 bytes) |
| **Adaptive 30s Volume** | **2.84 MB (2,981,888 bytes)** |
| **Throughput Volume Improvement** | **+130.8%** |
| **Adaptive Peak Speed** | **0.40 MB/s** |
| **Errors Observed** | **0** |
| **File Integrity** | **Verified (100% positional byte match, FFprobe valid)** |

---

## 2. Comparative Analysis

```text
Throughput Comparison (30-second measurement window):

Baseline (Monolithic Partitioning):
[============] 1.23 MB (~0.04–0.06 MB/s)

Adaptive Downloader (2 MiB Work-Stealing + Watchdog):
[============================] 2.84 MB (~0.09 MB/s sustained, 0.40 MB/s peak)
```

1. **Burst Advantage**: Dividing the stream into 2 MiB sub-ranges allows each worker to capture fresh TCP slow-start burst windows (`initcwndbps=648375`), driving peak speeds up to **0.40 MB/s** (higher than raw single-connection requests at 0.33 MB/s).
2. **Stall Mitigation**: Workers that stall below 100 KB/s for longer than 3 seconds abort their choked sockets and re-request remaining unwritten bytes, doubling sustained volume (+130.8%).

---

## 3. Scope Boundary: Multi-Megabyte Throughput

While downloader-side adaptive connection recovery doubles throughput on standard YouTube media streams, **multi-megabyte throughput (5–10 MB/s) remains strictly outside the current scope**.

As proven in `YOUTUBE_CLIENT_THROUGHPUT_EXPERIMENT.md`:
- YouTube CDN (`googlevideo.com`) universally throttles raw media URLs from Web/VisionOS clients to ~50–80 KB/s per connection.
- Mobile and TV streams (`ANDROID`, `IOS`, `TVHTML5`) that avoid this specific throttle require valid Proof-of-Origin (PO) or Botguard tokens.
- FreeFetch strictly adheres to its clean-room, direct-vector execution boundary and does not implement unauthorized authentication bypasses, Botguard extractors, or token-minting VMs.
