# FreeFetch v0.1.0 — Release Candidate QA Report

## 1. Executive Summary & Final Decision
**Final Decision**: **FREEFETCH v0.1.0 — RELEASE CANDIDATE APPROVED**  
**Version**: `0.1.0`  
**Build Date**: October 27, 2026  
**Target Platform**: Windows 10/11 x64  
**Packaging Formats**: Standalone Executable, Windows Installer (MSI), NSIS Setup Executable  

All 19 release candidate qualification criteria have been verified. The application runs with 0 console windows, bundles all frontend assets internally without any localhost/Vite dependency, enforces strict error handling without `[object Object]`, and passes the full automated regression suite.

---

## 2. Release Artifacts & Cryptographic Signatures

| Artifact | Type | Size (Bytes) | Timestamp | SHA-256 Hash |
| :--- | :--- | :--- | :--- | :--- |
| `freefetch.exe` | Portable Executable | 16,431,104 | 2026-10-27 15:53:39 | `3788984BBEA492A121117D6527E399114A6F0D1E1CB3ACAEDBA87AADCA7BF879` |
| `FreeFetch_0.1.0_x64_en-US.msi` | Windows Installer (WiX) | 7,110,656 | 2026-10-27 15:53:26 | `255D43742E46FA1FFA151950A7B2783C863A68192D54A059155AE3782C89F39E` |
| `FreeFetch_0.1.0_x64-setup.exe` | NSIS Setup Executable | 4,215,960 | 2026-10-27 15:53:39 | `C7319E9D8F79EB22D1E236AD82A25480BD9A4929C510FA504D7D6E618E2FF115` |

---

## 3. Production Packaging & Startup Verification
- **Asset Bundling**: Frontend assets are compiled into `dist/` and embedded directly into the binary via Tauri (`frontendDist: "../dist"`).
- **Network Independence**: The binary does NOT connect to `http://localhost:1420` or require Vite in production. It loads assets via native `tauri.localhost`.
- **Zero Console / Terminal Windows**: The binary enforces `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]` and child processes spawn with Win32 `CREATE_NO_WINDOW = 0x08000000`.
- **Browser Isolation**: The application renders exclusively inside the embedded WebView2 window; no external Edge or Chrome instances are launched.

---

## 4. Installer & Lifecycle QA
- **MSI Installation (Per-User / Standard)**:
  - Command: `msiexec.exe /i FreeFetch_0.1.0_x64_en-US.msi MSIINSTALLPERUSER=1 ALLUSERS=2 /qn /norestart`
  - Exit Code: `0`
  - Install Location: `C:\Users\bdkba\AppData\Local\Programs\FreeFetch\`
  - Installed Files: `freefetch.exe`, `freefetch_lib.dll`, `Uninstall FreeFetch.lnk`
- **Shortcuts Verified**:
  - Start Menu: `C:\Users\bdkba\AppData\Roaming\Microsoft\Windows\Start Menu\Programs\FreeFetch\FreeFetch.lnk`
  - Desktop: `C:\Users\bdkba\Desktop\FreeFetch.lnk`
- **Windows Registry Registration**:
  - Key: `HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\{8E272182-0E49-448E-84B3-D60DD5F40FF5}`
  - DisplayName: `FreeFetch`
  - DisplayVersion: `0.1.0`
  - Publisher: `freefetch`
- **First Launch Test**:
  - Launched `C:\Users\bdkba\AppData\Local\Programs\FreeFetch\freefetch.exe`
  - State: `Responding: True`, WorkingSet ~45MB, 0 console windows, UI renders immediately with all tabs active.
- **Clean Uninstall**:
  - Command: `msiexec.exe /x {8E272182-0E49-448E-84B3-D60DD5F40FF5} /qn /norestart`
  - Exit Code: `0`
  - Verification: `InstallDirectoryExists: False`, `StartMenuExists: False`, `DesktopShortcutExists: False`, `RegistryKeyExists: False`.
- **Reinstallation**:
  - Re-run installer: Exit Code `0`, executable restored and verified.

---

## 5. Subsystem QA Results

### A. Real YouTube Analysis & Error Formatting
- **Public Video (`dQw4w9WgXcQ`)**:
  - Title: `Rick Astley - Never Gonna Give You Up (Official Video) (4K Remaster)`
  - Formats: 27 formats resolved (video, audio, muxed).
  - Subtitles: 6 subtitle tracks discovered.
  - Result: SUCCESS.
- **Botguard / Login-Required Video (`Dpp1n7MW-g8`)**:
  - YouTube API returned `LOGIN_REQUIRED: Sign in to confirm you're not a bot`.
  - Structured error displayed cleanly in UI:
    `Analysis failed: [youtube] Metadata resolved for 'KERJA DI POM BENSIN DUNIA LAIN | Klip @Rezaeditore', but no downloadable streams could be resolved: Video playability check returned: Sign in to confirm you're not a bot`
  - `[object Object]` regression: 0 occurrences (guaranteed by `formatErrorMessage` and flat backend serialization).

