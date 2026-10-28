import { invoke } from '@tauri-apps/api/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';
import {
  AppSettings,
  BatchAnalysisResult,
  BatchQueueResult,
  DownloadProgressPayload,
  DownloadRecord,
  DownloadStateChangedPayload,
  DownloadStatistics,
  HistoryFilter,
  HistoryPage,
  ParseResult,
  PlaylistDetection,
  PlaylistInfo,
  PlaylistItemSelection,
  QueueItem,
  QueueSummary,
  StartDownloadParams,
  YtDlpDiagnostics,
} from './types';

export const FreeFetchApi = {
  async parseUrl(url: string): Promise<ParseResult> {
    return invoke<ParseResult>('parse_url', { url });
  },

  async startDownload(params: StartDownloadParams): Promise<string> {
    return invoke<string>('start_download', { params });
  },

  async pauseDownload(id: string): Promise<void> {
    return invoke<void>('pause_download', { id });
  },

  async resumeDownload(id: string): Promise<void> {
    return invoke<void>('resume_download', { id });
  },

  async cancelDownload(id: string): Promise<void> {
    return invoke<void>('cancel_download', { id });
  },

  async deleteDownload(id: string, deleteFiles: boolean): Promise<void> {
    return invoke<void>('delete_download', { id, deleteFiles });
  },

  async getDownloads(): Promise<DownloadRecord[]> {
    return invoke<DownloadRecord[]>('get_downloads');
  },

  // History & Analytics Operations
  async getDownloadHistory(filter: HistoryFilter): Promise<HistoryPage> {
    return invoke<HistoryPage>('get_download_history', { filter });
  },

  async getDownloadHistoryItem(id: string): Promise<DownloadRecord | null> {
    return invoke<DownloadRecord | null>('get_download_history_item', { id });
  },

  async deleteDownloadHistoryItem(id: string, deleteFile?: boolean): Promise<void> {
    return invoke<void>('delete_download_history_item', { id, deleteFile });
  },

  async clearDownloadHistory(statusFilter?: string): Promise<number> {
    return invoke<number>('clear_download_history', { statusFilter });
  },

  async getDownloadStatistics(): Promise<DownloadStatistics> {
    return invoke<DownloadStatistics>('get_download_statistics');
  },

  async exportDownloadHistory(format: string): Promise<string> {
    return invoke<string>('export_download_history', { format });
  },

  async retryHistoryDownload(id: string): Promise<string> {
    return invoke<string>('retry_history_download', { id });
  },

  async getSettings(): Promise<AppSettings> {
    return invoke<AppSettings>('get_settings');
  },

  async updateSettings(settings: AppSettings): Promise<void> {
    return invoke<void>('update_settings', { settings });
  },

  async checkFfmpegStatus(): Promise<string> {
    return invoke<string>('check_ffmpeg_status');
  },

  async checkYtDlpStatus(): Promise<string> {
    return invoke<string>('check_ytdlp_status');
  },

  async diagnoseYtDlp(url?: string): Promise<YtDlpDiagnostics> {
    return invoke<YtDlpDiagnostics>('diagnose_ytdlp', { url: url || null });
  },

  async updateYtDlp(): Promise<string> {
    return invoke<string>('update_ytdlp');
  },

  async openDownloadedFile(path: string): Promise<void> {
    return invoke<void>('open_downloaded_file', { path });
  },

  async openContainingFolder(path: string): Promise<void> {
    return invoke<void>('open_containing_folder', { path });
  },

  // Queue Operations
  async queueAdd(params: StartDownloadParams): Promise<QueueItem> {
    return invoke<QueueItem>('queue_add', { params });
  },

  async queueRemove(id: string): Promise<void> {
    return invoke<void>('queue_remove', { id });
  },

  async queueStart(): Promise<void> {
    return invoke<void>('queue_start');
  },

  async queuePause(id?: string): Promise<void> {
    return invoke<void>('queue_pause', { id });
  },

  async queueResume(id?: string): Promise<void> {
    return invoke<void>('queue_resume', { id });
  },

  async queueCancel(id?: string): Promise<void> {
    return invoke<void>('queue_cancel', { id });
  },

  async queueRetry(id: string): Promise<void> {
    return invoke<void>('queue_retry', { id });
  },

  async queueReorder(id: string, targetPosition: number): Promise<void> {
    return invoke<void>('queue_reorder', { id, targetPosition });
  },

  async queueMoveUp(id: string): Promise<void> {
    return invoke<void>('queue_move_up', { id });
  },

  async queueMoveDown(id: string): Promise<void> {
    return invoke<void>('queue_move_down', { id });
  },

  async queueGet(): Promise<QueueItem[]> {
    return invoke<QueueItem[]>('queue_get');
  },

  async queueClearCompleted(): Promise<void> {
    return invoke<void>('queue_clear_completed');
  },

  async queueClearFailed(): Promise<void> {
    return invoke<void>('queue_clear_failed');
  },

  async queueSetConcurrency(concurrency: number): Promise<void> {
    return invoke<void>('queue_set_concurrency', { concurrency });
  },

  async getQueueSummary(): Promise<QueueSummary> {
    return invoke<QueueSummary>('get_queue_summary');
  },

  // Batch & Playlist Operations
  async parseBatchUrlsText(rawText: string): Promise<string[]> {
    return invoke<string[]>('parse_batch_urls_text', { rawText });
  },

  async analyzeBatchUrls(urls: string[]): Promise<BatchAnalysisResult> {
    return invoke<BatchAnalysisResult>('analyze_batch_urls', { urls });
  },

  async addBatchToQueue(items: StartDownloadParams[]): Promise<BatchQueueResult> {
    return invoke<BatchQueueResult>('add_batch_to_queue', { items });
  },

  async detectPlaylistUrl(url: string): Promise<PlaylistDetection | null> {
    return invoke<PlaylistDetection | null>('detect_playlist_url', { url });
  },

  async analyzePlaylist(url: string): Promise<PlaylistInfo> {
    return invoke<PlaylistInfo>('analyze_playlist', { url });
  },

  async addPlaylistToQueue(
    items: PlaylistItemSelection[],
    quality: string
  ): Promise<BatchQueueResult> {
    return invoke<BatchQueueResult>('add_playlist_to_queue', { items, quality });
  },


  // Events
  async onProgress(callback: (payload: DownloadProgressPayload) => void): Promise<UnlistenFn> {
    return listen<DownloadProgressPayload>('download-progress', (event) => {
      callback(event.payload);
    });
  },

  async onStateChanged(
    callback: (payload: DownloadStateChangedPayload) => void
  ): Promise<UnlistenFn> {
    return listen<DownloadStateChangedPayload>('download-state-changed', (event) => {
      callback(event.payload);
    });
  },

  async onQueueItemAdded(callback: (item: QueueItem) => void): Promise<UnlistenFn> {
    return listen<QueueItem>('queue:item-added', (event) => {
      callback(event.payload);
    });
  },

  async onQueueItemUpdated(callback: (item: QueueItem) => void): Promise<UnlistenFn> {
    return listen<QueueItem>('queue:item-updated', (event) => {
      callback(event.payload);
    });
  },

  async onQueueItemRemoved(callback: (data: { id: string }) => void): Promise<UnlistenFn> {
    return listen<{ id: string }>('queue:item-removed', (event) => {
      callback(event.payload);
    });
  },

  async onQueueUpdated(callback: (items: QueueItem[]) => void): Promise<UnlistenFn> {
    return listen<QueueItem[]>('queue:updated', (event) => {
      callback(event.payload);
    });
  },

  async onQueueSummary(callback: (summary: QueueSummary) => void): Promise<UnlistenFn> {
    return listen<QueueSummary>('queue:summary', (event) => {
      callback(event.payload);
    });
  },
};
