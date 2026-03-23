#!/usr/bin/env python3
"""
Import Schwab CSV transactions into the Options Wheel Tracker.

This script reads a Schwab brokerage transaction CSV export and replays the
relevant options-wheel events against the app's REST API.  The CSV lists
transactions newest-first, so we reverse the rows to process them in
chronological order — this ensures that "Sell to Open" creates a trade
*before* a later "Buy to Close" or "Expired" tries to close it.

Supported Schwab actions → app actions:
    Sell to Open   → POST /api/accounts/:id/puts  (or /calls)  — opens a new trade
    Buy to Close   → POST /api/trades/:type/:id/close  action=BOUGHT_BACK
    Expired        → POST /api/trades/:type/:id/close  action=EXPIRED
    Assigned (PUT) → POST /api/trades/puts/:id/close   action=ASSIGNED  (creates share lot)
    Assigned (CALL)→ POST /api/trades/calls/:id/close  action=CALLED_AWAY (sells shares)

Skipped actions (not part of the wheel strategy tracking):
    Buy, Sell, Journal, Cash Dividend, Credit Interest, Reinvest Dividend,
    Reinvest Shares, Wire Sent, Service Fee, Misc Cash Entry, Buy to Open

Usage:
    python3 scripts/import_csv.py <csv_file> <account_id> [--api-url http://localhost:3003] [--purge]

Options:
    --api-url URL   Override the default API base URL (default: http://localhost:3003)
    --purge         Delete all existing trades and share lots for the account before importing
"""

import csv
import json
import re
import sys
import urllib.request

API_URL = "http://localhost:3003"


def parse_symbol(symbol: str):
    """Parse a Schwab option symbol into its components.

    Schwab format: 'CRWV 03/20/2026 140.00 C'
                    ^^^^  ^^^^^^^^^^  ^^^^^^ ^
                    ticker  expiry    strike type (C=call, P=put)

    Returns (ticker, expiry_iso, strike, opt_type) or None if not an option symbol.
    The expiry is converted from MM/DD/YYYY to YYYY-MM-DD (ISO format) for the API.
    """
    parts = symbol.strip().split()
    if len(parts) < 4:
        # Not an option symbol (e.g. plain stock ticker like "AAPL")
        return None
    ticker = parts[0]
    expiry_raw = parts[1]  # MM/DD/YYYY
    strike = float(parts[2])
    opt_type = parts[3]  # C or P
    m, d, y = expiry_raw.split("/")
    expiry = f"{y}-{m}-{d}"
    return ticker, expiry, strike, opt_type


def parse_date(date_str: str) -> str:
    """Parse Schwab date field to ISO format (YYYY-MM-DD).

    Schwab sometimes shows two dates for settlement vs. trade date:
        '02/02/2026 as of 01/30/2026'
    The "as of" date is the actual trade/event date, so we prefer it.
    Plain dates like '03/16/2026' are used directly.
    """
    match = re.search(r"as of (\d{2}/\d{2}/\d{4})", date_str)
    if match:
        raw = match.group(1)
    else:
        raw = date_str.strip()
    m, d, y = raw.split("/")
    return f"{y}-{m}-{d}"


def parse_amount(amount_str: str) -> float:
    """Parse a dollar amount string like '$200.00' or '-$10.66' to float.
    The Amount column represents the net cash impact of the transaction.
    Empty strings (e.g. for expirations with no cash movement) return 0.0.
    """
    if not amount_str.strip():
        return 0.0
    s = amount_str.strip().replace("$", "").replace(",", "")
    return float(s)


def parse_price(price_str: str) -> float:
    """Parse the per-contract option price (e.g. '$2.00' means $2.00 per share).
    Multiply by 100 to get the per-contract dollar amount.
    """
    if not price_str.strip():
        return 0.0
    return float(price_str.strip().replace("$", "").replace(",", ""))


