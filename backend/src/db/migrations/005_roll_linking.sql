-- Add roll-linking columns to trades.
-- rolled_from_trade_id: set on the NEW trade, points back to the previous rolled leg.
-- rolled_to_trade_id:   set on the OLD trade, points forward to the replacement leg.
-- Both are nullable integers. No FK constraint to avoid circular reference complexity.
ALTER TABLE trades ADD COLUMN rolled_from_trade_id INTEGER;
ALTER TABLE trades ADD COLUMN rolled_to_trade_id INTEGER;
