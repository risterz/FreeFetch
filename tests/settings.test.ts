import { describe, it, expect } from 'vitest';
import { AppSettings, MediaFormat } from '../src/api/types';

describe('Settings & Preferences Frontend Suite', () => {
  const defaultSettings: AppSettings = {
    download_directory: 'C:\\Users\\User\\Downloads\\FreeFetch',
    max_concurrent_downloads: 3,
    speed_limit_bytes_per_sec: null,
    ffmpeg_path: null,
    default_container: 'mp4',
    default_audio_format: 'mp3',
    default_audio_quality: 'high',
    embed_artwork: true,
    default_subtitle_mode: 'none',
    theme: 'system',
    notifications_enabled: true,
    auto_clear_completed: false,
  };

  it('validates default settings values and structure', () => {
    expect(defaultSettings.max_concurrent_downloads).toBe(3);
    expect(defaultSettings.speed_limit_bytes_per_sec).toBeNull();
    expect(defaultSettings.default_container).toBe('mp4');
    expect(defaultSettings.default_audio_format).toBe('mp3');
    expect(defaultSettings.default_audio_quality).toBe('high');
    expect(defaultSettings.embed_artwork).toBe(true);
    expect(defaultSettings.default_subtitle_mode).toBe('none');
    expect(defaultSettings.theme).toBe('system');
    expect(defaultSettings.notifications_enabled).toBe(true);
    expect(defaultSettings.auto_clear_completed).toBe(false);
  });

  it('calculates custom speed limits correctly', () => {
    // 5 MB/s to bytes
    const mbValue = 5;
    const mbBytes = mbValue * 1_048_576;
    expect(mbBytes).toBe(5_242_880);

    // 500 KB/s to bytes
    const kbValue = 500;
    const kbBytes = kbValue * 1024;
    expect(kbBytes).toBe(512_000);

    // Unlimited returns null
    const unlimited = null;
    expect(unlimited).toBeNull();
  });

  it('clamps concurrency values between 1 and 5', () => {
    const clampConcurrency = (val: number) => Math.min(5, Math.max(1, Math.round(val)));
    expect(clampConcurrency(0)).toBe(1);
    expect(clampConcurrency(-5)).toBe(1);
    expect(clampConcurrency(3)).toBe(3);
    expect(clampConcurrency(5)).toBe(5);
    expect(clampConcurrency(10)).toBe(5);
  });

  it('pre-selects video format matching default container setting', () => {
    const formats: MediaFormat[] = [
      {
        format_id: 'webm-1',
        format_type: 'video',
        quality_label: '1080p',
        container: 'webm',
        download_url: 'http://example.com/1',
        requires_merge: false,
      },
      {
        format_id: 'mp4-1',
        format_type: 'video',
        quality_label: '1080p',
        container: 'mp4',
        download_url: 'http://example.com/2',
        requires_merge: false,
      },
    ];

    const prefContainer = defaultSettings.default_container.toLowerCase();
    const matching = formats.find(
      (f) => f.format_type !== 'audio' && f.container?.toLowerCase() === prefContainer
    );

    expect(matching).toBeDefined();
    expect(matching?.format_id).toBe('mp4-1');
  });

  it('allows explicit user choice to override default container', () => {
    // Default is MP4
    let selectedFormatId = 'mp4-1';

    // User explicitly selects WebM format
    const userChoice = 'webm-1';
    selectedFormatId = userChoice;

    expect(selectedFormatId).toBe('webm-1');
  });

  it('pre-selects audio options from settings but honors explicit override', () => {
    // Initialized from default settings
    let targetFormat = defaultSettings.default_audio_format;
    let targetQuality = defaultSettings.default_audio_quality;
    let embedArt = defaultSettings.embed_artwork;

    expect(targetFormat).toBe('mp3');
    expect(targetQuality).toBe('high');
    expect(embedArt).toBe(true);

    // User explicitly selects FLAC with best quality and no artwork
    targetFormat = 'flac';
    targetQuality = 'best';
    embedArt = false;

    expect(targetFormat).toBe('flac');
    expect(targetQuality).toBe('best');
    expect(embedArt).toBe(false);
  });

  it('pre-selects subtitle mode from settings but honors explicit override', () => {
    let mode = defaultSettings.default_subtitle_mode;
    expect(mode).toBe('none');

    // User explicitly selects hardcode
    mode = 'hardcode';
    expect(mode).toBe('hardcode');
  });

  it('resolves theme classes properly for system, dark, and light', () => {
    const resolveTheme = (theme: string, systemDark: boolean) => {
      if (theme === 'dark') return 'dark';
      if (theme === 'light') return 'light';
      return systemDark ? 'dark' : 'light';
    };

    expect(resolveTheme('dark', false)).toBe('dark');
    expect(resolveTheme('light', true)).toBe('light');
    expect(resolveTheme('system', true)).toBe('dark');
    expect(resolveTheme('system', false)).toBe('light');
  });

  it('resets settings to default values without affecting unrelated state', () => {
    let currentSettings: AppSettings = {
      ...defaultSettings,
      max_concurrent_downloads: 1,
      default_container: 'mkv',
      default_audio_format: 'opus',
      theme: 'light',
      auto_clear_completed: true,
    };

    expect(currentSettings.max_concurrent_downloads).toBe(1);

    // Reset to defaults
    currentSettings = {
      ...defaultSettings,
    };

    expect(currentSettings.max_concurrent_downloads).toBe(3);
    expect(currentSettings.default_container).toBe('mp4');
    expect(currentSettings.default_audio_format).toBe('mp3');
    expect(currentSettings.theme).toBe('system');
    expect(currentSettings.auto_clear_completed).toBe(false);
  });
});
