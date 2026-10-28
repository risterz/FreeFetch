# FreeFetch Modern UI/UX Design System

## 1. Executive Summary & Brand Identity

**FreeFetch** is a high-performance, cross-platform media downloader engineered with Tauri v2, Rust, and React 18. This document formalizes the complete Modern UI/UX architecture and design system implemented in FreeFetch.

### Brand Promise: Speed + Simplicity + Media
- **Fast**: Instant input parsing, event-driven streaming progress, zero visual stutter, and GPU-accelerated layout transitions.
- **Clean**: High-contrast, typography-first aesthetic inspired by Linear and Raycast; zero AI-slop gradients, zero decorative clutter.
- **Premium**: Pixel-precise alignment, restrained elevation shadows, accessible WCAG AA contrast ratios, and tactile micro-interactions.

---

## 2. Design Tokens

### 2.1 Color Palette

The interface operates under a dual-theme architecture supporting dynamic **Light**, **Dark**, and **System** OS modes with seamless switching.

| Token | Light Theme | Dark Theme | Purpose |
| :--- | :--- | :--- | :--- |
| `bg-app` | `#F8FAFC` (`slate-50`) | `#020617` (`slate-950`) | Primary desktop viewport canvas |
| `surface-base` | `#FFFFFF` (`white`) | `#0F172A` (`slate-900`) | Main application cards and containers |
| `surface-elevated` | `#F1F5F9` (`slate-100`) | `#1E293B` (`slate-800`) | Toolbars, secondary controls, input backgrounds |
| `surface-hover` | `#E2E8F0` (`slate-200`) | `#334155` (`slate-700`) | Hovered interactive elements |
| `border-subtle` | `#E2E8F0` (`slate-200`) | `#1E293B` (`slate-800`) | Card containers and structural dividers |
| `border-interactive` | `#CBD5E1` (`slate-300`) | `#334155` (`slate-700`) | Inputs, button perimeters, active borders |
| `text-primary` | `#0F172A` (`slate-900`) | `#F8FAFC` (`slate-50`) | Main headings, critical titles, active labels |
| `text-secondary` | `#475569` (`slate-600`) | `#94A3B8` (`slate-400`) | Descriptions, subtitles, metric descriptions |
| `text-muted` | `#94A3B8` (`slate-400`) | `#64748B` (`slate-500`) | Placeholder hints, timestamps, footer notes |
| `accent-primary` | `#059669` (`emerald-600`) | `#10B981` (`emerald-500`) | Primary download actions, ready badges, success |
| `accent-nav` | `#4F46E5` (`indigo-600`) | `#6366F1` (`indigo-500`) | Active tab pill, Remux indicator, Queue badges |
| `status-warning` | `#D97706` (`amber-600`) | `#F59E0B` (`amber-500`) | Paused downloads, hardcode notices, private items |
| `status-danger` | `#E11D48` (`rose-600`) | `#F43F5E` (`rose-500`) | Failed downloads, delete modals, error banners |

### 2.2 Typography Scale

FreeFetch uses system font stacks prioritizing geometric sans-serif typefaces (`Inter`, `-apple-system`, `BlinkMacSystemFont`, `Segoe UI`, `Roboto`):

- **Display / Hero**: `text-2xl font-bold tracking-tight` (Header title and hero callouts).
- **Page Heading**: `text-xl font-bold tracking-tight` (Settings and Analytics headings).
- **Section Heading**: `text-sm font-semibold` (Card section grouping titles).
- **Card Title**: `text-sm font-medium` (Queue item media title, truncated with ellipsis).
- **Body Regular**: `text-xs font-normal` (Descriptions and explanatory copy).
- **Metadata / Monospace**: `text-[11px] font-mono` (Speeds, ETAs, file sizes, resolutions, keyboard hints).
- **Micro Badge**: `text-[10px] font-bold uppercase tracking-wider` (Container pills: MP4, MKV, MP3, Remux).

### 2.3 Spacing Scale & Border Radius

