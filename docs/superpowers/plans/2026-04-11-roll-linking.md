# Roll Linking Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Track rolled options positions so yield calculations treat a roll chain (close + re-open same position) as a single trade spanning the full duration.

**Architecture:** Two nullable integer columns (`rolled_from_trade_id`, `rolled_to_trade_id`) are added to `trades` via simple ALTER TABLE (no table recreation). A roll chain is traversed by following `rolled_from_trade_id` backwards to the original leg. `ROLLED` is a display-only concept — DB status stays `BOUGHT_BACK`; the presence of `rolled_to_trade_id IS NOT NULL` signals that a trade was rolled. The yield calc skips non-final rolled legs and walks the chain for the final leg.

**Tech Stack:** Rust/Axum/SQLx (backend), Next.js 16 / React 19 / base-ui (frontend), SQLite

---

## File Map

**New files:**
- `backend/src/db/migrations/005_roll_linking.sql`
- `frontend/src/components/history/LinkRollModal.tsx`

**Modified files:**
- `backend/src/models/trade.rs` — add `rolled_from_trade_id`, `rolled_to_trade_id` fields + new methods
- `backend/src/handlers/yield_calc.rs` — skip non-final rolled legs; walk chain for open_date + net_premium
- `backend/src/handlers/puts.rs` — accept `rolled_from_trade_id` in `OpenPut`; set `rolled_to_trade_id` on prev trade after create; add `link_roll` handler
- `backend/src/handlers/calls.rs` — accept `rolled_from_trade_id` in `OpenCall`; same rolled_to update
- `backend/src/routes.rs` — register `POST /api/trades/:id/link-roll`
- `frontend/src/lib/types.ts` — add `rolled_from_trade_id`, `rolled_to_trade_id` to `Trade`; add `TradeStatus = 'ROLLED'`
- `frontend/src/lib/api.ts` — add `trades.linkRoll`
- `frontend/src/components/trades/ClosePutModal.tsx` — add Roll option; navigate to new-put on submit
- `frontend/src/components/trades/CloseCallModal.tsx` — add Roll option; add `shareLotId` prop; navigate to new-call on submit
- `frontend/src/components/dashboard/ActivePositions.tsx` — pass `shareLotId` to CloseCallModal
- `frontend/src/components/trades/PutForm.tsx` — read `?rolled_from` from URL; show roll banner; submit `rolled_from_trade_id`
- `frontend/src/components/trades/CallForm.tsx` — read `?rolled_from` + `?lot_id` from URL; pre-select lot; submit `rolled_from_trade_id`
- `frontend/src/components/history/TradeTable.tsx` — show ROLLED badge; show Link Roll button

---

## Task 1: Migration — add roll columns

**Files:**
- Create: `backend/src/db/migrations/005_roll_linking.sql`

- [ ] **Step 1: Write the migration**

```sql
-- Add roll-linking columns to trades.
-- rolled_from_trade_id: set on the NEW trade, points back to the previous rolled leg.
-- rolled_to_trade_id:   set on the OLD trade, points forward to the replacement leg.
-- Both are nullable integers. No FK constraint to avoid circular reference complexity.
ALTER TABLE trades ADD COLUMN rolled_from_trade_id INTEGER;
ALTER TABLE trades ADD COLUMN rolled_to_trade_id INTEGER;
```

- [ ] **Step 2: Run migration smoke test**

```bash
cd /root/options_wheel_tracker/dev
bash scripts/test-migration.sh
```
Expected: `Migration test passed`

- [ ] **Step 3: Commit**

```bash
git add backend/src/db/migrations/005_roll_linking.sql
git commit -m "feat: migration 005 — add roll_from/roll_to columns to trades"
```

---

## Task 2: Backend model — Trade struct + new methods

**Files:**
- Modify: `backend/src/models/trade.rs`

Every SELECT in this file lists columns explicitly. Both new columns must be added to every SELECT and RETURNING clause.

- [ ] **Step 1: Add fields to the Trade struct**

In `backend/src/models/trade.rs`, change the `Trade` struct to add after `deleted_at`:

```rust
pub deleted_at: Option<String>,
pub rolled_from_trade_id: Option<i64>,
pub rolled_to_trade_id: Option<i64>,
```

- [ ] **Step 2: Add `rolled_from_trade_id` to `CreateTrade`**

```rust
pub struct CreateTrade {
    pub account_id: i64,
    pub trade_type: String,
    pub ticker: String,
    pub strike_price: f64,
    pub expiry_date: String,
    pub open_date: String,
    pub premium_received: f64,
    pub fees_open: f64,
    pub share_lot_id: Option<i64>,
    pub quantity: Option<i64>,
    pub rolled_from_trade_id: Option<i64>,
}
```

- [ ] **Step 3: Update `Trade::create` — INSERT + RETURNING**

Replace the query in `Trade::create`:

```rust
let trade = sqlx::query_as::<_, Trade>(
    "INSERT INTO trades (account_id, trade_type, ticker, strike_price, expiry_date, open_date, premium_received, fees_open, share_lot_id, quantity, rolled_from_trade_id)
     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
     RETURNING id, account_id, trade_type, ticker, strike_price, expiry_date, open_date, premium_received, fees_open, status, close_date, close_premium, fees_close, share_lot_id, quantity, created_at, deleted_at, rolled_from_trade_id, rolled_to_trade_id"
)
.bind(input.account_id)
.bind(&input.trade_type)
.bind(&input.ticker)
.bind(input.strike_price)
.bind(&input.expiry_date)
.bind(&input.open_date)
.bind(input.premium_received)
.bind(input.fees_open)
.bind(input.share_lot_id)
.bind(qty)
.bind(input.rolled_from_trade_id)
.fetch_one(pool)
.await?;
```

- [ ] **Step 4: Update `Trade::get` SELECT**

Replace the SELECT in `Trade::get`:

```rust
let trade = sqlx::query_as::<_, Trade>(
    "SELECT id, account_id, trade_type, ticker, strike_price, expiry_date, open_date, premium_received, fees_open, status, close_date, close_premium, fees_close, share_lot_id, quantity, created_at, deleted_at, rolled_from_trade_id, rolled_to_trade_id
     FROM trades WHERE id = ?"
)
```

- [ ] **Step 5: Update `Trade::list_open` SELECT**

```rust
let trades = sqlx::query_as::<_, Trade>(
    "SELECT id, account_id, trade_type, ticker, strike_price, expiry_date, open_date, premium_received, fees_open, status, close_date, close_premium, fees_close, share_lot_id, quantity, created_at, deleted_at, rolled_from_trade_id, rolled_to_trade_id
     FROM trades WHERE account_id = ? AND status = 'OPEN' AND deleted_at IS NULL"
)
```

