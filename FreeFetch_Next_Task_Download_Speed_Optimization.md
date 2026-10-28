# FreeFetch — Next Development Task
## Optimize Download Speed Without Breaking Reliability

PROJECT:

```text
D:\Antigravity\FreeFetch
```

CURRENT STATUS:

FreeFetch v0.1 MVP is working.

Verified:

- YouTube URL analysis
- YouTube format selection
- Separate video/audio streams
- HTTP Range downloads
- `.part` resumability
- Automatic retry
- Exponential backoff
- YouTube signed URL refresh
- HTTP 403/410 recovery
- Pause/resume
- Cancel
- FFmpeg remux
- SQLite persistence
- Real YouTube downloads
- Network interruption recovery
- 13 resilience tests passing
- 9 unit tests passing
- 6 integration tests passing
- 6 frontend tests passing

IMPORTANT:

Do NOT rewrite the downloader architecture.

Do NOT remove the existing resilience/retry system.

Do NOT weaken any existing tests.

The current problem is:

**Download speed is significantly slower than the user's actual internet connection.**

Example observed behavior:

```text
Download:
0.9 MB / 8.4 MB
55.4%
Speed: ~31 KB/s

Network:
Stable enough to eventually complete, but FreeFetch is using bandwidth very inefficiently.
```

The download eventually completed successfully, so reliability is working.

Now optimize throughput.

---

# 1. FIRST — AUDIT THE CURRENT DOWNLOAD PATH

Before changing code, inspect:

```text
src-tauri/src/engine/
src-tauri/src/engine/chunk_downloader.rs
src-tauri/src/engine/worker.rs
src-tauri/src/engine/mod.rs
src-tauri/src/engine/queue.rs
src-tauri/src/engine/rate_limiter.rs
```

Determine:

1. Is each stream currently downloaded using one HTTP request?
2. Is HTTP connection reuse enabled?
3. Is the reqwest Client shared or recreated per request?
4. Are HTTP requests sequential?
5. Is Range downloading actually concurrent?
6. How large are the read buffers?
7. Is disk I/O blocking network I/O?
8. Is the global rate limiter accidentally limiting downloads?
9. Is there an artificial speed limit enabled by default?
10. Is progress calculation causing excessive UI/event overhead?
11. Are video and audio streams downloaded sequentially?
12. Is YouTube CDN throughput being limited by request configuration?

Do not guess.

Produce a short internal diagnosis before implementing changes.

---

# 2. BENCHMARK CURRENT THROUGHPUT

Create a deterministic benchmark using the existing mock HTTP server.

Measure:

```text
1. Single sequential download
2. Single Range download
3. Multiple concurrent Range requests
4. Different chunk sizes
5. Different concurrency levels
```

Test at least:

```text
1 connection
2 connections
4 connections
8 connections
```

And chunk sizes such as:

```text
4 MB
8 MB
16 MB
32 MB
64 MB
```

Do not assume that maximum concurrency is fastest.

Find the best balance.

---

# 3. IMPLEMENT ADAPTIVE RANGE DOWNLOAD

If the current downloader uses one sequential HTTP request, implement bounded parallel Range downloading.

Example:

```text
File
├── Range 0 ────────────────┐
├── Range 1 ────────────────┤
├── Range 2 ────────────────┤
└── Range 3 ────────────────┘
                             ↓
                         .part file
```

Important:

DO NOT allow unlimited connections.

Start with:

```text
default concurrency = 4
maximum concurrency = 8
```

Make the implementation configurable internally.

Do not expose a complicated setting in the UI yet.

---

# 4. PRESERVE FILE INTEGRITY

Parallel chunks must never corrupt the `.part` file.

Each worker must have a deterministic byte range:

```text
worker 1:
bytes 0 - N

worker 2:
bytes N+1 - 2N

worker 3:
bytes 2N+1 - 3N

worker 4:
bytes 3N+1 - END
```

Use positional writes or another safe strategy.

DO NOT allow multiple workers to append to the same file cursor.

