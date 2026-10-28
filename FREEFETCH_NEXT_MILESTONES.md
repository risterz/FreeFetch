# FreeFetch Roadmap & Next Milestones

**Document**: `FREEFETCH_NEXT_MILESTONES.md`  
**Current Milestone State**: Downloader Optimization & Adaptive Recovery Completed  
**Release Readiness**: Production Ready (Core Application Architecture Frozen)

---

## 1. Frozen Production Subsystems (DO NOT TOUCH)

The following core application subsystems have completed their production freeze cycles and must **NOT** be modified for further speculative performance tuning:

1. **Downloader Engine**:
   - `RangeWorkQueue` and work-stealing concurrency model.
   - 2 MiB dynamic sub-range partitioning for YouTube.
   - Monolithic partitioning for standard CDNs.
   - `StallWatchdog` and connection recovery algorithms.
   - Positional file writes (`seek_write`) and `.part.meta` crash recovery.
2. **Queue Architecture**:
   - Concurrency limits, priority queue, pause/resume, cancellation lifecycle.
3. **Audio Subsystem**:
   - Audio extraction, quality presets, FFmpeg transcoding, metadata tagging, artwork embedding.
4. **Subtitle Subsystem**:
   - Subtitle parsing, conversion (VTT to SRT), embedding, hardcoding, and failure isolation.
5. **FFmpeg Pipeline**:
   - Direct-vector execution, hidden console processes, stream multiplexing.
6. **Storage & History**:
   - SQLite schema, repository queries, analytics, schema migrations.
7. **Settings & Preferences**:
   - Persisted configuration defaults, user overrides, validation rules.
8. **IPC Contracts & Events**:
   - Tauri v2 IPC command signatures and frontend event channels.

---

## 2. Important Security & Legal Boundary: PO-Token Handling

FreeFetch adheres strictly to legal, clean-room, and safe software practices:

- **Access Controls & DRM**: FreeFetch MUST NOT attempt to bypass authentication, access controls, DRM, or platform integrity protections.
- **Evaluation Criteria**: Any future research into Proof-of-Origin (PO) tokens or mobile/TV Innertube clients must be evaluated strictly as a legitimate, sanctioned extraction capability (e.g., using official APIs, user-supplied cookies, or documented developer mechanisms).
- **Prohibition**: Botguard reverse-engineering, token-minting VMs, or browser exploit mechanisms will NOT be integrated into FreeFetch merely to evade YouTube's bandwidth pacing.

---

## 3. Future Milestone Candidates (Post-Freeze Research)

Future development cycles may explore the following areas without modifying the frozen core:

### Milestone A: Legitimate Extraction Resilience & Platform Expansion
- Support for additional media hosting platforms (SoundCloud, Vimeo, Dailymotion, Bandcamp).
- Generic open-graph fallback enhancements.
- Official API / OAuth token integration for user-authenticated media libraries.

### Milestone B: Desktop Packaging & Distribution
- Windows code signing configuration.
- NSIS installer polish and automated update verification.
- Portable release packaging and artifact integrity hashing.

### Milestone C: UI & User Experience Enhancements
- Visual format selection filtering (codec badges, audio quality tags).
- Download throughput graphing and worker activity visualization.
- Batch URL import ergonomics and clipboard auto-detection toggles.
