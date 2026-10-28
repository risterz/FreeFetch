import React, { useState } from 'react';
import { Download, Clipboard, Search, AlertCircle, CheckCircle2, ListPlus } from 'lucide-react';

interface HeaderProps {
  onAnalyze: (url: string) => Promise<void>;
  isAnalyzing: boolean;
  ffmpegStatus: string | null;
  onOpenMultiUrl?: () => void;
}

export const Header: React.FC<HeaderProps> = ({
  onAnalyze,
  isAnalyzing,
  ffmpegStatus,
  onOpenMultiUrl,
}) => {
  const [url, setUrl] = useState('');
  const [inputError, setInputError] = useState<string | null>(null);

  const handleSubmit = async (e?: React.FormEvent) => {
    if (e) e.preventDefault();
    const trimmed = url.trim();
    if (!trimmed) {
      setInputError('Please enter a media URL');
      return;
    }
    if (!trimmed.startsWith('http://') && !trimmed.startsWith('https://')) {
      setInputError('URL must start with http:// or https://');
      return;
    }
    setInputError(null);
    await onAnalyze(trimmed);
  };

  const handlePaste = async () => {
    try {
      const text = await navigator.clipboard.readText();
      const trimmed = text.trim();
      if (trimmed.startsWith('http://') || trimmed.startsWith('https://')) {
        setUrl(trimmed);
        setInputError(null);
        await onAnalyze(trimmed);
      } else {
        setUrl(trimmed);
        setInputError('Clipboard does not contain a valid HTTP/HTTPS link');
      }
    } catch {
      setInputError('Unable to read clipboard. Please paste manually.');
    }
  };

  return (
    <header className="bg-white/95 dark:bg-slate-900/90 backdrop-blur-md border-b border-slate-200 dark:border-slate-800 px-6 py-4 sticky top-0 z-30 transition-colors">
      <div className="max-w-6xl mx-auto flex flex-col md:flex-row md:items-center justify-between gap-4">
        {/* Logo and Brand */}
        <div className="flex items-center gap-3">
          <div className="w-10 h-10 rounded-xl bg-gradient-to-tr from-emerald-600 to-teal-400 flex items-center justify-center shadow-lg shadow-emerald-500/20">
            <Download className="w-5 h-5 text-white stroke-[2.5]" />
          </div>
          <div>
            <div className="flex items-center gap-2">
              <h1 className="font-bold text-xl tracking-tight text-slate-900 dark:text-white">FreeFetch</h1>
              <span className="text-xs font-semibold px-2 py-0.5 rounded-full bg-emerald-500/10 text-emerald-700 dark:text-emerald-400 border border-emerald-500/20">
                v0.1
              </span>
            </div>
            <p className="text-xs text-slate-500 dark:text-slate-400">Clean-room High-Fidelity Desktop Downloader</p>
          </div>
        </div>

        {/* FFmpeg Badge */}
        <div className="flex items-center gap-2">
          {ffmpegStatus ? (
            <div
              className="flex items-center gap-1.5 text-xs px-2.5 py-1 rounded-full bg-emerald-50 dark:bg-slate-800 border border-emerald-200 dark:border-slate-700/60 text-emerald-800 dark:text-slate-300 font-medium shadow-xs"
              title={`FFmpeg engine ready: ${ffmpegStatus}`}
            >
              <CheckCircle2 className="w-3.5 h-3.5 text-emerald-600 dark:text-emerald-400" />
              <span>FFmpeg Sidecar Ready</span>
            </div>
          ) : (
            <div
              className="flex items-center gap-1.5 text-xs px-2.5 py-1 rounded-full bg-amber-50 dark:bg-amber-500/10 border border-amber-200 dark:border-amber-500/20 text-amber-800 dark:text-amber-300 font-medium"
              title="FFmpeg not detected. Multiplexing requires FFmpeg on host or sidecars folder."
            >
              <AlertCircle className="w-3.5 h-3.5 text-amber-600 dark:text-amber-400" />
              <span>FFmpeg Not Detected</span>
            </div>
          )}
        </div>
      </div>

      {/* URL Input Bar */}
      <div className="max-w-6xl mx-auto mt-4">
        <form onSubmit={handleSubmit} className="flex gap-2">
          <div className="relative flex-1">
            <input
              type="text"
              value={url}
              onChange={(e) => {
                setUrl(e.target.value);
                if (inputError) setInputError(null);
              }}
              placeholder="Paste public video URL (e.g. YouTube, direct MP4) or press Ctrl+V"
              className="w-full bg-slate-50 dark:bg-slate-800/80 border border-slate-300 dark:border-slate-700 rounded-xl px-4 py-2.5 text-sm text-slate-900 dark:text-slate-100 placeholder:text-slate-400 dark:placeholder:text-slate-500 focus:outline-none focus:bg-white dark:focus:bg-slate-800 focus:ring-2 focus:ring-emerald-500/40 focus:border-emerald-500 transition-all pr-12 shadow-inner"
              disabled={isAnalyzing}
            />
            {url && (
              <button
                type="button"
                onClick={() => setUrl('')}
                className="absolute right-3 top-1/2 -translate-y-1/2 text-slate-500 hover:text-slate-700 dark:text-slate-400 dark:hover:text-slate-200 text-xs px-1.5 py-0.5 rounded bg-slate-200/80 dark:bg-slate-700/50"
              >
                Clear
              </button>
            )}
          </div>

          <button
            type="button"
            onClick={handlePaste}
            disabled={isAnalyzing}
            className="flex items-center gap-1.5 px-3.5 py-2.5 bg-white hover:bg-slate-100 dark:bg-slate-800 dark:hover:bg-slate-700 border border-slate-300 dark:border-slate-700 text-slate-700 dark:text-slate-200 rounded-xl text-sm font-medium transition-colors shadow-xs disabled:opacity-50"
            title="Paste URL from Clipboard and Analyze (Ctrl+V)"
          >
            <Clipboard className="w-4 h-4 text-slate-500 dark:text-slate-400" />
            <span className="hidden sm:inline">Paste</span>
            <kbd className="hidden lg:inline text-[10px] font-mono text-slate-400 dark:text-slate-500 bg-slate-100 dark:bg-slate-750 px-1 py-0.5 rounded border border-slate-200 dark:border-slate-700">Ctrl+V</kbd>
          </button>

          {onOpenMultiUrl && (
            <button
              type="button"
              onClick={onOpenMultiUrl}
              disabled={isAnalyzing}
              className="flex items-center gap-1.5 px-3.5 py-2.5 bg-white hover:bg-slate-100 dark:bg-slate-800 dark:hover:bg-slate-700 border border-slate-300 dark:border-slate-700 text-slate-700 dark:text-slate-200 rounded-xl text-sm font-medium transition-colors shadow-xs disabled:opacity-50"
              title="Add Multiple URLs at Once"
            >
              <ListPlus className="w-4 h-4 text-emerald-600 dark:text-emerald-400" />
              <span className="hidden sm:inline">Batch URLs</span>
            </button>
          )}

          <button
            type="submit"
            disabled={isAnalyzing || !url.trim()}
            className="flex items-center gap-2 px-5 py-2.5 bg-emerald-600 hover:bg-emerald-500 text-white rounded-xl text-sm font-medium transition-all shadow-md shadow-emerald-600/20 disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {isAnalyzing ? (
              <>
                <div className="w-4 h-4 border-2 border-white/30 border-t-white rounded-full animate-spin" />
                <span>Analyzing...</span>
              </>
            ) : (
              <>
                <Search className="w-4 h-4" />
                <span>Analyze</span>
              </>
            )}
          </button>
        </form>

        {inputError && (
          <div className="flex items-center gap-1.5 mt-2 text-rose-600 dark:text-rose-400 text-xs font-medium">
            <AlertCircle className="w-3.5 h-3.5" />
            <span>{inputError}</span>
          </div>
        )}
      </div>
    </header>
  );
};