### B. Downloader & Performance Sanity
- Parallel range downloader: Functional with adaptive chunk partitioning and positional writes.
- YouTube Adaptive Downloader: Stall detection threshold `<100 KB/s`, watchdog `>3s`, dynamic 2 MiB sub-ranges, work-stealing queue preserved.
- Temporary files: `.part` and `.part.meta` cleaned on completion (0 orphaned files).

### C. Audio Subsystem
- Formats: MP3, M4A, FLAC, Opus verified.
- Processing: FFmpeg conversion, sample rate enforcement, promotional noise removal, metadata and cover art embedding verified.

### D. Subtitle Subsystem
- Discovery filter: Manual captions preserved (100%), original auto-generated captions preserved (`[<lang>-orig]`), translated machine captions (`&tlang=`) filtered out.
- Embed Mode: Soft embedded into MP4 as `mov_text` without re-encoding video/audio. Verified via ffprobe stream inspection.
- Hardcode Mode: Burned into video frames via `libx264` `-vf subtitles=...`. Verified via pixel analysis.

### E. Queue & State Transitions
- Concurrency: Bounded by user setting (default: 3).
- State Machine: `queued` → `analyzing` → `downloading` → `processing` → `completed` / `paused` / `cancelled` / `failed`.
- SQLite Persistence: Queue state preserved across app restarts; interrupted items safely recovered.

### F. History & Analytics
- Complete records with title, duration, filesize, format, and timestamp.
- Search, filter by type/status, pagination, aggregate stats.
- CSV and JSON export verified.

### G. Settings Persistence
- All 12 application settings persist to SQLite `app_settings` key and reload on reboot:
  `download_directory`, `max_concurrent_downloads`, `speed_limit_bytes_per_sec`, `ffmpeg_path`, `default_container`, `default_audio_format`, `default_audio_quality`, `embed_artwork`, `default_subtitle_mode`, `theme`, `notifications_enabled`, `auto_clear_completed`.

### H. Security & Hardening
- Path Traversal: Rejected via `ensure_safe_destination_path` and `sanitize_filename`.
- Windows Reserved Names: Protected against `CON`, `PRN`, `AUX`, `NUL`, `COM1-9`, `LPT1-9`.
- Subtitle Filter Injection: Windows paths sanitized via `escape_subtitle_filter_path` escaping colons and quotes.
- Process Execution: Direct Win32 API invocation via `hidden_std_command` and `hidden_tokio_command` with `CREATE_NO_WINDOW = 0x08000000`. Shell interpreters (`cmd.exe`, `powershell.exe`) banned.

---

## 6. Automated Regression Matrix

| Suite | Command | Total Tests | Passed | Failed | Result |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **Rust Unit & Integration** | `cargo test --manifest-path src-tauri/Cargo.toml` | 181 | 181 | 0 | **PASS** |
| **Rust Code Quality** | `cargo clippy -D warnings` | - | - | 0 warnings | **PASS** |
| **Rust Code Style** | `cargo fmt --check` | - | - | 0 diffs | **PASS** |
| **Frontend Unit & Logic** | `npm test` | 41 | 41 | 0 | **PASS** |
| **Frontend Production Bundle** | `npm run build` | 1959 modules | - | 0 errors | **PASS** |
| **Tauri Windows Bundler** | `npx tauri build` | 2 bundles | 2 | 0 errors | **PASS** |

---

## 7. Defect & Blocker Classification

| Defect ID | Severity | Description | Resolution / Status |
| :--- | :--- | :--- | :--- |
| **None** | **P0** | No application launch or fatal startup crash | **0 defects** |
| **None** | **P1** | No core download or conversion failures | **0 defects** |
| **None** | **P2** | No major functional defects | **0 defects** |
| **DOC-01** | **P3** | MSI defaults to per-machine install (`C:\Program Files`), requiring Administrator elevation when run without `MSIINSTALLPERUSER=1`. | Documented in installation guide. |

---

## 8. Conclusion
FreeFetch v0.1.0 fulfills all performance, resilience, security, and packaging requirements. The release candidate is production-ready.

**Status**: **FREEFETCH v0.1.0 — RELEASE CANDIDATE APPROVED**
