# Options Wheel Tracker — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a responsive web app to track options wheel strategy trades across multiple brokerage accounts, with cost basis tracking and yield metrics.

**Architecture:** Rust/Axum REST API backend with SQLite via SQLx; Next.js frontend consuming the API. Apps run directly on the home server (no Docker for MVP). Two git worktrees under the project root — `prod/` tracks `main`, `dev/` tracks the dev branch — with separate SQLite files so production data is never touched during development. Docker support is planned for the future.

**Tech Stack:** Rust, Axum, SQLx, SQLite, Next.js 14 (App Router), TypeScript, Tailwind CSS, shadcn/ui

---

## File Map

### Backend (`backend/`)
```
Cargo.toml
Dockerfile
src/
  main.rs                  — server startup, router assembly
  config.rs                — env config (port, db path)
  errors.rs                — AppError type, IntoResponse impl
  db/
    mod.rs                 — connection pool init
    migrations/
      001_initial.sql      — full schema
  models/
    mod.rs
    account.rs             — Account struct + DB queries
    trade.rs               — Trade struct + DB queries (PUT & CALL)
    share_lot.rs           — ShareLot struct + DB queries
  handlers/
    mod.rs
    accounts.rs            — CRUD for accounts
    puts.rs                — open/close PUT trades
    calls.rs               — open/close CALL trades
    share_lots.rs          — list lots, manual lot creation
    dashboard.rs           — aggregate metrics
    history.rs             — filtered trade history
  routes.rs                — all route definitions
```

### Frontend (`frontend/`)
```
Dockerfile
next.config.ts
src/
  lib/
    api.ts                 — typed fetch wrappers for all endpoints
    types.ts               — TypeScript interfaces mirroring backend models
    utils.ts               — date formatting, currency, annualized yield calc
  components/
    ui/                    — shadcn/ui components (auto-generated)
    layout/
      Sidebar.tsx          — nav: Dashboard, New Trade, History
      AccountSelector.tsx  — global account context switcher
    dashboard/
      MetricCard.tsx       — reusable stat card
      ActivePositions.tsx  — table of open puts + share lots w/ calls
    trades/
      PutForm.tsx          — STO PUT entry form
      CallForm.tsx         — STO CALL entry form (lot picker)
      ClosePutModal.tsx    — expire / buy back / assign flow
      CloseCallModal.tsx   — expire / buy back / called away flow
    history/
      TradeTable.tsx       — sortable trade history table
      FilterBar.tsx        — time period + ticker filters
  app/
    layout.tsx             — root layout with sidebar
    page.tsx               — dashboard
    history/page.tsx       — history page
    trades/
      new-put/page.tsx
      new-call/page.tsx
```

### Root (outside both worktrees, at `/root/options_wheel_tracker/`)
```
Makefile              — start/stop prod+dev, refresh-dev-db
.gitignore            — excludes .env and data/ (committed to repo)
scripts/
  refresh-dev-db.sh   — safely copies prod.db → dev.db
data/
  prod.db             — production SQLite file (never in a worktree)
  dev.db              — dev SQLite file
logs/
  refresh.log         — output from scheduled db refresh

prod/                 — git worktree tracking main branch
dev/                  — git worktree tracking dev branch
  backend/
  frontend/
  .env.example        — committed template
  .env                — local config (not committed)
```

Each worktree contains `backend/`, `frontend/`, `.env.example` (committed), and `.env` (not committed). The `data/` directory lives outside both worktrees so SQLite files can never be accidentally committed or affected by branch switches.

---

## Database Schema

```sql
-- 001_initial.sql

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
  expiry_date      TEXT NOT NULL,        -- ISO8601 date
  open_date        TEXT NOT NULL,        -- ISO8601 date
  premium_received REAL NOT NULL,        -- total, not per share
  fees_open        REAL NOT NULL DEFAULT 0,
  status           TEXT NOT NULL DEFAULT 'OPEN'
                   CHECK(status IN ('OPEN','EXPIRED','BOUGHT_BACK','ASSIGNED','CALLED_AWAY')),
  close_date       TEXT,
  close_premium    REAL,                 -- price paid to buy back (positive = cost)
  fees_close       REAL,
  share_lot_id     INTEGER REFERENCES share_lots(id),  -- for CALL trades
  created_at       TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE share_lots (
  id                   INTEGER PRIMARY KEY AUTOINCREMENT,
  account_id           INTEGER NOT NULL REFERENCES accounts(id),
  ticker               TEXT NOT NULL,
  quantity             INTEGER NOT NULL DEFAULT 100,
  original_cost_basis  REAL NOT NULL,   -- per share
  adjusted_cost_basis  REAL NOT NULL,   -- per share, decreases as calls are sold
  acquisition_date     TEXT NOT NULL,
  acquisition_type     TEXT NOT NULL CHECK(acquisition_type IN ('MANUAL','ASSIGNED')),
  source_trade_id      INTEGER REFERENCES trades(id),  -- set when ASSIGNED
  status               TEXT NOT NULL DEFAULT 'ACTIVE'
                       CHECK(status IN ('ACTIVE','CALLED_AWAY')),
  created_at           TEXT NOT NULL DEFAULT (datetime('now'))
);
```

---

## Task 1: Project Scaffold

**Files:**
- Create: `backend/Cargo.toml`
- Create: `backend/src/main.rs`
- Create: `frontend/` (Next.js app)
- Create: `docker-compose.yml`
- Create: `.env.example`

- [ ] **Step 1: Initialize backend**
```bash
cd /root/options_wheel_tracker
cargo new backend
cd backend
```

- [ ] **Step 2: Set Cargo.toml dependencies**

Replace `backend/Cargo.toml` contents:
```toml
[package]
name = "wheel-tracker"
version = "0.1.0"
edition = "2021"

[dependencies]
axum = { version = "0.7", features = ["macros"] }
tokio = { version = "1", features = ["full"] }
sqlx = { version = "0.7", features = ["sqlite", "runtime-tokio", "migrate", "chrono"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tower-http = { version = "0.5", features = ["cors"] }
chrono = { version = "0.4", features = ["serde"] }
dotenvy = "0.15"
tracing = "0.1"
tracing-subscriber = "0.3"
thiserror = "1"

[dev-dependencies]
axum-test = "15"
```

- [ ] **Step 3: Initialize frontend**
```bash
cd /root/options_wheel_tracker
npx create-next-app@latest frontend \
  --typescript --tailwind --eslint \
  --app --src-dir --no-turbopack \
  --import-alias "@/*"
```

- [ ] **Step 4: Install frontend deps**
```bash
cd frontend
npx shadcn@latest init -d
npx shadcn@latest add button card input label select table dialog form
npm install @tanstack/react-query axios date-fns
```

- [ ] **Step 5: Create .gitignore**

Create `.gitignore` at the repo root:
```
.env
data/
logs/
target/
node_modules/
.next/
```

- [ ] **Step 6: Create .env.example**

Create `.env.example` in the repo root (committed — this is the template both worktrees use):
```
# Three slashes = sqlite:// + absolute path. SQLx requires this exact format.
DATABASE_URL=sqlite:///root/options_wheel_tracker/data/dev.db
BACKEND_PORT=3003
FRONTEND_PORT=3002
NEXT_PUBLIC_API_URL=http://localhost:3003
```

- [ ] **Step 7: Create local .env and data directory**
```bash
# Run from whichever worktree you are setting up (dev or prod)
cp .env.example .env
# Edit .env — set correct DATABASE_URL and ports for this environment:
#   prod: DATABASE_URL=.../data/prod.db, BACKEND_PORT=3001, FRONTEND_PORT=3000
#   dev:  DATABASE_URL=.../data/dev.db,  BACKEND_PORT=3003, FRONTEND_PORT=3002
mkdir -p /root/options_wheel_tracker/data
mkdir -p /root/options_wheel_tracker/logs
```

- [ ] **Step 8: Verify backend compiles**
```bash
cd backend && cargo build
```
Expected: compiles successfully.

- [ ] **Step 9: Commit**
```bash
cd /root/options_wheel_tracker
git init
git add .
git commit -m "feat: project scaffold — Rust/Axum backend + Next.js frontend"
```

---

## Task 2: Database Setup + Migration

**Files:**
- Create: `backend/src/db/mod.rs`
- Create: `backend/src/db/migrations/001_initial.sql`
- Modify: `backend/src/main.rs`

- [ ] **Step 1: Create migration file**
```bash
mkdir -p backend/src/db/migrations
```

Create `backend/src/db/migrations/001_initial.sql` with the schema from the File Map section above.

- [ ] **Step 2: Create db/mod.rs**
```rust
// backend/src/db/mod.rs
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};

pub type Pool = SqlitePool;

pub async fn init_pool(database_url: &str) -> SqlitePool {
    SqlitePoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await
        .expect("Failed to connect to database")
}

pub async fn run_migrations(pool: &SqlitePool) {
    sqlx::migrate!("src/db/migrations")
        .run(pool)
        .await
        .expect("Failed to run migrations");
}
```

- [ ] **Step 3: Wire up main.rs**
```rust
// backend/src/main.rs
mod db;
mod errors;
mod models;
mod handlers;
mod routes;
mod config;

use dotenvy::dotenv;
use std::env;

#[tokio::main]
async fn main() {
    dotenv().ok();
    tracing_subscriber::fmt::init();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let port = env::var("PORT").unwrap_or_else(|_| "3001".to_string());

    let pool = db::init_pool(&database_url).await;
    db::run_migrations(&pool).await;

    let app = routes::create_router(pool);
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port))
        .await
        .unwrap();

    tracing::info!("Listening on port {}", port);
    axum::serve(listener, app).await.unwrap();
}
```

- [ ] **Step 4: Create stub modules so it compiles**

Create `backend/src/errors.rs`:
```rust
use axum::{http::StatusCode, response::{IntoResponse, Response}, Json};
use serde_json::json;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("Not found")]
    NotFound,
    #[error("Bad request: {0}")]
    BadRequest(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AppError::NotFound => (StatusCode::NOT_FOUND, self.to_string()),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            AppError::Database(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Database error".to_string()),
        };
        (status, Json(json!({ "error": message }))).into_response()
    }
}
```

Create `backend/src/config.rs`: `// placeholder`
Create `backend/src/models/mod.rs`: `// placeholder`
Create `backend/src/handlers/mod.rs`: `// placeholder`

Create `backend/src/routes.rs`:
```rust
use axum::Router;
use sqlx::SqlitePool;
use tower_http::cors::CorsLayer;

pub fn create_router(pool: SqlitePool) -> Router {
    Router::new()
        .layer(CorsLayer::permissive())
        .with_state(pool)
}
```

- [ ] **Step 5: Verify it compiles and runs**
```bash
cd backend && cargo run
```
Expected: "Listening on port 3001", database file created at `data/wheel.db`.

- [ ] **Step 6: Commit**
```bash
git add backend/src/ && git commit -m "feat: database schema + SQLx migration setup"
```

---

## Task 3: Account Model + API

**Files:**
- Create: `backend/src/models/account.rs`
- Create: `backend/src/handlers/accounts.rs`
- Modify: `backend/src/models/mod.rs`
- Modify: `backend/src/handlers/mod.rs`
- Modify: `backend/src/routes.rs`

- [ ] **Step 1: Write failing tests**

Create `backend/src/handlers/accounts.rs` with tests first:
```rust
#[cfg(test)]
mod tests {
    use axum_test::TestServer;
    use serde_json::json;
    use crate::routes::create_router;
    use crate::db;

    async fn test_server() -> TestServer {
        let pool = db::init_pool("sqlite::memory:").await;
        db::run_migrations(&pool).await;
        TestServer::new(create_router(pool)).unwrap()
    }

    #[tokio::test]
    async fn test_create_account() {
        let server = test_server().await;
        let res = server.post("/api/accounts")
            .json(&json!({ "name": "Fidelity" }))
            .await;
        res.assert_status_created();
        let body = res.json::<serde_json::Value>();
        assert_eq!(body["name"], "Fidelity");
        assert!(body["id"].is_number());
    }

    #[tokio::test]
    async fn test_list_accounts() {
        let server = test_server().await;
        server.post("/api/accounts").json(&json!({ "name": "Fidelity" })).await;
        let res = server.get("/api/accounts").await;
        res.assert_status_ok();
        let body = res.json::<serde_json::Value>();
        assert_eq!(body.as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_delete_account() {
        let server = test_server().await;
        let create = server.post("/api/accounts")
            .json(&json!({ "name": "TDA" })).await;
        let id = create.json::<serde_json::Value>()["id"].as_i64().unwrap();
        let res = server.delete(&format!("/api/accounts/{}", id)).await;
        res.assert_status_ok();
    }
}
```

