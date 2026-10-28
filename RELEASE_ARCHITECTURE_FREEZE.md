# FreeFetch Release Architecture Freeze

## 1. Status

```text
STATUS: FROZEN
STAGE: Production Baseline — Pre-Settings & UI Phase
```

### Production QA Status

- **Backend Rust Tests**: **155 / 155 PASS** (0 failures, 0 ignored)
- **Frontend Vitest Tests**: **26 / 26 PASS** (0 failures)
- **Rust Toolchain**: `cargo fmt -- --check` PASS, `cargo check` PASS, `cargo clippy -D warnings` PASS
- **Frontend Build**: `tsc && vite build` PASS (0 TypeScript errors, 1957 modules transformed, production assets compiled in 4.47s)
- **Defect Inventory**:
  - P0 (Critical): **0**
  - P1 (High): **0**
  - P2 (Medium): **0**
  - P3 (Low / Informational): **1** (Deferred to v0.2 milestone)

---

## 2. Frozen Subsystems

The following subsystems are production-verified and strictly **FROZEN**. No modifications to their algorithms, data structures, invariants, or execution flows are permitted during upcoming UI and Settings development:

### 1. Downloader Subsystem
- **Parallel Chunk Downloader**: Range chunking calculations, 2 to 8 worker allocation based on payload size, positional file writer, and buffer reuse.
- **Resilience Engine**: Exponential backoff, jitter, Retry-After header parsing, HTTP 429 rate-limiting backoff, HTTP 403 / 410 CDN URL token refresh without resetting progress.
- **Artifact Isolation**: `.part` data files and `.part.meta` worker state descriptors.
- **Resumption Semantics**: Preservation of `.part` and `.part.meta` upon user pause or application termination.
- **Progress Reporting**: Decoupled atomic byte counters and thread-safe progress receiver channel.

### 2. Queue Engine
- **State Machine**: Validated transitions across `Pending`, `Downloading`, `Processing`, `Paused`, `Completed`, `Failed`, and `Cancelled`.
- **Slot Isolation**: Thread-safe concurrency limits (`max_concurrent_downloads`), queue permit acquisition, and instant release upon job completion or termination.
- **Persistence**: SQLite-backed `queue_items` table with dynamic reordering, position shifting, and crash recovery.
- **Destination Collision Prevention**: In-memory path reservation preventing queue items from claiming colliding final destinations or colliding temporary `.part` files.

### 3. Audio Subsystem
- **Frozen Contracts**:
  - `docs/AUDIO_SYSTEM_ARCHITECTURE.md`
  - `docs/AUDIO_SYSTEM_API.md`
  - `docs/AUDIO_REGRESSION_INVARIANTS.md`
- **Invariants A1–A12**:
  - **A1**: Valid audio streams and codecs for MP3 (`libmp3lame`), M4A (`aac`), FLAC (`flac`), Opus (`libopus`).
  - **A2**: Audio extraction never requires or pulls video streams.
  - **A3**: Subtitle isolation — audio downloads strictly bypass and ignore subtitle processing.
  - **A4**: Cancellation terminates FFmpeg child processes and reaps PIDs.
  - **A5**: Cancellation deletes partial audio outputs, artwork temps, and working files.
  - **A6**: Filename template traversal protection and Windows reserved device name safeguards.
  - **A7**: Direct vector process invocation without shell string interpolation.
  - **A8**: Queue slot concurrency isolation.
  - **A9**: FLAC lossless preservation (sample rate, bit depth, rejection of invalid lossy bitrates).
  - **A10**: Opus artwork limitation handled gracefully without failing transcoding.
  - **A11**: Metadata heuristic tag sanitization and promotional noise removal.
  - **A12**: Zero regression to video remuxing, subtitle extraction, or SQLite storage.

### 4. Subtitle Subsystem
- **Frozen Contract**: `docs/SUBTITLE_SYSTEM_ARCHITECTURE.md`
- **Subtitle Modes**:
  - `SubtitleMode::None`: No subtitle remuxing or encoding.
  - `SubtitleMode::Embed`: Lossless video/audio remux (`-c copy`) with `mov_text` subtitle track added into MP4 (or SRT into MKV). Selectable in media players.
  - `SubtitleMode::Hardcode`: Video re-encoded with `-vf subtitles='<escaped-path>' -c:v libx264 -crf 20 -preset medium -c:a copy`. Subtitles permanently rendered into frame pixels.
