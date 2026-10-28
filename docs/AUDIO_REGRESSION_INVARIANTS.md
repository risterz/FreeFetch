# FreeFetch Audio System Regression Invariants

> **Mandatory Quality Gates & Regression Contracts**  
> Status: FROZEN / PRODUCTION READY  
> Scope: All future FreeFetch pull requests, refactors, and feature additions.

Any modification to FreeFetch that violates any of the following invariants is a breaking regression and must not be merged.

---

### Invariant A1 — Audio Must Contain a Valid Audio Stream
- **Rule**: Every generated `.mp3`, `.m4a`, `.flac`, or `.opus` file must be a valid container containing the expected audio codec (`libmp3lame`, `aac`, `flac`, or `libopus`).
- **Enforcement**: Automated tests must inspect file streams via FFprobe JSON or demuxer tests.
- **Prohibition**: Never produce empty files, video-only files, or mock containers.

### Invariant A2 — Audio Must Not Require Video Streams
- **Rule**: Audio conversion must operate exclusively on raw audio streams (e.g. YouTube format 140 / 251 or direct audio feeds).
- **Enforcement**: The pipeline must never download or require high-resolution video streams when the user requests an audio-only download.

### Invariant A3 — Audio Must Never Embed Subtitles
- **Rule**: Audio downloads must never attempt subtitle embedding, regardless of whether subtitles were selected globally or inherited from batch settings.
- **Guard**: [`src-tauri/src/engine/mod.rs:maybe_embed_subtitles`](file:///D:/Antigravity/FreeFetch/src-tauri/src/engine/mod.rs#L1266) must maintain the guard:
  ```rust
  if !params.embed_subtitles || params.selected_subtitles.is_empty() || params.audio_options.is_some() {
      return Ok(());
  }
  ```
- **Prohibition**: Never pass subtitle mapping flags (`-map 0:s`) or video stream mapping (`-map 0:v`) during audio conversion.

### Invariant A4 — Cancellation Must Terminate and Reap Subprocesses
- **Rule**: Cancelling an audio job during FFmpeg conversion must immediately terminate the FFmpeg child process and reap its process handle.
- **Enforcement**: Subprocesses must use `.kill_on_drop(true)` and explicitly await `child.kill().await` and `child.wait().await`.
- **Prohibition**: No orphaned `ffmpeg.exe` processes may linger in Windows Task Manager after cancellation or failure.

### Invariant A5 — Cancellation Must Purge Incomplete Artifacts
- **Rule**: When an audio download is cancelled, all associated temporary and partial artifacts must be deleted from disk:
  1. Partial final output file (`<destination>`).
  2. Raw download chunks (`.part`, `.part.meta`, `.audio.part`, `.audio.part.meta`).
  3. Temporary cover artwork (`{job_id}.cover.tmp.jpg` and `*.cover.tmp.jpg`).
- **Enforcement**: Verified via [`clean_cancelled_files_with_id`](file:///D:/Antigravity/FreeFetch/src-tauri/src/engine/mod.rs#L437) and automated cancellation tests.

### Invariant A6 — Strict Filename Traversal and Reserved Name Immunity
- **Rule**: No user-supplied title, artist, album, uploader, or custom template may escape the intended destination folder or crash the Windows filesystem.
- **Mandates**:
  - All directory traversal sequences (`..`, `/`, `\`) must be converted to underscores.
  - Windows illegal characters (`< > : " / \ | ? *`) must be stripped.
  - Windows device names (`CON`, `PRN`, `AUX`, `NUL`, `COM1..9`, `LPT1..9`) must be prefixed with `_`.
  - Stems must be truncated to 200 characters or less.

### Invariant A7 — Zero Shell Interpolation / Direct Vector Execution
- **Rule**: FFmpeg must be invoked strictly through direct process argument vectors ([`std::process::Command`](file:///D:/Antigravity/FreeFetch/src-tauri/src/media/ffmpeg.rs#L300)).
- **Prohibition**: Never invoke FFmpeg through a shell interpreter (`cmd.exe`, `powershell.exe`, `/bin/sh`). Never concatenate command strings with format strings.

### Invariant A8 — Concurrency & Queue Slot Isolation
- **Rule**: An audio job in `Processing` state occupies one active concurrency slot until FFmpeg finishes, fails, or is cancelled.
- **Isolation**: Failure, pause, or cancellation of an audio item must never leak active queue slots, block other items, or corrupt active jobs in the queue.

### Invariant A9 — FLAC Must Always Be Lossless
- **Rule**: FLAC encoding must remain bit-exact lossless (`-compression_level 5`).
- **Prohibition**: Never expose lossy bitrate controls for FLAC in the UI or allow `custom_bitrate_kbps` in backend validation. `AudioConversionOptions::validate` must reject custom bitrates for FLAC.

### Invariant A10 — Accurate Opus Artwork Representation
- **Rule**: The application must not claim to support attached picture streams in Ogg Opus files while silently generating invalid streams.
- **Current Behavior**: `AudioTargetFormat::Opus.supports_artwork()` returns `false`. FFmpeg skips attached picture video mapping for Opus. Any future artwork support for Opus must use base64 `METADATA_BLOCK_PICTURE` Vorbis comments without breaking playback.

### Invariant A11 — Metadata Tag Sanitization
- **Rule**: Metadata tags must be cleaned of ASCII control characters (`\x00..\x1F`), promotional noise, and surrounding quotes before being passed to FFmpeg.
- **Invariance**: Tag values must not cause FFmpeg argument parsing errors or corrupt player tag libraries.

### Invariant A12 — Video and Subtitle Pipeline Non-Regression
- **Rule**: Audio features and models must never alter or regress the existing video download engine, parallel chunk downloader, subtitle parser/converter, or SQLite history repository.
- **Enforcement**: Full test suites (`tests/unit_tests.rs`, `tests/resilience_tests.rs`, `tests/subtitle_tests.rs`, `tests/queue_tests.rs`, `tests/hardening_tests.rs`, `tests/speed_optimization_tests.rs`) must continue to pass 100% green.

---

# Subtitle System Invariants (S1–S3)

### Invariant S1 — Strict Separation of Subtitle Processing Modes
- **Rule**: FreeFetch must strictly differentiate between `SubtitleMode::None`, `SubtitleMode::Embed`, and `SubtitleMode::Hardcode`.
- **Soft Embedding (`Embed`)**: Must use lossless remuxing (`-c:v copy -c:a copy -c:s mov_text`) and produce an MP4 with selectable subtitle track(s). Multi-language selection is supported.
- **Hardcoding (`Hardcode`)**: Must physically re-encode video frames (`-vf subtitles='...' -c:v libx264 -crf 20 -preset medium -c:a copy`) with exactly ONE subtitle track burned into pixels. The final container must contain 0 subtitle streams.
- **Audio Guard (Invariant A3)**: `params.audio_options.is_some()` must NEVER process, embed, or hardcode subtitles under any circumstances.

### Invariant S2 — Zero Shell Interpolation & Safe Subtitle Filter Path Escaping
- **Rule**: Subtitle filter paths passed to `-vf subtitles=...` must be strictly sanitized and escaped for FFmpeg's double-parsing filtergraph engine:
  - Windows backslashes normalized to `/`.
  - Colons escaped as `\:`.
  - Single quotes double-escaped as `'\\\''`.
  - Entire path wrapped in single quotes `'...'`.
- **Execution**: Subprocesses must be spawned via direct argument vectors ([`Command::new`](file:///D:/Antigravity/FreeFetch/src-tauri/src/media/ffmpeg.rs)), never string-interpolated into a shell command.

### Invariant S3 — Atomic Subtitle File Lifecycle and Cancellation Cleanup
- **Rule**: Intermediate subtitle artifacts (`{job_id}.embed.*.srt`, `{job_id}.hardcode.*.srt`, `embed.tmp.*`, `hardcode.tmp.*`) must be removed from disk upon completion, failure, cancellation, or job deletion. No orphaned temporary files or stalled background encoding tasks may persist.
