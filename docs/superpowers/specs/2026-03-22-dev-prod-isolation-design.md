# Dev/Prod Environment Isolation — Design Spec

## Goal

Isolate the production and development environments on a single home server so that developing new features never risks corrupting production data. Both environments run simultaneously on different ports.

## Constraints

- Single home server, apps run directly (no Docker for now)
- Single git repo hosted on GitHub
- SQLite database (file-based)
- Single user

---

## Directory Structure

Two git worktrees under the project root, with shared `data/`, `scripts/`, `logs/`, and a root `Makefile` for operational tasks:

```
/root/options_wheel_tracker/
  prod/               ← git worktree, always tracks `main` branch
    backend/
    frontend/
    .env              ← prod config (not committed)

  dev/                ← git worktree, always tracks `dev` branch
    backend/
    frontend/
    .env              ← dev config (not committed)

  data/
    prod.db           ← production SQLite file
    dev.db            ← dev SQLite file

  scripts/
    refresh-dev-db.sh ← copies prod.db → dev.db safely

  logs/
    refresh.log       ← output from scheduled refresh runs

  Makefile            ← start/stop prod, start/stop dev, refresh-dev
```

**Why `data/` lives outside both worktrees:** SQLite files must never be inside a git worktree — they would be picked up by `git status`, risk accidental commits, and could be corrupted by branch switches. Keeping them in a shared `data/` directory makes the separation explicit and safe.

---

## Environment Files

Each worktree has its own `.env` file (listed in `.gitignore`, never committed). The only meaningful differences are the database path and ports:

```bash
# prod/.env
DATABASE_URL=sqlite:///root/options_wheel_tracker/data/prod.db
BACKEND_PORT=3001
FRONTEND_PORT=3000

# dev/.env
DATABASE_URL=sqlite:///root/options_wheel_tracker/data/dev.db
BACKEND_PORT=3003
FRONTEND_PORT=3002
```

The project uses SQLx, which expects three slashes for an absolute path (`sqlite://` + `/absolute/path` = `sqlite:///absolute/path`). Four slashes is a common mistake and will cause a connection error.

The backend loads `.env` automatically via the `dotenvy` crate (`dotenvy::dotenv().ok()` in `main.rs`). This is already wired into the implementation plan.

The repo's committed `.gitignore` must include `.env` and `data/`. Since both worktrees check out the same `.gitignore` from the repo, this protection applies to both automatically. The `data/` directory lives outside both worktrees so it cannot be committed by either — the `.gitignore` entry there is belt-and-suspenders. The `.env` entry is the critical one: it prevents accidentally committing credentials or environment-specific config.

---

## Dev Database Refresh

The dev database is periodically refreshed from prod so it stays representative of real data. SQLite's `.backup` command is used rather than a raw `cp` — it is safe to run against a live database without stopping prod.

### Script: `scripts/refresh-dev-db.sh`

```bash
#!/bin/bash
set -e

PROD_DB="/root/options_wheel_tracker/data/prod.db"
DEV_DB="/root/options_wheel_tracker/data/dev.db"

echo "[$(date)] Refreshing dev database from prod..."
# SQLite .backup does not interpret shell quoting — pass the path without quotes
sqlite3 "$PROD_DB" ".backup $DEV_DB"
echo "[$(date)] Done. Restart the dev backend to pick up the new database."
```

### Triggers

**Manual** — via Makefile target, run from anywhere:
```bash
make -C /root/options_wheel_tracker refresh-dev
```

**Scheduled** — cron job running nightly at 2am:
```
0 2 * * * /root/options_wheel_tracker/scripts/refresh-dev-db.sh >> /root/options_wheel_tracker/logs/refresh.log 2>&1
```

---

## Makefile (Operational Tasks)

The root `Makefile` centralises all runbook knowledge in one place:

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

Note: `|| true` prevents Make from treating a "no process found" result as an error. Background processes (`&`) survive terminal/session close — this is intentional for a persistent server process.

---

## Git Workflow

```
GitHub (remote)
  └── main      ← production code
  └── dev       ← active development branch
  └── feature/* ← feature branches, merged into dev

Home server
  └── prod/     ← worktree on main
  └── dev/      ← worktree on dev (or a feature branch)
```

Day-to-day:
1. All development happens in `dev/` worktree against `dev.db`
2. Feature branches cut from `dev`, merged back to `dev` via PR
3. When ready to ship: PR from `dev` → `main` on GitHub, then `git pull` in `prod/`
4. Dev database refreshed from prod manually before starting a new feature, and automatically every night

---

## Initial Setup Sequence

Starting from a fresh server (one-time):

```bash
# 1. Clone the repo — this becomes the prod worktree
git clone git@github.com:<you>/options_wheel_tracker.git /root/options_wheel_tracker/prod
cd /root/options_wheel_tracker/prod

# 2. Add the dev worktree on the dev branch
git worktree add /root/options_wheel_tracker/dev dev

# 3. Create shared directories
mkdir -p /root/options_wheel_tracker/data
mkdir -p /root/options_wheel_tracker/logs
mkdir -p /root/options_wheel_tracker/scripts

# 4. Place the root Makefile
#    The Makefile lives at /root/options_wheel_tracker/Makefile — outside both worktrees.
#    It is tracked in the repo, so copy it from either worktree:
cp /root/options_wheel_tracker/prod/Makefile /root/options_wheel_tracker/Makefile
#    The scripts/ directory is also outside both worktrees. Copy from the repo:
cp -r /root/options_wheel_tracker/prod/scripts /root/options_wheel_tracker/scripts

# 5. Create .env files in each worktree (never committed)
cp /root/options_wheel_tracker/prod/.env.example /root/options_wheel_tracker/prod/.env
cp /root/options_wheel_tracker/dev/.env.example  /root/options_wheel_tracker/dev/.env
# Edit each .env to set correct DATABASE_URL and ports

# 6. Add the cron job
crontab -e
# Add: 0 2 * * * /root/options_wheel_tracker/scripts/refresh-dev-db.sh >> /root/options_wheel_tracker/logs/refresh.log 2>&1
```

`logs/refresh.log` will grow over time. Consider adding a logrotate config or periodically clearing it — for a nightly job this is low priority but worth noting.

---

## What Is NOT In Scope

- Docker / docker-compose (future)
- Process management / auto-restart on reboot (future, e.g. systemd)
- Database backups / prod data safety (separate concern)