- [ ] **Step 2: Run tests — verify they fail**
```bash
cd backend && cargo test test_create_account 2>&1 | head -20
```
Expected: compile error (handlers not implemented yet).

- [ ] **Step 3: Implement Account model**

Create `backend/src/models/account.rs`:
```rust
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use crate::errors::AppError;

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Account {
    pub id: i64,
    pub name: String,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateAccount {
    pub name: String,
}

impl Account {
    pub async fn create(pool: &SqlitePool, name: &str) -> Result<Account, AppError> {
        let account = sqlx::query_as!(
            Account,
            "INSERT INTO accounts (name) VALUES (?) RETURNING id, name, created_at",
            name
        )
        .fetch_one(pool)
        .await?;
        Ok(account)
    }

    pub async fn list(pool: &SqlitePool) -> Result<Vec<Account>, AppError> {
        let accounts = sqlx::query_as!(Account, "SELECT id, name, created_at FROM accounts ORDER BY created_at")
            .fetch_all(pool)
            .await?;
        Ok(accounts)
    }

    pub async fn delete(pool: &SqlitePool, id: i64) -> Result<(), AppError> {
        let result = sqlx::query!("DELETE FROM accounts WHERE id = ?", id)
            .execute(pool)
            .await?;
        if result.rows_affected() == 0 {
            return Err(AppError::NotFound);
        }
        Ok(())
    }
}
```

Update `backend/src/models/mod.rs`:
```rust
pub mod account;
```

- [ ] **Step 4: Implement account handlers**

Replace `backend/src/handlers/accounts.rs` with full implementation + tests:
```rust
use axum::{extract::{Path, State}, http::StatusCode, Json};
use sqlx::SqlitePool;
use crate::{errors::AppError, models::account::{Account, CreateAccount}};

pub async fn list_accounts(
    State(pool): State<SqlitePool>,
) -> Result<Json<Vec<Account>>, AppError> {
    Account::list(&pool).await.map(Json)
}

pub async fn create_account(
    State(pool): State<SqlitePool>,
    Json(payload): Json<CreateAccount>,
) -> Result<(StatusCode, Json<Account>), AppError> {
    if payload.name.trim().is_empty() {
        return Err(AppError::BadRequest("name cannot be empty".to_string()));
    }
    Account::create(&pool, &payload.name)
        .await
        .map(|a| (StatusCode::CREATED, Json(a)))
}

pub async fn delete_account(
    State(pool): State<SqlitePool>,
    Path(id): Path<i64>,
) -> Result<StatusCode, AppError> {
    Account::delete(&pool, id).await?;
    Ok(StatusCode::OK)
}

#[cfg(test)]
mod tests {
    use axum_test::TestServer;
    use serde_json::json;
    use crate::routes::create_router;
    use crate::db;

    async fn test_server() -> TestServer {
        let pool = db::init_pool("sqlite::memory:").await;
        db::run_migrations(&pool).await;
        TestServer::new(create_router(pool)).unwrap()
    }

    #[tokio::test]
    async fn test_create_account() {
        let server = test_server().await;
        let res = server.post("/api/accounts")
            .json(&json!({ "name": "Fidelity" }))
            .await;
        res.assert_status_created();
        let body = res.json::<serde_json::Value>();
        assert_eq!(body["name"], "Fidelity");
        assert!(body["id"].is_number());
    }

    #[tokio::test]
    async fn test_list_accounts() {
        let server = test_server().await;
        server.post("/api/accounts").json(&json!({ "name": "Fidelity" })).await;
        let res = server.get("/api/accounts").await;
        res.assert_status_ok();
        let body = res.json::<serde_json::Value>();
        assert_eq!(body.as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_delete_account() {
        let server = test_server().await;
        let create = server.post("/api/accounts")
            .json(&json!({ "name": "TDA" })).await;
        let id = create.json::<serde_json::Value>()["id"].as_i64().unwrap();
        let res = server.delete(&format!("/api/accounts/{}", id)).await;
        res.assert_status_ok();
    }
}
```

Update `backend/src/handlers/mod.rs`:
```rust
pub mod accounts;
```

- [ ] **Step 5: Wire routes**

Update `backend/src/routes.rs`:
```rust
use axum::{routing::{delete, get, post}, Router};
use sqlx::SqlitePool;
use tower_http::cors::CorsLayer;
use crate::handlers::accounts;

pub fn create_router(pool: SqlitePool) -> Router {
    Router::new()
        .route("/api/accounts", get(accounts::list_accounts).post(accounts::create_account))
        .route("/api/accounts/:id", delete(accounts::delete_account))
        .layer(CorsLayer::permissive())
        .with_state(pool)
}
```

- [ ] **Step 6: Run tests — verify they pass**
```bash
cd backend && cargo test accounts 2>&1
```
Expected: 3 tests pass.

- [ ] **Step 7: Commit**
```bash
git add backend/src/ && git commit -m "feat: account model + REST API (list, create, delete)"
```

---

## Task 4: Share Lot Model

**Files:**
- Create: `backend/src/models/share_lot.rs`
- Modify: `backend/src/models/mod.rs`

- [ ] **Step 1: Write failing tests**

Create `backend/src/models/share_lot.rs` with tests:
```rust
#[cfg(test)]
mod tests {
    use crate::db;
    use crate::models::{account::Account, share_lot::{CreateShareLot, ShareLot}};

    #[tokio::test]
    async fn test_create_and_list_lot() {
        let pool = db::init_pool("sqlite::memory:").await;
        db::run_migrations(&pool).await;
        let acct = Account::create(&pool, "Test").await.unwrap();
        let lot = ShareLot::create(&pool, &CreateShareLot {
            account_id: acct.id,
            ticker: "AAPL".to_string(),
            original_cost_basis: 150.00,
            adjusted_cost_basis: None, // defaults to original
            acquisition_date: "2025-01-10".to_string(),
            acquisition_type: "MANUAL".to_string(),
            source_trade_id: None,
        }).await.unwrap();
        assert_eq!(lot.ticker, "AAPL");
        assert_eq!(lot.adjusted_cost_basis, 150.00);

        let lots = ShareLot::list_active(&pool, acct.id).await.unwrap();
        assert_eq!(lots.len(), 1);
    }

    #[tokio::test]
    async fn test_reduce_cost_basis() {
        let pool = db::init_pool("sqlite::memory:").await;
        db::run_migrations(&pool).await;
        let acct = Account::create(&pool, "Test").await.unwrap();
        let lot = ShareLot::create(&pool, &CreateShareLot {
            account_id: acct.id,
            ticker: "AAPL".to_string(),
            original_cost_basis: 150.00,
            adjusted_cost_basis: None,
            acquisition_date: "2025-01-10".to_string(),
            acquisition_type: "MANUAL".to_string(),
            source_trade_id: None,
        }).await.unwrap();
        // Premium of $50 total on 100 shares = $0.50/share reduction
        ShareLot::reduce_cost_basis(&pool, lot.id, 50.0).await.unwrap();
        let updated = ShareLot::get(&pool, lot.id).await.unwrap();
        assert!((updated.adjusted_cost_basis - 149.50).abs() < 0.001);
    }
}
```

- [ ] **Step 2: Run — verify fails**
```bash
cd backend && cargo test share_lot 2>&1 | head -10
```

- [ ] **Step 3: Implement ShareLot model**
```rust
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use crate::errors::AppError;

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct ShareLot {
    pub id: i64,
    pub account_id: i64,
    pub ticker: String,
    pub quantity: i64,
    pub original_cost_basis: f64,
    pub adjusted_cost_basis: f64,
    pub acquisition_date: String,
    pub acquisition_type: String,
    pub source_trade_id: Option<i64>,
    pub status: String,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateShareLot {
    pub account_id: i64,
    pub ticker: String,
    pub original_cost_basis: f64,
    /// If None, defaults to original_cost_basis (manual entry). Pass explicitly for ASSIGNED lots.
    pub adjusted_cost_basis: Option<f64>,
    pub acquisition_date: String,
    pub acquisition_type: String,
    pub source_trade_id: Option<i64>,
}

impl ShareLot {
    pub async fn create(pool: &SqlitePool, input: &CreateShareLot) -> Result<ShareLot, AppError> {
        let adj_cb = input.adjusted_cost_basis.unwrap_or(input.original_cost_basis);
        let lot = sqlx::query_as!(ShareLot,
            r#"INSERT INTO share_lots
               (account_id, ticker, original_cost_basis, adjusted_cost_basis, acquisition_date, acquisition_type, source_trade_id)
               VALUES (?, ?, ?, ?, ?, ?, ?)
               RETURNING id, account_id, ticker, quantity, original_cost_basis, adjusted_cost_basis,
                         acquisition_date, acquisition_type, source_trade_id, status, created_at"#,
            input.account_id, input.ticker, input.original_cost_basis, adj_cb,
            input.acquisition_date, input.acquisition_type, input.source_trade_id
        ).fetch_one(pool).await?;
        Ok(lot)
    }

    pub async fn get(pool: &SqlitePool, id: i64) -> Result<ShareLot, AppError> {
        sqlx::query_as!(ShareLot,
            "SELECT id, account_id, ticker, quantity, original_cost_basis, adjusted_cost_basis, acquisition_date, acquisition_type, source_trade_id, status, created_at FROM share_lots WHERE id = ?",
            id
        ).fetch_optional(pool).await?.ok_or(AppError::NotFound)
    }

    pub async fn list_active(pool: &SqlitePool, account_id: i64) -> Result<Vec<ShareLot>, AppError> {
        sqlx::query_as!(ShareLot,
            "SELECT id, account_id, ticker, quantity, original_cost_basis, adjusted_cost_basis, acquisition_date, acquisition_type, source_trade_id, status, created_at FROM share_lots WHERE account_id = ? AND status = 'ACTIVE' ORDER BY acquisition_date DESC",
            account_id
        ).fetch_all(pool).await.map_err(Into::into)
    }

    pub async fn reduce_cost_basis(pool: &SqlitePool, id: i64, premium_total: f64) -> Result<(), AppError> {
        // premium_total is total $ received; divide by 100 shares for per-share reduction
        sqlx::query!(
            "UPDATE share_lots SET adjusted_cost_basis = adjusted_cost_basis - ? WHERE id = ?",
            premium_total / 100.0, id
        ).execute(pool).await?;
        Ok(())
    }

    pub async fn mark_called_away(pool: &SqlitePool, id: i64) -> Result<(), AppError> {
        sqlx::query!("UPDATE share_lots SET status = 'CALLED_AWAY' WHERE id = ?", id)
            .execute(pool).await?;
        Ok(())
    }
}
```

Update `backend/src/models/mod.rs`:
```rust
pub mod account;
pub mod share_lot;
```

- [ ] **Step 4: Run tests — verify pass**
```bash
cd backend && cargo test share_lot 2>&1
```

- [ ] **Step 5: Commit**
```bash
git add backend/src/ && git commit -m "feat: share lot model with cost basis tracking"
```

---

## Task 5: Trade Model (PUT + CALL)

**Files:**
- Create: `backend/src/models/trade.rs`
- Modify: `backend/src/models/mod.rs`