- **Invariants S1–S3**:
  - **S1**: Tri-state processing exclusivity (`None`, `Embed`, `Hardcode`).
  - **S2**: Hardcode track exclusivity — strictly 1 subtitle track allowed for hardcoding.
  - **S3**: Filtergraph path escaping — platform-safe escaping for Windows drive colons, slashes, and quotes.

### 5. FFmpeg Process Management
- Direct executable execution via `tokio::process::Command::new(&self.ffmpeg_path)`.
- Zero shell interpolation (no `cmd.exe`, `powershell.exe`, or string splitting).
- Asynchronous PID reaping and child process killing on cancellation tokens.
- Stderr streaming and buffered diagnostic capture.
- Atomic file replacement via temporary `.tmp` extensions.

### 6. History and Analytics
- SQLite WAL journal mode and foreign key constraints.
- Reverse chronological pagination (`created_at DESC`).
- Multi-dimensional filtering by status (`completed`, `failed`, `cancelled`) and media type (`video`, `audio`).
- Aggregated metrics (download counts, total bytes, average speed).
- Export compliance: RFC-4180 CSV export (CRLF line endings, double-quote escaping) and JSON serialization.
- Retry workflow re-injecting historical downloads into the live queue.

---

## 3. System Architecture Diagram

```text
               +-------------------------------------------+
               |        React 18 + TypeScript UI           |
               | (Header, Queue, FormatModal, HistoryView) |
               +-------------------------------------------+
                                     |
                                     | Tauri IPC (invoke / listen)
                                     v
               +-------------------------------------------+
               |       Tauri Command & AppState Layer      |
               |  (src-tauri/src/ipc/commands.rs, mod.rs)  |
               +-------------------------------------------+
                                     |
                  +------------------+------------------+
                  |                                     |
                  v                                     v
   +------------------------------+     +------------------------------+
   |        QueueManager          |     |    Batch & Playlist Engine   |
   | (State Machine, Concurrency) |<----+  (Deduplication, Resolution) |
   +------------------------------+     +------------------------------+
                  |
                  v
   +------------------------------+
   |        EngineManager         |
   | (Job Lifecycle, Cancel Token)|
   +------------------------------+
                  |
         +--------+--------+
         |                 |
         v                 v
   +------------+   +------------------------------------------------+
   | Extractor  |   |              Downloader Engine                 |
   |  Registry  |   | - Parallel Downloader (Workers, Positional IO) |
   | (YouTube,  |   | - Chunk Downloader (Single stream fallback)    |
   |  Generic)  |   | - Rate Limiter (Token Bucket)                  |
   +------------+   +------------------------------------------------+
                           |
                           v
   +-----------------------------------------------------------------+
   |                    Media Processing Layer                       |
   | - Audio Transcoder (MP3, M4A, FLAC, Opus via FfmpegController)  |
   | - Subtitle Engine (VTT -> SRT parser, mov_text embed, hardcode) |
   | - Artwork Downloader (Magic-byte validation, thumbnail fetch)   |
   +-----------------------------------------------------------------+
                           |
                           v
   +-----------------------------------------------------------------+
   |                  FFmpeg Subprocess Execution                    |
   | (tokio::process::Command, Direct Args, PID Reaping, No Shell)   |
   +-----------------------------------------------------------------+
                           |
                  +--------+--------+
                  |                 |
                  v                 v
   +-------------------------+   +-----------------------------------+
   |   Filesystem Output     |   |          SQLite Storage           |
   | (Sanitized Paths, Part  |   | - WAL mode, Foreign keys          |
   |  Isolation, Safe Moves) |   | - downloads, queue_items, streams |
   +-------------------------+   +-----------------------------------+
```

---

## 4. Frontend Architecture

