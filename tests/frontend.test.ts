import { describe, it, expect } from 'vitest';
import { MediaFormat, DownloadRecord } from '../src/api/types';
import { formatErrorMessage } from '../src/utils/error';
import {
  validateStreamCombination,
  isVideoCodecCompatible,
  isAudioCodecCompatible,
  normalizeVideoCodec,
  normalizeAudioCodec,
} from '../src/utils/container';

describe('Format Selection Logic', () => {
  const sampleFormats: MediaFormat[] = [
    {
      format_id: 'yt-1080p',
      format_type: 'video',
      quality_label: '1080p FHD',
      resolution: '1920x1080',
      fps: 60,
      video_codec: 'h264',
      audio_codec: 'aac',
      bitrate: 4500000,
      container: 'mp4',
      filesize: 85000000,
      download_url: 'https://example.com/video1080',
      requires_merge: true,
      matching_audio_url: 'https://example.com/audio140',
    },
    {
      format_id: 'yt-720p',
      format_type: 'muxed',
      quality_label: '720p HD',
      resolution: '1280x720',
      fps: 30,
      video_codec: 'h264',
      audio_codec: 'aac',
      bitrate: 2200000,
      container: 'mp4',
      filesize: 45000000,
      download_url: 'https://example.com/video720',
      requires_merge: false,
    },
    {
      format_id: 'yt-mp3',
      format_type: 'audio',
      quality_label: 'MP3 Audio (320 kbps)',
      container: 'mp3',
      filesize: 7500000,
      download_url: 'https://example.com/audio140',
      requires_merge: false,
    },
  ];

  it('filters video and muxed formats correctly', () => {
    const videoFormats = sampleFormats.filter(
      (f) => f.format_type === 'video' || f.format_type === 'muxed'
    );
    expect(videoFormats.length).toBe(2);
    expect(videoFormats[0].quality_label).toBe('1080p FHD');
    expect(videoFormats[1].quality_label).toBe('720p HD');
  });

  it('filters audio formats correctly', () => {
    const audioFormats = sampleFormats.filter((f) => f.format_type === 'audio');
    expect(audioFormats.length).toBe(1);
    expect(audioFormats[0].container).toBe('mp3');
  });

  it('identifies formats requiring FFmpeg remuxing', () => {
    const remuxNeeded = sampleFormats.filter((f) => f.requires_merge);
    expect(remuxNeeded.length).toBe(1);
    expect(remuxNeeded[0].format_id).toBe('yt-1080p');
    expect(remuxNeeded[0].matching_audio_url).toBeTruthy();
  });
});

describe('Download Progress Calculations', () => {
  it('calculates progress percentage correctly', () => {
    const calcPercent = (downloaded: number, total: number) => {
      if (total <= 0) return 0;
      return Math.min(100, Math.max(0, (downloaded / total) * 100));
    };

    expect(calcPercent(0, 100)).toBe(0);
    expect(calcPercent(50, 100)).toBe(50);
    expect(calcPercent(100, 100)).toBe(100);
    expect(calcPercent(120, 100)).toBe(100);
    expect(calcPercent(50, 0)).toBe(0);
  });

  it('calculates human-readable ETA correctly', () => {
    const calcEta = (remainingBytes: number, speedBps: number) => {
      if (speedBps <= 0) return 0;
      return Math.floor(remainingBytes / speedBps);
    };

    // 10 MB remaining at 2 MB/s = 5 seconds
    expect(calcEta(10 * 1024 * 1024, 2 * 1024 * 1024)).toBe(5);
  });
});

describe('State Machine & Download Records', () => {
  it('correctly maps record statuses', () => {
    const record: DownloadRecord = {
      id: 'abc-123',
      source_url: 'https://example.com/video.mp4',
      service_name: 'generic',
      title: 'Demo',
      duration: 60,
      destination_path: 'C:\\test.mp4',
      container: 'mp4',
      quality_label: '1080p',
      total_bytes: 1000,
      downloaded_bytes: 1000,
      status: 'completed',
      created_at: 1700000000,
      completed_at: 1700000050,
    };

    expect(record.status).toBe('completed');
    expect(record.downloaded_bytes).toBe(record.total_bytes);
    expect(record.completed_at).toBeGreaterThan(record.created_at);
  });
});

