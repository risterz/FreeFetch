/**
 * Container and codec compatibility utility.
 *
 * Enforces strict container multiplexing invariants:
 * - WebM Video: VP8, VP9, AV1
 * - WebM Audio: Opus, Vorbis
 * - WebM Subtitles: WebVTT
 * - MP4: H.264, H.265/HEVC, AV1, VP9; AAC, MP3, Opus, ALAC, AC3
 * - MKV: Universal container accepting all standard codecs
 */

export function normalizeVideoCodec(vcodec?: string | null): string {
  if (!vcodec) return 'none';
  const lower = vcodec.trim().toLowerCase();
  if (!lower || lower === 'none' || lower === 'null') return 'none';
  if (lower.startsWith('vp09') || lower === 'vp9') return 'vp9';
  if (lower.startsWith('vp08') || lower === 'vp8') return 'vp8';
  if (lower.startsWith('av01') || lower === 'av1') return 'av1';
  if (lower.startsWith('avc1') || lower.startsWith('avc3') || lower === 'h264' || lower === 'x264') return 'h264';
  if (lower.startsWith('hev1') || lower.startsWith('hvc1') || lower === 'h265' || lower === 'hevc' || lower === 'x265') return 'hevc';
  return lower.split('.')[0].trim();
}

export function normalizeAudioCodec(acodec?: string | null): string {
  if (!acodec) return 'none';
  const lower = acodec.trim().toLowerCase();
  if (!lower || lower === 'none' || lower === 'null') return 'none';
  if (lower.startsWith('mp4a') || lower === 'aac') return 'aac';
  if (lower === 'opus' || lower.startsWith('opus')) return 'opus';
  if (lower === 'vorbis' || lower.startsWith('vorbis')) return 'vorbis';
  if (lower === 'mp3' || lower.startsWith('mp3')) return 'mp3';
  if (lower === 'flac' || lower.startsWith('flac')) return 'flac';
  if (lower === 'alac' || lower.startsWith('alac')) return 'alac';
  if (lower === 'ac3' || lower === 'ac-3') return 'ac3';
  if (lower === 'eac3' || lower === 'ec-3') return 'eac3';
  return lower.split('.')[0].trim();
}

export function normalizeContainer(container: string): string {
  return container.trim().replace(/^\./, '').toLowerCase();
}

export function isVideoCodecCompatible(container: string, vcodec?: string | null): boolean {
  const c = normalizeContainer(container);
  const v = normalizeVideoCodec(vcodec);
  if (v === 'none' || v === 'auto') return true;

  if (c === 'webm') {
    return v === 'vp8' || v === 'vp9' || v === 'av1';
  }
  if (c === 'mp4' || c === 'm4v' || c === 'mov') {
    return v === 'h264' || v === 'hevc' || v === 'av1' || v === 'vp9';
  }
  return true; // MKV accepts all
}

export function isAudioCodecCompatible(container: string, acodec?: string | null): boolean {
  const c = normalizeContainer(container);
  const a = normalizeAudioCodec(acodec);
  if (a === 'none' || a === 'auto') return true;

  if (c === 'webm') {
    return a === 'opus' || a === 'vorbis';
  }
  if (c === 'mp4' || c === 'm4v' || c === 'mov') {
    return a === 'aac' || a === 'mp3' || a === 'opus' || a === 'alac' || a === 'ac3' || a === 'eac3';
  }
  if (c === 'mp3') return a === 'mp3';
  if (c === 'm4a') return a === 'aac' || a === 'alac';
  if (c === 'flac') return a === 'flac';
  if (c === 'opus') return a === 'opus';
  return true; // MKV accepts all
}

export function isSubtitleCodecCompatible(
  container: string,
  subMode?: string | null,
  subFormat?: string | null
): boolean {
  const mode = (subMode || '').trim().toLowerCase();
  if (mode === 'none' || mode === 'hardcode') return true;
  if (mode !== 'embed') return true;

  const c = normalizeContainer(container);
  const s = (subFormat || '').trim().replace(/^\./, '').toLowerCase();

  if (c === 'webm') {
    return !s || s === 'vtt' || s === 'webvtt';
  }
  return true;
}

export interface StreamValidationResult {
  valid: boolean;
  error?: string;
}

export function validateStreamCombination(
  container: string,
  vcodec?: string | null,
  acodec?: string | null,
  subMode?: string | null,
  subFormat?: string | null
): StreamValidationResult {
  const c = normalizeContainer(container);

  if (vcodec) {
    const normV = normalizeVideoCodec(vcodec);
    if (normV !== 'none' && normV !== 'auto' && !isVideoCodecCompatible(c, normV)) {
      return {
        valid: false,
        error: `Video codec '${vcodec}' (${normV}) is not supported in ${c.toUpperCase()} container. WebM requires VP8, VP9, or AV1 video.`,
      };
    }
  }

  if (acodec) {
    const normA = normalizeAudioCodec(acodec);
    if (normA !== 'none' && normA !== 'auto' && !isAudioCodecCompatible(c, normA)) {
      return {
        valid: false,
        error: `Audio codec '${acodec}' (${normA}) is not supported in ${c.toUpperCase()} container. WebM requires Opus or Vorbis audio.`,
      };
    }
  }

  if (subMode && !isSubtitleCodecCompatible(c, subMode, subFormat)) {
    return {
      valid: false,
      error: `Subtitle format '${subFormat || 'unknown'}' cannot be embedded directly into ${c.toUpperCase()} container. WebM only supports WebVTT.`,
    };
  }

  return { valid: true };
}
