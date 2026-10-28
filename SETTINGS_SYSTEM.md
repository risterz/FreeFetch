# FreeFetch Settings & Preferences Subsystem

## 1. Overview & Architecture

The **Settings & Preferences** subsystem provides a centralized, persistent, validated configuration engine for the FreeFetch application.

It adheres strictly to the **Release Architecture Freeze** contract (`RELEASE_ARCHITECTURE_FREEZE.md`):
- Core downloader algorithms, queue state machine transitions, audio extraction, subtitle pipelines, and FFmpeg process invocations remain **FROZEN**.
- Settings act as **defaults** and operational boundaries; they **never override explicit user choices** made in format selection dialogs.
- Configuration persistence utilizes the existing SQLite `settings` table without altering `downloads`, `download_streams`, or `queue_items`.

```text
               +-------------------------------------------+
               |        React 18 SettingsView UI           |
               | (General, Downloads, Audio, Subs, Theme)  |
               +-------------------------------------------+
                                     |
                                     | Tauri IPC: get_settings / update_settings
                                     v
               +-------------------------------------------+
               |         Tauri IPC Command Layer           |
               |   (src-tauri/src/ipc/commands.rs)         |
               +-------------------------------------------+
                                     |
                                     v
               +-------------------------------------------+
               |       AppSettings Domain Entity           |
               |    - sanitization & clamping              |
               |    - backward-compatible deserialization  |
               +-------------------------------------------+
                                     |
                  +------------------+------------------+
                  |                                     |
                  v                                     v
   +------------------------------+     +------------------------------+
   |   In-Memory AppState Cache   |     |    SQLite `settings` Table   |
   | (Thread-safe Arc<Mutex<...>>) |     | (Key: 'app_settings', WAL)   |
   +------------------------------+     +------------------------------+
                  |
         +--------+--------+
         |                 |
         v                 v
   +------------+   +------------+
   |QueueManager|   |RateLimiter |
   |(Concurrency|   |(Token      |
   | Clamping)  |   | Bucket)    |
   +------------+   +------------+
```

---

## 2. Configuration Keys & Defaults

| Setting Key | Type | Default Value | Category | Description |
| :--- | :--- | :--- | :--- | :--- |
| `download_directory` | `String` | System Downloads / FreeFetch | General | Default folder destination for new downloads |
| `max_concurrent_downloads` | `usize` | `3` | Downloads | Maximum concurrent downloads in active queue (1–5) |
| `speed_limit_bytes_per_sec` | `Option<u64>` | `None` (Unlimited) | Downloads | Per-stream download rate throttle in bytes/second |
| `default_container` | `String` | `"mp4"` | Downloads | Pre-selected video container (`mp4`, `mkv`, `webm`) |
| `default_audio_format` | `String` | `"mp3"` | Audio | Pre-selected audio extraction format (`mp3`, `m4a`, `flac`, `opus`) |
| `default_audio_quality` | `String` | `"high"` | Audio | Pre-selected audio quality preset (`low`, `medium`, `high`, `best`) |
| `embed_artwork` | `bool` | `true` | Audio | Embed cover thumbnail into MP3, M4A, FLAC (bypasses Opus) |
| `default_subtitle_mode` | `String` | `"none"` | Subtitles | Pre-selected subtitle mode (`none`, `embed`, `hardcode`) |
| `theme` | `String` | `"system"` | Appearance | UI visual theme (`system`, `dark`, `light`) |
| `notifications_enabled` | `bool` | `true` | Notifications | Display OS notification on download completion |
| `auto_clear_completed` | `bool` | `false` | Queue | Automatically clear finished downloads from active queue display |
| `ffmpeg_path` | `Option<String>` | `None` (Bundled Sidecar) | Advanced | Path to custom FFmpeg executable binary |

---

## 3. Persistence & Backward Compatibility

