import React, { useState, useEffect } from 'react';
import {
  X,
  Film,
  Music,
  Check,
  HardDrive,
  Layers,
  DownloadCloud,
  ListPlus,
  Subtitles,
  ChevronDown,
  ChevronUp,
  CheckSquare,
  Square,
  AlertTriangle,
} from 'lucide-react';
import { validateStreamCombination } from '../../utils/container';
import {
  MediaFormat,
  ParseResult,
  SubtitleFormat,
  SubtitleSelection,
  SubtitleMode,
  AudioConversionOptions,
  AudioTargetFormat,
  AudioQualityPreset,
  AppSettings,
} from '../../api/types';

interface FormatModalProps {
  parseResult: ParseResult | null;
  isOpen: boolean;
  onClose: () => void;
  defaultSettings?: AppSettings | null;
  onConfirm: (
    selectedFormat: MediaFormat,
    subtitles: SubtitleSelection[],
    subtitleMode: SubtitleMode,
    audioOptions?: AudioConversionOptions
  ) => void;
  onAddToQueue?: (
    selectedFormat: MediaFormat,
    subtitles: SubtitleSelection[],
    subtitleMode: SubtitleMode,
    audioOptions?: AudioConversionOptions
  ) => void;
}

export const FormatModal: React.FC<FormatModalProps> = ({
  parseResult,
  isOpen,
  onClose,
  defaultSettings,
  onConfirm,
  onAddToQueue,
}) => {
  const [activeTab, setActiveTab] = useState<'video' | 'audio'>('video');
  const [selectedFormatId, setSelectedFormatId] = useState<string | null>(null);

  // Audio Conversion State
  const [targetAudioFormat, setTargetAudioFormat] = useState<AudioTargetFormat>(
    (defaultSettings?.default_audio_format as AudioTargetFormat) || 'mp3'
  );
  const [audioQualityPreset, setAudioQualityPreset] = useState<AudioQualityPreset>(
    (defaultSettings?.default_audio_quality as AudioQualityPreset) || 'best'
  );
  const [customBitrate, setCustomBitrate] = useState<number>(320);
  const [embedAudioMetadata, setEmbedAudioMetadata] = useState(true);
  const [embedArtwork, setEmbedArtwork] = useState<boolean>(
    defaultSettings?.embed_artwork ?? true
  );
  const [filenameTemplate, setFilenameTemplate] = useState('{artist} - {title}');

  // Subtitles state
  const [isSubtitlesOpen, setIsSubtitlesOpen] = useState(true);
  const [selectedSubIds, setSelectedSubIds] = useState<Set<string>>(new Set());
  const [targetSubFormat, setTargetSubFormat] = useState<SubtitleFormat>('srt');
  const [subtitleMode, setSubtitleMode] = useState<SubtitleMode>(
    (defaultSettings?.default_subtitle_mode as SubtitleMode) || 'embed'
  );

  // Sync settings when modal opens
  useEffect(() => {
    if (isOpen && defaultSettings) {
      if (defaultSettings.default_audio_format) {
        setTargetAudioFormat(defaultSettings.default_audio_format as AudioTargetFormat);
      }
      if (defaultSettings.default_audio_quality) {
        setAudioQualityPreset(defaultSettings.default_audio_quality as AudioQualityPreset);
      }
      if (typeof defaultSettings.embed_artwork === 'boolean') {
        setEmbedArtwork(defaultSettings.embed_artwork);
      }
      if (defaultSettings.default_subtitle_mode) {
        setSubtitleMode(defaultSettings.default_subtitle_mode as SubtitleMode);
      }
    }
  }, [isOpen, defaultSettings]);

  // Initial video container selection based on settings
  useEffect(() => {
    if (!parseResult || !parseResult.formats || parseResult.formats.length === 0) return;
    const prefContainer = defaultSettings?.default_container?.toLowerCase() || 'mp4';
    const matching = parseResult.formats.find(
      (f) => f.format_type !== 'audio' && f.container?.toLowerCase() === prefContainer
    );
    if (matching) {
      setSelectedFormatId(matching.format_id);
    }
  }, [parseResult, defaultSettings]);

  // Default subtitle track auto-selection:
  // English manual -> first manual -> first auto-generated -> none
  useEffect(() => {
    if (!parseResult?.subtitles || parseResult.subtitles.length === 0) {
      setSelectedSubIds(new Set());
      return;
    }
    const subs = parseResult.subtitles;
    const enManual = subs.find(
      (s) => !s.is_auto_generated && s.language_code.toLowerCase().startsWith('en')
    );
    if (enManual) {
      setSelectedSubIds(new Set([enManual.id]));
      return;
    }
    const firstManual = subs.find((s) => !s.is_auto_generated);
    if (firstManual) {
      setSelectedSubIds(new Set([firstManual.id]));
      return;
    }
    const firstAuto =
      subs.find(
        (s) => s.is_auto_generated && s.language_code.toLowerCase().startsWith('en')
      ) || subs[0];
    if (firstAuto) {
      setSelectedSubIds(new Set([firstAuto.id]));
      return;
    }
    setSelectedSubIds(new Set());
  }, [parseResult]);

  if (!isOpen || !parseResult) return null;

  const { metadata, formats, subtitles = [] } = parseResult;

  const videoFormats = formats.filter(
    (f) => f.format_type === 'video' || f.format_type === 'muxed'
  );
  const audioFormats = formats.filter((f) => f.format_type === 'audio');

  const currentFormats = activeTab === 'video' ? videoFormats : audioFormats;

  // Auto-select first format if none selected
  const activeFormat =
    currentFormats.find((f) => f.format_id === selectedFormatId) ||
    currentFormats[0] ||
    formats[0];

  const formatFileSize = (bytes?: number | null) => {
    if (!bytes) return 'Variable / Stream';
    const mb = bytes / (1024 * 1024);
    if (mb >= 1024) return `${(mb / 1024).toFixed(2)} GB`;
    return `${mb.toFixed(1)} MB`;
  };

  const formatDuration = (seconds: number) => {
    if (!seconds) return '';
    const mins = Math.floor(seconds / 60);
    const secs = seconds % 60;
    return `${mins}:${secs.toString().padStart(2, '0')}`;
  };

  const getSubtitleSelections = (): SubtitleSelection[] => {
    return subtitles
      .filter((s) => selectedSubIds.has(s.id))
      .map((s) => ({
        language_code: s.language_code,
        language_name: s.language_name,
        target_format: targetSubFormat,
        is_auto_generated: s.is_auto_generated,
        download_url: s.download_url ?? null,
      }));
  };

  const toggleSub = (id: string) => {
    setSelectedSubIds((prev) => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        if (subtitleMode === 'hardcode') {
          next.clear();
        }
        next.add(id);
      }
      return next;
    });
  };

  const selectAllSubs = () => {
    if (subtitleMode === 'hardcode') {
      setSubtitleMode('embed');
    }
    setSelectedSubIds(new Set(subtitles.map((s) => s.id)));
  };

  const clearSubs = () => {
    setSelectedSubIds(new Set());
  };

  const getAudioOptions = (): AudioConversionOptions | undefined => {
    if (activeTab !== 'audio') return undefined;
    return {
      target_format: targetAudioFormat,
      quality_preset: audioQualityPreset,
      bitrate_mode: 'cbr',
      custom_bitrate_kbps: audioQualityPreset === 'custom' ? customBitrate : undefined,
      sample_rate: undefined,
      embed_metadata: embedAudioMetadata,
      embed_artwork: embedArtwork && targetAudioFormat !== 'opus',
      filename_template: filenameTemplate.trim() ? filenameTemplate.trim() : undefined,
    };
  };

  const compatibilityValidation =
    activeFormat && activeTab === 'video'
      ? validateStreamCombination(
          activeFormat.container,
          activeFormat.video_codec,
          activeFormat.audio_codec,
          selectedSubIds.size > 0 ? subtitleMode : 'none',
          targetSubFormat
        )
      : { valid: true };

  const handleDownload = () => {
    if (activeFormat && compatibilityValidation.valid) {
      const effectiveMode: SubtitleMode = selectedSubIds.size > 0 ? subtitleMode : 'none';
      onConfirm(activeFormat, getSubtitleSelections(), effectiveMode, getAudioOptions());
    }
  };

  const handleQueue = () => {
    if (activeFormat && onAddToQueue && compatibilityValidation.valid) {
      const effectiveMode: SubtitleMode = selectedSubIds.size > 0 ? subtitleMode : 'none';
      onAddToQueue(activeFormat, getSubtitleSelections(), effectiveMode, getAudioOptions());
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-slate-950/70 backdrop-blur-sm animate-in fade-in duration-200">
      <div className="bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded-2xl w-full max-w-2xl shadow-2xl overflow-hidden flex flex-col max-h-[92vh] text-slate-900 dark:text-slate-100">
        {/* Modal Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-slate-200 dark:border-slate-800 bg-slate-50/60 dark:bg-slate-900/60">
          <div className="flex items-center gap-2">
            <DownloadCloud className="w-5 h-5 text-emerald-600 dark:text-emerald-400" />
            <h2 className="font-semibold text-lg text-slate-900 dark:text-white">Select Download Quality</h2>
          </div>
          <button
            onClick={onClose}
            className="p-1.5 rounded-lg text-slate-400 hover:text-slate-700 hover:bg-slate-100 dark:hover:text-white dark:hover:bg-slate-800 transition-colors"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Media Preview Header */}
        <div className="p-6 border-b border-slate-200 dark:border-slate-800/80 bg-slate-50 dark:bg-slate-950/40 flex gap-4 items-start">
          {metadata.thumbnail_url ? (
            <img
              src={metadata.thumbnail_url}
              alt={metadata.title}
              className="w-32 h-20 object-cover rounded-xl border border-slate-200 dark:border-slate-800 flex-shrink-0 bg-slate-100 dark:bg-slate-850 shadow-xs"
            />
          ) : (
            <div className="w-32 h-20 rounded-xl bg-slate-200 dark:bg-slate-800 flex items-center justify-center flex-shrink-0 text-slate-400 dark:text-slate-500">
              <Film className="w-8 h-8" />
            </div>
          )}

          <div className="flex-1 min-w-0">
            <h3 className="font-medium text-slate-900 dark:text-slate-100 text-sm md:text-base line-clamp-2 leading-snug">
              {metadata.title}
            </h3>
            <div className="flex flex-wrap items-center gap-2 mt-2 text-xs text-slate-500 dark:text-slate-400">
              {metadata.uploader && (
                <span className="font-medium text-slate-700 dark:text-slate-300">{metadata.uploader}</span>
              )}
              {metadata.duration > 0 && (
                <span>• {formatDuration(metadata.duration)}</span>
              )}
              <span className="px-2 py-0.5 rounded-md bg-slate-200 dark:bg-slate-800 text-slate-700 dark:text-slate-300 uppercase tracking-wide text-[10px] font-bold">
                {metadata.service_name}
              </span>
            </div>
          </div>
        </div>

        {/* Scrollable Body: Tabs + Formats + Subtitles */}
        <div className="flex-1 overflow-y-auto">
          {/* Tab Selection */}
          <div className="flex border-b border-slate-200 dark:border-slate-800 px-6 pt-3 gap-6 bg-white dark:bg-slate-900 sticky top-0 z-10">
            <button
              onClick={() => {
                setActiveTab('video');
                if (videoFormats[0]) setSelectedFormatId(videoFormats[0].format_id);
              }}
              className={`flex items-center gap-2 pb-3 font-semibold text-sm transition-colors border-b-2 ${
                activeTab === 'video'
                  ? 'border-emerald-600 dark:border-emerald-500 text-emerald-600 dark:text-emerald-400'
                  : 'border-transparent text-slate-500 hover:text-slate-800 dark:text-slate-400 dark:hover:text-slate-200'
              }`}
            >
              <Film className="w-4 h-4" />
              <span>Video Formats ({videoFormats.length})</span>
            </button>

            <button
              onClick={() => {
                setActiveTab('audio');
                if (audioFormats[0]) setSelectedFormatId(audioFormats[0].format_id);
              }}
              className={`flex items-center gap-2 pb-3 font-semibold text-sm transition-colors border-b-2 ${
                activeTab === 'audio'
                  ? 'border-emerald-600 dark:border-emerald-500 text-emerald-600 dark:text-emerald-400'
                  : 'border-transparent text-slate-500 hover:text-slate-800 dark:text-slate-400 dark:hover:text-slate-200'
              }`}
            >
              <Music className="w-4 h-4" />
              <span>Audio Only ({audioFormats.length})</span>
            </button>
          </div>

          {/* Format List */}
          <div className="p-6 space-y-2.5">
            {currentFormats.map((format) => {
              const isSelected = activeFormat?.format_id === format.format_id;
              return (
                <div
                  key={format.format_id}
                  onClick={() => setSelectedFormatId(format.format_id)}
                  className={`p-3.5 rounded-xl border transition-all duration-150 cursor-pointer flex items-center justify-between ${
                    isSelected
                      ? 'border-emerald-500 bg-emerald-500/10 shadow-xs'
                      : 'border-slate-700/80 bg-slate-900/90 hover:bg-slate-800/90 hover:border-slate-600 text-slate-100 dark:border-slate-700/80 dark:bg-slate-900/90 dark:hover:bg-slate-800/90 dark:hover:border-slate-600'
                  }`}
                >
                  <div className="flex items-center gap-3">
                    <div
                      className={`w-5 h-5 rounded-full flex items-center justify-center border transition-all ${
                        isSelected
                          ? 'border-emerald-500 bg-emerald-500 text-white'
                          : 'border-slate-600 bg-slate-950/40'
                      }`}
                    >
                      {isSelected && <Check className="w-3 h-3 stroke-[3]" />}
                    </div>

                    <div>
                      <div className="flex items-center gap-2">
                        <span className={`font-semibold text-sm ${isSelected ? 'text-white' : 'text-slate-100'}`}>
                          {format.quality_label}
                        </span>
                        <span className="text-[11px] font-bold px-1.5 py-0.5 rounded bg-slate-800 text-slate-300 uppercase border border-slate-700">
                          {format.container}
                        </span>
                        {format.requires_merge && (
                          <span
                            className="text-[10px] font-medium px-1.5 py-0.5 rounded bg-indigo-500/10 text-indigo-300 border border-indigo-500/20 flex items-center gap-1"
                            title="Adaptive elementary streams will be merged losslessly using FFmpeg"
                          >
                            <Layers className="w-2.5 h-2.5" />
                            <span>Remux</span>
                          </span>
                        )}
                      </div>
                      <div className="flex items-center gap-3 text-xs text-slate-400 mt-1">
                        {format.resolution && <span>Res: {format.resolution}</span>}
                        {format.fps && <span>{format.fps} fps</span>}
                        {format.video_codec && <span>Codec: {format.video_codec}</span>}
                        {format.audio_codec && <span>Audio: {format.audio_codec}</span>}
                      </div>
                    </div>
                  </div>

                  <div className="text-right flex items-center gap-1.5 text-xs text-slate-300 font-medium">
                    <HardDrive className="w-3.5 h-3.5 text-slate-400" />
                    <span>{formatFileSize(format.filesize)}</span>
                  </div>
                </div>
              );
            })}

            {currentFormats.length === 0 && (
              <div className="text-center py-6 text-slate-500 dark:text-slate-400 text-sm">
                No formats available in this category.
              </div>
            )}

            {!compatibilityValidation.valid && (
              <div className="p-3.5 rounded-xl bg-rose-500/10 border border-rose-500/30 text-rose-300 text-xs flex items-start gap-2.5">
                <AlertTriangle className="w-4 h-4 text-rose-400 flex-shrink-0 mt-0.5" />
                <div className="space-y-1">
                  <div className="font-semibold text-rose-200">Incompatible Container &amp; Codec Combination</div>
                  <div className="text-rose-300/90 leading-relaxed">
                    {compatibilityValidation.error}
                  </div>
                </div>
              </div>
            )}
          </div>

          {/* Audio Conversion Settings Panel (Audio tab only) */}
          {activeTab === 'audio' && (
            <div className="border-t border-slate-200 dark:border-slate-800 px-6 py-4 bg-slate-50/70 dark:bg-slate-900/60 space-y-4">
              <div className="flex items-center gap-2 text-xs font-bold uppercase tracking-wider text-slate-700 dark:text-slate-200">
                <Music className="w-4 h-4 text-emerald-600 dark:text-emerald-400" />
                <span>Audio Extraction & Conversion Settings</span>
              </div>

              {/* Target Format Pills */}
              <div className="space-y-1.5">
                <label className="text-xs text-slate-600 dark:text-slate-400 font-medium">Output Format:</label>
                <div className="grid grid-cols-4 gap-2">
                  {(['mp3', 'm4a', 'flac', 'opus'] as AudioTargetFormat[]).map((fmt) => (
                    <button
                      key={fmt}
                      type="button"
                      onClick={() => setTargetAudioFormat(fmt)}
                      className={`py-2 px-3 rounded-xl text-xs font-bold transition-all border ${
                        targetAudioFormat === fmt
                          ? 'bg-emerald-600 border-emerald-500 text-white shadow-xs'
                          : 'bg-white hover:bg-slate-100 border-slate-200 text-slate-700 dark:bg-slate-800/80 dark:border-slate-700/80 dark:text-slate-300 dark:hover:bg-slate-800 dark:hover:text-white'
                      }`}
                    >
                      {fmt.toUpperCase()}
                    </button>
                  ))}
                </div>
              </div>

              {/* Quality Preset */}
              <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
                <div className="space-y-1.5">
                  <label className="text-xs text-slate-600 dark:text-slate-400 font-medium">Quality Preset:</label>
                  <select
                    value={audioQualityPreset}
                    onChange={(e) => setAudioQualityPreset(e.target.value as AudioQualityPreset)}
                    className="w-full bg-white dark:bg-slate-800 border border-slate-300 dark:border-slate-700 rounded-xl px-3 py-2 text-xs text-slate-900 dark:text-slate-200 font-semibold focus:outline-none focus:ring-2 focus:ring-emerald-500/40"
                  >
                    {targetAudioFormat === 'flac' ? (
                      <option value="best">Lossless (Level 5 Compression)</option>
                    ) : (
                      <>
                        <option value="best">Best Quality ({targetAudioFormat === 'mp3' ? '320 kbps' : targetAudioFormat === 'm4a' ? '256 kbps' : '160 kbps'})</option>
                        <option value="high">High Quality ({targetAudioFormat === 'mp3' ? '256 kbps' : targetAudioFormat === 'm4a' ? '192 kbps' : '128 kbps'})</option>
                        <option value="medium">Medium Quality ({targetAudioFormat === 'mp3' ? '192 kbps' : targetAudioFormat === 'm4a' ? '160 kbps' : '96 kbps'})</option>
                        <option value="low">Standard / Low ({targetAudioFormat === 'mp3' ? '128 kbps' : targetAudioFormat === 'm4a' ? '128 kbps' : '64 kbps'})</option>
                        <option value="custom">Custom Bitrate...</option>
                      </>
                    )}
                  </select>
                </div>

                {audioQualityPreset === 'custom' && targetAudioFormat !== 'flac' && (
                  <div className="space-y-1.5">
                    <label className="text-xs text-slate-600 dark:text-slate-400 font-medium">Custom Bitrate (kbps):</label>
                    <input
                      type="number"
                      min={32}
                      max={512}
                      step={16}
                      value={customBitrate}
                      onChange={(e) => setCustomBitrate(Number(e.target.value))}
                      className="w-full bg-white dark:bg-slate-800 border border-slate-300 dark:border-slate-700 rounded-xl px-3 py-2 text-xs text-slate-900 dark:text-slate-200 font-semibold focus:outline-none focus:ring-2 focus:ring-emerald-500/40"
                    />
                  </div>
                )}
              </div>

              {/* FLAC Disclaimer */}
              {targetAudioFormat === 'flac' && (
                <div className="p-3 bg-amber-500/10 border border-amber-500/20 rounded-xl text-xs text-amber-700 dark:text-amber-300 flex items-start gap-2">
                  <span className="font-bold">Notice:</span>
                  <span>
                    FLAC creates a lossless container. Converting from lossy source audio does not restore frequencies removed by source compression.
                  </span>
                </div>
              )}

              {/* Metadata & Artwork Checkboxes */}
              <div className="space-y-2 pt-1">
                <label className="flex items-center gap-2 cursor-pointer text-xs text-slate-700 dark:text-slate-300">
                  <input
                    type="checkbox"
                    checked={embedAudioMetadata}
                    onChange={(e) => setEmbedAudioMetadata(e.target.checked)}
                    className="rounded border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-800 text-emerald-600 dark:text-emerald-500 focus:ring-emerald-500/30"
                  />
                  <span>Embed metadata tags (Artist, Title, Album, Date)</span>
                </label>

                <label className="flex items-center gap-2 cursor-pointer text-xs text-slate-700 dark:text-slate-300">
                  <input
                    type="checkbox"
                    checked={embedArtwork && targetAudioFormat !== 'opus'}
                    disabled={targetAudioFormat === 'opus'}
                    onChange={(e) => setEmbedArtwork(e.target.checked)}
                    className="rounded border-slate-300 dark:border-slate-700 bg-white dark:bg-slate-800 text-emerald-600 dark:text-emerald-500 focus:ring-emerald-500/30 disabled:opacity-40"
                  />
                  <span>
                    Embed cover artwork (Thumbnail)
                    {targetAudioFormat === 'opus' && (
                      <span className="text-slate-500 text-[11px] ml-1.5">(Not supported for Opus containers)</span>
                    )}
                  </span>
                </label>
              </div>

              {/* Filename Template */}
              <div className="space-y-1.5 pt-1">
                <div className="flex items-center justify-between text-xs text-slate-600 dark:text-slate-400">
                  <label className="font-medium">Filename Template:</label>
                  <div className="flex items-center gap-1.5">
                    <button
                      type="button"
                      onClick={() => setFilenameTemplate('{artist} - {title}')}
                      className="px-1.5 py-0.5 rounded bg-white hover:bg-slate-100 text-[10px] text-slate-700 border border-slate-300 dark:bg-slate-800 dark:hover:bg-slate-700 dark:text-slate-300 dark:border-slate-700 shadow-xs"
                    >
                      Artist - Title
                    </button>
                    <button
                      type="button"
                      onClick={() => setFilenameTemplate('{title}')}
                      className="px-1.5 py-0.5 rounded bg-white hover:bg-slate-100 text-[10px] text-slate-700 border border-slate-300 dark:bg-slate-800 dark:hover:bg-slate-700 dark:text-slate-300 dark:border-slate-700 shadow-xs"
                    >
                      Title
                    </button>
                    <button
                      type="button"
                      onClick={() => setFilenameTemplate('{title} ({id})')}
                      className="px-1.5 py-0.5 rounded bg-white hover:bg-slate-100 text-[10px] text-slate-700 border border-slate-300 dark:bg-slate-800 dark:hover:bg-slate-700 dark:text-slate-300 dark:border-slate-700 shadow-xs"
                    >
                      Title (ID)
                    </button>
                  </div>
                </div>
                <input
                  type="text"
                  value={filenameTemplate}
                  onChange={(e) => setFilenameTemplate(e.target.value)}
                  placeholder="{artist} - {title}"
                  className="w-full bg-white dark:bg-slate-800 border border-slate-300 dark:border-slate-700 rounded-xl px-3 py-2 text-xs text-slate-900 dark:text-slate-200 font-mono focus:outline-none focus:ring-2 focus:ring-emerald-500/40"
                />
              </div>
            </div>
          )}

          {/* Subtitles & Captions Section (Video tab only) */}
          {activeTab === 'video' && (
            <div className="border-t border-slate-200 dark:border-slate-800 px-6 py-4 bg-slate-50/50 dark:bg-slate-900/40">
              <div
                onClick={() => setIsSubtitlesOpen(!isSubtitlesOpen)}
                className="flex items-center justify-between cursor-pointer select-none py-1"
              >
                <div className="flex items-center gap-2">
                  <Subtitles className="w-4 h-4 text-emerald-600 dark:text-emerald-400" />
                  <span className="text-xs font-bold uppercase tracking-wider text-slate-800 dark:text-slate-200">
                    Subtitles & Captions ({subtitles.length})
                  </span>
                  {selectedSubIds.size > 0 && (
                    <span className="text-[10px] px-2 py-0.5 rounded-full bg-emerald-100 text-emerald-700 dark:bg-emerald-500/20 dark:text-emerald-300 font-semibold border border-emerald-300 dark:border-emerald-500/30">
                      {selectedSubIds.size} selected
                    </span>
                  )}
                </div>
                <div className="text-slate-400 hover:text-slate-700 dark:hover:text-slate-200">
                  {isSubtitlesOpen ? (
                    <ChevronUp className="w-4 h-4" />
                  ) : (
                    <ChevronDown className="w-4 h-4" />
                  )}
                </div>
              </div>

              {isSubtitlesOpen && (
                <div className="mt-3 space-y-3 animate-in fade-in duration-150">
                  {subtitles.length > 0 ? (
                    <>
                      {/* Subtitle Controls Bar */}
                      <div className="flex flex-wrap items-center justify-between gap-2 text-xs">
                        <div className="flex items-center gap-2">
                          <button
                            type="button"
                            onClick={selectAllSubs}
                            className="px-2 py-1 rounded bg-white hover:bg-slate-100 text-slate-700 border border-slate-300 dark:bg-slate-800 dark:hover:bg-slate-700 dark:text-slate-300 dark:border-slate-700 font-medium transition-colors shadow-xs"
                          >
                            Select All
                          </button>
                          <button
                            type="button"
                            onClick={clearSubs}
                            className="px-2 py-1 rounded bg-white hover:bg-slate-100 text-slate-700 border border-slate-300 dark:bg-slate-800 dark:hover:bg-slate-700 dark:text-slate-300 dark:border-slate-700 font-medium transition-colors shadow-xs"
                          >
                            Clear
                          </button>
                        </div>

                        {/* Format Selector: SRT vs VTT */}
                        <div className="flex items-center gap-1.5 bg-slate-100 dark:bg-slate-800/80 p-1 rounded-lg border border-slate-200 dark:border-slate-700/60">
                          <span className="text-[11px] text-slate-500 dark:text-slate-400 px-1">Format:</span>
                          <button
                            type="button"
                            onClick={() => setTargetSubFormat('srt')}
                            className={`px-2 py-0.5 rounded text-xs font-semibold transition-all ${
                              targetSubFormat === 'srt'
                                ? 'bg-emerald-600 text-white shadow-xs'
                                : 'text-slate-600 hover:text-slate-900 dark:text-slate-400 dark:hover:text-slate-200'
                            }`}
                          >
                            SRT
                          </button>
                          <button
                            type="button"
                            onClick={() => setTargetSubFormat('vtt')}
                            className={`px-2 py-0.5 rounded text-xs font-semibold transition-all ${
                              targetSubFormat === 'vtt'
                                ? 'bg-emerald-600 text-white shadow-xs'
                                : 'text-slate-600 hover:text-slate-900 dark:text-slate-400 dark:hover:text-slate-200'
                            }`}
                          >
                            VTT
                          </button>
                        </div>
                      </div>

                      {/* Subtitle Track Checklist */}
                      <div className="max-h-40 overflow-y-auto space-y-1.5 p-2 bg-slate-50 dark:bg-slate-950/40 rounded-xl border border-slate-200 dark:border-slate-800">
                        {subtitles.map((sub) => {
                          const isChecked = selectedSubIds.has(sub.id);
                          return (
                            <div
                              key={sub.id}
                              onClick={() => toggleSub(sub.id)}
                              className={`flex items-center justify-between p-2 rounded-lg cursor-pointer text-xs transition-colors ${
                                isChecked
                                  ? 'bg-emerald-50 dark:bg-emerald-500/10 border border-emerald-300 dark:border-emerald-500/30 text-emerald-950 dark:text-slate-100 font-medium'
                                  : 'hover:bg-slate-100 dark:hover:bg-slate-800/50 text-slate-700 dark:text-slate-300 border border-transparent'
                              }`}
                            >
                              <div className="flex items-center gap-2">
                                {isChecked ? (
                                  <CheckSquare className="w-4 h-4 text-emerald-600 dark:text-emerald-400 shrink-0" />
                                ) : (
                                  <Square className="w-4 h-4 text-slate-400 dark:text-slate-600 shrink-0" />
                                )}
                                <span className="font-medium">{sub.language_name}</span>
                                <span className="text-[10px] font-mono text-slate-500 uppercase">
                                  [{sub.language_code}]
                                </span>
                              </div>

                              <div>
                                {sub.is_auto_generated ? (
                                  <span className="text-[10px] px-1.5 py-0.2 rounded bg-amber-500/10 text-amber-700 dark:text-amber-300 border border-amber-500/20 font-medium">
                                    Auto-generated
                                  </span>
                                ) : (
                                  <span className="text-[10px] px-1.5 py-0.2 rounded bg-emerald-500/10 text-emerald-700 dark:text-emerald-300 border border-emerald-500/20 font-medium">
                                    Manual
                                  </span>
                                )}
                              </div>
                            </div>
                          );
                        })}
                      </div>

                      {/* Subtitle Processing Mode */}
                      <div className="pt-2 border-t border-slate-200 dark:border-slate-800/80 space-y-2">
                        <div className="text-xs font-semibold text-slate-800 dark:text-slate-300">Subtitle Options</div>
                        <div className="grid grid-cols-1 sm:grid-cols-2 gap-2">
                          <label
                            className={`flex items-start gap-2.5 p-2.5 rounded-lg border cursor-pointer transition-all ${
                              subtitleMode === 'embed'
                                ? 'bg-emerald-50 dark:bg-emerald-500/10 border-emerald-500 text-emerald-950 dark:text-emerald-300 shadow-xs'
                                : 'bg-white dark:bg-slate-900/60 border-slate-200 dark:border-slate-800 text-slate-600 dark:text-slate-400 hover:border-slate-300 dark:hover:border-slate-700'
                            }`}
                          >
                            <input
                              type="radio"
                              name="subtitle-mode"
                              value="embed"
                              checked={subtitleMode === 'embed'}
                              onChange={() => setSubtitleMode('embed')}
                              className="mt-0.5 text-emerald-600 dark:text-emerald-500 focus:ring-emerald-500/30"
                            />
                            <div>
                              <div className="text-xs font-semibold text-slate-900 dark:text-slate-200">Embed Subtitles</div>
                              <div className="text-[11px] text-slate-500 dark:text-slate-400 leading-tight mt-0.5">
                                Selectable track • Lossless remux (no re-encoding)
                              </div>
                            </div>
                          </label>

                          <label
                            className={`flex items-start gap-2.5 p-2.5 rounded-lg border cursor-pointer transition-all ${
                              subtitleMode === 'hardcode'
                                ? 'bg-amber-50 dark:bg-amber-500/10 border-amber-500 text-amber-950 dark:text-amber-300 shadow-xs'
                                : 'bg-white dark:bg-slate-900/60 border-slate-200 dark:border-slate-800 text-slate-600 dark:text-slate-400 hover:border-slate-300 dark:hover:border-slate-700'
                            }`}
                          >
                            <input
                              type="radio"
                              name="subtitle-mode"
                              value="hardcode"
                              checked={subtitleMode === 'hardcode'}
                              onChange={() => {
                                setSubtitleMode('hardcode');
                                if (selectedSubIds.size > 1) {
                                  const first = Array.from(selectedSubIds)[0];
                                  setSelectedSubIds(new Set([first]));
                                }
                              }}
                              className="mt-0.5 text-amber-600 dark:text-amber-500 focus:ring-amber-500/30"
                            />
                            <div>
                              <div className="text-xs font-semibold text-slate-900 dark:text-slate-200">Hardcode / Burn-in</div>
                              <div className="text-[11px] text-slate-500 dark:text-slate-400 leading-tight mt-0.5">
                                Permanently visible in frames • Re-encodes video
                              </div>
                            </div>
                          </label>
                        </div>

                        {subtitleMode === 'hardcode' && (
                          <div className="p-2.5 rounded-xl bg-amber-500/10 border border-amber-500/20 text-amber-800 dark:text-amber-300/90 text-[11px] leading-relaxed">
                            ⚠️ <strong>Hardcoded subtitles</strong> are permanently rendered into the video frames. The video will be re-encoded using H.264 (processing will take longer). Exactly one subtitle track is burned.
                          </div>
                        )}
                      </div>
                    </>
                  ) : (
                    <div className="text-xs text-slate-500 italic py-2">
                      No subtitles available for this video.
                    </div>
                  )}
                </div>
              )}
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="px-6 py-4 bg-slate-50 dark:bg-slate-950/60 border-t border-slate-200 dark:border-slate-800 flex items-center justify-between gap-4">
          <div className="text-xs text-slate-600 dark:text-slate-400">
            {activeTab === 'audio' ? (
              <div>
                Target Audio:{' '}
                <span className="text-slate-900 dark:text-white font-semibold uppercase">
                  {targetAudioFormat} ({audioQualityPreset})
                </span>
                <span className="text-emerald-600 dark:text-emerald-400 ml-2 font-medium">
                  • Source: {activeFormat ? activeFormat.quality_label : 'Raw Audio'}
                </span>
              </div>
            ) : (
              <div>
                Selected format:{' '}
                <span className="text-slate-900 dark:text-white font-medium">
                  {activeFormat
                    ? `${activeFormat.quality_label} (${activeFormat.container.toUpperCase()})`
                    : 'None'}
                </span>
                {selectedSubIds.size > 0 && (
                  <span className="text-emerald-600 dark:text-emerald-400 ml-2 font-medium">
                    + {selectedSubIds.size} subtitle{selectedSubIds.size > 1 ? 's' : ''} (
                    {targetSubFormat.toUpperCase()}
                    {subtitleMode === 'embed'
                      ? ', Embedded'
                      : subtitleMode === 'hardcode'
                      ? ', Hardcoded'
                      : ''}
                    )
                  </span>
                )}
              </div>
            )}
          </div>

          <div className="flex items-center gap-2.5">
            <button
              onClick={onClose}
              className="px-4 py-2 rounded-xl text-sm font-semibold text-slate-600 hover:text-slate-900 hover:bg-slate-200/70 dark:text-slate-300 dark:hover:text-white dark:hover:bg-slate-800 transition-colors"
            >
              Cancel
            </button>
            {onAddToQueue && (
              <button
                onClick={handleQueue}
                disabled={!activeFormat || !compatibilityValidation.valid}
                className="px-4 py-2 rounded-xl text-sm font-semibold bg-indigo-600 hover:bg-indigo-500 text-white shadow-md shadow-indigo-600/20 transition-all flex items-center gap-1.5 disabled:opacity-50"
                title={
                  !compatibilityValidation.valid
                    ? compatibilityValidation.error
                    : 'Add to persistent queue without starting immediately'
                }
              >
                <ListPlus className="w-4 h-4" />
                <span>Add to Queue</span>
              </button>
            )}
            <button
              onClick={handleDownload}
              disabled={!activeFormat || !compatibilityValidation.valid}
              className="px-5 py-2 rounded-xl text-sm font-semibold bg-emerald-600 hover:bg-emerald-500 text-white shadow-md shadow-emerald-600/20 transition-all flex items-center gap-1.5 disabled:opacity-50"
              title={
                !compatibilityValidation.valid
                  ? compatibilityValidation.error
                  : 'Start download'
              }
            >
              <DownloadCloud className="w-4 h-4" />
              <span>Download Now</span>
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};
