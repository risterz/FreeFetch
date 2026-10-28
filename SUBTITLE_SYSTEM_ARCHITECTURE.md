# FreeFetch Subtitle System Architecture

## Overview
FreeFetch provides comprehensive subtitle processing for video downloads, supporting standalone extraction, lossless soft embedding, and burned-in (hardcoded) subtitle rendering.

```text
               YouTube / Video Extraction
                           ↓
               Selected Subtitle Tracks
                           ↓
               Subtitle Mode Resolution
                           ↓
    ┌──────────────────────┼──────────────────────┐
    ↓                      ↓                      ↓
 [None]                 [Embed]               [Hardcode]
 Standalone .srt/.vtt   Lossless remux        Video re-encode (libx264)
 download into queue    -c:v copy -c:a copy   -vf subtitles='...'
 or folder              -c:s mov_text         -c:v libx264 -c:a copy
                        Selectable track      Burned into pixels (0 sub streams)
```

## Subtitle Modes

| Mode | Identifier | Video Re-encode | Codec / Filter | Output Stream Characteristics |
|------|------------|-----------------|----------------|-------------------------------|
| **None** | `none` | No | None | Separate `.srt` / `.vtt` file added to queue or saved standalone |
| **Embed** | `embed` | No (Lossless) | `-c:s mov_text` | MP4 with Video + Audio + 1..N `mov_text` subtitle tracks |
| **Hardcode** | `hardcode` | Yes | `-vf subtitles='<path>'` `-c:v libx264` | MP4 with Video + Audio, **0** subtitle streams (burned into frames) |

## Filtergraph Path Escaping
FFmpeg filter descriptions undergo multi-layer parsing (filtergraph tokenizer followed by option parsing). Windows paths with special characters are safely escaped via:
```rust
pub fn escape_subtitle_filter_path(path: &Path) -> String {
    let p_str = path.to_string_lossy();
    let normalized = p_str.replace('\\', "/");
    let escaped_colons = normalized.replace(':', r"\:");
    let escaped_quotes = escaped_colons.replace('\'', r"'\\\''");
    format!("subtitles='{escaped_quotes}'")
}
```
This guarantees immunity against spaces, colons, parentheses, apostrophes, brackets, Unicode, ampersands, and hashes.

## Hardcode Encoding Baseline
- **Video Codec**: `libx264`
- **Rate Control**: `-crf 20` (visually lossless default)
- **Preset**: `-preset medium`
- **Audio Stream**: `-c:a copy` (lossless audio stream preservation)
- **Container Optimizations**: `-movflags +faststart`

## Backward Compatibility
- Payloads supplying `embed_subtitles: true` without `subtitle_mode` automatically resolve to `SubtitleMode::Embed`.
- Payloads supplying `embed_subtitles: false` without `subtitle_mode` automatically resolve to `SubtitleMode::None`.
- New clients supply explicit `subtitle_mode: "none" | "embed" | "hardcode"`.

## Subtitle Discovery and Filtering

### Authoritative Discovery Rule
> FreeFetch exposes manually supplied subtitle tracks and the video's original auto-generated subtitle track. Translated/derived auto-generated subtitle tracks are filtered from the selectable subtitle list because they are not reliably supported by the current extraction pipeline.

### YouTube Original-Only Caption Strategy
1. **Root Cause for Filtering Machine-Generated Translations**:
   YouTube provides dozens to hundreds of auto-translated caption tracks (e.g., `iu`, `ga`, `fr`, `es`) derived dynamically from the source transcript. The timedtext endpoints for these tracks append `&tlang=<target_lang>`, which consistently trigger HTTP 429 (Too Many Requests) or empty responses when accessed directly by automated media pipelines.
2. **Track Identification**:
   - **Manual Captions**: Placed under yt-dlp `subtitles`. Marked `is_auto_generated: false`. 100% preserved.
   - **Original Auto-Generated Captions**: Placed under yt-dlp `automatic_captions` with key `<lang>-orig` (e.g., `id-orig`, `en-orig`) and direct timedtext endpoint (`kind=asr&lang=<lang>`) without `tlang=`.
   - **Derived / Translated Auto-Generated Captions**: Placed under yt-dlp `automatic_captions` with bare duplicate keys or target language keys (e.g., `iu`, `ga`, `fr`) bearing `&tlang=`. These are discarded during backend normalization.
3. **Display Label & Code Invariants**:
   - Language code set to `<lang>-orig` (e.g. `id-orig`).
   - Display name formatted as `"{Language Name} (Original) (Auto-generated)"`.
4. **URL Refresh Compatibility**:
   In `engine::mod`, subtitle track lookup during download execution matches exact language code or prefix (`s.language_code.starts_with(lang) || lang.starts_with(&s.language_code)`), preventing breakages if requested as `id` or `id-orig`.