1. **Storage Mechanism**:
   - Persisted in the SQLite database `%APPDATA%\FreeFetch\freefetch.db` within the `settings` key-value table:
     ```sql
     INSERT INTO settings (key, value) VALUES ('app_settings', ?1)
     ON CONFLICT(key) DO UPDATE SET value = ?1;
     ```
2. **Schema Compatibility**:
   - `AppSettings` derives `#[serde(default)]`.
   - Legacy installations containing only the initial 5 fields deserialize cleanly without migration scripts.
   - Missing fields are automatically populated with safe defaults.
3. **In-Memory Cache**:
   - Synchronized atomically in `AppState.settings` (`Arc<Mutex<AppSettings>>`).
   - Reads occur in-memory without disk overhead (`get_settings`).

---

## 4. Validation & Sanitization Rules

The backend sanitizes all incoming configuration in `AppSettings::sanitize()` before persistence:

- **Concurrency**: Clamped strictly to `1..=5`.
- **Speed Limit**: Values `<= 0` are coerced to `None` (unlimited, zero-overhead).
- **Download Directory**: If empty or whitespace, defaults to `dirs_fallback()`. Directory is verified and created recursively (`fs::create_dir_all`) upon save.
- **Media Formats**:
  - `default_container` must be one of `mp4`, `mkv`, `webm`. Defaults to `mp4`.
  - `default_audio_format` must be one of `mp3`, `m4a`, `flac`, `opus`. Defaults to `mp3`.
  - `default_audio_quality` must be one of `low`, `medium`, `high`, `best`. Defaults to `high`.
  - `default_subtitle_mode` must be one of `none`, `embed`, `hardcode`. Defaults to `none`.
  - `theme` must be one of `system`, `dark`, `light`. Defaults to `system`.
- **Custom FFmpeg Path**:
  - Validates that the file exists and is an executable file.
  - Executes `FfmpegController::discover(Some(path))` to verify binary functionality.
  - Invalid binaries reject the update with `FreeFetchError::Generic`, preserving previous valid configurations.

---

## 5. Explicit User Override Guarantees

In accordance with architectural principles, **Settings represent initial defaults only**:

1. **Format Selection Override**:
   - When the user opens `FormatModal`, initial tabs, audio formats, bitrates, containers, and subtitle modes populate from `AppSettings`.
   - Any manual user interaction inside `FormatModal` (e.g. clicking FLAC, Best, Hardcode, or MKV) immediately overrides the default.
2. **Path Override**:
   - Any download parameter providing an explicit `destination_dir` uses that destination regardless of default configuration.
   - Defaults only supply the destination when `params.destination_dir` is `None`.
3. **Queue Cleanup vs History Isolation**:
   - `auto_clear_completed` clears completed items from the active queue display (`queue_clear_completed`).
   - Historical download records in `downloads` are **never deleted** by queue cleanup.

---

## 6. Reset to Defaults

The "Reset to Defaults" action:
- Overwrites `app_settings` in SQLite with `AppSettings::default()`.
- Updates active in-memory cache and queue concurrency.
- Immediately reflects default options across the UI.
- Strictly preserves:
  - Download history in `downloads`
  - Active and queued items in `queue_items`
  - Downloaded media files on the filesystem

---

## 7. Verification Evidence

- Backend Test Suite: [`src-tauri/tests/settings_tests.rs`](file:///D:/Antigravity/FreeFetch/src-tauri/tests/settings_tests.rs) (6/6 PASS).
- Frontend Test Suite: [`tests/settings.test.ts`](file:///D:/Antigravity/FreeFetch/tests/settings.test.ts) (9/9 PASS).
- Toolchain Gates:
  - `cargo fmt -- --check`: PASS
  - `cargo check`: PASS
  - `cargo clippy -D warnings`: PASS
  - `cargo test`: 161 PASS (0 failures)
  - `npm test`: 35 PASS (0 failures)
  - `npm run build`: PASS (Vite & TypeScript clean build)
