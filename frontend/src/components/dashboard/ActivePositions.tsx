'use client';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';
import { Badge } from '@/components/ui/badge';
import { formatCurrency, daysToExpiry } from '@/lib/utils';
import { ClosePutModal } from '@/components/trades/ClosePutModal';
import { CloseCallModal } from '@/components/trades/CloseCallModal';
import type { ShareLot, Trade } from '@/lib/types';

interface Props {
  openTrades: Trade[];
  activeLots: ShareLot[];
  onTradeClose?: () => void;
}

export function ActivePositions({ openTrades, activeLots, onTradeClose }: Props) {
  return (
    <div className="space-y-6">
      <div>
        <h3 className="font-semibold mb-2">Open Trades</h3>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Ticker</TableHead>
              <TableHead>Type</TableHead>
              <TableHead>Qty</TableHead>
              <TableHead>Strike</TableHead>
              <TableHead>Expiry</TableHead>
              <TableHead>DTE</TableHead>
              <TableHead>Premium</TableHead>
              <TableHead></TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {openTrades.length === 0 && (
              <TableRow><TableCell colSpan={8} className="text-center text-muted-foreground">No open trades</TableCell></TableRow>
            )}
            {openTrades.map((t) => (
              <TableRow key={t.id}>
                <TableCell className="font-medium">{t.ticker}</TableCell>
                <TableCell><Badge variant={t.trade_type === 'PUT' ? 'secondary' : 'default'}>{t.trade_type}</Badge></TableCell>
                <TableCell>{t.quantity}</TableCell>
                <TableCell>{formatCurrency(t.strike_price)}</TableCell>
                <TableCell>{t.expiry_date}</TableCell>
                <TableCell>{daysToExpiry(t.expiry_date)}d</TableCell>
                <TableCell>{formatCurrency(t.premium_received)}</TableCell>
                <TableCell>
                  {t.trade_type === 'PUT'
                    ? <ClosePutModal tradeId={t.id} onClose={onTradeClose ?? (() => {})} />
                    : <CloseCallModal tradeId={t.id} onClose={onTradeClose ?? (() => {})} />
                  }
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </div>

      <div>
        <h3 className="font-semibold mb-2">Share Lots</h3>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Ticker</TableHead>
              <TableHead>Shares</TableHead>
              <TableHead>Original CB</TableHead>
              <TableHead>Adjusted CB</TableHead>
              <TableHead>CB Reduction</TableHead>
              <TableHead>Source</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {activeLots.length === 0 && (
              <TableRow><TableCell colSpan={6} className="text-center text-muted-foreground">No share lots</TableCell></TableRow>
            )}
            {activeLots.map((lot) => (
              <TableRow key={lot.id}>
                <TableCell className="font-medium">{lot.ticker}</TableCell>
                <TableCell>{lot.quantity}</TableCell>
                <TableCell>{formatCurrency(lot.original_cost_basis)}</TableCell>
                <TableCell className="font-medium">{formatCurrency(lot.adjusted_cost_basis)}</TableCell>
                <TableCell className="text-green-600">
                  -{formatCurrency(lot.original_cost_basis - lot.adjusted_cost_basis)}
                </TableCell>
                <TableCell><Badge variant="outline">{lot.acquisition_type}</Badge></TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </div>
    </div>
  );
}