- [ ] **Step 6: Update `Trade::list_with_filters` SELECT**

```rust
let all = sqlx::query_as::<_, Trade>(
    "SELECT id, account_id, trade_type, ticker, strike_price, expiry_date, open_date, premium_received, fees_open, status, close_date, close_premium, fees_close, share_lot_id, quantity, created_at, deleted_at, rolled_from_trade_id, rolled_to_trade_id
     FROM trades ORDER BY open_date DESC"
)
```

- [ ] **Step 7: Add `Trade::set_rolled_to` method**

Add this after `Trade::soft_delete`:

```rust
pub async fn set_rolled_to(pool: &SqlitePool, id: i64, rolled_to_id: i64) -> Result<(), AppError> {
    let result = sqlx::query("UPDATE trades SET rolled_to_trade_id = ? WHERE id = ?")
        .bind(rolled_to_id)
        .bind(id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

pub async fn set_rolled_from(pool: &SqlitePool, id: i64, rolled_from_id: i64) -> Result<(), AppError> {
    let result = sqlx::query("UPDATE trades SET rolled_from_trade_id = ? WHERE id = ?")
        .bind(rolled_from_id)
        .bind(id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}
```

- [ ] **Step 8: Write a test for `set_rolled_to` / `set_rolled_from`**

In the `#[cfg(test)]` block in `trade.rs`, add:

```rust
#[tokio::test]
async fn test_roll_linking() {
    let (pool, account_id) = setup().await;

    let input = CreateTrade {
        account_id,
        trade_type: "PUT".to_string(),
        ticker: "AAPL".to_string(),
        strike_price: 150.0,
        expiry_date: "2025-02-21".to_string(),
        open_date: "2025-01-10".to_string(),
        premium_received: 200.0,
        fees_open: 1.3,
        share_lot_id: None,
        quantity: None,
        rolled_from_trade_id: None,
    };
    let trade_a = TradeModel::create(&pool, &input).await.unwrap();

    let input_b = CreateTrade {
        open_date: "2025-01-20".to_string(),
        rolled_from_trade_id: Some(trade_a.id),
        ..input
    };
    let trade_b = TradeModel::create(&pool, &input_b).await.unwrap();

    // Link A -> B
    TradeModel::set_rolled_to(&pool, trade_a.id, trade_b.id).await.unwrap();

    let a = TradeModel::get(&pool, trade_a.id).await.unwrap();
    let b = TradeModel::get(&pool, trade_b.id).await.unwrap();

    assert_eq!(a.rolled_to_trade_id, Some(trade_b.id));
    assert_eq!(b.rolled_from_trade_id, Some(trade_a.id));
}
```

- [ ] **Step 9: Run tests**

```bash
cd /root/options_wheel_tracker/dev/backend && cargo test 2>&1 | tail -20
```
Expected: all tests pass (including the new `test_roll_linking`)

- [ ] **Step 10: Commit**

```bash
git add backend/src/models/trade.rs
git commit -m "feat: add rolled_from/rolled_to fields to Trade model"
```

---

## Task 3: Yield calculation — skip rolled legs, walk chain

**Files:**
- Modify: `backend/src/handlers/yield_calc.rs`

- [ ] **Step 1: Write the failing test for roll-aware yield**

In the `#[cfg(test)]` block in `yield_calc.rs`, add:

```rust
#[tokio::test]
async fn test_yield_roll_chain_uses_original_open_date() {
    // Leg A: opened 2025-01-01, closed (rolled) 2025-01-06 (5 days), net = -100.0
    // Leg B: opened 2025-01-06, closed 2025-01-16 (10 days), net = +200.0
    // Chain net = +100.0, chain open date = 2025-01-01, close date = 2025-01-16 (15 days)
    // capital = 10000 (strike 100 * 100 * 1)
    // expected ann = (100 / 10000) * (365 / 15) * 100 = 24.33%
    let (pool, acct_id) = setup().await;

    let leg_a = TradeModel::create(
        &pool,
        &CreateTrade {
            account_id: acct_id,
            trade_type: "PUT".to_string(),
            ticker: "TEST".to_string(),
            strike_price: 100.0,
            expiry_date: "2025-01-10".to_string(),
            open_date: "2025-01-01".to_string(),
            premium_received: 300.0,
            fees_open: 0.0,
            share_lot_id: None,
            quantity: Some(1),
            rolled_from_trade_id: None,
        },
    )
    .await
    .unwrap();

    let leg_a_closed = TradeModel::close(
        &pool, leg_a.id, "BOUGHT_BACK",
        Some(400.0), None, Some("2025-01-06".to_string()),
    )
    .await
    .unwrap();
    // net = 300 - 400 = -100

    let leg_b = TradeModel::create(
        &pool,
        &CreateTrade {
            account_id: acct_id,
            trade_type: "PUT".to_string(),
            ticker: "TEST".to_string(),
            strike_price: 100.0,
            expiry_date: "2025-01-20".to_string(),
            open_date: "2025-01-06".to_string(),
            premium_received: 200.0,
            fees_open: 0.0,
            share_lot_id: None,
            quantity: Some(1),
            rolled_from_trade_id: Some(leg_a.id),
        },
    )
    .await
    .unwrap();

    let leg_b_closed = TradeModel::close(
        &pool, leg_b.id, "EXPIRED",
        None, None, Some("2025-01-16".to_string()),
    )
    .await
    .unwrap();
    // net = 200

    // Link A -> B
    TradeModel::set_rolled_to(&pool, leg_a.id, leg_b.id).await.unwrap();

    // Re-fetch leg_a with rolled_to set
    let leg_a_final = TradeModel::get(&pool, leg_a.id).await.unwrap();

    let trades = vec![leg_a_final, leg_b_closed];
    let result = calculate_yields(&pool, &trades).await;

    // Only leg B contributes, with chain net=100, open_date=2025-01-01 (15 days)
    // ann = (100 / 10000) * (365 / 15) * 100 = 24.33%
    assert!((result.realized_yield - 24.33).abs() < 0.1, "got {}", result.realized_yield);
    // Leg A (rolled) should not appear in yield calc
    assert!(result.open_yield == 0.0);
}
```

- [ ] **Step 2: Run test to see it fail**

```bash
cd /root/options_wheel_tracker/dev/backend && cargo test test_yield_roll_chain_uses_original_open_date 2>&1 | tail -20
```
Expected: FAIL (yield is still wrong — chain not walked)

