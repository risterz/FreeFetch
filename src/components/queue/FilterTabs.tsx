import React from 'react';
import { DownloadRecord } from '../../api/types';

export type FilterType = 'all' | 'active' | 'completed' | 'failed';

interface FilterTabsProps {
  currentFilter: FilterType;
  onFilterChange: (filter: FilterType) => void;
  downloads: DownloadRecord[];
}

export const FilterTabs: React.FC<FilterTabsProps> = ({
  currentFilter,
  onFilterChange,
  downloads,
}) => {
  const activeCount = downloads.filter(
    (d) =>
      d.status === 'downloading' ||
      d.status === 'retrying' ||
      d.status === 'processing' ||
      d.status === 'queued'
  ).length;

  const completedCount = downloads.filter((d) => d.status === 'completed').length;
  const failedCount = downloads.filter((d) => d.status === 'failed' || d.status === 'cancelled').length;

  const tabs: { id: FilterType; label: string; count: number }[] = [
    { id: 'all', label: 'All Downloads', count: downloads.length },
    { id: 'active', label: 'Active', count: activeCount },
    { id: 'completed', label: 'Completed', count: completedCount },
    { id: 'failed', label: 'Failed / Cancelled', count: failedCount },
  ];

  return (
    <div className="flex items-center gap-2 border-b border-slate-800 pb-3">
      {tabs.map((tab) => (
        <button
          key={tab.id}
          onClick={() => onFilterChange(tab.id)}
          className={`flex items-center gap-2 px-3 py-1.5 rounded-lg text-xs font-semibold transition-colors ${
            currentFilter === tab.id
              ? 'bg-slate-800 text-white shadow-sm'
              : 'text-slate-400 hover:text-slate-200 hover:bg-slate-850'
          }`}
        >
          <span>{tab.label}</span>
          <span
            className={`px-1.5 py-0.2 rounded-full text-[10px] ${
              currentFilter === tab.id
                ? 'bg-emerald-500/20 text-emerald-300'
                : 'bg-slate-800/80 text-slate-400'
            }`}
          >
            {tab.count}
          </span>
        </button>
      ))}
    </div>
  );
};
