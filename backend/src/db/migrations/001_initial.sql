CREATE TABLE accounts (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  name       TEXT NOT NULL,
  created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE trades (
  id               INTEGER PRIMARY KEY AUTOINCREMENT,
  account_id       INTEGER NOT NULL REFERENCES accounts(id),
  trade_type       TEXT NOT NULL CHECK(trade_type IN ('PUT', 'CALL')),
  ticker           TEXT NOT NULL,
  strike_price     REAL NOT NULL,
  expiry_date      TEXT NOT NULL,
  open_date        TEXT NOT NULL,
  premium_received REAL NOT NULL,
  fees_open        REAL NOT NULL DEFAULT 0,
  status           TEXT NOT NULL DEFAULT 'OPEN'
                   CHECK(status IN ('OPEN','EXPIRED','BOUGHT_BACK','ASSIGNED','CALLED_AWAY')),
  close_date       TEXT,
  close_premium    REAL,
  fees_close       REAL,
  share_lot_id     INTEGER REFERENCES share_lots(id),
  created_at       TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE share_lots (
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
                       CHECK(status IN ('ACTIVE','CALLED_AWAY')),
  created_at           TEXT NOT NULL DEFAULT (datetime('now'))
);