- [ ] **Step 3: Add chain-walking helper to `yield_calc.rs`**

Add this function after `get_capital_for_trade`:

```rust
/// Walk backwards through a roll chain to find the original open_date and
/// accumulated net_premium. Returns (original_open_date, total_net_premium).
///
/// The `trade` passed in is the FINAL (non-rolled) leg.
/// Each prior leg is found via `rolled_from_trade_id`.
pub async fn get_roll_chain_data(pool: &SqlitePool, trade: &Trade) -> (String, f64) {
    let mut total_net = trade.net_premium().unwrap_or(0.0);
    let mut open_date = trade.open_date.clone();
    let mut prev_id = trade.rolled_from_trade_id;

    while let Some(id) = prev_id {
        match Trade::get(pool, id).await {
            Ok(prev) => {
                total_net += prev.net_premium().unwrap_or(0.0);
                open_date = prev.open_date.clone();
                prev_id = prev.rolled_from_trade_id;
            }
            Err(_) => break,
        }
    }

    (open_date, total_net)
}
```

Note: `Trade::get` is in `crate::models::trade`. It's already imported at the top of this file as `use crate::models::trade::Trade;` — verify this import exists and add it if missing.

- [ ] **Step 4: Update `calculate_yields` to skip rolled legs and walk chain**

Replace the body of `calculate_yields`:

```rust
pub async fn calculate_yields(pool: &SqlitePool, trades: &[Trade]) -> YieldResult {
    let mut realized_weighted = 0.0;
    let mut realized_capital = 0.0;
    let mut open_weighted = 0.0;
    let mut open_capital = 0.0;
    let today_str = chrono::Local::now().format("%Y-%m-%d").to_string();

    for trade in trades {
        if trade.deleted_at.is_some() {
            continue;
        }
        // Skip non-final rolled legs — they are accounted for via the final leg's chain walk
        if trade.rolled_to_trade_id.is_some() {
            continue;
        }

        let capital = get_capital_for_trade(pool, trade).await;

        if trade.status == "OPEN" {
            let days = days_between(&trade.open_date, &today_str);
            let net = trade.net_premium().unwrap_or(0.0);
            if capital > 0.0 {
                let ann = (net / capital) * (365.0 / days);
                open_weighted += ann * capital;
                open_capital += capital;
            }
        } else {
            let close_date = trade.close_date.as_deref().unwrap_or(&today_str);
            let (open_date, net) = if trade.rolled_from_trade_id.is_some() {
                get_roll_chain_data(pool, trade).await
            } else {
                (trade.open_date.clone(), trade.net_premium().unwrap_or(0.0))
            };
            let days = days_between(&open_date, close_date);
            if capital > 0.0 {
                let ann = (net / capital) * (365.0 / days);
                realized_weighted += ann * capital;
                realized_capital += capital;
            }
        }
    }

    let realized_yield = if realized_capital > 0.0 {
        round2((realized_weighted / realized_capital) * 100.0)
    } else {
        0.0
    };

    let open_yield = if open_capital > 0.0 {
        round2((open_weighted / open_capital) * 100.0)
    } else {
        0.0
    };

    YieldResult {
        realized_yield,
        open_yield,
    }
}
```

- [ ] **Step 5: Run all yield tests**

```bash
cd /root/options_wheel_tracker/dev/backend && cargo test yield 2>&1 | tail -20
```
Expected: all yield tests pass, including `test_yield_roll_chain_uses_original_open_date`

- [ ] **Step 6: Commit**

```bash
git add backend/src/handlers/yield_calc.rs
git commit -m "feat: yield calc skips rolled legs and walks chain for original open date"
```

---

## Task 4: Backend handlers — open_put/open_call accept rolled_from; link-roll endpoint

**Files:**
- Modify: `backend/src/handlers/puts.rs`
- Modify: `backend/src/handlers/calls.rs`

- [ ] **Step 1: Write failing test for roll-aware open_put**

In `puts.rs` test block, add:

```rust
#[tokio::test]
async fn test_open_put_with_rolled_from_links_trades() {
    let (server, acct_id) = server().await;

    // Create original PUT
    let res = server
        .post(&format!("/api/accounts/{}/puts", acct_id))
        .json(&json!({
            "ticker": "AAPL", "strike_price": 150.0,
            "expiry_date": "2025-02-21", "open_date": "2025-01-10",
            "premium_received": 200.0, "fees_open": 1.30
        }))
        .await;
    let orig_id = res.json::<serde_json::Value>()["id"].as_i64().unwrap();

    // Close original as BOUGHT_BACK
    server
        .post(&format!("/api/trades/puts/{}/close", orig_id))
        .json(&json!({"action": "BOUGHT_BACK", "close_premium": 300.0, "fees_close": 1.30, "close_date": "2025-01-15"}))
        .await;

    // Open new PUT with rolled_from_trade_id
    let res = server
        .post(&format!("/api/accounts/{}/puts", acct_id))
        .json(&json!({
            "ticker": "AAPL", "strike_price": 148.0,
            "expiry_date": "2025-03-21", "open_date": "2025-01-15",
            "premium_received": 250.0, "fees_open": 1.30,
            "rolled_from_trade_id": orig_id
        }))
        .await;
    res.assert_status(StatusCode::CREATED);
    let new_trade = res.json::<serde_json::Value>();

    assert_eq!(new_trade["rolled_from_trade_id"], orig_id);

    // Verify original trade now has rolled_to_trade_id set to new trade
    let history = server.get(&format!("/api/history?account_id={}", acct_id)).await;
    let trades = history.json::<serde_json::Value>();
    let orig = trades.as_array().unwrap().iter()
        .find(|t| t["id"] == orig_id).unwrap();
    assert_eq!(orig["rolled_to_trade_id"], new_trade["id"]);
}
```

- [ ] **Step 2: Run test to see it fail**

```bash
cd /root/options_wheel_tracker/dev/backend && cargo test test_open_put_with_rolled_from 2>&1 | tail -20
```
Expected: FAIL — `rolled_from_trade_id` field missing from `OpenPut`

- [ ] **Step 3: Update `OpenPut` in `puts.rs` and `open_put` handler**

Add `rolled_from_trade_id` to `OpenPut` struct:

```rust
pub struct OpenPut {
    pub ticker: String,
    pub strike_price: f64,
    pub expiry_date: String,
    pub open_date: String,
    pub premium_received: f64,
    pub fees_open: f64,
    pub quantity: Option<i64>,
    pub rolled_from_trade_id: Option<i64>,
}
```

Update `open_put` handler body:

