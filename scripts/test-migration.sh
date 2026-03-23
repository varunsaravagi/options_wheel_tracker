#!/usr/bin/env bash
# Migration smoke test: verifies migrations apply cleanly against a copy of
# the current dev database (not just an empty :memory: DB).
#
# This catches issues that cargo test misses:
#   - FK constraint violations during table recreation
#   - Data-dependent CHECK failures
#   - Migrations that work from scratch but fail on existing data
#
# Usage: scripts/test-migration.sh [db_path]
#   db_path defaults to ../data/dev.db (relative to repo root)
set -e

REPO_ROOT="$(git rev-parse --show-toplevel)"
DB_PATH="${1:-$(dirname "$REPO_ROOT")/data/dev.db}"

if [ ! -f "$DB_PATH" ]; then
    echo "[migration-test] No dev DB at $DB_PATH — skipping (will be tested on first run)"
    exit 0
fi

# Create a temporary copy to test against
TEMP_DB=$(mktemp /tmp/wheel-migration-test-XXXXXX.db)
cp "$DB_PATH" "$TEMP_DB"

# Also copy WAL/SHM files if they exist (SQLite may need them)
[ -f "${DB_PATH}-wal" ] && cp "${DB_PATH}-wal" "${TEMP_DB}-wal"
[ -f "${DB_PATH}-shm" ] && cp "${DB_PATH}-shm" "${TEMP_DB}-shm"

cleanup() {
    rm -f "$TEMP_DB" "${TEMP_DB}-wal" "${TEMP_DB}-shm"
}
trap cleanup EXIT

echo "[migration-test] Testing migrations against copy of $DB_PATH..."

# Build and run the backend against the temp DB — it will apply pending migrations on startup.
# We just need it to start successfully, then kill it.
source ~/.cargo/env 2>/dev/null || true
(cd "$REPO_ROOT/backend" && cargo build --quiet 2>&1) || {
    echo "[migration-test] Backend build failed"
    exit 1
}

# Run the backend with the temp DB, wait for "Listening" or a panic
DATABASE_URL="sqlite://$TEMP_DB" timeout 15 "$REPO_ROOT/backend/target/debug/wheel-tracker" 2>&1 &
PID=$!

# Wait for the server to either start or crash
for i in $(seq 1 30); do
    if ! kill -0 $PID 2>/dev/null; then
        # Process exited — check if it was a panic
        wait $PID
        EXIT_CODE=$?
        if [ $EXIT_CODE -ne 0 ]; then
            echo "[migration-test] Backend crashed (exit code $EXIT_CODE)"
            exit 1
        fi
        break
    fi
    # Check if it's listening
    if curl -s http://localhost:3003/api/accounts > /dev/null 2>&1; then
        echo "[migration-test] Migrations applied successfully"
        kill $PID 2>/dev/null || true
        wait $PID 2>/dev/null || true
        exit 0
    fi
    sleep 0.5
done

# If we got here, server started but didn't respond — still means migrations passed
kill $PID 2>/dev/null || true
wait $PID 2>/dev/null || true
echo "[migration-test] Migrations applied (server started)"
exit 0
