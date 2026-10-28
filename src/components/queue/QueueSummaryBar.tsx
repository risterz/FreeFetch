import React from 'react';
import {
  Play,
  Pause,
  XCircle,
  RotateCcw,
  Trash2,
  Sliders,
  Layers,
  CheckCircle2,
  AlertTriangle,
} from 'lucide-react';
import { QueueSummary } from '../../api/types';
import { ProgressBar } from '../common/ProgressBar';

interface QueueSummaryBarProps {
  summary: QueueSummary;
  concurrency: number;
  onConcurrencyChange: (newConcurrency: number) => void;
  onStartAll: () => void;
  onPauseAll: () => void;
  onResumeAll: () => void;
  onCancelAll: () => void;
  onClearCompleted: () => void;
  onClearFailed: () => void;
}

export const QueueSummaryBar: React.FC<QueueSummaryBarProps> = ({
  summary,
  concurrency,
  onConcurrencyChange,
  onStartAll,
  onPauseAll,
  onResumeAll,
  onCancelAll,
  onClearCompleted,
  onClearFailed,
}) => {
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

  return (
    <div className="bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded-2xl p-4 sm:p-5 shadow-sm space-y-4 mb-4 transition-colors">
      {/* Top Header: Counts & Concurrency Selector */}
      <div className="flex flex-wrap items-center justify-between gap-3 pb-3 border-b border-slate-200 dark:border-slate-800/80">
        <div className="flex items-center gap-2.5">
          <Layers className="w-5 h-5 text-indigo-600 dark:text-indigo-400" />
          <h2 className="text-sm font-semibold text-slate-900 dark:text-white">Queue Manager</h2>
          <span className="text-xs px-2 py-0.5 rounded-full bg-slate-100 dark:bg-slate-800 text-slate-700 dark:text-slate-300 font-medium border border-slate-200 dark:border-slate-700/60">
            {summary.completed_count} / {summary.total_items} Completed
          </span>
        </div>

        <div className="flex items-center gap-3">
          <div className="flex items-center gap-1.5 text-xs text-slate-600 dark:text-slate-400 bg-slate-50 dark:bg-slate-950/60 px-3 py-1.5 rounded-xl border border-slate-200 dark:border-slate-800">
            <Sliders className="w-3.5 h-3.5 text-indigo-600 dark:text-indigo-400" />
            <span>Simultaneous:</span>
            <select
              value={concurrency}
              onChange={(e) => onConcurrencyChange(Number(e.target.value))}
              aria-label="Maximum simultaneous downloads"
              className="bg-white dark:bg-slate-900 border border-slate-300 dark:border-slate-700 text-slate-900 dark:text-white rounded-md text-xs px-2 py-0.5 focus:outline-none focus:border-indigo-500 font-semibold cursor-pointer"
            >
              <option value={1}>1</option>
              <option value={2}>2 (Recommended)</option>
              <option value={3}>3</option>
              <option value={4}>4</option>
              <option value={5}>5</option>
            </select>
          </div>
        </div>
      </div>

      {/* Progress & Speed Breakdown */}
      <div className="space-y-2">
        <div className="flex items-center justify-between text-xs">
          <div className="flex items-center gap-3">
            <span className="font-semibold text-slate-800 dark:text-slate-200">Overall Progress</span>
            <span className="text-slate-500 dark:text-slate-400">
              {formatBytes(summary.downloaded_bytes)}
              {summary.total_bytes > 0 && ` / ${formatBytes(summary.total_bytes)}`}
            </span>
          </div>

          <div className="flex items-center gap-3 font-medium">
            {summary.total_speed_bytes_per_sec > 0 && (
              <span className="text-emerald-600 dark:text-emerald-400 font-semibold font-mono">
                {formatSpeed(summary.total_speed_bytes_per_sec)}
              </span>
            )}
            <span className="text-slate-900 dark:text-white font-bold text-sm">
              {summary.overall_progress_percent.toFixed(1)}%
            </span>
          </div>
        </div>

        <ProgressBar
          percent={summary.overall_progress_percent}
          status={summary.downloading_count > 0 ? 'downloading' : summary.completed_count === summary.total_items && summary.total_items > 0 ? 'completed' : 'paused'}
        />

        {/* State Badges Row */}
        <div className="flex flex-wrap items-center gap-2 pt-1 text-xs">
          {summary.downloading_count > 0 && (
            <span className="px-2 py-0.5 rounded-lg bg-blue-500/10 text-blue-700 dark:text-blue-400 border border-blue-500/20 flex items-center gap-1 font-medium">
              <span className="w-1.5 h-1.5 rounded-full bg-blue-600 dark:bg-blue-400 animate-pulse" />
              {summary.downloading_count} downloading
            </span>
          )}
          {summary.queued_count > 0 && (
            <span className="px-2 py-0.5 rounded-lg bg-slate-100 dark:bg-slate-800 text-slate-700 dark:text-slate-300 border border-slate-200 dark:border-slate-700 flex items-center gap-1 font-medium">
              {summary.queued_count} queued
            </span>
          )}
          {summary.paused_count > 0 && (
            <span className="px-2 py-0.5 rounded-lg bg-amber-500/10 text-amber-700 dark:text-amber-400 border border-amber-500/20 flex items-center gap-1 font-medium">
              {summary.paused_count} paused
            </span>
          )}
          {summary.completed_count > 0 && (
            <span className="px-2 py-0.5 rounded-lg bg-emerald-500/10 text-emerald-700 dark:text-emerald-400 border border-emerald-500/20 flex items-center gap-1 font-medium">
              <CheckCircle2 className="w-3 h-3" />
              {summary.completed_count} completed
            </span>
          )}
          {summary.failed_count > 0 && (
            <span className="px-2 py-0.5 rounded-lg bg-rose-500/10 text-rose-700 dark:text-rose-400 border border-rose-500/20 flex items-center gap-1 font-medium">
              <AlertTriangle className="w-3 h-3" />
              {summary.failed_count} failed
            </span>
          )}
        </div>
      </div>

      {/* Global Action Controls */}
      <div className="flex flex-wrap items-center justify-between gap-2 pt-2 border-t border-slate-200 dark:border-slate-800/80">
        <div className="flex flex-wrap items-center gap-2">
          <button
            onClick={onStartAll}
            className="px-3 py-1.5 rounded-xl bg-emerald-600 hover:bg-emerald-500 text-white text-xs font-semibold flex items-center gap-1.5 transition-colors shadow-sm"
            title="Start or process next items"
          >
            <Play className="w-3.5 h-3.5 fill-current" />
            <span>Start All</span>
          </button>

          <button
            onClick={onResumeAll}
            className="px-3 py-1.5 rounded-xl bg-slate-100 hover:bg-slate-200 text-slate-700 border-slate-200 dark:bg-slate-800 dark:hover:bg-slate-700 dark:text-slate-200 text-xs font-medium flex items-center gap-1.5 transition-colors border dark:border-slate-700"
            title="Resume all paused items"
          >
            <RotateCcw className="w-3.5 h-3.5" />
            <span>Resume All</span>
          </button>

          <button
            onClick={onPauseAll}
            className="px-3 py-1.5 rounded-xl bg-slate-100 hover:bg-slate-200 text-slate-700 border-slate-200 dark:bg-slate-800 dark:hover:bg-slate-700 dark:text-slate-200 text-xs font-medium flex items-center gap-1.5 transition-colors border dark:border-slate-700"
            title="Pause all active downloading items"
          >
            <Pause className="w-3.5 h-3.5 fill-current" />
            <span>Pause All</span>
          </button>

          <button
            onClick={onCancelAll}
            className="px-3 py-1.5 rounded-xl bg-rose-500/10 hover:bg-rose-500/20 text-rose-700 dark:text-rose-300 text-xs font-medium flex items-center gap-1.5 transition-colors border border-rose-500/20"
            title="Cancel all active and waiting items"
          >
            <XCircle className="w-3.5 h-3.5" />
            <span>Cancel All</span>
          </button>
        </div>

        <div className="flex items-center gap-2">
          {summary.completed_count > 0 && (
            <button
              onClick={onClearCompleted}
              className="px-2.5 py-1.5 rounded-xl text-slate-600 hover:text-slate-900 hover:bg-slate-100 dark:text-slate-400 dark:hover:text-slate-200 text-xs flex items-center gap-1 transition-colors dark:hover:bg-slate-800"
              title="Remove finished downloads from queue view"
            >
              <Trash2 className="w-3 h-3" />
              <span>Clear Completed</span>
            </button>
          )}

          {summary.failed_count > 0 && (
            <button
              onClick={onClearFailed}
              className="px-2.5 py-1.5 rounded-xl text-rose-600 hover:text-rose-700 dark:text-rose-400 dark:hover:text-rose-300 text-xs flex items-center gap-1 transition-colors hover:bg-rose-500/10"
              title="Remove failed/cancelled downloads from queue"
            >
              <Trash2 className="w-3 h-3" />
              <span>Clear Failed</span>
            </button>
          )}
        </div>
      </div>
    </div>
  );
};
