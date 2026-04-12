'use client';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';
import { EditTradeModal } from '@/components/trades/EditTradeModal';
import { LinkRollModal } from '@/components/history/LinkRollModal';
import { api } from '@/lib/api';
import { formatCurrency } from '@/lib/utils';
import type { Trade } from '@/lib/types';

const STATUS_COLORS: Record<string, 'default' | 'secondary' | 'destructive' | 'outline'> = {
  OPEN: 'default', EXPIRED: 'secondary', BOUGHT_BACK: 'outline',
  ASSIGNED: 'secondary', CALLED_AWAY: 'outline', ROLLED: 'outline',
};

function netPremium(t: Trade): number {
  return t.premium_received - t.fees_open - (t.close_premium ?? 0) - (t.fees_close ?? 0);
}

function displayStatus(t: Trade): string {
  if (t.rolled_to_trade_id !== null) return 'ROLLED';
  return t.status;
}

interface Props { trades: Trade[]; onTradeUpdate?: () => void; }

export function TradeTable({ trades, onTradeUpdate }: Props) {
  const handleDelete = async (tradeId: number) => {
    await api.trades.delete(tradeId);
    onTradeUpdate?.();
  };

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
        {trades.map((t) => {
          const status = displayStatus(t);
          return (
            <TableRow key={t.id} className={t.deleted_at ? 'opacity-50' : ''}>
              <TableCell className={`font-medium ${t.deleted_at ? 'line-through' : ''}`}>{t.ticker}</TableCell>
              <TableCell><Badge variant={t.trade_type === 'PUT' ? 'secondary' : 'default'}>{t.trade_type}</Badge></TableCell>
              <TableCell className={t.deleted_at ? 'line-through' : ''}>{t.quantity}</TableCell>
              <TableCell className={t.deleted_at ? 'line-through' : ''}>{formatCurrency(t.strike_price)}</TableCell>
              <TableCell className={t.deleted_at ? 'line-through' : ''}>{t.open_date}</TableCell>
              <TableCell className={t.deleted_at ? 'line-through' : ''}>{t.close_date ?? '—'}</TableCell>
              <TableCell className={t.deleted_at ? 'line-through' : ''}>{formatCurrency(t.premium_received)}</TableCell>
              <TableCell className={`${t.deleted_at ? 'line-through ' : ''}${netPremium(t) >= 0 ? 'text-green-600' : 'text-red-500'}`}>
                {formatCurrency(netPremium(t))}
              </TableCell>
              <TableCell><Badge variant={STATUS_COLORS[status] ?? 'outline'}>{status}</Badge></TableCell>
              <TableCell className="space-x-1">
                {!t.deleted_at && (
                  <>
                    <EditTradeModal trade={t} onSave={onTradeUpdate ?? (() => {})} />
                    {t.status === 'BOUGHT_BACK' && t.rolled_to_trade_id === null && t.rolled_from_trade_id === null && (
                      <LinkRollModal trade={t} trades={trades} onLink={onTradeUpdate ?? (() => {})} />
                    )}
                    <Button variant="destructive" size="xs" onClick={() => handleDelete(t.id)}>Delete</Button>
                  </>
                )}
              </TableCell>
            </TableRow>
          );
        })}
      </TableBody>
    </Table>
  );
}
