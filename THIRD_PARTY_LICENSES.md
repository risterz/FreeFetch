# FreeFetch — Third-Party Licenses & Redistribution Notices

This document details the licensing terms, redistribution notices, and source code availability for all third-party software, binaries, and libraries bundled with or utilized by FreeFetch.

---

## 1. FreeFetch Application Source Code
- **License**: MIT License (see `LICENSE`)
- **Copyright**: (c) 2026 FreeFetch Team & Contributors
- **Terms**: Permissive; grants rights to use, copy, modify, merge, publish, distribute, sublicense, and sell.

---

## 2. Bundled External Binaries

### A. FFmpeg (`ffmpeg.exe`)
- **Version**: 7.1.1-essentials (Windows x86_64)
- **License**: GNU General Public License v3.0 (GPLv3)
- **Copyright**: (c) 2000-2025 the FFmpeg developers
- **Upstream Source**: https://ffmpeg.org
- **Build Provider**: gyan.dev (https://www.gyan.dev/ffmpeg/builds/)
- **Configuration**:
  ```
  --enable-gpl --enable-version3 --enable-static --disable-w32threads --disable-autodetect
  --enable-fontconfig --enable-iconv --enable-gnutls --enable-libxml2 --enable-gmp --enable-bzlib
  --enable-lzma --enable-zlib --enable-libsrt --enable-libssh --enable-libzmq --enable-avisynth
  --enable-sdl2 --enable-libwebp --enable-libx264 --enable-libx265 --enable-libxvid --enable-libaom
  --enable-libopenjpeg --enable-libvpx --enable-mediafoundation --enable-libass --enable-libfreetype
  --enable-libfribidi --enable-libharfbuzz --enable-libvidstab --enable-libvmaf --enable-libzimg
  --enable-amf --enable-cuda-llvm --enable-cuvid --enable-dxva2 --enable-d3d11va --enable-d3d12va
  --enable-ffnvcodec --enable-libvpl --enable-nvdec --enable-nvenc --enable-vaapi --enable-libgme
  --enable-libopenmpt --enable-libopencore-amrwb --enable-libmp3lame --enable-libtheora
  --enable-libvo-amrwbenc --enable-libgsm --enable-libopencore-amrnb --enable-libopus --enable-libspeex
  --enable-libvorbis --enable-librubberband
  ```
- **License Implications & GPLv3 Compliance**:
  - The bundled FFmpeg binary includes GPLv3-licensed components (`libx264`, `libx265`, etc.).
  - Under GPLv3, distributing a binary package containing GPLv3 code requires that the complete corresponding source code for all bundled software is made available under compatible terms.
  - FreeFetch source code is released under the MIT license, which is explicitly GPLv3-compatible. The complete FreeFetch source repository is openly available at https://github.com/freefetch/freefetch.
  - FFmpeg source code can be downloaded directly from https://ffmpeg.org/download.html or cloned from `git://source.ffmpeg.org/ffmpeg.git`.

### B. yt-dlp (`yt-dlp.exe`)
- **Version**: 2026.10.26 (Windows x86_64 standalone single-file binary)
- **License**: The Unlicense (Public Domain Dedication)
- **Upstream Source**: https://github.com/yt-dlp/yt-dlp
- **Terms**:
  ```
  This is free and unencumbered software released into the public domain.

  Anyone is free to copy, modify, publish, use, compile, sell, or
  distribute this software, either in source code form or as a compiled
  binary, for any purpose, commercial or non-commercial, and by any
  means.
  ```

---

## 3. Rust Backend Dependencies (`src-tauri/Cargo.toml`)

| Crate | Version | License | Upstream |
| :--- | :--- | :--- | :--- |
| `tauri` | 2.x | MIT / Apache-2.0 | https://github.com/tauri-apps/tauri |
| `tokio` | 1.x | MIT | https://github.com/tokio-rs/tokio |
| `reqwest` | 0.12 | MIT / Apache-2.0 | https://github.com/seanmonstar/reqwest |
| `rusqlite` | 0.32 | MIT | https://github.com/rusqlite/rusqlite |
| `serde` | 1.x | MIT / Apache-2.0 | https://github.com/serde-rs/serde |
| `serde_json` | 1.x | MIT / Apache-2.0 | https://github.com/serde-rs/json |
| `uuid` | 1.x | MIT / Apache-2.0 | https://github.com/uuid-rs/uuid |
| `chrono` | 0.4 | MIT / Apache-2.0 | https://github.com/chronotope/chrono |
| `regex` | 1.x | MIT / Apache-2.0 | https://github.com/rust-lang/regex |
| `url` | 2.x | MIT / Apache-2.0 | https://github.com/servo/rust-url |
| `thiserror` | 2.x | MIT / Apache-2.0 | https://github.com/dtolnay/thiserror |
| `futures-util` | 0.3 | MIT / Apache-2.0 | https://github.com/rust-lang/futures-rs |

---

## 4. Frontend Web Dependencies (`package.json`)

| Package | Version | License | Upstream |
| :--- | :--- | :--- | :--- |
| `react` | 18.3.1 | MIT | https://github.com/facebook/react |
| `react-dom` | 18.3.1 | MIT | https://github.com/facebook/react |
| `@tauri-apps/api` | 2.2.0 | MIT / Apache-2.0 | https://github.com/tauri-apps/tauri |
| `lucide-react` | 1.16.0 | ISC | https://github.com/lucide-icons/lucide |
| `tailwindcss` | 3.4.17 | MIT | https://github.com/tailwindlabs/tailwindcss |
| `clsx` | 2.1.1 | MIT | https://github.com/lukeed/clsx |
| `tailwind-merge` | 2.6.0 | MIT | https://github.com/dcastil/tailwind-merge |
| `vite` | 6.1.0 | MIT | https://github.com/vitejs/vite |
| `vitest` | 3.0.5 | MIT | https://github.com/vitest-dev/vitest |