```rust
pub async fn open_put(
    State(pool): State<SqlitePool>,
    Path(account_id): Path<i64>,
    Json(payload): Json<OpenPut>,
) -> Result<(StatusCode, Json<Trade>), AppError> {
    let input = CreateTrade {
        account_id,
        trade_type: "PUT".to_string(),
        ticker: payload.ticker,
        strike_price: payload.strike_price,
        expiry_date: payload.expiry_date,
        open_date: payload.open_date,
        premium_received: payload.premium_received,
        fees_open: payload.fees_open,
        share_lot_id: None,
        quantity: payload.quantity,
        rolled_from_trade_id: payload.rolled_from_trade_id,
    };
    let trade = Trade::create(&pool, &input).await?;

    if let Some(prev_id) = payload.rolled_from_trade_id {
        Trade::set_rolled_to(&pool, prev_id, trade.id).await?;
    }

    Ok((StatusCode::CREATED, Json(trade)))
}
```

- [ ] **Step 4: Update `OpenCall` in `calls.rs` and `open_call` handler**

Add to `OpenCall` struct:

```rust
pub struct OpenCall {
    pub share_lot_id: i64,
    pub ticker: String,
    pub strike_price: f64,
    pub expiry_date: String,
    pub open_date: String,
    pub premium_received: f64,
    pub fees_open: f64,
    pub quantity: Option<i64>,
    pub rolled_from_trade_id: Option<i64>,
}
```

Update `open_call` handler — add after `let trade = Trade::create(&pool, &input).await?;`:

```rust
let input = CreateTrade {
    account_id,
    trade_type: "CALL".to_string(),
    ticker: payload.ticker,
    strike_price: payload.strike_price,
    expiry_date: payload.expiry_date,
    open_date: payload.open_date,
    premium_received: payload.premium_received,
    fees_open: payload.fees_open,
    share_lot_id: Some(payload.share_lot_id),
    quantity: payload.quantity,
    rolled_from_trade_id: payload.rolled_from_trade_id,
};
let trade = Trade::create(&pool, &input).await?;

if let Some(prev_id) = payload.rolled_from_trade_id {
    Trade::set_rolled_to(&pool, prev_id, trade.id).await?;
}

Ok((StatusCode::CREATED, Json(trade)))
```

- [ ] **Step 5: Add the `link_roll` handler to `puts.rs`**

Add this struct and handler after `delete_trade`:

```rust
#[derive(Deserialize)]
pub struct LinkRollPayload {
    pub target_trade_id: i64,
}

pub async fn link_roll(
    State(pool): State<SqlitePool>,
    Path(source_id): Path<i64>,
    Json(payload): Json<LinkRollPayload>,
) -> Result<Json<serde_json::Value>, AppError> {
    // Verify both trades exist
    Trade::get(&pool, source_id).await?;
    Trade::get(&pool, payload.target_trade_id).await?;

    Trade::set_rolled_to(&pool, source_id, payload.target_trade_id).await?;
    Trade::set_rolled_from(&pool, payload.target_trade_id, source_id).await?;

    Ok(Json(json!({
        "source_id": source_id,
        "target_id": payload.target_trade_id
    })))
}
```

- [ ] **Step 6: Run all tests**

```bash
cd /root/options_wheel_tracker/dev/backend && cargo test 2>&1 | tail -20
```
Expected: all tests pass

- [ ] **Step 7: Commit**

```bash
git add backend/src/handlers/puts.rs backend/src/handlers/calls.rs
git commit -m "feat: open_put/open_call accept rolled_from_trade_id; add link_roll handler"
```

---

## Task 5: Backend routes — register link-roll

**Files:**
- Modify: `backend/src/routes.rs`

- [ ] **Step 1: Register the route**

In `create_router`, add after the `/api/trades/:id` route:

```rust
.route("/api/trades/:id/link-roll", post(puts::link_roll))
```

Full updated `create_router`:

```rust
pub fn create_router(pool: SqlitePool) -> Router {
    Router::new()
        .route(
            "/api/accounts",
            get(accounts::list_accounts).post(accounts::create_account),
        )
        .route("/api/accounts/:id", delete(accounts::delete_account))
        .route(
            "/api/accounts/:id/purge",
            delete(accounts::purge_account_data),
        )
        .route("/api/accounts/:id/puts", post(puts::open_put))
        .route("/api/trades/puts/:id/close", post(puts::close_put))
        .route(
            "/api/trades/:id",
            put(puts::edit_trade).delete(puts::delete_trade),
        )
        .route("/api/trades/:id/link-roll", post(puts::link_roll))
        .route("/api/accounts/:id/calls", post(calls::open_call))
        .route(
            "/api/accounts/:id/share-lots",
            get(calls::list_share_lots).post(share_lots::create_manual_lot),
        )
        .route("/api/share-lots/:id/sell", put(share_lots::sell_share_lot))
        .route(
            "/api/share-lots/recalculate",
            post(share_lots::recalculate_all),
        )
        .route("/api/trades/calls/:id/close", post(calls::close_call))
        .route("/api/dashboard", get(dashboard::get_dashboard))
        .route("/api/history", get(history::get_history))
        .route("/api/statistics", get(statistics::get_statistics))
        .layer(CorsLayer::permissive())
        .with_state(pool)
}
```

- [ ] **Step 2: cargo check**

```bash
cd /root/options_wheel_tracker/dev/backend && cargo check 2>&1 | tail -10
```
Expected: no errors (warnings OK)

- [ ] **Step 3: Commit**

```bash
git add backend/src/routes.rs
git commit -m "feat: register POST /api/trades/:id/link-roll route"
```

---

## Task 6: Frontend types and API client

**Files:**
- Modify: `frontend/src/lib/types.ts`
- Modify: `frontend/src/lib/api.ts`

- [ ] **Step 1: Update `TradeStatus` and `Trade` in `types.ts`**

```typescript
export type TradeStatus = 'OPEN' | 'EXPIRED' | 'BOUGHT_BACK' | 'ASSIGNED' | 'CALLED_AWAY';

export interface Trade {
  id: number;
  account_id: number;
  trade_type: TradeType;
  ticker: string;
  strike_price: number;
  expiry_date: string;
  open_date: string;
  premium_received: number;
  fees_open: number;
  status: TradeStatus;
  close_date: string | null;
  close_premium: number | null;
  fees_close: number | null;
  share_lot_id: number | null;
  quantity: number;
  created_at: string;
  deleted_at: string | null;
  rolled_from_trade_id: number | null;
  rolled_to_trade_id: number | null;
}
```

Note: `rolled_to_trade_id IS NOT NULL` is used by the frontend to display "ROLLED" badge. There is no `ROLLED` DB status — `status` stays `BOUGHT_BACK`.