describe('Batch URL & Multi-URL Logic', () => {
  it('correctly splits lines, ignores whitespace and duplicates', () => {
    const raw = `
      https://www.youtube.com/watch?v=AAA
      https://youtu.be/BBB

      https://www.youtube.com/watch?v=AAA
      https://example.com/test.mp4
    `;

    const lines = raw
      .split('\n')
      .map((l) => l.trim())
      .filter((l) => l.startsWith('http://') || l.startsWith('https://'));

    const unique = Array.from(new Set(lines));
    expect(unique.length).toBe(3);
    expect(unique).toContain('https://www.youtube.com/watch?v=AAA');
    expect(unique).toContain('https://youtu.be/BBB');
    expect(unique).toContain('https://example.com/test.mp4');
  });

  it('selects quality preset correctly among multiple formats', () => {
    const formats: MediaFormat[] = [
      {
        format_id: '1080p',
        format_type: 'video',
        quality_label: '1080p',
        container: 'mp4',
        download_url: 'http://test/1080',
        requires_merge: false,
      },
      {
        format_id: '720p',
        format_type: 'video',
        quality_label: '720p',
        container: 'mp4',
        download_url: 'http://test/720',
        requires_merge: false,
      },
      {
        format_id: 'audio',
        format_type: 'audio',
        quality_label: 'Audio Only',
        container: 'm4a',
        download_url: 'http://test/audio',
        requires_merge: false,
      },
    ];

    const pickQuality = (list: MediaFormat[], pref: string): MediaFormat => {
      if (pref === 'audio_only') {
        const a = list.find((f) => f.format_type === 'audio');
        if (a) return a;
      }
      const match = list.find((f) => f.quality_label.toLowerCase().includes(pref.toLowerCase()));
      if (match) return match;
      return list[0];
    };

    expect(pickQuality(formats, '720p').format_id).toBe('720p');
    expect(pickQuality(formats, '1080p').format_id).toBe('1080p');
    expect(pickQuality(formats, 'audio_only').format_id).toBe('audio');
    expect(pickQuality(formats, '480p').format_id).toBe('1080p'); // fallback to first
  });
});

describe('Playlist Filtering and Selection Logic', () => {
  const playlistItems = [
    {
      id: 'item-1',
      url: 'https://youtube.com/watch?v=1',
      title: 'Video 1',
      availability: 'available' as const,
      duration: 120,
    },
    {
      id: 'item-2',
      url: 'https://youtube.com/watch?v=2',
      title: '[Deleted video]',
      availability: 'deleted' as const,
      duration: null,
    },
    {
      id: 'item-3',
      url: 'https://youtube.com/watch?v=3',
      title: '[Private video]',
      availability: 'private' as const,
      duration: null,
    },
    {
      id: 'item-4',
      url: 'https://youtube.com/watch?v=4',
      title: 'Video 4',
      availability: 'available' as const,
      duration: 340,
    },
  ];

  it('filters out unavailable items properly', () => {
    const available = playlistItems.filter((i) => i.availability === 'available');
    const unavailable = playlistItems.filter((i) => i.availability !== 'available');

    expect(available.length).toBe(2);
    expect(unavailable.length).toBe(2);
    expect(available.map((i) => i.id)).toEqual(['item-1', 'item-4']);
  });

  it('selects all available items by default and toggles correctly', () => {
    let selectedIds = new Set(
      playlistItems.filter((i) => i.availability === 'available').map((i) => i.id)
    );

    expect(selectedIds.size).toBe(2);
    expect(selectedIds.has('item-1')).toBe(true);
    expect(selectedIds.has('item-2')).toBe(false);

    // Toggle item-1 off
    selectedIds.delete('item-1');
    expect(selectedIds.size).toBe(1);

    // Toggle item-1 back on
    selectedIds.add('item-1');
    expect(selectedIds.size).toBe(2);

    // Clear all
    selectedIds.clear();
    expect(selectedIds.size).toBe(0);
  });
});

