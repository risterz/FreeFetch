# FreeFetch Audio System Architecture

> **Authoritative Technical Specification & Production Baseline**  
> Status: FROZEN / PRODUCTION READY  
> Verified Baseline: Backend 119/119 tests PASS, Frontend 16/16 tests PASS, Clippy zero warnings, Production build PASS.

---

## 1. Overview

The FreeFetch Audio System provides high-fidelity audio extraction, format transcoding, metadata injection, and cover art embedding from media URLs. It builds on FreeFetch's parallel range downloader, queue state machine, and FFmpeg execution controller without rewriting existing core architecture.

### End-to-End Pipeline

```text
Source URL
    ↓
Extractor ([`ExtractorRegistry`](file:///D:/Antigravity/FreeFetch/src-tauri/src/extractor/mod.rs) / [`YouTubeExtractor`](file:///D:/Antigravity/FreeFetch/src-tauri/src/extractor/implementations/youtube.rs))
    ↓
Raw Audio Stream Selection (best available audio-only stream, e.g., format 140 / 251)
    ↓
Queue Item Creation ([`QueueManager::add`](file:///D:/Antigravity/FreeFetch/src-tauri/src/engine/queue.rs))
    ↓  (format key: `{format_id}:{ext}`, safe template destination rendered)
Parallel Download Worker ([`download_stream_parallel`](file:///D:/Antigravity/FreeFetch/src-tauri/src/engine/parallel_downloader.rs))
    ↓  (resumable .part + .part.meta chunked ranges)
Raw Audio Downloaded (`.part` file assembled and validated)
    ↓
State Transition: `Downloading` → `Processing` ([`EngineManager::run_job`](file:///D:/Antigravity/FreeFetch/src-tauri/src/engine/mod.rs))
    ↓
Artwork Resolution ([`ArtworkDownloader::download_cover_art`](file:///D:/Antigravity/FreeFetch/src-tauri/src/media/artwork.rs))
    ↓  (magic-byte validation: JPEG/PNG/WebP, isolated failure handling)
Metadata Normalization ([`AudioMetadata::from_media_metadata`](file:///D:/Antigravity/FreeFetch/src-tauri/src/media/audio_metadata.rs))
    ↓  (noise stripped, quote trimmed, artist-title parsed, control chars cleaned)
FFmpeg Transcoding ([`FfmpegController::convert_audio_cancellable`](file:///D:/Antigravity/FreeFetch/src-tauri/src/media/ffmpeg.rs))
    ↓  (direct process vector, active cancellation, child reaping, stream mapping)
Cleanup ([`ArtworkDownloader::cleanup_cover_art`](file:///D:/Antigravity/FreeFetch/src-tauri/src/media/artwork.rs) + raw `.part` removal)
    ↓
State Transition: `Processing` → `Completed`
```

