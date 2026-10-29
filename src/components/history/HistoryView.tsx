import React, { useState, useEffect, useCallback } from 'react';
import {
  Search,
  X,
  RefreshCw,
  Trash2,
  Download,
  FileVideo,
  FileAudio,
  FolderOpen,
  Play,
  RotateCcw,
  Copy,
  Check,
  AlertCircle,
  Clock,
  HardDrive,
  BarChart3,
  CheckCircle2,
  XCircle,
  Filter,
  ChevronLeft,
  ChevronRight,
} from 'lucide-react';
import {
  DownloadProgressPayload,
  DownloadRecord,
  DownloadStatistics,
  HistoryFilter,
  HistoryPage,
} from '../../api/types';
import { FreeFetchApi } from '../../api/ipc';
import { ProgressBar } from '../common/ProgressBar';

interface HistoryViewProps {
  onNavigateToQueue?: () => void;
  refreshTrigger?: number;
}

export type HistoryTabFilter = 'all' | 'video' | 'audio' | 'completed' | 'failed' | 'cancelled';

export const HistoryView: React.FC<HistoryViewProps> = ({ onNavigateToQueue, refreshTrigger }) => {
  // State
  const [filterTab, setFilterTab] = useState<HistoryTabFilter>('all');
  const [searchQuery, setSearchQuery] = useState('');
  const [debouncedSearch, setDebouncedSearch] = useState('');
  const [page, setPage] = useState(0);
  const pageSize = 20;

  const [historyPage, setHistoryPage] = useState<HistoryPage>({
    items: [],
    total_count: 0,
    limit: pageSize,
    offset: 0,
  });
  const [stats, setStats] = useState<DownloadStatistics | null>(null);
  const [liveProgress, setLiveProgress] = useState<Record<string, DownloadProgressPayload>>({});
  const [isLoading, setIsLoading] = useState(false);
  const [actionMessage, setActionMessage] = useState<string | null>(null);

  // Copy feedback state
  const [copiedId, setCopiedId] = useState<string | null>(null);

  // Modal dialog states
  const [isClearModalOpen, setIsClearModalOpen] = useState(false);
  const [clearFilterOption, setClearFilterOption] = useState<'all' | 'failed' | 'completed'>('all');
  const [itemToDelete, setItemToDelete] = useState<DownloadRecord | null>(null);
  const [deletePhysicalFile, setDeletePhysicalFile] = useState(false);

  // Debounce search query
  useEffect(() => {
    const timer = setTimeout(() => {
      setDebouncedSearch(searchQuery);
      setPage(0); // Reset to page 0 on search change
    }, 250);
    return () => clearTimeout(timer);
  }, [searchQuery]);

  // Load stats
  const loadStatistics = useCallback(async () => {
    try {
      const s = await FreeFetchApi.getDownloadStatistics();
      setStats(s);
    } catch (err) {
      console.error('Failed to load statistics:', err);
    }
  }, []);

  // Load history records
  const loadHistory = useCallback(async () => {
    setIsLoading(true);
    try {
      const filter: HistoryFilter = {
        search: debouncedSearch.trim() ? debouncedSearch.trim() : null,
        status:
          filterTab === 'completed' || filterTab === 'failed' || filterTab === 'cancelled'
            ? filterTab
            : null,
        media_type:
          filterTab === 'audio' ? 'audio' : filterTab === 'video' ? 'video' : null,
        limit: pageSize,
        offset: page * pageSize,
      };

      const result = await FreeFetchApi.getDownloadHistory(filter);
      setHistoryPage(result);
    } catch (err) {
      console.error('Failed to load history:', err);
    } finally {
      setIsLoading(false);
    }
  }, [debouncedSearch, filterTab, page, pageSize]);

  // Initial load and tab/page change
  useEffect(() => {
    loadHistory();
  }, [loadHistory, refreshTrigger]);

  useEffect(() => {
    loadStatistics();
  }, [loadStatistics, refreshTrigger]);

  // Listen to live download progress and state changes
  useEffect(() => {
    let unlistenProgress: (() => void) | undefined;
    let unlistenState: (() => void) | undefined;

    const setupListeners = async () => {
      try {
        unlistenProgress = await FreeFetchApi.onProgress((payload) => {
          setLiveProgress((prev) => ({
            ...prev,
            [payload.id]: payload,
          }));
        });

        unlistenState = await FreeFetchApi.onStateChanged((payload) => {
          setHistoryPage((prev) => ({
            ...prev,
            items: prev.items.map((item) =>
              item.id === payload.id
                ? {
                    ...item,
                    status: payload.status,
                    error_message: payload.errorMessage ?? item.error_message,
                  }
                : item
            ),
          }));

          if (payload.status === 'completed' || payload.status === 'failed') {
            loadHistory();
            loadStatistics();
          }
        });
      } catch (err) {
        console.warn('Tauri listeners inactive in HistoryView:', err);
      }
    };

    setupListeners();

    return () => {
      if (unlistenProgress) unlistenProgress();
      if (unlistenState) unlistenState();
    };
  }, [loadHistory, loadStatistics]);

  // Handlers
  const handleTabChange = (tab: HistoryTabFilter) => {
    setFilterTab(tab);
    setPage(0);
  };

  const handleCopyPathOrUrl = async (id: string, record: DownloadRecord) => {
    const textToCopy =
      record.destination_path && record.destination_path.trim().length > 0
        ? record.destination_path
        : record.source_url;
    try {
      await navigator.clipboard.writeText(textToCopy);
      setCopiedId(id);
      setTimeout(() => setCopiedId(null), 2000);
      setActionMessage(
        record.destination_path && record.destination_path.trim().length > 0
          ? 'File path copied to clipboard!'
          : 'Source URL copied to clipboard!'
      );
      setTimeout(() => setActionMessage(null), 3000);
    } catch {
      // Fallback
    }
  };

  const handleOpenFile = async (path: string) => {
    if (!path || !path.trim()) {
      setActionMessage('Cannot open file: No destination file path recorded.');
      setTimeout(() => setActionMessage(null), 4000);
      return;
    }
    try {
      await FreeFetchApi.openDownloadedFile(path);
    } catch (err) {
      setActionMessage(`Could not open file: ${err instanceof Error ? err.message : String(err)}`);
      setTimeout(() => setActionMessage(null), 4000);
    }
  };

  const handleOpenFolder = async (path: string) => {
    if (!path || !path.trim()) {
      setActionMessage('Cannot open folder: No destination path recorded.');
      setTimeout(() => setActionMessage(null), 4000);
      return;
    }
    try {
      await FreeFetchApi.openContainingFolder(path);
    } catch (err) {
      setActionMessage(`Could not open folder: ${err instanceof Error ? err.message : String(err)}`);
      setTimeout(() => setActionMessage(null), 4000);
    }
  };

  const handleRetry = async (record: DownloadRecord) => {
    try {
      setIsLoading(true);
      await FreeFetchApi.retryHistoryDownload(record.id);
      setActionMessage(`"${record.title}" re-added to queue!`);
      setTimeout(() => setActionMessage(null), 4000);
      loadStatistics();
      loadHistory();
      if (onNavigateToQueue) {
        onNavigateToQueue();
      }
    } catch (err) {
      setActionMessage(`Retry failed: ${err instanceof Error ? err.message : String(err)}`);
      setTimeout(() => setActionMessage(null), 4000);
    } finally {
      setIsLoading(false);
    }
  };

  const confirmDeleteItem = async () => {
    if (!itemToDelete) return;
    try {
      await FreeFetchApi.deleteDownloadHistoryItem(itemToDelete.id, deletePhysicalFile);
      setItemToDelete(null);
      setDeletePhysicalFile(false);
      loadStatistics();
      loadHistory();
    } catch (err) {
      setActionMessage(`Delete failed: ${err instanceof Error ? err.message : String(err)}`);
      setTimeout(() => setActionMessage(null), 4000);
    }
  };

  const confirmClearHistory = async () => {
    try {
      const filterArg = clearFilterOption === 'all' ? undefined : clearFilterOption;
      await FreeFetchApi.clearDownloadHistory(filterArg);
      setIsClearModalOpen(false);
      loadStatistics();
      loadHistory();
    } catch (err) {
      setActionMessage(`Clear failed: ${err instanceof Error ? err.message : String(err)}`);
      setTimeout(() => setActionMessage(null), 4000);
    }
  };

  const handleExport = async (format: 'json' | 'csv') => {
    try {
      const data = await FreeFetchApi.exportDownloadHistory(format);
      const mimeType = format === 'json' ? 'application/json' : 'text/csv;charset=utf-8;';
      const blob = new Blob([data], { type: mimeType });
      const url = URL.createObjectURL(blob);
      const link = document.createElement('a');
      const dateStr = new Date().toISOString().slice(0, 10);
      link.href = url;
      link.setAttribute('download', `freefetch_history_${dateStr}.${format}`);
      document.body.appendChild(link);
      link.click();
      document.body.removeChild(link);
      URL.revokeObjectURL(url);
      setActionMessage(`Exported history as ${format.toUpperCase()}`);
      setTimeout(() => setActionMessage(null), 3000);
    } catch (err) {
      setActionMessage(`Export failed: ${err instanceof Error ? err.message : String(err)}`);
      setTimeout(() => setActionMessage(null), 4000);
    }
  };

  // Format helpers
  const formatBytes = (bytes: number): string => {
    if (bytes <= 0) return '0 B';
    const mb = bytes / (1024 * 1024);
    if (mb >= 1024) return `${(mb / 1024).toFixed(2)} GB`;
    return `${mb.toFixed(1)} MB`;
  };

  const formatDuration = (seconds: number): string => {
    if (seconds <= 0) return '0:00';
    const hrs = Math.floor(seconds / 3600);
    const mins = Math.floor((seconds % 3600) / 60);
    const secs = seconds % 60;
    if (hrs > 0) {
      return `${hrs}h ${mins}m ${secs}s`;
    }
    return `${mins}:${secs < 10 ? '0' : ''}${secs}`;
  };

  const formatDate = (timestamp: number): string => {
    if (!timestamp) return 'Unknown';
    const d = new Date(timestamp * 1000);
    return d.toLocaleDateString(undefined, {
      month: 'short',
      day: 'numeric',
      year: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    });
  };

  const isAudioFormat = (container: string): boolean => {
    const c = container.toLowerCase();
    return ['mp3', 'm4a', 'flac', 'opus', 'aac', 'ogg', 'wav'].includes(c);
  };

  const totalPages = Math.ceil(historyPage.total_count / pageSize);

  return (
    <div className="flex-1 flex flex-col space-y-4">
      {/* Action toast message */}
      {actionMessage && (
        <div className="bg-sky-500/20 border border-sky-500/30 text-sky-200 px-4 py-2.5 rounded-xl text-sm flex items-center justify-between animate-fadeIn shadow-lg">
          <span>{actionMessage}</span>
          <button
            onClick={() => setActionMessage(null)}
            className="text-sky-400 hover:text-white p-1 rounded transition-colors"
          >
            <X className="w-4 h-4" />
          </button>
        </div>
      )}

      {/* Analytics Summary Cards */}
      <div className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-6 gap-3">
        {/* Total Downloads */}
        <div className="bg-white dark:bg-slate-900/60 border border-slate-200 dark:border-slate-800/80 rounded-xl p-3.5 flex flex-col justify-between shadow-xs">
          <div className="flex items-center justify-between text-slate-500 dark:text-slate-400 text-xs font-medium">
            <span>Total Records</span>
            <BarChart3 className="w-4 h-4 text-sky-600 dark:text-sky-400" />
          </div>
          <div className="mt-2 text-xl font-bold text-slate-900 dark:text-white tracking-tight">
            {stats ? stats.total_downloads : '...'}
          </div>
        </div>

        {/* Completed Downloads */}
        <div className="bg-white dark:bg-slate-900/60 border border-slate-200 dark:border-slate-800/80 rounded-xl p-3.5 flex flex-col justify-between shadow-xs">
          <div className="flex items-center justify-between text-slate-500 dark:text-slate-400 text-xs font-medium">
            <span>Completed</span>
            <CheckCircle2 className="w-4 h-4 text-emerald-600 dark:text-emerald-400" />
          </div>
          <div className="mt-2 text-xl font-bold text-emerald-600 dark:text-emerald-400 tracking-tight">
            {stats ? stats.completed_downloads : '...'}
            {stats && stats.total_downloads > 0 && (
              <span className="text-xs text-slate-400 dark:text-slate-500 font-normal ml-1.5">
                ({Math.round((stats.completed_downloads / stats.total_downloads) * 100)}%)
              </span>
            )}
          </div>
        </div>

        {/* Failed / Cancelled */}
        <div className="bg-white dark:bg-slate-900/60 border border-slate-200 dark:border-slate-800/80 rounded-xl p-3.5 flex flex-col justify-between shadow-xs">
          <div className="flex items-center justify-between text-slate-500 dark:text-slate-400 text-xs font-medium">
            <span>Failed / Cancelled</span>
            <XCircle className="w-4 h-4 text-rose-600 dark:text-rose-400" />
          </div>
          <div className="mt-2 text-xl font-bold text-rose-600 dark:text-rose-400 tracking-tight">
            {stats ? stats.failed_downloads + stats.cancelled_downloads : '...'}
          </div>
        </div>

        {/* Total Data Volume */}
        <div className="bg-white dark:bg-slate-900/60 border border-slate-200 dark:border-slate-800/80 rounded-xl p-3.5 flex flex-col justify-between shadow-xs">
          <div className="flex items-center justify-between text-slate-500 dark:text-slate-400 text-xs font-medium">
            <span>Data Volume</span>
            <HardDrive className="w-4 h-4 text-indigo-600 dark:text-indigo-400" />
          </div>
          <div className="mt-2 text-xl font-bold text-slate-900 dark:text-white tracking-tight">
            {stats ? formatBytes(stats.total_downloaded_bytes) : '...'}
          </div>
        </div>

        {/* Media Distribution */}
        <div className="bg-white dark:bg-slate-900/60 border border-slate-200 dark:border-slate-800/80 rounded-xl p-3.5 flex flex-col justify-between shadow-xs">
          <div className="flex items-center justify-between text-slate-500 dark:text-slate-400 text-xs font-medium">
            <span>Audio / Video</span>
            <FileAudio className="w-4 h-4 text-amber-600 dark:text-amber-400" />
          </div>
          <div className="mt-2 text-xl font-bold text-slate-900 dark:text-white tracking-tight flex items-baseline space-x-1.5">
            <span className="text-amber-600 dark:text-amber-400">{stats ? stats.audio_downloads : 0}</span>
            <span className="text-slate-400 dark:text-slate-600 text-sm">/</span>
            <span className="text-sky-600 dark:text-sky-400">{stats ? stats.video_downloads : 0}</span>
          </div>
        </div>

        {/* Total Duration */}
        <div className="bg-white dark:bg-slate-900/60 border border-slate-200 dark:border-slate-800/80 rounded-xl p-3.5 flex flex-col justify-between shadow-xs">
          <div className="flex items-center justify-between text-slate-500 dark:text-slate-400 text-xs font-medium">
            <span>Total Playtime</span>
            <Clock className="w-4 h-4 text-purple-600 dark:text-purple-400" />
          </div>
          <div className="mt-2 text-xl font-bold text-slate-900 dark:text-white tracking-tight">
            {stats ? formatDuration(stats.total_duration_seconds) : '...'}
          </div>
        </div>
      </div>

      {/* Control Bar: Search, Filters, and Actions */}
      <div className="flex flex-col md:flex-row items-stretch md:items-center justify-between gap-3 bg-white dark:bg-slate-900/40 p-3 rounded-2xl border border-slate-200 dark:border-slate-800/60 shadow-xs">
        {/* Search Input */}
        <div className="relative flex-1 max-w-md">
          <Search className="w-4 h-4 text-slate-400 absolute left-3.5 top-1/2 -translate-y-1/2" />
          <input
            type="text"
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            placeholder="Search by title or source URL..."
            className="w-full bg-slate-50 dark:bg-slate-950/80 border border-slate-300 dark:border-slate-800/80 rounded-xl pl-9 pr-9 py-2 text-sm text-slate-900 dark:text-slate-200 placeholder-slate-400 dark:placeholder-slate-500 focus:outline-none focus:bg-white dark:focus:bg-slate-950 focus:border-sky-500 transition-colors"
          />
          {searchQuery && (
            <button
              onClick={() => setSearchQuery('')}
              className="absolute right-3 top-1/2 -translate-y-1/2 text-slate-400 hover:text-slate-600 dark:hover:text-slate-200"
            >
              <X className="w-4 h-4" />
            </button>
          )}
        </div>

        {/* Filter Pills */}
        <div className="flex items-center overflow-x-auto space-x-1 pb-1 md:pb-0">
          {(
            [
              { key: 'all', label: 'All' },
              { key: 'video', label: 'Video' },
              { key: 'audio', label: 'Audio' },
              { key: 'completed', label: 'Completed' },
              { key: 'failed', label: 'Failed' },
              { key: 'cancelled', label: 'Cancelled' },
            ] as { key: HistoryTabFilter; label: string }[]
          ).map((tab) => (
            <button
              key={tab.key}
              onClick={() => handleTabChange(tab.key)}
              className={`px-3 py-1.5 text-xs font-medium rounded-lg transition-all ${
                filterTab === tab.key
                  ? 'bg-sky-50 dark:bg-sky-500/20 text-sky-700 dark:text-sky-300 border border-sky-200 dark:border-sky-500/40 font-semibold'
                  : 'text-slate-600 dark:text-slate-400 hover:text-slate-900 dark:hover:text-slate-200 hover:bg-slate-100 dark:hover:bg-slate-800/50 border border-transparent'
              }`}
            >
              {tab.label}
            </button>
          ))}
        </div>

        {/* Actions: Export, Clear, Refresh */}
        <div className="flex items-center space-x-2">
          {/* Export JSON */}
          <button
            onClick={() => handleExport('json')}
            title="Export history records as JSON"
            className="px-2.5 py-1.5 bg-white hover:bg-slate-100 text-slate-700 border-slate-300 dark:bg-slate-800/70 dark:hover:bg-slate-700/70 dark:text-slate-300 text-xs font-medium rounded-lg border dark:border-slate-700/60 flex items-center space-x-1.5 transition-colors shadow-xs"
          >
            <Download className="w-3.5 h-3.5 text-slate-400" />
            <span>JSON</span>
          </button>

          {/* Export CSV */}
          <button
            onClick={() => handleExport('csv')}
            title="Export history records as CSV"
            className="px-2.5 py-1.5 bg-white hover:bg-slate-100 text-slate-700 border-slate-300 dark:bg-slate-800/70 dark:hover:bg-slate-700/70 dark:text-slate-300 text-xs font-medium rounded-lg border dark:border-slate-700/60 flex items-center space-x-1.5 transition-colors shadow-xs"
          >
            <Download className="w-3.5 h-3.5 text-slate-400" />
            <span>CSV</span>
          </button>

          {/* Clear History */}
          <button
            onClick={() => setIsClearModalOpen(true)}
            title="Clear download history records"
            className="px-2.5 py-1.5 bg-rose-50 hover:bg-rose-100 text-rose-700 border-rose-200 dark:bg-rose-500/10 dark:hover:bg-rose-500/20 dark:text-rose-300 text-xs font-medium rounded-lg border dark:border-rose-500/30 flex items-center space-x-1.5 transition-colors shadow-xs"
          >
            <Trash2 className="w-3.5 h-3.5 text-rose-600 dark:text-rose-400" />
            <span>Clear</span>
          </button>

          {/* Refresh */}
          <button
            onClick={() => {
              loadStatistics();
              loadHistory();
            }}
            title="Refresh history and statistics"
            className="p-1.5 bg-white hover:bg-slate-100 text-slate-700 border-slate-300 dark:bg-slate-800/70 dark:hover:bg-slate-700/70 dark:text-slate-300 rounded-lg border dark:border-slate-700/60 transition-colors shadow-xs"
          >
            <RefreshCw className={`w-3.5 h-3.5 ${isLoading ? 'animate-spin text-sky-600 dark:text-sky-400' : ''}`} />
          </button>
        </div>
      </div>

      {/* History Items List */}
      <div className="space-y-2.5 flex-1">
        {historyPage.items.map((record) => {
          const isAudio = isAudioFormat(record.container);
          const isFailed = record.status === 'failed';
          const isCompleted = record.status === 'completed';
          const isCancelled = record.status === 'cancelled';
          const isActive =
            record.status === 'downloading' ||
            record.status === 'processing' ||
            record.status === 'retrying';

          const prog = liveProgress[record.id];
          const downloadedBytes = prog?.downloadedBytes ?? record.downloaded_bytes;
          const totalBytes = prog?.totalBytes ?? record.total_bytes;
          const percent =
            prog?.progressPercent ??
            (totalBytes > 0
              ? (downloadedBytes / totalBytes) * 100
              : isCompleted
              ? 100
              : 0);
          const speed = prog?.speedBytesPerSec ?? 0;
          const eta = prog?.etaSeconds ?? 0;

          return (
            <div
              key={record.id}
              className="bg-white dark:bg-slate-900/60 border border-slate-200 dark:border-slate-800/80 rounded-2xl p-4 hover:border-slate-300 dark:hover:border-slate-700 transition-all shadow-xs group"
            >
              <div className="flex flex-col md:flex-row items-start md:items-center justify-between gap-3">
                {/* Left side: Thumbnail / Icon + Title & Metadata */}
                <div className="flex items-start space-x-3.5 flex-1 min-w-0">
                  {/* Thumbnail / Media Icon */}
                  <div className="relative w-14 h-14 rounded-xl overflow-hidden bg-slate-100 dark:bg-slate-800/90 flex-shrink-0 flex items-center justify-center border border-slate-200 dark:border-slate-700/60">
                    {record.thumbnail_url ? (
                      <img
                        src={record.thumbnail_url}
                        alt={record.title}
                        className="w-full h-full object-cover"
                        referrerPolicy="no-referrer"
                        onError={(e) => {
                          e.currentTarget.style.display = 'none';
                        }}
                      />
                    ) : null}
                    <div className="absolute inset-0 flex items-center justify-center bg-black/10 dark:bg-black/20">
                      {isAudio ? (
                        <FileAudio className="w-6 h-6 text-amber-500 dark:text-amber-400/80" />
                      ) : (
                        <FileVideo className="w-6 h-6 text-sky-500 dark:text-sky-400/80" />
                      )}
                    </div>
                  </div>

                  {/* Info */}
                  <div className="flex-1 min-w-0 space-y-1">
                    <div className="flex items-center space-x-2">
                      <span
                        className={`text-[10px] font-bold uppercase tracking-wider px-2 py-0.5 rounded ${
                          isAudio
                            ? 'bg-amber-500/15 text-amber-700 dark:text-amber-300 border border-amber-500/30'
                            : 'bg-sky-500/15 text-sky-700 dark:text-sky-300 border border-sky-500/30'
                        }`}
                      >
                        {isAudio ? 'Audio' : 'Video'}
                      </span>
                      <span className="text-[11px] font-semibold text-slate-700 dark:text-slate-400 uppercase bg-slate-100 dark:bg-slate-800 px-1.5 py-0.5 rounded border border-slate-200 dark:border-transparent">
                        {record.container}
                      </span>
                      <span className="text-xs text-slate-500">{record.quality_label}</span>
                      <span className="text-slate-400 dark:text-slate-600 text-xs">•</span>
                      <span className="text-xs text-slate-500 dark:text-slate-400">{record.service_name}</span>
                    </div>

                    <h4 className="font-semibold text-slate-900 dark:text-slate-200 text-sm truncate" title={record.title}>
                      {record.title}
                    </h4>

                    <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-slate-500">
                      <span>
                        {formatBytes(downloadedBytes || totalBytes)}
                        {totalBytes > 0 && downloadedBytes < totalBytes && ` / ${formatBytes(totalBytes)}`}
                      </span>
                      {isActive && speed > 0 && (
                        <>
                          <span>•</span>
                          <span className="text-sky-600 dark:text-sky-400 font-medium">{formatBytes(speed)}/s</span>
                        </>
                      )}
                      {isActive && eta > 0 && (
                        <>
                          <span>•</span>
                          <span className="text-slate-500 dark:text-slate-400">ETA {formatDuration(eta)}</span>
                        </>
                      )}
                      {record.duration > 0 && (
                        <>
                          <span>•</span>
                          <span>{formatDuration(record.duration)}</span>
                        </>
                      )}
                      <span>•</span>
                      <span>{formatDate(record.created_at)}</span>
                    </div>

                    {/* Progress Bar for in-flight download */}
                    {isActive && (
                      <div className="pt-2 pb-1 max-w-md">
                        <ProgressBar percent={percent} status={record.status} />
                      </div>
                    )}

                    {/* Error message display only if failed or retrying */}
                    {record.error_message && (isFailed || record.status === 'retrying') && (
                      <div className="text-xs text-rose-600 dark:text-rose-400/90 flex items-center space-x-1.5 pt-0.5">
                        <AlertCircle className="w-3.5 h-3.5 flex-shrink-0" />
                        <span className="truncate">{record.error_message}</span>
                      </div>
                    )}
                  </div>
                </div>

                {/* Right side: Status Badge + Action Buttons */}
                <div className="flex items-center space-x-2 self-end md:self-center flex-shrink-0">
                  {/* Status Badge */}
                  <div className="mr-2">
                    {isCompleted && (
                      <span className="inline-flex items-center space-x-1 text-xs font-semibold px-2.5 py-1 rounded-full bg-emerald-500/15 text-emerald-700 dark:text-emerald-400 border border-emerald-500/30">
                        <Check className="w-3 h-3" />
                        <span>Completed</span>
                      </span>
                    )}
                    {isFailed && (
                      <span className="inline-flex items-center space-x-1 text-xs font-semibold px-2.5 py-1 rounded-full bg-rose-500/15 text-rose-700 dark:text-rose-400 border border-rose-500/30">
                        <X className="w-3 h-3" />
                        <span>Failed</span>
                      </span>
                    )}
                    {isCancelled && (
                      <span className="inline-flex items-center space-x-1 text-xs font-semibold px-2.5 py-1 rounded-full bg-amber-500/15 text-amber-700 dark:text-amber-400 border border-amber-500/30">
                        <span>Cancelled</span>
                      </span>
                    )}
                    {!isCompleted && !isFailed && !isCancelled && (
                      <span className="inline-flex items-center space-x-1.5 text-xs font-semibold px-2.5 py-1 rounded-full bg-sky-500/15 text-sky-700 dark:text-sky-400 border border-sky-500/30 capitalize">
                        {isActive && (
                          <span className="w-1.5 h-1.5 rounded-full bg-sky-600 dark:bg-sky-400 animate-pulse" />
                        )}
                        <span>
                          {record.status === 'downloading'
                            ? `Downloading ${percent > 0 ? percent.toFixed(1) + '%' : ''}`
                            : record.status}
                        </span>
                      </span>
                    )}
                  </div>

                  {/* Open File (if completed) */}
                  {isCompleted && (
                    <button
                      onClick={() => handleOpenFile(record.destination_path)}
                      title="Open file"
                      className="p-2 text-slate-600 hover:text-slate-900 bg-slate-100 hover:bg-slate-200 dark:text-slate-400 dark:hover:text-white dark:bg-slate-800/80 dark:hover:bg-slate-700/80 rounded-xl transition-colors border border-slate-200 dark:border-slate-700/60"
                    >
                      <Play className="w-4 h-4 fill-current" />
                    </button>
                  )}

                  {/* Open Containing Folder */}
                  <button
                    onClick={() => handleOpenFolder(record.destination_path)}
                    title="Open destination folder in Explorer"
                    className="p-2 text-slate-600 hover:text-slate-900 bg-slate-100 hover:bg-slate-200 dark:text-slate-400 dark:hover:text-white dark:bg-slate-800/80 dark:hover:bg-slate-700/80 rounded-xl transition-colors border border-slate-200 dark:border-slate-700/60"
                  >
                    <FolderOpen className="w-4 h-4" />
                  </button>

                  {/* Copy File Path or Source URL */}
                  <button
                    onClick={() => handleCopyPathOrUrl(record.id, record)}
                    title={record.destination_path ? "Copy file path" : "Copy source URL"}
                    className="p-2 text-slate-600 hover:text-slate-900 bg-slate-100 hover:bg-slate-200 dark:text-slate-400 dark:hover:text-white dark:bg-slate-800/80 dark:hover:bg-slate-700/80 rounded-xl transition-colors border border-slate-200 dark:border-slate-700/60 relative"
                  >
                    {copiedId === record.id ? (
                      <Check className="w-4 h-4 text-emerald-600 dark:text-emerald-400" />
                    ) : (
                      <Copy className="w-4 h-4" />
                    )}
                  </button>

                  {/* Re-download / Retry button */}
                  <button
                    onClick={() => handleRetry(record)}
                    title="Re-download / Retry"
                    className="p-2 text-sky-600 hover:text-sky-700 dark:text-sky-400 dark:hover:text-sky-300 bg-sky-500/10 hover:bg-sky-500/20 rounded-xl transition-colors border border-sky-500/30"
                  >
                    <RotateCcw className="w-4 h-4" />
                  </button>

                  {/* Delete Item button */}
                  <button
                    onClick={() => {
                      setItemToDelete(record);
                      setDeletePhysicalFile(false);
                    }}
                    title="Delete record from history"
                    className="p-2 text-slate-400 hover:text-rose-600 dark:hover:text-rose-400 bg-slate-100 hover:bg-rose-500/10 dark:bg-slate-800/80 rounded-xl transition-colors border border-slate-200 dark:border-slate-700/60 hover:border-rose-500/30"
                  >
                    <Trash2 className="w-4 h-4" />
                  </button>
                </div>
              </div>
            </div>
          );
        })}

        {/* Empty State */}
        {historyPage.items.length === 0 && !isLoading && (
          <div className="h-64 border border-dashed border-slate-300 dark:border-slate-800 rounded-2xl flex flex-col items-center justify-center text-center p-6 bg-white dark:bg-slate-900/20 shadow-xs">
            <div className="w-12 h-12 rounded-full bg-slate-100 dark:bg-slate-800/80 flex items-center justify-center text-slate-400 dark:text-slate-500 mb-3">
              <Filter className="w-6 h-6 stroke-[1.5]" />
            </div>
            <h3 className="font-semibold text-slate-800 dark:text-slate-300 text-sm">No download records found</h3>
            <p className="text-xs text-slate-500 max-w-sm mt-1">
              {searchQuery
                ? `No downloads match "${searchQuery}"`
                : 'Completed or failed downloads will appear in this history view.'}
            </p>
          </div>
        )}
      </div>

      {/* Pagination Controls */}
      {totalPages > 1 && (
        <div className="flex items-center justify-between pt-2 border-t border-slate-200 dark:border-slate-800/60 text-xs text-slate-500 dark:text-slate-400 px-1">
          <div>
            Showing {page * pageSize + 1}–
            {Math.min((page + 1) * pageSize, historyPage.total_count)} of{' '}
            {historyPage.total_count} records
          </div>
          <div className="flex items-center space-x-2">
            <button
              onClick={() => setPage((p) => Math.max(0, p - 1))}
              disabled={page === 0}
              className="p-1.5 bg-white dark:bg-slate-800/80 hover:bg-slate-100 dark:hover:bg-slate-700/80 disabled:opacity-40 disabled:pointer-events-none rounded-lg border border-slate-300 dark:border-slate-700/60 transition-colors shadow-xs"
            >
              <ChevronLeft className="w-4 h-4 text-slate-600 dark:text-slate-300" />
            </button>
            <span className="font-medium text-slate-700 dark:text-slate-300 px-1">
              Page {page + 1} of {totalPages}
            </span>
            <button
              onClick={() => setPage((p) => Math.min(totalPages - 1, p + 1))}
              disabled={page >= totalPages - 1}
              className="p-1.5 bg-white dark:bg-slate-800/80 hover:bg-slate-100 dark:hover:bg-slate-700/80 disabled:opacity-40 disabled:pointer-events-none rounded-lg border border-slate-300 dark:border-slate-700/60 transition-colors shadow-xs"
            >
              <ChevronRight className="w-4 h-4 text-slate-600 dark:text-slate-300" />
            </button>
          </div>
        </div>
      )}

      {/* Clear History Confirmation Modal */}
      {isClearModalOpen && (
        <div className="fixed inset-0 bg-slate-950/60 backdrop-blur-sm z-50 flex items-center justify-center p-4 animate-in fade-in duration-150">
          <div className="bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded-2xl p-6 max-w-md w-full shadow-2xl animate-scaleIn">
            <div className="flex items-center space-x-3 text-rose-600 dark:text-rose-400 mb-4">
              <Trash2 className="w-6 h-6 flex-shrink-0" />
              <h3 className="text-lg font-bold text-slate-900 dark:text-white">Clear Download History</h3>
            </div>
            <p className="text-sm text-slate-600 dark:text-slate-300 mb-4">
              Choose which records to remove from history. This only deletes log entries; downloaded media files on disk are safely preserved.
            </p>

            {/* Clear Scope Selection */}
            <div className="space-y-2 mb-6">
              {[
                { id: 'all', label: 'All history records' },
                { id: 'failed', label: 'Only failed & cancelled records' },
                { id: 'completed', label: 'Only completed records' },
              ].map((opt) => (
                <label
                  key={opt.id}
                  className={`flex items-center space-x-2.5 p-3 rounded-xl border cursor-pointer transition-colors ${
                    clearFilterOption === opt.id
                      ? 'bg-rose-50 dark:bg-rose-500/15 border-rose-300 dark:border-rose-500/40 text-rose-900 dark:text-white font-medium'
                      : 'bg-slate-50 dark:bg-slate-950/60 border-slate-200 dark:border-slate-800 text-slate-700 dark:text-slate-400 hover:bg-slate-100 dark:hover:bg-slate-800/50'
                  }`}
                >
                  <input
                    type="radio"
                    name="clearOption"
                    value={opt.id}
                    checked={clearFilterOption === opt.id}
                    onChange={() => setClearFilterOption(opt.id as any)}
                    className="text-rose-600 focus:ring-rose-500"
                  />
                  <span className="text-sm font-medium">{opt.label}</span>
                </label>
              ))}
            </div>

            <div className="flex items-center justify-end space-x-3">
              <button
                onClick={() => setIsClearModalOpen(false)}
                className="px-4 py-2 text-sm text-slate-600 hover:text-slate-900 bg-slate-100 hover:bg-slate-200 dark:text-slate-400 dark:hover:text-white dark:bg-slate-800/80 dark:hover:bg-slate-700/80 rounded-xl transition-colors"
              >
                Cancel
              </button>
              <button
                onClick={confirmClearHistory}
                className="px-4 py-2 text-sm font-semibold text-white bg-rose-600 hover:bg-rose-500 rounded-xl shadow-lg shadow-rose-600/20 transition-colors"
              >
                Clear Records
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Delete Item Confirmation Modal */}
      {itemToDelete && (
        <div className="fixed inset-0 bg-slate-950/60 backdrop-blur-sm z-50 flex items-center justify-center p-4 animate-in fade-in duration-150">
          <div className="bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded-2xl p-6 max-w-md w-full shadow-2xl animate-scaleIn">
            <div className="flex items-center space-x-3 text-rose-600 dark:text-rose-400 mb-3">
              <Trash2 className="w-5 h-5 flex-shrink-0" />
              <h3 className="text-base font-bold text-slate-900 dark:text-white">Delete Download Record</h3>
            </div>
            <p className="text-sm text-slate-600 dark:text-slate-300 mb-3 line-clamp-2">
              Are you sure you want to remove &quot;{itemToDelete.title}&quot; from history?
            </p>

            {/* Checkbox to delete physical file */}
            <label className="flex items-center space-x-2.5 p-3 rounded-xl bg-slate-50 dark:bg-slate-950/80 border border-slate-200 dark:border-slate-800 mb-5 cursor-pointer">
              <input
                type="checkbox"
                checked={deletePhysicalFile}
                onChange={(e) => setDeletePhysicalFile(e.target.checked)}
                className="rounded text-rose-600 focus:ring-rose-500"
              />
              <span className="text-xs text-slate-700 dark:text-slate-300">
                Also permanently delete file from disk ({itemToDelete.destination_path})
              </span>
            </label>

            <div className="flex items-center justify-end space-x-3">
              <button
                onClick={() => {
                  setItemToDelete(null);
                  setDeletePhysicalFile(false);
                }}
                className="px-4 py-2 text-sm text-slate-600 hover:text-slate-900 bg-slate-100 hover:bg-slate-200 dark:text-slate-400 dark:hover:text-white dark:bg-slate-800/80 dark:hover:bg-slate-700/80 rounded-xl transition-colors"
              >
                Cancel
              </button>
              <button
                onClick={confirmDeleteItem}
                className="px-4 py-2 text-sm font-semibold text-white bg-rose-600 hover:bg-rose-500 rounded-xl shadow-lg shadow-rose-600/20 transition-colors"
              >
                Delete
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};
