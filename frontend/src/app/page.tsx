'use client';
import { useCallback, useEffect, useState } from 'react';
import { MetricCard } from '@/components/dashboard/MetricCard';
import { ActivePositions } from '@/components/dashboard/ActivePositions';
import { api } from '@/lib/api';
import { formatCurrency, formatPercent } from '@/lib/utils';
import { useAccounts } from '@/contexts/AccountContext';
import type { DashboardData } from '@/lib/types';

export default function DashboardPage() {
  const { selectedAccountId } = useAccounts();
  const [data, setData] = useState<DashboardData | null>(null);

  const refreshDashboard = useCallback(() => {
    api.dashboard(selectedAccountId ?? undefined).then(setData);
  }, [selectedAccountId]);

  useEffect(() => {
    refreshDashboard();
  }, [refreshDashboard]);

  if (!data) return <div className="text-muted-foreground">Loading...</div>;

  return (
    <div className="space-y-6">
      <h1 className="text-2xl font-bold">Dashboard</h1>
      <div className="grid grid-cols-1 sm:grid-cols-4 gap-4">
        <MetricCard title="Total Premium Collected" value={formatCurrency(data.total_premium_collected)} subtitle="All closed trades" />
        <MetricCard title="Capital Deployed" value={formatCurrency(data.total_capital_deployed)} subtitle="Open positions" />
        <MetricCard title="Realized Yield (Ann.)" value={formatPercent(data.realized_annualized_yield)} subtitle="Closed trades" />
        <MetricCard title="Open Yield (Ann.)" value={formatPercent(data.open_annualized_yield)} subtitle="Current open trades" />
      </div>
      <ActivePositions openTrades={data.open_trades} activeLots={data.active_share_lots} onTradeClose={refreshDashboard} />
    </div>
  );
}
