import React from 'react';
import {
  Play,
  Pause,
  XCircle,
  FolderOpen,
  FileVideo,
  Trash2,
  AlertCircle,
  Clock,
  Gauge,
  Film,
} from 'lucide-react';
import { DownloadProgressPayload, DownloadRecord } from '../../api/types';
import { ProgressBar } from '../common/ProgressBar';

interface DownloadCardProps {
  record: DownloadRecord;
  liveProgress?: DownloadProgressPayload;
  onPause: (id: string) => void;
  onResume: (id: string) => void;
  onCancel: (id: string) => void;
  onDelete: (id: string, deleteFiles: boolean) => void;
  onOpenFile: (path: string) => void;
  onOpenFolder: (path: string) => void;
}

export const DownloadCard: React.FC<DownloadCardProps> = ({
  record,
  liveProgress,
  onPause,
  onResume,
  onCancel,
  onDelete,
  onOpenFile,
  onOpenFolder,
}) => {
  const downloaded = liveProgress?.downloadedBytes ?? record.downloaded_bytes;
  const total = liveProgress?.totalBytes ?? record.total_bytes;
  const speed = liveProgress?.speedBytesPerSec ?? 0;
  const eta = liveProgress?.etaSeconds ?? 0;
  const percent =
    liveProgress?.progressPercent ??
    (total > 0 ? (downloaded / total) * 100 : record.status === 'completed' ? 100 : 0);

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

  const getStatusBadge = () => {
    switch (record.status) {
      case 'downloading':
        return (
          <span className="px-2 py-0.5 rounded-full bg-blue-500/10 text-blue-400 border border-blue-500/20 text-xs font-semibold flex items-center gap-1">
            <span className="w-1.5 h-1.5 rounded-full bg-blue-400 animate-pulse" />
            Downloading
          </span>
        );
      case 'retrying':
        return (
          <span className="px-2 py-0.5 rounded-full bg-amber-500/10 text-amber-400 border border-amber-500/20 text-xs font-semibold flex items-center gap-1">
            <span className="w-1.5 h-1.5 rounded-full bg-amber-400 animate-pulse" />
            Retrying
          </span>
        );
      case 'processing':
        return (
          <span className="px-2 py-0.5 rounded-full bg-indigo-500/10 text-indigo-400 border border-indigo-500/20 text-xs font-semibold flex items-center gap-1">
            <span className="w-1.5 h-1.5 rounded-full bg-indigo-400 animate-pulse" />
            Remuxing
          </span>
        );
      case 'completed':
        return (
          <span className="px-2 py-0.5 rounded-full bg-emerald-500/10 text-emerald-400 border border-emerald-500/20 text-xs font-semibold">
            Completed
          </span>
        );
      case 'paused':
        return (
          <span className="px-2 py-0.5 rounded-full bg-amber-500/10 text-amber-400 border border-amber-500/20 text-xs font-semibold">
            Paused
          </span>
        );
      case 'failed':
        return (
          <span className="px-2 py-0.5 rounded-full bg-rose-500/10 text-rose-400 border border-rose-500/20 text-xs font-semibold">
            Failed
          </span>
        );
      case 'cancelled':
        return (
          <span className="px-2 py-0.5 rounded-full bg-slate-500/10 text-slate-400 border border-slate-500/20 text-xs font-semibold">
            Cancelled
          </span>
        );
      default:
        return (
          <span className="px-2 py-0.5 rounded-full bg-slate-800 text-slate-400 text-xs font-semibold">
            Queued
          </span>
        );
    }
  };

  const isDownloading =
    record.status === 'downloading' ||
    record.status === 'retrying' ||
    record.status === 'processing';

  return (
    <div className="bg-slate-900 border border-slate-800 hover:border-slate-750 transition-all rounded-2xl p-4 shadow-sm flex flex-col sm:flex-row gap-4 items-start sm:items-center justify-between">
      {/* Thumbnail and Info */}
      <div className="flex gap-3.5 items-center min-w-0 flex-1">
        {record.thumbnail_url ? (
          <img
            src={record.thumbnail_url}
            alt={record.title}
            className="w-24 h-16 object-cover rounded-xl border border-slate-800 flex-shrink-0 bg-slate-950"
          />
        ) : (
          <div className="w-24 h-16 rounded-xl bg-slate-800 flex items-center justify-center flex-shrink-0 text-slate-500">
            <Film className="w-6 h-6" />
          </div>
        )}

        <div className="min-w-0 flex-1 space-y-1.5">
          <div className="flex items-center gap-2 flex-wrap">
            <h4 className="font-medium text-slate-100 text-sm truncate max-w-md" title={record.title}>
              {record.title}
            </h4>
            {getStatusBadge()}
          </div>

          <div className="flex items-center gap-2 text-xs text-slate-400">
            <span className="font-semibold text-slate-300">{record.quality_label}</span>
            <span>•</span>
            <span className="uppercase font-bold text-slate-300">{record.container}</span>
            <span>•</span>
            <span className="capitalize">{record.service_name}</span>
          </div>

          {/* Progress Bar & Numerical stats */}
          <div className="space-y-1 pt-1 max-w-md">
            <ProgressBar percent={percent} status={record.status} />

            <div className="flex items-center justify-between text-[11px] text-slate-400 font-mono">
              <span>
                {formatBytes(downloaded)} / {formatBytes(total)} ({percent.toFixed(1)}%)
              </span>

              {isDownloading && (
                <div className="flex items-center gap-3">
                  <span className="flex items-center gap-1 text-emerald-400 font-medium">
                    <Gauge className="w-3 h-3" />
                    {formatSpeed(speed)}
                  </span>
                  {eta > 0 && (
                    <span className="flex items-center gap-1 text-slate-400">
                      <Clock className="w-3 h-3" />
                      {formatEta(eta)}
                    </span>
                  )}
                </div>
              )}
            </div>
          </div>

          {record.error_message && (
            <div className="flex items-center gap-1 text-rose-400 text-xs font-medium pt-1">
              <AlertCircle className="w-3.5 h-3.5 flex-shrink-0" />
              <span className="truncate" title={record.error_message}>
                {record.error_message}
              </span>
            </div>
          )}
        </div>
      </div>

      {/* Action Controls */}
      <div className="flex items-center gap-2 self-end sm:self-center flex-shrink-0">
        {record.status === 'downloading' && (
          <button
            onClick={() => onPause(record.id)}
            className="p-2 rounded-xl bg-slate-800 hover:bg-slate-700 text-slate-300 hover:text-white border border-slate-700 transition-colors"
            title="Pause download"
          >
            <Pause className="w-4 h-4" />
          </button>
        )}

        {record.status === 'paused' && (
          <button
            onClick={() => onResume(record.id)}
            className="p-2 rounded-xl bg-emerald-600/20 hover:bg-emerald-600/30 text-emerald-300 border border-emerald-500/30 transition-colors"
            title="Resume download"
          >
            <Play className="w-4 h-4" />
          </button>
        )}

        {record.status === 'failed' && (
          <button
            onClick={() => onResume(record.id)}
            className="p-2 rounded-xl bg-blue-600/20 hover:bg-blue-600/30 text-blue-300 border border-blue-500/30 transition-colors"
            title="Retry download"
          >
            <Play className="w-4 h-4" />
          </button>
        )}

        {isDownloading && (
          <button
            onClick={() => onCancel(record.id)}
            className="p-2 rounded-xl bg-slate-800 hover:bg-rose-500/20 text-slate-400 hover:text-rose-400 border border-slate-700 hover:border-rose-500/30 transition-colors"
            title="Cancel download"
          >
            <XCircle className="w-4 h-4" />
          </button>
        )}

        {record.status === 'completed' && (
          <>
            <button
              onClick={() => onOpenFile(record.destination_path)}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-xl bg-emerald-600/15 hover:bg-emerald-600/25 text-emerald-300 border border-emerald-500/30 text-xs font-semibold transition-colors"
              title="Open file"
            >
              <FileVideo className="w-3.5 h-3.5" />
              <span>Open File</span>
            </button>

            <button
              onClick={() => onOpenFolder(record.destination_path)}
              className="p-2 rounded-xl bg-slate-800 hover:bg-slate-700 text-slate-300 hover:text-white border border-slate-700 transition-colors"
              title="Open containing folder"
            >
              <FolderOpen className="w-4 h-4" />
            </button>
          </>
        )}

        <button
          onClick={() => onDelete(record.id, record.status === 'completed')}
          className="p-2 rounded-xl bg-slate-800 hover:bg-rose-500/20 text-slate-400 hover:text-rose-400 border border-slate-700 hover:border-rose-500/30 transition-colors"
          title="Remove from history"
        >
          <Trash2 className="w-4 h-4" />
        </button>
      </div>
    </div>
  );
};
