export type DownloadStatus =
  | 'queued'
  | 'analyzing'
  | 'downloading'
  | 'retrying'
  | 'processing'
  | 'completed'
  | 'failed'
  | 'paused'
  | 'cancelled';

export interface MediaMetadata {
  id: string;
  url: string;
  title: string;
  duration: number;
  thumbnail_url?: string | null;
  uploader?: string | null;
  service_name: string;
  description?: string | null;
  upload_date?: string | null;
}

export interface MediaFormat {
  format_id: string;
  format_type: 'video' | 'audio' | 'muxed';
  quality_label: string;
  resolution?: string | null;
  width?: number | null;
  height?: number | null;
  fps?: number | null;
  video_codec?: string | null;
  audio_codec?: string | null;
  mime_type?: string | null;
  bitrate?: number | null;
  container: string;
  filesize?: number | null;
  download_url: string;
  requires_merge: boolean;
  matching_audio_url?: string | null;
}

export type SubtitleFormat = 'srt' | 'vtt';

export interface SubtitleTrack {
  id: string;
  language_code: string;
  language_name: string;
  format: SubtitleFormat;
  is_auto_generated: boolean;
  is_default: boolean;
  download_url?: string | null;
}

export interface SubtitleSelection {
  language_code: string;
  language_name: string;
  target_format: SubtitleFormat;
  is_auto_generated: boolean;
  download_url?: string | null;
}

export interface ParseResult {
  metadata: MediaMetadata;
  formats: MediaFormat[];
  subtitles?: SubtitleTrack[];
}

export interface DownloadRecord {
  id: string;
  source_url: string;
  service_name: string;
  title: string;
  duration: number;
  thumbnail_url?: string | null;
  destination_path: string;
  container: string;
  quality_label: string;
  total_bytes: number;
  downloaded_bytes: number;
  status: DownloadStatus;
  error_message?: string | null;
  created_at: number;
  completed_at?: number | null;
}

export interface HistoryFilter {
  search?: string | null;
  status?: string | null;
  media_type?: string | null;
  limit?: number | null;
  offset?: number | null;
}

export interface HistoryPage {
  items: DownloadRecord[];
  total_count: number;
  limit: number;
  offset: number;
}

export interface DownloadStatistics {
  total_downloads: number;
  completed_downloads: number;
  failed_downloads: number;
  cancelled_downloads: number;
  total_downloaded_bytes: number;
  audio_downloads: number;
  video_downloads: number;
  total_duration_seconds: number;
}

export type AudioTargetFormat = 'mp3' | 'm4a' | 'flac' | 'opus';
export type AudioQualityPreset = 'best' | 'high' | 'medium' | 'low' | 'custom';
export type BitrateMode = 'cbr' | 'vbr';

export interface AudioConversionOptions {
  target_format?: AudioTargetFormat;
  quality_preset?: AudioQualityPreset;
  bitrate_mode?: BitrateMode;
  custom_bitrate_kbps?: number | null;
  sample_rate?: number | null;
  embed_metadata?: boolean;
  embed_artwork?: boolean;
  filename_template?: string | null;
}

export type SubtitleMode = 'none' | 'embed' | 'hardcode';

export interface StartDownloadParams {
  source_url: string;
  service_name: string;
  title: string;
  duration: number;
  thumbnail_url?: string | null;
  format_id: string;
  download_url: string;
  requires_merge: boolean;
  matching_audio_url?: string | null;
  container: string;
  quality_label: string;
  destination_dir?: string | null;
  selected_subtitles?: SubtitleSelection[];
  embed_subtitles?: boolean;
  subtitle_mode?: SubtitleMode;
  audio_options?: AudioConversionOptions | null;
  video_codec?: string | null;
  audio_codec?: string | null;
}

export interface AppSettings {
  download_directory: string;
  max_concurrent_downloads: number;
  speed_limit_bytes_per_sec?: number | null;
  ffmpeg_path?: string | null;
  default_container: string;
  default_audio_format?: string;
  default_audio_quality?: string;
  embed_artwork?: boolean;
  default_subtitle_mode?: string;
  theme?: string;
  notifications_enabled?: boolean;
  auto_clear_completed?: boolean;
}

export interface DownloadProgressPayload {
  id: string;
  downloadedBytes: number;
  totalBytes: number;
  speedBytesPerSec: number;
  etaSeconds: number;
  progressPercent: number;
}

export interface DownloadStateChangedPayload {
  id: string;
  status: DownloadStatus;
  errorMessage?: string | null;
}

export type QueueStatus =
  | 'queued'
  | 'preparing'
  | 'downloading'
  | 'paused'
  | 'processing'
  | 'completed'
  | 'failed'
  | 'cancelled'
  | 'retrying';

export interface QueueItem {
  id: string;
  url: string;
  canonical_url: string;
  title: string;
  thumbnail?: string | null;
  status: QueueStatus;
  selected_format: string;
  format_id: string;
  output_path: string;
  file_size: number;
  downloaded_bytes: number;
  progress: number;
  speed: number;
  eta: number;
  error_code?: string | null;
  error_message?: string | null;
  retry_count: number;
  created_at: number;
  started_at?: number | null;
  completed_at?: number | null;
  updated_at: number;
  position: number;
  params_json: string;
}

export interface QueueSummary {
  total_items: number;
  queued_count: number;
  downloading_count: number;
  paused_count: number;
  completed_count: number;
  failed_count: number;
  total_bytes: number;
  downloaded_bytes: number;
  overall_progress_percent: number;
  total_speed_bytes_per_sec: number;
}

export type ItemAvailability = 'available' | 'unavailable' | 'private' | 'deleted';

export interface PlaylistItem {
  id: string;
  index: number;
  url: string;
  canonical_url: string;
  title: string;
  thumbnail?: string | null;
  duration: number;
  availability: ItemAvailability;
}

export interface PlaylistInfo {
  id: string;
  canonical_url: string;
  title: string;
  thumbnail?: string | null;
  uploader?: string | null;
  item_count: number;
  items: PlaylistItem[];
}

export interface PlaylistDetection {
  playlist_id: string;
  is_pure_playlist: boolean;
  video_id?: string | null;
  canonical_playlist_url: string;
}

export type BatchItemStatus = 'pending' | 'analyzing' | 'ready' | 'failed';

export interface BatchAnalysisItem {
  original_url: string;
  canonical_url: string;
  status: BatchItemStatus;
  metadata?: MediaMetadata | null;
  formats: MediaFormat[];
  subtitles?: SubtitleTrack[];
  error_message?: string | null;
}

export interface BatchAnalysisResult {
  total: number;
  ready_count: number;
  failed_count: number;
  items: BatchAnalysisItem[];
}

export interface BatchQueueResult {
  added_count: number;
  duplicate_count: number;
  failed_count: number;
  unavailable_count: number;
  added_ids: string[];
  errors: string[];
}

export interface PlaylistItemSelection {
  id: string;
  url: string;
  title: string;
  duration: number;
  thumbnail?: string | null;
  format_id?: string | null;
  selected_subtitles?: SubtitleSelection[];
}