- [ ] **Step 2: Add `trades.linkRoll` to `api.ts`**

In the `trades` section:

```typescript
trades: {
  edit: (tradeId: number, data: object) =>
    request<Trade>(`/api/trades/${tradeId}`, { method: 'PUT', body: JSON.stringify(data) }),
  delete: (tradeId: number) =>
    request<Trade>(`/api/trades/${tradeId}`, { method: 'DELETE' }),
  linkRoll: (sourceId: number, targetTradeId: number) =>
    request<{ source_id: number; target_id: number }>(
      `/api/trades/${sourceId}/link-roll`,
      { method: 'POST', body: JSON.stringify({ target_trade_id: targetTradeId }) }
    ),
},
```

- [ ] **Step 3: Commit**

```bash
git add frontend/src/lib/types.ts frontend/src/lib/api.ts
git commit -m "feat: add rolled_from/rolled_to to Trade type; add api.trades.linkRoll"
```

---

## Task 7: Close modals — add Roll option

**Files:**
- Modify: `frontend/src/components/trades/ClosePutModal.tsx`
- Modify: `frontend/src/components/trades/CloseCallModal.tsx`
- Modify: `frontend/src/components/dashboard/ActivePositions.tsx`

When "Roll" is selected, the modal collects the same fields as BOUGHT_BACK (buy-back price, fees, date), then calls the close API with `action: "BOUGHT_BACK"`, and finally navigates to the new trade form with `?rolled_from=<tradeId>`.

- [ ] **Step 1: Update `ClosePutModal.tsx`**

Replace the entire file content:

```typescript
'use client';
import { useState } from 'react';
import { useRouter } from 'next/navigation';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogTrigger } from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { api } from '@/lib/api';

interface Props {
  tradeId: number;
  onClose: () => void;
}

export function ClosePutModal({ tradeId, onClose }: Props) {
  const router = useRouter();
  const [open, setOpen] = useState(false);
  const [action, setAction] = useState('EXPIRED');
  const [closeDate, setCloseDate] = useState(new Date().toISOString().split('T')[0]);
  const [closePremium, setClosePremium] = useState('');
  const [feesClose, setFeesClose] = useState('1.30');
  const [error, setError] = useState('');

  const isRoll = action === 'ROLLED';
  const needsPremium = action === 'BOUGHT_BACK' || isRoll;

  const handleSubmit = async () => {
    try {
      await api.puts.close(tradeId, {
        action: isRoll ? 'BOUGHT_BACK' : action,
        close_date: closeDate,
        ...(needsPremium && { close_premium: parseFloat(closePremium), fees_close: parseFloat(feesClose) }),
      });
      setOpen(false);
      if (isRoll) {
        router.push(`/trades/new-put?rolled_from=${tradeId}`);
      } else {
        onClose();
      }
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : 'Failed to close trade');
    }
  };

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger render={<Button size="sm" variant="outline" />}>
        Close
      </DialogTrigger>
      <DialogContent>
        <DialogHeader><DialogTitle>Close PUT Trade</DialogTitle></DialogHeader>
        <div className="space-y-4">
          <div className="space-y-1">
            <Label>Action</Label>
            <Select value={action} onValueChange={(v) => v && setAction(v)}>
              <SelectTrigger><SelectValue /></SelectTrigger>
              <SelectContent>
                <SelectItem value="EXPIRED">Expired Worthless</SelectItem>
                <SelectItem value="BOUGHT_BACK">Bought Back</SelectItem>
                <SelectItem value="ASSIGNED">Assigned (got shares)</SelectItem>
                <SelectItem value="ROLLED">Roll to New PUT</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div className="space-y-1">
            <Label>Close Date</Label>
            <Input type="date" value={closeDate} onChange={(e) => setCloseDate(e.target.value)} />
          </div>
          {needsPremium && (
            <>
              <div className="space-y-1">
                <Label>Buy Back Price ($)</Label>
                <Input type="number" value={closePremium} onChange={(e) => setClosePremium(e.target.value)} placeholder="50.00" />
              </div>
              <div className="space-y-1">
                <Label>Fees ($)</Label>
                <Input type="number" value={feesClose} onChange={(e) => setFeesClose(e.target.value)} />
              </div>
            </>
          )}
          {isRoll && (
            <p className="text-sm text-muted-foreground">
              After confirming, you&apos;ll be taken to a pre-filled new PUT form to open the replacement leg.
            </p>
          )}
          {error && <p className="text-sm text-destructive">{error}</p>}
          <Button className="w-full" onClick={handleSubmit}>
            {isRoll ? 'Close & Roll' : 'Confirm Close'}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
```

- [ ] **Step 2: Update `CloseCallModal.tsx` — add Roll option + `shareLotId` prop**

Replace the entire file content:

```typescript
'use client';
import { useState } from 'react';
import { useRouter } from 'next/navigation';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogTrigger } from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { api } from '@/lib/api';

interface Props {
  tradeId: number;
  shareLotId: number | null;
  onClose: () => void;
}

export function CloseCallModal({ tradeId, shareLotId, onClose }: Props) {
  const router = useRouter();
  const [open, setOpen] = useState(false);
  const [action, setAction] = useState('EXPIRED');
  const [closeDate, setCloseDate] = useState(new Date().toISOString().split('T')[0]);
  const [closePremium, setClosePremium] = useState('');
  const [feesClose, setFeesClose] = useState('1.30');
  const [error, setError] = useState('');

  const isRoll = action === 'ROLLED';
  const needsPremium = action === 'BOUGHT_BACK' || isRoll;

  const handleSubmit = async () => {
    try {
      await api.calls.close(tradeId, {
        action: isRoll ? 'BOUGHT_BACK' : action,
        close_date: closeDate,
        ...(needsPremium && { close_premium: parseFloat(closePremium), fees_close: parseFloat(feesClose) }),
      });
      setOpen(false);
      if (isRoll) {
        const params = new URLSearchParams({ rolled_from: String(tradeId) });
        if (shareLotId) params.set('lot_id', String(shareLotId));
        router.push(`/trades/new-call?${params}`);
      } else {
        onClose();
      }
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : 'Failed');
    }
  };

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger render={<Button size="sm" variant="outline" />}>Close</DialogTrigger>
      <DialogContent>
        <DialogHeader><DialogTitle>Close CALL Trade</DialogTitle></DialogHeader>
        <div className="space-y-4">
          <div className="space-y-1">
            <Label>Action</Label>
            <Select value={action} onValueChange={(v) => v && setAction(v)}>
              <SelectTrigger><SelectValue /></SelectTrigger>
              <SelectContent>
                <SelectItem value="EXPIRED">Expired Worthless</SelectItem>
                <SelectItem value="BOUGHT_BACK">Bought Back</SelectItem>
                <SelectItem value="CALLED_AWAY">Called Away (shares sold)</SelectItem>
                <SelectItem value="ROLLED">Roll to New CALL</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div className="space-y-1">
            <Label>Close Date</Label>
            <Input type="date" value={closeDate} onChange={(e) => setCloseDate(e.target.value)} />
          </div>
          {needsPremium && (
            <>
              <div className="space-y-1">
                <Label>Buy Back Price ($)</Label>
                <Input type="number" value={closePremium} onChange={(e) => setClosePremium(e.target.value)} />
              </div>
              <div className="space-y-1">
                <Label>Fees ($)</Label>
                <Input type="number" value={feesClose} onChange={(e) => setFeesClose(e.target.value)} />
              </div>
            </>
          )}
          {isRoll && (
            <p className="text-sm text-muted-foreground">
              After confirming, you&apos;ll be taken to a pre-filled new CALL form to open the replacement leg.
            </p>
          )}
          {error && <p className="text-sm text-destructive">{error}</p>}
          <Button className="w-full" onClick={handleSubmit}>
            {isRoll ? 'Close & Roll' : 'Confirm Close'}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
```

