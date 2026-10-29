# FreeFetch v0.1.0 — Public Beta Release Checklist

## 1. Milestone Sign-Off Checklist

- [x] **1. Code Freeze Confirmed**:
  Functional code frozen; multiplatform support (TikTok, Google Drive, Dropbox), native ShellExecute file opener, and parallel downloader frozen.
- [x] **2. Clean Production Build**:
  `freefetch.exe`, MSI installer, and NSIS installer built via `npx tauri build`.
- [x] **3. Production Packaging Verified**:
  Frontend assets bundled internally in `dist/`. No reliance on `localhost:1420` or Vite dev server. Native `tauri.localhost` protocol used. Zero external browser launches.
- [x] **4. Child Process Silence**:
  Win32 `CREATE_NO_WINDOW = 0x08000000` enforced on all child processes. Zero terminal / console windows appear during analysis, download, FFmpeg, or subtitle operations.
- [x] **5. MSI Installer Installation Verified**:
  Installs cleanly via `msiexec.exe /i FreeFetch_0.1.0_x64_en-US.msi MSIINSTALLPERUSER=1 ALLUSERS=2 /qn /norestart`. Exit code: 0.
- [x] **6. Shortcuts & System Integration**:
  Start Menu shortcut and Desktop shortcut created. Windows Uninstall registry entry registered under `HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\{8E272182-0E49-448E-84B3-D60DD5F40FF5}`.
- [x] **7. Installed App First-Launch**:
  Installed binary `C:\Users\bdkba\AppData\Local\Programs\FreeFetch\freefetch.exe` opens with `Responding: True`, WorkingSet ~46MB, UI renders immediately with all tabs functional.
- [x] **8. Clean MSI Uninstallation Verified**:
  Uninstalls via `msiexec.exe /x ... /qn`. Exit code: 0. All binaries, shortcuts, and registry entries removed cleanly with 0 orphans.
- [x] **9. Clean Reinstallation Verified**:
  Re-running installer returns exit code 0; application restored cleanly.
- [x] **10. Settings Persistence Verified**:
  All 12 settings persist across restarts in SQLite `app_settings` key.
- [x] **11. Structured Error Handling (No `[object Object]`)**:
  Error formatting utility and backend string serialization prevent `[object Object]`. Actionable error messages displayed for rate limits, bot challenges, and missing dependencies.
- [x] **12. Real YouTube Analysis Verified**:
  `dQw4w9WgXcQ` resolves all 27 formats and 6 subtitles. `Dpp1n7MW-g8` cleanly displays YouTube sign-in requirement without crashes or raw objects.
- [x] **13. Downloader & Temporary File Hygiene**:
  Adaptive range downloader active. `.part` and `.part.meta` cleaned up upon download completion (0 orphans).
- [x] **14. Audio Subsystem Qualified**:
  MP3, M4A, FLAC, Opus conversion, metadata tagging, and cover art embedding verified.
- [x] **15. Subtitle Subsystem Qualified**:
  Machine-translated tracks (`&tlang=`) filtered out. Manual and original auto-generated tracks preserved. Soft embedding (`mov_text`) and hardcoded burning verified.
- [x] **16. Queue State Machine Qualified**:
  Concurrency bounded, item reordering, pause, resume, cancel, and crash recovery verified.
- [x] **17. History & Analytics Qualified**:
  Search, filtering, aggregate stats, and CSV/JSON export verified.
- [x] **18. Security Invariants Qualified**:
  Path traversal protection, Windows reserved filename blocking, filtergraph path escaping, and direct executable invocation verified.
- [x] **19. Multiplatform Extraction & Downloads Qualified**:
  - TikTok: Extractor routing, video download, MP4 H.264/AAC playback verified via ffprobe.
  - Google Drive: File ID extraction, direct download, and Content-Disposition filename parsing verified.
  - Dropbox: Shared link format normalization and direct download verified.
  - ShellExecute file & folder opener: Native Win32 `ShellExecuteW` execution verified for paths containing spaces, commas, quotes, and Unicode characters.
