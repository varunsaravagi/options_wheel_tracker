'use client';
import { useState } from 'react';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogTrigger } from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { api } from '@/lib/api';

interface Props {
  tradeId: number;
  onClose: () => void;
}

export function ClosePutModal({ tradeId, onClose }: Props) {
  const [open, setOpen] = useState(false);
  const [action, setAction] = useState('EXPIRED');
  const [closeDate, setCloseDate] = useState(new Date().toISOString().split('T')[0]);
  const [closePremium, setClosePremium] = useState('');
  const [feesClose, setFeesClose] = useState('1.30');
  const [error, setError] = useState('');

  const handleSubmit = async () => {
    try {
      await api.puts.close(tradeId, {
        action,
        close_date: closeDate,
        ...(action === 'BOUGHT_BACK' && { close_premium: parseFloat(closePremium), fees_close: parseFloat(feesClose) }),
      });
      setOpen(false);
      onClose();
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : 'Failed to close trade');
    }
  };

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger render={<Button size="sm" variant="outline" />}>
        Close
      </DialogTrigger>
      <DialogContent>
        <DialogHeader><DialogTitle>Close PUT Trade</DialogTitle></DialogHeader>
        <div className="space-y-4">
          <div className="space-y-1">
            <Label>Action</Label>
            <Select value={action} onValueChange={(v) => v && setAction(v)}>
              <SelectTrigger><SelectValue /></SelectTrigger>
              <SelectContent>
                <SelectItem value="EXPIRED">Expired Worthless</SelectItem>
                <SelectItem value="BOUGHT_BACK">Bought Back</SelectItem>
                <SelectItem value="ASSIGNED">Assigned (got shares)</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div className="space-y-1">
            <Label>Close Date</Label>
            <Input type="date" value={closeDate} onChange={(e) => setCloseDate(e.target.value)} />
          </div>
          {action === 'BOUGHT_BACK' && (
            <>
              <div className="space-y-1">
                <Label>Buy Back Price ($)</Label>
                <Input type="number" value={closePremium} onChange={(e) => setClosePremium(e.target.value)} placeholder="50.00" />
              </div>
              <div className="space-y-1">
                <Label>Fees ($)</Label>
                <Input type="number" value={feesClose} onChange={(e) => setFeesClose(e.target.value)} />
              </div>
            </>
          )}
          {error && <p className="text-sm text-destructive">{error}</p>}
          <Button className="w-full" onClick={handleSubmit}>Confirm Close</Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
