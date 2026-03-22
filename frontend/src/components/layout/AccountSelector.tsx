'use client';
import { useState } from 'react';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { api } from '@/lib/api';
import { useAccounts } from '@/contexts/AccountContext';

export function AccountSelector() {
  const { accounts, selectedAccountId, setSelectedAccountId, refresh } = useAccounts();
  const [adding, setAdding] = useState(false);
  const [newName, setNewName] = useState('');

  const handleAdd = async () => {
    if (!newName.trim()) return;
    await api.accounts.create(newName.trim());
    setNewName('');
    setAdding(false);
    refresh();
  };

  return (
    <div className="space-y-2">
      <Select
        value={selectedAccountId?.toString() ?? ''}
        onValueChange={(v) => setSelectedAccountId(Number(v))}
      >
        <SelectTrigger className="w-full">
          <SelectValue placeholder="Select account" />
        </SelectTrigger>
        <SelectContent>
          {accounts.map((a) => (
            <SelectItem key={a.id} value={a.id.toString()}>{a.name}</SelectItem>
          ))}
        </SelectContent>
      </Select>
      {adding ? (
        <div className="flex gap-2">
          <Input value={newName} onChange={(e) => setNewName(e.target.value)}
            placeholder="Account name" onKeyDown={(e) => e.key === 'Enter' && handleAdd()} />
          <Button size="sm" onClick={handleAdd}>Add</Button>
          <Button size="sm" variant="ghost" onClick={() => setAdding(false)}>Cancel</Button>
        </div>
      ) : (
        <Button size="sm" variant="outline" className="w-full" onClick={() => setAdding(true)}>
          + Add Account
        </Button>
      )}
    </div>
  );
}
