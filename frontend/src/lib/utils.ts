import { clsx, type ClassValue } from "clsx"
import { twMerge } from "tailwind-merge"

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs))
}

export function formatCurrency(value: number): string {
  return new Intl.NumberFormat('en-US', { style: 'currency', currency: 'USD' }).format(value);
}

export function formatPercent(value: number): string {
  return `${value.toFixed(2)}%`;
}

export function daysToExpiry(expiryDate: string): number {
  const today = new Date();
  today.setHours(0, 0, 0, 0);
  const expiry = new Date(expiryDate + 'T00:00:00');
  return Math.ceil((expiry.getTime() - today.getTime()) / (1000 * 60 * 60 * 24));
}

export function getDateRangePreset(preset: string): { date_from: string; date_to: string } {
  const today = new Date();
  const fmt = (d: Date) => d.toISOString().split('T')[0];
  const ago = (days: number) => { const d = new Date(today); d.setDate(d.getDate() - days); return d; };

  switch (preset) {
    case '30d': return { date_from: fmt(ago(30)), date_to: fmt(today) };
    case '60d': return { date_from: fmt(ago(60)), date_to: fmt(today) };
    case '90d': return { date_from: fmt(ago(90)), date_to: fmt(today) };
    case 'ytd': return { date_from: `${today.getFullYear()}-01-01`, date_to: fmt(today) };
    default:
      if (/^\d{4}$/.test(preset)) return { date_from: `${preset}-01-01`, date_to: `${preset}-12-31` };
      return { date_from: '', date_to: '' };
  }
}