### Entry Point & Component Hierarchy
- `src/main.tsx`: React DOM bootstrap.
- `src/App.tsx`: Central coordinator managing active views, global event subscriptions, URL input handling, and modal triggers.
- **Core Components**:
  - `src/components/layout/Header.tsx`: URL input form, clipboard paste listener, FFmpeg sidecar readiness indicator, Batch, Playlist, and History view toggles.
  - `src/components/queue/DownloadCard.tsx`: Individual job card rendering thumbnail, metadata, live progress bar, speed, ETA, and actions (Pause, Resume, Cancel, Delete, Open File, Open Folder).
  - `src/components/queue/FilterTabs.tsx`: Status filtering (`all`, `active`, `completed`, `failed`), queue batch controls (Clear, Pause All, Resume All), and concurrency dropdown.
  - `src/components/modal/FormatModal.tsx`: Format selector (Video vs Audio), Audio conversion options, Subtitle selector, and Subtitle Mode selector (None, Embed, Hardcode).
  - `src/components/modal/BatchModal.tsx`: Multi-URL batch ingestion interface.
  - `src/components/modal/PlaylistModal.tsx`: YouTube playlist item selection and quality selection interface.
  - `src/components/history/HistoryView.tsx`: Full-page history inspection table, search, filters, pagination, statistics cards, and CSV/JSON export actions.
  - `src/components/common/ProgressBar.tsx`: Reusable progress bar component.

### State Management & IPC Bridge
- **Local State**: Managed with React `useState`, `useCallback`, and `useEffect`.
- **IPC Interface**: Encapsulated strictly in `src/api/ipc.ts` using `@tauri-apps/api/core` (`invoke`) and `@tauri-apps/api/event` (`listen`).
- **Data Types**: Defined in `src/api/types.ts` mirroring backend Rust serde structures.

### Modification Safety Boundary for UI/UX
- **Safe to Modify**:
  - CSS, styling, Tailwind classes, typography, spacing, color themes.
  - Layout hierarchies, card layouts, table presentation.
  - Visual animations, transition effects, icons (lucide-react).
  - Pure presentational modals and toast/alert popups.
- **Unsafe / Requires Freeze Review**:
  - Changing payload structures in `src/api/types.ts`.
  - Altering invoke command names or parameters in `src/api/ipc.ts`.
  - Altering event listener payloads or unsubscribing hooks in `src/App.tsx`.
  - Bypassing format validation logic before sending download parameters to Rust.

---

## 5. Backend Architecture

### 1. IPC Command Layer (`src-tauri/src/ipc/`)
- **Responsibility**: Translates Tauri frontend IPC calls into internal service invocations.
- **State Container**: `AppState` containing thread-safe shared references:
  - `repo`: `Arc<dyn Repository + Send + Sync>`
  - `engine`: `Arc<EngineManager>`
  - `queue`: `Arc<QueueManager>`
  - `settings`: `Arc<Mutex<AppSettings>>`
  - `extractors`: `Arc<ExtractorRegistry>`

### 2. Queue Manager (`src-tauri/src/engine/queue.rs`)
- **Responsibility**: Coordinates queue prioritization, concurrency clamping, job dispatch, and state synchronization between SQLite and active runtime jobs.
- **Interface**: `add`, `remove`, `start`, `pause_item`, `resume_item`, `cancel_item`, `retry_item`, `reorder`, `set_concurrency`.
- **Failure Behavior**: On worker error, marks queue item `Failed` and advances the queue to start the next pending job.
- **Cancellation**: Triggers job cancellation token, reaps concurrency permit, marks status `Cancelled`.

### 3. Downloader Engine (`src-tauri/src/engine/parallel_downloader.rs`)
- **Responsibility**: High-throughput HTTP stream downloading via positional writing.
- **Workers**: Spawns 2 to 8 parallel tasks requesting HTTP byte ranges (`Range: bytes=start-end`).
- **Metadata**: Continuously synchronizes byte progress to `.part.meta`.
- **Resilience**: Automatically recovers from transient disconnections, rate limits, and expired tokens.

### 4. Media Processing Layer (`src-tauri/src/media/`)
- **`ffmpeg.rs`**: Core wrapper executing FFmpeg commands with direct vector arguments and cancellation watchers.
- **`audio.rs` & `audio_metadata.rs`**: Transcoding engine supporting MP3, M4A, FLAC, and Opus with tag writing and promotional noise stripping.
- **`artwork.rs`**: Downloads and validates JPEG/PNG magic bytes before passing cover art to FFmpeg.
- **`subtitle.rs`, `subtitle_parser.rs`, `subtitle_converter.rs`**: Fetches VTT captions, normalizes styling tags, parses timestamps, converts to SubRip (SRT), and injects into remux/transcode pipelines.

### 5. Storage Layer (`src-tauri/src/storage/`)
- **`repository.rs`**: Thread-safe SQLite repository wrapping `rusqlite::Connection` in `Arc<Mutex<Connection>>`.
- **`migrations.rs`**: Schema migrations, WAL configuration, indexes, and crash recovery query.

