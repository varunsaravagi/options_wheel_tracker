'use client';
import { useState } from 'react';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogTrigger } from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { api } from '@/lib/api';
import { formatCurrency } from '@/lib/utils';
import type { Trade } from '@/lib/types';

interface Props {
  trade: Trade;
  trades: Trade[];
  onLink: () => void;
}

export function LinkRollModal({ trade, trades, onLink }: Props) {
  const [open, setOpen] = useState(false);
  const [error, setError] = useState('');

  // Candidates: same ticker + type, opened on or after this trade's close date, not already linked
  const candidates = trades.filter(
    (t) =>
      t.id !== trade.id &&
      t.ticker === trade.ticker &&
      t.trade_type === trade.trade_type &&
      trade.close_date !== null &&
      t.open_date >= trade.close_date &&
      t.rolled_from_trade_id === null
  );

  const handleLink = async (targetId: number) => {
    try {
      await api.trades.linkRoll(trade.id, targetId);
      setOpen(false);
      onLink();
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : 'Failed to link roll');
    }
  };

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger render={<Button size="xs" variant="outline" />}>
        Link Roll
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Link Roll — {trade.ticker} {trade.trade_type}</DialogTitle>
        </DialogHeader>
        <div className="space-y-3">
          <p className="text-sm text-muted-foreground">
            Select the trade this was rolled into. Trade #{trade.id} will be marked as ROLLED.
          </p>
          {candidates.length === 0 && (
            <p className="text-sm text-muted-foreground">
              No candidates found. The replacement trade must have the same ticker, same type, and open on or after {trade.close_date}.
            </p>
          )}
          {candidates.map((t) => (
            <div key={t.id} className="flex items-center justify-between border rounded p-3">
              <div className="text-sm space-y-0.5">
                <div className="font-medium">#{t.id} — {t.ticker} {t.trade_type}</div>
                <div className="text-muted-foreground">
                  Opened {t.open_date} · Strike {formatCurrency(t.strike_price)} · {t.status}
                </div>
              </div>
              <Button size="sm" onClick={() => handleLink(t.id)}>Select</Button>
            </div>
          ))}
          {error && <p className="text-sm text-destructive">{error}</p>}
        </div>
      </DialogContent>
    </Dialog>
  );
}
