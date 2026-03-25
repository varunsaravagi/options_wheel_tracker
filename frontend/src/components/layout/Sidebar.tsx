import Link from 'next/link';
import { AccountSelector } from './AccountSelector';

const navLinks = [
  { href: '/', label: 'Dashboard' },
  { href: '/trades/new-put', label: 'Sell PUT' },
  { href: '/trades/new-call', label: 'Sell CALL' },
  { href: '/trades/new-lot', label: 'Add Share Lot' },
  { href: '/history', label: 'History' },
  { href: '/statistics', label: 'Statistics' },
];

export function Sidebar() {
  return (
    <aside className="w-56 min-h-screen bg-card border-r flex flex-col p-4 gap-6">
      <div className="font-semibold text-lg">Wheel Tracker</div>
      <AccountSelector />
      <nav className="flex flex-col gap-1">
        {navLinks.map((link) => (
          <Link key={link.href} href={link.href}
            className="px-3 py-2 rounded-md text-sm hover:bg-accent hover:text-accent-foreground transition-colors">
            {link.label}
          </Link>
        ))}
      </nav>
    </aside>
  );
}