- [ ] **Step 3: Pass `shareLotId` in `ActivePositions.tsx`**

Change the `CloseCallModal` call in `ActivePositions.tsx` from:

```typescript
<CloseCallModal tradeId={t.id} onClose={onTradeClose ?? (() => {})} />
```

to:

```typescript
<CloseCallModal tradeId={t.id} shareLotId={t.share_lot_id} onClose={onTradeClose ?? (() => {})} />
```

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/trades/ClosePutModal.tsx frontend/src/components/trades/CloseCallModal.tsx frontend/src/components/dashboard/ActivePositions.tsx
git commit -m "feat: add Roll option to close PUT/CALL modals"
```

---

## Task 8: PutForm — read rolled_from query param and submit it

**Files:**
- Modify: `frontend/src/components/trades/PutForm.tsx`

`PutForm` is already a `'use client'` component. It can call `useSearchParams()` directly to read `?rolled_from`.

- [ ] **Step 1: Update `PutForm.tsx`**

Replace the entire file:

```typescript
'use client';
import { useState } from 'react';
import { useRouter, useSearchParams } from 'next/navigation';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { api } from '@/lib/api';
import { useAccounts } from '@/contexts/AccountContext';

export function PutForm() {
  const router = useRouter();
  const searchParams = useSearchParams();
  const rolledFromParam = searchParams.get('rolled_from');
  const rolledFromTradeId = rolledFromParam ? parseInt(rolledFromParam) : undefined;

  const { selectedAccountId } = useAccounts();
  const [form, setForm] = useState({
    ticker: '', strike_price: '', expiry_date: '',
    open_date: new Date().toISOString().split('T')[0],
    premium_received: '', fees_open: '1.30', quantity: '1',
  });
  const [error, setError] = useState('');

  const set = (k: string) => (e: React.ChangeEvent<HTMLInputElement>) =>
    setForm((f) => ({ ...f, [k]: e.target.value }));

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!selectedAccountId) { setError('Select an account first'); return; }
    try {
      await api.puts.open(selectedAccountId, {
        ...form,
        ticker: form.ticker.toUpperCase(),
        strike_price: parseFloat(form.strike_price),
        premium_received: parseFloat(form.premium_received),
        fees_open: parseFloat(form.fees_open),
        quantity: parseInt(form.quantity),
        ...(rolledFromTradeId !== undefined && { rolled_from_trade_id: rolledFromTradeId }),
      });
      router.push('/');
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : 'Failed to open trade');
    }
  };

  return (
    <Card className="max-w-md">
      <CardHeader>
        <CardTitle>Sell to Open — PUT</CardTitle>
        {rolledFromTradeId && (
          <p className="text-sm text-muted-foreground">Rolling from trade #{rolledFromTradeId}</p>
        )}
      </CardHeader>
      <CardContent>
        <form onSubmit={handleSubmit} className="space-y-4">
          {[
            { label: 'Ticker', key: 'ticker', placeholder: 'AAPL', type: 'text' },
            { label: 'Strike Price', key: 'strike_price', placeholder: '150.00', type: 'number' },
            { label: 'Expiry Date', key: 'expiry_date', placeholder: '', type: 'date' },
            { label: 'Open Date', key: 'open_date', placeholder: '', type: 'date' },
            { label: 'Premium Received ($)', key: 'premium_received', placeholder: '200.00', type: 'number' },
            { label: 'Fees ($)', key: 'fees_open', placeholder: '1.30', type: 'number' },
            { label: 'Quantity (contracts)', key: 'quantity', placeholder: '1', type: 'number' },
          ].map(({ label, key, placeholder, type }) => (
            <div key={key} className="space-y-1">
              <Label>{label}</Label>
              <Input type={type} placeholder={placeholder}
                value={form[key as keyof typeof form]} onChange={set(key)} required />
            </div>
          ))}
          {error && <p className="text-sm text-destructive">{error}</p>}
          <Button type="submit" className="w-full">
            {rolledFromTradeId ? 'Open Rolled PUT' : 'Open PUT Trade'}
          </Button>
        </form>
      </CardContent>
    </Card>
  );
}
```

- [ ] **Step 2: Commit**

```bash
git add frontend/src/components/trades/PutForm.tsx
git commit -m "feat: PutForm reads rolled_from query param and includes it in submit"
```

---

## Task 9: CallForm — read rolled_from + lot_id query params

**Files:**
- Modify: `frontend/src/components/trades/CallForm.tsx`

When `?rolled_from=N&lot_id=M` is present, the form pre-selects the lot and shows a roll banner.

- [ ] **Step 1: Update `CallForm.tsx`**

Replace the `useEffect` and the initial state to incorporate `useSearchParams`:

```typescript
'use client';
import { useEffect, useState } from 'react';
import { useRouter, useSearchParams } from 'next/navigation';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { api } from '@/lib/api';
import { formatCurrency } from '@/lib/utils';
import { useAccounts } from '@/contexts/AccountContext';
import type { ShareLot } from '@/lib/types';

