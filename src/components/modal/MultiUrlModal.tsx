import React, { useState } from 'react';
import {
  X,
  ListPlus,
  CheckSquare,
  Square,
  AlertCircle,
  CheckCircle2,
  Layers,
  ArrowRight,
} from 'lucide-react';
import { BatchAnalysisItem, BatchQueueResult, StartDownloadParams } from '../../api/types';
import { FreeFetchApi } from '../../api/ipc';
import { ProgressBar } from '../common/ProgressBar';

interface MultiUrlModalProps {
  isOpen: boolean;
  onClose: () => void;
  onQueueSuccess: () => void;
  onNavigateToQueue: () => void;
}

export const MultiUrlModal: React.FC<MultiUrlModalProps> = ({
  isOpen,
  onClose,
  onQueueSuccess,
  onNavigateToQueue,
}) => {
  const [rawText, setRawText] = useState('');
  const [detectedUrls, setDetectedUrls] = useState<string[]>([]);
  const [isAnalyzing, setIsAnalyzing] = useState(false);
  const [analyzedItems, setAnalyzedItems] = useState<BatchAnalysisItem[]>([]);
  const [selectedIndices, setSelectedIndices] = useState<Set<number>>(new Set());
  const [selectedFormats, setSelectedFormats] = useState<Record<number, string>>({}); // index -> format_id
  const [globalQuality, setGlobalQuality] = useState('720p');
  const [isQueuing, setIsQueuing] = useState(false);
  const [batchResult, setBatchResult] = useState<BatchQueueResult | null>(null);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  if (!isOpen) return null;

  const handleTextChange = async (val: string) => {
    setRawText(val);
    setErrorMsg(null);
    setBatchResult(null);
    try {
      const urls = await FreeFetchApi.parseBatchUrlsText(val);
      setDetectedUrls(urls);
    } catch {
      setDetectedUrls([]);
    }
  };

  const handlePasteClipboard = async () => {
    try {
      const text = await navigator.clipboard.readText();
      const next = rawText ? `${rawText}\n${text}` : text;
      handleTextChange(next);
    } catch {
      setErrorMsg('Unable to read clipboard.');
    }
  };

  const handleAnalyze = async () => {
    if (detectedUrls.length === 0) return;
    setIsAnalyzing(true);
    setErrorMsg(null);
    setBatchResult(null);

    try {
      const result = await FreeFetchApi.analyzeBatchUrls(detectedUrls);
      setAnalyzedItems(result.items);

      // Select all ready items by default
      const initialSelected = new Set<number>();
      const initialFormats: Record<number, string> = {};

      result.items.forEach((item, idx) => {
        if (item.status === 'ready' && item.formats.length > 0) {
          initialSelected.add(idx);
          // Pick default matching globalQuality
          const matched =
            item.formats.find(
              (f) =>
                f.format_type !== 'audio' &&
                f.quality_label.toLowerCase().includes(globalQuality.toLowerCase())
            ) ||
            item.formats.find((f) => f.format_type !== 'audio') ||
            item.formats[0];
          if (matched) {
            initialFormats[idx] = matched.format_id;
          }
        }
      });

      setSelectedIndices(initialSelected);
      setSelectedFormats(initialFormats);
    } catch (e: unknown) {
      setErrorMsg(String(e));
    } finally {
      setIsAnalyzing(false);
    }
  };

  const handleGlobalQualityChange = (q: string) => {
    setGlobalQuality(q);
    const updatedFormats = { ...selectedFormats };

    analyzedItems.forEach((item, idx) => {
      if (item.status === 'ready' && item.formats.length > 0) {
        if (q.startsWith('audio')) {
          const audioFmt =
            item.formats.find((f) => f.format_type === 'audio') ||
            item.formats.find((f) => f.matching_audio_url != null) ||
            item.formats[0];
          if (audioFmt) updatedFormats[idx] = audioFmt.format_id;
        } else if (q === 'best') {
          const videoFmts = item.formats.filter((f) => f.format_type !== 'audio');
          videoFmts.sort((a, b) => (b.height ?? 0) - (a.height ?? 0));
          const best = videoFmts[0] || item.formats[0];
          if (best) updatedFormats[idx] = best.format_id;
        } else {
          const matched =
            item.formats.find(
              (f) =>
                f.format_type !== 'audio' &&
                f.quality_label.toLowerCase().includes(q.toLowerCase())
            ) ||
            item.formats.find((f) => f.format_type !== 'audio') ||
            item.formats[0];
          if (matched) updatedFormats[idx] = matched.format_id;
        }
      }
    });

    setSelectedFormats(updatedFormats);
  };

  const toggleSelectAll = () => {
    const readyIndices = analyzedItems
      .map((item, idx) => (item.status === 'ready' ? idx : -1))
      .filter((i) => i !== -1);

    if (selectedIndices.size === readyIndices.length) {
      setSelectedIndices(new Set());
    } else {
      setSelectedIndices(new Set(readyIndices));
    }
  };

  const toggleItem = (idx: number) => {
    const next = new Set(selectedIndices);
    if (next.has(idx)) {
      next.delete(idx);
    } else {
      next.add(idx);
    }
    setSelectedIndices(next);
  };

  const handleAddToQueue = async () => {
    if (selectedIndices.size === 0) return;
    setIsQueuing(true);
    setErrorMsg(null);

    const itemsToQueue: StartDownloadParams[] = [];

    Array.from(selectedIndices)
      .sort((a, b) => a - b)
      .forEach((idx) => {
        const item = analyzedItems[idx];
        if (!item || item.status !== 'ready' || !item.metadata) return;

        const formatId = selectedFormats[idx];
        const chosenFmt =
          item.formats.find((f) => f.format_id === formatId) || item.formats[0];
        if (!chosenFmt) return;

        const isAudioTarget = globalQuality.startsWith('audio');
        const audioFormat =
          globalQuality === 'audio-m4a'
            ? 'm4a'
            : globalQuality === 'audio-flac'
            ? 'flac'
            : globalQuality === 'audio-opus'
            ? 'opus'
            : 'mp3';
        const audioOpts = isAudioTarget
          ? {
              target_format: audioFormat as any,
              quality_preset: 'best' as any,
              bitrate_mode: 'cbr' as any,
              embed_metadata: true,
              embed_artwork: audioFormat !== 'opus',
              filename_template: '{artist} - {title}.{ext}',
            }
          : undefined;

        itemsToQueue.push({
          source_url: item.metadata.url,
          service_name: item.metadata.service_name,
          title: item.metadata.title,
          duration: item.metadata.duration,
          thumbnail_url: item.metadata.thumbnail_url,
          format_id: chosenFmt.format_id,
          download_url: chosenFmt.download_url,
          requires_merge: isAudioTarget ? false : chosenFmt.requires_merge,
          matching_audio_url: isAudioTarget ? null : chosenFmt.matching_audio_url,
          container: isAudioTarget ? audioFormat : chosenFmt.container,
          quality_label: isAudioTarget
            ? `${audioFormat.toUpperCase()} (Best Audio)`
            : chosenFmt.quality_label,
          destination_dir: null,
          audio_options: audioOpts || null,
        });
      });

    try {
      const res = await FreeFetchApi.addBatchToQueue(itemsToQueue);
      setBatchResult(res);
      onQueueSuccess();
    } catch (e: unknown) {
      setErrorMsg(String(e));
    } finally {
      setIsQueuing(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-slate-950/70 backdrop-blur-sm animate-fade-in">
      <div className="bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded-2xl w-full max-w-4xl max-h-[90vh] flex flex-col shadow-2xl overflow-hidden text-slate-900 dark:text-slate-100">
        {/* Modal Header */}
        <div className="px-6 py-4 border-b border-slate-200 dark:border-slate-800 flex items-center justify-between bg-slate-50/60 dark:bg-slate-900/60">
          <div className="flex items-center gap-2.5">
            <div className="w-8 h-8 rounded-lg bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 flex items-center justify-center border border-emerald-500/20">
              <ListPlus className="w-4 h-4" />
            </div>
            <div>
              <h2 className="text-base font-bold text-slate-900 dark:text-white">Add Multiple URLs</h2>
              <p className="text-xs text-slate-500 dark:text-slate-400">
                Paste one link per line to batch analyze and add to download queue
              </p>
            </div>
          </div>
          <button
            onClick={onClose}
            className="text-slate-400 hover:text-slate-700 hover:bg-slate-100 dark:hover:text-slate-200 dark:hover:bg-slate-800 p-1.5 rounded-lg transition-colors"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Modal Body */}
        <div className="flex-1 overflow-y-auto p-6 space-y-6">
          {/* Step 1: Textarea Input */}
          {analyzedItems.length === 0 && (
            <div className="space-y-4">
              <div className="flex items-center justify-between">
                <label className="text-xs font-semibold text-slate-700 dark:text-slate-300 uppercase tracking-wider">
                  Paste Media Links (One per line)
                </label>
                <div className="flex items-center gap-2">
                  <button
                    type="button"
                    onClick={handlePasteClipboard}
                    className="text-xs text-emerald-600 dark:text-emerald-400 hover:text-emerald-700 dark:hover:text-emerald-300 font-medium px-2 py-1 rounded bg-emerald-50 dark:bg-emerald-500/10 border border-emerald-200 dark:border-emerald-500/20 hover:bg-emerald-100 dark:hover:bg-emerald-500/20 transition-all shadow-xs"
                  >
                    Paste from Clipboard
                  </button>
                  {rawText && (
                    <button
                      type="button"
                      onClick={() => handleTextChange('')}
                      className="text-xs text-slate-600 hover:text-slate-900 px-2 py-1 rounded bg-slate-100 hover:bg-slate-200 border border-slate-300 dark:bg-slate-800 dark:hover:bg-slate-700 dark:text-slate-400 dark:hover:text-slate-200 dark:border-transparent shadow-xs"
                    >
                      Clear
                    </button>
                  )}
                </div>
              </div>

              <textarea
                value={rawText}
                onChange={(e) => handleTextChange(e.target.value)}
                placeholder={`https://www.youtube.com/watch?v=...\nhttps://youtu.be/...\nhttps://example.com/video.mp4`}
                rows={8}
                disabled={isAnalyzing}
                className="w-full font-mono text-xs bg-slate-50 dark:bg-slate-950/80 border border-slate-300 dark:border-slate-700 rounded-xl p-4 text-slate-900 dark:text-slate-100 placeholder:text-slate-400 dark:placeholder:text-slate-600 focus:outline-none focus:ring-2 focus:ring-emerald-500/40 focus:border-emerald-500 transition-all resize-none"
              />

              <div className="flex items-center justify-between text-xs text-slate-500 dark:text-slate-400">
                <span>
                  Detected unique URLs:{' '}
                  <strong className="text-emerald-600 dark:text-emerald-400 font-semibold">
                    {detectedUrls.length}
                  </strong>
                </span>
                <span>Duplicates and blank lines are automatically filtered.</span>
              </div>
            </div>
          )}

          {/* Analysis Progress */}
          {isAnalyzing && (
            <div className="p-6 bg-slate-50 dark:bg-slate-800/40 border border-slate-200 dark:border-slate-800 rounded-xl space-y-3">
              <div className="flex items-center justify-between text-sm text-slate-800 dark:text-slate-200">
                <span className="flex items-center gap-2">
                  <div className="w-4 h-4 border-2 border-emerald-600 dark:border-emerald-500 border-t-transparent rounded-full animate-spin" />
                  <span>Analyzing {detectedUrls.length} URLs asynchronously...</span>
                </span>
                <span className="text-xs text-slate-500 dark:text-slate-400">Bounded concurrency: 3</span>
              </div>
              <ProgressBar percent={50} status="downloading" />
            </div>
          )}

          {/* Step 2: Analyzed Results List */}
          {analyzedItems.length > 0 && (
            <div className="space-y-4">
              {/* Batch Controls Toolbar */}
              <div className="flex flex-wrap items-center justify-between gap-3 p-3 bg-slate-50 dark:bg-slate-800/50 border border-slate-200 dark:border-slate-800 rounded-xl">
                <div className="flex items-center gap-2">
                  <button
                    type="button"
                    onClick={toggleSelectAll}
                    className="flex items-center gap-1.5 text-xs font-semibold px-2.5 py-1.5 rounded-lg bg-white hover:bg-slate-100 text-slate-700 border border-slate-300 dark:bg-slate-800 dark:hover:bg-slate-700 dark:text-slate-200 dark:border-slate-700 transition-all shadow-xs"
                  >
                    {selectedIndices.size ===
                    analyzedItems.filter((i) => i.status === 'ready').length ? (
                      <>
                        <CheckSquare className="w-3.5 h-3.5 text-emerald-600 dark:text-emerald-400" />
                        <span>Clear All</span>
                      </>
                    ) : (
                      <>
                        <Square className="w-3.5 h-3.5 text-slate-400" />
                        <span>Select All Ready</span>
                      </>
                    )}
                  </button>
                  <span className="text-xs text-slate-600 dark:text-slate-400">
                    Selected:{' '}
                    <strong className="text-emerald-600 dark:text-emerald-400 font-semibold">
                      {selectedIndices.size}
                    </strong>{' '}
                    / {analyzedItems.filter((i) => i.status === 'ready').length} ready
                  </span>
                </div>

                <div className="flex items-center gap-2 text-xs">
                  <span className="text-slate-600 dark:text-slate-400 font-medium">Apply Quality to All:</span>
                  <select
                    value={globalQuality}
                    onChange={(e) => handleGlobalQualityChange(e.target.value)}
                    className="bg-white dark:bg-slate-800 border border-slate-300 dark:border-slate-700 text-slate-900 dark:text-slate-200 rounded-lg px-2.5 py-1.5 text-xs font-semibold focus:outline-none focus:ring-2 focus:ring-emerald-500/40"
                  >
                    <option value="best">Best Quality</option>
                    <option value="1080p">1080p</option>
                    <option value="720p">720p</option>
                    <option value="480p">480p</option>
                    <option value="audio-mp3">Audio: MP3 (320k)</option>
                    <option value="audio-m4a">Audio: M4A (AAC)</option>
                    <option value="audio-flac">Audio: FLAC (Lossless)</option>
                    <option value="audio-opus">Audio: Opus</option>
                  </select>
                </div>
              </div>

              {/* Items List */}
              <div className="space-y-2 max-h-[45vh] overflow-y-auto pr-1">
                {analyzedItems.map((item, idx) => {
                  const isSelected = selectedIndices.has(idx);
                  const isReady = item.status === 'ready';
                  const chosenFormatId = selectedFormats[idx] || '';

                  return (
                    <div
                      key={idx}
                      className={`flex items-center gap-3 p-3 rounded-xl border transition-all ${
                        isReady
                          ? isSelected
                            ? 'bg-emerald-50/70 border-emerald-400/80 dark:bg-slate-800/80 dark:border-emerald-500/40 shadow-xs'
                            : 'bg-white hover:bg-slate-50 border-slate-200 dark:bg-slate-800/30 dark:border-slate-800 dark:hover:border-slate-700'
                          : 'bg-rose-50 border-rose-200 dark:bg-rose-950/10 dark:border-rose-900/30 opacity-80'
                      }`}
                    >
                      {/* Checkbox */}
                      <button
                        type="button"
                        disabled={!isReady}
                        onClick={() => toggleItem(idx)}
                        className="text-slate-400 hover:text-slate-700 dark:hover:text-white disabled:opacity-30 disabled:cursor-not-allowed"
                      >
                        {isSelected ? (
                          <CheckSquare className="w-5 h-5 text-emerald-600 dark:text-emerald-400" />
                        ) : (
                          <Square className="w-5 h-5 text-slate-400 dark:text-slate-500" />
                        )}
                      </button>

                      {/* Thumbnail / Status icon */}
                      {item.metadata?.thumbnail_url ? (
                        <img
                          src={item.metadata.thumbnail_url}
                          alt=""
                          className="w-16 h-10 object-cover rounded-md bg-slate-100 dark:bg-slate-950 shrink-0 border border-slate-200 dark:border-slate-700/50 shadow-xs"
                        />
                      ) : (
                        <div className="w-16 h-10 rounded-md bg-slate-100 dark:bg-slate-950 shrink-0 border border-slate-200 dark:border-slate-700/50 flex items-center justify-center text-slate-400 dark:text-slate-500">
                          {isReady ? (
                            <CheckCircle2 className="w-4 h-4 text-emerald-600 dark:text-emerald-500" />
                          ) : (
                            <AlertCircle className="w-4 h-4 text-rose-500" />
                          )}
                        </div>
                      )}

                      {/* Details */}
                      <div className="flex-1 min-w-0">
                        <div className="flex items-center gap-2">
                          <h4 className="text-xs font-semibold text-slate-900 dark:text-slate-100 truncate">
                            {item.metadata?.title || item.original_url}
                          </h4>
                          <span
                            className={`text-[10px] px-1.5 py-0.5 rounded uppercase font-bold shrink-0 ${
                              isReady
                                ? 'bg-emerald-100 text-emerald-800 border border-emerald-300 dark:bg-emerald-500/10 dark:text-emerald-400 dark:border-emerald-500/20'
                                : 'bg-rose-100 text-rose-800 border border-rose-300 dark:bg-rose-500/10 dark:text-rose-400 dark:border-rose-500/20'
                            }`}
                          >
                            {item.status}
                          </span>
                        </div>
                        <p className="text-[11px] text-slate-500 truncate mt-0.5">
                          {item.error_message || item.original_url}
                        </p>
                      </div>

                      {/* Format Selector */}
                      {isReady && item.formats.length > 0 && (
                        <div className="shrink-0">
                          <select
                            value={chosenFormatId}
                            onChange={(e) => {
                              setSelectedFormats({
                                ...selectedFormats,
                                [idx]: e.target.value,
                              });
                            }}
                            className="bg-white dark:bg-slate-900 border border-slate-300 dark:border-slate-700 text-slate-900 dark:text-slate-200 rounded-lg px-2 py-1 text-xs font-medium focus:outline-none focus:ring-2 focus:ring-emerald-500/40"
                          >
                            {item.formats.map((f) => (
                              <option key={f.format_id} value={f.format_id}>
                                {f.quality_label} ({f.container.toUpperCase()})
                              </option>
                            ))}
                          </select>
                        </div>
                      )}
                    </div>
                  );
                })}
              </div>
            </div>
          )}

          {/* Import Result Summary Card */}
          {batchResult && (
            <div className="p-4 bg-emerald-50 dark:bg-emerald-950/20 border border-emerald-200 dark:border-emerald-500/30 rounded-xl space-y-3 animate-fade-in">
              <div className="flex items-center gap-2 text-emerald-700 dark:text-emerald-400 font-semibold text-sm">
                <CheckCircle2 className="w-4 h-4" />
                <span>Batch Addition Complete</span>
              </div>
              <div className="grid grid-cols-2 sm:grid-cols-4 gap-2 text-xs">
                <div className="bg-white dark:bg-slate-900/60 p-2.5 rounded-lg border border-slate-200 dark:border-slate-800 shadow-xs">
                  <span className="text-slate-500 dark:text-slate-400 block">Added to Queue</span>
                  <span className="text-emerald-600 dark:text-emerald-400 font-bold text-base">
                    {batchResult.added_count}
                  </span>
                </div>
                <div className="bg-white dark:bg-slate-900/60 p-2.5 rounded-lg border border-slate-200 dark:border-slate-800 shadow-xs">
                  <span className="text-slate-500 dark:text-slate-400 block">Duplicates Skipped</span>
                  <span className="text-amber-600 dark:text-amber-400 font-bold text-base">
                    {batchResult.duplicate_count}
                  </span>
                </div>
                <div className="bg-white dark:bg-slate-900/60 p-2.5 rounded-lg border border-slate-200 dark:border-slate-800 shadow-xs">
                  <span className="text-slate-500 dark:text-slate-400 block">Unavailable / Skipped</span>
                  <span className="text-slate-700 dark:text-slate-400 font-bold text-base">
                    {batchResult.unavailable_count}
                  </span>
                </div>
                <div className="bg-white dark:bg-slate-900/60 p-2.5 rounded-lg border border-slate-200 dark:border-slate-800 shadow-xs">
                  <span className="text-slate-500 dark:text-slate-400 block">Failed</span>
                  <span className="text-rose-600 dark:text-rose-400 font-bold text-base">
                    {batchResult.failed_count}
                  </span>
                </div>
              </div>
              <div className="pt-2 flex justify-end">
                <button
                  type="button"
                  onClick={() => {
                    onClose();
                    onNavigateToQueue();
                  }}
                  className="flex items-center gap-1.5 px-3.5 py-1.5 rounded-lg bg-emerald-600 hover:bg-emerald-500 text-white text-xs font-semibold transition-all shadow-md shadow-emerald-600/20"
                >
                  <span>View in Queue</span>
                  <ArrowRight className="w-3.5 h-3.5" />
                </button>
              </div>
            </div>
          )}

          {errorMsg && (
            <div className="p-3 bg-rose-50 dark:bg-rose-500/10 border border-rose-200 dark:border-rose-500/20 text-rose-700 dark:text-rose-400 text-xs rounded-xl flex items-center gap-2">
              <AlertCircle className="w-4 h-4 shrink-0" />
              <span>{errorMsg}</span>
            </div>
          )}
        </div>

        {/* Modal Footer */}
        <div className="px-6 py-4 border-t border-slate-200 dark:border-slate-800 bg-slate-50/60 dark:bg-slate-900/60 flex items-center justify-between">
          <div>
            {analyzedItems.length > 0 && !batchResult && (
              <button
                type="button"
                onClick={() => {
                  setAnalyzedItems([]);
                  setSelectedIndices(new Set());
                }}
                className="text-xs text-slate-600 hover:text-slate-900 dark:text-slate-400 dark:hover:text-slate-200 transition-colors font-medium"
              >
                ← Back to Edit URLs
              </button>
            )}
          </div>

          <div className="flex items-center gap-3">
            <button
              type="button"
              onClick={onClose}
              className="px-4 py-2 rounded-xl text-xs font-semibold text-slate-600 hover:text-slate-900 hover:bg-slate-200/70 dark:text-slate-400 dark:hover:text-slate-200 dark:hover:bg-slate-800 transition-colors"
            >
              Close
            </button>

            {analyzedItems.length === 0 ? (
              <button
                type="button"
                disabled={isAnalyzing || detectedUrls.length === 0}
                onClick={handleAnalyze}
                className="flex items-center gap-2 px-5 py-2.5 rounded-xl bg-emerald-600 hover:bg-emerald-500 text-white text-xs font-semibold transition-all shadow-lg shadow-emerald-600/20 disabled:opacity-50 disabled:cursor-not-allowed"
              >
                <Layers className="w-4 h-4" />
                <span>Analyze {detectedUrls.length} URLs</span>
              </button>
            ) : !batchResult ? (
              <button
                type="button"
                disabled={isQueuing || selectedIndices.size === 0}
                onClick={handleAddToQueue}
                className="flex items-center gap-2 px-5 py-2.5 rounded-xl bg-emerald-600 hover:bg-emerald-500 text-white text-xs font-semibold transition-all shadow-lg shadow-emerald-600/20 disabled:opacity-50 disabled:cursor-not-allowed"
              >
                {isQueuing ? (
                  <>
                    <div className="w-3.5 h-3.5 border-2 border-white/30 border-t-white rounded-full animate-spin" />
                    <span>Adding to Queue...</span>
                  </>
                ) : (
                  <>
                    <ListPlus className="w-4 h-4" />
                    <span>Add {selectedIndices.size} Selected to Queue</span>
                  </>
                )}
              </button>
            ) : null}
          </div>
        </div>
      </div>
    </div>
  );
};
