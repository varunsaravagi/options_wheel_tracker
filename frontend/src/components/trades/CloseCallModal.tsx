'use client';
import { useState } from 'react';
import { useRouter } from 'next/navigation';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogTrigger } from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { api } from '@/lib/api';

interface Props {
  tradeId: number;
  shareLotId: number | null;
  onClose: () => void;
}

export function CloseCallModal({ tradeId, shareLotId, onClose }: Props) {
  const router = useRouter();
  const [open, setOpen] = useState(false);
  const [action, setAction] = useState('EXPIRED');
  const [closeDate, setCloseDate] = useState(new Date().toISOString().split('T')[0]);
  const [closePremium, setClosePremium] = useState('');
  const [feesClose, setFeesClose] = useState('1.30');
  const [error, setError] = useState('');

  const isRoll = action === 'ROLLED';
  const needsPremium = action === 'BOUGHT_BACK' || isRoll;

  const handleSubmit = async () => {
    try {
      await api.calls.close(tradeId, {
        action: isRoll ? 'BOUGHT_BACK' : action,
        close_date: closeDate,
        ...(needsPremium && { close_premium: parseFloat(closePremium), fees_close: parseFloat(feesClose) }),
      });
      setOpen(false);
      if (isRoll) {
        const params = new URLSearchParams({ rolled_from: String(tradeId) });
        if (shareLotId) params.set('lot_id', String(shareLotId));
        router.push(`/trades/new-call?${params}`);
      } else {
        onClose();
      }
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : 'Failed');
    }
  };

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger render={<Button size="sm" variant="outline" />}>Close</DialogTrigger>
      <DialogContent>
        <DialogHeader><DialogTitle>Close CALL Trade</DialogTitle></DialogHeader>
        <div className="space-y-4">
          <div className="space-y-1">
            <Label>Action</Label>
            <Select value={action} onValueChange={(v) => v && setAction(v)}>
              <SelectTrigger><SelectValue /></SelectTrigger>
              <SelectContent>
                <SelectItem value="EXPIRED">Expired Worthless</SelectItem>
                <SelectItem value="BOUGHT_BACK">Bought Back</SelectItem>
                <SelectItem value="CALLED_AWAY">Called Away (shares sold)</SelectItem>
                <SelectItem value="ROLLED">Roll to New CALL</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div className="space-y-1">
            <Label>Close Date</Label>
            <Input type="date" value={closeDate} onChange={(e) => setCloseDate(e.target.value)} />
          </div>
          {needsPremium && (
            <>
              <div className="space-y-1">
                <Label>Buy Back Price ($)</Label>
                <Input type="number" value={closePremium} onChange={(e) => setClosePremium(e.target.value)} />
              </div>
              <div className="space-y-1">
                <Label>Fees ($)</Label>
                <Input type="number" value={feesClose} onChange={(e) => setFeesClose(e.target.value)} />
              </div>
            </>
          )}
          {isRoll && (
            <p className="text-sm text-muted-foreground">
              After confirming, you&apos;ll be taken to a pre-filled new CALL form to open the replacement leg.
            </p>
          )}
          {error && <p className="text-sm text-destructive">{error}</p>}
          <Button className="w-full" onClick={handleSubmit}>
            {isRoll ? 'Close & Roll' : 'Confirm Close'}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
