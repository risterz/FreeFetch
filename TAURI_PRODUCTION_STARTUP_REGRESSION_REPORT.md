# TAURI PRODUCTION STARTUP REGRESSION REPORT

## 1. Incident Summary & Symptom

- **Severity**: Critical (P0 Production Launch Blocker)
- **Symptom**: On launching the release binary `src-tauri\target\release\freefetch.exe`, the native Tauri WebView2 container opened but displayed a browser network error:
  ```text
  "Hmmm... can't reach this page"
  "localhost refused to connect."
  "ERR_CONNECTION_REFUSED"
  ```
- **Context**: The build was expected to load packaged static production assets (`dist/`), but instead attempted to connect to the Vite development server (`http://localhost:1420`), failing when no local server was running.

---

## 2. Root Cause Analysis

### Code Generation & Feature Flag Mechanism

In Tauri v2 (`tauri` crate `v2.12.5` & `tauri-build` `v2.7.4`):

1. **`tauri/build.rs` Logic**:
   ```rust
   let custom_protocol = has_feature(&mut checked_features, "custom-protocol");
   let dev = !custom_protocol;
   println!("cargo:dev={dev}");
   ```
   If the `custom-protocol` Cargo feature is NOT active, Tauri's build script emits `cargo:dev=true`.

2. **`tauri-build/src/lib.rs` Logic**:
   ```rust
   pub fn is_dev() -> bool {
       env::var_os("DEP_TAURI_DEV").expect("...") == "true"
   }
   ```
   When `DEP_TAURI_DEV` is `"true"`, `tauri-build` treats the compilation as a development build, regardless of whether `--release` profile was used with Cargo.

3. **`tauri-codegen/src/context.rs` Logic**:
   ```rust
   let assets = if let Some(assets) = assets {
       quote!(#assets)
   } else if dev && config.build.dev_url.is_some() {
       let assets = EmbeddedAssets::default();
       quote!(#assets)
   } else {
       // Embeds assets from frontendDist
   };
   ```
   Because `dev` was `true`, `tauri-codegen` skipped bundling the assets in `../dist` and generated code routing the main window to `build.devUrl` (`http://localhost:1420`).

4. **Missing Manifest Configuration in `src-tauri/Cargo.toml`**:
   The `src-tauri/Cargo.toml` file did not define a `[features]` section mapping `custom-protocol = ["tauri/custom-protocol"]` and had no default feature configured. When `cargo build --release` was invoked directly, Cargo executed without `custom-protocol`, forcing Tauri into `dev=true` mode and causing WebView2 to navigate to `localhost:1420`.

---

## 3. Resolution & Configuration Fix

In [`src-tauri/Cargo.toml`](file:///D:/Antigravity/FreeFetch/src-tauri/Cargo.toml), added the standard Tauri production protocol feature specification:

```toml
[features]
default = ["custom-protocol"]
custom-protocol = ["tauri/custom-protocol"]
```

### Invariants Maintained:
- **Zero changes** to downloader architecture (`RangeWorkQueue`, `StallWatchdog`, adaptive sub-ranges).
- **Zero changes** to YouTube extraction, audio, subtitle, FFmpeg, SQLite, or queue subsystems.
- Both development (`npm run dev` on port 1420) and production (`npm run build` -> `cargo build --release`) are fully supported.
- In production, static assets are embedded into `freefetch.exe` and served via `http://tauri.localhost/`.

---

## 4. Verification & Validation Evidence

### Toolchain Quality Gates:
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check`: **PASS** (0 diffs)
- `cargo check --manifest-path src-tauri/Cargo.toml`: **PASS** (0 errors)
- `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings`: **PASS** (0 warnings)
- `cargo test --manifest-path src-tauri/Cargo.toml`: **173 / 173 PASS** (32 unit + 141 integration)
- `npm test`: **35 / 35 PASS**
- `npm run build`: **PASS** (1958 modules transformed, clean `dist/` bundle)
- `cargo build --release --manifest-path src-tauri/Cargo.toml`: **PASS**

### Binary Artifact Inspection:
- **Path**: `src-tauri\target\release\freefetch.exe`
- **File Size**: `16,418,304 bytes` (increased from 16,336,384 bytes, confirming embedded `tauri-codegen-assets`)
- **Generated Codegen Directory**: `src-tauri\target\release\build\freefetch-53ef6a32631a2157\out\tauri-codegen-assets\` verified populated with embedded HTML, CSS, and JS chunks.

### Production Smoke Test & URL Inspection:
The compiled `freefetch.exe` was executed in isolated production mode with no Vite/Node server running. The internal WebView2 navigation database (`EBWebView\Default\History`) was queried:

```text
URL: http://tauri.localhost/
Title: FreeFetch
Visit Count: 11
Status: 200 OK (Loaded from embedded internal assets)
```

No connection refusal or localhost dependency occurred.

---

## 5. Final Classification

**FIXED — PRODUCTION STARTUP VERIFIED**