- **Grid Spacing**: 4px base (`p-1`: 4px, `p-2`: 8px, `p-3`: 12px, `p-4`: 16px, `p-5`: 20px, `p-6`: 24px).
- **Card Radius**: `rounded-2xl` (16px) for major modal cards, settings groups, and hero search containers; `rounded-xl` (12px) for item rows, inputs, and buttons; `rounded-lg` (8px) for badges and micro-toggles.
- **Elevation / Shadows**:
  - `shadow-xs`: Subtle card perimeters (`box-shadow: 0 1px 2px 0 rgb(0 0 0 / 0.05)`).
  - `shadow-md`: Primary action button glow (`emerald-600/20` and `indigo-600/20`).
  - `shadow-2xl`: High-z-index modal dialog overlays.

---

## 3. Application Shell & Navigation

```text
┌───────────────────────────────────────────────────────────────────────────────┐
│ [Logo] FreeFetch  •  Ready (FFmpeg 7.1)                [+ Multi-URL] [v0.1.0] │
├───────────────────────────────────────────────────────────────────────────────┤
│                                                                               │
│   ┌───────────────────────────────────────────────────────────────────────┐   │
│   │ 🔗  Paste video or audio link (Ctrl+V)...             [ Analyze URL ] │   │
│   └───────────────────────────────────────────────────────────────────────┘   │
│                                                                               │
│   [ 1: Download Queue (3) ]   [ 2: All History (12) ]   [ 3: Settings ]  (↻)  │
├───────────────────────────────────────────────────────────────────────────────┤
│                                                                               │
│   MAIN CONTENT AREA (Queue, History Analytics, or Settings)                   │
│                                                                               │
└───────────────────────────────────────────────────────────────────────────────┘
```

### 3.1 Global Navigation Tabs
1. **Download Queue (`1`)**: Active transfer engine with aggregate throughput metrics, concurrency selector (1–5), batch operations, and real-time download cards.
2. **All History (`2`)**: Historical SQLite-persisted downloads, statistical cards (Total, Completed, Transferred Bytes, Average Speed), multi-attribute filter pills, text search, CSV/JSON export, and individual file actions.
3. **Settings (`3`)**: Comprehensive preferences categorized into General, Downloads, Audio Defaults, Subtitles, Appearance (Theme), Notifications, Queue Cleanup, and Advanced FFmpeg Engine diagnostics.

### 3.2 Global Keyboard Shortcuts
- **`Esc`**: Closes active modal (`FormatModal`, `MultiUrlModal`, `PlaylistPromptModal`, `PlaylistModal`) in reverse order of presentation without triggering background operations.
- **`Ctrl+V` / `Cmd+V`**: Inspects system clipboard when not focused on an input element. If a valid URL is discovered, instantly triggers analysis.
- **`1` / `2` / `3`**: Switches between Queue, History, and Settings views when no modal is open and user is not actively typing in an input field.

---

## 4. Screen-by-Screen Architectural Implementation

### 4.1 Home Screen & Hero Input Bar (`Header.tsx`)
- High-visibility central input with keyboard icon reminder (`Ctrl+V`).
- Integrated clipboard detection banner with non-destructive `Paste & Analyze` and `Dismiss` actions.
- Multi-URL batch entry point button (`Add Multiple URLs`).
- Real-time sidecar/system FFmpeg engine status indicator with colored diagnostic pill.

### 4.2 Queue Engine & Active Downloads (`QueueItemCard.tsx`, `QueueSummaryBar.tsx`)
- **QueueSummaryBar**:
  - Global throughput speed counter (MB/s).
  - Aggregate progress bar with smooth transition timing.
  - Concurrency selector directly bound to `update_settings(max_concurrent_downloads)`.
  - Batch action cluster: `Start All`, `Pause All`, `Resume All`, `Cancel All`, `Clear Completed`.
- **QueueItemCard**:
  - Real-time progress bar responding to decoupled atomic backend events.
  - Progress metrics: Transferred size / Total size, Speed, and dynamic remaining time (ETA).
  - Real-time lifecycle state badges: `Downloading` (Indigo), `Queued` (Slate), `Paused` (Amber), `Processing / Remuxing` (Violet), `Completed` (Emerald), `Failed` (Rose), `Cancelled` (Neutral).
  - Contextual action buttons: Pause, Resume, Cancel, Retry, Move Up, Move Down, Open File, Open Folder, and Remove.

### 4.3 Format Selection Modal (`FormatModal.tsx`)
- Categorized tab switcher: **Video Formats** vs. **Audio Only**.
- **Video Options**:
  - Quality labels (`1080p`, `720p`, `480p`, `360p`), resolution, framerate, video codec, audio codec, and estimated file size.
  - Remux badge for adaptive elementary stream merging.
  - Subtitle integration with multi-track selection and exclusive **Embed (Soft)** vs. **Hardcode (Burn-in)** modes.