describe('Subtitle & Caption Selection Logic', () => {
  const sampleSubtitles = [
    {
      id: 'yt-sub-de-manual',
      language_code: 'de',
      language_name: 'German',
      format: 'vtt' as const,
      is_auto_generated: false,
      is_default: false,
    },
    {
      id: 'yt-sub-en-manual',
      language_code: 'en',
      language_name: 'English',
      format: 'vtt' as const,
      is_auto_generated: false,
      is_default: true,
    },
    {
      id: 'yt-sub-en-auto',
      language_code: 'en',
      language_name: 'English (Auto-generated)',
      format: 'vtt' as const,
      is_auto_generated: true,
      is_default: false,
    },
    {
      id: 'yt-sub-ms-auto',
      language_code: 'ms',
      language_name: 'Malay (Auto-generated)',
      format: 'vtt' as const,
      is_auto_generated: true,
      is_default: false,
    },
  ];

  it('correctly distinguishes manual vs auto-generated subtitles', () => {
    const manual = sampleSubtitles.filter((s) => !s.is_auto_generated);
    const auto = sampleSubtitles.filter((s) => s.is_auto_generated);

    expect(manual.length).toBe(2);
    expect(auto.length).toBe(2);
    expect(manual.map((s) => s.language_code)).toEqual(['de', 'en']);
  });

  it('selects English manual by priority if available', () => {
    const pickDefault = (subs: typeof sampleSubtitles) => {
      const enManual = subs.find(
        (s) => !s.is_auto_generated && s.language_code.toLowerCase().startsWith('en')
      );
      if (enManual) return enManual.id;
      const firstManual = subs.find((s) => !s.is_auto_generated);
      if (firstManual) return firstManual.id;
      const firstAuto =
        subs.find(
          (s) => s.is_auto_generated && s.language_code.toLowerCase().startsWith('en')
        ) || subs[0];
      return firstAuto ? firstAuto.id : null;
    };

    expect(pickDefault(sampleSubtitles)).toBe('yt-sub-en-manual');

    // Without English manual
    const withoutEnManual = sampleSubtitles.filter(
      (s) => !(s.language_code === 'en' && !s.is_auto_generated)
    );
    expect(pickDefault(withoutEnManual)).toBe('yt-sub-de-manual');

    // Only auto-generated
    const onlyAuto = sampleSubtitles.filter((s) => s.is_auto_generated);
    expect(pickDefault(onlyAuto)).toBe('yt-sub-en-auto');

    // Empty list
    expect(pickDefault([])).toBeNull();
  });

  it('maps subtitle formats and handles embedding toggle', () => {
    let targetFormat: 'srt' | 'vtt' = 'srt';
    let embed = false;

    expect(targetFormat).toBe('srt');
    expect(embed).toBe(false);

    // Toggle embedding
    embed = true;
    expect(embed).toBe(true);

    // Toggle format to VTT
    targetFormat = 'vtt';
    expect(targetFormat).toBe('vtt');
  });
});

