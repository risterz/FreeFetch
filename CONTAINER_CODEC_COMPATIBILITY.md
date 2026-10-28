# Container and Codec Compatibility Architecture

## 1. Overview & Objective

FreeFetch strictly enforces container and codec compatibility invariants so that invalid stream combinations (such as pairing VP9 video with AAC audio into a WebM container) can never be produced by the extractor, scheduled by the queue, submitted across IPC, or sent to FFmpeg.

FFmpeg enforces strict codec constraints on container formats:
- **WebM**: Only supports **VP8**, **VP9**, or **AV1** video, and **Opus** or **Vorbis** audio. Subtitle tracks must be **WebVTT**. Attempting to multiplex AAC (`mp4a`) or other codecs into WebM causes FFmpeg to terminate with exit code `-22` (`Invalid argument` / `Could not write header`).
- **MP4 / M4V / MOV**: Supports **H.264**, **H.265/HEVC**, **AV1**, **VP9** video, and **AAC**, **MP3**, **Opus**, **ALAC**, **AC3**, **E-AC-3** audio. Subtitles must be `mov_text`.
- **MKV (Matroska)**: Universal container accepting all standard video, audio, and subtitle streams.

---

## 2. Multi-Tier Defense Matrix

To guarantee zero regression and absolute reliability, FreeFetch implements defense-in-depth across four distinct tiers:

```
[ Tier 1: Extractor Pairing ]
   └── Matches WebM video with best Opus audio (ITAG 251/250/249)
   └── Matches MP4 video with best AAC audio (ITAG 140)
   └── Populates format.audio_codec for adaptive formats

[ Tier 2: Frontend UI Validation (FormatModal.tsx) ]
   └── Real-time validation via src/utils/container.ts
   └── Displays prominent alert banner for incompatible selections
   └── Disables "Download Now" and "Add to Queue" buttons

[ Tier 3: Engine & IPC Command Gate (commands.rs & engine/mod.rs) ]
   └── start_download, start_download_with_id, queue_add, download_job_inner
   └── Validates container, video_codec, audio_codec, subtitle_mode
   └── Returns structured FreeFetchError::IncompatibleContainerCodec

[ Tier 4: FFmpeg Subprocess Shield (ffmpeg.rs & probe.rs) ]
   └── Pre-probes video and audio elementary streams before remuxing to WebM
   └── Intercepts FFmpeg exit code -22 ("Only VP8 or VP9 or AV1...")
   └── Encodes with libvpx-vp9 for WebM hardcoded subtitles (no libx264 in WebM)
```

---

## 3. Tier 1: Extractor Automatic Pairing

