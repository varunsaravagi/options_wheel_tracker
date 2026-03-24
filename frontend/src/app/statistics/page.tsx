'use client';
import { useCallback, useEffect, useState } from 'react';
import { MetricCard } from '@/components/dashboard/MetricCard';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { api } from '@/lib/api';
import { formatCurrency, formatPercent } from '@/lib/utils';
import { useAccounts } from '@/contexts/AccountContext';
import type { StatisticsData } from '@/lib/types';

function MonthlyIncomeChart({ data }: { data: StatisticsData }) {
  const items = data.monthly_income;
  if (items.length === 0) return <p className="text-muted-foreground text-sm">No data yet</p>;

  const maxVal = Math.max(...items.map(d => Math.max(d.sto_income, d.btc_cost, Math.abs(d.net_income))), 1);
  const chartH = 240;
  const barW = Math.min(60, Math.max(20, 600 / items.length / 3));
  const gap = barW * 0.3;
  const groupW = barW * 2 + gap * 3;
  const chartW = Math.max(items.length * groupW + 60, 300);

  // Net income line points
  const linePoints = items.map((d, i) => {
    const x = 40 + i * groupW + groupW / 2;
    const y = chartH - (d.net_income / maxVal) * (chartH - 40) - 20;
    return `${x},${y}`;
  }).join(' ');

  return (
    <div className="overflow-x-auto">
      <svg width={chartW} height={chartH + 50} className="text-xs">
        {/* Y axis line */}
        <line x1="38" y1="10" x2="38" y2={chartH - 18} stroke="currentColor" strokeOpacity="0.2" />

        {/* Y axis labels */}
        {[0, 0.25, 0.5, 0.75, 1].map((frac) => {
          const y = chartH - 20 - frac * (chartH - 40);
          const val = frac * maxVal;
          return (
            <g key={frac}>
              <line x1="38" x2={chartW} y1={y} y2={y} stroke="currentColor" strokeOpacity="0.08" />
              <text x="35" y={y + 4} textAnchor="end" fill="currentColor" opacity="0.5" fontSize="10">
                {val >= 1000 ? `${(val / 1000).toFixed(1)}k` : val.toFixed(0)}
              </text>
            </g>
          );
        })}

        {items.map((d, i) => {
          const x = 40 + i * groupW + gap;
          const stoH = (d.sto_income / maxVal) * (chartH - 40);
          const btcH = (d.btc_cost / maxVal) * (chartH - 40);
          const baseY = chartH - 20;

          return (
            <g key={d.month}>
              {/* STO bar */}
              <rect x={x} y={baseY - stoH} width={barW} height={stoH} fill="#22c55e" rx="2">
                <title>STO: {formatCurrency(d.sto_income)}</title>
              </rect>
              {/* BTC bar */}
              <rect x={x + barW + gap} y={baseY - btcH} width={barW} height={btcH} fill="#ef4444" rx="2">
                <title>BTC: {formatCurrency(d.btc_cost)}</title>
              </rect>
              {/* Month label */}
              <text x={x + barW + gap / 2} y={baseY + 14} textAnchor="middle" fill="currentColor" opacity="0.6" fontSize="10">
                {d.month}
              </text>
            </g>
          );
        })}

        {/* Net income line */}
        <polyline points={linePoints} fill="none" stroke="#3b82f6" strokeWidth="2" />
        {items.map((d, i) => {
          const x = 40 + i * groupW + groupW / 2;
          const y = chartH - (d.net_income / maxVal) * (chartH - 40) - 20;
          return (
            <circle key={i} cx={x} cy={y} r="3" fill="#3b82f6">
              <title>Net: {formatCurrency(d.net_income)}</title>
            </circle>
          );
        })}
      </svg>
      <div className="flex gap-4 mt-2 text-xs text-muted-foreground">
        <span className="flex items-center gap-1"><span className="inline-block w-3 h-3 rounded-sm bg-green-500" /> STO Income</span>
        <span className="flex items-center gap-1"><span className="inline-block w-3 h-3 rounded-sm bg-red-500" /> BTC Cost</span>
        <span className="flex items-center gap-1"><span className="inline-block w-3 h-3 rounded-sm bg-blue-500" /> Net Income</span>
      </div>
    </div>
  );
}