- [ ] **Step 1: Write failing tests**
```rust
// In backend/src/models/trade.rs
#[cfg(test)]
mod tests {
    use crate::db;
    use crate::models::{account::Account, trade::{CreateTrade, Trade}};

    async fn setup() -> (crate::db::Pool, i64) {
        let pool = db::init_pool("sqlite::memory:").await;
        db::run_migrations(&pool).await;
        let acct = Account::create(&pool, "Test").await.unwrap();
        (pool, acct.id)
    }

    #[tokio::test]
    async fn test_create_put_trade() {
        let (pool, acct_id) = setup().await;
        let trade = Trade::create(&pool, &CreateTrade {
            account_id: acct_id,
            trade_type: "PUT".to_string(),
            ticker: "AAPL".to_string(),
            strike_price: 150.0,
            expiry_date: "2025-02-21".to_string(),
            open_date: "2025-01-10".to_string(),
            premium_received: 200.0,
            fees_open: 1.30,
            share_lot_id: None,
        }).await.unwrap();
        assert_eq!(trade.status, "OPEN");
        assert_eq!(trade.trade_type, "PUT");
    }

    #[tokio::test]
    async fn test_close_trade_expired() {
        let (pool, acct_id) = setup().await;
        let trade = Trade::create(&pool, &CreateTrade {
            account_id: acct_id,
            trade_type: "PUT".to_string(),
            ticker: "AAPL".to_string(),
            strike_price: 150.0,
            expiry_date: "2025-02-21".to_string(),
            open_date: "2025-01-10".to_string(),
            premium_received: 200.0,
            fees_open: 1.30,
            share_lot_id: None,
        }).await.unwrap();
        Trade::close(&pool, trade.id, "EXPIRED", None, None, None).await.unwrap();
        let updated = Trade::get(&pool, trade.id).await.unwrap();
        assert_eq!(updated.status, "EXPIRED");
    }

    #[tokio::test]
    async fn test_net_premium_expired() {
        let (pool, acct_id) = setup().await;
        let trade = Trade::create(&pool, &CreateTrade {
            account_id: acct_id, trade_type: "PUT".to_string(), ticker: "AAPL".to_string(),
            strike_price: 150.0, expiry_date: "2025-02-21".to_string(),
            open_date: "2025-01-10".to_string(), premium_received: 200.0,
            fees_open: 1.30, share_lot_id: None,
        }).await.unwrap();
        Trade::close(&pool, trade.id, "EXPIRED", None, None, Some("2025-02-21".to_string())).await.unwrap();
        let t = Trade::get(&pool, trade.id).await.unwrap();
        // net = premium - fees_open - fees_close (0 for expiry)
        assert!((t.net_premium().unwrap() - 198.70).abs() < 0.001);
    }
}
```

- [ ] **Step 2: Run — verify fails**
```bash
cd backend && cargo test trade 2>&1 | head -10
```

- [ ] **Step 3: Implement Trade model**
```rust
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use crate::errors::AppError;

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Trade {
    pub id: i64,
    pub account_id: i64,
    pub trade_type: String,
    pub ticker: String,
    pub strike_price: f64,
    pub expiry_date: String,
    pub open_date: String,
    pub premium_received: f64,
    pub fees_open: f64,
    pub status: String,
    pub close_date: Option<String>,
    pub close_premium: Option<f64>,
    pub fees_close: Option<f64>,
    pub share_lot_id: Option<i64>,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
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
}

impl Trade {
    /// Net premium: received - fees_open - close_premium (buyback cost) - fees_close
    pub fn net_premium(&self) -> Option<f64> {
        let close_cost = self.close_premium.unwrap_or(0.0);
        let fees_close = self.fees_close.unwrap_or(0.0);
        Some(self.premium_received - self.fees_open - close_cost - fees_close)
    }

    pub async fn create(pool: &SqlitePool, input: &CreateTrade) -> Result<Trade, AppError> {
        sqlx::query_as!(Trade,
            r#"INSERT INTO trades (account_id, trade_type, ticker, strike_price, expiry_date, open_date, premium_received, fees_open, share_lot_id)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
               RETURNING id, account_id, trade_type, ticker, strike_price, expiry_date, open_date, premium_received, fees_open, status, close_date, close_premium, fees_close, share_lot_id, created_at"#,
            input.account_id, input.trade_type, input.ticker, input.strike_price,
            input.expiry_date, input.open_date, input.premium_received, input.fees_open, input.share_lot_id
        ).fetch_one(pool).await.map_err(Into::into)
    }

    pub async fn get(pool: &SqlitePool, id: i64) -> Result<Trade, AppError> {
        sqlx::query_as!(Trade,
            "SELECT id, account_id, trade_type, ticker, strike_price, expiry_date, open_date, premium_received, fees_open, status, close_date, close_premium, fees_close, share_lot_id, created_at FROM trades WHERE id = ?",
            id
        ).fetch_optional(pool).await?.ok_or(AppError::NotFound)
    }

    pub async fn close(
        pool: &SqlitePool,
        id: i64,
        status: &str,
        close_premium: Option<f64>,
        fees_close: Option<f64>,
        close_date: Option<String>,
    ) -> Result<(), AppError> {
        let date = close_date.unwrap_or_else(|| chrono::Local::now().format("%Y-%m-%d").to_string());
        sqlx::query!(
            "UPDATE trades SET status = ?, close_date = ?, close_premium = ?, fees_close = ? WHERE id = ?",
            status, date, close_premium, fees_close, id
        ).execute(pool).await?;
        Ok(())
    }

    pub async fn list_open(pool: &SqlitePool, account_id: i64) -> Result<Vec<Trade>, AppError> {
        sqlx::query_as!(Trade,
            "SELECT id, account_id, trade_type, ticker, strike_price, expiry_date, open_date, premium_received, fees_open, status, close_date, close_premium, fees_close, share_lot_id, created_at FROM trades WHERE account_id = ? AND status = 'OPEN' ORDER BY open_date DESC",
            account_id
        ).fetch_all(pool).await.map_err(Into::into)
    }

    pub async fn list_with_filters(
        pool: &SqlitePool,
        account_id: Option<i64>,
        ticker: Option<&str>,
        date_from: Option<&str>,
        date_to: Option<&str>,
    ) -> Result<Vec<Trade>, AppError> {
        // Build dynamic query — SQLx doesn't support fully dynamic queries,
        // so we use a base query and apply post-fetch filtering for simplicity at MVP scale
        let mut trades = sqlx::query_as!(Trade,
            "SELECT id, account_id, trade_type, ticker, strike_price, expiry_date, open_date, premium_received, fees_open, status, close_date, close_premium, fees_close, share_lot_id, created_at FROM trades ORDER BY open_date DESC"
        ).fetch_all(pool).await?;

        if let Some(aid) = account_id {
            trades.retain(|t| t.account_id == aid);
        }
        if let Some(tk) = ticker {
            let tk_upper = tk.to_uppercase();
            trades.retain(|t| t.ticker.to_uppercase().contains(&tk_upper));
        }
        if let Some(from) = date_from {
            trades.retain(|t| t.open_date.as_str() >= from);
        }
        if let Some(to) = date_to {
            trades.retain(|t| t.open_date.as_str() <= to);
        }
        Ok(trades)
    }
}
```

Update `backend/src/models/mod.rs`:
```rust
pub mod account;
pub mod share_lot;
pub mod trade;
```

- [ ] **Step 4: Run tests — verify pass**
```bash
cd backend && cargo test trade 2>&1
```

- [ ] **Step 5: Commit**
```bash
git add backend/src/ && git commit -m "feat: trade model (PUT/CALL) with close lifecycle"
```

---

## Task 6: PUT Trade Handlers + Routes

**Files:**
- Create: `backend/src/handlers/puts.rs`
- Modify: `backend/src/handlers/mod.rs`
- Modify: `backend/src/routes.rs`

Endpoints:
- `POST /api/accounts/:id/puts` — open a PUT trade
- `POST /api/trades/puts/:id/close` — close: expire, buy back, or assign

- [ ] **Step 1: Write failing tests**
```rust
// In backend/src/handlers/puts.rs
#[cfg(test)]
mod tests {
    use axum_test::TestServer;
    use serde_json::json;
    use crate::{db, routes::create_router};

    async fn server() -> (TestServer, i64) {
        let pool = db::init_pool("sqlite::memory:").await;
        db::run_migrations(&pool).await;
        let res = axum_test::TestServer::new(create_router(pool.clone())).unwrap()
            .post("/api/accounts").json(&json!({"name":"Test"})).await;
        let id = res.json::<serde_json::Value>()["id"].as_i64().unwrap();
        (axum_test::TestServer::new(create_router(pool)).unwrap(), id)
    }

    #[tokio::test]
    async fn test_open_put() {
        let (server, acct_id) = server().await;
        let res = server.post(&format!("/api/accounts/{}/puts", acct_id))
            .json(&json!({
                "ticker": "AAPL", "strike_price": 150.0,
                "expiry_date": "2025-02-21", "open_date": "2025-01-10",
                "premium_received": 200.0, "fees_open": 1.30
            })).await;
        res.assert_status_created();
        assert_eq!(res.json::<serde_json::Value>()["status"], "OPEN");
    }

    #[tokio::test]
    async fn test_close_put_expired() {
        let (server, acct_id) = server().await;
        let put = server.post(&format!("/api/accounts/{}/puts", acct_id))
            .json(&json!({"ticker":"AAPL","strike_price":150.0,"expiry_date":"2025-02-21","open_date":"2025-01-10","premium_received":200.0,"fees_open":1.3})).await;
        let id = put.json::<serde_json::Value>()["id"].as_i64().unwrap();
        let res = server.post(&format!("/api/trades/puts/{}/close", id))
            .json(&json!({"action": "EXPIRED", "close_date": "2025-02-21"})).await;
        res.assert_status_ok();
        assert_eq!(res.json::<serde_json::Value>()["status"], "EXPIRED");
    }

    #[tokio::test]
    async fn test_close_put_assigned_creates_lot() {
        let (server, acct_id) = server().await;
        let put = server.post(&format!("/api/accounts/{}/puts", acct_id))
            .json(&json!({"ticker":"AAPL","strike_price":150.0,"expiry_date":"2025-02-21","open_date":"2025-01-10","premium_received":200.0,"fees_open":1.3})).await;
        let id = put.json::<serde_json::Value>()["id"].as_i64().unwrap();
        let res = server.post(&format!("/api/trades/puts/{}/close", id))
            .json(&json!({"action": "ASSIGNED", "close_date": "2025-02-21"})).await;
        res.assert_status_ok();
        // adjusted cost basis = strike - premium_net/100
        let body = res.json::<serde_json::Value>();
        assert!(body["share_lot"].is_object());
        let adj = body["share_lot"]["adjusted_cost_basis"].as_f64().unwrap();
        // 150 - (200 - 1.3) / 100 = 150 - 1.987 = 148.013
        assert!((adj - 148.013).abs() < 0.01);
    }
}
```

- [ ] **Step 2: Run — verify fails**
```bash
cd backend && cargo test puts 2>&1 | head -10
```