---

## 6. IPC Contract Freeze

The following 39 Tauri commands and 7 event channels represent the frozen IPC surface:

### IPC Commands

| Command | Direction | Parameters | Return Type | Status |
| :--- | :--- | :--- | :--- | :--- |
| `parse_url` | Frontend → Rust | `{ url: String }` | `ParseResult` | **FROZEN** |
| `start_download` | Frontend → Rust | `{ params: StartDownloadParams }` | `String` (ID) | **FROZEN** |
| `pause_download` | Frontend → Rust | `{ id: String }` | `()` | **FROZEN** |
| `resume_download` | Frontend → Rust | `{ id: String }` | `()` | **FROZEN** |
| `cancel_download` | Frontend → Rust | `{ id: String }` | `()` | **FROZEN** |
| `delete_download` | Frontend → Rust | `{ id: String, deleteFiles: bool }` | `()` | **FROZEN** |
| `get_downloads` | Frontend → Rust | None | `Vec<DownloadRecord>` | **FROZEN** |
| `get_download_history` | Frontend → Rust | `{ filter: HistoryFilter }` | `HistoryPage` | **FROZEN** |
| `get_download_history_item` | Frontend → Rust | `{ id: String }` | `Option<DownloadRecord>` | **FROZEN** |
| `delete_download_history_item`| Frontend → Rust | `{ id: String, deleteFile: Option<bool> }` | `()` | **FROZEN** |
| `clear_download_history` | Frontend → Rust | `{ statusFilter: Option<String> }` | `usize` (Deleted count) | **FROZEN** |
| `get_download_statistics` | Frontend → Rust | None | `DownloadStatistics` | **FROZEN** |
| `export_download_history` | Frontend → Rust | `{ format: String }` | `String` (JSON or CSV) | **FROZEN** |
| `retry_history_download` | Frontend → Rust | `{ id: String }` | `String` (Queue ID) | **FROZEN** |
| `get_settings` | Frontend → Rust | None | `AppSettings` | **FROZEN** |
| `update_settings` | Frontend → Rust | `{ settings: AppSettings }` | `()` | **FROZEN** |
| `check_ffmpeg_status` | Frontend → Rust | None | `String` (Binary Path) | **FROZEN** |
| `open_downloaded_file` | Frontend → Rust | `{ path: String }` | `()` | **FROZEN** |
| `open_containing_folder` | Frontend → Rust | `{ path: String }` | `()` | **FROZEN** |
| `queue_add` | Frontend → Rust | `{ params: StartDownloadParams }` | `QueueItem` | **FROZEN** |
| `queue_remove` | Frontend → Rust | `{ id: String }` | `()` | **FROZEN** |
| `queue_start` | Frontend → Rust | None | `()` | **FROZEN** |
| `queue_pause` | Frontend → Rust | `{ id: Option<String> }` | `()` | **FROZEN** |
| `queue_resume` | Frontend → Rust | `{ id: Option<String> }` | `()` | **FROZEN** |
| `queue_cancel` | Frontend → Rust | `{ id: Option<String> }` | `()` | **FROZEN** |
| `queue_retry` | Frontend → Rust | `{ id: String }` | `()` | **FROZEN** |
| `queue_reorder` | Frontend → Rust | `{ id: String, targetPosition: usize }` | `()` | **FROZEN** |
| `queue_move_up` | Frontend → Rust | `{ id: String }` | `()` | **FROZEN** |
| `queue_move_down` | Frontend → Rust | `{ id: String }` | `()` | **FROZEN** |
| `queue_get` | Frontend → Rust | None | `Vec<QueueItem>` | **FROZEN** |
| `queue_clear_completed` | Frontend → Rust | None | `()` | **FROZEN** |
| `queue_clear_failed` | Frontend → Rust | None | `()` | **FROZEN** |
| `queue_set_concurrency` | Frontend → Rust | `{ concurrency: usize }` | `()` | **FROZEN** |
| `get_queue_summary` | Frontend → Rust | None | `QueueSummary` | **FROZEN** |
| `parse_batch_urls_text` | Frontend → Rust | `{ rawText: String }` | `Vec<String>` | **FROZEN** |
| `analyze_batch_urls` | Frontend → Rust | `{ urls: Vec<String> }` | `BatchAnalysisResult` | **FROZEN** |
| `add_batch_to_queue` | Frontend → Rust | `{ items: Vec<StartDownloadParams> }` | `BatchQueueResult` | **FROZEN** |
| `detect_playlist_url` | Frontend → Rust | `{ url: String }` | `Option<PlaylistDetection>` | **FROZEN** |
| `analyze_playlist` | Frontend → Rust | `{ url: String }` | `PlaylistInfo` | **FROZEN** |
| `add_playlist_to_queue` | Frontend → Rust | `{ items: Vec<PlaylistItemSelection>, quality: String }` | `BatchQueueResult` | **FROZEN** |

