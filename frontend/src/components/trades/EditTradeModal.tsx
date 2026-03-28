'use client';
import { useState } from 'react';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogTrigger } from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { api } from '@/lib/api';
import type { Trade } from '@/lib/types';

interface Props {
  trade: Trade;
  onSave: () => void;
}

export function EditTradeModal({ trade, onSave }: Props) {
  const [open, setOpen] = useState(false);
  const [strikePrice, setStrikePrice] = useState(String(trade.strike_price));
  const [expiryDate, setExpiryDate] = useState(trade.expiry_date);
  const [openDate, setOpenDate] = useState(trade.open_date);
  const [premiumReceived, setPremiumReceived] = useState(String(trade.premium_received));
  const [feesOpen, setFeesOpen] = useState(String(trade.fees_open));
  const [quantity, setQuantity] = useState(String(trade.quantity));
  const [closeDate, setCloseDate] = useState(trade.close_date ?? '');
  const [closePremium, setClosePremium] = useState(String(trade.close_premium ?? ''));
  const [feesClose, setFeesClose] = useState(String(trade.fees_close ?? ''));
  const [error, setError] = useState('');

  const isClosed = trade.status !== 'OPEN';

  const handleSubmit = async () => {
    try {
      const data: Record<string, unknown> = {
        strike_price: parseFloat(strikePrice),
        expiry_date: expiryDate,
        open_date: openDate,
        premium_received: parseFloat(premiumReceived),
        fees_open: parseFloat(feesOpen),
        quantity: parseInt(quantity, 10),
      };
      if (isClosed) {
        if (closeDate) data.close_date = closeDate;
        if (closePremium) data.close_premium = parseFloat(closePremium);
        if (feesClose) data.fees_close = parseFloat(feesClose);
      }
      await api.trades.edit(trade.id, data);
      setOpen(false);
      setError('');
      onSave();
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : 'Failed to update trade');
    }
  };

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger render={<Button size="sm" variant="outline" />}>
        Edit
      </DialogTrigger>
      <DialogContent>
        <DialogHeader><DialogTitle>Edit {trade.trade_type} Trade — {trade.ticker}</DialogTitle></DialogHeader>
        <div className="space-y-4">
          <div className="space-y-1">
            <Label>Strike Price ($)</Label>
            <Input type="number" value={strikePrice} onChange={(e) => setStrikePrice(e.target.value)} />
          </div>
          <div className="space-y-1">
            <Label>Open Date</Label>
            <Input type="date" value={openDate} onChange={(e) => setOpenDate(e.target.value)} />
          </div>
          <div className="space-y-1">
            <Label>Expiry Date</Label>
            <Input type="date" value={expiryDate} onChange={(e) => setExpiryDate(e.target.value)} />
          </div>
          <div className="space-y-1">
            <Label>Premium Received ($)</Label>
            <Input type="number" value={premiumReceived} onChange={(e) => setPremiumReceived(e.target.value)} />
          </div>
          <div className="space-y-1">
            <Label>Opening Fees ($)</Label>
            <Input type="number" value={feesOpen} onChange={(e) => setFeesOpen(e.target.value)} />
          </div>
          <div className="space-y-1">
            <Label>Quantity</Label>
            <Input type="number" value={quantity} onChange={(e) => setQuantity(e.target.value)} />
          </div>
          {isClosed && (
            <>
              <div className="space-y-1">
                <Label>Close Date</Label>
                <Input type="date" value={closeDate} onChange={(e) => setCloseDate(e.target.value)} />
              </div>
              <div className="space-y-1">
                <Label>Buy Back Premium ($)</Label>
                <Input type="number" value={closePremium} onChange={(e) => setClosePremium(e.target.value)} />
              </div>
              <div className="space-y-1">
                <Label>Closing Fees ($)</Label>
                <Input type="number" value={feesClose} onChange={(e) => setFeesClose(e.target.value)} />
              </div>
            </>
          )}
          {error && <p className="text-sm text-destructive">{error}</p>}
          <Button className="w-full" onClick={handleSubmit}>Save Changes</Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
