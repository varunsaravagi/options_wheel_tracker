-- Add soft-delete support for trades
ALTER TABLE trades ADD COLUMN deleted_at TEXT;
