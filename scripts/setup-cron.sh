#!/usr/bin/env bash
# Set up the cron job for automatic issue processing.
# Runs every 30 minutes, logs output to /root/options_wheel_tracker/logs/cron.log
#
# Usage: bash scripts/setup-cron.sh [install|remove|status]

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"
LOG_DIR="$(dirname "$REPO_ROOT")/logs"
# Capture the current PATH so cron has access to npm, cargo, claude, gh, etc.
# Cron runs with a minimal PATH that won't find these tools.
CRON_PATH="$HOME/.cargo/bin:$HOME/.local/bin:$(dirname "$(which node 2>/dev/null || echo /usr/bin/node)"):$PATH"
CRON_CMD="export PATH=$CRON_PATH && cd $REPO_ROOT && /usr/bin/python3 scripts/process_issues.py >> $LOG_DIR/cron.log 2>&1"
CRON_ENTRY="*/30 * * * * $CRON_CMD"
CRON_MARKER="process_issues.py"

mkdir -p "$LOG_DIR"

case "${1:-status}" in
    install)
        # Remove existing entry if present, then add.
        # Use temp file to avoid pipe issues with set -e and empty crontabs.
        TMPFILE=$(mktemp)
        crontab -l 2>/dev/null | grep -v "$CRON_MARKER" > "$TMPFILE" || true
        echo "$CRON_ENTRY" >> "$TMPFILE"
        crontab "$TMPFILE"
        rm -f "$TMPFILE"
        echo "Cron job installed: runs every 30 minutes"
        echo "  Logs: $LOG_DIR/cron.log"
        echo "  Agent logs: $LOG_DIR/issue-*.log"
        crontab -l | grep "$CRON_MARKER"
        ;;
    remove)
        TMPFILE=$(mktemp)
        crontab -l 2>/dev/null | grep -v "$CRON_MARKER" > "$TMPFILE" || true
        crontab "$TMPFILE"
        rm -f "$TMPFILE"
        echo "Cron job removed"
        ;;
    status)
        if crontab -l 2>/dev/null | grep -q "$CRON_MARKER"; then
            echo "Cron job is ACTIVE:"
            crontab -l | grep "$CRON_MARKER"
        else
            echo "Cron job is NOT installed"
            echo "Run: bash scripts/setup-cron.sh install"
        fi
        ;;
    *)
        echo "Usage: $0 [install|remove|status]"
        exit 1
        ;;
esac