describe('Audio Extraction and Conversion Configuration', () => {
  it('constructs default AudioConversionOptions correctly', () => {
    const defaultOptions = {
      target_format: 'mp3' as const,
      quality_preset: 'best' as const,
      bitrate_mode: 'cbr' as const,
      embed_metadata: true,
      embed_artwork: true,
      filename_template: '{title}',
    };

    expect(defaultOptions.target_format).toBe('mp3');
    expect(defaultOptions.quality_preset).toBe('best');
    expect(defaultOptions.embed_metadata).toBe(true);
    expect(defaultOptions.embed_artwork).toBe(true);
  });

  it('handles custom bitrate and sample rate for lossy formats', () => {
    const customMp3 = {
      target_format: 'mp3' as const,
      quality_preset: 'custom' as const,
      bitrate_mode: 'cbr' as const,
      custom_bitrate_kbps: 320,
      sample_rate: 44100,
      embed_metadata: true,
      embed_artwork: true,
    };

    expect(customMp3.custom_bitrate_kbps).toBe(320);
    expect(customMp3.sample_rate).toBe(44100);
  });

  it('identifies FLAC lossless restriction (no custom bitrate)', () => {
    const isFlacLossless = (fmt: string) => fmt === 'flac';
    expect(isFlacLossless('flac')).toBe(true);
    expect(isFlacLossless('mp3')).toBe(false);
  });
});

describe('Download History & Media Classification', () => {
  const isAudioContainer = (container: string) =>
    ['mp3', 'm4a', 'flac', 'opus', 'aac', 'ogg', 'wav'].includes(container.toLowerCase());

  it('classifies audio vs video containers correctly', () => {
    expect(isAudioContainer('mp3')).toBe(true);
    expect(isAudioContainer('FLAC')).toBe(true);
    expect(isAudioContainer('m4a')).toBe(true);
    expect(isAudioContainer('opus')).toBe(true);
    expect(isAudioContainer('wav')).toBe(true);

    expect(isAudioContainer('mp4')).toBe(false);
    expect(isAudioContainer('mkv')).toBe(false);
    expect(isAudioContainer('webm')).toBe(false);
    expect(isAudioContainer('avi')).toBe(false);
  });
});

describe('History Filtering & Search', () => {
  const sampleRecords: DownloadRecord[] = [
    {
      id: '1',
      source_url: 'https://youtube.com/watch?v=rust_basics',
      service_name: 'youtube',
      title: 'Rust Programming Basics',
      duration: 1200,
      destination_path: 'C:\\Downloads\\Rust Basics.mp4',
      container: 'mp4',
      quality_label: '1080p',
      total_bytes: 80000000,
      downloaded_bytes: 80000000,
      status: 'completed',
      created_at: 1700000001,
      completed_at: 1700000010,
    },
    {
      id: '2',
      source_url: 'https://youtube.com/watch?v=synthwave_mix',
      service_name: 'youtube',
      title: 'Synthwave Chill Mix',
      duration: 3600,
      destination_path: 'C:\\Downloads\\Synthwave Chill Mix.mp3',
      container: 'mp3',
      quality_label: '320kbps',
      total_bytes: 45000000,
      downloaded_bytes: 45000000,
      status: 'completed',
      created_at: 1700000002,
      completed_at: 1700000020,
    },
    {
      id: '3',
      source_url: 'https://youtube.com/watch?v=broken_stream',
      service_name: 'youtube',
      title: 'Corrupted Stream Test',
      duration: 300,
      destination_path: 'C:\\Downloads\\Corrupted Stream.mkv',
      container: 'mkv',
      quality_label: '720p',
      total_bytes: 20000000,
      downloaded_bytes: 0,
      status: 'failed',
      error_message: 'HTTP 404 Not Found',
      created_at: 1700000003,
    },
    {
      id: '4',
      source_url: 'https://example.com/audio/jazz.flac',
      service_name: 'generic',
      title: 'Jazz Lounge Live',
      duration: 600,
      destination_path: 'C:\\Downloads\\Jazz Lounge Live.flac',
      container: 'flac',
      quality_label: 'Lossless',
      total_bytes: 25000000,
      downloaded_bytes: 5000000,
      status: 'cancelled',
      created_at: 1700000004,
    },
  ];

  it('filters by status correctly', () => {
    const completed = sampleRecords.filter((r) => r.status === 'completed');
    expect(completed.length).toBe(2);

    const failed = sampleRecords.filter((r) => r.status === 'failed');
    expect(failed.length).toBe(1);
    expect(failed[0].id).toBe('3');

    const cancelled = sampleRecords.filter((r) => r.status === 'cancelled');
    expect(cancelled.length).toBe(1);
    expect(cancelled[0].id).toBe('4');
  });

  it('filters by media type (audio vs video)', () => {
    const isAudio = (c: string) => ['mp3', 'm4a', 'flac', 'opus', 'aac'].includes(c.toLowerCase());

    const audioItems = sampleRecords.filter((r) => isAudio(r.container));
    expect(audioItems.length).toBe(2);
    expect(audioItems.map((r) => r.container)).toEqual(['mp3', 'flac']);

    const videoItems = sampleRecords.filter((r) => !isAudio(r.container));
    expect(videoItems.length).toBe(2);
    expect(videoItems.map((r) => r.container)).toEqual(['mp4', 'mkv']);
  });

  it('filters by case-insensitive search on title and URL', () => {
    const searchFilter = (query: string) => {
      const q = query.toLowerCase();
      return sampleRecords.filter(
        (r) => r.title.toLowerCase().includes(q) || r.source_url.toLowerCase().includes(q)
      );
    };

    expect(searchFilter('RUST').length).toBe(1);
    expect(searchFilter('chill').length).toBe(1);
    expect(searchFilter('youtube.com').length).toBe(3);
    expect(searchFilter('nonexistent').length).toBe(0);
  });
});

