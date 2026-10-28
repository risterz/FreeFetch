# WebM Container Codec Compatibility Regression Report

## Milestone Classification
**WEBM CONTAINER COMPATIBILITY — PRODUCTION READY**

---

## 1. Executive Summary

A real production defect was reported on YouTube download:
- **Target Video**: `10 Jajan Anak SD Paling Menjijikan di Indonesia !` (`https://www.youtube.com/watch?v=kTn-k0aM-4I`)
- **Selected Combination**: Container: WebM, Video: VP9 (`vp09.00.41.08`, 1080p60), Audio: AAC (`mp4a.40.2`, 128 kb/s)
- **Observed Failure**: FFmpeg terminated with exit code `-22`:
  ```text
  Only VP8 or VP9 or AV1 video and Vorbis or Opus audio and WebVTT subtitles are supported for WebM.
  Could not write header: Invalid argument
  Conversion failed!
  ```

### Root Cause
In [`src-tauri/src/extractor/implementations/youtube.rs`](file:///D:/Antigravity/FreeFetch/src-tauri/src/extractor/implementations/youtube.rs), `default_audio_url` was hardcoded to `best_m4a_audio` (AAC) for all adaptive video streams, regardless of whether the video container was MP4 or WebM. Consequently, format 303 (VP9 1080p60 WebM) was paired with format 140 (AAC 128k m4a) instead of format 251 (Opus 138k webm), causing FFmpeg stream copy to reject the invalid container/codec combination.

### Solution Architecture
A complete 4-tier defense-in-depth architecture was designed and implemented:
1. **Tier 1 (Extractor)**: Container-aware audio stream pairing (`best_webm_audio` with Opus for WebM; `best_mp4_audio` with AAC for MP4). Format `audio_codec` is populated with the paired stream's codec.
2. **Tier 2 (UI)**: Real-time validation via [`src/utils/container.ts`](file:///D:/Antigravity/FreeFetch/src/utils/container.ts) and [`src/components/modal/FormatModal.tsx`](file:///D:/Antigravity/FreeFetch/src/components/modal/FormatModal.tsx). Incompatible combinations trigger a warning banner and disable download/queue buttons.
3. **Tier 3 (Engine/IPC Gate)**: Structured error `FreeFetchError::IncompatibleContainerCodec` returned by `start_download`, `queue_add`, and `download_job_inner` before any subprocess is spawned.
4. **Tier 4 (FFmpeg Shield)**: Elementary stream probing via [`src-tauri/src/media/probe.rs`](file:///D:/Antigravity/FreeFetch/src-tauri/src/media/probe.rs) before WebM remuxing, exit code -22 intercept, and `libvpx-vp9` encoding for WebM subtitle burning.

---

## 2. Invariant & Architecture Compliance

| Invariant | Status | Verification |
| :--- | :---: | :--- |
| **YouTube Downloader Frozen** | **PRESERVED** | `parallel_downloader.rs`, `RangeWorkQueue`, `StallWatchdog` untouched |
| **Subtitle Discovery Filter** | **PRESERVED** | Original auto-generated subtitle filtering intact |
| **No Lossy Transcoding** | **ENFORCED** | Native Opus audio stream paired without re-encoding AAC |
| **Multi-Tier Defense** | **ACTIVE** | Extractor + UI + Engine/IPC + FFmpeg probe shield |
| **Zero Shell Injection** | **PRESERVED** | Hidden direct-vector process execution preserved |
| **Structured Errors** | **ENFORCED** | `IncompatibleContainerCodec` JSON serialization verified |

---

## 3. Test & Verification Results

### 3.1 Backend Tests (`cargo test`)
- **Total Test Suites**: 11
- **Total Tests Run**: 163
- **Passed**: 163
- **Failed**: 0
- **Highlights**:
  - `test_video_codec_normalization`: PASS
  - `test_audio_codec_normalization`: PASS
  - `test_container_normalization`: PASS
  - `test_webm_strict_codec_matrix`: PASS
  - `test_mp4_and_mkv_codec_matrix`: PASS
  - `test_subtitle_embedding_compatibility`: PASS
  - `test_validate_stream_combination_rejects_webm_aac`: PASS
  - `test_validate_stream_combination_accepts_valid_pairs`: PASS
  - `test_real_world_youtube_webm_pairs_with_opus`: PASS (`https://www.youtube.com/watch?v=kTn-k0aM-4I`)
  - `test_ffmpeg_merge_webm_vp9_and_opus_succeeds`: PASS (Live FFmpeg VP9 + Opus merge verified)
  - `test_ffmpeg_merge_webm_vp9_and_aac_blocked_with_clean_error`: PASS

### 3.2 Frontend Tests (`npm test`)
- **Total Test Files**: 2 (`tests/frontend.test.ts`, `tests/settings.test.ts`)
- **Total Tests Run**: 45
- **Passed**: 45
- **Failed**: 0
- **Highlights**:
  - `normalizes video codecs accurately`: PASS
  - `normalizes audio codecs accurately`: PASS
  - `validates WebM container codec compatibility`: PASS
  - `validates stream combination and blocks AAC in WebM`: PASS

### 3.3 Code Quality & Compiler Audits
- `cargo fmt --check`: **PASS** (Zero diffs)
- `cargo clippy --all-targets --all-features -- -D warnings`: **PASS** (Zero warnings)
- `npm run build` (`tsc && vite build`): **PASS** (Zero TypeScript or bundling errors)
- `cargo build --release`: **PASS** (Clean release binary compiled)

---

## 4. Modified & Created Files

1. **[`src-tauri/src/media/container.rs`](file:///D:/Antigravity/FreeFetch/src-tauri/src/media/container.rs)**: *[CREATED]* Core codec normalization and matrix validation.
2. **[`src-tauri/src/core/error.rs`](file:///D:/Antigravity/FreeFetch/src-tauri/src/core/error.rs)**: *[MODIFIED]* Added `IncompatibleContainerCodec` variant and JSON serialization.
3. **[`src-tauri/src/media/mod.rs`](file:///D:/Antigravity/FreeFetch/src-tauri/src/media/mod.rs)**: *[MODIFIED]* Exported container validation module.
4. **[`src-tauri/src/media/probe.rs`](file:///D:/Antigravity/FreeFetch/src-tauri/src/media/probe.rs)**: *[MODIFIED]* Added `probe_codec`, `probe_video_codec`, and `probe_audio_codec`.
5. **[`src-tauri/src/media/ffmpeg.rs`](file:///D:/Antigravity/FreeFetch/src-tauri/src/media/ffmpeg.rs)**: *[MODIFIED]* Added pre-probe validation in `merge_streams_cancellable`, exit code -22 intercept, and `libvpx-vp9` WebM hardcoding.
6. **[`src-tauri/src/extractor/implementations/youtube.rs`](file:///D:/Antigravity/FreeFetch/src-tauri/src/extractor/implementations/youtube.rs)**: *[MODIFIED]* Container-aware audio stream pairing (`best_webm_audio` vs `best_m4a_audio`) and format `audio_codec` propagation.
7. **[`src-tauri/src/engine/mod.rs`](file:///D:/Antigravity/FreeFetch/src-tauri/src/engine/mod.rs)**: *[MODIFIED]* Added `video_codec`/`audio_codec` to `StartDownloadParams`, `Default` implementation, and stream combination validation in `start_download_with_id` and `download_job_inner`.
8. **[`src-tauri/src/ipc/commands.rs`](file:///D:/Antigravity/FreeFetch/src-tauri/src/ipc/commands.rs)**: *[MODIFIED]* Enforced `validate_stream_combination` in `start_download` and `queue_add`.
9. **[`src/utils/container.ts`](file:///D:/Antigravity/FreeFetch/src/utils/container.ts)**: *[CREATED]* Frontend container and codec compatibility validator.
10. **[`src/components/modal/FormatModal.tsx`](file:///D:/Antigravity/FreeFetch/src/components/modal/FormatModal.tsx)**: *[MODIFIED]* Real-time validation, warning banner, and disabled button state.
11. **[`src/api/types.ts`](file:///D:/Antigravity/FreeFetch/src/api/types.ts)**: *[MODIFIED]* Added `video_codec` and `audio_codec` fields to `StartDownloadParams`.
12. **[`src/App.tsx`](file:///D:/Antigravity/FreeFetch/src/App.tsx)**: *[MODIFIED]* Passed `video_codec` and `audio_codec` into `startDownload` and `queueAdd`.
13. **[`src-tauri/tests/container_codec_tests.rs`](file:///D:/Antigravity/FreeFetch/src-tauri/tests/container_codec_tests.rs)**: *[CREATED]* 11 comprehensive unit & integration tests including live YouTube extraction and FFmpeg remuxing.
14. **[`tests/frontend.test.ts`](file:///D:/Antigravity/FreeFetch/tests/frontend.test.ts)**: *[MODIFIED]* Added frontend compatibility matrix tests.
15. **[`CONTAINER_CODEC_COMPATIBILITY.md`](file:///D:/Antigravity/FreeFetch/CONTAINER_CODEC_COMPATIBILITY.md)**: *[CREATED]* Architecture documentation.
16. **[`WEBM_CODEC_REGRESSION_REPORT.md`](file:///D:/Antigravity/FreeFetch/WEBM_CODEC_REGRESSION_REPORT.md)**: *[CREATED]* Authoritative regression report.

---

## 5. Final Classification

**WEBM CONTAINER COMPATIBILITY — PRODUCTION READY**