Each chunk must write to its correct offset.

---

# 5. RESUME SUPPORT

This is critical.

The existing resumability MUST remain intact.

If:

```text
400 MB total
250 MB already downloaded
```

FreeFetch must not restart from zero.

It should determine which ranges are already complete and only download missing ranges.

If a chunk fails:

```text
Chunk 3 fails
        ↓
retry Chunk 3
        ↓
do NOT restart chunks 1, 2 and 4
```

Existing retry/backoff logic must remain active at the individual request/chunk level.

---

# 6. YOUTUBE COMPATIBILITY

YouTube media URLs are temporary.

Do not persist signed media URLs.

Keep:

```text
Canonical YouTube URL
Video ID
Selected format information
```

as the persistent state.

Signed `googlevideo.com` URLs remain memory-only.

If a parallel range request receives:

```text
403
410
```

use the existing URL refresh mechanism.

After refreshing:

```text
identify failed range
        ↓
obtain fresh media URL
        ↓
retry ONLY failed range
        ↓
continue
```

Do not restart the complete stream.

---

# 7. CONNECTION REUSE

Inspect the reqwest Client architecture.

Prefer a shared HTTP client with connection pooling rather than creating a new Client for every Range request.

Configure sensible connection behavior.

Do not create hundreds of connections.

Target:

```text
HTTP connection pool
        ↓
4–8 active requests
        ↓
reuse existing connections
```

Keep TLS verification enabled.

Do not weaken certificate validation.

---

# 8. BUFFERING

Benchmark the current buffer size.

If the implementation is using very small read buffers, increase them appropriately.

Test reasonable values such as:

```text
64 KB
256 KB
1 MB
```

Choose based on benchmark results.

Do not blindly choose the largest value.

Avoid excessive memory usage.

---

# 9. DISK I/O

Ensure disk writes do not unnecessarily block network workers.

Inspect whether:

```text
network read
    ↓
disk write
    ↓
network read
```

is serializing throughput.

Use appropriate buffered/async file operations.

For parallel Range requests, use positional writes safely.

Do not trade reliability for speed.

---

# 10. VIDEO + AUDIO STREAMS

Current architecture downloads:

```text
video stream
audio stream
```

before:

```text
FFmpeg remux
```

Investigate whether video and audio can safely download concurrently.

Preferred:

```text
Video Range Workers ──────┐
                          ├── FFmpeg
Audio Range Workers ──────┘
```

However:

DO NOT exceed the global connection/concurrency limit.

For example:

```text
Global maximum:
8 workers

Video:
6 workers

Audio:
2 workers
```

or dynamically allocate workers.

Do not blindly use 8 + 8 connections.

---

# 11. RATE LIMITER AUDIT

Inspect:

```text
src-tauri/src/engine/rate_limiter.rs
```

Verify that the default configuration is NOT unintentionally limiting bandwidth.

The user currently has no intention of using a download speed limiter.

If no explicit speed limit is configured:

```text
NO ARTIFICIAL SPEED LIMIT
```

The rate limiter should only activate when the user explicitly configures one.

Add a regression test proving:

```text
default settings
→ unlimited download throughput
```

---

# 12. PROGRESS EVENT OPTIMIZATION

Do not emit a Tauri frontend event for every tiny read operation.

This can cause unnecessary IPC/UI overhead.

Throttle progress events.

Target approximately:

```text
5–10 progress updates per second
```

while still maintaining smooth UI animation.

Backend should maintain accurate byte counters.

Frontend should render smoothly without receiving thousands of events per second.

---

# 13. SPEED CALCULATION

Improve speed measurement.

Use a rolling measurement window rather than calculating speed from a single tiny interval.

Display:

```text
8.4 MB/s
```

rather than unstable values such as:

```text
31 KB/s
0 B/s
2.3 MB/s
17 KB/s
```

every few milliseconds.

Use a sensible moving average.

ETA should use the same stabilized throughput measurement.

---

# 14. ADAPTIVE CONCURRENCY