### Event Channels

| Channel | Direction | Payload Type | Purpose | Status |
| :--- | :--- | :--- | :--- | :--- |
| `download-progress` | Rust → Frontend | `DownloadProgressPayload` | Real-time byte, speed, and ETA metrics | **FROZEN** |
| `download-state-changed` | Rust → Frontend | `DownloadStateChangedPayload` | Lifecycle changes (`downloading`, `processing`, etc.) | **FROZEN** |
| `queue:item-added` | Rust → Frontend | `QueueItem` | Dispatched when a job enters queue | **FROZEN** |
| `queue:item-updated` | Rust → Frontend | `QueueItem` | Dispatched when queue metadata/status changes | **FROZEN** |
| `queue:item-removed` | Rust → Frontend | `{ id: String }` | Dispatched when job is deleted from queue | **FROZEN** |
| `queue:updated` | Rust → Frontend | `Vec<QueueItem>` | Dispatched on full queue state change or reorder | **FROZEN** |
| `queue:summary` | Rust → Frontend | `QueueSummary` | Dispatched with active/pending/failed counts | **FROZEN** |

---

## 7. Database Contract Freeze

### Database Configuration
- **Location**:
  - Windows: `%APPDATA%\FreeFetch\freefetch.db`
  - Linux/macOS: `$HOME/.freefetch/freefetch.db`
- **Pragmas**:
  - `PRAGMA journal_mode = WAL;` (Write-Ahead Logging for high concurrent read/write throughput)
  - `PRAGMA foreign_keys = ON;` (Referential integrity on cascaded deletions)
  - `PRAGMA synchronous = NORMAL;`

### Core Tables
1. **`settings`** (`WITHOUT ROWID`): Key-value storage for application configuration.
2. **`downloads`**: Immutable record of all historical and current downloads with timing, size, status, and destination path.
3. **`download_streams`**: Child table tracking individual video/audio streams belonging to a parent download job.
4. **`queue_items`**: Live queue state tracking position, status, and serialized `params_json`.

### Indexes
- `idx_downloads_status`: Optimizes status filtering in history view.
- `idx_downloads_created_at`: Optimizes reverse chronological pagination (`created_at DESC`).
- `idx_downloads_container`: Optimizes format filtering.
- `idx_download_streams_download_id`: Optimizes stream lookups and cascading deletions.
- `idx_queue_status`: Optimizes queue processor scheduling.
- `idx_queue_position`: Optimizes queue prioritization ordering (`position ASC`).
- `idx_queue_canonical_format`: Optimizes duplicate job detection.

### Startup Crash Recovery Invariant
On every application launch, migrations enforce:
```sql
UPDATE downloads SET status = 'paused' WHERE status IN ('downloading', 'processing', 'remuxing');
UPDATE queue_items SET status = 'paused' WHERE status IN ('downloading', 'processing', 'preparing', 'retrying');
```
*Backward Compatibility Invariant*: Existing SQLite schemas and rows must remain fully valid and migratable across all future UI and Settings releases.

---

## 8. Filesystem Contract

All file operations follow strict naming, isolation, and safety conventions:

### File Typology