- [ ] **Step 3: Implement PUT handlers**
```rust
use axum::{extract::{Path, State}, http::StatusCode, Json};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::SqlitePool;
use crate::{errors::AppError, models::{share_lot::{CreateShareLot, ShareLot}, trade::{CreateTrade, Trade}}};

#[derive(Deserialize)]
pub struct OpenPut {
    pub ticker: String,
    pub strike_price: f64,
    pub expiry_date: String,
    pub open_date: String,
    pub premium_received: f64,
    pub fees_open: f64,
}

#[derive(Deserialize)]
pub struct ClosePut {
    pub action: String, // EXPIRED | BOUGHT_BACK | ASSIGNED
    pub close_date: Option<String>,
    pub close_premium: Option<f64>, // for BOUGHT_BACK
    pub fees_close: Option<f64>,
}

pub async fn open_put(
    State(pool): State<SqlitePool>,
    Path(account_id): Path<i64>,
    Json(payload): Json<OpenPut>,
) -> Result<(StatusCode, Json<Trade>), AppError> {
    let trade = Trade::create(&pool, &CreateTrade {
        account_id,
        trade_type: "PUT".to_string(),
        ticker: payload.ticker.to_uppercase(),
        strike_price: payload.strike_price,
        expiry_date: payload.expiry_date,
        open_date: payload.open_date,
        premium_received: payload.premium_received,
        fees_open: payload.fees_open,
        share_lot_id: None,
    }).await?;
    Ok((StatusCode::CREATED, Json(trade)))
}

pub async fn close_put(
    State(pool): State<SqlitePool>,
    Path(trade_id): Path<i64>,
    Json(payload): Json<ClosePut>,
) -> Result<Json<Value>, AppError> {
    let trade = Trade::get(&pool, trade_id).await?;
    if trade.trade_type != "PUT" {
        return Err(AppError::BadRequest("trade is not a PUT".to_string()));
    }
    if trade.status != "OPEN" {
        return Err(AppError::BadRequest("trade is already closed".to_string()));
    }

    match payload.action.as_str() {
        "EXPIRED" => {
            Trade::close(&pool, trade_id, "EXPIRED", None, None, payload.close_date).await?;
            let updated = Trade::get(&pool, trade_id).await?;
            Ok(Json(json!(updated)))
        }
        "BOUGHT_BACK" => {
            let close_premium = payload.close_premium
                .ok_or_else(|| AppError::BadRequest("close_premium required for BOUGHT_BACK".to_string()))?;
            Trade::close(&pool, trade_id, "BOUGHT_BACK", Some(close_premium), payload.fees_close, payload.close_date).await?;
            let updated = Trade::get(&pool, trade_id).await?;
            Ok(Json(json!(updated)))
        }
        "ASSIGNED" => {
            // Use a transaction so trade close + lot creation are atomic
            let mut tx = pool.begin().await?;
            let close_date = payload.close_date.unwrap_or_else(|| chrono::Local::now().format("%Y-%m-%d").to_string());
            sqlx::query!(
                "UPDATE trades SET status = 'ASSIGNED', close_date = ? WHERE id = ?",
                close_date, trade_id
            ).execute(&mut *tx).await?;
            // net premium = received - fees_open (no close_premium for assignment)
            let net = trade.premium_received - trade.fees_open;
            // adjusted cost basis = strike - net_premium_per_share
            let adjusted_cb = trade.strike_price - (net / 100.0);
            let lot = sqlx::query_as!(ShareLot,
                r#"INSERT INTO share_lots
                   (account_id, ticker, original_cost_basis, adjusted_cost_basis, acquisition_date, acquisition_type, source_trade_id)
                   VALUES (?, ?, ?, ?, ?, 'ASSIGNED', ?)
                   RETURNING id, account_id, ticker, quantity, original_cost_basis, adjusted_cost_basis,
                             acquisition_date, acquisition_type, source_trade_id, status, created_at"#,
                trade.account_id, trade.ticker, trade.strike_price, adjusted_cb, close_date, trade_id
            ).fetch_one(&mut *tx).await?;
            tx.commit().await?;
            let updated = Trade::get(&pool, trade_id).await?;
            Ok(Json(json!({ "trade": updated, "share_lot": lot })))
        }
        _ => Err(AppError::BadRequest(format!("unknown action: {}", payload.action))),
    }
}
```

- [ ] **Step 4: Wire routes + update mods**

Update `backend/src/handlers/mod.rs`:
```rust
pub mod accounts;
pub mod puts;
```

Update routes in `backend/src/routes.rs` — add:
```rust
use crate::handlers::puts;
// In router:
.route("/api/accounts/:id/puts", post(puts::open_put))
.route("/api/trades/puts/:id/close", post(puts::close_put))
```

- [ ] **Step 5: Run tests — verify pass**
```bash
cd backend && cargo test puts 2>&1
```

- [ ] **Step 6: Commit**
```bash
git add backend/src/ && git commit -m "feat: PUT trade open/close handlers with assignment → share lot creation"
```

---

## Task 7: CALL Trade Handlers + Routes

**Files:**
- Create: `backend/src/handlers/calls.rs`
- Modify: `backend/src/handlers/mod.rs`, `routes.rs`

Endpoints:
- `POST /api/accounts/:id/calls` — open a CALL (requires share_lot_id)
- `POST /api/trades/calls/:id/close` — expire / buy back / called away
- `GET /api/accounts/:id/share-lots` — list active lots for account

- [ ] **Step 1: Write failing tests**
```rust
#[cfg(test)]
mod tests {
    use axum_test::TestServer;
    use serde_json::json;
    use crate::{db, routes::create_router};

    async fn server_with_lot() -> (TestServer, i64, i64) {
        let pool = db::init_pool("sqlite::memory:").await;
        db::run_migrations(&pool).await;
        let srv = TestServer::new(create_router(pool)).unwrap();
        let acct_id = srv.post("/api/accounts").json(&json!({"name":"Test"})).await
            .json::<serde_json::Value>()["id"].as_i64().unwrap();
        // Create a PUT and assign it to get a lot
        let put = srv.post(&format!("/api/accounts/{}/puts", acct_id))
            .json(&json!({"ticker":"AAPL","strike_price":150.0,"expiry_date":"2025-02-21","open_date":"2025-01-10","premium_received":200.0,"fees_open":1.3})).await;
        let put_id = put.json::<serde_json::Value>()["id"].as_i64().unwrap();
        let assign = srv.post(&format!("/api/trades/puts/{}/close", put_id))
            .json(&json!({"action":"ASSIGNED","close_date":"2025-02-21"})).await;
        let lot_id = assign.json::<serde_json::Value>()["share_lot"]["id"].as_i64().unwrap();
        (srv, acct_id, lot_id)
    }

    #[tokio::test]
    async fn test_open_call_on_lot() {
        let (server, acct_id, lot_id) = server_with_lot().await;
        let res = server.post(&format!("/api/accounts/{}/calls", acct_id))
            .json(&json!({
                "share_lot_id": lot_id,
                "ticker": "AAPL", "strike_price": 155.0,
                "expiry_date": "2025-03-21", "open_date": "2025-02-22",
                "premium_received": 150.0, "fees_open": 1.30
            })).await;
        res.assert_status_created();
        assert_eq!(res.json::<serde_json::Value>()["status"], "OPEN");
    }

    #[tokio::test]
    async fn test_close_call_expired_reduces_cost_basis() {
        let (server, acct_id, lot_id) = server_with_lot().await;
        let call = server.post(&format!("/api/accounts/{}/calls", acct_id))
            .json(&json!({"share_lot_id":lot_id,"ticker":"AAPL","strike_price":155.0,"expiry_date":"2025-03-21","open_date":"2025-02-22","premium_received":150.0,"fees_open":1.30})).await;
        let call_id = call.json::<serde_json::Value>()["id"].as_i64().unwrap();

        let lots_before = server.get(&format!("/api/accounts/{}/share-lots", acct_id)).await;
        let cb_before = lots_before.json::<serde_json::Value>()[0]["adjusted_cost_basis"].as_f64().unwrap();

        server.post(&format!("/api/trades/calls/{}/close", call_id))
            .json(&json!({"action":"EXPIRED","close_date":"2025-03-21"})).await
            .assert_status_ok();

        let lots_after = server.get(&format!("/api/accounts/{}/share-lots", acct_id)).await;
        let cb_after = lots_after.json::<serde_json::Value>()[0]["adjusted_cost_basis"].as_f64().unwrap();
        // cost basis should decrease by (150 - 1.30) / 100 = 1.487
        assert!(cb_before - cb_after > 1.4);
    }

    #[tokio::test]
    async fn test_close_call_called_away_marks_lot() {
        let (server, acct_id, lot_id) = server_with_lot().await;
        let call = server.post(&format!("/api/accounts/{}/calls", acct_id))
            .json(&json!({"share_lot_id":lot_id,"ticker":"AAPL","strike_price":155.0,"expiry_date":"2025-03-21","open_date":"2025-02-22","premium_received":150.0,"fees_open":1.30})).await;
        let call_id = call.json::<serde_json::Value>()["id"].as_i64().unwrap();
        server.post(&format!("/api/trades/calls/{}/close", call_id))
            .json(&json!({"action":"CALLED_AWAY","close_date":"2025-03-21"})).await
            .assert_status_ok();
        let lots = server.get(&format!("/api/accounts/{}/share-lots", acct_id)).await;
        // active lots should now be empty
        assert_eq!(lots.json::<serde_json::Value>().as_array().unwrap().len(), 0);
    }
}
```

- [ ] **Step 2: Run — verify fails**
```bash
cd backend && cargo test calls 2>&1 | head -10
```

- [ ] **Step 3: Implement CALL handlers**
```rust
use axum::{extract::{Path, State}, http::StatusCode, Json};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::SqlitePool;
use crate::{errors::AppError, models::{share_lot::ShareLot, trade::{CreateTrade, Trade}}};

#[derive(Deserialize)]
pub struct OpenCall {
    pub share_lot_id: i64,
    pub ticker: String,
    pub strike_price: f64,
    pub expiry_date: String,
    pub open_date: String,
    pub premium_received: f64,
    pub fees_open: f64,
}

#[derive(Deserialize)]
pub struct CloseCall {
    pub action: String, // EXPIRED | BOUGHT_BACK | CALLED_AWAY
    pub close_date: Option<String>,
    pub close_premium: Option<f64>,
    pub fees_close: Option<f64>,
}

pub async fn list_share_lots(
    State(pool): State<SqlitePool>,
    Path(account_id): Path<i64>,
) -> Result<Json<Vec<ShareLot>>, AppError> {
    ShareLot::list_active(&pool, account_id).await.map(Json)
}

pub async fn open_call(
    State(pool): State<SqlitePool>,
    Path(account_id): Path<i64>,
    Json(payload): Json<OpenCall>,
) -> Result<(StatusCode, Json<Trade>), AppError> {
    // Verify lot belongs to this account and is active
    let lot = ShareLot::get(&pool, payload.share_lot_id).await?;
    if lot.account_id != account_id || lot.status != "ACTIVE" {
        return Err(AppError::BadRequest("invalid or inactive share lot".to_string()));
    }
    let trade = Trade::create(&pool, &CreateTrade {
        account_id,
        trade_type: "CALL".to_string(),
        ticker: payload.ticker.to_uppercase(),
        strike_price: payload.strike_price,
        expiry_date: payload.expiry_date,
        open_date: payload.open_date,
        premium_received: payload.premium_received,
        fees_open: payload.fees_open,
        share_lot_id: Some(payload.share_lot_id),
    }).await?;
    Ok((StatusCode::CREATED, Json(trade)))
}

pub async fn close_call(
    State(pool): State<SqlitePool>,
    Path(trade_id): Path<i64>,
    Json(payload): Json<CloseCall>,
) -> Result<Json<Value>, AppError> {
    let trade = Trade::get(&pool, trade_id).await?;
    if trade.trade_type != "CALL" {
        return Err(AppError::BadRequest("trade is not a CALL".to_string()));
    }
    if trade.status != "OPEN" {
        return Err(AppError::BadRequest("trade is already closed".to_string()));
    }
    let lot_id = trade.share_lot_id
        .ok_or_else(|| AppError::BadRequest("CALL has no associated share lot".to_string()))?;

    match payload.action.as_str() {
        "EXPIRED" | "BOUGHT_BACK" => {
            let close_premium = if payload.action == "BOUGHT_BACK" {
                Some(payload.close_premium.ok_or_else(|| AppError::BadRequest("close_premium required".to_string()))?)
            } else { None };
            Trade::close(&pool, trade_id, &payload.action, close_premium, payload.fees_close, payload.close_date).await?;
            let updated = Trade::get(&pool, trade_id).await?;
            // Reduce cost basis by net premium
            let net = updated.net_premium().unwrap_or(0.0);
            ShareLot::reduce_cost_basis(&pool, lot_id, net).await?;
            let lot = ShareLot::get(&pool, lot_id).await?;
            Ok(Json(json!({ "trade": updated, "share_lot": lot })))
        }
        "CALLED_AWAY" => {
            Trade::close(&pool, trade_id, "CALLED_AWAY", None, payload.fees_close, payload.close_date).await?;
            let updated = Trade::get(&pool, trade_id).await?;
            let net = updated.net_premium().unwrap_or(0.0);
            ShareLot::reduce_cost_basis(&pool, lot_id, net).await?;
            ShareLot::mark_called_away(&pool, lot_id).await?;
            Ok(Json(json!(updated)))
        }
        _ => Err(AppError::BadRequest(format!("unknown action: {}", payload.action))),
    }
}
```

- [ ] **Step 4: Update mods + routes**

`handlers/mod.rs`: add `pub mod calls;`

`routes.rs` — add:
```rust
use crate::handlers::calls;
// in router:
.route("/api/accounts/:id/calls", post(calls::open_call))
.route("/api/accounts/:id/share-lots", get(calls::list_share_lots))
.route("/api/trades/calls/:id/close", post(calls::close_call))
```

