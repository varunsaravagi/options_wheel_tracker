# Options Wheel Tracker

Options wheel strategy tracker — a web app to track selling cash-secured puts and covered calls across multiple brokerage accounts.

## Architecture

**Backend**: Rust / Axum 0.7, SQLite via SQLx 0.7, port 3003 (dev) / 3001 (prod)
**Frontend**: Next.js 16.2.1, React 19, base-ui components, port 3002 (dev) / 3000 (prod)
**Database**: SQLite files at `/root/options_wheel_tracker/data/{dev,prod}.db` (outside worktrees)

```
backend/
  src/
    handlers/           # HTTP handlers
      accounts.rs       # Account CRUD + purge (nulls FKs before bulk delete)
      puts.rs           # PUT open/close (EXPIRED, BOUGHT_BACK, ASSIGNED)
      calls.rs          # CALL open/close + share lot listing
      share_lots.rs     # Manual lot creation + sell
      dashboard.rs      # Dashboard metrics aggregation
      history.rs        # Trade history with filters
      statistics.rs     # Monthly income, cumulative P&L, ticker premium breakdown
      yield_calc.rs     # Shared annualized yield calculation logic
    models/             # Data models with SQLx runtime queries
      account.rs        # Account: id, name, created_at
      trade.rs          # Trade: lifecycle + net_premium(), unit tests
      share_lot.rs      # ShareLot: cost basis tracking, ACTIVE/CALLED_AWAY/SOLD, unit tests
    db/
      mod.rs            # Pool init (create_if_missing), migration runner
      migrations/       # Numbered SQL migrations (001_, 002_, 003_, ...)
    routes.rs           # All route registration + CORS
    errors.rs           # AppError (Database/NotFound/BadRequest) → JSON responses
    config.rs           # Config placeholder
    main.rs             # Entrypoint: load .env, init pool, run migrations, bind
frontend/
  src/
    app/                # Next.js App Router pages
      layout.tsx        # Root layout: Sidebar + AccountProvider
      page.tsx          # Dashboard (metrics + active positions)
      globals.css       # Tailwind + custom styles
      trades/
        new-put/page.tsx    # Sell PUT form
        new-call/page.tsx   # Sell CALL form (lot selector)
        new-lot/page.tsx    # Add manual share lot
      history/page.tsx      # Trade history with filter bar
      statistics/page.tsx   # Charts: monthly income, cumulative P&L, ticker premium
    components/
      layout/
        Sidebar.tsx           # Navigation + AccountSelector
        AccountSelector.tsx   # Account dropdown with create button
      dashboard/
        MetricCard.tsx        # Summary card (title, value, subtitle)
        ActivePositions.tsx   # Open trades + active lots tables with close buttons
      trades/
        PutForm.tsx           # PUT trade entry
        CallForm.tsx          # CALL trade entry
        ClosePutModal.tsx     # PUT close dialog
        CloseCallModal.tsx    # CALL close dialog
      history/
        FilterBar.tsx         # Ticker, date range, preset filters
        TradeTable.tsx        # Trade list table with status badges
      ui/                     # base-ui primitives (button, input, dialog, select, etc.)
    contexts/
      AccountContext.tsx   # Global account state: list, selectedId, setSelected, refresh
    lib/
      types.ts    # TypeScript interfaces for all API types
      api.ts      # API client (relative URLs via Next.js rewrites)
      utils.ts    # cn(), formatCurrency(), formatPercent(), daysToExpiry(), getDateRangePreset()
scripts/
  import_csv.py       # Schwab CSV transaction import
  process_issues.py   # GitHub issue processing workflow
  pre-commit          # Git pre-commit hook (fmt, lint, migration test)
  test-migration.sh   # Migration smoke test against real DB
  refresh-dev-db.sh   # sqlite3 .backup prod → dev
  setup-cron.sh       # Cron job setup
  issue-prompt-template.md
docs/
  superpowers/specs/  # Feature specs
  superpowers/plans/  # Implementation plans
Makefile              # start/stop for dev and prod, promote, deploy-prod
```

