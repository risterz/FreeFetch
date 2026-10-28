import React from 'react';
import { X, ListVideo, Play, ArrowRight } from 'lucide-react';
import { PlaylistDetection } from '../../api/types';

interface PlaylistPromptModalProps {
  isOpen: boolean;
  detection: PlaylistDetection | null;
  onClose: () => void;
  onViewPlaylist: () => void;
  onAnalyzeSingle: () => void;
}

export const PlaylistPromptModal: React.FC<PlaylistPromptModalProps> = ({
  isOpen,
  detection,
  onClose,
  onViewPlaylist,
  onAnalyzeSingle,
}) => {
  if (!isOpen || !detection) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-slate-950/70 backdrop-blur-sm animate-fade-in">
      <div className="bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded-2xl w-full max-w-lg shadow-2xl overflow-hidden text-slate-900 dark:text-slate-100">
        {/* Header */}
        <div className="px-6 py-4 border-b border-slate-200 dark:border-slate-800 flex items-center justify-between bg-slate-50/60 dark:bg-slate-900/60">
          <div className="flex items-center gap-2.5">
            <div className="w-8 h-8 rounded-lg bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 flex items-center justify-center border border-emerald-500/20">
              <ListVideo className="w-4 h-4" />
            </div>
            <h2 className="text-base font-bold text-slate-900 dark:text-white">Playlist Detected</h2>
          </div>
          <button
            onClick={onClose}
            className="text-slate-400 hover:text-slate-700 hover:bg-slate-100 dark:hover:text-slate-200 dark:hover:bg-slate-800 p-1.5 rounded-lg transition-colors"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Body */}
        <div className="p-6 space-y-4">
          <p className="text-xs text-slate-600 dark:text-slate-300 leading-relaxed">
            This link contains a playlist reference ({detection.playlist_id}). Would you
            like to discover all videos in the playlist or analyze the video directly?
          </p>

          <div className="space-y-3 pt-2">
            <button
              type="button"
              onClick={onViewPlaylist}
              className="w-full flex items-center justify-between p-3.5 rounded-xl bg-emerald-50 hover:bg-emerald-100/80 border border-emerald-200 text-emerald-900 dark:bg-emerald-600/10 dark:border-emerald-500/30 dark:hover:bg-emerald-600/20 dark:text-emerald-300 transition-all text-left group shadow-xs"
            >
              <div className="flex items-center gap-3">
                <div className="w-8 h-8 rounded-lg bg-emerald-500/20 text-emerald-600 dark:text-emerald-400 flex items-center justify-center shrink-0">
                  <ListVideo className="w-4 h-4" />
                </div>
                <div>
                  <h4 className="text-xs font-bold text-slate-900 dark:text-white group-hover:text-emerald-700 dark:group-hover:text-emerald-300 transition-colors">
                    View Entire Playlist
                  </h4>
                  <p className="text-[11px] text-slate-500 dark:text-slate-400">
                    Discover and select tracks to queue in batch
                  </p>
                </div>
              </div>
              <ArrowRight className="w-4 h-4 text-emerald-600 dark:text-emerald-400 opacity-70 group-hover:opacity-100 group-hover:translate-x-0.5 transition-all" />
            </button>

            {detection.video_id && (
              <button
                type="button"
                onClick={onAnalyzeSingle}
                className="w-full flex items-center justify-between p-3.5 rounded-xl bg-white hover:bg-slate-50 border border-slate-200 dark:bg-slate-800/40 dark:border-slate-700/60 dark:hover:bg-slate-800 text-slate-700 dark:text-slate-300 transition-all text-left group shadow-xs"
              >
                <div className="flex items-center gap-3">
                  <div className="w-8 h-8 rounded-lg bg-slate-100 text-slate-600 dark:bg-slate-700/50 dark:text-slate-300 flex items-center justify-center shrink-0">
                    <Play className="w-4 h-4" />
                  </div>
                  <div>
                    <h4 className="text-xs font-bold text-slate-900 dark:text-white group-hover:text-indigo-600 dark:group-hover:text-slate-100 transition-colors">
                      Analyze This Video Only
                    </h4>
                    <p className="text-[11px] text-slate-500 dark:text-slate-400">
                      Ignore playlist context and download single video
                    </p>
                  </div>
                </div>
                <ArrowRight className="w-4 h-4 text-slate-400 opacity-70 group-hover:opacity-100 group-hover:translate-x-0.5 transition-all" />
              </button>
            )}
          </div>
        </div>

        {/* Footer */}
        <div className="px-6 py-3.5 border-t border-slate-200 dark:border-slate-800 bg-slate-50/60 dark:bg-slate-900/60 flex justify-end">
          <button
            type="button"
            onClick={onClose}
            className="px-4 py-2 rounded-xl text-xs font-semibold text-slate-600 hover:text-slate-900 hover:bg-slate-200/70 dark:text-slate-400 dark:hover:text-slate-200 dark:hover:bg-slate-800 transition-colors"
          >
            Cancel
          </button>
        </div>
      </div>
    </div>
  );
};