export function CallForm() {
  const router = useRouter();
  const searchParams = useSearchParams();
  const rolledFromParam = searchParams.get('rolled_from');
  const lotIdParam = searchParams.get('lot_id');
  const rolledFromTradeId = rolledFromParam ? parseInt(rolledFromParam) : undefined;

  const { selectedAccountId } = useAccounts();
  const [lots, setLots] = useState<ShareLot[]>([]);
  const [selectedLotId, setSelectedLotId] = useState(lotIdParam ?? '');
  const [form, setForm] = useState({
    ticker: '', strike_price: '', expiry_date: '',
    open_date: new Date().toISOString().split('T')[0],
    premium_received: '', fees_open: '1.30', quantity: '1',
  });
  const [error, setError] = useState('');

  useEffect(() => {
    if (selectedAccountId) {
      api.shareLots.list(selectedAccountId).then((l) => {
        setLots(l);
        const initialId = lotIdParam ?? (l.length === 1 ? String(l[0].id) : '');
        if (initialId) {
          setSelectedLotId(initialId);
          const lot = l.find((x) => x.id === Number(initialId));
          if (lot) setForm((f) => ({ ...f, ticker: lot.ticker }));
        }
      });
    }
  }, [selectedAccountId, lotIdParam]);

  const handleLotChange = (id: string | null) => {
    if (!id) return;
    setSelectedLotId(id);
    const lot = lots.find((l) => l.id === Number(id));
    if (lot) setForm((f) => ({ ...f, ticker: lot.ticker }));
  };

  const set = (k: string) => (e: React.ChangeEvent<HTMLInputElement>) =>
    setForm((f) => ({ ...f, [k]: e.target.value }));

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!selectedAccountId) { setError('Select an account first'); return; }
    if (!selectedLotId) { setError('Select a share lot'); return; }
    try {
      await api.calls.open(selectedAccountId, {
        share_lot_id: Number(selectedLotId),
        ...form,
        ticker: form.ticker.toUpperCase(),
        strike_price: parseFloat(form.strike_price),
        premium_received: parseFloat(form.premium_received),
        fees_open: parseFloat(form.fees_open),
        quantity: parseInt(form.quantity),
        ...(rolledFromTradeId !== undefined && { rolled_from_trade_id: rolledFromTradeId }),
      });
      router.push('/');
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : 'Failed to open trade');
    }
  };

  const selectedLot = lots.find((l) => l.id === Number(selectedLotId));

  return (
    <Card className="max-w-md">
      <CardHeader>
        <CardTitle>Sell to Open — CALL</CardTitle>
        {rolledFromTradeId && (
          <p className="text-sm text-muted-foreground">Rolling from trade #{rolledFromTradeId}</p>
        )}
      </CardHeader>
      <CardContent>
        <form onSubmit={handleSubmit} className="space-y-4">
          <div className="space-y-1">
            <Label>Share Lot</Label>
            {lots.length === 0 ? (
              <p className="text-sm text-muted-foreground">No active share lots. Assign a PUT first.</p>
            ) : (
              <Select value={selectedLotId || null} onValueChange={handleLotChange}>
                <SelectTrigger>
                  <SelectValue placeholder="Select lot">
                    {selectedLot
                      ? `${selectedLot.ticker} — ${selectedLot.quantity} shares @ ${formatCurrency(selectedLot.adjusted_cost_basis)} adj. CB`
                      : undefined}
                  </SelectValue>
                </SelectTrigger>
                <SelectContent>
                  {lots.map((l) => (
                    <SelectItem key={l.id} value={String(l.id)}>
                      {l.ticker} — {l.quantity} shares @ {formatCurrency(l.adjusted_cost_basis)} adj. CB
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            )}
          </div>
          {selectedLot && (
            <div className="text-sm bg-muted rounded p-3 space-y-1">
              <div>Original CB: {formatCurrency(selectedLot.original_cost_basis)}/share</div>
              <div>Adjusted CB: <strong>{formatCurrency(selectedLot.adjusted_cost_basis)}/share</strong></div>
            </div>
          )}
          {[
            { label: 'Ticker', key: 'ticker', placeholder: 'AAPL', type: 'text' },
            { label: 'Strike Price', key: 'strike_price', placeholder: '155.00', type: 'number' },
            { label: 'Expiry Date', key: 'expiry_date', placeholder: '', type: 'date' },
            { label: 'Open Date', key: 'open_date', placeholder: '', type: 'date' },
            { label: 'Premium Received ($)', key: 'premium_received', placeholder: '150.00', type: 'number' },
            { label: 'Fees ($)', key: 'fees_open', placeholder: '1.30', type: 'number' },
            { label: 'Quantity (contracts)', key: 'quantity', placeholder: '1', type: 'number' },
          ].map(({ label, key, placeholder, type }) => (
            <div key={key} className="space-y-1">
              <Label>{label}</Label>
              <Input type={type} placeholder={placeholder}
                value={form[key as keyof typeof form]} onChange={set(key)} required />
            </div>
          ))}
          {error && <p className="text-sm text-destructive">{error}</p>}
          <Button type="submit" className="w-full" disabled={lots.length === 0}>
            {rolledFromTradeId ? 'Open Rolled CALL' : 'Open CALL Trade'}
          </Button>
        </form>
      </CardContent>
    </Card>
  );
}
```

- [ ] **Step 2: Commit**

```bash
git add frontend/src/components/trades/CallForm.tsx
git commit -m "feat: CallForm reads rolled_from + lot_id query params"
```

---

## Task 10: TradeTable — ROLLED badge + Link Roll button

**Files:**
- Modify: `frontend/src/components/history/TradeTable.tsx`

A trade is displayed as "ROLLED" when `rolled_to_trade_id !== null`. The "Link Roll" button appears on `BOUGHT_BACK` trades where `rolled_to_trade_id === null && rolled_from_trade_id === null`.

- [ ] **Step 1: Update `TradeTable.tsx`**

Replace the entire file:

```typescript
'use client';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';
import { EditTradeModal } from '@/components/trades/EditTradeModal';
import { LinkRollModal } from '@/components/history/LinkRollModal';
import { api } from '@/lib/api';
import { formatCurrency } from '@/lib/utils';
import type { Trade } from '@/lib/types';

const STATUS_COLORS: Record<string, 'default' | 'secondary' | 'destructive' | 'outline'> = {
  OPEN: 'default', EXPIRED: 'secondary', BOUGHT_BACK: 'outline',
  ASSIGNED: 'secondary', CALLED_AWAY: 'outline', ROLLED: 'outline',
};

function netPremium(t: Trade): number {
  return t.premium_received - t.fees_open - (t.close_premium ?? 0) - (t.fees_close ?? 0);
}

function displayStatus(t: Trade): string {
  if (t.rolled_to_trade_id !== null) return 'ROLLED';
  return t.status;
}

interface Props { trades: Trade[]; onTradeUpdate?: () => void; }

export function TradeTable({ trades, onTradeUpdate }: Props) {
  const handleDelete = async (tradeId: number) => {
    await api.trades.delete(tradeId);
    onTradeUpdate?.();
  };

  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>Ticker</TableHead>
          <TableHead>Type</TableHead>
          <TableHead>Qty</TableHead>
          <TableHead>Strike</TableHead>
          <TableHead>Open Date</TableHead>
          <TableHead>Close Date</TableHead>
          <TableHead>Premium</TableHead>
          <TableHead>Net</TableHead>
          <TableHead>Status</TableHead>
          <TableHead></TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {trades.length === 0 && (
          <TableRow><TableCell colSpan={10} className="text-center text-muted-foreground">No trades found</TableCell></TableRow>
        )}
        {trades.map((t) => {
          const status = displayStatus(t);
          return (
            <TableRow key={t.id} className={t.deleted_at ? 'opacity-50' : ''}>
              <TableCell className={`font-medium ${t.deleted_at ? 'line-through' : ''}`}>{t.ticker}</TableCell>
              <TableCell><Badge variant={t.trade_type === 'PUT' ? 'secondary' : 'default'}>{t.trade_type}</Badge></TableCell>
              <TableCell className={t.deleted_at ? 'line-through' : ''}>{t.quantity}</TableCell>
              <TableCell className={t.deleted_at ? 'line-through' : ''}>{formatCurrency(t.strike_price)}</TableCell>
              <TableCell className={t.deleted_at ? 'line-through' : ''}>{t.open_date}</TableCell>
              <TableCell className={t.deleted_at ? 'line-through' : ''}>{t.close_date ?? '—'}</TableCell>
              <TableCell className={t.deleted_at ? 'line-through' : ''}>{formatCurrency(t.premium_received)}</TableCell>
              <TableCell className={`${t.deleted_at ? 'line-through ' : ''}${netPremium(t) >= 0 ? 'text-green-600' : 'text-red-500'}`}>
                {formatCurrency(netPremium(t))}
              </TableCell>
              <TableCell><Badge variant={STATUS_COLORS[status] ?? 'outline'}>{status}</Badge></TableCell>
              <TableCell className="space-x-1">
                {!t.deleted_at && (
                  <>
                    <EditTradeModal trade={t} onSave={onTradeUpdate ?? (() => {})} />
                    {t.status === 'BOUGHT_BACK' && t.rolled_to_trade_id === null && t.rolled_from_trade_id === null && (
                      <LinkRollModal trade={t} trades={trades} onLink={onTradeUpdate ?? (() => {})} />
                    )}
                    <Button variant="destructive" size="xs" onClick={() => handleDelete(t.id)}>Delete</Button>
                  </>
                )}
              </TableCell>
            </TableRow>
          );
        })}
      </TableBody>
    </Table>
  );
}
```

- [ ] **Step 2: Commit (after LinkRollModal is written in next task)**

Hold this commit until Task 11 is done.

---

## Task 11: LinkRollModal — retroactive roll linking

**Files:**
- Create: `frontend/src/components/history/LinkRollModal.tsx`

This modal shows a list of trades that the source trade could have been rolled into: same ticker, same trade type, opened on or after the source trade's close date, and not already linked.

- [ ] **Step 1: Create `LinkRollModal.tsx`**

```typescript
'use client';
import { useState } from 'react';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogTrigger } from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { api } from '@/lib/api';
import { formatCurrency } from '@/lib/utils';
import type { Trade } from '@/lib/types';