- [x] **20. Dependency Distribution & Sidecars Qualified**:
  - Evaluated sidecar bundling vs. runtime setup wizard. Selected verified sidecar distribution inside the installer to guarantee out-of-the-box operation on clean Windows installations without developer tools.
  - Standalone `ffmpeg.exe` (v7.1.1 essentials build) and `yt-dlp.exe` (v2026.10.26 standalone binary) bundled into installer under `sidecars/`.
  - Executable discovery verifies paths, versions, and execution permissions with Win32 `CREATE_NO_WINDOW`. Actionable error messages provided if external tools are removed.
- [x] **21. Licensing & Legal Compliance Qualified**:
  - Added MIT `LICENSE` covering FreeFetch application source code.
  - Added `THIRD_PARTY_LICENSES.md` documenting GPLv3 obligations for FFmpeg, Unlicense for yt-dlp, and MIT/Apache-2.0 for all Rust and npm dependencies.
- [x] **22. Code Signing & SmartScreen Strategy Qualified**:
  - Release explicitly classified as **Unsigned Public Beta**; zero invented certificates or mock signatures.
  - Documented Authenticode requirements (EV/Standard certificate or Azure Trusted Signing). Documented Windows SmartScreen bypass instructions and published SHA-256 checksums in README.
- [x] **23. Update Strategy Qualified**:
  - Secure Tauri auto-updater deferred until public production CI/CD infrastructure is active.
  - Documented manual installer update workflow from official GitHub Releases.
  - In-app `yt-dlp` independent self-updater verified via `Settings > Advanced > Update yt-dlp`.
- [x] **24. Full Automated Regression Passed**:
  - `cargo check`: PASS (0 errors)
  - `cargo test --lib`: PASS (36/36 tests)
  - `cargo test --tests`: PASS (181/181 tests)
  - `npm test`: PASS (48/48 tests)
  - `npm run build`: PASS (1960 modules transformed)
  - `npx tauri build`: PASS (MSI + NSIS bundles generated)

---

## 2. Release Artifacts & Cryptographic Signatures

| Artifact | Type | Size (Bytes) | Exact Absolute Path | SHA-256 Hash |
| :--- | :--- | :--- | :--- | :--- |
| `freefetch.exe` | Portable Executable | 18,669,568 | `D:\Antigravity\FreeFetch\src-tauri\target\release\freefetch.exe` | `3CA0DE43CB70C8DC881B3E83D65AA63966B6804796FF19F0107EAAA0A0DAF168` |
| `FreeFetch_0.1.0_x64_en-US.msi` | Windows Installer (WiX) | 62,484,480 | `D:\Antigravity\FreeFetch\src-tauri\target\release\bundle\msi\FreeFetch_0.1.0_x64_en-US.msi` | `EB05459247D3B806C84B405D5E3B8B0A538F877CAA5FF528CAA7F0CDE702E66F` |
| `FreeFetch_0.1.0_x64-setup.exe` | NSIS Setup Executable | 51,094,956 | `D:\Antigravity\FreeFetch\src-tauri\target\release\bundle\nsis\FreeFetch_0.1.0_x64-setup.exe` | `4AFF64BAD042B7AE3141E3CF52D75EF020F0AC4D90E7790D7B7C4036430B44A9` |

---

## 3. Bundled Sidecar Verification

| Sidecar Binary | Upstream Version | License | Bundle Relative Path | Verified SHA-256 Hash |
| :--- | :--- | :--- | :--- | :--- |
| `ffmpeg.exe` | 7.1.1-essentials | GPLv3 | `sidecars/ffmpeg.exe` | `b90225987bdd042cca09a1efb5e34e9848f2d1dbf5fbcd388753a44145522997` |
| `yt-dlp.exe` | 2026.10.26 (standalone) | The Unlicense | `sidecars/yt-dlp.exe` | `f5389d3698ecde4ef06f4e6b1ce0294fe9ac1396228c8990b8d3416d6d177a7d` |

---

## 4. Final Milestone Decision

**FREEFETCH v0.1.0 — PUBLIC BETA QUALIFIED & APPROVED FOR RELEASE**