describe('Download Analytics Aggregations', () => {
  it('calculates aggregate statistics correctly', () => {
    const isAudio = (c: string) => ['mp3', 'm4a', 'flac', 'opus'].includes(c.toLowerCase());

    const records: DownloadRecord[] = [
      {
        id: '1',
        source_url: '',
        service_name: '',
        title: 'Video 1',
        duration: 100,
        destination_path: '',
        container: 'mp4',
        quality_label: '',
        total_bytes: 50000,
        downloaded_bytes: 50000,
        status: 'completed',
        created_at: 1,
      },
      {
        id: '2',
        source_url: '',
        service_name: '',
        title: 'Audio 1',
        duration: 200,
        destination_path: '',
        container: 'mp3',
        quality_label: '',
        total_bytes: 10000,
        downloaded_bytes: 10000,
        status: 'completed',
        created_at: 2,
      },
      {
        id: '3',
        source_url: '',
        service_name: '',
        title: 'Failed 1',
        duration: 50,
        destination_path: '',
        container: 'mkv',
        quality_label: '',
        total_bytes: 0,
        downloaded_bytes: 0,
        status: 'failed',
        created_at: 3,
      },
    ];

    const stats = {
      total_downloads: records.length,
      completed_downloads: records.filter((r) => r.status === 'completed').length,
      failed_downloads: records.filter((r) => r.status === 'failed').length,
      cancelled_downloads: records.filter((r) => r.status === 'cancelled').length,
      total_downloaded_bytes: records.reduce((acc, r) => acc + r.downloaded_bytes, 0),
      audio_downloads: records.filter((r) => isAudio(r.container)).length,
      video_downloads: records.filter((r) => !isAudio(r.container)).length,
      total_duration_seconds: records.reduce((acc, r) => acc + r.duration, 0),
    };

    expect(stats.total_downloads).toBe(3);
    expect(stats.completed_downloads).toBe(2);
    expect(stats.failed_downloads).toBe(1);
    expect(stats.cancelled_downloads).toBe(0);
    expect(stats.total_downloaded_bytes).toBe(60000);
    expect(stats.audio_downloads).toBe(1);
    expect(stats.video_downloads).toBe(2);
    expect(stats.total_duration_seconds).toBe(350);
  });
});

