# FreeFetch Subtitle Discovery Filter Report

## 1. Classification & Status
**Status**: **SUBTITLE DISCOVERY FILTER — PRODUCTION READY**  
**Date**: October 27, 2026  
**Scope**: YouTube subtitle extraction, filtering of machine-translated captions, preservation of manual and original auto-generated subtitles.

---

## 2. Executive Summary & Root Cause Analysis

### Observed Problem
When extracting YouTube video metadata, FreeFetch previously surfaced all tracks found in the provider's caption payloads. For videos with auto-generated subtitles, YouTube generates up to 150+ derived, machine-translated tracks (e.g., Inuktitut `[IU]`, Irish `[GA]`, French `[FR]`, Indonesian `[ID]`).

When users selected these translated auto-generated tracks for downloading, embedding (`-c:s mov_text`), or hardcoding (`-vf subtitles=...`), downloads failed with network or processing errors.

### Root Cause
1. **TimedText API Rate Limiting (`tlang=`)**:
   YouTube's backend dynamically translates ASR (automatic speech recognition) tracks when the URL query includes `&tlang=<target_language>`. Direct HTTP requests by clients to these dynamic translation endpoints trigger `HTTP 429: Too Many Requests` or return truncated/empty payloads.
2. **Original ASR Track Stability**:
   The native audio transcription track (e.g. `kind=asr&lang=id` for an Indonesian video) does not include `tlang=`. It returns standard WebVTT/SRT data with `HTTP 200 OK` reliably.
3. **yt-dlp Metadata Keys**:
   yt-dlp represents the original ASR transcript under `<lang>-orig` in `automatic_captions` (e.g. `id-orig`, `en-orig`), alongside bare alias keys (`id`, `en`) and machine translations (`iu`, `ga`).

---

## 3. Discovery & Filtering Strategy

### Authoritative Architecture Rule
> **"FreeFetch exposes manually supplied subtitle tracks and the video's original auto-generated subtitle track. Translated/derived auto-generated subtitle tracks are filtered from the selectable subtitle list because they are not reliably supported by the current extraction pipeline."**

### Identification Matrix

| Category | Source in yt-dlp / API | Filter Criterion | Presentation | Action |
| :--- | :--- | :--- | :--- | :--- |
| **Manual Subtitles** | `subtitles` dict | `is_auto_generated: false` | Standard language name + code (e.g. `English [en]`) | **Retain 100%** |
| **Original Auto-Generated** | `automatic_captions` dict | Key ends with `-orig` OR matches native video language without `tlang=` | Display: `"{Name} (Original) (Auto-generated)"`<br>Code: `<lang>-orig` (e.g. `[id-orig]`) | **Retain 1 track** |
| **Bare Duplicate Auto Key** | `automatic_captions` dict | Key equals bare code (e.g. `id`) when `-orig` exists | Suppressed to eliminate duplicate choices | **Discard** |
| **Derived / Translated Auto** | `automatic_captions` dict | Query string contains `tlang=` | Unreliable HTTP 429 endpoints | **Discard** |

---

## 4. Implementation Details

### A. YouTube Extractor Normalization ([`src-tauri/src/extractor/implementations/youtube.rs`](file:///D:/Antigravity/FreeFetch/src-tauri/src/extractor/implementations/youtube.rs))
1. **Helper Function `is_translated_caption_url`**:
   Inspects query parameters for `tlang=` in timedtext URLs.
2. **`normalize_ytdlp_subtitles` Logic**:
   - Preserves all manual subtitles from `dump.subtitles`.
   - Inspects `dump.automatic_captions`. If any key ends with `-orig`, filters out all non-`-orig` keys and bare duplicate keys.
   - If no `-orig` key exists, checks video language and drops all URLs containing `tlang=`.
   - Sets `language_code` to `<lang>-orig` (e.g. `id-orig`, `en-orig`) and formats `display_name` to `"{Language} (Original) (Auto-generated)"`.
3. **`try_extract_with_innertube` Parity**:
   Applies identical `is_translated_caption_url` filtering and original track label formatting.

### B. Subtitle URL Refresh Resilience ([`src-tauri/src/engine/mod.rs`](file:///D:/Antigravity/FreeFetch/src-tauri/src/engine/mod.rs))
When refreshing expired URLs during download execution, FreeFetch matches requested tracks using:
```rust
s.language_code.starts_with(lang.as_str()) || lang.starts_with(&s.language_code)
```
This guarantees backward compatibility if a queue item or client references `"id"` while the refreshed extractor returns `"id-orig"`.

### C. Downloader Architecture Protection
- No modifications were made to `parallel_downloader.rs`, `RangeWorkQueue`, `StallWatchdog`, or chunk management.
- Queue concurrency, SQLite persistence, and IPC contracts remain fully intact and frozen.

---

## 5. Verification & Test Results

### 1. Integration & Unit Tests ([`src-tauri/tests/youtube_subtitle_filter_tests.rs`](file:///D:/Antigravity/FreeFetch/src-tauri/tests/youtube_subtitle_filter_tests.rs))
7 targeted test scenarios implemented and verified:
- `test_filter_youtube_subtitles_keeps_orig_and_drops_translated`: Verifies `id-orig` retained, duplicate `id` and translated `iu`, `ga`, `en` discarded.
- `test_filter_preserves_manual_subtitles`: Verifies manual tracks (`en`, `es`) kept intact while translated auto tracks dropped.
- `test_filter_auto_generated_without_orig_key`: Verifies filtering when `-orig` key is absent but `tlang=` is present.
- `test_filter_handles_no_subtitles`: Verifies empty input handles safely without panic.
- `test_is_translated_caption_url`: Verifies detection of `&tlang=`, `?tlang=`, and clean URLs.
- `test_real_youtube_indonesian_video_subtitles`: Real-world extraction test on Indonesian YouTube video (`YMXJICVux6A`) yielding only `[id-orig]`.
- `test_real_youtube_english_video_subtitles`: Real-world extraction test on English YouTube video (`j8n1pj8RfFs`) yielding only `[en-orig]`.

### 2. Comprehensive Test Suite
- **Rust Backend Tests**: `cargo test --manifest-path src-tauri/Cargo.toml`  
  **Result**: **180 passed, 0 failed** (32 unit tests + 148 integration tests).
- **Clippy Code Quality**: `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings`  
  **Result**: **PASS (0 warnings)**.
- **Rust Formatting**: `cargo fmt --manifest-path src-tauri/Cargo.toml --check`  
  **Result**: **PASS (0 diffs)**.
- **Frontend Vitest Suite**: `npm test`  
  **Result**: **36 passed, 0 failed** (27 frontend tests + 9 settings tests).
- **Vite Production Bundle**: `npm run build`  
  **Result**: **PASS** (1958 modules transformed).
- **Release Compilation**: `cargo build --release --manifest-path src-tauri/Cargo.toml`  
  **Result**: **PASS** (`target/release/freefetch.exe` successfully compiled).

---

## 6. Conclusion
The subtitle discovery filter successfully eliminates fragile and rate-limited machine-translated auto-generated tracks while preserving manual captions and the single verified original auto-generated caption track. Downloader, queue, audio, and UI subsystems remain completely unaffected and production-ready.
