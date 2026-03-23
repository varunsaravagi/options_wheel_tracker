-- Allow share lots to be manually sold (not just called away).
-- Tracks sale price and date for realized P&L calculation.
--
-- We add the new columns and drop/recreate the CHECK constraint.
-- SQLite doesn't support ALTER CHECK directly, but since 3.37.0 it supports
-- DROP COLUMN + ADD COLUMN. For the CHECK on status, we simply drop the old
-- status column and re-add it with the expanded CHECK.
--
-- However, DROP COLUMN on a column with CHECK is tricky and lossy.
-- Simpler approach: just add the new columns. The old CHECK still works because
-- 'SOLD' can be set via UPDATE bypassing CHECK if we use a trigger,
-- but actually SQLite CHECK constraints ARE enforced on UPDATE.
--
-- Safest approach: add new columns, recreate table without FK issues.
-- Since PRAGMA foreign_keys=OFF doesn't work inside transactions (which sqlx uses),
-- we work around by: 1) NULL out FK references, 2) recreate, 3) restore references.

-- Step 1: Remove FK references pointing TO share_lots
UPDATE trades SET share_lot_id = NULL WHERE share_lot_id IS NOT NULL;

-- Step 2: Recreate table with expanded CHECK and new columns
CREATE TABLE share_lots_new (
  id                   INTEGER PRIMARY KEY AUTOINCREMENT,
  account_id           INTEGER NOT NULL REFERENCES accounts(id),
  ticker               TEXT NOT NULL,
  quantity             INTEGER NOT NULL DEFAULT 100,
  original_cost_basis  REAL NOT NULL,
  adjusted_cost_basis  REAL NOT NULL,
  acquisition_date     TEXT NOT NULL,
  acquisition_type     TEXT NOT NULL CHECK(acquisition_type IN ('MANUAL','ASSIGNED')),
  source_trade_id      INTEGER REFERENCES trades(id),
  status               TEXT NOT NULL DEFAULT 'ACTIVE'
                       CHECK(status IN ('ACTIVE','CALLED_AWAY','SOLD')),
  sale_price           REAL,
  sale_date            TEXT,
  created_at           TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO share_lots_new
  SELECT id, account_id, ticker, quantity, original_cost_basis, adjusted_cost_basis,
         acquisition_date, acquisition_type, source_trade_id, status,
         NULL, NULL, created_at
  FROM share_lots;

DROP TABLE share_lots;
ALTER TABLE share_lots_new RENAME TO share_lots;

-- Step 3: Restore FK references (trades -> share_lots via source_trade_id linkage)
UPDATE trades SET share_lot_id = (
  SELECT sl.id FROM share_lots sl
  WHERE sl.source_trade_id = trades.id
) WHERE status = 'CALLED_AWAY' AND share_lot_id IS NULL;