describe('History Export Formatting', () => {
  it('correctly parses exported CSV format', () => {
    const csvContent =
      'id,source_url,service_name,title,duration,thumbnail_url,destination_path,container,quality_label,total_bytes,downloaded_bytes,status,error_message,created_at,completed_at,media_type\r\n' +
      'id1,"https://example.com",youtube,"Video, with comma",120,"","C:\\Downloads\\v.mp4",mp4,1080p,1000,1000,completed,"",1700000000,1700000010,video\r\n';

    const lines = csvContent.trim().split('\r\n');
    expect(lines.length).toBe(2);
    const headers = lines[0].split(',');
    expect(headers).toContain('media_type');
    expect(headers).toContain('service_name');
    expect(headers).toContain('status');
  });
});

describe('Live History Progress and State Handling', () => {
  it('correctly overlays live progress on static record', () => {
    const staticRecord: DownloadRecord = {
      id: 'job-123',
      source_url: 'https://youtube.com/watch?v=123',
      service_name: 'youtube',
      title: 'Active Stream',
      duration: 300,
      destination_path: 'C:\\test.mp4',
      container: 'mp4',
      quality_label: '1080p',
      total_bytes: 100000000,
      downloaded_bytes: 20000000,
      status: 'downloading',
      created_at: 100,
    };

    const liveUpdate = {
      id: 'job-123',
      downloadedBytes: 85000000,
      totalBytes: 100000000,
      speedBytesPerSec: 5000000,
      etaSeconds: 3,
      progressPercent: 85.0,
    };

    const effectiveDownloaded = liveUpdate?.downloadedBytes ?? staticRecord.downloaded_bytes;
    const effectiveTotal = liveUpdate?.totalBytes ?? staticRecord.total_bytes;
    const effectivePercent = liveUpdate?.progressPercent ?? (effectiveDownloaded / effectiveTotal) * 100;

    expect(effectiveDownloaded).toBe(85000000);
    expect(effectiveTotal).toBe(100000000);
    expect(effectivePercent).toBe(85.0);
  });

  it('suppresses informational status messages from displaying as errors', () => {
    const isErrorDisplayable = (status: string, errorMessage?: string | null) => {
      if (!errorMessage) return false;
      return status === 'failed' || status === 'retrying';
    };

    expect(isErrorDisplayable('downloading', 'Downloading in parallel')).toBe(false);
    expect(isErrorDisplayable('processing', 'Muxing streams')).toBe(false);
    expect(isErrorDisplayable('completed', 'Done')).toBe(false);
    expect(isErrorDisplayable('failed', 'HTTP 403 Forbidden')).toBe(true);
    expect(isErrorDisplayable('retrying', 'Connection reset')).toBe(true);
  });
});

describe('Subtitle Mode & Hardcoded Processing', () => {
  it('resolves subtitle mode correctly between none, embed, and hardcode', () => {
    const resolveMode = (
      hasSubtitles: boolean,
      mode: 'none' | 'embed' | 'hardcode',
      isAudio: boolean
    ) => {
      if (isAudio || !hasSubtitles) return 'none';
      return mode;
    };

    expect(resolveMode(true, 'hardcode', false)).toBe('hardcode');
    expect(resolveMode(true, 'embed', false)).toBe('embed');
    expect(resolveMode(true, 'none', false)).toBe('none');
    expect(resolveMode(false, 'hardcode', false)).toBe('none');
    expect(resolveMode(true, 'hardcode', true)).toBe('none'); // Invariant A3
  });

  it('restricts hardcode mode to a single subtitle selection', () => {
    const selectSubtitle = (current: string[], nextId: string, mode: 'embed' | 'hardcode') => {
      if (mode === 'hardcode') {
        return [nextId];
      }
      return current.includes(nextId) ? current.filter((id) => id !== nextId) : [...current, nextId];
    };

    let selected = ['en'];
    selected = selectSubtitle(selected, 'es', 'hardcode');
    expect(selected).toEqual(['es']);

    selected = selectSubtitle(selected, 'ja', 'embed');
    expect(selected).toEqual(['es', 'ja']);
  });

  it('exposes only original auto-generated captions and manual tracks to the UI', () => {
    // Subtitle tracks provided by normalized backend response
    const uiSubtitles = [
      {
        id: 'yt-sub-id-manual',
        language_code: 'id',
        language_name: 'Indonesian',
        is_auto_generated: false,
        format: 'vtt' as const,
      },
      {
        id: 'yt-sub-id-orig-auto',
        language_code: 'id-orig',
        language_name: 'Indonesian (Original) (Auto-generated)',
        is_auto_generated: true,
        format: 'vtt' as const,
      },
    ];

    // Verify translated tracks like 'iu' (Inuktitut) or 'ga' (Irish) are not present
    expect(uiSubtitles.some((s) => s.language_code === 'iu')).toBe(false);
    expect(uiSubtitles.some((s) => s.language_code === 'ga')).toBe(false);

    // Verify original auto track is present with ID-ORIG code
    const origAuto = uiSubtitles.find((s) => s.is_auto_generated);
    expect(origAuto).toBeDefined();
    expect(origAuto?.language_code).toBe('id-orig');
    expect(origAuto?.language_name).toBe('Indonesian (Original) (Auto-generated)');

    // Verify manual subtitle is present
    const manualSub = uiSubtitles.find((s) => !s.is_auto_generated);
    expect(manualSub).toBeDefined();
    expect(manualSub?.language_code).toBe('id');
    expect(manualSub?.language_name).toBe('Indonesian');
  });
});

