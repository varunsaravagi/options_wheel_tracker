#!/bin/bash
set -e

PROD_DB="/root/options_wheel_tracker/data/prod.db"
DEV_DB="/root/options_wheel_tracker/data/dev.db"

echo "[$(date)] Refreshing dev database from prod..."
# SQLite .backup does not interpret shell quoting — pass path without quotes
sqlite3 "$PROD_DB" ".backup $DEV_DB"
echo "[$(date)] Done. Restart the dev backend to pick up the new database."