- [ ] **Step 5: Run tests — verify pass**
```bash
cd backend && cargo test calls 2>&1
```

- [ ] **Step 6: Commit**
```bash
git add backend/src/ && git commit -m "feat: CALL trade open/close with cost basis reduction and called-away flow"
```

---

## Task 8: Manual Share Lot Entry API + Frontend Form

The spec explicitly covers shares already held before using this app: "The shares are already in the account and the user decides to sell calls on it to generate additional income. In this case, ask the user to enter the cost basis of those 100 shares."

**Files:**
- Create: `backend/src/handlers/share_lots.rs`
- Modify: `backend/src/handlers/mod.rs`, `routes.rs`
- Create: `frontend/src/components/trades/ManualLotForm.tsx`
- Create: `frontend/src/app/trades/new-lot/page.tsx`
- Modify: `frontend/src/components/layout/Sidebar.tsx`

Endpoint: `POST /api/accounts/:id/share-lots`

- [ ] **Step 1: Write failing test**
```rust
// backend/src/handlers/share_lots.rs
#[cfg(test)]
mod tests {
    use axum_test::TestServer;
    use serde_json::json;
    use crate::{db, routes::create_router};

    #[tokio::test]
    async fn test_create_manual_lot() {
        let pool = db::init_pool("sqlite::memory:").await;
        db::run_migrations(&pool).await;
        let srv = TestServer::new(create_router(pool)).unwrap();
        let acct_id = srv.post("/api/accounts").json(&json!({"name":"T"})).await
            .json::<serde_json::Value>()["id"].as_i64().unwrap();
        let res = srv.post(&format!("/api/accounts/{}/share-lots", acct_id))
            .json(&json!({
                "ticker": "MSFT",
                "cost_basis": 300.00,
                "acquisition_date": "2024-06-01"
            })).await;
        res.assert_status_created();
        let body = res.json::<serde_json::Value>();
        assert_eq!(body["ticker"], "MSFT");
        assert_eq!(body["acquisition_type"], "MANUAL");
        assert_eq!(body["adjusted_cost_basis"].as_f64().unwrap(), 300.00);
    }
}
```

- [ ] **Step 2: Run — verify fails**
```bash
cd backend && cargo test manual_lot 2>&1 | head -10
```

- [ ] **Step 3: Implement handler**
```rust
use axum::{extract::{Path, State}, http::StatusCode, Json};
use serde::Deserialize;
use sqlx::SqlitePool;
use crate::{errors::AppError, models::share_lot::{CreateShareLot, ShareLot}};

#[derive(Deserialize)]
pub struct CreateManualLot {
    pub ticker: String,
    pub cost_basis: f64,
    pub acquisition_date: String,
}

pub async fn create_manual_lot(
    State(pool): State<SqlitePool>,
    Path(account_id): Path<i64>,
    Json(payload): Json<CreateManualLot>,
) -> Result<(StatusCode, Json<ShareLot>), AppError> {
    if payload.cost_basis <= 0.0 {
        return Err(AppError::BadRequest("cost_basis must be positive".to_string()));
    }
    let lot = ShareLot::create(&pool, &CreateShareLot {
        account_id,
        ticker: payload.ticker.to_uppercase(),
        original_cost_basis: payload.cost_basis,
        adjusted_cost_basis: None, // starts equal to original
        acquisition_date: payload.acquisition_date,
        acquisition_type: "MANUAL".to_string(),
        source_trade_id: None,
    }).await?;
    Ok((StatusCode::CREATED, Json(lot)))
}
```

- [ ] **Step 4: Update mods + routes**

`handlers/mod.rs`: add `pub mod share_lots;`

`routes.rs` — add:
```rust
use crate::handlers::share_lots;
// in router (this route is additive alongside the existing GET):
.route("/api/accounts/:id/share-lots",
    get(calls::list_share_lots).post(share_lots::create_manual_lot))
```

- [ ] **Step 5: Run tests — verify pass**
```bash
cd backend && cargo test manual_lot 2>&1
```

- [ ] **Step 6: Create ManualLotForm frontend component**

Create `frontend/src/components/trades/ManualLotForm.tsx`:
```tsx
'use client';
import { useState } from 'react';
import { useRouter } from 'next/navigation';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { api } from '@/lib/api';
import { useAccounts } from '@/contexts/AccountContext';

export function ManualLotForm() {
  const router = useRouter();
  const { selectedAccountId } = useAccounts();
  const [form, setForm] = useState({
    ticker: '',
    cost_basis: '',
    acquisition_date: new Date().toISOString().split('T')[0],
  });
  const [error, setError] = useState('');

  const set = (k: string) => (e: React.ChangeEvent<HTMLInputElement>) =>
    setForm((f) => ({ ...f, [k]: e.target.value }));

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!selectedAccountId) { setError('Select an account first'); return; }
    try {
      await fetch(`${process.env.NEXT_PUBLIC_API_URL ?? 'http://localhost:3001'}/api/accounts/${selectedAccountId}/share-lots`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          ticker: form.ticker.toUpperCase(),
          cost_basis: parseFloat(form.cost_basis),
          acquisition_date: form.acquisition_date,
        }),
      }).then((r) => { if (!r.ok) throw new Error('Failed'); return r.json(); });
      router.push('/trades/new-call');
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : 'Failed to add lot');
    }
  };

  return (
    <Card className="max-w-md">
      <CardHeader><CardTitle>Add Existing Share Lot</CardTitle></CardHeader>
      <CardContent>
        <p className="text-sm text-muted-foreground mb-4">
          Add shares you already own (purchased before using this app) so you can sell covered calls on them.
        </p>
        <form onSubmit={handleSubmit} className="space-y-4">
          {[
            { label: 'Ticker', key: 'ticker', placeholder: 'AAPL', type: 'text' },
            { label: 'Cost Basis (per share)', key: 'cost_basis', placeholder: '150.00', type: 'number' },
            { label: 'Purchase Date', key: 'acquisition_date', placeholder: '', type: 'date' },
          ].map(({ label, key, placeholder, type }) => (
            <div key={key} className="space-y-1">
              <Label>{label}</Label>
              <Input type={type} placeholder={placeholder}
                value={form[key as keyof typeof form]} onChange={set(key)} required />
            </div>
          ))}
          {error && <p className="text-sm text-destructive">{error}</p>}
          <Button type="submit" className="w-full">Add Share Lot</Button>
        </form>
      </CardContent>
    </Card>
  );
}
```

- [ ] **Step 7: Create page**

Create `frontend/src/app/trades/new-lot/page.tsx`:
```tsx
import { ManualLotForm } from '@/components/trades/ManualLotForm';
export default function NewLotPage() {
  return (
    <div className="space-y-4">
      <h1 className="text-2xl font-bold">Add Existing Share Lot</h1>
      <ManualLotForm />
    </div>
  );
}
```

- [ ] **Step 8: Add nav link to Sidebar**

In `frontend/src/components/layout/Sidebar.tsx`, add to `navLinks`:
```tsx
{ href: '/trades/new-lot', label: 'Add Share Lot' },
```

- [ ] **Step 9: Verify build**
```bash
cd frontend && npm run build 2>&1 | tail -10
```

- [ ] **Step 10: Commit**
```bash
git add backend/src/ frontend/src/ && git commit -m "feat: manual share lot entry — API + frontend form"
```

---

## Task 9: Dashboard + History API  <!-- was Task 8 -->

**Files:**
- Create: `backend/src/handlers/dashboard.rs`
- Create: `backend/src/handlers/history.rs`
- Modify: `backend/src/handlers/mod.rs`, `routes.rs`

Endpoints:
- `GET /api/dashboard?account_id=` — aggregate metrics
- `GET /api/history?account_id=&ticker=&date_from=&date_to=` — filtered history

### Yield formula
```
annualized_yield = (net_premium / capital_deployed) * (365 / days_held)
```
- **Realized trades**: `days_held = close_date - open_date`
- **Open trades**: `days_held = today - open_date`
- **Capital deployed (PUT)**: `strike_price * 100`
- **Capital deployed (CALL)**: `adjusted_cost_basis * 100` (of the linked lot)

- [ ] **Step 1: Write failing tests**
```rust
// backend/src/handlers/dashboard.rs
#[cfg(test)]
mod tests {
    use axum_test::TestServer;
    use serde_json::json;
    use crate::{db, routes::create_router};

    #[tokio::test]
    async fn test_dashboard_totals() {
        let pool = db::init_pool("sqlite::memory:").await;
        db::run_migrations(&pool).await;
        let srv = TestServer::new(create_router(pool)).unwrap();
        let acct_id = srv.post("/api/accounts").json(&json!({"name":"T"})).await
            .json::<serde_json::Value>()["id"].as_i64().unwrap();
        // Open a PUT
        srv.post(&format!("/api/accounts/{}/puts", acct_id))
            .json(&json!({"ticker":"AAPL","strike_price":150.0,"expiry_date":"2025-12-19","open_date":"2025-01-10","premium_received":200.0,"fees_open":1.3})).await;
        let res = srv.get("/api/dashboard").await;
        res.assert_status_ok();
        let body = res.json::<serde_json::Value>();
        assert!(body["total_premium_collected"].as_f64().unwrap() >= 0.0);
        assert!(body["open_trades"].as_array().is_some());
    }
}
```

- [ ] **Step 2: Implement dashboard handler**
```rust
use axum::{extract::{Query, State}, Json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::SqlitePool;
use chrono::NaiveDate;
use crate::{errors::AppError, models::{share_lot::ShareLot, trade::Trade}};

#[derive(Deserialize)]
pub struct DashboardQuery {
    pub account_id: Option<i64>,
}

fn days_between(from: &str, to: &str) -> f64 {
    let parse = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok();
    match (parse(from), parse(to)) {
        (Some(f), Some(t)) => (t - f).num_days().max(1) as f64,
        _ => 1.0,
    }
}

fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

pub async fn get_dashboard(
    State(pool): State<SqlitePool>,
    Query(params): Query<DashboardQuery>,
) -> Result<Json<Value>, AppError> {
    let all_trades = Trade::list_with_filters(&pool, params.account_id, None, None, None).await?;
    let today = today();

    let mut total_premium = 0.0f64;
    let mut realized_yield_sum = 0.0f64;
    let mut realized_capital_sum = 0.0f64;
    let mut open_yield_sum = 0.0f64;
    let mut open_capital_sum = 0.0f64;
    let mut total_capital_deployed = 0.0f64;

    for trade in &all_trades {
        let net = trade.net_premium().unwrap_or(0.0);
        // For CALL trades, capital is the adjusted cost basis of the linked lot (not the call strike).
        // For PUT trades, capital is the cash collateral = strike * 100.
        let capital = if trade.trade_type == "CALL" {
            if let Some(lot_id) = trade.share_lot_id {
                match ShareLot::get(&pool, lot_id).await {
                    Ok(lot) => lot.adjusted_cost_basis * 100.0,
                    Err(_) => trade.strike_price * 100.0, // fallback
                }
            } else {
                trade.strike_price * 100.0
            }
        } else {
            trade.strike_price * 100.0
        };

        if trade.status == "OPEN" {
            let days = days_between(&trade.open_date, &today);
            let annualized = (net / capital) * (365.0 / days);
            open_yield_sum += annualized * capital;
            open_capital_sum += capital;
            total_capital_deployed += capital;
        } else {
            total_premium += net;
            let close = trade.close_date.as_deref().unwrap_or(&today);
            let days = days_between(&trade.open_date, close);
            let annualized = (net / capital) * (365.0 / days);
            realized_yield_sum += annualized * capital;
            realized_capital_sum += capital;
        }
    }

    let realized_yield = if realized_capital_sum > 0.0 { realized_yield_sum / realized_capital_sum } else { 0.0 };
    let open_yield = if open_capital_sum > 0.0 { open_yield_sum / open_capital_sum } else { 0.0 };

    let open_trades: Vec<&Trade> = all_trades.iter().filter(|t| t.status == "OPEN").collect();

    let active_lots = if let Some(aid) = params.account_id {
        ShareLot::list_active(&pool, aid).await?
    } else {
        // Fetch lots across all accounts if no account filter
        sqlx::query_as!(ShareLot,
            "SELECT id, account_id, ticker, quantity, original_cost_basis, adjusted_cost_basis, acquisition_date, acquisition_type, source_trade_id, status, created_at FROM share_lots WHERE status = 'ACTIVE' ORDER BY acquisition_date DESC"
        ).fetch_all(&pool).await.unwrap_or_default()
    };

    Ok(Json(json!({
        "total_premium_collected": (total_premium * 100.0).round() / 100.0,
        "total_capital_deployed": (total_capital_deployed * 100.0).round() / 100.0,
        "realized_annualized_yield": (realized_yield * 10000.0).round() / 100.0, // as %
        "open_annualized_yield": (open_yield * 10000.0).round() / 100.0,
        "open_trades": open_trades,
        "active_share_lots": active_lots,
    })))
}
```