| File Extension / Pattern | Purpose | Lifetime |
| :--- | :--- | :--- |
| `<dest>.<container>.part` | Data chunk file for single-stream downloads | Kept during transfer & pause; deleted on cancel or complete |
| `<dest>.<container>.part.meta`| Parallel chunk worker state descriptor | Kept during transfer & pause; deleted on cancel or complete |
| `<dest>.video.part` | Separate video stream chunk file | Kept during transfer & pause; deleted on cancel or complete |
| `<dest>.video.part.meta` | Separate video worker state descriptor | Kept during transfer & pause; deleted on cancel or complete |
| `<dest>.audio.part` | Separate audio stream chunk file | Kept during transfer & pause; deleted on cancel or complete |
| `<dest>.audio.part.meta` | Separate audio worker state descriptor | Kept during transfer & pause; deleted on cancel or complete |
| `<dest>.embed.tmp.<container>`| Temporary remuxed file during soft-sub embedding | Ephemeral; atomically renamed to final destination on success |
| `<dest>.hardcode.tmp.<container>`| Temporary re-encoded file during hardcoding | Ephemeral; atomically renamed to final destination on success |
| `<dest>.cover.tmp.jpg` | Temporary downloaded cover art image | Ephemeral; deleted immediately after audio conversion |
| `<dest>.<container>` | Final completed media file | Permanent |

### Cleanup Rules
- On **Complete**: Temporary part and metadata files are deleted immediately following FFmpeg verification and atomic file move.
- On **Cancel** or **Delete**: `clean_cancelled_files_with_id` attempts up to 5 times to remove all `.part`, `.part.meta`, `.tmp`, and `.cover.tmp.jpg` files.
- On **Pause**: `.part` and `.part.meta` files are strictly **preserved** on disk to allow byte-exact resumption.

---

## 9. Security Contract

FreeFetch enforces the following security boundaries:

