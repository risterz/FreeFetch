import React from 'react';
import {
  Play,
  Pause,
  XCircle,
  RotateCcw,
  Trash2,
  FolderOpen,
  FileVideo,
  ChevronUp,
  ChevronDown,
  Film,
  AlertCircle,
  Clock,
  Gauge,
  CheckCircle2,
} from 'lucide-react';
import { DownloadProgressPayload, QueueItem } from '../../api/types';
import { ProgressBar } from '../common/ProgressBar';

interface QueueItemCardProps {
  item: QueueItem;
  liveProgress?: DownloadProgressPayload;
  isFirst: boolean;
  isLast: boolean;
  onPause: (id: string) => void;
  onResume: (id: string) => void;
  onCancel: (id: string) => void;
  onRetry: (id: string) => void;
  onRemove: (id: string) => void;
  onMoveUp: (id: string) => void;
  onMoveDown: (id: string) => void;
  onOpenFile: (path: string) => void;
  onOpenFolder: (path: string) => void;
}

export const QueueItemCard: React.FC<QueueItemCardProps> = ({
  item,
  liveProgress,
  isFirst,
  isLast,
  onPause,
  onResume,
  onCancel,
  onRetry,
  onRemove,
  onMoveUp,
  onMoveDown,
  onOpenFile,
  onOpenFolder,
}) => {
  const downloaded = liveProgress?.downloadedBytes ?? item.downloaded_bytes;
  const total = liveProgress?.totalBytes ?? item.file_size;
  const speed = liveProgress?.speedBytesPerSec ?? item.speed;
  const eta = liveProgress?.etaSeconds ?? item.eta;
  const percent =
    liveProgress?.progressPercent ??
    (total > 0
      ? (downloaded / total) * 100
      : item.status === 'completed'
      ? 100
      : item.progress);

  const formatBytes = (bytes: number) => {
    if (bytes <= 0) return '0 B';
    const mb = bytes / (1024 * 1024);
    if (mb >= 1024) return `${(mb / 1024).toFixed(2)} GB`;
    return `${mb.toFixed(1)} MB`;
  };

  const formatSpeed = (bytesPerSec: number) => {
    if (bytesPerSec <= 0) return '0 KB/s';
    const kb = bytesPerSec / 1024;
    if (kb >= 1024) return `${(kb / 1024).toFixed(2)} MB/s`;
    return `${kb.toFixed(0)} KB/s`;
  };

  const formatEta = (seconds: number) => {
    if (seconds <= 0) return '0s';
    const mins = Math.floor(seconds / 60);
    const secs = seconds % 60;
    if (mins > 0) return `${mins}m ${secs}s`;
    return `${secs}s`;
  };

  const renderStatusBadge = () => {
    switch (item.status) {
      case 'queued':
        return (
          <span className="px-2 py-0.5 rounded-full bg-slate-100 dark:bg-slate-800 text-slate-700 dark:text-slate-300 border border-slate-200 dark:border-slate-700 text-xs font-semibold">
            Waiting in Queue
          </span>
        );
      case 'preparing':
        return (
          <span className="px-2 py-0.5 rounded-full bg-indigo-500/10 text-indigo-700 dark:text-indigo-400 border border-indigo-500/20 text-xs font-semibold flex items-center gap-1">
            <span className="w-1.5 h-1.5 rounded-full bg-indigo-600 dark:bg-indigo-400 animate-pulse" />
            Preparing
          </span>
        );
      case 'downloading':
        return (
          <span className="px-2 py-0.5 rounded-full bg-blue-500/10 text-blue-700 dark:text-blue-400 border border-blue-500/20 text-xs font-semibold flex items-center gap-1">
            <span className="w-1.5 h-1.5 rounded-full bg-blue-600 dark:bg-blue-400 animate-pulse" />
            Downloading
          </span>
        );
      case 'retrying':
        return (
          <span className="px-2 py-0.5 rounded-full bg-amber-500/10 text-amber-700 dark:text-amber-400 border border-amber-500/20 text-xs font-semibold flex items-center gap-1">
            <span className="w-1.5 h-1.5 rounded-full bg-amber-600 dark:bg-amber-400 animate-pulse" />
            Retrying
          </span>
        );
      case 'processing':
        return (
          <span className="px-2 py-0.5 rounded-full bg-indigo-500/10 text-indigo-700 dark:text-indigo-400 border border-indigo-500/20 text-xs font-semibold flex items-center gap-1">
            <span className="w-1.5 h-1.5 rounded-full bg-indigo-600 dark:bg-indigo-400 animate-pulse" />
            Remuxing
          </span>
        );
      case 'completed':
        return (
          <span className="px-2 py-0.5 rounded-full bg-emerald-500/10 text-emerald-700 dark:text-emerald-400 border border-emerald-500/20 text-xs font-semibold flex items-center gap-1">
            <CheckCircle2 className="w-3 h-3" />
            Completed
          </span>
        );
      case 'paused':
        return (
          <span className="px-2 py-0.5 rounded-full bg-amber-500/10 text-amber-700 dark:text-amber-400 border border-amber-500/20 text-xs font-semibold">
            Paused
          </span>
        );
      case 'cancelled':
        return (
          <span className="px-2 py-0.5 rounded-full bg-slate-100 dark:bg-slate-800 text-slate-600 dark:text-slate-400 border border-slate-200 dark:border-slate-700 text-xs font-semibold">
            Cancelled
          </span>
        );
      case 'failed':
        return (
          <span className="px-2 py-0.5 rounded-full bg-rose-500/10 text-rose-700 dark:text-rose-400 border border-rose-500/20 text-xs font-semibold flex items-center gap-1">
            <AlertCircle className="w-3 h-3" />
            Failed
          </span>
        );
    }
  };

  const isActive =
    item.status === 'downloading' ||
    item.status === 'preparing' ||
    item.status === 'processing' ||
    item.status === 'retrying';

  return (
    <div className="bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800/90 rounded-2xl p-4 sm:p-5 shadow-sm hover:border-slate-300 dark:hover:border-slate-700/80 transition-all flex flex-col gap-3">
      {/* Upper Section */}
      <div className="flex gap-4 items-start">
        {/* Position Number */}
        <div className="hidden sm:flex flex-col items-center justify-center w-6 text-slate-400 dark:text-slate-500 font-mono text-xs font-semibold pt-1">
          #{item.position + 1}
        </div>

        {/* Thumbnail Preview */}
        {item.thumbnail ? (
          <img
            src={item.thumbnail}
            alt={item.title}
            className="w-24 h-16 sm:w-28 sm:h-18 object-cover rounded-xl border border-slate-200 dark:border-slate-800 flex-shrink-0 bg-slate-100 dark:bg-slate-950"
          />
        ) : (
          <div className="w-24 h-16 sm:w-28 sm:h-18 rounded-xl bg-slate-100 dark:bg-slate-800 flex items-center justify-center flex-shrink-0 text-slate-400 dark:text-slate-500 border border-slate-200 dark:border-slate-750">
            <Film className="w-6 h-6" />
          </div>
        )}

        {/* Info Column */}
        <div className="flex-1 min-w-0">
          <div className="flex items-start justify-between gap-2">
            <h4
              className="text-sm sm:text-base font-semibold text-slate-900 dark:text-slate-100 truncate hover:text-emerald-600 dark:hover:text-white transition-colors"
              title={item.title}
            >
              {item.title}
            </h4>
            <div className="flex items-center gap-2 flex-shrink-0">
              {renderStatusBadge()}
            </div>
          </div>

          <div className="flex flex-wrap items-center gap-2 mt-1.5 text-xs text-slate-500 dark:text-slate-400">
            <span className="px-2 py-0.5 rounded bg-slate-100 dark:bg-slate-800 text-slate-700 dark:text-slate-300 font-medium border border-slate-200 dark:border-slate-700/60">
              {item.selected_format}
            </span>
            {item.retry_count > 0 && (
              <span className="text-amber-600 dark:text-amber-400 font-medium">
                • Retries: {item.retry_count}
              </span>
            )}
            <span className="text-slate-400 dark:text-slate-500 truncate max-w-xs font-mono text-[11px]">
              {item.canonical_url}
            </span>
          </div>

          {item.error_message && (
            <div className="mt-2 text-xs text-rose-700 dark:text-rose-400 bg-rose-500/10 border border-rose-500/20 px-2.5 py-1.5 rounded-lg flex items-center gap-1.5">
              <AlertCircle className="w-3.5 h-3.5 flex-shrink-0" />
              <span className="truncate">{item.error_message}</span>
            </div>
          )}
        </div>
      </div>

      {/* Progress Bar (when downloading, paused, or completed) */}
      {(isActive || item.status === 'paused' || item.status === 'completed') && (
        <div className="space-y-1.5 pt-1">
          <ProgressBar percent={percent} status={item.status} />

          <div className="flex items-center justify-between text-xs text-slate-500 dark:text-slate-400 font-mono">
            <div>
              <span>{formatBytes(downloaded)}</span>
              {total > 0 && <span> / {formatBytes(total)}</span>}
              <span className="ml-2 font-bold text-slate-800 dark:text-slate-200">({percent.toFixed(1)}%)</span>
            </div>

            {isActive && (
              <div className="flex items-center gap-3">
                {speed > 0 && (
                  <span className="text-emerald-600 dark:text-emerald-400 font-semibold flex items-center gap-1">
                    <Gauge className="w-3.5 h-3.5" />
                    {formatSpeed(speed)}
                  </span>
                )}
                {eta > 0 && (
                  <span className="flex items-center gap-1">
                    <Clock className="w-3.5 h-3.5" />
                    ETA: {formatEta(eta)}
                  </span>
                )}
              </div>
            )}
          </div>
        </div>
      )}

      {/* Action Controls Toolbar */}
      <div className="flex items-center justify-between pt-2 border-t border-slate-200 dark:border-slate-800/80 gap-2">
        {/* Reordering Buttons (Only for non-active items) */}
        <div className="flex items-center gap-1">
          {!isActive && (
            <>
              <button
                onClick={() => onMoveUp(item.id)}
                disabled={isFirst}
                className="p-1 rounded-lg text-slate-400 hover:text-slate-900 hover:bg-slate-100 dark:hover:text-white dark:hover:bg-slate-800 transition-colors disabled:opacity-30 disabled:hover:bg-transparent"
                title="Move Up in Queue"
              >
                <ChevronUp className="w-4 h-4" />
              </button>
              <button
                onClick={() => onMoveDown(item.id)}
                disabled={isLast}
                className="p-1 rounded-lg text-slate-400 hover:text-slate-900 hover:bg-slate-100 dark:hover:text-white dark:hover:bg-slate-800 transition-colors disabled:opacity-30 disabled:hover:bg-transparent"
                title="Move Down in Queue"
              >
                <ChevronDown className="w-4 h-4" />
              </button>
            </>
          )}
        </div>

        {/* State-Specific Controls */}
        <div className="flex items-center gap-2">
          {/* Active Downloading: Pause & Cancel */}
          {isActive && (
            <>
              <button
                onClick={() => onPause(item.id)}
                className="px-3 py-1 rounded-xl bg-slate-100 hover:bg-slate-200 text-slate-700 border-slate-200 dark:bg-slate-800 dark:hover:bg-slate-700 dark:text-slate-200 text-xs font-medium flex items-center gap-1.5 transition-colors border dark:border-slate-700"
                title="Pause Download"
              >
                <Pause className="w-3.5 h-3.5 fill-current" />
                <span>Pause</span>
              </button>
              <button
                onClick={() => onCancel(item.id)}
                className="px-3 py-1 rounded-xl bg-rose-500/10 hover:bg-rose-500/20 text-rose-700 dark:text-rose-300 text-xs font-medium flex items-center gap-1.5 transition-colors border border-rose-500/20"
                title="Cancel Download"
              >
                <XCircle className="w-3.5 h-3.5" />
                <span>Cancel</span>
              </button>
            </>
          )}

          {/* Paused: Resume, Cancel, Remove */}
          {item.status === 'paused' && (
            <>
              <button
                onClick={() => onResume(item.id)}
                className="px-3 py-1 rounded-xl bg-emerald-600 hover:bg-emerald-500 text-white text-xs font-semibold flex items-center gap-1.5 transition-colors shadow-sm"
                title="Resume Download"
              >
                <Play className="w-3.5 h-3.5 fill-current" />
                <span>Resume</span>
              </button>
              <button
                onClick={() => onCancel(item.id)}
                className="px-3 py-1 rounded-xl bg-slate-100 hover:bg-slate-200 text-slate-700 border-slate-200 dark:bg-slate-800 dark:hover:bg-slate-700 dark:text-slate-300 text-xs font-medium flex items-center gap-1.5 transition-colors border dark:border-slate-700"
                title="Cancel Download"
              >
                <XCircle className="w-3.5 h-3.5" />
                <span>Cancel</span>
              </button>
              <button
                onClick={() => onRemove(item.id)}
                className="p-1.5 rounded-lg text-slate-400 hover:text-rose-600 hover:bg-rose-500/10 transition-colors"
                title="Remove from Queue"
              >
                <Trash2 className="w-4 h-4" />
              </button>
            </>
          )}

          {/* Queued: Start, Cancel, Remove */}
          {item.status === 'queued' && (
            <>
              <button
                onClick={() => onResume(item.id)}
                className="px-3 py-1 rounded-xl bg-indigo-600 hover:bg-indigo-500 text-white text-xs font-medium flex items-center gap-1.5 transition-colors shadow-sm"
                title="Start Now"
              >
                <Play className="w-3.5 h-3.5 fill-current" />
                <span>Start</span>
              </button>
              <button
                onClick={() => onCancel(item.id)}
                className="px-3 py-1 rounded-xl bg-slate-100 hover:bg-slate-200 text-slate-700 border-slate-200 dark:bg-slate-800 dark:hover:bg-slate-700 dark:text-slate-300 text-xs font-medium flex items-center gap-1.5 transition-colors border dark:border-slate-700"
                title="Cancel Download"
              >
                <XCircle className="w-3.5 h-3.5" />
                <span>Cancel</span>
              </button>
              <button
                onClick={() => onRemove(item.id)}
                className="p-1.5 rounded-lg text-slate-400 hover:text-rose-600 hover:bg-rose-500/10 transition-colors"
                title="Remove from Queue"
              >
                <Trash2 className="w-4 h-4" />
              </button>
            </>
          )}

          {/* Failed or Cancelled: Retry, Remove */}
          {(item.status === 'failed' || item.status === 'cancelled') && (
            <>
              <button
                onClick={() => onRetry(item.id)}
                className="px-3 py-1 rounded-xl bg-emerald-600 hover:bg-emerald-500 text-white text-xs font-semibold flex items-center gap-1.5 transition-colors shadow-sm"
                title="Retry Download"
              >
                <RotateCcw className="w-3.5 h-3.5" />
                <span>Retry</span>
              </button>
              <button
                onClick={() => onRemove(item.id)}
                className="p-1.5 rounded-lg text-slate-400 hover:text-rose-600 hover:bg-rose-500/10 transition-colors"
                title="Remove from Queue"
              >
                <Trash2 className="w-4 h-4" />
              </button>
            </>
          )}

          {/* Completed: Open File, Open Folder, Remove */}
          {item.status === 'completed' && (
            <>
              <button
                onClick={() => onOpenFile(item.output_path)}
                className="px-3 py-1 rounded-xl bg-slate-100 hover:bg-slate-200 text-slate-700 border-slate-200 dark:bg-slate-800 dark:hover:bg-slate-700 dark:text-slate-200 text-xs font-medium flex items-center gap-1.5 transition-colors border dark:border-slate-700"
                title="Open Downloaded File"
              >
                <FileVideo className="w-3.5 h-3.5 text-emerald-600 dark:text-emerald-400" />
                <span>Open File</span>
              </button>
              <button
                onClick={() => onOpenFolder(item.output_path)}
                className="px-3 py-1 rounded-xl bg-slate-100 hover:bg-slate-200 text-slate-700 border-slate-200 dark:bg-slate-800 dark:hover:bg-slate-700 dark:text-slate-200 text-xs font-medium flex items-center gap-1.5 transition-colors border dark:border-slate-700"
                title="Open Containing Folder"
              >
                <FolderOpen className="w-3.5 h-3.5 text-indigo-600 dark:text-indigo-400" />
                <span>Open Folder</span>
              </button>
              <button
                onClick={() => onRemove(item.id)}
                className="p-1.5 rounded-lg text-slate-400 hover:text-slate-700 dark:hover:text-slate-200 hover:bg-slate-100 dark:hover:bg-slate-800 transition-colors"
                title="Remove from Queue"
              >
                <Trash2 className="w-4 h-4" />
              </button>
            </>
          )}
        </div>
      </div>
    </div>
  );
};
