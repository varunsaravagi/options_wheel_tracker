'use client';
import { useCallback, useEffect, useState } from 'react';
import { FilterBar } from '@/components/history/FilterBar';
import { TradeTable } from '@/components/history/TradeTable';
import { api } from '@/lib/api';
import { useAccounts } from '@/contexts/AccountContext';
import type { HistoryFilters, Trade } from '@/lib/types';

export default function HistoryPage() {
  const { selectedAccountId } = useAccounts();
  const [trades, setTrades] = useState<Trade[]>([]);
  const [filters, setFilters] = useState<HistoryFilters>({});

  const load = useCallback((f: HistoryFilters) => {
    api.history({ ...f, account_id: selectedAccountId ?? undefined }).then(setTrades);
  }, [selectedAccountId]);

  useEffect(() => { load(filters); }, [load, filters]);

  const handleFilterChange = (f: HistoryFilters) => {
    setFilters(f);
  };

  return (
    <div className="space-y-6">
      <h1 className="text-2xl font-bold">Trade History</h1>
      <FilterBar filters={filters} onChange={handleFilterChange} />
      <TradeTable trades={trades} onTradeUpdate={() => load(filters)} />
    </div>
  );
}
