import { Suspense } from 'react';
import { CallForm } from '@/components/trades/CallForm';

export default function NewCallPage() {
  return (
    <div className="space-y-4">
      <h1 className="text-2xl font-bold">Sell to Open — CALL</h1>
      <Suspense>
        <CallForm />
      </Suspense>
    </div>
  );
}
