'use client';
import { createContext, useContext, useEffect, useState } from 'react';
import { api } from '@/lib/api';
import type { Account } from '@/lib/types';

interface AccountContextValue {
  accounts: Account[];
  selectedAccountId: number | null;
  setSelectedAccountId: (id: number | null) => void;
  refresh: () => void;
}

const AccountContext = createContext<AccountContextValue>({
  accounts: [], selectedAccountId: null,
  setSelectedAccountId: () => {}, refresh: () => {},
});

export function AccountProvider({ children }: { children: React.ReactNode }) {
  const [accounts, setAccounts] = useState<Account[]>([]);
  const [selectedAccountId, setSelectedAccountId] = useState<number | null>(null);

  const refresh = () => api.accounts.list().then((accts) => {
    setAccounts(accts);
    if (accts.length > 0 && !selectedAccountId) setSelectedAccountId(accts[0].id);
  });

  // eslint-disable-next-line react-hooks/exhaustive-deps
  useEffect(() => { refresh(); }, []);

  return (
    <AccountContext.Provider value={{ accounts, selectedAccountId, setSelectedAccountId, refresh }}>
      {children}
    </AccountContext.Provider>
  );
}

export const useAccounts = () => useContext(AccountContext);
