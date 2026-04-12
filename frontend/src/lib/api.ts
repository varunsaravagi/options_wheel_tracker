import type { Account, DashboardData, HistoryFilters, ShareLot, StatisticsData, Trade } from './types';

const BASE = '';

async function request<T>(path: string, options?: RequestInit): Promise<T> {
  const res = await fetch(`${BASE}${path}`, {
    headers: { 'Content-Type': 'application/json' },
    ...options,
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: res.statusText }));
    throw new Error(err.error ?? 'Request failed');
  }
  return res.json();
}

export const api = {
  accounts: {
    list: () => request<Account[]>('/api/accounts'),
    create: (name: string) => request<Account>('/api/accounts', { method: 'POST', body: JSON.stringify({ name }) }),
    delete: (id: number) => request<void>(`/api/accounts/${id}`, { method: 'DELETE' }),
  },
  trades: {
    edit: (tradeId: number, data: object) =>
      request<Trade>(`/api/trades/${tradeId}`, { method: 'PUT', body: JSON.stringify(data) }),
    delete: (tradeId: number) =>
      request<Trade>(`/api/trades/${tradeId}`, { method: 'DELETE' }),
    linkRoll: (sourceId: number, targetTradeId: number) =>
      request<{ source_id: number; target_id: number }>(
        `/api/trades/${sourceId}/link-roll`,
        { method: 'POST', body: JSON.stringify({ target_trade_id: targetTradeId }) }
      ),
  },
  puts: {
    open: (accountId: number, data: object) =>
      request<Trade>(`/api/accounts/${accountId}/puts`, { method: 'POST', body: JSON.stringify(data) }),
    close: (tradeId: number, data: object) =>
      request<unknown>(`/api/trades/puts/${tradeId}/close`, { method: 'POST', body: JSON.stringify(data) }),
  },
  calls: {
    open: (accountId: number, data: object) =>
      request<Trade>(`/api/accounts/${accountId}/calls`, { method: 'POST', body: JSON.stringify(data) }),
    close: (tradeId: number, data: object) =>
      request<unknown>(`/api/trades/calls/${tradeId}/close`, { method: 'POST', body: JSON.stringify(data) }),
  },
  shareLots: {
    list: (accountId: number) => request<ShareLot[]>(`/api/accounts/${accountId}/share-lots`),
    create: (accountId: number, data: object) =>
      request<ShareLot>(`/api/accounts/${accountId}/share-lots`, { method: 'POST', body: JSON.stringify(data) }),
  },
  dashboard: (accountId?: number) => {
    const qs = accountId ? `?account_id=${accountId}` : '';
    return request<DashboardData>(`/api/dashboard${qs}`);
  },
  statistics: (accountId?: number) => {
    const qs = accountId ? `?account_id=${accountId}` : '';
    return request<StatisticsData>(`/api/statistics${qs}`);
  },
  history: (filters: HistoryFilters) => {
    const params = new URLSearchParams();
    if (filters.account_id) params.set('account_id', String(filters.account_id));
    if (filters.ticker) params.set('ticker', filters.ticker);
    if (filters.date_from) params.set('date_from', filters.date_from);
    if (filters.date_to) params.set('date_to', filters.date_to);
    return request<Trade[]>(`/api/history?${params}`);
  },
};
