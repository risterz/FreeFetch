import React, { useEffect, useState, useCallback } from 'react';
import {
  DownloadRecord,
  DownloadProgressPayload,
  MediaFormat,
  ParseResult,
  QueueItem,
  QueueSummary,
  PlaylistInfo,
  PlaylistDetection,
  SubtitleSelection,
  SubtitleMode,
  AudioConversionOptions,
} from './api/types';
import { FreeFetchApi } from './api/ipc';
import { Header } from './components/layout/Header';
import { QueueSummaryBar } from './components/queue/QueueSummaryBar';
import { QueueItemCard } from './components/queue/QueueItemCard';
import { FormatModal } from './components/modal/FormatModal';
import { MultiUrlModal } from './components/modal/MultiUrlModal';
import { PlaylistModal } from './components/modal/PlaylistModal';
import { PlaylistPromptModal } from './components/modal/PlaylistPromptModal';
import { HistoryView } from './components/history/HistoryView';
import { SettingsView } from './components/settings/SettingsView';
import { AlertCircle, RefreshCw, ListOrdered, History, Settings } from 'lucide-react';
import { AppSettings } from './api/types';
import { formatErrorMessage } from './utils/error';

export const App: React.FC = () => {
  const [activeTab, setActiveTab] = useState<'queue' | 'history' | 'settings'>('queue');
  const [appSettings, setAppSettings] = useState<AppSettings>({
    download_directory: '',
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
  });
  const [queueItems, setQueueItems] = useState<QueueItem[]>([]);
  const [queueSummary, setQueueSummary] = useState<QueueSummary>({
    total_items: 0,
    queued_count: 0,
    downloading_count: 0,
    paused_count: 0,
    completed_count: 0,
    failed_count: 0,
    total_bytes: 0,
    downloaded_bytes: 0,
    overall_progress_percent: 0,
    total_speed_bytes_per_sec: 0,
  });
  const [concurrency, setConcurrency] = useState<number>(2);

  const [downloads, setDownloads] = useState<DownloadRecord[]>([]);
  const [liveProgress, setLiveProgress] = useState<Record<string, DownloadProgressPayload>>({});
  const [historyRefreshKey, setHistoryRefreshKey] = useState<number>(0);
  const [isAnalyzing, setIsAnalyzing] = useState(false);
  const [ffmpegStatus, setFfmpegStatus] = useState<string | null>(null);
  const [activeParseResult, setActiveParseResult] = useState<ParseResult | null>(null);
  const [isModalOpen, setIsModalOpen] = useState(false);
  const [globalError, setGlobalError] = useState<string | null>(null);
  const [clipboardDetectedUrl, setClipboardDetectedUrl] = useState<string | null>(null);

  // Multi-URL and Playlist state
  const [isMultiUrlOpen, setIsMultiUrlOpen] = useState(false);
  const [isPlaylistModalOpen, setIsPlaylistModalOpen] = useState(false);
  const [isPlaylistPromptOpen, setIsPlaylistPromptOpen] = useState(false);
  const [activePlaylist, setActivePlaylist] = useState<PlaylistInfo | null>(null);
  const [pendingPlaylistDetection, setPendingPlaylistDetection] = useState<PlaylistDetection | null>(null);
  const [pendingUrl, setPendingUrl] = useState<string | null>(null);

  // Load downloads from SQLite
  const loadDownloads = useCallback(async () => {
    try {
      const records = await FreeFetchApi.getDownloads();
      setDownloads(records);
    } catch (err) {
      console.error('Failed to load downloads:', err);
    }
  }, []);

  // Load Queue from SQLite
  const loadQueue = useCallback(async () => {
    try {
      const items = await FreeFetchApi.queueGet();
      setQueueItems(items);
      const summary = await FreeFetchApi.getQueueSummary();
      setQueueSummary(summary);
      const settings = await FreeFetchApi.getSettings();
      setAppSettings(settings);
      setConcurrency(settings.max_concurrent_downloads || 2);
    } catch (err) {
      console.error('Failed to load queue:', err);
    }
  }, []);

  // Apply theme dynamically
  useEffect(() => {
    const theme = (appSettings.theme || 'system').toLowerCase();
    const root = document.documentElement;
    const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');

    const applyTheme = () => {
      if (theme === 'dark') {
        root.classList.add('dark');
        root.classList.remove('light');
      } else if (theme === 'light') {
        root.classList.add('light');
        root.classList.remove('dark');
      } else {
        if (mediaQuery.matches) {
          root.classList.add('dark');
          root.classList.remove('light');
        } else {
          root.classList.add('light');
          root.classList.remove('dark');
        }
      }
    };

    applyTheme();
    if (theme === 'system') {
      mediaQuery.addEventListener('change', applyTheme);
      return () => mediaQuery.removeEventListener('change', applyTheme);
    }
  }, [appSettings.theme]);

  // Check FFmpeg status
  const checkFfmpeg = useCallback(async () => {
    try {
      const status = await FreeFetchApi.checkFfmpegStatus();
      setFfmpegStatus(status);
    } catch {
      setFfmpegStatus(null);
    }
  }, []);

  // Clipboard URL detection on window focus
  useEffect(() => {
    let lastChecked = '';
    const handleFocus = async () => {
      try {
        const text = await navigator.clipboard.readText();
        const trimmed = text?.trim();
        if (
          trimmed &&
          trimmed !== lastChecked &&
          (trimmed.startsWith('http://') || trimmed.startsWith('https://')) &&
          (trimmed.includes('youtube.com') ||
            trimmed.includes('youtu.be') ||
            trimmed.endsWith('.mp4') ||
            trimmed.endsWith('.webm') ||
            trimmed.endsWith('.mp3') ||
            trimmed.endsWith('.mkv'))
        ) {
          lastChecked = trimmed;
          setClipboardDetectedUrl(trimmed);
        }
      } catch {
        // Clipboard read permission might not be granted in background
      }
    };

    window.addEventListener('focus', handleFocus);
    return () => window.removeEventListener('focus', handleFocus);
  }, []);

  // Initial load and event listeners setup
  useEffect(() => {
    loadDownloads();
    loadQueue();
    checkFfmpeg();

    let unlistenProgress: (() => void) | undefined;
    let unlistenState: (() => void) | undefined;
    let unlistenQueueAdded: (() => void) | undefined;
    let unlistenQueueUpdated: (() => void) | undefined;
    let unlistenQueueRemoved: (() => void) | undefined;
    let unlistenQueueSummary: (() => void) | undefined;

    const setupListeners = async () => {
      try {
        unlistenProgress = await FreeFetchApi.onProgress((payload) => {
          setLiveProgress((prev) => ({
            ...prev,
            [payload.id]: payload,
          }));
        });

        unlistenState = await FreeFetchApi.onStateChanged((payload) => {
          setDownloads((prev) =>
            prev.map((d) =>
              d.id === payload.id
                ? {
                    ...d,
                    status: payload.status,
                    error_message: payload.errorMessage ?? d.error_message,
                  }
                : d
            )
          );
          if (payload.status === 'completed') {
            loadDownloads();
            loadQueue();
            if (
              appSettings.notifications_enabled &&
              'Notification' in window &&
              Notification.permission === 'granted'
            ) {
              try {
                new Notification('FreeFetch Download Complete', {
                  body: 'A download in queue has finished successfully.',
                });
              } catch (e) {
                console.warn('Failed to dispatch notification:', e);
              }
            }
            if (appSettings.auto_clear_completed) {
              FreeFetchApi.queueClearCompleted().then(() => loadQueue());
            }
          } else if (payload.status === 'failed') {
            loadDownloads();
            loadQueue();
          }
        });

        unlistenQueueAdded = await FreeFetchApi.onQueueItemAdded(() => {
          loadQueue();
        });

        unlistenQueueUpdated = await FreeFetchApi.onQueueUpdated((items) => {
          setQueueItems(items);
        });

        unlistenQueueRemoved = await FreeFetchApi.onQueueItemRemoved(() => {
          loadQueue();
        });

        unlistenQueueSummary = await FreeFetchApi.onQueueSummary((summary) => {
          setQueueSummary(summary);
        });
      } catch (e) {
        console.warn('Tauri event listeners not active (browser mock mode or unattached):', e);
      }
    };

    setupListeners();

    return () => {
      if (unlistenProgress) unlistenProgress();
      if (unlistenState) unlistenState();
      if (unlistenQueueAdded) unlistenQueueAdded();
      if (unlistenQueueUpdated) unlistenQueueUpdated();
      if (unlistenQueueRemoved) unlistenQueueRemoved();
      if (unlistenQueueSummary) unlistenQueueSummary();
    };
  }, [loadDownloads, loadQueue, checkFfmpeg]);

  // Handle URL analysis (supports single video, playlist, and watch-with-playlist)
  const handleAnalyze = useCallback(async (url: string) => {
    setIsAnalyzing(true);
    setGlobalError(null);
    try {
      // 1. Detect if URL is a playlist or watch URL with playlist parameter
      const detection = await FreeFetchApi.detectPlaylistUrl(url);
      if (detection) {
        if (detection.is_pure_playlist) {
          // Direct playlist view
          const pl = await FreeFetchApi.analyzePlaylist(url);
          setActivePlaylist(pl);
          setIsPlaylistModalOpen(true);
          return;
        } else if (detection.video_id) {
          // Video URL with &list=...: ask user intention
          setPendingPlaylistDetection(detection);
          setPendingUrl(url);
          setIsPlaylistPromptOpen(true);
          return;
        }
      }

      // 2. Regular single video analysis
      const result = await FreeFetchApi.parseUrl(url);
      setActiveParseResult(result);
      setIsModalOpen(true);
    } catch (err: unknown) {
      setGlobalError(`Analysis failed: ${formatErrorMessage(err)}`);
    } finally {
      setIsAnalyzing(false);
    }
  }, []);

  // Global Keyboard Shortcuts (Esc = close modals, Ctrl/Cmd+V = analyze clipboard, 1/2/3 = tabs)
  useEffect(() => {
    const handleKeyDown = async (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null;
      const isInput =
        target &&
        (target.tagName === 'INPUT' ||
          target.tagName === 'TEXTAREA' ||
          target.isContentEditable);

      // 1. Escape: Close open modals in priority order
      if (e.key === 'Escape') {
        if (isModalOpen) {
          setIsModalOpen(false);
          e.preventDefault();
        } else if (isMultiUrlOpen) {
          setIsMultiUrlOpen(false);
          e.preventDefault();
        } else if (isPlaylistPromptOpen) {
          setIsPlaylistPromptOpen(false);
          setPendingUrl(null);
          e.preventDefault();
        } else if (isPlaylistModalOpen) {
          setIsPlaylistModalOpen(false);
          setActivePlaylist(null);
          e.preventDefault();
        }
        return;
      }

      // Do not trigger global navigation/clipboard when focused in input/textarea
      if (isInput) return;

      // 2. Ctrl+V / Cmd+V anywhere: analyze clipboard link directly
      if ((e.ctrlKey || e.metaKey) && (e.key === 'v' || e.key === 'V')) {
        try {
          const text = await navigator.clipboard.readText();
          const trimmed = text?.trim();
          if (
            trimmed &&
            (trimmed.startsWith('http://') || trimmed.startsWith('https://'))
          ) {
            e.preventDefault();
            handleAnalyze(trimmed);
          }
        } catch {
          // Clipboard read access denied or unavailable
        }
        return;
      }

      // 3. Tab switching keys 1, 2, 3 (when not typing and no modals open)
      const hasOpenModal =
        isModalOpen || isMultiUrlOpen || isPlaylistPromptOpen || isPlaylistModalOpen;
      if (!hasOpenModal && !e.ctrlKey && !e.altKey && !e.metaKey) {
        if (e.key === '1') {
          setActiveTab('queue');
          e.preventDefault();
        } else if (e.key === '2') {
          setActiveTab('history');
          e.preventDefault();
        } else if (e.key === '3') {
          setActiveTab('settings');
          e.preventDefault();
        }
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [
    isModalOpen,
    isMultiUrlOpen,
    isPlaylistPromptOpen,
    isPlaylistModalOpen,
    handleAnalyze,
  ]);

  // Prompt choice: View entire playlist
  const handlePromptViewPlaylist = async () => {
    setIsPlaylistPromptOpen(false);
    if (!pendingUrl) return;
    setIsAnalyzing(true);
    setGlobalError(null);
    try {
      const pl = await FreeFetchApi.analyzePlaylist(pendingUrl);
      setActivePlaylist(pl);
      setIsPlaylistModalOpen(true);
    } catch (err: unknown) {
      setGlobalError(`Playlist discovery failed: ${formatErrorMessage(err)}`);
    } finally {
      setIsAnalyzing(false);
    }
  };

  // Prompt choice: Analyze single video
  const handlePromptAnalyzeSingle = async () => {
    setIsPlaylistPromptOpen(false);
    if (!pendingUrl) return;
    setIsAnalyzing(true);
    setGlobalError(null);
    try {
      const result = await FreeFetchApi.parseUrl(pendingUrl);
      setActiveParseResult(result);
      setIsModalOpen(true);
    } catch (err: unknown) {
      setGlobalError(`Analysis failed: ${formatErrorMessage(err)}`);
    } finally {
      setIsAnalyzing(false);
    }
  };

  // Confirm format selection and begin immediate download
  const handleConfirmDownload = async (
    selectedFormat: MediaFormat,
    subtitles: SubtitleSelection[],
    subtitleMode: SubtitleMode,
    audioOptions?: AudioConversionOptions
  ) => {
    if (!activeParseResult) return;
    setIsModalOpen(false);
    try {
      const container = audioOptions?.target_format || selectedFormat.container;
      const qualityLabel = audioOptions?.quality_preset
        ? `${audioOptions.quality_preset.toUpperCase()} (${(audioOptions.target_format || 'mp3').toUpperCase()})`
        : selectedFormat.quality_label;

      const effectiveMode: SubtitleMode = audioOptions ? 'none' : subtitleMode;

      await FreeFetchApi.startDownload({
        source_url: activeParseResult.metadata.url,
        service_name: activeParseResult.metadata.service_name,
        title: activeParseResult.metadata.title,
        duration: activeParseResult.metadata.duration,
        thumbnail_url: activeParseResult.metadata.thumbnail_url,
        format_id: selectedFormat.format_id,
        download_url: selectedFormat.download_url,
        requires_merge: audioOptions ? false : selectedFormat.requires_merge,
        matching_audio_url: audioOptions ? null : selectedFormat.matching_audio_url,
        container,
        quality_label: qualityLabel,
        destination_dir: null,
        selected_subtitles: audioOptions ? [] : subtitles,
        embed_subtitles: effectiveMode === 'embed',
        subtitle_mode: effectiveMode,
        audio_options: audioOptions || null,
        video_codec: selectedFormat.video_codec,
        audio_codec: selectedFormat.audio_codec,
      });

      // If user selected subtitles and NOT embedding/hardcoding into video, also enqueue subtitle jobs
      if (!audioOptions && effectiveMode === 'none' && subtitles.length > 0) {
        for (const sub of subtitles) {
          const subFormatId = `sub:${sub.language_code}:${sub.is_auto_generated ? 'auto:' : ''}${sub.target_format}`;
          const subTitle = `${activeParseResult.metadata.title}.${sub.language_code}${sub.is_auto_generated ? '.auto' : ''}`;
          try {
            await FreeFetchApi.queueAdd({
              source_url: activeParseResult.metadata.url,
              service_name: activeParseResult.metadata.service_name,
              title: subTitle,
              duration: activeParseResult.metadata.duration,
              thumbnail_url: activeParseResult.metadata.thumbnail_url,
              format_id: subFormatId,
              download_url: sub.download_url || '',
              requires_merge: false,
              matching_audio_url: null,
              container: sub.target_format,
              quality_label: sub.language_name,
              destination_dir: null,
              selected_subtitles: [],
              embed_subtitles: false,
              subtitle_mode: 'none',
              audio_options: null,
            });
          } catch (e) {
            console.warn('Failed to enqueue subtitle track:', e);
          }
        }
      }

      await loadDownloads();
      await loadQueue();
    } catch (err) {
      setGlobalError(`Failed to start download: ${formatErrorMessage(err)}`);
    }
  };

  // Add format to persistent queue
  const handleAddToQueue = async (
    selectedFormat: MediaFormat,
    subtitles: SubtitleSelection[],
    subtitleMode: SubtitleMode,
    audioOptions?: AudioConversionOptions
  ) => {
    if (!activeParseResult) return;
    setIsModalOpen(false);
    try {
      const container = audioOptions?.target_format || selectedFormat.container;
      const qualityLabel = audioOptions?.quality_preset
        ? `${audioOptions.quality_preset.toUpperCase()} (${(audioOptions.target_format || 'mp3').toUpperCase()})`
        : selectedFormat.quality_label;

      const effectiveMode: SubtitleMode = audioOptions ? 'none' : subtitleMode;

      await FreeFetchApi.queueAdd({
        source_url: activeParseResult.metadata.url,
        service_name: activeParseResult.metadata.service_name,
        title: activeParseResult.metadata.title,
        duration: activeParseResult.metadata.duration,
        thumbnail_url: activeParseResult.metadata.thumbnail_url,
        format_id: selectedFormat.format_id,
        download_url: selectedFormat.download_url,
        requires_merge: audioOptions ? false : selectedFormat.requires_merge,
        matching_audio_url: audioOptions ? null : selectedFormat.matching_audio_url,
        container,
        quality_label: qualityLabel,
        destination_dir: null,
        selected_subtitles: audioOptions ? [] : subtitles,
        embed_subtitles: effectiveMode === 'embed',
        subtitle_mode: effectiveMode,
        audio_options: audioOptions || null,
        video_codec: selectedFormat.video_codec,
        audio_codec: selectedFormat.audio_codec,
      });
      setActiveTab('queue');
      await loadQueue();
    } catch (err) {
      setGlobalError(`Failed to add to queue: ${formatErrorMessage(err)}`);
    }
  };

  // Queue Global Controls
  const handleQueueConcurrencyChange = async (newConcurrency: number) => {
    try {
      setConcurrency(newConcurrency);
      await FreeFetchApi.queueSetConcurrency(newConcurrency);
    } catch (e) {
      console.error(e);
    }
  };

  const handleQueueStartAll = async () => {
    try {
      await FreeFetchApi.queueStart();
      await loadQueue();
    } catch (e) {
      console.error(e);
    }
  };

  const handleQueuePauseAll = async () => {
    try {
      await FreeFetchApi.queuePause();
      await loadQueue();
    } catch (e) {
      console.error(e);
    }
  };

  const handleQueueResumeAll = async () => {
    try {
      await FreeFetchApi.queueResume();
      await loadQueue();
    } catch (e) {
      console.error(e);
    }
  };

  const handleQueueCancelAll = async () => {
    try {
      await FreeFetchApi.queueCancel();
      await loadQueue();
    } catch (e) {
      console.error(e);
    }
  };

  const handleQueueClearCompleted = async () => {
    try {
      await FreeFetchApi.queueClearCompleted();
      await loadQueue();
    } catch (e) {
      console.error(e);
    }
  };

  const handleQueueClearFailed = async () => {
    try {
      await FreeFetchApi.queueClearFailed();
      await loadQueue();
    } catch (e) {
      console.error(e);
    }
  };

  // Queue Item Actions
  const handleQueuePauseItem = async (id: string) => {
    try {
      await FreeFetchApi.queuePause(id);
      await loadQueue();
    } catch (e) {
      console.error(e);
    }
  };

  const handleQueueResumeItem = async (id: string) => {
    try {
      await FreeFetchApi.queueResume(id);
      await loadQueue();
    } catch (e) {
      console.error(e);
    }
  };

  const handleQueueCancelItem = async (id: string) => {
    try {
      await FreeFetchApi.queueCancel(id);
      await loadQueue();
    } catch (e) {
      console.error(e);
    }
  };

  const handleQueueRetryItem = async (id: string) => {
    try {
      await FreeFetchApi.queueRetry(id);
      await loadQueue();
    } catch (e) {
      console.error(e);
    }
  };

  const handleQueueRemoveItem = async (id: string) => {
    try {
      await FreeFetchApi.queueRemove(id);
      await loadQueue();
    } catch (e) {
      console.error(e);
    }
  };

  const handleQueueMoveUp = async (id: string) => {
    try {
      await FreeFetchApi.queueMoveUp(id);
      await loadQueue();
    } catch (e) {
      console.error(e);
    }
  };

  const handleQueueMoveDown = async (id: string) => {
    try {
      await FreeFetchApi.queueMoveDown(id);
      await loadQueue();
    } catch (e) {
      console.error(e);
    }
  };

  const handleOpenFile = async (path: string) => {
    try {
      await FreeFetchApi.openDownloadedFile(path);
    } catch (err) {
      setGlobalError(`Could not open file: ${formatErrorMessage(err)}`);
    }
  };

  const handleOpenFolder = async (path: string) => {
    try {
      await FreeFetchApi.openContainingFolder(path);
    } catch (err) {
      setGlobalError(`Could not open folder: ${formatErrorMessage(err)}`);
    }
  };

  return (
    <div className="min-h-screen bg-slate-950 flex flex-col">
      <Header
        onAnalyze={handleAnalyze}
        isAnalyzing={isAnalyzing}
        ffmpegStatus={ffmpegStatus}
        onOpenMultiUrl={() => setIsMultiUrlOpen(true)}
      />

      {clipboardDetectedUrl && (
        <div className="max-w-6xl mx-auto w-full px-6 pt-3">
          <div className="bg-emerald-500/10 border border-emerald-500/30 rounded-xl p-3 text-emerald-300 text-xs flex items-center justify-between gap-3 shadow-sm">
            <div className="flex items-center gap-2 truncate">
              <span className="font-semibold text-emerald-400 flex-shrink-0">Clipboard URL Detected:</span>
              <span className="truncate text-slate-300 font-mono text-[11px]">{clipboardDetectedUrl}</span>
            </div>
            <div className="flex items-center gap-2 flex-shrink-0">
              <button
                onClick={() => {
                  const target = clipboardDetectedUrl;
                  setClipboardDetectedUrl(null);
                  handleAnalyze(target);
                }}
                className="px-3 py-1 bg-emerald-600 hover:bg-emerald-500 text-white rounded-lg font-medium text-xs shadow-sm transition-colors"
              >
                Analyze Now
              </button>
              <button
                onClick={() => setClipboardDetectedUrl(null)}
                className="px-2 py-1 text-slate-400 hover:text-slate-200 text-xs transition-colors"
              >
                Dismiss
              </button>
            </div>
          </div>
        </div>
      )}

      {globalError && (
        <div className="max-w-6xl mx-auto w-full px-6 pt-4">
          <div className="bg-rose-50 dark:bg-rose-500/10 border border-rose-200 dark:border-rose-500/30 rounded-xl p-3 text-rose-800 dark:text-rose-300 text-xs flex items-center justify-between shadow-xs">
            <div className="flex items-center gap-2">
              <AlertCircle className="w-4 h-4 text-rose-600 dark:text-rose-400 flex-shrink-0" />
              <span>{globalError}</span>
            </div>
            <button
              onClick={() => setGlobalError(null)}
              className="text-slate-500 hover:text-slate-800 dark:text-slate-400 dark:hover:text-white text-xs px-2 py-0.5 rounded transition-colors"
            >
              Dismiss
            </button>
          </div>
        </div>
      )}

      {/* Main Content Area */}
      <main className="max-w-6xl mx-auto w-full px-6 py-6 flex-1 flex flex-col">
        {/* Navigation Tabs: Queue vs History vs Settings */}
        <div className="flex items-center justify-between border-b border-slate-200 dark:border-slate-800 pb-4 mb-5">
          <div className="flex items-center gap-3">
            <button
              onClick={() => setActiveTab('queue')}
              className={`flex items-center gap-2 px-4 py-2 rounded-xl text-sm font-semibold transition-all ${
                activeTab === 'queue'
                  ? 'bg-indigo-600 text-white shadow-md shadow-indigo-600/20'
                  : 'bg-white text-slate-700 hover:text-slate-900 hover:bg-slate-100 border border-slate-300 dark:bg-slate-900 dark:text-slate-400 dark:hover:text-slate-200 dark:hover:bg-slate-850 dark:border-slate-800 shadow-xs'
              }`}
            >
              <ListOrdered className="w-4 h-4" />
              <span>Download Queue</span>
              {queueItems.length > 0 && (
                <span className={`text-xs px-1.5 py-0.2 rounded-full font-bold ${
                  activeTab === 'queue'
                    ? 'bg-indigo-800 text-indigo-100'
                    : 'bg-indigo-100 text-indigo-700 dark:bg-indigo-950 dark:text-indigo-200 border border-indigo-200 dark:border-indigo-700/50'
                }`}>
                  {queueItems.length}
                </span>
              )}
              <span className={`text-[10px] font-mono px-1 py-0.2 rounded ml-0.5 ${
                activeTab === 'queue' ? 'text-indigo-200 bg-indigo-700/60' : 'text-slate-400 bg-slate-100 dark:bg-slate-800'
              }`}>
                1
              </span>
            </button>

            <button
              onClick={() => setActiveTab('history')}
              className={`flex items-center gap-2 px-4 py-2 rounded-xl text-sm font-semibold transition-all ${
                activeTab === 'history'
                  ? 'bg-indigo-600 text-white shadow-md shadow-indigo-600/20'
                  : 'bg-white text-slate-700 hover:text-slate-900 hover:bg-slate-100 border border-slate-300 dark:bg-slate-900 dark:text-slate-400 dark:hover:text-slate-200 dark:hover:bg-slate-850 dark:border-slate-800 shadow-xs'
              }`}
            >
              <History className="w-4 h-4" />
              <span>All History</span>
              {downloads.length > 0 && (
                <span className={`text-xs px-1.5 py-0.2 rounded-full font-bold ${
                  activeTab === 'history'
                    ? 'bg-indigo-800 text-indigo-100'
                    : 'bg-slate-100 text-slate-700 dark:bg-slate-800 dark:text-slate-300'
                }`}>
                  {downloads.length}
                </span>
              )}
              <span className={`text-[10px] font-mono px-1 py-0.2 rounded ml-0.5 ${
                activeTab === 'history' ? 'text-indigo-200 bg-indigo-700/60' : 'text-slate-400 bg-slate-100 dark:bg-slate-800'
              }`}>
                2
              </span>
            </button>

            <button
              onClick={() => setActiveTab('settings')}
              className={`flex items-center gap-2 px-4 py-2 rounded-xl text-sm font-semibold transition-all ${
                activeTab === 'settings'
                  ? 'bg-indigo-600 text-white shadow-md shadow-indigo-600/20'
                  : 'bg-white text-slate-700 hover:text-slate-900 hover:bg-slate-100 border border-slate-300 dark:bg-slate-900 dark:text-slate-400 dark:hover:text-slate-200 dark:hover:bg-slate-850 dark:border-slate-800 shadow-xs'
              }`}
            >
              <Settings className="w-4 h-4" />
              <span>Settings</span>
              <span className={`text-[10px] font-mono px-1 py-0.2 rounded ml-0.5 ${
                activeTab === 'settings' ? 'text-indigo-200 bg-indigo-700/60' : 'text-slate-400 bg-slate-100 dark:bg-slate-800'
              }`}>
                3
              </span>
            </button>
          </div>

          <button
            onClick={() => {
              loadDownloads();
              loadQueue();
              setHistoryRefreshKey((k) => k + 1);
            }}
            className="flex items-center gap-1.5 text-xs text-slate-600 hover:text-slate-900 hover:bg-slate-100 dark:text-slate-400 dark:hover:text-slate-200 px-3 py-1.5 rounded-lg bg-white dark:bg-slate-900 border border-slate-300 dark:border-slate-800 transition-colors shadow-xs"
            title="Refresh state"
          >
            <RefreshCw className="w-3.5 h-3.5" />
            <span className="hidden sm:inline font-medium">Refresh</span>
          </button>
        </div>

        {/* View 1: Download Queue */}
        {activeTab === 'queue' && (
          <div className="flex-1 flex flex-col">
            {/* Global Queue Summary Bar */}
            <QueueSummaryBar
              summary={queueSummary}
              concurrency={concurrency}
              onConcurrencyChange={handleQueueConcurrencyChange}
              onStartAll={handleQueueStartAll}
              onPauseAll={handleQueuePauseAll}
              onResumeAll={handleQueueResumeAll}
              onCancelAll={handleQueueCancelAll}
              onClearCompleted={handleQueueClearCompleted}
              onClearFailed={handleQueueClearFailed}
            />

            {/* Queue Item Cards */}
            <div className="space-y-3 flex-1">
              {queueItems.map((item, idx) => (
                <QueueItemCard
                  key={item.id}
                  item={item}
                  liveProgress={liveProgress[item.id]}
                  isFirst={idx === 0}
                  isLast={idx === queueItems.length - 1}
                  onPause={handleQueuePauseItem}
                  onResume={handleQueueResumeItem}
                  onCancel={handleQueueCancelItem}
                  onRetry={handleQueueRetryItem}
                  onRemove={handleQueueRemoveItem}
                  onMoveUp={handleQueueMoveUp}
                  onMoveDown={handleQueueMoveDown}
                  onOpenFile={handleOpenFile}
                  onOpenFolder={handleOpenFolder}
                />
              ))}

              {queueItems.length === 0 && (
                <div className="h-64 border border-dashed border-slate-300 dark:border-slate-800 rounded-2xl flex flex-col items-center justify-center text-center p-6 bg-slate-50/50 dark:bg-slate-900/20">
                  <div className="w-12 h-12 rounded-full bg-slate-200 dark:bg-slate-800/80 flex items-center justify-center text-slate-400 dark:text-slate-500 mb-3">
                    <ListOrdered className="w-6 h-6 stroke-[1.5]" />
                  </div>
                  <h3 className="font-semibold text-slate-800 dark:text-slate-300 text-sm">Download Queue is Empty</h3>
                  <p className="text-xs text-slate-500 dark:text-slate-400 max-w-sm mt-1">
                    Analyze any media link above or press <kbd className="px-1.5 py-0.5 text-[10px] font-mono font-semibold bg-white dark:bg-slate-800 border border-slate-300 dark:border-slate-700 rounded shadow-xs text-slate-700 dark:text-slate-300">Ctrl+V</kbd> to add downloads to queue.
                  </p>
                </div>
              )}
            </div>
          </div>
        )}

        {/* View 2: Download History & Analytics */}
        {activeTab === 'history' && (
          <HistoryView
            refreshTrigger={historyRefreshKey}
            onNavigateToQueue={() => {
              setActiveTab('queue');
              loadQueue();
            }}
          />
        )}

        {/* View 3: Settings & Preferences */}
        {activeTab === 'settings' && (
          <SettingsView
            settings={appSettings}
            onSettingsSaved={(newSettings) => {
              setAppSettings(newSettings);
              setConcurrency(newSettings.max_concurrent_downloads || 2);
            }}
            onNavigateToQueue={() => {
              setActiveTab('queue');
              loadQueue();
            }}
          />
        )}
      </main>

      {/* Format Selection Modal (Single Video) */}
      <FormatModal
        parseResult={activeParseResult}
        isOpen={isModalOpen}
        defaultSettings={appSettings}
        onClose={() => setIsModalOpen(false)}
        onConfirm={handleConfirmDownload}
        onAddToQueue={handleAddToQueue}
      />

      {/* Multi-URL Batch Modal */}
      <MultiUrlModal
        isOpen={isMultiUrlOpen}
        onClose={() => setIsMultiUrlOpen(false)}
        onQueueSuccess={() => {
          loadQueue();
        }}
        onNavigateToQueue={() => {
          setActiveTab('queue');
        }}
      />

      {/* Playlist Prompt Modal (Watch URL with Playlist param) */}
      <PlaylistPromptModal
        isOpen={isPlaylistPromptOpen}
        detection={pendingPlaylistDetection}
        onClose={() => {
          setIsPlaylistPromptOpen(false);
          setPendingUrl(null);
        }}
        onViewPlaylist={handlePromptViewPlaylist}
        onAnalyzeSingle={handlePromptAnalyzeSingle}
      />

      {/* Playlist Discovery Modal */}
      <PlaylistModal
        isOpen={isPlaylistModalOpen}
        playlist={activePlaylist}
        onClose={() => {
          setIsPlaylistModalOpen(false);
          setActivePlaylist(null);
        }}
        onQueueSuccess={() => {
          loadQueue();
        }}
        onNavigateToQueue={() => {
          setActiveTab('queue');
        }}
      />
    </div>
  );
};

export default App;
