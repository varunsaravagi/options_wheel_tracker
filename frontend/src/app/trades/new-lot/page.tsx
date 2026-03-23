'use client';
import { useState } from 'react';
import { useRouter } from 'next/navigation';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { api } from '@/lib/api';
import { useAccounts } from '@/contexts/AccountContext';

export default function NewLotPage() {
  const router = useRouter();
  const { selectedAccountId } = useAccounts();
  const [form, setForm] = useState({
    ticker: '',
    cost_basis: '',
    acquisition_date: new Date().toISOString().split('T')[0],
  });
  const [error, setError] = useState('');

  const set = (k: string) => (e: React.ChangeEvent<HTMLInputElement>) =>
    setForm((f) => ({ ...f, [k]: e.target.value }));

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!selectedAccountId) { setError('Select an account first'); return; }
    try {
      await api.shareLots.create(selectedAccountId, {
        ticker: form.ticker.toUpperCase(),
        cost_basis: parseFloat(form.cost_basis),
        acquisition_date: form.acquisition_date,
      });
      router.push('/');
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : 'Failed to add lot');
    }
  };

  return (
    <div className="space-y-4">
      <h1 className="text-2xl font-bold">Add Existing Share Lot</h1>
      <Card className="max-w-md">
        <CardHeader><CardTitle>Add Existing Share Lot</CardTitle></CardHeader>
        <CardContent>
          <p className="text-sm text-muted-foreground mb-4">
            Add shares you already own (purchased before using this app) so you can sell covered calls on them.
          </p>
          <form onSubmit={handleSubmit} className="space-y-4">
            {[
              { label: 'Ticker', key: 'ticker', placeholder: 'AAPL', type: 'text' },
              { label: 'Cost Basis (per share)', key: 'cost_basis', placeholder: '150.00', type: 'number' },
              { label: 'Purchase Date', key: 'acquisition_date', placeholder: '', type: 'date' },
            ].map(({ label, key, placeholder, type }) => (
              <div key={key} className="space-y-1">
                <Label>{label}</Label>
                <Input type={type} placeholder={placeholder}
                  value={form[key as keyof typeof form]} onChange={set(key)} required />
              </div>
            ))}
            {error && <p className="text-sm text-destructive">{error}</p>}
            <Button type="submit" className="w-full">Add Share Lot</Button>
          </form>
        </CardContent>
      </Card>
    </div>
  );
}
