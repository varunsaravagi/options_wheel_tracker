'use client';
import { useState } from 'react';
import { useRouter } from 'next/navigation';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { api } from '@/lib/api';
import { useAccounts } from '@/contexts/AccountContext';

export function PutForm() {
  const router = useRouter();
  const { selectedAccountId } = useAccounts();
  const [form, setForm] = useState({
    ticker: '', strike_price: '', expiry_date: '',
    open_date: new Date().toISOString().split('T')[0],
    premium_received: '', fees_open: '1.30', quantity: '1',
  });
  const [error, setError] = useState('');

  const set = (k: string) => (e: React.ChangeEvent<HTMLInputElement>) =>
    setForm((f) => ({ ...f, [k]: e.target.value }));

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!selectedAccountId) { setError('Select an account first'); return; }
    try {
      await api.puts.open(selectedAccountId, {
        ...form,
        ticker: form.ticker.toUpperCase(),
        strike_price: parseFloat(form.strike_price),
        premium_received: parseFloat(form.premium_received),
        fees_open: parseFloat(form.fees_open),
        quantity: parseInt(form.quantity),
      });
      router.push('/');
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : 'Failed to open trade');
    }
  };

  return (
    <Card className="max-w-md">
      <CardHeader><CardTitle>Sell to Open — PUT</CardTitle></CardHeader>
      <CardContent>
        <form onSubmit={handleSubmit} className="space-y-4">
          {[
            { label: 'Ticker', key: 'ticker', placeholder: 'AAPL', type: 'text' },
            { label: 'Strike Price', key: 'strike_price', placeholder: '150.00', type: 'number' },
            { label: 'Expiry Date', key: 'expiry_date', placeholder: '', type: 'date' },
            { label: 'Open Date', key: 'open_date', placeholder: '', type: 'date' },
            { label: 'Premium Received ($)', key: 'premium_received', placeholder: '200.00', type: 'number' },
            { label: 'Fees ($)', key: 'fees_open', placeholder: '1.30', type: 'number' },
            { label: 'Quantity (contracts)', key: 'quantity', placeholder: '1', type: 'number' },
          ].map(({ label, key, placeholder, type }) => (
            <div key={key} className="space-y-1">
              <Label>{label}</Label>
              <Input type={type} placeholder={placeholder}
                value={form[key as keyof typeof form]} onChange={set(key)} required />
            </div>
          ))}
          {error && <p className="text-sm text-destructive">{error}</p>}
          <Button type="submit" className="w-full">Open PUT Trade</Button>
        </form>
      </CardContent>
    </Card>
  );
}