- **Audio Options**:
  - Direct output format pills: `MP3`, `M4A`, `FLAC`, `Opus`.
  - Bitrate quality presets (`Best: 320k`, `High: 256k`, `Medium: 192k`, `Low: 128k`, `Custom`).
  - Metadata tag embedding and thumbnail cover art embedding checkboxes.
  - Custom filename templating with clickable presets (`{artist} - {title}`, `{title}`, `{title} ({id})`).

### 4.4 Multi-URL & Playlist Interfaces (`MultiUrlModal.tsx`, `PlaylistModal.tsx`)
- Batch URL text area with clipboard import, empty line filtering, and URL deduplication.
- Asynchronous batch analysis with bounded concurrency (3 concurrent queries).
- Global quality preset applicator across all validated links.
- Batch addition summary card showing enqueued, duplicate, unavailable, and failed counts.
- Full playlist explorer with track numbers, durations, thumbnail previews, and availability flags (`private`, `deleted`, `unavailable`).

### 4.5 History & Analytics (`HistoryView.tsx`)
- Four primary KPI cards: Total Downloads, Completed Count, Total Transferred Volume, and Average Download Speed.
- Quick filter chips: `All`, `Video`, `Audio`, `Completed`, `Failed`.
- Full-text search across titles and source URLs with clear button.
- Comprehensive data export: RFC-4180 CSV export (CRLF line endings, double-quote escaping) and JSON export.
- Row-level actions: Open File in default player, Open Folder in File Explorer, Retry download, and Delete record (with optional physical file deletion).

### 4.6 Settings & Preferences (`SettingsView.tsx`)
- Direct visual mapping to `AppSettings` entity:
  - **General**: Default Download Directory with directory picker.
  - **Downloads & Queue**: Max Concurrency (1–5), Default Container (`MP4`, `MKV`, `WebM`), Speed Limit (Unlimited or Custom KB/s / MB/s).
  - **Audio Defaults**: Default Audio Format, Bitrate Quality, Cover Artwork Embedding.
  - **Subtitles**: Default Subtitle Mode (`None`, `Embed`, `Hardcode`).
  - **Appearance & Cleanup**: Theme Switcher (`System`, `Dark`, `Light`), Completion Notification toggle, Queue Auto-clear toggle.
  - **Advanced**: Custom FFmpeg Binary Path with real-time `Check Status` validation.
  - **Controls**: `Reset to Defaults` and `Save Settings` with auto-dismissing toast feedback.

---

## 5. Accessibility, Motion & Responsive Design

1. **Accessibility (WCAG AA)**:
   - All text combinations exceed a 4.5:1 contrast ratio against their respective surface backgrounds.
   - Interactive elements feature explicit `focus:ring-2 focus:ring-emerald-500/40` focus indicators for full keyboard navigability.
   - Buttons and interactive cards maintain a minimum 36px touch/click target size.
2. **Reduced Motion**:
   - Micro-interactions (hover transitions, tab indicators, spinners) observe CSS transition times between 150ms and 200ms.
   - Smooth progress updates leverage GPU-accelerated CSS `transition: width 200ms ease-out` rather than high-frequency React rerenders.
   - Respects user OS `prefers-reduced-motion` settings.
3. **Desktop Viewport Responsiveness**:
   - Fully optimized for resolutions ranging from minimum 800×600 up to 4K displays.
   - Flex and grid layouts ensure zero horizontal scrollbars, zero overlapping buttons, and zero clipped text strings.

---

## 6. Architecture & Backend Invariant Preservation

The Modern UI/UX redesign strictly conforms to the **Release Architecture Freeze**:
- **Zero Backend Changes**: Backend Rust logic, models, extractors, database schemas, and IPC endpoints remain 100% frozen.
- **Invariants A1–A12 (Audio)**: Validated codecs, cancellation process termination, filename template sanitization, and FLAC lossless preservation are strictly intact.
- **Invariants S1–S3 (Subtitles)**: Tri-state mode exclusivity, single-track hardcoding limit, and filtergraph path escaping remain guaranteed.
- **Production Contracts**: All 39 Tauri IPC commands and 7 real-time event channels remain unmodified.