### API Endpoints

```
GET    /api/accounts                  List accounts
POST   /api/accounts                  Create account
DELETE /api/accounts/:id              Delete account
DELETE /api/accounts/:id/purge        Purge all trades + share lots for account
POST   /api/accounts/:id/puts         Open a PUT trade
POST   /api/accounts/:id/calls        Open a CALL trade (requires active share lot)
POST   /api/trades/puts/:id/close     Close PUT (EXPIRED, BOUGHT_BACK, ASSIGNED)
POST   /api/trades/calls/:id/close    Close CALL (EXPIRED, BOUGHT_BACK, CALLED_AWAY)
GET    /api/accounts/:id/share-lots   List active share lots
POST   /api/accounts/:id/share-lots   Create manual share lot
PUT    /api/share-lots/:id/sell       Sell share lot manually
GET    /api/dashboard                 Dashboard metrics (optional ?account_id=)
GET    /api/history                   Trade history (optional filters: account_id, ticker, date_from, date_to)
GET    /api/statistics                Aggregated stats (optional ?account_id=)
```

### Data Model

- **Account**: brokerage account (name)
- **Trade**: an options trade (PUT or CALL) with lifecycle: OPEN → EXPIRED / BOUGHT_BACK / ASSIGNED / CALLED_AWAY
- **ShareLot**: shares held, created via PUT assignment or manual entry. Status: ACTIVE → CALLED_AWAY / SOLD. Tracks original and adjusted cost basis (reduced by covered call premiums). Has `sale_price` and `sale_date` for SOLD status.

Circular FK relationship: `trades.share_lot_id → share_lots(id)` and `share_lots.source_trade_id → trades(id)`. Must null out cross-references before bulk deleting either table.

### Key Calculations

**Cost basis for assigned PUT:**
```
adjusted_cb = strike - (premium_received - fees_open) / (100 * quantity)
```

**Cost basis reduction when selling CALL:**
```
per_share = (premium_received - fees_open) / lot.quantity
adjusted_cost_basis -= per_share
```

**Annualized yield:**
```
yield = (net_premium / capital) * (365 / days_held) * 100
capital for PUT  = strike_price * 100 * quantity
capital for CALL = lot.adjusted_cost_basis * 100 * quantity
```

**Net premium:**
```
net_premium = premium_received - fees_open - close_premium - fees_close
```

## Development Guidelines

### Git Workflow

- **Feature branches fork from `dev`**: `git checkout dev && git checkout -b feat/...` or `fix/...`
- **PRs always target `dev`** as base branch (`gh pr create --base dev`)
- **`main` only receives merges from `dev`** — never merge feature branches directly into main
- Branch naming: `feat/description` for features, `fix/description` for bugs

### Backend Rules

- **SQLx runtime queries only** — use `sqlx::query_as::<_, Type>(...)`, never compile-time macros (`query!`, `query_as!`)
- **All handlers return `Result<Json<T>, AppError>`** or `Result<(StatusCode, Json<T>), AppError>`
- **Trade quantity**: every trade has a `quantity` field (number of contracts). Premium, capital, and cost basis calculations must account for quantity
- **Cost basis formula for assigned PUTs**: `adjusted_cb = strike - (premium - fees) / (100 * quantity)`
- **Share lot cost basis reduction**: when a covered call is sold, `per_share = premium_total / lot.quantity`
- **DB creation**: use `SqliteConnectOptions::from_str(url).create_if_missing(true)` with `connect_with()` — plain `connect()` won't create missing SQLite files
- **Error variants**: `AppError::NotFound` (404), `AppError::BadRequest(String)` (400), `AppError::Database(sqlx::Error)` (500)

### Frontend Rules

- **Next.js 16 has breaking changes** from older versions. Always read `node_modules/next/dist/docs/` before using unfamiliar APIs
- **base-ui components** (not Radix/shadcn). Some differences:
  - `SelectValue` renders the raw `value` prop by default — pass explicit children for display text
  - `DialogTrigger` doesn't support `asChild` — use the `render` prop instead
  - `Select.onValueChange` signature: `(value: string | null, ...)`