Located in [`src-tauri/src/extractor/implementations/youtube.rs`](file:///D:/Antigravity/FreeFetch/src-tauri/src/extractor/implementations/youtube.rs):
- **yt-dlp Extraction (`normalize_ytdlp_dump`)**:
  - Discovers `best_webm_audio` (`ext == "webm"` or `acodec` is `opus`/`vorbis`).
  - Discovers `best_m4a_audio` (`ext == "m4a"` or `acodec` is `mp4a`/`aac`).
  - When constructing adaptive video `MediaFormat`:
    - If `container == "webm"`, pairs with `best_webm_audio` (falling back to `best_any_audio`).
    - If `container == "mp4"`, pairs with `best_m4a_audio` (falling back to `best_any_audio`).
  - Sets `format.audio_codec` to the paired stream's codec so callers and downstream validation inspect the true combined profile.
- **Innertube Extraction (`try_extract_with_innertube`)**:
  - Extracts both `best_webm_audio_url` (`audio/webm`) and `best_mp4_audio_url` (`audio/mp4`).
  - Pairs WebM video formats with WebM Opus audio and MP4 formats with AAC audio.

---

## 4. Tier 2: Frontend Compatibility Utility & UI

- **Utility Module**: [`src/utils/container.ts`](file:///D:/Antigravity/FreeFetch/src/utils/container.ts)
  - `normalizeVideoCodec`: Canonicalizes `vp09.*` -> `vp9`, `av01.*` -> `av1`, `avc1.*` -> `h264`, `hev1.*` -> `hevc`.
  - `normalizeAudioCodec`: Canonicalizes `mp4a.*` -> `aac`, `opus` -> `opus`, `vorbis` -> `vorbis`, `mp3` -> `mp3`.
  - `isVideoCodecCompatible(container, vcodec)`: Enforces WebM video constraints.
  - `isAudioCodecCompatible(container, acodec)`: Enforces WebM audio constraints.
  - `validateStreamCombination`: Evaluates container, video, audio, and subtitle mode.
- **UI Component**: [`src/components/modal/FormatModal.tsx`](file:///D:/Antigravity/FreeFetch/src/components/modal/FormatModal.tsx)
  - Runs `validateStreamCombination` whenever an active format is selected.
  - Displays a red/amber alert banner if an incompatible container/codec combination is detected:
    > "AAC audio is not compatible with WebM. Choose Opus/Vorbis audio or switch the container to MP4/MKV."
  - Disables the `Download Now` and `Add to Queue` buttons with the descriptive error reason.

---

## 5. Tier 3: Engine & IPC Command Gate

- **Error Definition**: [`src-tauri/src/core/error.rs`](file:///D:/Antigravity/FreeFetch/src-tauri/src/core/error.rs)
  ```rust
  #[error("Incompatible codec for container {container}: {message}")]
  IncompatibleContainerCodec { container: String, message: String }
  ```
  Serialized as a first-class structured JSON object: `{"type": "IncompatibleContainerCodec", "container": "webm", "message": "..."}`.
- **Backend Validation Module**: [`src-tauri/src/media/container.rs`](file:///D:/Antigravity/FreeFetch/src-tauri/src/media/container.rs)
  - Exports `validate_stream_combination` and normalizers.
- **Enforcement Points**:
  - `start_download` IPC command ([`src-tauri/src/ipc/commands.rs`](file:///D:/Antigravity/FreeFetch/src-tauri/src/ipc/commands.rs#L59))
  - `queue_add` IPC command ([`src-tauri/src/ipc/commands.rs`](file:///D:/Antigravity/FreeFetch/src-tauri/src/ipc/commands.rs#L244))
  - `start_download_with_id` in [`src-tauri/src/engine/mod.rs`](file:///D:/Antigravity/FreeFetch/src-tauri/src/engine/mod.rs#L202)
  - `download_job_inner` before initiating downloads ([`src-tauri/src/engine/mod.rs`](file:///D:/Antigravity/FreeFetch/src-tauri/src/engine/mod.rs#L1000))

---

## 6. Tier 4: FFmpeg Subprocess Shield

- **Elementary Stream Probing**: [`src-tauri/src/media/probe.rs`](file:///D:/Antigravity/FreeFetch/src-tauri/src/media/probe.rs)
  - Added `probe_codec`, `probe_video_codec`, and `probe_audio_codec` using `ffprobe -select_streams <selector> -show_entries stream=codec_name`.
- **Pre-remux Validation**: [`src-tauri/src/media/ffmpeg.rs`](file:///D:/Antigravity/FreeFetch/src-tauri/src/media/ffmpeg.rs#L186)
  - Before invoking `ffmpeg` for WebM containers, `merge_streams_cancellable` probes both elementary files to confirm video is VP8/VP9/AV1 and audio is Opus/Vorbis.
- **FFmpeg Error Intercept**:
  - In `merge_streams_cancellable`, if FFmpeg fails with exit code -22 containing `Only VP8 or VP9 or AV1 video and Vorbis or Opus audio and WebVTT subtitles are supported for WebM`, the error is mapped to `FreeFetchError::IncompatibleContainerCodec`.
- **Hardcode Subtitle Engine**:
  - WebM hardcoding encodes with `libvpx-vp9` rather than `libx264` and omits incompatible `movflags`.

---

## 7. Verification Matrix

| Test Suite | Tests | Result | Description |
| :--- | :---: | :---: | :--- |
| `tests/container_codec_tests.rs` | 11 | **PASS** | Validates normalization, matrix, YouTube real extraction, WebM VP9+Opus merge, WebM VP9+AAC rejection |
| `tests/frontend.test.ts` | 36 | **PASS** | Vitest verification of normalizers, compatibility matrix, and UI error handling |
| `tests/settings.test.ts` | 9 | **PASS** | Default container and audio settings invariants |
| `tests/audio_conversion_tests.rs` | 7 | **PASS** | Audio extraction and conversion invariants |
| `tests/subtitle_embedding_tests.rs` | 11 | **PASS** | Subtitle embedding compatibility |
| `tests/subtitle_hardcode_tests.rs` | 11 | **PASS** | Hardcoded subtitle rendering |
| `src/lib.rs` (Unit tests) | 36 | **PASS** | Core logic and normalizers |
| Full Cargo Test Suite | 163 | **PASS** | Zero failures across entire backend |
| Full Vitest Suite | 45 | **PASS** | Zero failures across entire frontend |
| `cargo clippy` | All | **PASS** | Zero warnings with `-D warnings` |
| `cargo fmt` | All | **PASS** | Formatted cleanly |
