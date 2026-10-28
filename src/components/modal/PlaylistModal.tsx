import React, { useState } from 'react';
import {
  X,
  ListVideo,
  CheckSquare,
  Square,
  AlertCircle,
  CheckCircle2,
  Lock,
  Trash2,
  Clock,
  ArrowRight,
  Plus,
} from 'lucide-react';
import { BatchQueueResult, PlaylistInfo, PlaylistItemSelection } from '../../api/types';
import { FreeFetchApi } from '../../api/ipc';

interface PlaylistModalProps {
  isOpen: boolean;
  playlist: PlaylistInfo | null;
  onClose: () => void;
  onQueueSuccess: () => void;
  onNavigateToQueue: () => void;
}

export const PlaylistModal: React.FC<PlaylistModalProps> = ({
  isOpen,
  playlist,
  onClose,
  onQueueSuccess,
  onNavigateToQueue,
}) => {
  const [selectedIds, setSelectedIds] = useState<Set<string>>(() => {
    if (!playlist) return new Set();
    // Default select all available items
    return new Set(
      playlist.items.filter((i) => i.availability === 'available').map((i) => i.id)
    );
  });
  const [qualityPref, setQualityPref] = useState('720p');
  const [isQueuing, setIsQueuing] = useState(false);
  const [queueResult, setQueueResult] = useState<BatchQueueResult | null>(null);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  if (!isOpen || !playlist) return null;

  const availableItems = playlist.items.filter((i) => i.availability === 'available');
  const privateCount = playlist.items.filter((i) => i.availability === 'private').length;
  const deletedCount = playlist.items.filter((i) => i.availability === 'deleted').length;
  const unavailableCount = playlist.items.filter(
    (i) => i.availability === 'unavailable'
  ).length;

  const selectAll = () => {
    setSelectedIds(new Set(availableItems.map((i) => i.id)));
  };

  const selectNone = () => {
    setSelectedIds(new Set());
  };

  const toggleItem = (id: string) => {
    const next = new Set(selectedIds);
    if (next.has(id)) {
      next.delete(id);
    } else {
      next.add(id);
    }
    setSelectedIds(next);
  };

  const formatDuration = (seconds: number) => {
    if (!seconds) return '--:--';
    const mins = Math.floor(seconds / 60);
    const secs = seconds % 60;
    return `${mins}:${secs.toString().padStart(2, '0')}`;
  };

  const handleAddToQueue = async () => {
    if (selectedIds.size === 0) return;
    setIsQueuing(true);
    setErrorMsg(null);
    setQueueResult(null);

    // Filter and map selected items in playlist order
    const selectedList: PlaylistItemSelection[] = playlist.items
      .filter((item) => selectedIds.has(item.id))
      .map((item) => ({
        id: item.id,
        url: item.url,
        title: item.title,
        duration: item.duration,
        thumbnail: item.thumbnail,
        format_id: null,
      }));

    try {
      const res = await FreeFetchApi.addPlaylistToQueue(selectedList, qualityPref);
      setQueueResult(res);
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
          <div className="flex items-center gap-3">
            <div className="w-10 h-10 rounded-xl bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 flex items-center justify-center border border-emerald-500/20 shrink-0">
              <ListVideo className="w-5 h-5" />
            </div>
            <div className="min-w-0">
              <h2 className="text-base font-bold text-slate-900 dark:text-white truncate max-w-lg">
                {playlist.title}
              </h2>
              <div className="flex flex-wrap items-center gap-2 mt-0.5 text-xs text-slate-500 dark:text-slate-400">
                {playlist.uploader && <span>by {playlist.uploader}</span>}
                <span>•</span>
                <span>{playlist.item_count} items</span>
                <span>•</span>
                <span className="text-emerald-600 dark:text-emerald-400 font-semibold">
                  {availableItems.length} available
                </span>
                {privateCount > 0 && (
                  <span className="text-amber-600 dark:text-amber-400 font-medium">
                    • {privateCount} private
                  </span>
                )}
                {deletedCount > 0 && (
                  <span className="text-rose-600 dark:text-rose-400 font-medium">
                    • {deletedCount} deleted
                  </span>
                )}
                {unavailableCount > 0 && (
                  <span className="text-slate-500 font-medium">
                    • {unavailableCount} unavailable
                  </span>
                )}
              </div>
            </div>
          </div>
          <button
            onClick={onClose}
            className="text-slate-400 hover:text-slate-700 hover:bg-slate-100 dark:hover:text-slate-200 dark:hover:bg-slate-800 p-1.5 rounded-lg transition-colors shrink-0"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Modal Body */}
        <div className="flex-1 overflow-y-auto p-6 space-y-4">
          {/* Quick Selection Toolbar */}
          <div className="flex flex-wrap items-center justify-between gap-3 p-3 bg-slate-50 dark:bg-slate-800/50 border border-slate-200 dark:border-slate-800 rounded-xl">
            <div className="flex items-center gap-2">
              <button
                type="button"
                onClick={selectAll}
                className="text-xs font-semibold px-2.5 py-1.5 rounded-lg bg-white hover:bg-slate-100 text-slate-700 border border-slate-300 dark:bg-slate-800 dark:hover:bg-slate-700 dark:text-slate-200 dark:border-slate-700 transition-all shadow-xs"
              >
                Select All ({availableItems.length})
              </button>
              <button
                type="button"
                onClick={selectNone}
                className="text-xs font-semibold px-2.5 py-1.5 rounded-lg bg-white hover:bg-slate-100 text-slate-700 border border-slate-300 dark:bg-slate-800 dark:hover:bg-slate-700 dark:text-slate-200 dark:border-slate-700 transition-all shadow-xs"
              >
                Select None
              </button>
              <span className="text-xs text-slate-600 dark:text-slate-400 ml-2">
                Selected:{' '}
                <strong className="text-emerald-600 dark:text-emerald-400 font-semibold">
                  {selectedIds.size}
                </strong>
              </span>
            </div>

            <div className="flex items-center gap-2 text-xs">
              <span className="text-slate-600 dark:text-slate-400 font-medium">Quality Preference:</span>
              <select
                value={qualityPref}
                onChange={(e) => setQualityPref(e.target.value)}
                className="bg-white dark:bg-slate-800 border border-slate-300 dark:border-slate-700 text-slate-900 dark:text-slate-200 rounded-lg px-2.5 py-1.5 text-xs font-semibold focus:outline-none focus:ring-1 focus:ring-emerald-500"
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
          <div className="space-y-2 max-h-[50vh] overflow-y-auto pr-1">
            {playlist.items.map((item) => {
              const isSelected = selectedIds.has(item.id);
              const isAvailable = item.availability === 'available';

              return (
                <div
                  key={item.id}
                  className={`flex items-center gap-3 p-2.5 rounded-xl border transition-all ${
                    isAvailable
                      ? isSelected
                        ? 'bg-emerald-50/70 border-emerald-400/80 dark:bg-slate-800/80 dark:border-emerald-500/40 shadow-xs'
                        : 'bg-white hover:bg-slate-50 border-slate-200 dark:bg-slate-800/30 dark:border-slate-800 dark:hover:border-slate-700'
                      : 'bg-slate-100/60 dark:bg-slate-950/40 border-slate-200 dark:border-slate-900 opacity-60'
                  }`}
                >
                  {/* Checkbox */}
                  <button
                    type="button"
                    disabled={!isAvailable}
                    onClick={() => toggleItem(item.id)}
                    className="text-slate-400 hover:text-slate-700 dark:hover:text-white disabled:opacity-20 disabled:cursor-not-allowed shrink-0"
                  >
                    {isSelected ? (
                      <CheckSquare className="w-5 h-5 text-emerald-600 dark:text-emerald-400" />
                    ) : (
                      <Square className="w-5 h-5 text-slate-400 dark:text-slate-500" />
                    )}
                  </button>

                  {/* Index badge */}
                  <span className="w-6 text-center font-mono text-xs text-slate-500 dark:text-slate-400 font-semibold shrink-0">
                    {item.index.toString().padStart(2, '0')}
                  </span>

                  {/* Thumbnail */}
                  {item.thumbnail ? (
                    <img
                      src={item.thumbnail}
                      alt=""
                      className="w-16 h-10 object-cover rounded-md bg-slate-100 dark:bg-slate-950 shrink-0 border border-slate-200 dark:border-slate-700/40 shadow-xs"
                    />
                  ) : (
                    <div className="w-16 h-10 rounded-md bg-slate-100 dark:bg-slate-950 shrink-0 border border-slate-200 dark:border-slate-700/40 flex items-center justify-center text-slate-400 dark:text-slate-600">
                      <ListVideo className="w-4 h-4" />
                    </div>
                  )}

                  {/* Title and metadata */}
                  <div className="flex-1 min-w-0">
                    <h4
                      className={`text-xs font-semibold truncate ${
                        isAvailable ? 'text-slate-900 dark:text-slate-100' : 'text-slate-400 line-through'
                      }`}
                    >
                      {item.title}
                    </h4>
                    <div className="flex items-center gap-3 mt-0.5 text-[11px] text-slate-500 dark:text-slate-400">
                      <span className="flex items-center gap-1">
                        <Clock className="w-3 h-3 text-slate-400 dark:text-slate-500" />
                        <span>{formatDuration(item.duration)}</span>
                      </span>

                      {item.availability !== 'available' && (
                        <span
                          className={`text-[10px] px-1.5 py-0.2 rounded uppercase font-bold flex items-center gap-1 ${
                            item.availability === 'private'
                              ? 'bg-amber-100 text-amber-800 border border-amber-300 dark:bg-amber-500/10 dark:text-amber-400 dark:border-amber-500/20'
                              : 'bg-rose-100 text-rose-800 border border-rose-300 dark:bg-rose-500/10 dark:text-rose-400 dark:border-rose-500/20'
                          }`}
                        >
                          {item.availability === 'private' && (
                            <Lock className="w-2.5 h-2.5" />
                          )}
                          {item.availability === 'deleted' && (
                            <Trash2 className="w-2.5 h-2.5" />
                          )}
                          <span>{item.availability}</span>
                        </span>
                      )}
                    </div>
                  </div>
                </div>
              );
            })}
          </div>

          {/* Import Result Summary Card */}
          {queueResult && (
            <div className="p-4 bg-emerald-50 dark:bg-emerald-950/20 border border-emerald-200 dark:border-emerald-500/30 rounded-xl space-y-3 animate-fade-in">
              <div className="flex items-center gap-2 text-emerald-700 dark:text-emerald-400 font-semibold text-sm">
                <CheckCircle2 className="w-4 h-4" />
                <span>Playlist Enqueued Successfully</span>
              </div>
              <div className="grid grid-cols-2 sm:grid-cols-4 gap-2 text-xs">
                <div className="bg-white dark:bg-slate-900/60 p-2.5 rounded-lg border border-slate-200 dark:border-slate-800 shadow-xs">
                  <span className="text-slate-500 dark:text-slate-400 block">Added to Queue</span>
                  <span className="text-emerald-600 dark:text-emerald-400 font-bold text-base">
                    {queueResult.added_count}
                  </span>
                </div>
                <div className="bg-white dark:bg-slate-900/60 p-2.5 rounded-lg border border-slate-200 dark:border-slate-800 shadow-xs">
                  <span className="text-slate-500 dark:text-slate-400 block">Duplicates Skipped</span>
                  <span className="text-amber-600 dark:text-amber-400 font-bold text-base">
                    {queueResult.duplicate_count}
                  </span>
                </div>
                <div className="bg-white dark:bg-slate-900/60 p-2.5 rounded-lg border border-slate-200 dark:border-slate-800 shadow-xs">
                  <span className="text-slate-500 dark:text-slate-400 block">Unavailable / Skipped</span>
                  <span className="text-slate-700 dark:text-slate-400 font-bold text-base">
                    {queueResult.unavailable_count}
                  </span>
                </div>
                <div className="bg-white dark:bg-slate-900/60 p-2.5 rounded-lg border border-slate-200 dark:border-slate-800 shadow-xs">
                  <span className="text-slate-500 dark:text-slate-400 block">Failed</span>
                  <span className="text-rose-600 dark:text-rose-400 font-bold text-base">
                    {queueResult.failed_count}
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
          <span className="text-xs text-slate-500 dark:text-slate-400">
            Selected items will be queued in playlist sequence
          </span>

          <div className="flex items-center gap-3">
            <button
              type="button"
              onClick={onClose}
              className="px-4 py-2 rounded-xl text-xs font-semibold text-slate-600 hover:text-slate-900 hover:bg-slate-200/70 dark:text-slate-400 dark:hover:text-slate-200 dark:hover:bg-slate-800 transition-colors"
            >
              Close
            </button>

            {!queueResult && (
              <button
                type="button"
                disabled={isQueuing || selectedIds.size === 0}
                onClick={handleAddToQueue}
                className="flex items-center gap-2 px-5 py-2.5 rounded-xl bg-emerald-600 hover:bg-emerald-500 text-white text-xs font-semibold transition-all shadow-lg shadow-emerald-600/20 disabled:opacity-50 disabled:cursor-not-allowed"
              >
                {isQueuing ? (
                  <>
                    <div className="w-3.5 h-3.5 border-2 border-white/30 border-t-white rounded-full animate-spin" />
                    <span>Resolving & Queuing...</span>
                  </>
                ) : (
                  <>
                    <Plus className="w-4 h-4" />
                    <span>Add {selectedIds.size} Selected to Queue</span>
                  </>
                )}
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
};
