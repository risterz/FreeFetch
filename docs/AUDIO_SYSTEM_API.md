# FreeFetch Audio System API Contract

> **Authoritative Stable Interface Specification**  
> Status: FROZEN / PRODUCTION READY  
> Target: Core Engine, Queue Manager, Services, and UI Consumers

---

## 1. Overview of Public vs Internal Interfaces

| Interface | Type | Visibility | Scope |
| :--- | :--- | :--- | :--- |
| `AudioTargetFormat` | Enum | Public | Supported output audio formats |
| `AudioQualityPreset` | Enum | Public | Target bitrate/quality presets |
| `BitrateMode` | Enum | Public | Encoding bitrate mode (CBR / VBR) |
| `AudioConversionOptions` | Struct | Public | User conversion configuration |
| `AudioMetadata` | Struct | Public | Normalized audio tags & metadata builder |
| `ArtworkDownloader` | Struct | Internal | Thumbnail retrieval & magic-byte validator |
| `FilenameTemplateEngine` | Struct | Public | Safe destination filename renderer |
| `FfmpegController::convert_audio_cancellable` | Method | Internal / Engine | Core FFmpeg transcode & stream mapper |
| `StartDownloadParams.audio_options` | Field | Public | Queue initiation contract |

---

## 2. Configuration & Models

### `AudioTargetFormat`
- **Purpose**: Defines target output audio container and default codec.
- **Variants**:
  - `Mp3` (default): Uses `libmp3lame`, extension `.mp3`.
  - `M4a`: Uses `aac`, extension `.m4a`.
  - `Flac`: Uses `flac`, extension `.flac`.
  - `Opus`: Uses `libopus`, extension `.opus`.
- **Key Methods**:
  - `extension(&self) -> &'static str`: Returns canonical file extension.
  - `default_codec(&self) -> &'static str`: Returns default FFmpeg codec string.
  - `supports_artwork(&self) -> bool`: Returns `true` for Mp3, M4a, Flac; `false` for Opus.
- **Invariants**: Extension returned is always lowercase without leading dot.

---

### `AudioQualityPreset`
- **Purpose**: Maps user quality preferences to concrete target bitrates.
- **Variants**: `Best`, `High`, `Medium`, `Low`, `Custom`.
- **Key Methods**:
  - `resolve_bitrate_kbps(&self, format: AudioTargetFormat, custom: Option<u32>) -> Option<u32>`
    - Returns `None` for `Flac` (lossless).
    - Resolves presets:
      - `Mp3`: Best 320, High 256, Medium 192, Low 128.
      - `M4a`: Best 256, High 192, Medium 160, Low 128.
      - `Opus`: Best 160, High 128, Medium 96, Low 64.
