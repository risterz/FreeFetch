# FreeFetch Sidecars Directory

This directory contains bundled external binary executables for desktop builds:

- `ffmpeg.exe`: FFmpeg v7.1.1 essentials (x86_64) from Gyan.dev (GPLv3).
- `yt-dlp.exe`: yt-dlp v2026.10.26 standalone executable (The Unlicense).

These raw executable binaries (`*.exe`) are excluded from Git repository history via `.gitignore` to keep the source repository lightweight and clone-friendly.

During production release packaging (`npm run tauri build`), Tauri embeds these binaries into the installer payload under `resources: ["sidecars/*"]`.