- [ ] **Step 3: Implement history handler**
```rust
// backend/src/handlers/history.rs
use axum::{extract::{Query, State}, Json};
use serde::Deserialize;
use sqlx::SqlitePool;
use crate::{errors::AppError, models::trade::Trade};

#[derive(Deserialize)]
pub struct HistoryQuery {
    pub account_id: Option<i64>,
    pub ticker: Option<String>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
}

pub async fn get_history(
    State(pool): State<SqlitePool>,
    Query(params): Query<HistoryQuery>,
) -> Result<Json<Vec<Trade>>, AppError> {
    Trade::list_with_filters(
        &pool,
        params.account_id,
        params.ticker.as_deref(),
        params.date_from.as_deref(),
        params.date_to.as_deref(),
    ).await.map(Json)
}
```

- [ ] **Step 4: Wire up**

`handlers/mod.rs`: add `pub mod dashboard; pub mod history;`

`routes.rs` add:
```rust
use crate::handlers::{dashboard, history};
.route("/api/dashboard", get(dashboard::get_dashboard))
.route("/api/history", get(history::get_history))
```

- [ ] **Step 5: Run all backend tests**
```bash
cd backend && cargo test 2>&1
```
Expected: all pass.

- [ ] **Step 6: Commit**
```bash
git add backend/src/ && git commit -m "feat: dashboard metrics + history API with filters"
```

---

## Task 9: Frontend — Types, API Client, Utils

**Files:**
- Create: `frontend/src/lib/types.ts`
- Create: `frontend/src/lib/api.ts`
- Create: `frontend/src/lib/utils.ts` (extend existing)

- [ ] **Step 1: Define TypeScript types**

Create `frontend/src/lib/types.ts`:
```typescript
export interface Account {
  id: number;
  name: string;
  created_at: string;
}

export type TradeType = 'PUT' | 'CALL';
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
  created_at: string;
}

export type AcquisitionType = 'MANUAL' | 'ASSIGNED';
export type LotStatus = 'ACTIVE' | 'CALLED_AWAY';

export interface ShareLot {
  id: number;
  account_id: number;
  ticker: string;
  quantity: number;
  original_cost_basis: number;
  adjusted_cost_basis: number;
  acquisition_date: string;
  acquisition_type: AcquisitionType;
  source_trade_id: number | null;
  status: LotStatus;
  created_at: string;
}

export interface DashboardData {
  total_premium_collected: number;
  total_capital_deployed: number;
  realized_annualized_yield: number;
  open_annualized_yield: number;
  open_trades: Trade[];
  active_share_lots: ShareLot[];
}

export interface HistoryFilters {
  account_id?: number;
  ticker?: string;
  date_from?: string;
  date_to?: string;
}
```

- [ ] **Step 2: Create API client**

Create `frontend/src/lib/api.ts`:
```typescript
const BASE = process.env.NEXT_PUBLIC_API_URL ?? 'http://localhost:3001';

async function request<T>(path: string, options?: RequestInit): Promise<T> {
  const res = await fetch(`${BASE}${path}`, {
    headers: { 'Content-Type': 'application/json' },
    ...options,
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: res.statusText }));
    throw new Error(err.error ?? 'Request failed');
  }
  return res.json();
}

import type { Account, DashboardData, HistoryFilters, ShareLot, Trade } from './types';

export const api = {
  accounts: {
    list: () => request<Account[]>('/api/accounts'),
    create: (name: string) => request<Account>('/api/accounts', { method: 'POST', body: JSON.stringify({ name }) }),
    delete: (id: number) => request<void>(`/api/accounts/${id}`, { method: 'DELETE' }),
  },
  puts: {
    open: (accountId: number, data: object) =>
      request<Trade>(`/api/accounts/${accountId}/puts`, { method: 'POST', body: JSON.stringify(data) }),
    close: (tradeId: number, data: object) =>
      request<unknown>(`/api/trades/puts/${tradeId}/close`, { method: 'POST', body: JSON.stringify(data) }),
  },
  calls: {
    open: (accountId: number, data: object) =>
      request<Trade>(`/api/accounts/${accountId}/calls`, { method: 'POST', body: JSON.stringify(data) }),
    close: (tradeId: number, data: object) =>
      request<unknown>(`/api/trades/calls/${tradeId}/close`, { method: 'POST', body: JSON.stringify(data) }),
  },
  shareLots: {
    list: (accountId: number) => request<ShareLot[]>(`/api/accounts/${accountId}/share-lots`),
  },
  dashboard: (accountId?: number) => {
    const qs = accountId ? `?account_id=${accountId}` : '';
    return request<DashboardData>(`/api/dashboard${qs}`);
  },
  history: (filters: HistoryFilters) => {
    const params = new URLSearchParams();
    if (filters.account_id) params.set('account_id', String(filters.account_id));
    if (filters.ticker) params.set('ticker', filters.ticker);
    if (filters.date_from) params.set('date_from', filters.date_from);
    if (filters.date_to) params.set('date_to', filters.date_to);
    return request<Trade[]>(`/api/history?${params}`);
  },
};
```

- [ ] **Step 3: Add utility functions**

Add to `frontend/src/lib/utils.ts` (keep existing cn export):
```typescript
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
      // year like "2025"
      if (/^\d{4}$/.test(preset)) return { date_from: `${preset}-01-01`, date_to: `${preset}-12-31` };
      return { date_from: '', date_to: '' };
  }
}
```

- [ ] **Step 4: Commit**
```bash
cd frontend && git add src/lib/ && git commit -m "feat: frontend types, API client, and utility functions"
```

---

## Task 10: Frontend Layout + Account Selector

**Files:**
- Create: `frontend/src/components/layout/Sidebar.tsx`
- Create: `frontend/src/components/layout/AccountSelector.tsx`
- Create: `frontend/src/contexts/AccountContext.tsx`
- Modify: `frontend/src/app/layout.tsx`

- [ ] **Step 1: Create account context**

Create `frontend/src/contexts/AccountContext.tsx`:
```tsx
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

  useEffect(() => { refresh(); }, []);

  return (
    <AccountContext.Provider value={{ accounts, selectedAccountId, setSelectedAccountId, refresh }}>
      {children}
    </AccountContext.Provider>
  );
}

export const useAccounts = () => useContext(AccountContext);
```

- [ ] **Step 2: Create AccountSelector component**

Create `frontend/src/components/layout/AccountSelector.tsx`:
```tsx
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
```

- [ ] **Step 3: Create Sidebar**

Create `frontend/src/components/layout/Sidebar.tsx`:
```tsx
import Link from 'next/link';
import { AccountSelector } from './AccountSelector';

const navLinks = [
  { href: '/', label: 'Dashboard' },
  { href: '/trades/new-put', label: 'Sell PUT' },
  { href: '/trades/new-call', label: 'Sell CALL' },
  { href: '/history', label: 'History' },
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
```

- [ ] **Step 4: Update root layout**

Replace `frontend/src/app/layout.tsx`:
```tsx
import type { Metadata } from 'next';
import { Inter } from 'next/font/google';
import './globals.css';
import { Sidebar } from '@/components/layout/Sidebar';
import { AccountProvider } from '@/contexts/AccountContext';

const inter = Inter({ subsets: ['latin'] });

export const metadata: Metadata = {
  title: 'Wheel Tracker',
  description: 'Options wheel strategy tracker',
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en">
      <body className={inter.className}>
        <AccountProvider>
          <div className="flex min-h-screen">
            <Sidebar />
            <main className="flex-1 p-6 bg-background">{children}</main>
          </div>
        </AccountProvider>
      </body>
    </html>
  );
}
```

- [ ] **Step 5: Verify frontend builds**
```bash
cd frontend && npm run build 2>&1 | tail -20
```
Expected: build succeeds (may have type warnings, no errors).

- [ ] **Step 6: Commit**
```bash
git add frontend/src/ && git commit -m "feat: app layout with sidebar and account context/selector"
```

---

## Task 11: Dashboard Page

**Files:**
- Create: `frontend/src/components/dashboard/MetricCard.tsx`
- Create: `frontend/src/components/dashboard/ActivePositions.tsx`
- Modify: `frontend/src/app/page.tsx`

- [ ] **Step 1: MetricCard component**

Create `frontend/src/components/dashboard/MetricCard.tsx`:
```tsx
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';

interface Props {
  title: string;
  value: string;
  subtitle?: string;
}

export function MetricCard({ title, value, subtitle }: Props) {
  return (
    <Card>
      <CardHeader className="pb-2">
        <CardTitle className="text-sm font-medium text-muted-foreground">{title}</CardTitle>
      </CardHeader>
      <CardContent>
        <div className="text-2xl font-bold">{value}</div>
        {subtitle && <p className="text-xs text-muted-foreground mt-1">{subtitle}</p>}
      </CardContent>
    </Card>
  );
}
```

- [ ] **Step 2: ActivePositions component**

Create `frontend/src/components/dashboard/ActivePositions.tsx`:
```tsx
'use client';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';
import { Badge } from '@/components/ui/badge';
import { formatCurrency, daysToExpiry } from '@/lib/utils';
import type { ShareLot, Trade } from '@/lib/types';

interface Props {
  openTrades: Trade[];
  activeLots: ShareLot[];
}

export function ActivePositions({ openTrades, activeLots }: Props) {
  return (
    <div className="space-y-6">
      <div>
        <h3 className="font-semibold mb-2">Open Trades</h3>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Ticker</TableHead>
              <TableHead>Type</TableHead>
              <TableHead>Strike</TableHead>
              <TableHead>Expiry</TableHead>
              <TableHead>DTE</TableHead>
              <TableHead>Premium</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {openTrades.length === 0 && (
              <TableRow><TableCell colSpan={6} className="text-center text-muted-foreground">No open trades</TableCell></TableRow>
            )}
            {openTrades.map((t) => (
              <TableRow key={t.id}>
                <TableCell className="font-medium">{t.ticker}</TableCell>
                <TableCell><Badge variant={t.trade_type === 'PUT' ? 'secondary' : 'default'}>{t.trade_type}</Badge></TableCell>
                <TableCell>{formatCurrency(t.strike_price)}</TableCell>
                <TableCell>{t.expiry_date}</TableCell>
                <TableCell>{daysToExpiry(t.expiry_date)}d</TableCell>
                <TableCell>{formatCurrency(t.premium_received)}</TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </div>

      <div>
        <h3 className="font-semibold mb-2">Share Lots</h3>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Ticker</TableHead>
              <TableHead>Shares</TableHead>
              <TableHead>Original CB</TableHead>
              <TableHead>Adjusted CB</TableHead>
              <TableHead>CB Reduction</TableHead>
              <TableHead>Source</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {activeLots.length === 0 && (
              <TableRow><TableCell colSpan={6} className="text-center text-muted-foreground">No share lots</TableCell></TableRow>
            )}
            {activeLots.map((lot) => (
              <TableRow key={lot.id}>
                <TableCell className="font-medium">{lot.ticker}</TableCell>
                <TableCell>{lot.quantity}</TableCell>
                <TableCell>{formatCurrency(lot.original_cost_basis)}</TableCell>
                <TableCell className="font-medium">{formatCurrency(lot.adjusted_cost_basis)}</TableCell>
                <TableCell className="text-green-600">
                  -{formatCurrency(lot.original_cost_basis - lot.adjusted_cost_basis)}
                </TableCell>
                <TableCell><Badge variant="outline">{lot.acquisition_type}</Badge></TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </div>
    </div>
  );
}
```

- [ ] **Step 3: Dashboard page**