1. **Path Traversal Protection**:
   - `FilenameTemplateEngine` strips relative traversal markers (`..`, `/`, `\`) and replaces illegal filesystem characters (`:`, `*`, `?`, `"`, `<`, `>`, `|`) with safe alternatives.
2. **Windows Reserved Device Protection**:
   - Device names (`CON`, `PRN`, `AUX`, `NUL`, `COM1`–`COM9`, `LPT1`–`LPT9`) are automatically prefixed with `_` (e.g., `_CON.mp4`) preventing Windows OS filesystem lockups.
3. **Path Length Bounding**:
   - Filename stems are truncated to `<= 180` characters to ensure the total absolute path does not exceed Windows `MAX_PATH` (260 characters).
4. **Direct Vector Subprocess Invocation**:
   - Zero shell execution (`cmd /c` or `powershell -c` are strictly prohibited). FFmpeg is executed as an isolated binary vector with individual arguments.
5. **Subtitle Filtergraph Escaping (Invariant S3)**:
   - File paths passed to FFmpeg's `-vf subtitles='...'` filter are sanitized: backslashes become forward slashes, drive colons are escaped (`\:`), and quotes are double-escaped (`'\\\'`).
6. **Destination Collision Avoidance**:
   - In-memory path reservation ensures that simultaneous queue items never generate identical part file or destination file paths.

---

## 10. Settings & UI Change Boundary

To maintain complete architectural integrity during the upcoming **Settings & UI/UX** milestone, the following change boundaries are established:

### Allowed (Frontend & UX Scope)
- Visual styling: Colors, dark/light themes, typography, padding, margins, borders, icons.
- Structural layout: Sidebar navigation, tabbed views, Settings drawer or modal, responsive grids.
- Preferences UI: Setting toggles, dropdowns, folder picker dialogs, slider controls.
- Component refactoring: Splitting large frontend components into smaller presentational subcomponents without altering IPC call signatures.
- Local UX state: Filter tab persistence, sorting selections, view mode switches (grid vs list).

### Not Allowed Without Architectural Review
- Modifying downloader engine files (`chunk_downloader.rs`, `parallel_downloader.rs`).
- Modifying queue state transitions or concurrency dispatching in `queue.rs`.
- Changing FFmpeg argument construction in `ffmpeg.rs`, `audio.rs`, or `subtitle.rs`.
- Changing SQLite schema or modifying `migrations.rs` without formal migration scripts.
- Modifying IPC command names, parameter schemas, or event channel names.
- Weakening security checks (path sanitization, Windows reserved names).

---

## 11. Future Settings Design Boundary

The table below outlines architectural boundaries for prospective preference categories in the upcoming milestone:

| Setting Key | Category | Persistence | Frontend Only? | Backend Impact? | Architectural Risk |
| :--- | :--- | :--- | :--- | :--- | :--- |
| `theme` | Appearance | LocalStorage / SQLite | Yes | None | **Low** |
| `download_directory` | General / Download | SQLite (`settings`) | No | Reads/writes in `AppSettings` | **Medium** (Path validation required) |
| `max_concurrent_downloads` | Download | SQLite (`settings`) | No | Dynamically updates `QueueManager` | **Low** (Already implemented) |
| `speed_limit_bytes_per_sec`| Download | SQLite (`settings`) | No | Dynamically updates `RateLimiter` | **Low** (Already supported in core) |
| `default_container` | Download | SQLite (`settings`) | No | Sets default format in `FormatModal`| **Low** |
| `default_subtitle_mode` | Subtitles | SQLite (`settings`) | Yes | Pre-selects None / Embed / Hardcode | **Low** |
| `default_audio_format` | Audio | SQLite (`settings`) | Yes | Pre-selects MP3 / M4A / FLAC / Opus | **Low** |
| `default_audio_bitrate` | Audio | SQLite (`settings`) | Yes | Pre-selects bitrate preset | **Low** |
| `embed_artwork` | Audio | SQLite (`settings`) | Yes | Pre-selects cover art embedding | **Low** |
| `ffmpeg_custom_path` | Advanced | SQLite (`settings`) | No | Validated via `check_ffmpeg_status` | **Medium** (Must verify binary) |
| `notifications_enabled` | Notifications | LocalStorage / SQLite | Yes | Web Notification API trigger | **Low** |
| `auto_clear_completed` | Queue | SQLite (`settings`) | Yes | Frontend queue trigger | **Low** |

---

## 12. P3 Issue Policy

### Issue Reference: `ISSUE-01`
- **Description**: External hard process termination (e.g. `Stop-Process -Force` or OS crash) leaves `.part.meta` in the downloads folder.
- **Classification**: **P3 — Low (Informational)**
- **Status**: **DEFERRED TO v0.2 MILESTONE**
- **Policy Justification**:
  1. In-flight `.part` and `.part.meta` files legitimately represent resumable work.
  2. The application crash recovery routine deliberately marks interrupted jobs as `Paused` so users can resume them.
  3. Automatic startup pruning without exhaustive cross-referencing with `downloads` and `queue_items` creates severe data loss risks for legitimate paused jobs.
  4. Cleanup of aborted jobs is already fully operational via the UI `Delete` / `Cancel` actions.

---

## 13. Regression Gate

Any future code change or commit MUST satisfy the following gate before merge:

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
npm test
npm run build
```

**Mandatory Invariant Gates**:
- Audio Invariants **A1–A12** must remain 100% passing.
- Subtitle Invariants **S1–S3** must remain 100% passing.
- Queue slot isolation and destination reservation must remain intact.
- SQLite WAL crash recovery must continue resetting in-flight downloads to `Paused`.
- Direct vector FFmpeg process execution must be maintained with zero shell interpolation.

---

## 14. Release Change Policy

1. **Feature Additions**:
   - Must be strictly additive.
   - Must not break existing IPC command schemas or event structures.
2. **Refactoring**:
   - Permitted only within the allowed UI/UX boundaries defined in Section 10.
   - Backend refactoring requires explicit pre-approval and comprehensive regression tests.
3. **Bug Fixes**:
   - Must follow: Reproduction Test → Root Cause Isolation → Minimal Fix → Regression Verification.
4. **New Settings**:
   - Must follow the categories, persistence models, and risk boundaries documented in Section 11.

---

## 15. Release Checklist

- [x] Production QA complete (155 backend tests, 26 frontend tests)
- [x] Architecture frozen
- [x] Downloader subsystem frozen
- [x] Queue subsystem frozen
- [x] Audio subsystem frozen (Invariants A1–A12)
- [x] Subtitle subsystem frozen (Invariants S1–S3)
- [x] FFmpeg process management frozen
- [x] History & analytics subsystem frozen
- [x] Filesystem security frozen
- [x] IPC contract documented & frozen (39 commands, 7 events)
- [x] Database contract documented & frozen (SQLite WAL, migrations, recovery)
- [x] Settings boundary documented
- [x] UI modification boundary documented
- [x] P3 issue (`ISSUE-01`) explicitly deferred to v0.2
- [x] Toolchain verification passed (`cargo fmt`, `cargo check`, `cargo clippy`, `cargo test`, `npm test`, `npm run build`)

---

## 16. Final Baseline Declaration

# **`ARCHITECTURE FREEZE COMPLETE`**

The FreeFetch production baseline is frozen, documented, and fully verified. The project is certified ready to begin the **Settings & UI/UX** milestone.
