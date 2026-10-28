import React, { useState, useEffect } from 'react';
import {
  Folder,
  Gauge,
  Layers,
  Music,
  Subtitles,
  Sun,
  Moon,
  Monitor,
  Bell,
  Trash2,
  Terminal,
  RotateCcw,
  Save,
  CheckCircle2,
  AlertCircle,
  FolderOpen,
} from 'lucide-react';
import { AppSettings, YtDlpDiagnostics } from '../../api/types';
import { FreeFetchApi } from '../../api/ipc';

interface SettingsViewProps {
  settings: AppSettings;
  onSettingsSaved: (newSettings: AppSettings) => void;
  onNavigateToQueue: () => void;
}

export const SettingsView: React.FC<SettingsViewProps> = ({
  settings,
  onSettingsSaved,
  onNavigateToQueue,
}) => {
  const [formData, setFormData] = useState<AppSettings>(settings);
  const [speedLimitType, setSpeedLimitType] = useState<'unlimited' | 'custom'>(
    settings.speed_limit_bytes_per_sec && settings.speed_limit_bytes_per_sec > 0
      ? 'custom'
      : 'unlimited'
  );
  const [speedValue, setSpeedValue] = useState<number>(() => {
    if (!settings.speed_limit_bytes_per_sec || settings.speed_limit_bytes_per_sec === 0) return 5;
    if (settings.speed_limit_bytes_per_sec >= 1_048_576) {
      return Math.round(settings.speed_limit_bytes_per_sec / 1_048_576);
    }
    return Math.round(settings.speed_limit_bytes_per_sec / 1024);
  });
  const [speedUnit, setSpeedUnit] = useState<'KB' | 'MB'>(() => {
    if (
      settings.speed_limit_bytes_per_sec &&
      settings.speed_limit_bytes_per_sec >= 1_048_576
    ) {
      return 'MB';
    }
    return 'KB';
  });

  const [ffmpegStatusText, setFfmpegStatusText] = useState<string | null>(null);
  const [isCheckingFfmpeg, setIsCheckingFfmpeg] = useState(false);
  const [ytdlpStatusText, setYtdlpStatusText] = useState<string | null>(null);
  const [isCheckingYtdlp, setIsCheckingYtdlp] = useState(false);
  const [diagnostics, setDiagnostics] = useState<YtDlpDiagnostics | null>(null);
  const [isDiagnosing, setIsDiagnosing] = useState(false);
  const [isUpdatingYtdlp, setIsUpdatingYtdlp] = useState(false);
  const [updateMessage, setUpdateMessage] = useState<string | null>(null);
  const [saveStatus, setSaveStatus] = useState<{ type: 'success' | 'error'; message: string } | null>(
    null
  );
  const [isSaving, setIsSaving] = useState(false);

  useEffect(() => {
    setFormData(settings);
    setSpeedLimitType(
      settings.speed_limit_bytes_per_sec && settings.speed_limit_bytes_per_sec > 0
        ? 'custom'
        : 'unlimited'
    );
  }, [settings]);

  // Check FFmpeg status
  const handleCheckFfmpeg = async () => {
    setIsCheckingFfmpeg(true);
    try {
      const status = await FreeFetchApi.checkFfmpegStatus();
      setFfmpegStatusText(`Ready (${status})`);
    } catch {
      setFfmpegStatusText('FFmpeg executable not found or invalid');
    } finally {
      setIsCheckingFfmpeg(false);
    }
  };

  // Check yt-dlp status
  const handleCheckYtDlp = async () => {
    setIsCheckingYtdlp(true);
    try {
      const status = await FreeFetchApi.checkYtDlpStatus();
      setYtdlpStatusText(`Ready (${status})`);
    } catch {
      setYtdlpStatusText('yt-dlp executable not found or invalid');
    } finally {
      setIsCheckingYtdlp(false);
    }
  };

  const handleDiagnoseYtDlp = async () => {
    setIsDiagnosing(true);
    try {
      const diag = await FreeFetchApi.diagnoseYtDlp();
      setDiagnostics(diag);
    } catch (err: unknown) {
      setDiagnostics({
        executable_path: 'Unknown',
        version: 'Error',
        has_impersonation: false,
        available_impersonate_targets: [],
        status: `Failed: ${err instanceof Error ? err.message : String(err)}`,
        recommendations: ['Check whether yt-dlp is installed and in PATH.'],
      });
    } finally {
      setIsDiagnosing(false);
    }
  };

  const handleUpdateYtDlp = async () => {
    setIsUpdatingYtdlp(true);
    setUpdateMessage(null);
    try {
      const res = await FreeFetchApi.updateYtDlp();
      setUpdateMessage(res || 'yt-dlp update process finished');
    } catch (err: unknown) {
      setUpdateMessage(`Update failed: ${err instanceof Error ? err.message : String(err)}`);
    } finally {
      setIsUpdatingYtdlp(false);
    }
  };

  const handleOpenFolder = async () => {
    try {
      await FreeFetchApi.openContainingFolder(formData.download_directory);
    } catch (err) {
      console.error('Failed to open folder:', err);
    }
  };

  const handleSave = async (e?: React.FormEvent) => {
    if (e) e.preventDefault();
    setIsSaving(true);
    setSaveStatus(null);

    try {
      // Calculate speed limit bytes
      let calculatedSpeedLimit: number | null = null;
      if (speedLimitType === 'custom') {
        const factor = speedUnit === 'MB' ? 1_048_576 : 1024;
        calculatedSpeedLimit = Math.max(0, Math.round(speedValue * factor));
      }

      const toSave: AppSettings = {
        ...formData,
        speed_limit_bytes_per_sec: calculatedSpeedLimit,
      };

      await FreeFetchApi.updateSettings(toSave);
      onSettingsSaved(toSave);
      setSaveStatus({ type: 'success', message: 'Settings saved successfully' });
      setTimeout(() => setSaveStatus(null), 3000);
    } catch (err: unknown) {
      const message =
        typeof err === 'object' && err !== null && 'message' in err
          ? (err as { message: string }).message
          : String(err);
      setSaveStatus({ type: 'error', message: `Failed to save: ${message}` });
    } finally {
      setIsSaving(false);
    }
  };

  const handleResetToDefaults = async () => {
    try {
      const defaults: AppSettings = {
        download_directory: settings.download_directory, // Preserve platform directory
        max_concurrent_downloads: 3,
        speed_limit_bytes_per_sec: null,
        ffmpeg_path: null,
        ytdlp_path: null,
        default_container: 'mp4',
        default_audio_format: 'mp3',
        default_audio_quality: 'high',
        embed_artwork: true,
        default_subtitle_mode: 'none',
        theme: 'system',
        notifications_enabled: true,
        auto_clear_completed: false,
      };

      await FreeFetchApi.updateSettings(defaults);
      setFormData(defaults);
      setSpeedLimitType('unlimited');
      onSettingsSaved(defaults);
      setSaveStatus({ type: 'success', message: 'Reset all settings to defaults' });
      setTimeout(() => setSaveStatus(null), 3000);
    } catch (err) {
      setSaveStatus({ type: 'error', message: `Reset failed: ${String(err)}` });
    }
  };

  return (
    <div className="flex-1 flex flex-col p-6 max-w-4xl mx-auto w-full">
      {/* Header */}
      <div className="flex items-center justify-between pb-5 border-b border-slate-200 dark:border-slate-800 mb-6">
        <div>
          <h2 className="text-xl font-bold text-slate-900 dark:text-white tracking-tight">Settings & Preferences</h2>
          <p className="text-sm text-slate-500 dark:text-slate-400">
            Configure application defaults, transfer limits, themes, and media processing options
          </p>
        </div>
        <button
          type="button"
          onClick={onNavigateToQueue}
          className="px-3.5 py-1.5 text-xs font-semibold rounded-lg bg-white hover:bg-slate-100 text-slate-700 border border-slate-300 dark:bg-slate-800 dark:hover:bg-slate-700 dark:text-slate-300 dark:border-slate-700 transition-colors shadow-xs"
        >
          Back to Queue
        </button>
      </div>

      {saveStatus && (
        <div
          className={`flex items-center gap-2 p-3.5 rounded-xl text-sm mb-6 ${
            saveStatus.type === 'success'
              ? 'bg-emerald-50 dark:bg-emerald-950/60 border border-emerald-200 dark:border-emerald-500/30 text-emerald-800 dark:text-emerald-300'
              : 'bg-rose-50 dark:bg-rose-950/60 border border-rose-200 dark:border-rose-500/30 text-rose-800 dark:text-rose-300'
          }`}
        >
          {saveStatus.type === 'success' ? (
            <CheckCircle2 className="w-4 h-4 text-emerald-600 dark:text-emerald-400 shrink-0" />
          ) : (
            <AlertCircle className="w-4 h-4 text-rose-600 dark:text-rose-400 shrink-0" />
          )}
          <span>{saveStatus.message}</span>
        </div>
      )}

      <form onSubmit={handleSave} className="space-y-6">
        {/* Section 1: General */}
        <div className="bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded-2xl p-5 shadow-xs space-y-4">
          <div className="flex items-center gap-2.5 text-slate-900 dark:text-slate-200 font-semibold text-sm">
            <Folder className="w-4 h-4 text-emerald-600 dark:text-emerald-400" />
            <span>General</span>
          </div>

          <div>
            <label className="block text-xs font-medium text-slate-600 dark:text-slate-400 mb-1.5">
              Default Download Location
            </label>
            <div className="flex gap-2">
              <input
                type="text"
                value={formData.download_directory}
                onChange={(e) =>
                  setFormData({ ...formData, download_directory: e.target.value })
                }
                placeholder="Folder path (e.g. C:\Downloads\FreeFetch)"
                className="flex-1 bg-slate-50 dark:bg-slate-800 border border-slate-300 dark:border-slate-700 rounded-xl px-3.5 py-2 text-sm text-slate-900 dark:text-slate-100 placeholder:text-slate-400 dark:placeholder:text-slate-500 focus:outline-none focus:bg-white dark:focus:bg-slate-800 focus:ring-2 focus:ring-emerald-500/40"
              />
              <button
                type="button"
                onClick={handleOpenFolder}
                className="flex items-center gap-1.5 px-3.5 py-2 bg-white hover:bg-slate-100 text-slate-700 border border-slate-300 dark:bg-slate-800 dark:hover:bg-slate-700 dark:text-slate-200 dark:border-slate-700 rounded-xl text-xs font-medium transition-colors shadow-xs"
                title="Open download folder in File Explorer"
              >
                <FolderOpen className="w-3.5 h-3.5 text-slate-500 dark:text-slate-400" />
                <span>Open Folder</span>
              </button>
            </div>
            <p className="text-[11px] text-slate-500 mt-1">
              Newly added downloads without explicit paths will save to this directory.
            </p>
          </div>
        </div>

        {/* Section 2: Downloads & Queue */}
        <div className="bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded-2xl p-5 shadow-xs space-y-4">
          <div className="flex items-center gap-2.5 text-slate-900 dark:text-slate-200 font-semibold text-sm">
            <Layers className="w-4 h-4 text-emerald-600 dark:text-emerald-400" />
            <span>Downloads & Queue</span>
          </div>

          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            {/* Concurrency */}
            <div>
              <label className="block text-xs font-medium text-slate-600 dark:text-slate-400 mb-1.5">
                Maximum Concurrent Downloads
              </label>
              <div className="flex gap-1.5">
                {[1, 2, 3, 4, 5].map((num) => (
                  <button
                    key={num}
                    type="button"
                    onClick={() =>
                      setFormData({ ...formData, max_concurrent_downloads: num })
                    }
                    className={`flex-1 py-1.5 text-xs font-semibold rounded-lg border transition-all ${
                      formData.max_concurrent_downloads === num
                        ? 'bg-emerald-600 border-emerald-500 text-white shadow-xs'
                        : 'bg-slate-50 hover:bg-slate-100 text-slate-700 border-slate-200 dark:bg-slate-800 dark:border-slate-700 dark:text-slate-300 dark:hover:bg-slate-750'
                    }`}
                  >
                    {num}
                  </button>
                ))}
              </div>
              <p className="text-[11px] text-slate-500 mt-1">
                Controls parallel active queue slots (1 to 5).
              </p>
            </div>

            {/* Default Video Container */}
            <div>
              <label className="block text-xs font-medium text-slate-600 dark:text-slate-400 mb-1.5">
                Default Video Format
              </label>
              <div className="flex gap-2">
                {['mp4', 'mkv', 'webm'].map((fmt) => (
                  <button
                    key={fmt}
                    type="button"
                    onClick={() => setFormData({ ...formData, default_container: fmt })}
                    className={`flex-1 py-1.5 text-xs font-semibold uppercase rounded-lg border transition-all ${
                      formData.default_container.toLowerCase() === fmt
                        ? 'bg-emerald-600 border-emerald-500 text-white shadow-xs'
                        : 'bg-slate-50 hover:bg-slate-100 text-slate-700 border-slate-200 dark:bg-slate-800 dark:border-slate-700 dark:text-slate-300 dark:hover:bg-slate-750'
                    }`}
                  >
                    {fmt}
                  </button>
                ))}
              </div>
              <p className="text-[11px] text-slate-500 mt-1">
                Initial selection in format selection dialog.
              </p>
            </div>
          </div>

          {/* Speed Limit */}
          <div className="pt-2 border-t border-slate-200 dark:border-slate-800/80">
            <div className="flex items-center justify-between mb-1.5">
              <label className="text-xs font-medium text-slate-600 dark:text-slate-400 flex items-center gap-1.5">
                <Gauge className="w-3.5 h-3.5 text-slate-500 dark:text-slate-400" />
                <span>Download Speed Limit</span>
              </label>
              <div className="flex items-center gap-2">
                <button
                  type="button"
                  onClick={() => setSpeedLimitType('unlimited')}
                  className={`px-2.5 py-1 text-xs font-medium rounded-lg border transition-all ${
                    speedLimitType === 'unlimited'
                      ? 'bg-emerald-600 border-emerald-500 text-white shadow-xs'
                      : 'bg-slate-50 text-slate-600 border-slate-200 dark:bg-slate-800 dark:border-slate-700 dark:text-slate-400'
                  }`}
                >
                  Unlimited
                </button>
                <button
                  type="button"
                  onClick={() => setSpeedLimitType('custom')}
                  className={`px-2.5 py-1 text-xs font-medium rounded-lg border transition-all ${
                    speedLimitType === 'custom'
                      ? 'bg-emerald-600 border-emerald-500 text-white shadow-xs'
                      : 'bg-slate-50 text-slate-600 border-slate-200 dark:bg-slate-800 dark:border-slate-700 dark:text-slate-400'
                  }`}
                >
                  Custom
                </button>
              </div>
            </div>

            {speedLimitType === 'custom' && (
              <div className="flex items-center gap-2 mt-2">
                <input
                  type="number"
                  min="1"
                  max="10000"
                  value={speedValue}
                  onChange={(e) => setSpeedValue(Math.max(1, parseInt(e.target.value) || 1))}
                  className="w-32 bg-slate-50 dark:bg-slate-800 border border-slate-300 dark:border-slate-700 rounded-xl px-3 py-1.5 text-sm text-slate-900 dark:text-slate-100 focus:outline-none focus:bg-white dark:focus:bg-slate-800 focus:ring-2 focus:ring-emerald-500/40"
                />
                <select
                  value={speedUnit}
                  onChange={(e) => setSpeedUnit(e.target.value as 'KB' | 'MB')}
                  className="bg-slate-50 dark:bg-slate-800 border border-slate-300 dark:border-slate-700 rounded-xl px-3 py-1.5 text-sm text-slate-900 dark:text-slate-100 focus:outline-none focus:bg-white dark:focus:bg-slate-800 focus:ring-2 focus:ring-emerald-500/40"
                >
                  <option value="KB">KB/s</option>
                  <option value="MB">MB/s</option>
                </select>
                <span className="text-xs text-slate-500 dark:text-slate-400">
                  Total per-download transfer limit
                </span>
              </div>
            )}
          </div>
        </div>

        {/* Section 3: Audio Defaults */}
        <div className="bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded-2xl p-5 shadow-xs space-y-4">
          <div className="flex items-center gap-2.5 text-slate-900 dark:text-slate-200 font-semibold text-sm">
            <Music className="w-4 h-4 text-emerald-600 dark:text-emerald-400" />
            <span>Audio Defaults</span>
          </div>

          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            {/* Audio Format */}
            <div>
              <label className="block text-xs font-medium text-slate-600 dark:text-slate-400 mb-1.5">
                Default Audio Format
              </label>
              <div className="flex gap-1.5">
                {['mp3', 'm4a', 'flac', 'opus'].map((fmt) => (
                  <button
                    key={fmt}
                    type="button"
                    onClick={() => setFormData({ ...formData, default_audio_format: fmt })}
                    className={`flex-1 py-1.5 text-xs font-semibold uppercase rounded-lg border transition-all ${
                      (formData.default_audio_format || 'mp3').toLowerCase() === fmt
                        ? 'bg-emerald-600 border-emerald-500 text-white shadow-xs'
                        : 'bg-slate-50 hover:bg-slate-100 text-slate-700 border-slate-200 dark:bg-slate-800 dark:border-slate-700 dark:text-slate-300 dark:hover:bg-slate-750'
                    }`}
                  >
                    {fmt}
                  </button>
                ))}
              </div>
            </div>

            {/* Audio Quality */}
            <div>
              <label className="block text-xs font-medium text-slate-600 dark:text-slate-400 mb-1.5">
                Default Audio Bitrate / Quality
              </label>
              <div className="flex gap-1.5">
                {[
                  { id: 'low', label: 'Low (96k)' },
                  { id: 'medium', label: 'Med (160k)' },
                  { id: 'high', label: 'High (256k)' },
                  { id: 'best', label: 'Best (320k)' },
                ].map((q) => (
                  <button
                    key={q.id}
                    type="button"
                    onClick={() => setFormData({ ...formData, default_audio_quality: q.id })}
                    className={`flex-1 py-1.5 text-[11px] font-semibold rounded-lg border transition-all ${
                      (formData.default_audio_quality || 'high').toLowerCase() === q.id
                        ? 'bg-emerald-600 border-emerald-500 text-white shadow-xs'
                        : 'bg-slate-50 hover:bg-slate-100 text-slate-700 border-slate-200 dark:bg-slate-800 dark:border-slate-700 dark:text-slate-300 dark:hover:bg-slate-750'
                    }`}
                  >
                    {q.label}
                  </button>
                ))}
              </div>
            </div>
          </div>

          {/* Embed Artwork */}
          <div className="flex items-center justify-between pt-2 border-t border-slate-200 dark:border-slate-800/80">
            <div>
              <span className="text-xs font-medium text-slate-800 dark:text-slate-300 block">Embed Cover Artwork</span>
              <span className="text-[11px] text-slate-500 block">
                Embeds video thumbnail into MP3, M4A, and FLAC audio files (bypasses Opus per spec).
              </span>
            </div>
            <button
              type="button"
              onClick={() =>
                setFormData({
                  ...formData,
                  embed_artwork: !(formData.embed_artwork ?? true),
                })
              }
              className={`w-12 h-6 flex items-center rounded-full p-1 transition-colors ${
                (formData.embed_artwork ?? true) ? 'bg-emerald-600' : 'bg-slate-300 dark:bg-slate-700'
              }`}
            >
              <div
                className={`bg-white w-4 h-4 rounded-full shadow-md transform transition-transform ${
                  (formData.embed_artwork ?? true) ? 'translate-x-6' : 'translate-x-0'
                }`}
              />
            </button>
          </div>
        </div>

        {/* Section 4: Subtitles */}
        <div className="bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded-2xl p-5 shadow-xs space-y-4">
          <div className="flex items-center gap-2.5 text-slate-900 dark:text-slate-200 font-semibold text-sm">
            <Subtitles className="w-4 h-4 text-emerald-600 dark:text-emerald-400" />
            <span>Subtitle Defaults</span>
          </div>

          <div>
            <label className="block text-xs font-medium text-slate-600 dark:text-slate-400 mb-1.5">
              Default Subtitle Processing Mode
            </label>
            <div className="grid grid-cols-1 sm:grid-cols-3 gap-2">
              {[
                { id: 'none', title: 'None', desc: 'No subtitle embedding' },
                { id: 'embed', title: 'Embed (Soft)', desc: 'Lossless remux, selectable track' },
                { id: 'hardcode', title: 'Hardcode (Burn)', desc: 'Permanently burned into frames' },
              ].map((m) => (
                <button
                  key={m.id}
                  type="button"
                  onClick={() => setFormData({ ...formData, default_subtitle_mode: m.id })}
                  className={`p-3 text-left rounded-xl border transition-all ${
                    (formData.default_subtitle_mode || 'none').toLowerCase() === m.id
                      ? 'bg-emerald-50 dark:bg-emerald-950/40 border-emerald-500 text-emerald-900 dark:text-white shadow-xs'
                      : 'bg-slate-50 hover:bg-slate-100 text-slate-700 border-slate-200 dark:bg-slate-800 dark:border-slate-700 dark:text-slate-300 dark:hover:bg-slate-750'
                  }`}
                >
                  <div className="font-semibold text-xs">{m.title}</div>
                  <div className="text-[11px] text-slate-500 dark:text-slate-400 mt-0.5">{m.desc}</div>
                </button>
              ))}
            </div>
            <p className="text-[11px] text-slate-500 mt-2">
              Pre-selected mode when downloading subtitles. Can be customized per download in the format dialog.
            </p>
          </div>
        </div>

        {/* Section 5: Appearance, Notifications & Cleanup */}
        <div className="bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded-2xl p-5 shadow-xs space-y-4">
          <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
            {/* Theme */}
            <div>
              <label className="text-xs font-medium text-slate-600 dark:text-slate-400 mb-2 flex items-center gap-1.5">
                <Sun className="w-3.5 h-3.5 text-amber-500" />
                <span>Theme</span>
              </label>
              <div className="flex gap-1.5">
                {[
                  { id: 'system', icon: Monitor, label: 'System' },
                  { id: 'dark', icon: Moon, label: 'Dark' },
                  { id: 'light', icon: Sun, label: 'Light' },
                ].map((t) => {
                  const Icon = t.icon;
                  return (
                    <button
                      key={t.id}
                      type="button"
                      onClick={() => setFormData({ ...formData, theme: t.id })}
                      className={`flex-1 py-2 px-1 text-xs font-medium rounded-xl border flex flex-col items-center gap-1 transition-all ${
                        (formData.theme || 'system').toLowerCase() === t.id
                          ? 'bg-emerald-600 border-emerald-500 text-white shadow-xs'
                          : 'bg-slate-50 border-slate-200 text-slate-600 hover:text-slate-900 dark:bg-slate-800 dark:border-slate-700 dark:text-slate-400 dark:hover:text-slate-200'
                      }`}
                    >
                      <Icon className="w-3.5 h-3.5" />
                      <span>{t.label}</span>
                    </button>
                  );
                })}
              </div>
            </div>

            {/* Notifications */}
            <div>
              <label className="text-xs font-medium text-slate-600 dark:text-slate-400 mb-2 flex items-center gap-1.5">
                <Bell className="w-3.5 h-3.5 text-blue-500" />
                <span>Notifications</span>
              </label>
              <div className="flex items-center justify-between p-2.5 bg-slate-50 dark:bg-slate-800 border border-slate-200 dark:border-slate-700 rounded-xl">
                <span className="text-xs text-slate-700 dark:text-slate-300">Completion Alerts</span>
                <button
                  type="button"
                  onClick={() =>
                    setFormData({
                      ...formData,
                      notifications_enabled: !(formData.notifications_enabled ?? true),
                    })
                  }
                  className={`w-11 h-6 flex items-center rounded-full p-1 transition-colors ${
                    (formData.notifications_enabled ?? true) ? 'bg-emerald-600' : 'bg-slate-300 dark:bg-slate-700'
                  }`}
                >
                  <div
                    className={`bg-white w-4 h-4 rounded-full shadow transform transition-transform ${
                      (formData.notifications_enabled ?? true)
                        ? 'translate-x-5'
                        : 'translate-x-0'
                    }`}
                  />
                </button>
              </div>
            </div>

            {/* Auto-Clear Queue */}
            <div>
              <label className="text-xs font-medium text-slate-600 dark:text-slate-400 mb-2 flex items-center gap-1.5">
                <Trash2 className="w-3.5 h-3.5 text-purple-500" />
                <span>Queue Cleanup</span>
              </label>
              <div className="flex items-center justify-between p-2.5 bg-slate-50 dark:bg-slate-800 border border-slate-200 dark:border-slate-700 rounded-xl">
                <span className="text-xs text-slate-700 dark:text-slate-300">Auto-Clear Queue</span>
                <button
                  type="button"
                  onClick={() =>
                    setFormData({
                      ...formData,
                      auto_clear_completed: !(formData.auto_clear_completed ?? false),
                    })
                  }
                  className={`w-11 h-6 flex items-center rounded-full p-1 transition-colors ${
                    formData.auto_clear_completed ? 'bg-emerald-600' : 'bg-slate-300 dark:bg-slate-700'
                  }`}
                >
                  <div
                    className={`bg-white w-4 h-4 rounded-full shadow transform transition-transform ${
                      formData.auto_clear_completed ? 'translate-x-5' : 'translate-x-0'
                    }`}
                  />
                </button>
              </div>
            </div>
          </div>
          <p className="text-[11px] text-slate-500">
            Auto-Clear Queue removes finished downloads from the active queue display. Full download audit records are always preserved in Download History.
          </p>
        </div>

        {/* Section 6: Advanced (FFmpeg Engine) */}
        <div className="bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded-2xl p-5 shadow-xs space-y-4">
          <div className="flex items-center gap-2.5 text-slate-900 dark:text-slate-200 font-semibold text-sm">
            <Terminal className="w-4 h-4 text-emerald-600 dark:text-emerald-400" />
            <span>Advanced (FFmpeg Engine)</span>
          </div>

          <div>
            <label className="block text-xs font-medium text-slate-600 dark:text-slate-400 mb-1.5">
              Custom FFmpeg Binary Path (Optional)
            </label>
            <div className="flex gap-2">
              <input
                type="text"
                value={formData.ffmpeg_path || ''}
                onChange={(e) =>
                  setFormData({
                    ...formData,
                    ffmpeg_path: e.target.value.trim() ? e.target.value : null,
                  })
                }
                placeholder="Leave blank to use bundled sidecar or system FFmpeg"
                className="flex-1 bg-slate-50 dark:bg-slate-800 border border-slate-300 dark:border-slate-700 rounded-xl px-3.5 py-2 text-sm text-slate-900 dark:text-slate-100 placeholder:text-slate-400 dark:placeholder:text-slate-500 focus:outline-none focus:bg-white dark:focus:bg-slate-800 focus:ring-2 focus:ring-emerald-500/40"
              />
              <button
                type="button"
                onClick={handleCheckFfmpeg}
                disabled={isCheckingFfmpeg}
                className="px-3.5 py-2 bg-white hover:bg-slate-100 text-slate-700 border border-slate-300 dark:bg-slate-800 dark:hover:bg-slate-700 dark:text-slate-200 dark:border-slate-700 rounded-xl text-xs font-medium transition-colors shadow-xs"
              >
                {isCheckingFfmpeg ? 'Checking...' : 'Check Status'}
              </button>
            </div>
            {ffmpegStatusText && (
              <p
                className={`text-xs mt-2 font-medium ${
                  ffmpegStatusText.startsWith('Ready')
                    ? 'text-emerald-600 dark:text-emerald-400'
                    : 'text-amber-600 dark:text-amber-400'
                }`}
              >
                {ffmpegStatusText}
              </p>
            )}
          </div>

          <div>
            <label className="block text-xs font-medium text-slate-600 dark:text-slate-400 mb-1.5">
              Custom yt-dlp Binary Path (Optional)
            </label>
            <div className="flex flex-wrap gap-2">
              <input
                type="text"
                value={formData.ytdlp_path || ''}
                onChange={(e) =>
                  setFormData({
                    ...formData,
                    ytdlp_path: e.target.value.trim() ? e.target.value : null,
                  })
                }
                placeholder="Leave blank to use system PATH or bundled yt-dlp"
                className="flex-1 min-w-[200px] bg-slate-50 dark:bg-slate-800 border border-slate-300 dark:border-slate-700 rounded-xl px-3.5 py-2 text-sm text-slate-900 dark:text-slate-100 placeholder:text-slate-400 dark:placeholder:text-slate-500 focus:outline-none focus:bg-white dark:focus:bg-slate-800 focus:ring-2 focus:ring-emerald-500/40"
              />
              <button
                type="button"
                onClick={handleCheckYtDlp}
                disabled={isCheckingYtdlp}
                className="px-3 py-2 bg-white hover:bg-slate-100 text-slate-700 border border-slate-300 dark:bg-slate-800 dark:hover:bg-slate-700 dark:text-slate-200 dark:border-slate-700 rounded-xl text-xs font-medium transition-colors shadow-xs"
              >
                {isCheckingYtdlp ? 'Checking...' : 'Check Status'}
              </button>
              <button
                type="button"
                onClick={handleDiagnoseYtDlp}
                disabled={isDiagnosing}
                className="px-3 py-2 bg-white hover:bg-slate-100 text-slate-700 border border-slate-300 dark:bg-slate-800 dark:hover:bg-slate-700 dark:text-slate-200 dark:border-slate-700 rounded-xl text-xs font-medium transition-colors shadow-xs"
              >
                {isDiagnosing ? 'Diagnosing...' : 'Run Diagnostics'}
              </button>
              <button
                type="button"
                onClick={handleUpdateYtDlp}
                disabled={isUpdatingYtdlp}
                className="px-3 py-2 bg-white hover:bg-slate-100 text-slate-700 border border-slate-300 dark:bg-slate-800 dark:hover:bg-slate-700 dark:text-slate-200 dark:border-slate-700 rounded-xl text-xs font-medium transition-colors shadow-xs"
              >
                {isUpdatingYtdlp ? 'Updating...' : 'Update yt-dlp'}
              </button>
            </div>
            {ytdlpStatusText && (
              <p
                className={`text-xs mt-2 font-medium ${
                  ytdlpStatusText.startsWith('Ready')
                    ? 'text-emerald-600 dark:text-emerald-400'
                    : 'text-amber-600 dark:text-amber-400'
                }`}
              >
                {ytdlpStatusText}
              </p>
            )}
            {updateMessage && (
              <p className="text-xs mt-2 text-slate-600 dark:text-slate-300 bg-slate-100 dark:bg-slate-800/80 p-2.5 rounded-lg font-mono">
                {updateMessage}
              </p>
            )}
            {diagnostics && (
              <div className="mt-3 p-3 bg-slate-50 dark:bg-slate-800/60 border border-slate-200 dark:border-slate-700/60 rounded-xl text-xs space-y-2">
                <div className="flex items-center justify-between">
                  <span className="font-semibold text-slate-700 dark:text-slate-300">yt-dlp Diagnostics</span>
                  <span className={`px-2 py-0.5 rounded-full font-medium ${
                    diagnostics.has_impersonation
                      ? 'bg-emerald-100 text-emerald-700 dark:bg-emerald-950/60 dark:text-emerald-400'
                      : 'bg-amber-100 text-amber-700 dark:bg-amber-950/60 dark:text-amber-400'
                  }`}>
                    {diagnostics.has_impersonation ? 'TLS Impersonation Active' : 'Impersonation Missing'}
                  </span>
                </div>
                <div className="text-slate-600 dark:text-slate-400 space-y-1">
                  <div><span className="font-medium">Version:</span> {diagnostics.version}</div>
                  <div className="truncate"><span className="font-medium">Binary:</span> {diagnostics.executable_path}</div>
                  <div><span className="font-medium">Status:</span> {diagnostics.status}</div>
                </div>
                {diagnostics.recommendations.length > 0 && (
                  <div className="mt-2 pt-2 border-t border-slate-200 dark:border-slate-700 space-y-1">
                    <span className="font-medium text-amber-600 dark:text-amber-400">Recommendations:</span>
                    {diagnostics.recommendations.map((rec, idx) => (
                      <p key={idx} className="text-slate-600 dark:text-slate-400 leading-relaxed">
                        • {rec}
                      </p>
                    ))}
                  </div>
                )}
              </div>
            )}
          </div>
        </div>

        {/* Action Buttons */}
        <div className="flex items-center justify-between pt-4 pb-12">
          <button
            type="button"
            onClick={handleResetToDefaults}
            className="flex items-center gap-2 px-4 py-2.5 bg-white hover:bg-slate-100 text-slate-600 hover:text-slate-900 border border-slate-300 dark:bg-slate-900 dark:hover:bg-slate-800 dark:text-slate-400 dark:hover:text-slate-200 dark:border-slate-800 rounded-xl text-xs font-semibold transition-colors shadow-xs"
          >
            <RotateCcw className="w-3.5 h-3.5" />
            <span>Reset to Defaults</span>
          </button>

          <button
            type="submit"
            disabled={isSaving}
            className="flex items-center gap-2 px-6 py-2.5 bg-emerald-600 hover:bg-emerald-500 text-white rounded-xl text-sm font-semibold transition-all shadow-md shadow-emerald-600/20 disabled:opacity-50"
          >
            <Save className="w-4 h-4" />
            <span>{isSaving ? 'Saving...' : 'Save Settings'}</span>
          </button>
        </div>
      </form>
    </div>
  );
};