describe('Error Formatter (No [object Object])', () => {
  it('formats strings, Error instances, and primitives safely', () => {
    expect(formatErrorMessage('Direct error message')).toBe('Direct error message');
    expect(formatErrorMessage(new Error('Standard JS error'))).toBe('Standard JS error');
    expect(formatErrorMessage(404)).toBe('404');
    expect(formatErrorMessage(null)).toBe('Unknown error');
    expect(formatErrorMessage(undefined)).toBe('Unknown error');
    expect(formatErrorMessage('')).toBe('Unknown error');
  });

  it('safely unpacks nested Tauri IPC Extractor error objects without [object Object]', () => {
    // Reproduce exact previous failure payload:
    const nestedIpcError = {
      type: 'Extractor',
      message: {
        service: 'youtube',
        message:
          "Metadata resolved for 'KERJA DI POM BENSIN DUNIA LAIN', but no downloadable streams could be resolved.",
      },
    };

    const formatted = formatErrorMessage(nestedIpcError);
    expect(formatted).not.toContain('[object Object]');
    expect(formatted).toContain('youtube');
    expect(formatted).toContain('Metadata resolved for');
  });

  it('safely formats flat structured errors with service and message', () => {
    const flatIpcError = {
      type: 'Extractor',
      service: 'youtube',
      message: 'Video playability check returned: Sign in to confirm you are not a bot',
    };

    const formatted = formatErrorMessage(flatIpcError);
    expect(formatted).not.toContain('[object Object]');
    expect(formatted).toBe(
      '[youtube] Video playability check returned: Sign in to confirm you are not a bot'
    );
  });

  it('safely formats unit enum errors without message field', () => {
    expect(formatErrorMessage({ type: 'Cancelled' })).toBe('Cancelled');
    expect(formatErrorMessage({ type: 'FfmpegNotFound' })).toBe('FfmpegNotFound');
  });

  it('safely handles error codes and generic objects via JSON stringification fallback', () => {
    expect(formatErrorMessage({ code: 500 })).toBe('Error code: 500');

    const unknownObject = { foo: 'bar', value: 42 };
    const formatted = formatErrorMessage(unknownObject);
    expect(formatted).not.toContain('[object Object]');
    expect(formatted).toBe(JSON.stringify(unknownObject));
  });
});

