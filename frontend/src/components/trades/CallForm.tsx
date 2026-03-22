'use client';
import { useEffect, useState } from 'react';
import { useRouter } from 'next/navigation';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { api } from '@/lib/api';
import { formatCurrency } from '@/lib/utils';
import { useAccounts } from '@/contexts/AccountContext';
import type { ShareLot } from '@/lib/types';

export function CallForm() {
  const router = useRouter();
  const { selectedAccountId } = useAccounts();
  const [lots, setLots] = useState<ShareLot[]>([]);
  const [selectedLotId, setSelectedLotId] = useState('');
  const [form, setForm] = useState({
    ticker: '', strike_price: '', expiry_date: '',
    open_date: new Date().toISOString().split('T')[0],
    premium_received: '', fees_open: '1.30',
  });
  const [error, setError] = useState('');

  useEffect(() => {
    if (selectedAccountId) {
      api.shareLots.list(selectedAccountId).then((l) => {
        setLots(l);
        if (l.length === 1) {
          setSelectedLotId(String(l[0].id));
          setForm((f) => ({ ...f, ticker: l[0].ticker }));
        }
      });
    }
  }, [selectedAccountId]);

  const handleLotChange = (id: string | null) => {
    if (!id) return;
    setSelectedLotId(id);
    const lot = lots.find((l) => l.id === Number(id));
    if (lot) setForm((f) => ({ ...f, ticker: lot.ticker }));
  };

  const set = (k: string) => (e: React.ChangeEvent<HTMLInputElement>) =>
    setForm((f) => ({ ...f, [k]: e.target.value }));

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!selectedAccountId) { setError('Select an account first'); return; }
    if (!selectedLotId) { setError('Select a share lot'); return; }
    try {
      await api.calls.open(selectedAccountId, {
        share_lot_id: Number(selectedLotId),
        ...form,
        ticker: form.ticker.toUpperCase(),
        strike_price: parseFloat(form.strike_price),
        premium_received: parseFloat(form.premium_received),
        fees_open: parseFloat(form.fees_open),
      });
      router.push('/');
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : 'Failed to open trade');
    }
  };

  const selectedLot = lots.find((l) => l.id === Number(selectedLotId));

  return (
    <Card className="max-w-md">
      <CardHeader><CardTitle>Sell to Open — CALL</CardTitle></CardHeader>
      <CardContent>
        <form onSubmit={handleSubmit} className="space-y-4">
          <div className="space-y-1">
            <Label>Share Lot</Label>
            {lots.length === 0 ? (
              <p className="text-sm text-muted-foreground">No active share lots. Assign a PUT first.</p>
            ) : (
              <Select value={selectedLotId || null} onValueChange={handleLotChange}>
                <SelectTrigger>
                  <SelectValue placeholder="Select lot" />
                </SelectTrigger>
                <SelectContent>
                  {lots.map((l) => (
                    <SelectItem key={l.id} value={String(l.id)}>
                      {l.ticker} — {l.quantity} shares @ {formatCurrency(l.adjusted_cost_basis)} adj. CB
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            )}
          </div>
          {selectedLot && (
            <div className="text-sm bg-muted rounded p-3 space-y-1">
              <div>Original CB: {formatCurrency(selectedLot.original_cost_basis)}/share</div>
              <div>Adjusted CB: <strong>{formatCurrency(selectedLot.adjusted_cost_basis)}/share</strong></div>
            </div>
          )}
          {[
            { label: 'Ticker', key: 'ticker', placeholder: 'AAPL', type: 'text' },
            { label: 'Strike Price', key: 'strike_price', placeholder: '155.00', type: 'number' },
            { label: 'Expiry Date', key: 'expiry_date', placeholder: '', type: 'date' },
            { label: 'Open Date', key: 'open_date', placeholder: '', type: 'date' },
            { label: 'Premium Received ($)', key: 'premium_received', placeholder: '150.00', type: 'number' },
            { label: 'Fees ($)', key: 'fees_open', placeholder: '1.30', type: 'number' },
          ].map(({ label, key, placeholder, type }) => (
            <div key={key} className="space-y-1">
              <Label>{label}</Label>
              <Input type={type} placeholder={placeholder}
                value={form[key as keyof typeof form]} onChange={set(key)} required />
            </div>
          ))}
          {error && <p className="text-sm text-destructive">{error}</p>}
          <Button type="submit" className="w-full" disabled={lots.length === 0}>Open CALL Trade</Button>
        </form>
      </CardContent>
    </Card>
  );
}
