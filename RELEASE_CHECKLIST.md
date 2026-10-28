# FreeFetch v0.1.0 — Release Checklist

## Milestone Sign-Off Checklist

- [x] **1. Code Freeze Confirmed**:
  Functional code frozen; no new features or unapproved changes added.
- [x] **2. Clean Production Build**:
  `freefetch.exe`, MSI installer, and NSIS installer built via `npx tauri build`.
- [x] **3. Production Packaging Verified**:
  Frontend assets bundled internally in `dist/`. No reliance on `localhost:1420` or Vite dev server. Native `tauri.localhost` protocol used. Zero external browser launches.
- [x] **4. Child Process Silence**:
  Win32 `CREATE_NO_WINDOW = 0x08000000` enforced on all child processes. Zero terminal / console windows appear during analysis, download, FFmpeg, or subtitle operations.
- [x] **5. MSI Installer Installation Verified**:
  Installs cleanly via `msiexec.exe /i FreeFetch_0.1.0_x64_en-US.msi MSIINSTALLPERUSER=1 ALLUSERS=2 /qn /norestart`. Exit code: 0.
- [x] **6. Shortcuts & System Integration**:
  Start Menu shortcut and Desktop shortcut created. Windows Uninstall registry entry created under `HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\{8E272182-0E49-448E-84B3-D60DD5F40FF5}`.
- [x] **7. Installed App First-Launch**:
  Installed binary `C:\Users\bdkba\AppData\Local\Programs\FreeFetch\freefetch.exe` opens with `Responding: True`, WorkingSet ~45MB, UI renders immediately with all 3 tabs functional.
- [x] **8. Clean MSI Uninstallation Verified**:
  Uninstalls via `msiexec.exe /x ... /qn`. Exit code: 0. All binaries, shortcuts, and registry entries removed cleanly with 0 orphans.
- [x] **9. Clean Reinstallation Verified**:
  Re-running installer returns exit code 0; application restored.
- [x] **10. Settings Persistence Verified**:
  All 12 settings persist across restarts in SQLite `app_settings` key.
- [x] **11. Structured Error Handling (No `[object Object]`)**:
  Error formatting utility and backend string serialization prevent `[object Object]`. Human-readable error messages displayed for rate limits and bot challenges.
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
- [x] **19. Full Automated Regression Passed**:
  - `cargo fmt --check`: PASS (0 diffs)
  - `cargo check`: PASS (0 errors)
  - `cargo clippy -D warnings`: PASS (0 warnings)
  - `cargo test`: PASS (181/181 backend tests)
  - `npm test`: PASS (41/41 frontend tests)
  - `npm run build`: PASS (1959 modules transformed)
  - `npx tauri build`: PASS (MSI + NSIS bundles generated)

---

## Final Milestone Decision
**FREEFETCH v0.1.0 — RELEASE CANDIDATE APPROVED**