Replace `frontend/src/app/page.tsx`:
```tsx
'use client';
import { useEffect, useState } from 'react';
import { MetricCard } from '@/components/dashboard/MetricCard';
import { ActivePositions } from '@/components/dashboard/ActivePositions';
import { api } from '@/lib/api';
import { formatCurrency, formatPercent } from '@/lib/utils';
import { useAccounts } from '@/contexts/AccountContext';
import type { DashboardData } from '@/lib/types';

export default function DashboardPage() {
  const { selectedAccountId } = useAccounts();
  const [data, setData] = useState<DashboardData | null>(null);

  useEffect(() => {
    api.dashboard(selectedAccountId ?? undefined).then(setData);
  }, [selectedAccountId]);

  if (!data) return <div className="text-muted-foreground">Loading...</div>;

  return (
    <div className="space-y-6">
      <h1 className="text-2xl font-bold">Dashboard</h1>
      <div className="grid grid-cols-1 sm:grid-cols-4 gap-4">
        <MetricCard
          title="Total Premium Collected"
          value={formatCurrency(data.total_premium_collected)}
          subtitle="All closed trades" />
        <MetricCard
          title="Capital Deployed"
          value={formatCurrency(data.total_capital_deployed)}
          subtitle="Open positions" />
        <MetricCard
          title="Realized Yield (Ann.)"
          value={formatPercent(data.realized_annualized_yield)}
          subtitle="Closed trades" />
        <MetricCard
          title="Open Yield (Ann.)"
          value={formatPercent(data.open_annualized_yield)}
          subtitle="Current open trades" />
      </div>
      <ActivePositions openTrades={data.open_trades} activeLots={data.active_share_lots} />
    </div>
  );
}
```

- [ ] **Step 4: Verify build**
```bash
cd frontend && npm run build 2>&1 | tail -20
```

- [ ] **Step 5: Commit**
```bash
git add frontend/src/ && git commit -m "feat: dashboard page with metrics and active positions"
```

---

## Task 12: PUT Trade Form + Close Modal

**Files:**
- Create: `frontend/src/components/trades/PutForm.tsx`
- Create: `frontend/src/components/trades/ClosePutModal.tsx`
- Create: `frontend/src/app/trades/new-put/page.tsx`

- [ ] **Step 1: PutForm component**

Create `frontend/src/components/trades/PutForm.tsx`:
```tsx
'use client';
import { useState } from 'react';
import { useRouter } from 'next/navigation';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { api } from '@/lib/api';
import { useAccounts } from '@/contexts/AccountContext';

export function PutForm() {
  const router = useRouter();
  const { selectedAccountId } = useAccounts();
  const [form, setForm] = useState({
    ticker: '', strike_price: '', expiry_date: '',
    open_date: new Date().toISOString().split('T')[0],
    premium_received: '', fees_open: '1.30',
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
      });
      router.push('/');
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : 'Failed to open trade');
    }
  };

  return (
    <Card className="max-w-md">
      <CardHeader><CardTitle>Sell to Open — PUT</CardTitle></CardHeader>
      <CardContent>
        <form onSubmit={handleSubmit} className="space-y-4">
          {[
            { label: 'Ticker', key: 'ticker', placeholder: 'AAPL', type: 'text' },
            { label: 'Strike Price', key: 'strike_price', placeholder: '150.00', type: 'number' },
            { label: 'Expiry Date', key: 'expiry_date', placeholder: '', type: 'date' },
            { label: 'Open Date', key: 'open_date', placeholder: '', type: 'date' },
            { label: 'Premium Received ($)', key: 'premium_received', placeholder: '200.00', type: 'number' },
            { label: 'Fees ($)', key: 'fees_open', placeholder: '1.30', type: 'number' },
          ].map(({ label, key, placeholder, type }) => (
            <div key={key} className="space-y-1">
              <Label>{label}</Label>
              <Input type={type} placeholder={placeholder}
                value={form[key as keyof typeof form]} onChange={set(key)} required />
            </div>
          ))}
          {error && <p className="text-sm text-destructive">{error}</p>}
          <Button type="submit" className="w-full">Open PUT Trade</Button>
        </form>
      </CardContent>
    </Card>
  );
}
```

- [ ] **Step 2: ClosePutModal**

Create `frontend/src/components/trades/ClosePutModal.tsx`:
```tsx
'use client';
import { useState } from 'react';
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
  const [open, setOpen] = useState(false);
  const [action, setAction] = useState('EXPIRED');
  const [closeDate, setCloseDate] = useState(new Date().toISOString().split('T')[0]);
  const [closePremium, setClosePremium] = useState('');
  const [feesClose, setFeesClose] = useState('1.30');
  const [error, setError] = useState('');

  const handleSubmit = async () => {
    try {
      await api.puts.close(tradeId, {
        action,
        close_date: closeDate,
        ...(action === 'BOUGHT_BACK' && { close_premium: parseFloat(closePremium), fees_close: parseFloat(feesClose) }),
      });
      setOpen(false);
      onClose();
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : 'Failed to close trade');
    }
  };

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        <Button size="sm" variant="outline">Close</Button>
      </DialogTrigger>
      <DialogContent>
        <DialogHeader><DialogTitle>Close PUT Trade</DialogTitle></DialogHeader>
        <div className="space-y-4">
          <div className="space-y-1">
            <Label>Action</Label>
            <Select value={action} onValueChange={setAction}>
              <SelectTrigger><SelectValue /></SelectTrigger>
              <SelectContent>
                <SelectItem value="EXPIRED">Expired Worthless</SelectItem>
                <SelectItem value="BOUGHT_BACK">Bought Back</SelectItem>
                <SelectItem value="ASSIGNED">Assigned (got shares)</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div className="space-y-1">
            <Label>Close Date</Label>
            <Input type="date" value={closeDate} onChange={(e) => setCloseDate(e.target.value)} />
          </div>
          {action === 'BOUGHT_BACK' && (
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
          {error && <p className="text-sm text-destructive">{error}</p>}
          <Button className="w-full" onClick={handleSubmit}>Confirm Close</Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
```

- [ ] **Step 3: Create page**

Create `frontend/src/app/trades/new-put/page.tsx`:
```tsx
import { PutForm } from '@/components/trades/PutForm';
export default function NewPutPage() {
  return (
    <div className="space-y-4">
      <h1 className="text-2xl font-bold">Sell to Open — PUT</h1>
      <PutForm />
    </div>
  );
}
```

- [ ] **Step 4: Verify build**
```bash
cd frontend && npm run build 2>&1 | tail -20
```

- [ ] **Step 5: Commit**
```bash
git add frontend/src/ && git commit -m "feat: PUT trade form and close modal"
```

---

## Task 13: CALL Trade Form + Close Modal

**Files:**
- Create: `frontend/src/components/trades/CallForm.tsx`
- Create: `frontend/src/components/trades/CloseCallModal.tsx`
- Create: `frontend/src/app/trades/new-call/page.tsx`

- [ ] **Step 1: CallForm component**

Create `frontend/src/components/trades/CallForm.tsx`:
```tsx
'use client';
import { useEffect, useState } from 'react';
import { useRouter } from 'next/navigation';
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
  const { selectedAccountId } = useAccounts();
  const [lots, setLots] = useState<ShareLot[]>([]);
  const [selectedLotId, setSelectedLotId] = useState('');
  const [form, setForm] = useState({
    ticker: '', strike_price: '', expiry_date: '',
    open_date: new Date().toISOString().split('T')[0],
    premium_received: '', fees_open: '1.30',
  });
  const [error, setError] = useState('');

  useEffect(() => {
    if (selectedAccountId) {
      api.shareLots.list(selectedAccountId).then((l) => {
        setLots(l);
        if (l.length === 1) {
          setSelectedLotId(String(l[0].id));
          setForm((f) => ({ ...f, ticker: l[0].ticker }));
        }
      });
    }
  }, [selectedAccountId]);

  const handleLotChange = (id: string) => {
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
      });
      router.push('/');
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : 'Failed to open trade');
    }
  };

  const selectedLot = lots.find((l) => l.id === Number(selectedLotId));

  return (
    <Card className="max-w-md">
      <CardHeader><CardTitle>Sell to Open — CALL</CardTitle></CardHeader>
      <CardContent>
        <form onSubmit={handleSubmit} className="space-y-4">
          <div className="space-y-1">
            <Label>Share Lot</Label>
            {lots.length === 0 ? (
              <p className="text-sm text-muted-foreground">No active share lots. Assign a PUT first.</p>
            ) : (
              <Select value={selectedLotId} onValueChange={handleLotChange}>
                <SelectTrigger>
                  <SelectValue placeholder="Select lot" />
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
          ].map(({ label, key, placeholder, type }) => (
            <div key={key} className="space-y-1">
              <Label>{label}</Label>
              <Input type={type} placeholder={placeholder}
                value={form[key as keyof typeof form]} onChange={set(key)} required />
            </div>
          ))}
          {error && <p className="text-sm text-destructive">{error}</p>}
          <Button type="submit" className="w-full" disabled={lots.length === 0}>Open CALL Trade</Button>
        </form>
      </CardContent>
    </Card>
  );
}
```

- [ ] **Step 2: CloseCallModal**

Create `frontend/src/components/trades/CloseCallModal.tsx`:
```tsx
'use client';
import { useState } from 'react';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogTrigger } from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { api } from '@/lib/api';

interface Props { tradeId: number; onClose: () => void; }

export function CloseCallModal({ tradeId, onClose }: Props) {
  const [open, setOpen] = useState(false);
  const [action, setAction] = useState('EXPIRED');
  const [closeDate, setCloseDate] = useState(new Date().toISOString().split('T')[0]);
  const [closePremium, setClosePremium] = useState('');
  const [feesClose, setFeesClose] = useState('1.30');
  const [error, setError] = useState('');

  const handleSubmit = async () => {
    try {
      await api.calls.close(tradeId, {
        action, close_date: closeDate,
        ...(action === 'BOUGHT_BACK' && { close_premium: parseFloat(closePremium), fees_close: parseFloat(feesClose) }),
      });
      setOpen(false);
      onClose();
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : 'Failed');
    }
  };

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild><Button size="sm" variant="outline">Close</Button></DialogTrigger>
      <DialogContent>
        <DialogHeader><DialogTitle>Close CALL Trade</DialogTitle></DialogHeader>
        <div className="space-y-4">
          <div className="space-y-1">
            <Label>Action</Label>
            <Select value={action} onValueChange={setAction}>
              <SelectTrigger><SelectValue /></SelectTrigger>
              <SelectContent>
                <SelectItem value="EXPIRED">Expired Worthless</SelectItem>
                <SelectItem value="BOUGHT_BACK">Bought Back</SelectItem>
                <SelectItem value="CALLED_AWAY">Called Away (shares sold)</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div className="space-y-1">
            <Label>Close Date</Label>
            <Input type="date" value={closeDate} onChange={(e) => setCloseDate(e.target.value)} />
          </div>
          {action === 'BOUGHT_BACK' && (
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
          {error && <p className="text-sm text-destructive">{error}</p>}
          <Button className="w-full" onClick={handleSubmit}>Confirm Close</Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
```

- [ ] **Step 3: Create page**

Create `frontend/src/app/trades/new-call/page.tsx`:
```tsx
import { CallForm } from '@/components/trades/CallForm';
export default function NewCallPage() {
  return (
    <div className="space-y-4">
      <h1 className="text-2xl font-bold">Sell to Open — CALL</h1>
      <CallForm />
    </div>
  );
}
```

- [ ] **Step 4: Verify build**
```bash
cd frontend && npm run build 2>&1 | tail -20
```

- [ ] **Step 5: Commit**
```bash
git add frontend/src/ && git commit -m "feat: CALL trade form and close modal"
```

---

## Task 14: History Page

**Files:**
- Create: `frontend/src/components/history/FilterBar.tsx`
- Create: `frontend/src/components/history/TradeTable.tsx`
- Create: `frontend/src/app/history/page.tsx`

- [ ] **Step 1: FilterBar component**