interface Props {
  trade: Trade;
  trades: Trade[];
  onLink: () => void;
}

export function LinkRollModal({ trade, trades, onLink }: Props) {
  const [open, setOpen] = useState(false);
  const [error, setError] = useState('');

  // Candidates: same ticker + type, opened on or after this trade's close date, not already linked
  const candidates = trades.filter(
    (t) =>
      t.id !== trade.id &&
      t.ticker === trade.ticker &&
      t.trade_type === trade.trade_type &&
      trade.close_date !== null &&
      t.open_date >= trade.close_date &&
      t.rolled_from_trade_id === null
  );

  const handleLink = async (targetId: number) => {
    try {
      await api.trades.linkRoll(trade.id, targetId);
      setOpen(false);
      onLink();
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : 'Failed to link roll');
    }
  };

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger render={<Button size="xs" variant="outline" />}>
        Link Roll
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Link Roll — {trade.ticker} {trade.trade_type}</DialogTitle>
        </DialogHeader>
        <div className="space-y-3">
          <p className="text-sm text-muted-foreground">
            Select the trade this was rolled into. Trade #{trade.id} will be marked as ROLLED.
          </p>
          {candidates.length === 0 && (
            <p className="text-sm text-muted-foreground">
              No candidates found. The replacement trade must have the same ticker, same type, and open on or after {trade.close_date}.
            </p>
          )}
          {candidates.map((t) => (
            <div key={t.id} className="flex items-center justify-between border rounded p-3">
              <div className="text-sm space-y-0.5">
                <div className="font-medium">#{t.id} — {t.ticker} {t.trade_type}</div>
                <div className="text-muted-foreground">
                  Opened {t.open_date} · Strike {formatCurrency(t.strike_price)} · {t.status}
                </div>
              </div>
              <Button size="sm" onClick={() => handleLink(t.id)}>Select</Button>
            </div>
          ))}
          {error && <p className="text-sm text-destructive">{error}</p>}
        </div>
      </DialogContent>
    </Dialog>
  );
}
```

- [ ] **Step 2: Commit TradeTable + LinkRollModal together**

```bash
git add frontend/src/components/history/TradeTable.tsx frontend/src/components/history/LinkRollModal.tsx
git commit -m "feat: ROLLED badge and Link Roll action in trade history"
```

---

## Task 12: Build verification

- [ ] **Step 1: Run backend tests**

```bash
cd /root/options_wheel_tracker/dev/backend && cargo test 2>&1 | tail -20
```
Expected: all tests pass

- [ ] **Step 2: Run frontend build**

```bash
cd /root/options_wheel_tracker/dev/frontend && npm run build 2>&1 | tail -30
```
Expected: Build completed successfully, no errors

- [ ] **Step 3: Run migration smoke test**

```bash
cd /root/options_wheel_tracker/dev && bash scripts/test-migration.sh
```
Expected: `Migration test passed`

- [ ] **Step 4: Final commit**

```bash
git add -u
git commit -m "chore: roll linking feature complete"
```