function CumulativePnlChart({ data }: { data: StatisticsData }) {
  const items = data.cumulative_pnl;
  if (items.length === 0) return <p className="text-muted-foreground text-sm">No data yet</p>;

  const values = items.map(d => d.cumulative);
  const maxVal = Math.max(...values, 0);
  const minVal = Math.min(...values, 0);
  const range = Math.max(maxVal - minVal, 1);
  const chartH = 220;
  const chartW = Math.max(items.length * 60 + 80, 300);

  const toY = (v: number) => 20 + ((maxVal - v) / range) * (chartH - 40);
  const zeroY = toY(0);

  const points = items.map((d, i) => {
    const x = 50 + (i / Math.max(items.length - 1, 1)) * (chartW - 80);
    const y = toY(d.cumulative);
    return `${x},${y}`;
  }).join(' ');

  // Area fill under the line
  const firstX = 50;
  const lastX = 50 + ((items.length - 1) / Math.max(items.length - 1, 1)) * (chartW - 80);
  const areaPath = `M${firstX},${zeroY} L${points.split(' ').map(p => `${p}`).join(' L')} L${lastX},${zeroY} Z`;

  return (
    <div className="overflow-x-auto">
      <svg width={chartW} height={chartH + 30} className="text-xs">
        {/* Zero line */}
        <line x1="48" x2={chartW} y1={zeroY} y2={zeroY} stroke="currentColor" strokeOpacity="0.15" strokeDasharray="4" />

        {/* Y axis labels */}
        {[minVal, minVal + range * 0.25, minVal + range * 0.5, minVal + range * 0.75, maxVal].map((val, idx) => {
          const y = toY(val);
          return (
            <text key={idx} x="45" y={y + 4} textAnchor="end" fill="currentColor" opacity="0.5" fontSize="10">
              {val >= 1000 || val <= -1000 ? `${(val / 1000).toFixed(1)}k` : val.toFixed(0)}
            </text>
          );
        })}

        {/* Area */}
        <path d={areaPath} fill="#3b82f6" opacity="0.1" />

        {/* Line */}
        <polyline points={points} fill="none" stroke="#3b82f6" strokeWidth="2" />

        {/* Points and labels */}
        {items.map((d, i) => {
          const x = 50 + (i / Math.max(items.length - 1, 1)) * (chartW - 80);
          const y = toY(d.cumulative);
          return (
            <g key={d.month}>
              <circle cx={x} cy={y} r="3" fill="#3b82f6">
                <title>{d.month}: {formatCurrency(d.cumulative)}</title>
              </circle>
              <text x={x} y={chartH + 10} textAnchor="middle" fill="currentColor" opacity="0.6" fontSize="10">
                {d.month}
              </text>
            </g>
          );
        })}
      </svg>
    </div>
  );
}

function PremiumByTickerChart({ data }: { data: StatisticsData }) {
  const items = data.premium_by_ticker;
  if (items.length === 0) return <p className="text-muted-foreground text-sm">No data yet</p>;

  const maxVal = Math.max(...items.map(d => Math.abs(d.net_premium)), 1);
  const barH = 28;
  const gap = 6;
  const chartH = items.length * (barH + gap) + 10;
  const chartW = 500;
  const labelW = 60;

  return (
    <div className="overflow-x-auto">
      <svg width={chartW} height={chartH} className="text-xs">
        {items.map((d, i) => {
          const y = i * (barH + gap) + 5;
          const w = (Math.abs(d.net_premium) / maxVal) * (chartW - labelW - 80);
          const isNeg = d.net_premium < 0;

          return (
            <g key={d.ticker}>
              <text x={labelW - 5} y={y + barH / 2 + 4} textAnchor="end" fill="currentColor" opacity="0.8" fontSize="12" fontWeight="500">
                {d.ticker}
              </text>
              <rect x={labelW} y={y} width={Math.max(w, 2)} height={barH} fill={isNeg ? '#ef4444' : '#22c55e'} rx="3">
                <title>{d.ticker}: {formatCurrency(d.net_premium)}</title>
              </rect>
              <text x={labelW + w + 6} y={y + barH / 2 + 4} fill="currentColor" opacity="0.7" fontSize="11">
                {formatCurrency(d.net_premium)}
              </text>
            </g>
          );
        })}
      </svg>
    </div>
  );
}

export default function StatisticsPage() {
  const { accounts } = useAccounts();
  const [data, setData] = useState<StatisticsData | null>(null);
  const [filterAccountId, setFilterAccountId] = useState<number | undefined>(undefined);

  const refresh = useCallback(() => {
    api.statistics(filterAccountId).then(setData).catch(() => setData(null));
  }, [filterAccountId]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-bold">Statistics</h1>
        <select
          className="border rounded-md px-3 py-1.5 text-sm bg-background"
          value={filterAccountId ?? ''}
          onChange={(e) => setFilterAccountId(e.target.value ? Number(e.target.value) : undefined)}
        >
          <option value="">All Accounts</option>
          {accounts.map((a) => (
            <option key={a.id} value={a.id}>{a.name}</option>
          ))}
        </select>
      </div>

      {!data ? (
        <div className="text-muted-foreground">Loading...</div>
      ) : (
        <>
          {/* Summary metrics */}
          <div className="grid grid-cols-1 sm:grid-cols-4 gap-4">
            <MetricCard title="Total Premium (Closed)" value={formatCurrency(data.total_premium)} subtitle="Net from closed trades" />
            <MetricCard title="Total Premium (Open)" value={formatCurrency(data.total_premium_open)} subtitle="Unrealized from open trades" />
            <MetricCard title="Yield (Closed)" value={formatPercent(data.yield_closed)} subtitle="Annualized, closed trades" />
            <MetricCard title="Yield (Open)" value={formatPercent(data.yield_open)} subtitle="Annualized, open trades" />
          </div>

          {/* Monthly Options Income */}
          <Card>
            <CardHeader>
              <CardTitle>Monthly Options Income</CardTitle>
            </CardHeader>
            <CardContent>
              <MonthlyIncomeChart data={data} />
            </CardContent>
          </Card>

          {/* Cumulative P&L */}
          <Card>
            <CardHeader>
              <CardTitle>Cumulative Options P&L</CardTitle>
            </CardHeader>
            <CardContent>
              <CumulativePnlChart data={data} />
            </CardContent>
          </Card>

          {/* Premium by Ticker */}
          <Card>
            <CardHeader>
              <CardTitle>Premium by Ticker</CardTitle>
            </CardHeader>
            <CardContent>
              <PremiumByTickerChart data={data} />
            </CardContent>
          </Card>
        </>
      )}
    </div>
  );
}