Create `frontend/src/components/history/FilterBar.tsx`:
```tsx
'use client';
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { getDateRangePreset } from '@/lib/utils';
import type { HistoryFilters } from '@/lib/types';

const CURRENT_YEAR = new Date().getFullYear();
const PRESETS = ['30d', '60d', '90d', 'ytd', String(CURRENT_YEAR), String(CURRENT_YEAR - 1)];

interface Props {
  filters: HistoryFilters;
  onChange: (f: HistoryFilters) => void;
}

export function FilterBar({ filters, onChange }: Props) {
  const [ticker, setTicker] = useState(filters.ticker ?? '');
  const [customFrom, setCustomFrom] = useState('');
  const [customTo, setCustomTo] = useState('');
  const [activePreset, setActivePreset] = useState('');

  const applyPreset = (preset: string) => {
    setActivePreset(preset);
    const range = getDateRangePreset(preset);
    onChange({ ...filters, date_from: range.date_from, date_to: range.date_to });
  };

  const applyCustom = () => {
    setActivePreset('custom');
    onChange({ ...filters, date_from: customFrom || undefined, date_to: customTo || undefined });
  };

  const applyTicker = () => onChange({ ...filters, ticker: ticker.toUpperCase() || undefined });

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap gap-2">
        {PRESETS.map((p) => (
          <Button key={p} size="sm"
            variant={activePreset === p ? 'default' : 'outline'}
            onClick={() => applyPreset(p)}>
            {p === 'ytd' ? 'YTD' : p}
          </Button>
        ))}
        <Button size="sm" variant={activePreset === '' ? 'default' : 'outline'} onClick={() => { setActivePreset(''); onChange({ ...filters, date_from: undefined, date_to: undefined }); }}>
          All Time
        </Button>
      </div>
      <div className="flex gap-2 items-end">
        <div>
          <p className="text-xs text-muted-foreground mb-1">From</p>
          <Input type="date" value={customFrom} onChange={(e) => setCustomFrom(e.target.value)} className="w-36" />
        </div>
        <div>
          <p className="text-xs text-muted-foreground mb-1">To</p>
          <Input type="date" value={customTo} onChange={(e) => setCustomTo(e.target.value)} className="w-36" />
        </div>
        <Button size="sm" variant="outline" onClick={applyCustom}>Apply Range</Button>
      </div>
      <div className="flex gap-2">
        <Input placeholder="Filter by ticker (AAPL)" value={ticker}
          onChange={(e) => setTicker(e.target.value)}
          onKeyDown={(e) => e.key === 'Enter' && applyTicker()}
          className="w-48" />
        <Button size="sm" variant="outline" onClick={applyTicker}>Search</Button>
        {(filters.ticker) && (
          <Button size="sm" variant="ghost" onClick={() => { setTicker(''); onChange({ ...filters, ticker: undefined }); }}>
            Clear
          </Button>
        )}
      </div>
    </div>
  );
}
```

- [ ] **Step 2: TradeTable component**

Create `frontend/src/components/history/TradeTable.tsx`:
```tsx
import { Badge } from '@/components/ui/badge';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';
import { formatCurrency } from '@/lib/utils';
import type { Trade } from '@/lib/types';

const STATUS_COLORS: Record<string, 'default' | 'secondary' | 'destructive' | 'outline'> = {
  OPEN: 'default', EXPIRED: 'secondary', BOUGHT_BACK: 'outline',
  ASSIGNED: 'secondary', CALLED_AWAY: 'outline',
};

function netPremium(t: Trade): number {
  return t.premium_received - t.fees_open - (t.close_premium ?? 0) - (t.fees_close ?? 0);
}

interface Props { trades: Trade[]; }

export function TradeTable({ trades }: Props) {
  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>Ticker</TableHead>
          <TableHead>Type</TableHead>
          <TableHead>Strike</TableHead>
          <TableHead>Open Date</TableHead>
          <TableHead>Close Date</TableHead>
          <TableHead>Premium</TableHead>
          <TableHead>Net</TableHead>
          <TableHead>Status</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {trades.length === 0 && (
          <TableRow><TableCell colSpan={8} className="text-center text-muted-foreground">No trades found</TableCell></TableRow>
        )}
        {trades.map((t) => (
          <TableRow key={t.id}>
            <TableCell className="font-medium">{t.ticker}</TableCell>
            <TableCell><Badge variant={t.trade_type === 'PUT' ? 'secondary' : 'default'}>{t.trade_type}</Badge></TableCell>
            <TableCell>{formatCurrency(t.strike_price)}</TableCell>
            <TableCell>{t.open_date}</TableCell>
            <TableCell>{t.close_date ?? '—'}</TableCell>
            <TableCell>{formatCurrency(t.premium_received)}</TableCell>
            <TableCell className={netPremium(t) >= 0 ? 'text-green-600' : 'text-red-500'}>
              {formatCurrency(netPremium(t))}
            </TableCell>
            <TableCell><Badge variant={STATUS_COLORS[t.status] ?? 'outline'}>{t.status}</Badge></TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
  );
}
```

- [ ] **Step 3: History page**

Create `frontend/src/app/history/page.tsx`:
```tsx
'use client';
import { useEffect, useState } from 'react';
import { FilterBar } from '@/components/history/FilterBar';
import { TradeTable } from '@/components/history/TradeTable';
import { api } from '@/lib/api';
import { useAccounts } from '@/contexts/AccountContext';
import type { HistoryFilters, Trade } from '@/lib/types';

export default function HistoryPage() {
  const { selectedAccountId } = useAccounts();
  const [trades, setTrades] = useState<Trade[]>([]);
  const [filters, setFilters] = useState<HistoryFilters>({});

  const load = (f: HistoryFilters) => {
    api.history({ ...f, account_id: selectedAccountId ?? undefined }).then(setTrades);
  };

  useEffect(() => { load(filters); }, [selectedAccountId, filters]);

  const handleFilterChange = (f: HistoryFilters) => {
    setFilters(f);
    load(f);
  };

  return (
    <div className="space-y-6">
      <h1 className="text-2xl font-bold">Trade History</h1>
      <FilterBar filters={filters} onChange={handleFilterChange} />
      <TradeTable trades={trades} />
    </div>
  );
}
```

- [ ] **Step 4: Verify final build**
```bash
cd frontend && npm run build 2>&1 | tail -20
```
Expected: build succeeds with no errors.

- [ ] **Step 5: Commit**
```bash
git add frontend/src/ && git commit -m "feat: history page with preset/custom filters and trade table"
```

---

## Task 15: Final Verification + Commit

- [ ] **Step 1: Run all backend tests**
```bash
cd /root/options_wheel_tracker/dev/backend && cargo test 2>&1
```
Expected: all tests pass.

- [ ] **Step 2: Run frontend build**
```bash
cd /root/options_wheel_tracker/dev/frontend && npm run build 2>&1 | tail -20
```
Expected: no errors.

- [ ] **Step 3: Final commit**
```bash
cd /root/options_wheel_tracker/dev
git add .
git commit -m "feat: complete MVP — wheel tracker backend + frontend"
```

---

## Task 16: Dev/Prod Environment Setup

**Context:** See full design spec at `docs/superpowers/specs/2026-03-22-dev-prod-isolation-design.md`.

This task is run **once on the home server** to establish the two-worktree structure. It is not part of the normal feature development cycle.

**Files:**
- Create: `scripts/refresh-dev-db.sh` (at repo root, outside worktrees)
- Create: `Makefile` (at repo root, outside worktrees)

- [ ] **Step 1: Set up worktrees on the home server**

```bash
# Clone the repo — this becomes the prod worktree
git clone git@github.com:<your-username>/options_wheel_tracker.git \
  /root/options_wheel_tracker/prod

cd /root/options_wheel_tracker/prod

# Create the dev worktree on the dev branch
git worktree add /root/options_wheel_tracker/dev dev

# Create shared directories (outside both worktrees)
mkdir -p /root/options_wheel_tracker/data
mkdir -p /root/options_wheel_tracker/logs
mkdir -p /root/options_wheel_tracker/scripts
```

- [ ] **Step 2: Create the refresh script**

Create `/root/options_wheel_tracker/scripts/refresh-dev-db.sh`:
```bash
#!/bin/bash
set -e

PROD_DB="/root/options_wheel_tracker/data/prod.db"
DEV_DB="/root/options_wheel_tracker/data/dev.db"

echo "[$(date)] Refreshing dev database from prod..."
# SQLite .backup does not interpret shell quoting — pass path without quotes
sqlite3 "$PROD_DB" ".backup $DEV_DB"
echo "[$(date)] Done. Restart the dev backend to pick up the new database."
```

```bash
chmod +x /root/options_wheel_tracker/scripts/refresh-dev-db.sh
```

- [ ] **Step 3: Create the root Makefile**

Create `/root/options_wheel_tracker/Makefile`:
```makefile
.PHONY: refresh-dev start-prod stop-prod start-dev stop-dev

PROD_DIR := /root/options_wheel_tracker/prod
DEV_DIR  := /root/options_wheel_tracker/dev

refresh-dev:
	@bash /root/options_wheel_tracker/scripts/refresh-dev-db.sh

# Each recipe line runs in its own subshell in Make.
# The `cd && command` pattern on a single line is intentional — do not split them.
start-prod:
	cd $(PROD_DIR)/backend && cargo run --release &
	cd $(PROD_DIR)/frontend && npm start &
	@echo "Started. Verify with: pgrep -a -f prod/backend"

stop-prod:
	pkill -f "prod/backend/target" || true
	pkill -f "prod/frontend"       || true

start-dev:
	cd $(DEV_DIR)/backend && cargo run &
	cd $(DEV_DIR)/frontend && npm run dev &
	@echo "Started. Verify with: pgrep -a -f dev/backend"

stop-dev:
	pkill -f "dev/backend/target" || true
	pkill -f "dev/frontend"       || true
```

- [ ] **Step 4: Configure .env files**

```bash
# Prod .env
cp /root/options_wheel_tracker/prod/.env.example \
   /root/options_wheel_tracker/prod/.env

# Edit prod/.env:
# DATABASE_URL=sqlite:///root/options_wheel_tracker/data/prod.db
# BACKEND_PORT=3001
# FRONTEND_PORT=3000
# NEXT_PUBLIC_API_URL=http://localhost:3001

# Dev .env (already matches .env.example defaults)
cp /root/options_wheel_tracker/dev/.env.example \
   /root/options_wheel_tracker/dev/.env
```

- [ ] **Step 5: Set up the nightly cron job**

```bash
crontab -e
```

Add this line:
```
0 2 * * * /root/options_wheel_tracker/scripts/refresh-dev-db.sh >> /root/options_wheel_tracker/logs/refresh.log 2>&1
```

- [ ] **Step 6: Verify both instances start cleanly**

```bash
# Start dev
make -C /root/options_wheel_tracker start-dev
pgrep -a -f dev/backend

# Start prod (after merging to main)
make -C /root/options_wheel_tracker start-prod
pgrep -a -f prod/backend
```

---

## Day-to-Day Workflow

```bash
# All development happens in the dev worktree
cd /root/options_wheel_tracker/dev

# Start dev environment
make -C /root/options_wheel_tracker start-dev
# Frontend: http://localhost:3002
# Backend:  http://localhost:3003

# Refresh dev database from prod (manual)
make -C /root/options_wheel_tracker refresh-dev
# Then restart dev backend to pick up new data

# Deploy to prod: merge PR on GitHub, then
cd /root/options_wheel_tracker/prod
git pull
make -C /root/options_wheel_tracker stop-prod
make -C /root/options_wheel_tracker start-prod
# Frontend: http://localhost:3000
# Backend:  http://localhost:3001
```

---

## API Reference Summary

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/accounts` | List accounts |
| POST | `/api/accounts` | Create account |
| DELETE | `/api/accounts/:id` | Delete account |
| POST | `/api/accounts/:id/puts` | Open PUT trade |
| POST | `/api/trades/puts/:id/close` | Close PUT (expire/buyback/assign) |
| POST | `/api/accounts/:id/calls` | Open CALL trade |
| POST | `/api/trades/calls/:id/close` | Close CALL (expire/buyback/called-away) |
| GET | `/api/accounts/:id/share-lots` | List active share lots |
| GET | `/api/dashboard?account_id=` | Dashboard metrics |
| GET | `/api/history?account_id=&ticker=&date_from=&date_to=` | Filtered history |
