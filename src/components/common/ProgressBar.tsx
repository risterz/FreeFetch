import React from 'react';

interface ProgressBarProps {
  percent: number;
  status: string;
}

export const ProgressBar: React.FC<ProgressBarProps> = ({ percent, status }) => {
  const clamped = Math.min(100, Math.max(0, percent));

  const getStatusColor = () => {
    switch (status) {
      case 'completed':
        return 'bg-emerald-500';
      case 'failed':
        return 'bg-rose-500';
      case 'paused':
        return 'bg-amber-500';
      case 'retrying':
        return 'bg-amber-400 animate-pulse';
      case 'processing':
        return 'bg-indigo-500 animate-pulse';
      case 'cancelled':
        return 'bg-slate-500';
      default:
        return 'bg-blue-500';
    }
  };

  return (
    <div className="w-full bg-slate-200 dark:bg-slate-800/80 rounded-full h-2 overflow-hidden">
      <div
        className={`h-full transition-all duration-300 ease-out ${getStatusColor()}`}
        style={{ width: `${clamped}%` }}
      />
    </div>
  );
};