### Module Distribution
- **Models & Enums**: [`src-tauri/src/media/audio.rs`](file:///D:/Antigravity/FreeFetch/src-tauri/src/media/audio.rs), [`src-tauri/src/core/types.rs`](file:///D:/Antigravity/FreeFetch/src-tauri/src/core/types.rs)
- **Metadata Sanitization**: [`src-tauri/src/media/audio_metadata.rs`](file:///D:/Antigravity/FreeFetch/src-tauri/src/media/audio_metadata.rs)
- **Artwork Fetcher**: [`src-tauri/src/media/artwork.rs`](file:///D:/Antigravity/FreeFetch/src-tauri/src/media/artwork.rs)
- **Template Engine**: [`src-tauri/src/engine/template.rs`](file:///D:/Antigravity/FreeFetch/src-tauri/src/engine/template.rs)
- **FFmpeg Transcoder**: [`src-tauri/src/media/ffmpeg.rs`](file:///D:/Antigravity/FreeFetch/src-tauri/src/media/ffmpeg.rs)
- **Engine Pipeline & Lifecycle**: [`src-tauri/src/engine/mod.rs`](file:///D:/Antigravity/FreeFetch/src-tauri/src/engine/mod.rs)
- **Queue Integration**: [`src-tauri/src/engine/queue.rs`](file:///D:/Antigravity/FreeFetch/src-tauri/src/engine/queue.rs)
- **Frontend Controls**: [`src/components/modal/FormatModal.tsx`](file:///D:/Antigravity/FreeFetch/src/components/modal/FormatModal.tsx), [`src/components/modal/MultiUrlModal.tsx`](file:///D:/Antigravity/FreeFetch/src/components/modal/MultiUrlModal.tsx), [`src/components/modal/PlaylistModal.tsx`](file:///D:/Antigravity/FreeFetch/src/components/modal/PlaylistModal.tsx)

---

## 2. Supported Formats

### MP3 (`.mp3`)
- **Encoder**: `libmp3lame`.
- **Bitrate Modes**: Constant Bitrate (CBR, `-b:a <kbps>k`) and Variable Bitrate (VBR, `-q:a <0..6>`).
- **Quality Presets**:
  - `Best`: 320 kbps (CBR) / `-q:a 0` (VBR).
  - `High`: 256 kbps (CBR) / `-q:a 2` (VBR).
  - `Medium`: 192 kbps (CBR) / `-q:a 4` (VBR).
  - `Low`: 128 kbps (CBR) / `-q:a 6` (VBR).
  - `Custom`: 32–512 kbps (CBR) / `-q:a 2` (VBR).
- **Metadata**: Native ID3v2.3 tags (`-id3v2_version 3`, `-write_id3v1 1`) for Windows Explorer, automobile heads, and player compatibility.
- **Artwork**: Stream 1 mapped as `mjpeg` with `-disposition:v:0 attached_pic` and `-metadata:s:v comment="Cover (front)"`.

### M4A (`.m4a`)
- **Encoder**: `aac`.
- **Stream-Copy Path**: If raw input audio stream is already AAC in an MP4 container, remuxing preserves stream bytes; otherwise, encodes using `aac`.
- **Bitrate Presets**:
  - `Best`: 256 kbps.
  - `High`: 192 kbps.
  - `Medium`: 160 kbps.
  - `Low`: 128 kbps.
  - `Custom`: 32–512 kbps.
- **Container Options**: Always sets `-movflags +faststart` for web/streaming optimization.
- **Metadata**: Native iTunes metadata atoms (`©nam`, `©ART`, `©alb`, `©day`, `trkn`).
- **Artwork**: Stream 1 mapped as `-c:v:0 copy` with `-disposition:v:0 attached_pic`.

### FLAC (`.flac`)
- **Encoder**: `flac`.
- **Compression**: Level 5 (`-compression_level 5`).
- **Bitrate Behavior**: Bitrate is strictly fixed to lossless. Custom bitrate is rejected during parameter validation. All quality presets map to lossless compression.
- **Metadata**: Vorbis comments (standard `TITLE`, `ARTIST`, `ALBUM`, `DATE`, `TRACKNUMBER`).
- **Artwork**: Embedded picture block via `-disposition:v:0 attached_pic`.

### Opus (`.opus`)
- **Encoder**: `libopus`.
- **Container**: Ogg Opus (`.opus`).
- **Bitrate Presets**:
  - `Best`: 160 kbps.
  - `High`: 128 kbps.
  - `Medium`: 96 kbps.
  - `Low`: 64 kbps.
  - `Custom`: 32–512 kbps.
- **VBR Mode**: Always enables native Opus VBR (`-vbr on`).
- **Metadata**: Vorbis comments stored in stream header.
- **Artwork Limitation**: Ogg container specification does not support video stream attachments directly. Artwork mapping is intentionally omitted for Opus files; audio conversion succeeds without attempting attached picture video streams.

---

## 3. Audio Options API

The audio configuration API is exposed in Rust via [`src-tauri/src/media/audio.rs`](file:///D:/Antigravity/FreeFetch/src-tauri/src/media/audio.rs) and in TypeScript via [`src/api/types.ts`](file:///D:/Antigravity/FreeFetch/src/api/types.ts).

### Rust Models

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AudioTargetFormat {
    #[default]
    Mp3,
    M4a,
    Flac,
    Opus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AudioQualityPreset {
    #[default]
    Best,
    High,
    Medium,
    Low,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum BitrateMode {
    #[default]
    Cbr,
    Vbr,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioConversionOptions {
    #[serde(default)]
    pub target_format: AudioTargetFormat,
    #[serde(default)]
    pub quality_preset: AudioQualityPreset,
    #[serde(default)]
    pub bitrate_mode: BitrateMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_bitrate_kbps: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample_rate: Option<u32>,
    #[serde(default = "default_true")]
    pub embed_metadata: bool,
    #[serde(default = "default_true")]
    pub embed_artwork: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename_template: Option<String>,
}
```

### Validation Constraints
- **FLAC Bitrate Restriction**: If `target_format == AudioTargetFormat::Flac` and `custom_bitrate_kbps.is_some()`, validation returns `Err("Bitrate cannot be configured for lossless FLAC encoding")`.
- **Bitrate Range**: Custom bitrate must satisfy `32 <= kbps <= 512`.
- **Sample Rate Range**: Sample rate must satisfy `8000 <= rate <= 192000`.

### JSON Payload Example
```json
{
  "target_format": "mp3",
  "quality_preset": "custom",
  "bitrate_mode": "cbr",
  "custom_bitrate_kbps": 320,
  "sample_rate": 44100,
  "embed_metadata": true,
  "embed_artwork": true,
  "filename_template": "{artist} - {title}"
}
```

---

## 4. Metadata Pipeline

The metadata normalization subsystem is implemented in [`src-tauri/src/media/audio_metadata.rs`](file:///D:/Antigravity/FreeFetch/src-tauri/src/media/audio_metadata.rs).

### Transformation Flow
```text
Raw Title & Channel Name
    ↓
Promotional Noise Stripping
    ↓  (removes [Official Music Video], (Lyrics), 4K, HD, etc.)
Heuristic "Artist - Title" Splitting
    ↓  (detects " - ", " – ", " — "; trims quotes, apostrophes, backticks)
Control Character Sanitization
    ↓  (removes ASCII \x00..\x1F to prevent terminal & tag corruption)
Playlist Metadata Enrichment
    ↓  (maps playlist title to Album; maps 1-based index to Track Number)
Date Normalization
    ↓  (extracts 4-digit year prefix from YYYYMMDD)
FFmpeg Argument Assembly
    ↓  (formats -metadata key=value, -id3v2_version 3 for MP3)
```

### Preserved Fields
- `title`: Sanitized song title.
- `artist`: Detected artist or channel uploader.
- `album`: Playlist title (if batch/playlist job) or uploader name.
- `album_artist`: Uploader / channel name.
- `date`: 4-digit release year.
- `track_number`: 1-based sequence index.
- `comment`: Canonical attribution string (`Downloaded via FreeFetch: <url>`).

---

## 5. Artwork Pipeline

The cover art retrieval and embedding subsystem is implemented in [`src-tauri/src/media/artwork.rs`](file:///D:/Antigravity/FreeFetch/src-tauri/src/media/artwork.rs).

### Validation and Isolation Flow
1. **HTTP Retrieval**: Download thumbnail from YouTube or OpenGraph URL with 15-second timeout.
2. **Magic-Byte Header Validation**:
   - JPEG: `FF D8 FF`
   - PNG: `89 50 4E 47 0D 0A 1A 0A` or `89 50 4E 47`
   - WebP: `RIFF .... WEBP`
   - *Any payload failing magic-byte check (e.g. HTML 404, empty buffer, corrupt stream) is discarded immediately.*
3. **Temporary Storage**: Saved to unique path `{job_id}.cover.tmp.jpg` in destination directory.
4. **Fault Isolation**: Artwork download errors (network timeout, HTTP 404/500, corrupt bytes) log a diagnostic message and return `None`. Transcoding proceeds without artwork; artwork failure **never** fails an audio download.
5. **Deterministic Cleanup**: Temporary file is deleted in a `finally` block following FFmpeg execution. If cancelled, [`clean_cancelled_files_with_id`](file:///D:/Antigravity/FreeFetch/src-tauri/src/engine/mod.rs#L437) removes any leftover artwork file.

---

## 6. Filename Security & Template Engine

The template engine is implemented in [`src-tauri/src/engine/template.rs`](file:///D:/Antigravity/FreeFetch/src-tauri/src/engine/template.rs).

### Available Tokens
- `{title}`: Sanitized title.
- `{artist}`: Sanitized artist.
- `{album}`: Sanitized album / playlist title.
- `{date}`: Release year.
- `{id}`: Extractor media ID.
- `{uploader}`: Channel / uploader name.
- `{ext}`: Output extension (e.g., `mp3`).

### Security Guards
1. **Directory Traversal**: Replaces all occurrences of `..`, `/`, and `\` with `_`. No metadata token can escape the configured destination directory.
2. **Windows Reserved Device Names**: If the rendered filename matches any reserved Windows device name (`CON`, `PRN`, `AUX`, `NUL`, `COM1`–`COM9`, `LPT1`–`LPT9`, case-insensitively), it is automatically prefixed with an underscore (e.g., `_NUL.mp3`).
3. **Illegal Characters**: Strips `< > : " / \ | ? *` and ASCII control characters (`0x00..0x1F`).
4. **Length Clamping**: Resulting stem is clamped to 200 characters to prevent Windows `MAX_PATH` filesystem overflow.
5. **Collision Protection**: If target file already exists, [`ensure_safe_destination_path`](file:///D:/Antigravity/FreeFetch/src-tauri/src/engine/state.rs) appends sequential counter ` (1)`, ` (2)`.

---

## 7. Queue Lifecycle & Concurrency

The queue system is implemented in [`src-tauri/src/engine/queue.rs`](file:///D:/Antigravity/FreeFetch/src-tauri/src/engine/queue.rs).

### State Transitions
```text
Queued → Preparing → Downloading → Processing → Completed
   ↓         ↓            ↓            ↓
Paused    Paused        Paused       Failed
   ↓         ↓            ↓            ↓
Cancelled Cancelled   Cancelled    Cancelled
```

### Format Deduplication Isolation
- Standard video items use `format_id` as the deduplication key.
- Audio items format their key as `{format_id}:{ext}` (e.g. `140:mp3`, `140:flac`).
- **Result**: A user can simultaneously queue the 1080p MP4 video and the MP3 audio for the same YouTube URL without deduplication collisions. Re-queueing the identical audio target format is rejected as duplicate.

### Slot Ownership
- The active concurrency slot is acquired in `Preparing` and held through `Downloading` and `Processing`.
- During FFmpeg conversion (`Processing`), the slot remains occupied to prevent spawning excessive parallel CPU-heavy transcodes.
- On completion, failure, or cancellation, the active job slot is immediately released and `process_next` advances the queue.

---

## 8. Cancellation Semantics

Cancellation handling is implemented in [`src-tauri/src/media/ffmpeg.rs`](file:///D:/Antigravity/FreeFetch/src-tauri/src/media/ffmpeg.rs) and [`src-tauri/src/engine/mod.rs`](file:///D:/Antigravity/FreeFetch/src-tauri/src/engine/mod.rs).

### Process Termination Contract
1. **Subprocess Management**: Spawned FFmpeg instances set `.kill_on_drop(true)`.
2. **Watch Channel Signaling**: Cancellation triggers `cancel_rx.changed()`.
3. **Process Killing & Reaping**:
   - Invokes `child.kill().await` to send termination signal.
   - Invokes `child.wait().await` to reap the child process exit status and avoid Windows zombie/orphan processes.
4. **Filesystem Cleanup**:
   - Partial output file is immediately removed.
   - Raw `.part` and `.part.meta` files are deleted.
   - Temporary artwork file `{job_id}.cover.tmp.jpg` is deleted.
   - Windows retry loop (5 attempts with 50ms backoff) overcomes transient filesystem locks.

---

## 9. Subtitle Isolation Invariant

> **CRITICAL INVARIANT**: Audio downloads must NEVER attempt subtitle embedding.

In [`src-tauri/src/engine/mod.rs:maybe_embed_subtitles`](file:///D:/Antigravity/FreeFetch/src-tauri/src/engine/mod.rs#L1266):
```rust
if !params.embed_subtitles || params.selected_subtitles.is_empty() || params.audio_options.is_some() {
    return Ok(());
}
```
If `params.audio_options.is_some()`, the subtitle embedder exits immediately. This prevents FFmpeg from attempting video stream mapping (`-map 0:v`) on pure audio files, preserving complete independence between audio conversions and subtitle tracks.

---

## 10. FFmpeg Security Model

1. **Direct Process Vectors**: All FFmpeg executions use [`std::process::Command`](file:///D:/Antigravity/FreeFetch/src-tauri/src/media/ffmpeg.rs#L300) with discrete arguments:
   ```rust
   let mut cmd = Command::new(&self.ffmpeg_path);
   cmd.arg("-y").arg("-i").arg(input_path);
   ```
2. **Zero Shell Execution**: No shell interpreter (`cmd.exe`, `powershell.exe`, `/bin/sh`) is ever invoked.
3. **Zero String Concatenation**: Command lines are never constructed via string formatting or interpolation.
4. **Command Injection Immunity**: Untrusted user or website input (titles, artist names, URLs, template tokens) cannot escape argument boundaries.

---

## 11. Testing Contract & Verification Matrix

### Verification Baseline
- **Backend Test Suites**: 119/119 PASS.
- **Frontend Test Suites**: 16/16 PASS.
- **Clippy Static Analysis**: Zero warnings (`-D warnings` enforced across all targets and features).
- **Format Check**: Clean (`cargo fmt --check`).
- **Production Build**: Clean (`tsc && vite build`).

### Validated Test Suites
1. **FFprobe Real Transcode Inspection**: Verified valid audio codecs, stream mappings, attached picture dispositions, and metadata tags on MP3, M4A, FLAC, and Opus.
2. **Quality Preset Mapping**: Verified resolution of CBR/VBR bitrates across all formats and presets.
3. **Sample Rate Constraints**: Verified 48 kHz transcode enforcement and rejection of `< 8000 Hz` or `> 192000 Hz`.
4. **Adversarial Metadata**: Quotes, apostrophes, Japanese Unicode, emojis, promotional noise patterns, and control characters.
5. **Filename Traversal & Reserved Names**: `..`, `/`, `\`, illegal characters, and Windows device names (`CON`, `PRN`, `AUX`, `NUL`, etc.).
6. **Artwork Fault-Isolation**: Magic-byte validation, corrupt payloads, and 404 resilience.
7. **Cancellation & Child Termination**: Verified child PID reaping and artifact cleanup.
8. **Mixed Queue & Duplicate Isolation**: Concurrent MP4, MP3, M4A, FLAC, and Opus items on identical URLs.
9. **Playlist Track Mapping**: Album title inheritance and track indexing.