- **API calls use relative URLs** through Next.js rewrites — `BASE = ''` in `api.ts`. Never hardcode backend IP/port in frontend code
- **API proxy** is configured in `next.config.ts` via `rewrites()` — do not modify the rewrite rules without understanding the proxy setup
- **Account state** is global via `AccountContext` — use `useAccounts()` hook to access selected account

### SQLite Migration Rules

- SQLite doesn't support `ALTER CHECK` — to change CHECK constraints, recreate the table
- When recreating tables with FK references, **null out cross-references first**, then drop/recreate, then restore references. `PRAGMA foreign_keys = OFF` does not work inside SQLx migration transactions
- Always test migrations against a real database with data, not just `:memory:` — use `scripts/test-migration.sh`
- Number migrations sequentially: `001_`, `002_`, `003_`, etc.
- Migration history: 001 = initial schema, 002 = add quantity to trades, 003 = add SOLD status + sale_price/sale_date to share_lots

### Testing

- `cargo check` — must pass for all backend changes
- `cargo test` — unit tests with in-memory SQLite, must pass for all backend changes
- `npm run build` — must pass for all frontend changes
- `scripts/test-migration.sh` — run when creating or modifying migrations (also runs automatically via pre-commit hook)
- When fixing bugs, write a regression test. When adding features, write tests if the feature involves new model/handler logic
- Backend tests use `axum_test::TestServer` and `db::init_pool("sqlite::memory:")`

### Pre-commit Hook

The pre-commit hook at `scripts/pre-commit` runs automatically (via `core.hooksPath = scripts`):
- **Rust files staged**: `cargo fmt --check`
- **Frontend files staged**: `eslint --max-warnings 0`
- **Migration files staged**: `scripts/test-migration.sh`

If the hook isn't running, set: `git config core.hooksPath scripts`

### Deployment

- No Docker — apps run directly on the server
- Two git worktrees: `dev/` (tracks `dev` branch) and `prod/` (tracks `main`)
- Use Makefile targets: `make start-dev`, `make stop-dev`, `make start-prod`, `make stop-prod`
- Dev DB refresh from prod: `make refresh-dev` (uses sqlite3 `.backup`, safe for live DB)
- Promote to prod: `make promote` (merges dev → main) then `make deploy-prod`

## Async Issue Processing Workflow

Issues are implemented automatically using Claude Code in headless mode, orchestrated by `scripts/process_issues.py`. This script is typically run on a cron schedule (every 30 minutes) but can also be invoked manually.

### How It Works

The script polls GitHub for issues with specific labels, picks the highest-priority one, creates an isolated git worktree, and spawns a Claude Code agent to implement the fix or feature. The agent runs headless (`claude -p --permission-mode auto`) inside the worktree and is expected to commit, push, and open a PR.

### Label State Machine

```
todo             → agent picks it up, transitions to in-progress
in-progress      → agent is working (skipped on next poll)
pr-ready         → agent finished, PR open and awaiting human review
needs-revision   → human left PR feedback, agent should revise (priority over todo)
needs-attention  → agent failed or hit a constraint, needs human to investigate
needs-clarification → agent posted a question on the issue, waiting for human reply
manual           → repeated failures, requires human intervention
```

**Poll priority**: `in-progress` (skip) > `needs-revision` > `pr-ready` (skip) > `todo`

Only one issue is processed per run. If anything is `in-progress` or `pr-ready`, the script exits without picking up new work.

### Running the Script

```bash
# Normal poll — picks up the highest-priority eligible issue
python3 scripts/process_issues.py

# Dry run — shows what would happen without executing
python3 scripts/process_issues.py --dry-run

# Force a specific issue (ignores label check)
python3 scripts/process_issues.py --issue 42
```

### Cron Setup

```bash
bash scripts/setup-cron.sh install   # Install cron job (runs every 30 min)
bash scripts/setup-cron.sh status    # Check if cron is active
bash scripts/setup-cron.sh remove    # Remove cron job
```

