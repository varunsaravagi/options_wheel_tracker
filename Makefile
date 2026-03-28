.PHONY: refresh-dev start-prod stop-prod start-dev stop-dev promote deploy-prod

PROD_DIR := /root/options_wheel_tracker
DEV_DIR  := /root/options_wheel_tracker/dev

refresh-dev:
	@bash /root/options_wheel_tracker/scripts/refresh-dev-db.sh

# Each recipe line runs in its own subshell in Make.
# The `cd && command` pattern on a single line is intentional — do not split them.
start-prod:
	cd $(PROD_DIR)/backend && cargo run --release &
	cd $(PROD_DIR)/frontend && PORT=$${FRONTEND_PORT:-3004} BACKEND_PORT=$${BACKEND_PORT:-3005} npm start &
	@echo "Started. Verify with: pgrep -a -f prod/backend"

stop-prod:
	fuser -k $${FRONTEND_PORT:-3004}/tcp 2>/dev/null || true
	fuser -k $${BACKEND_PORT:-3005}/tcp 2>/dev/null || true

start-dev:
	cd $(DEV_DIR)/backend && cargo run &
	cd $(DEV_DIR)/frontend && PORT=3001 npm run dev &
	@echo "Started. Verify with: pgrep -a -f dev/backend"

stop-dev:
	fuser -k 3001/tcp 2>/dev/null || true
	fuser -k 3003/tcp 2>/dev/null || true

promote:
	@echo "Merging dev into main..."
	cd $(PROD_DIR) && git fetch origin && git merge -X theirs origin/dev -m "Merge dev into main"
	cd $(PROD_DIR) && git push origin main
	@echo "main is now up to date with dev."

deploy-prod: stop-prod
	@echo "Pulling latest main in prod worktree..."
	cd $(PROD_DIR) && git pull origin main
	@echo "Building and starting prod..."
	cd $(PROD_DIR)/backend && cargo build --release
	cd $(PROD_DIR)/frontend && BACKEND_PORT=$${BACKEND_PORT:-3005} npm run build
	$(MAKE) start-prod
	@echo "Prod deployed."
