'use client';
import { Badge } from '@/components/ui/badge';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';
import { EditTradeModal } from '@/components/trades/EditTradeModal';
import { formatCurrency } from '@/lib/utils';
import type { Trade } from '@/lib/types';

const STATUS_COLORS: Record<string, 'default' | 'secondary' | 'destructive' | 'outline'> = {
  OPEN: 'default', EXPIRED: 'secondary', BOUGHT_BACK: 'outline',
  ASSIGNED: 'secondary', CALLED_AWAY: 'outline',
};

function netPremium(t: Trade): number {
  return t.premium_received - t.fees_open - (t.close_premium ?? 0) - (t.fees_close ?? 0);
}

interface Props { trades: Trade[]; onTradeUpdate?: () => void; }

export function TradeTable({ trades, onTradeUpdate }: Props) {
  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>Ticker</TableHead>
          <TableHead>Type</TableHead>
          <TableHead>Qty</TableHead>
          <TableHead>Strike</TableHead>
          <TableHead>Open Date</TableHead>
          <TableHead>Close Date</TableHead>
          <TableHead>Premium</TableHead>
          <TableHead>Net</TableHead>
          <TableHead>Status</TableHead>
          <TableHead></TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {trades.length === 0 && (
          <TableRow><TableCell colSpan={10} className="text-center text-muted-foreground">No trades found</TableCell></TableRow>
        )}
        {trades.map((t) => (
          <TableRow key={t.id}>
            <TableCell className="font-medium">{t.ticker}</TableCell>
            <TableCell><Badge variant={t.trade_type === 'PUT' ? 'secondary' : 'default'}>{t.trade_type}</Badge></TableCell>
            <TableCell>{t.quantity}</TableCell>
            <TableCell>{formatCurrency(t.strike_price)}</TableCell>
            <TableCell>{t.open_date}</TableCell>
            <TableCell>{t.close_date ?? '—'}</TableCell>
            <TableCell>{formatCurrency(t.premium_received)}</TableCell>
            <TableCell className={netPremium(t) >= 0 ? 'text-green-600' : 'text-red-500'}>
              {formatCurrency(netPremium(t))}
            </TableCell>
            <TableCell><Badge variant={STATUS_COLORS[t.status] ?? 'outline'}>{t.status}</Badge></TableCell>
            <TableCell>
              <EditTradeModal trade={t} onSave={onTradeUpdate ?? (() => {})} />
            </TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  );
}