def api_request(path: str, method: str = "GET", data=None):
    """Make an HTTP request to the app's REST API.
    Uses stdlib urllib so the script has zero external dependencies.
    Returns parsed JSON on success, or None on HTTP error (logged to stderr).
    """
    url = f"{API_URL}{path}"
    body = json.dumps(data).encode() if data else None
    req = urllib.request.Request(url, data=body, method=method)
    req.add_header("Content-Type", "application/json")
    try:
        with urllib.request.urlopen(req) as resp:
            return json.loads(resp.read().decode())
    except urllib.error.HTTPError as e:
        err_body = e.read().decode()
        print(f"  ERROR {e.code}: {err_body}")
        return None


def find_open_trade(account_id: int, symbol_info: tuple):
    """Find an existing OPEN trade that matches this option contract.

    Used by Buy to Close, Expired, and Assigned actions to locate the trade
    that was originally opened by a "Sell to Open". Matches on all four
    identifying fields: ticker, trade_type, expiry, and strike price.
    Strike uses a small epsilon (0.01) to avoid float comparison issues.
    """
    ticker, expiry, strike, opt_type = symbol_info
    trade_type = "PUT" if opt_type == "P" else "CALL"

    # Fetch all trades for this account and search for the matching open one
    trades = api_request(f"/api/history?account_id={account_id}")
    if not trades:
        return None
    for t in trades:
        if (t["status"] == "OPEN"
                and t["ticker"] == ticker
                and t["trade_type"] == trade_type
                and t["expiry_date"] == expiry
                and abs(t["strike_price"] - strike) < 0.01):
            return t
    return None