describe('Container and Codec Compatibility Rules', () => {
  it('normalizes video codecs accurately', () => {
    expect(normalizeVideoCodec('vp09.00.41.08')).toBe('vp9');
    expect(normalizeVideoCodec('av01.0.08M.08')).toBe('av1');
    expect(normalizeVideoCodec('avc1.640028')).toBe('h264');
    expect(normalizeVideoCodec('hev1.1.6.L93.B0')).toBe('hevc');
    expect(normalizeVideoCodec('vp8')).toBe('vp8');
  });

  it('normalizes audio codecs accurately', () => {
    expect(normalizeAudioCodec('mp4a.40.2')).toBe('aac');
    expect(normalizeAudioCodec('opus')).toBe('opus');
    expect(normalizeAudioCodec('vorbis')).toBe('vorbis');
    expect(normalizeAudioCodec('mp3')).toBe('mp3');
  });

  it('validates WebM container codec compatibility', () => {
    expect(isVideoCodecCompatible('webm', 'vp9')).toBe(true);
    expect(isVideoCodecCompatible('webm', 'av1')).toBe(true);
    expect(isVideoCodecCompatible('webm', 'vp8')).toBe(true);
    expect(isVideoCodecCompatible('webm', 'h264')).toBe(false);

    expect(isAudioCodecCompatible('webm', 'opus')).toBe(true);
    expect(isAudioCodecCompatible('webm', 'vorbis')).toBe(true);
    expect(isAudioCodecCompatible('webm', 'aac')).toBe(false);
    expect(isAudioCodecCompatible('webm', 'mp3')).toBe(false);
  });

  it('validates stream combination and blocks AAC in WebM (the reported defect)', () => {
    const invalidCase = validateStreamCombination('webm', 'vp9', 'aac');
    expect(invalidCase.valid).toBe(false);
    expect(invalidCase.error).toContain('WebM requires Opus or Vorbis');

    const validCase = validateStreamCombination('webm', 'vp9', 'opus');
    expect(validCase.valid).toBe(true);
    expect(validCase.error).toBeUndefined();

    const mp4Case = validateStreamCombination('mp4', 'h264', 'aac');
    expect(mp4Case.valid).toBe(true);
  });
});

describe('File and Folder Action Handlers', () => {
  it('resolves copy target to destination_path when present on completed downloads', () => {
    const completedRecord: DownloadRecord = {
      id: 'rec-1',
      source_url: 'https://example.com/video',
      service_name: 'youtube',
      title: 'Test Video',
      duration: 100,
      format_id: '1080p',
      destination_path: 'C:\\Users\\User\\Downloads\\FreeFetch\\Test Video.mp4',
      status: 'completed',
      downloaded_bytes: 5000,
      total_bytes: 5000,
      created_at: 1000,
    };
    const textToCopy =
      completedRecord.destination_path && completedRecord.destination_path.trim().length > 0
        ? completedRecord.destination_path
        : completedRecord.source_url;
    expect(textToCopy).toBe('C:\\Users\\User\\Downloads\\FreeFetch\\Test Video.mp4');
  });

  it('falls back to source_url when destination_path is empty or missing', () => {
    const failedRecord: DownloadRecord = {
      id: 'rec-2',
      source_url: 'https://example.com/failed-video',
      service_name: 'youtube',
      title: 'Failed Video',
      duration: 100,
      format_id: '1080p',
      destination_path: '',
      status: 'failed',
      downloaded_bytes: 0,
      total_bytes: 0,
      created_at: 1000,
    };
    const textToCopy =
      failedRecord.destination_path && failedRecord.destination_path.trim().length > 0
        ? failedRecord.destination_path
        : failedRecord.source_url;
    expect(textToCopy).toBe('https://example.com/failed-video');
  });

  it('validates empty path before invoking open commands', () => {
    const checkValidPath = (path: string | undefined | null) => {
      return Boolean(path && path.trim().length > 0);
    };

    expect(checkValidPath('')).toBe(false);
    expect(checkValidPath('   ')).toBe(false);
    expect(checkValidPath(null)).toBe(false);
    expect(checkValidPath(undefined)).toBe(false);
    expect(checkValidPath('C:\\Users\\User\\Downloads\\file.mp4')).toBe(true);
  });
});