- **Invariants**: Must always produce values synchronized with UI labels in [`FormatModal.tsx`](file:///D:/Antigravity/FreeFetch/src/components/modal/FormatModal.tsx).

---

### `BitrateMode`
- **Purpose**: Selects Constant Bitrate (CBR) or Variable Bitrate (VBR) encoding.
- **Variants**: `Cbr` (default), `Vbr`.
- **Behavior**: Primarily applies to MP3 (`-b:a <k>` vs `-q:a <0..6>`).

---

### `AudioConversionOptions`
- **Purpose**: Encapsulates all options required to transform a raw audio stream.
- **Fields**:
  - `target_format: AudioTargetFormat`
  - `quality_preset: AudioQualityPreset`
  - `bitrate_mode: BitrateMode`
  - `custom_bitrate_kbps: Option<u32>`
  - `sample_rate: Option<u32>`
  - `embed_metadata: bool` (default: `true`)
  - `embed_artwork: bool` (default: `true`)
  - `filename_template: Option<String>`
- **Validation**:
  - `validate(&self) -> Result<(), String>`:
    - Fails if `target_format == Flac` and `custom_bitrate_kbps.is_some()`.
    - Fails if `sample_rate` outside `8000..=192000`.
    - Fails if `custom_bitrate_kbps` outside `32..=512`.
- **Invariants**: Must be validated before launching FFmpeg processes.

---

## 3. Metadata Normalization Interface

### `AudioMetadata`
- **Module**: [`src-tauri/src/media/audio_metadata.rs`](file:///D:/Antigravity/FreeFetch/src-tauri/src/media/audio_metadata.rs)
- **Purpose**: Provides clean, noise-free, and safe metadata for FFmpeg arguments.

#### Methods:
1. `parse_artist_and_title(raw_title: &str, fallback_artist: Option<&str>) -> (String, Option<String>)`
   - **Inputs**: Raw video title, optional channel/uploader name.
   - **Outputs**: `(title, artist)`.
   - **Behavior**: Strips promotional patterns, splits on separators (`" - "`, `" – "`, `" — "`), and trims quotes.
2. `clean_title_noise(raw: &str) -> String`
   - **Inputs**: Raw title.
   - **Outputs**: Cleaned title without `[Official Video]`, `(Lyrics)`, `4K`, `HD`, etc.
3. `sanitize_tag(value: &str) -> String`
   - **Inputs**: Arbitrary tag string.
   - **Outputs**: String with ASCII control characters (`\x00..\x1F`) removed and whitespace trimmed.
4. `from_media_metadata(meta: &MediaMetadata, playlist_title: Option<&str>, track_number: Option<u32>) -> Self`
   - **Inputs**: Extractor `MediaMetadata`, optional playlist title, optional 1-based track number.
   - **Outputs**: Fully populated `AudioMetadata`.
5. `to_ffmpeg_args(&self, format: AudioTargetFormat) -> Vec<String>`
   - **Inputs**: Target format.
   - **Outputs**: Safe argument list formatted for FFmpeg command vector (e.g. `["-id3v2_version", "3", "-metadata", "title=..."]`).

---

## 4. Cover Art Interface

### `ArtworkDownloader` *(Internal)*
- **Module**: [`src-tauri/src/media/artwork.rs`](file:///D:/Antigravity/FreeFetch/src-tauri/src/media/artwork.rs)
- **Purpose**: Downloads and validates cover images with complete fault isolation.

#### Methods:
1. `is_valid_image_bytes(bytes: &[u8]) -> bool`
   - Validates header magic bytes for JPEG (`FF D8 FF`), PNG (`89 50 4E 47`), and WebP (`RIFF...WEBP`).
2. `download_cover_art(client: &Client, thumbnail_url: &str, dest_dir: P, job_id: &str) -> Option<PathBuf>`
   - **Inputs**: HTTP client, image URL, output directory, unique job ID.
   - **Outputs**: `Some(path)` on success, `None` on any failure.
   - **Failure Policy**: Non-fatal. Logs error and returns `None`. Never propagates errors to parent audio job.
   - **Filesystem**: Creates temporary file `{job_id}.cover.tmp.jpg`.
3. `cleanup_cover_art<P: AsRef<Path>>(path: P)`
   - Deletes temporary artwork file if it exists.

---

## 5. Filename Template Interface

### `FilenameTemplateEngine`
- **Module**: [`src-tauri/src/engine/template.rs`](file:///D:/Antigravity/FreeFetch/src-tauri/src/engine/template.rs)
- **Purpose**: Renders safe destination stems from customizable patterns.

#### Struct `TemplateContext`:
```rust
pub struct TemplateContext {
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub date: Option<String>,
    pub id: Option<String>,
    pub uploader: Option<String>,
    pub ext: String,
}
```

#### Methods:
`render_stem(template: Option<&str>, ctx: &TemplateContext) -> String`
- **Default Pattern**: `{artist} - {title}` if artist present, otherwise `{title}`.
- **Safety Guarantees**:
  - Traversal tokens (`..`, `/`, `\`) replaced with `_`.
  - Windows illegal characters (`< > : " / \ | ? *`) stripped.
  - Windows reserved device names (`CON`, `PRN`, `AUX`, `NUL`, `COM1..9`, `LPT1..9`) prefixed with `_`.
  - Result clamped to 200 characters.

---

## 6. FFmpeg Conversion Interface

### `FfmpegController::convert_audio_cancellable` *(Internal)*
- **Module**: [`src-tauri/src/media/ffmpeg.rs`](file:///D:/Antigravity/FreeFetch/src-tauri/src/media/ffmpeg.rs)
- **Purpose**: Executes FFmpeg audio transcoding with live cancellation and process cleanup.

#### Signature:
```rust
pub async fn convert_audio_cancellable(
    &self,
    input_path: &Path,
    output_path: &Path,
    options: &AudioConversionOptions,
    metadata: Option<&AudioMetadata>,
    artwork_path: Option<&Path>,
    cancel_rx: Option<watch::Receiver<bool>>,
) -> Result<(), FreeFetchError>
```

#### Contract:
- **Inputs**:
  - `input_path`: Path to raw downloaded audio file (`.part`).
  - `output_path`: Final destination path.
  - `options`: Validated conversion options.
  - `metadata`: Optional normalized metadata tags.
  - `artwork_path`: Optional local cover art image file.
  - `cancel_rx`: Optional tokio watch channel for cancellation signals.
- **Errors**: Returns `FreeFetchError::RemuxFailed` on transcode failure, `FreeFetchError::Cancelled` if cancelled.
- **Cancellation**:
  - Kills child process immediately.
  - Awaits child exit to reap PID.
  - Automatically deletes `output_path` if partially written.
- **Security Invariant**: Command arguments passed as direct vector slices. Never executes via shell.

---

## 7. Queue Parameter Contract

### `StartDownloadParams`
- **Module**: [`src-tauri/src/engine/mod.rs`](file:///D:/Antigravity/FreeFetch/src-tauri/src/engine/mod.rs)
- **Audio Field**: `#[serde(default)] pub audio_options: Option<AudioConversionOptions>`
- **Queue Behavior**:
  - If `audio_options.is_none()`: Standard video/audio download.
  - If `audio_options.is_some()`:
    - Queue deduplication key is `{format_id}:{ext}`.
    - Post-download step executes audio conversion pipeline.
    - Transitions to `Processing` during FFmpeg conversion.
    - `maybe_embed_subtitles` immediately exits without embedding.
