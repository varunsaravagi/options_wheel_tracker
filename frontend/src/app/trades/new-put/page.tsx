import { Suspense } from 'react';
import { PutForm } from '@/components/trades/PutForm';

export default function NewPutPage() {
  return (
    <div className="space-y-4">
      <h1 className="text-2xl font-bold">Sell to Open — PUT</h1>
      <Suspense>
        <PutForm />
      </Suspense>
    </div>
  );
}
