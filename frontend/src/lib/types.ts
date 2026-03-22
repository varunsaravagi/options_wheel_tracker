export interface Account {
  id: number;
  name: string;
  created_at: string;
}

export type TradeType = 'PUT' | 'CALL';
export type TradeStatus = 'OPEN' | 'EXPIRED' | 'BOUGHT_BACK' | 'ASSIGNED' | 'CALLED_AWAY';

export interface Trade {
  id: number;
  account_id: number;
  trade_type: TradeType;
  ticker: string;
  strike_price: number;
  expiry_date: string;
  open_date: string;
  premium_received: number;
  fees_open: number;
  status: TradeStatus;
  close_date: string | null;
  close_premium: number | null;
  fees_close: number | null;
  share_lot_id: number | null;
  quantity: number;
  created_at: string;
}

export type AcquisitionType = 'MANUAL' | 'ASSIGNED';
export type LotStatus = 'ACTIVE' | 'CALLED_AWAY';

export interface ShareLot {
  id: number;
  account_id: number;
  ticker: string;
  quantity: number;
  original_cost_basis: number;
  adjusted_cost_basis: number;
  acquisition_date: string;
  acquisition_type: AcquisitionType;
  source_trade_id: number | null;
  status: LotStatus;
  created_at: string;
}

export interface DashboardData {
  total_premium_collected: number;
  total_capital_deployed: number;
  realized_annualized_yield: number;
  open_annualized_yield: number;
  open_trades: Trade[];
  active_share_lots: ShareLot[];
}

export interface HistoryFilters {
  account_id?: number;
  ticker?: string;
  date_from?: string;
  date_to?: string;
}
