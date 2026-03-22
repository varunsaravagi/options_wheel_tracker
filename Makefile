.PHONY: refresh-dev start-prod stop-prod start-dev stop-dev

PROD_DIR := /root/options_wheel_tracker
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
