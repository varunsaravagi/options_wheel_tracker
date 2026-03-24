#!/usr/bin/env bash
# Set up the cron job for automatic issue processing.
# Runs every 30 minutes, logs output to /root/options_wheel_tracker/logs/cron.log
#
# Usage: bash scripts/setup-cron.sh [install|remove|status]

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"
LOG_DIR="$(dirname "$REPO_ROOT")/logs"
CRON_CMD="cd $REPO_ROOT && /usr/bin/python3 scripts/process_issues.py >> $LOG_DIR/cron.log 2>&1"
CRON_ENTRY="*/30 * * * * $CRON_CMD"
CRON_MARKER="process_issues.py"

mkdir -p "$LOG_DIR"

case "${1:-status}" in
    install)
        # Remove existing entry if present, then add
        (crontab -l 2>/dev/null | grep -v "$CRON_MARKER"; echo "$CRON_ENTRY") | crontab -
        echo "Cron job installed: runs every 30 minutes"
        echo "  Logs: $LOG_DIR/cron.log"
        echo "  Agent logs: $LOG_DIR/issue-*.log"
        crontab -l | grep "$CRON_MARKER"
        ;;
    remove)
        crontab -l 2>/dev/null | grep -v "$CRON_MARKER" | crontab -
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
