'use client';
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { getDateRangePreset } from '@/lib/utils';
import type { HistoryFilters } from '@/lib/types';

const CURRENT_YEAR = new Date().getFullYear();
const PRESETS = ['30d', '60d', '90d', 'ytd', String(CURRENT_YEAR), String(CURRENT_YEAR - 1)];

interface Props {
  filters: HistoryFilters;
  onChange: (f: HistoryFilters) => void;
}

export function FilterBar({ filters, onChange }: Props) {
  const [ticker, setTicker] = useState(filters.ticker ?? '');
  const [customFrom, setCustomFrom] = useState('');
  const [customTo, setCustomTo] = useState('');
  const [activePreset, setActivePreset] = useState('');

  const applyPreset = (preset: string) => {
    setActivePreset(preset);
    const range = getDateRangePreset(preset);
    onChange({ ...filters, date_from: range.date_from, date_to: range.date_to });
  };

  const applyCustom = () => {
    setActivePreset('custom');
    onChange({ ...filters, date_from: customFrom || undefined, date_to: customTo || undefined });
  };

  const applyTicker = () => onChange({ ...filters, ticker: ticker.toUpperCase() || undefined });

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap gap-2">
        {PRESETS.map((p) => (
          <Button key={p} size="sm"
            variant={activePreset === p ? 'default' : 'outline'}
            onClick={() => applyPreset(p)}>
            {p === 'ytd' ? 'YTD' : p}
          </Button>
        ))}
        <Button size="sm" variant={activePreset === '' ? 'default' : 'outline'} onClick={() => { setActivePreset(''); onChange({ ...filters, date_from: undefined, date_to: undefined }); }}>
          All Time
        </Button>
      </div>
      <div className="flex gap-2 items-end">
        <div>
          <p className="text-xs text-muted-foreground mb-1">From</p>
          <Input type="date" value={customFrom} onChange={(e) => setCustomFrom(e.target.value)} className="w-36" />
        </div>
        <div>
          <p className="text-xs text-muted-foreground mb-1">To</p>
          <Input type="date" value={customTo} onChange={(e) => setCustomTo(e.target.value)} className="w-36" />
        </div>
        <Button size="sm" variant="outline" onClick={applyCustom}>Apply Range</Button>
      </div>
      <div className="flex gap-2">
        <Input placeholder="Filter by ticker (AAPL)" value={ticker}
          onChange={(e) => setTicker(e.target.value)}
          onKeyDown={(e) => e.key === 'Enter' && applyTicker()}
          className="w-48" />
        <Button size="sm" variant="outline" onClick={applyTicker}>Search</Button>
        {(filters.ticker) && (
          <Button size="sm" variant="ghost" onClick={() => { setTicker(''); onChange({ ...filters, ticker: undefined }); }}>
            Clear
          </Button>
        )}
      </div>
    </div>
  );
}