If practical, implement adaptive behavior.

Start:

```text
4 workers
```

If throughput improves:

```text
4 → 6 → 8
```

If the CDN starts returning:

```text
429
503
timeouts
connection resets
```

reduce concurrency:

```text
8 → 6 → 4 → 2
```

After stability returns, gradually increase again.

Do not create an aggressive feedback loop.

The priority is:

```text
Reliability > Speed
```

---

# 15. DO NOT OVERLOAD YOUTUBE

Important:

Do NOT implement unlimited parallel requests.

Do NOT attempt to bypass YouTube throttling.

Do NOT rotate IPs.

Do NOT evade rate limits.

Do NOT implement proxy rotation.

Do NOT attempt to defeat anti-abuse systems.

Use reasonable HTTP behavior.

The goal is efficient use of the user's available network bandwidth, not bypassing service restrictions.

---

# 16. TESTS

Add automated tests for:

### Range correctness

```text
4 parallel ranges
→ correct final file
```

### Range failure

```text
worker 2 fails
→ worker 2 retries
→ other completed chunks remain untouched
```

### Resume

```text
partial file
→ only missing ranges downloaded
```

### URL refresh

```text
range request → 403
→ refresh YouTube URL
→ retry same range
```

### Concurrency reduction

```text
429/503
→ reduce concurrency
```

### Rate limiter

```text
no user limit
→ no artificial throttling
```

### Progress events

```text
large download
→ event frequency remains bounded
```

### File integrity

Generate a deterministic test file and compare:

```text
SHA-256(original)
==
SHA-256(downloaded)
```

---

# 17. REAL-WORLD YOUTUBE TEST

After implementation, test a public YouTube video.

Use a reasonably large file so speed differences are measurable.

Test:

```text
720p
1080p
1440p or 2160p if available
```

Record:

```text
format
file size
average speed
peak speed
download duration
retry count
number of workers
```

Compare:

```text
BEFORE
AFTER
```

Do not claim a speed improvement unless measured.

---

# 18. RELIABILITY REGRESSION

Repeat the previous unstable-network scenario.

The downloader must still:

```text
timeout
↓
retry
↓
resume
↓
continue
↓
complete
```

Verify that parallel downloading does not break the resilience system.

Especially test:

```text
403 during parallel download
```

and:

```text
timeout during parallel download
```

---

# 19. UI

Do NOT redesign the UI.

Only make small improvements if necessary.

The existing UI should continue showing:

```text
Downloading
Retrying
Completed
Failed
```

For retry status, change confusing wording such as:

"retrying from 103 MB (attempt 5/5)"

to something clearer:

"Connection interrupted — recovering from 103 MB..."

If the retry limit is actually exhausted, show:

"Retry limit reached"

Do not make "5/5" appear while the system is still successfully recovering.

---

# 20. BUILD AND TEST

Run:

```powershell
cd D:\Antigravity\FreeFetch

cargo fmt --manifest-path src-tauri/Cargo.toml

cargo check --manifest-path src-tauri/Cargo.toml

cargo test --manifest-path src-tauri/Cargo.toml

npm test

npm run build

npm run tauri dev
```

All existing tests must continue passing.

Do not remove or weaken existing resilience tests.

---

# 21. FINAL REPORT

Provide:

1. Current bottleneck
2. Before average speed
3. After average speed
4. Before peak speed
5. After peak speed
6. Selected concurrency
7. Selected chunk size
8. HTTP connection-pooling changes
9. Buffering changes
10. Disk I/O changes
11. Progress-event changes
12. Rate limiter findings
13. Adaptive concurrency behavior
14. Resume behavior
15. YouTube URL refresh behavior
16. New tests
17. Complete test results
18. Real YouTube benchmark results
19. Any remaining bottlenecks

IMPORTANT:

Do not optimize based on assumptions.

Measure first.

Do not sacrifice the existing retry/resume reliability to increase speed.

The final implementation should be:

FAST + RESUMABLE + RELIABLE

not merely FAST.