Cron logs: `/root/options_wheel_tracker/logs/cron.log`
Agent logs: `/root/options_wheel_tracker/logs/issue-{N}-{impl|revision}-{timestamp}.log`

### Worktree Isolation

Each issue gets its own git worktree at:
```
/root/options_wheel_tracker/worktrees/issue-{N}/
```

- **New issues**: branch created from `origin/dev` as `feat/issue-N-slug` or `fix/issue-N-slug`
- **Revisions**: existing PR branch is checked out into a fresh worktree
- `node_modules` is symlinked from the dev worktree if available (avoids reinstalling)
- Worktrees are cleaned up after the agent finishes (branch is preserved for the PR)

### Branch Naming

Branches are auto-generated from the issue title and number:
```
fix/issue-{N}-{slug}    # for bugs (label: bug, or title contains fix/error/crash)
feat/issue-{N}-{slug}   # for features/enhancements
```

Slug is the lowercased, hyphenated, 40-char-max title.

### Prompt Templates

Two templates drive agent behaviour:

**`scripts/issue-prompt-template.md`** — for new implementations:
- Instructs agent to read CLAUDE.md first, understand the issue, implement minimally
- Requires writing tests (regression for bugs, feature tests for new logic)
- Run `cargo check && cargo test`, `npm run build`, or `scripts/test-migration.sh` as appropriate
- Commit, push, and open a PR targeting `dev` with `Closes #{number}`

**`scripts/revision-prompt-template.md`** — for PR revisions after human feedback:
- Instructs agent to address each piece of PR feedback
- Stay on existing branch, push to update the existing PR (do NOT create a new one)
- Same verification steps as above

### Agent Constraints (enforced via prompt)

- Modify at most **5 files** per issue (unless explicitly required)
- Do not delete existing files
- Do not install new dependencies (no changes to `Cargo.toml` or `package.json`)
- Do not modify: `Makefile`, `.env*`, `next.config.ts`, `.claude/` settings
- If implementation requires >~200 lines of new code, stop and ask for guidance
- If tests fail after 2 fix attempts, stop and comment the error on the issue/PR
- Hard limits: **10 minutes** timeout, **$5 USD** max API spend per issue

### Retry Logic

- **Implementation failures**: up to 3 retries. After 3 failures → `manual` label
- **Revision failures**: up to 2 retries. After 2 failures → `manual` label
- Re-labeling a `manual` issue back to `todo` resets the retry counter (tracked via `[agent-reset]` comments)
- Retry counts are tracked by scanning issue comments for `[agent-retry]` / `[agent-reset]` markers

### Required GitHub Labels

The script creates these labels automatically if they don't exist:

| Label | Color | Meaning |
|-------|-------|---------|
| `todo` | green | Ready for agent to pick up |
| `in-progress` | yellow | Agent currently working |
| `pr-ready` | blue | PR open, awaiting review |
| `needs-revision` | light blue | PR has feedback to address |
| `needs-attention` | red-orange | Agent failed, needs investigation |
| `needs-clarification` | peach | Agent asked a question |
| `manual` | dark red | Repeated failures, human required |

### Human Workflow

1. Create a GitHub issue describing the bug or feature
2. Add the `todo` label (plus `bug` or `enhancement` as appropriate)
3. Wait for the cron job to pick it up (or run manually with `--issue N`)
4. Review the PR when it appears (`pr-ready` label on the issue)
5. If changes needed: leave PR review comments, then change label from `pr-ready` → `needs-revision`
6. Agent will pick it up on next poll and push a revision

### Things NOT to Do

- Do not use Docker for running or deploying the app
- Do not change `next.config.ts` rewrite rules or the API base URL without understanding the proxy setup
- Do not hardcode server IPs in committed code — use environment variables or proxy rewrites
- Do not use `sqlx::query!` or `sqlx::query_as!` compile-time macros
- Do not create PRs targeting `main` from feature branches
- Do not add `Cargo.toml`, `Makefile`, `.env*`, or `next.config.ts` to changes without explicit approval