def main():
    if len(sys.argv) < 3:
        print("Usage: python3 scripts/import_csv.py <csv_file> <account_id> [--api-url URL] [--purge]")
        sys.exit(1)

    csv_file = sys.argv[1]
    account_id = int(sys.argv[2])

    global API_URL
    if "--api-url" in sys.argv:
        idx = sys.argv.index("--api-url")
        API_URL = sys.argv[idx + 1]

    purge = "--purge" in sys.argv

    # Verify account exists
    accounts = api_request("/api/accounts")
    if not any(a["id"] == account_id for a in (accounts or [])):
        print(f"Account {account_id} not found. Available accounts:")
        for a in (accounts or []):
            print(f"  {a['id']}: {a['name']}")
        sys.exit(1)

    # If --purge is set, delete all existing trades and share lots for this account
    # before importing. This prevents duplicate data when re-running the script.
    if purge:
        print(f"Purging existing data for account {account_id}...")
        result = api_request(f"/api/accounts/{account_id}/purge", "DELETE")
        if result:
            print(f"  Deleted {result['trades_deleted']} trades, {result['share_lots_deleted']} share lots")
        else:
            print("  Failed to purge — aborting.")
            sys.exit(1)

    # Read CSV — Schwab exports have a header row followed by transactions newest-first
    with open(csv_file, newline="", encoding="utf-8") as f:
        reader = csv.DictReader(f)
        rows = list(reader)

    # Reverse to chronological order (oldest first) so that "Sell to Open"
    # creates trades before subsequent "Buy to Close" / "Expired" / "Assigned"
    # rows try to close them.
    rows.reverse()

    stats = {"opened": 0, "closed": 0, "assigned": 0, "skipped": 0, "errors": 0}

    for i, row in enumerate(rows):
        action = row["Action"]
        symbol = row["Symbol"]
        date = parse_date(row["Date"])
        # Quantity can be fractional for reinvested dividends (e.g. "11.98").
        # We parse as float first, then truncate to int for option contracts.
        qty_raw = row["Quantity"].strip().replace(",", "")
        qty = int(float(qty_raw)) if qty_raw else 0
        price = parse_price(row["Price"])
        fees = parse_amount(row["Fees & Comm"])
        amount = parse_amount(row["Amount"])

        # Skip actions that aren't part of the wheel strategy.
        # "Buy to Open" is skipped because we only sell options (wheel = sell-side).
        # Stock buys, dividends, and account-level entries are irrelevant.
        # Note: "Sell" of shares (non-option) IS handled below as a manual lot sale.
        if action in ("Buy", "Journal", "Cash Dividend", "Credit Interest",
                       "Reinvest Dividend", "Reinvest Shares", "Wire Sent",
                       "Service Fee", "Misc Cash Entry", "Buy to Open"):
            stats["skipped"] += 1
            continue

        # "Sell" of shares (not options) = manually selling a share lot
        if action == "Sell":
            symbol_info = parse_symbol(symbol)
            if symbol_info:
                # This is an option sell (Sell to Open is separate), skip it
                stats["skipped"] += 1
                continue
            # Plain stock sale — find the active lot for this ticker and mark it sold
            ticker = symbol.strip()
            if not ticker:
                stats["skipped"] += 1
                continue
            lots = api_request(f"/api/accounts/{account_id}/share-lots")
            lot = None
            if lots:
                for l in lots:
                    if l["ticker"] == ticker and l["status"] == "ACTIVE":
                        lot = l
                        break
            if not lot:
                print(f"  WARN row {i}: No active share lot for {ticker} to sell")
                stats["skipped"] += 1
                continue
            data = {"sale_price": price, "sale_date": date}
            result = api_request(f"/api/share-lots/{lot['id']}/sell", "PUT", data)
            if result:
                print(f"  SOLD share lot {ticker} @ ${price} on {date}")
                stats["closed"] += 1
            else:
                stats["errors"] += 1
            continue

        symbol_info = parse_symbol(symbol)
        if not symbol_info and action in ("Sell to Open", "Buy to Close", "Expired", "Assigned"):
            print(f"  SKIP row {i}: can't parse symbol '{symbol}' for action '{action}'")
            stats["skipped"] += 1
            continue

        if action == "Sell to Open":
            # Opening a new short option position (the core wheel action).
            # For PUTs: we're selling cash-secured puts.
            # For CALLs: we're selling covered calls against an existing share lot.
            ticker, expiry, strike, opt_type = symbol_info
            trade_type = "PUT" if opt_type == "P" else "CALL"

            # Schwab's "Price" is per-share (e.g. $2.00), but each contract = 100 shares,
            # so total premium = price * 100 * quantity_of_contracts.
            # We store gross premium and fees separately; the API computes net.
            premium_received = price * 100 * qty
            fees_open = abs(fees)

            if trade_type == "PUT":
                data = {
                    "ticker": ticker,
                    "strike_price": strike,
                    "expiry_date": expiry,
                    "open_date": date,
                    "premium_received": premium_received,
                    "fees_open": fees_open,
                    "quantity": qty,
                }
                result = api_request(f"/api/accounts/{account_id}/puts", "POST", data)
            else:
                # CALL — must be linked to an active share lot (covered call).
                # The app enforces this: you can only sell calls against shares you own.
                # Look up the first active lot for this ticker in the account.
                lots = api_request(f"/api/accounts/{account_id}/share-lots")
                lot = None
                if lots:
                    for l in lots:
                        if l["ticker"] == ticker and l["status"] == "ACTIVE":
                            lot = l
                            break
                if not lot:
                    print(f"  WARN row {i}: No active share lot for {ticker} CALL — creating trade without lot linkage")
                    stats["errors"] += 1
                    continue

                data = {
                    "share_lot_id": lot["id"],
                    "ticker": ticker,
                    "strike_price": strike,
                    "expiry_date": expiry,
                    "open_date": date,
                    "premium_received": premium_received,
                    "fees_open": fees_open,
                    "quantity": qty,
                }
                result = api_request(f"/api/accounts/{account_id}/calls", "POST", data)

            if result:
                print(f"  OPENED {trade_type} {ticker} ${strike} exp {expiry} qty={qty} premium=${premium_received} fees=${fees_open}")
                stats["opened"] += 1
            else:
                stats["errors"] += 1

        elif action == "Buy to Close":
            # Buying back a previously sold option to close the position early.
            # This is a voluntary exit — the trader pays a premium to close.
            ticker, expiry, strike, opt_type = symbol_info
            trade_type = "PUT" if opt_type == "P" else "CALL"
            trade = find_open_trade(account_id, symbol_info)
            if not trade:
                print(f"  WARN row {i}: No open {trade_type} trade for {ticker} ${strike} exp {expiry} to close")
                stats["errors"] += 1
                continue

            # close_premium = cost to buy back the option (reduces net profit)
            close_premium = price * 100 * qty
            fees_close = abs(fees)
            endpoint = "puts" if trade_type == "PUT" else "calls"
            data = {
                "action": "BOUGHT_BACK",
                "close_date": date,
                "close_premium": close_premium,
                "fees_close": fees_close,
            }
            result = api_request(f"/api/trades/{endpoint}/{trade['id']}/close", "POST", data)
            if result:
                print(f"  CLOSED (BOUGHT_BACK) {trade_type} {ticker} ${strike} exp {expiry} buy_price=${close_premium}")
                stats["closed"] += 1
            else:
                stats["errors"] += 1

        elif action == "Expired":
            # Option expired worthless — best outcome for the seller.
            # The full premium collected at open is kept as profit.
            if not symbol_info:
                stats["skipped"] += 1
                continue
            ticker, expiry, strike, opt_type = symbol_info
            trade_type = "PUT" if opt_type == "P" else "CALL"
            trade = find_open_trade(account_id, symbol_info)
            if not trade:
                print(f"  WARN row {i}: No open {trade_type} trade for {ticker} ${strike} exp {expiry} to expire")
                stats["errors"] += 1
                continue

            endpoint = "puts" if trade_type == "PUT" else "calls"
            data = {
                "action": "EXPIRED",
                "close_date": date,
            }
            result = api_request(f"/api/trades/{endpoint}/{trade['id']}/close", "POST", data)
            if result:
                print(f"  CLOSED (EXPIRED) {trade_type} {ticker} ${strike} exp {expiry}")
                stats["closed"] += 1
            else:
                stats["errors"] += 1

        elif action == "Assigned":
            # Option was exercised by the counterparty:
            #   PUT assigned  → we buy 100 shares at the strike price (share lot created)
            #   CALL assigned → our shares are sold (called away) at the strike price
            if not symbol_info:
                stats["skipped"] += 1
                continue
            ticker, expiry, strike, opt_type = symbol_info
            trade_type = "PUT" if opt_type == "P" else "CALL"
            trade = find_open_trade(account_id, symbol_info)
            if not trade:
                print(f"  WARN row {i}: No open {trade_type} trade for {ticker} ${strike} exp {expiry} to assign")
                stats["errors"] += 1
                continue

            if trade_type == "PUT":
                # PUT assigned → acquire shares. The API automatically creates a
                # share lot with cost basis = strike - net premium per share.
                data = {
                    "action": "ASSIGNED",
                    "close_date": date,
                }
                result = api_request(f"/api/trades/puts/{trade['id']}/close", "POST", data)
                if result:
                    print(f"  ASSIGNED PUT {ticker} ${strike} exp {expiry} -> share lot created")
                    stats["assigned"] += 1
                else:
                    stats["errors"] += 1
            else:
                # CALL assigned → shares called away at the strike price.
                # The API marks the linked share lot as CALLED_AWAY.
                data = {
                    "action": "CALLED_AWAY",
                    "close_date": date,
                }
                result = api_request(f"/api/trades/calls/{trade['id']}/close", "POST", data)
                if result:
                    print(f"  CALLED_AWAY CALL {ticker} ${strike} exp {expiry} -> shares sold")
                    stats["assigned"] += 1
                else:
                    stats["errors"] += 1

        else:
            stats["skipped"] += 1

    print(f"\nDone! Opened: {stats['opened']}, Closed: {stats['closed']}, "
          f"Assigned: {stats['assigned']}, Skipped: {stats['skipped']}, Errors: {stats['errors']}")


if __name__ == "__main__":
    main()
