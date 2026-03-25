#!/usr/bin/env python3
"""
Agentic issue processor for the Options Wheel Tracker.

Polls GitHub for issues labeled 'todo' or 'needs-revision', picks one based
on priority, and spawns Claude Code in headless mode to implement the fix,
feature, or revision. Each issue gets its own git worktree for isolation.

Label state machine:
    todo                → agent picks it up, moves to in-progress
    in-progress         → agent is working (skipped on next poll)
    pr-ready            → agent finished, PR is open for review
    needs-revision      → human left PR feedback, agent should revise (priority over todo)
    needs-attention     → agent failed or hit a constraint
    needs-clarification → agent commented a question, waiting for human
    manual              → repeated failures, needs human intervention

Poll priority: in-progress (skip) > needs-revision > pr-ready (skip) > todo

Usage:
    python3 scripts/process_issues.py [--dry-run] [--issue NUMBER]

Options:
    --dry-run       Show what would happen without executing
    --issue NUMBER  Process a specific issue by number (skips polling)
"""

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import time
from pathlib import Path

# --- Configuration ---

# Repository info (extracted from git remote)
REPO = None  # Set dynamically from git remote

# Paths
REPO_ROOT = Path(__file__).resolve().parent.parent  # /root/options_wheel_tracker/dev
WORKTREE_BASE = REPO_ROOT.parent / "worktrees"  # /root/options_wheel_tracker/worktrees
PROMPT_TEMPLATE = REPO_ROOT / "scripts" / "issue-prompt-template.md"
REVISION_PROMPT_TEMPLATE = REPO_ROOT / "scripts" / "revision-prompt-template.md"
LOG_DIR = REPO_ROOT.parent / "logs"

# Agent constraints
MAX_TIMEOUT_SECONDS = 600  # 10 minutes
MAX_RETRIES = 3
MAX_REVISION_RETRIES = 2
MAX_BUDGET_USD = 5.0  # Maximum API spend per issue


def run(cmd: list[str], cwd: str | None = None, check: bool = True,
        capture: bool = True, timeout: int | None = None) -> subprocess.CompletedProcess:
    """Run a subprocess command and return the result."""
    result = subprocess.run(
        cmd, cwd=cwd, capture_output=capture, text=True,
        timeout=timeout, check=False
    )
    if check and result.returncode != 0:
        stderr = result.stderr.strip() if result.stderr else ""
        raise RuntimeError(f"Command failed: {' '.join(cmd)}\n{stderr}")
    return result


def get_repo_name() -> str:
    """Extract owner/repo from git remote URL."""
    result = run(["gh", "repo", "view", "--json", "nameWithOwner", "-q", ".nameWithOwner"],
                 cwd=str(REPO_ROOT))
    return result.stdout.strip()


def log(msg: str):
    """Print a timestamped log message."""
    ts = time.strftime("%Y-%m-%d %H:%M:%S")
    print(f"[{ts}] {msg}")


def gh_api(endpoint: str, method: str = "GET", data: dict | None = None,
           cwd: str | None = None) -> dict | list | None:
    """Call GitHub API via gh CLI."""
    cmd = ["gh", "api", endpoint]
    if method != "GET":
        cmd.extend(["--method", method])
    if data:
        for key, value in data.items():
            cmd.extend(["-f", f"{key}={value}"])
    result = run(cmd, cwd=cwd or str(REPO_ROOT), check=False)
    if result.returncode != 0:
        log(f"  gh api error: {result.stderr.strip()}")
        return None
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError:
        return None


def fetch_todo_issues() -> list[dict]:
    """Fetch open issues labeled 'todo', oldest first."""
    result = run([
        "gh", "issue", "list",
        "--label", "todo",
        "--state", "open",
        "--search", "sort:created-asc",
        "--json", "number,title,body,labels,createdAt",
        "--limit", "10"
    ], cwd=str(REPO_ROOT))
    issues = json.loads(result.stdout)
    return issues


def fetch_issue(number: int) -> dict | None:
    """Fetch a specific issue by number."""
    result = run([
        "gh", "issue", "view", str(number),
        "--json", "number,title,body,labels,createdAt,state"
    ], cwd=str(REPO_ROOT), check=False)
    if result.returncode != 0:
        return None
    return json.loads(result.stdout)


def is_in_progress() -> bool:
    """Check if any issue is currently in-progress."""
    result = run([
        "gh", "issue", "list",
        "--label", "in-progress",
        "--state", "open",
        "--json", "number",
        "--limit", "1"
    ], cwd=str(REPO_ROOT))
    issues = json.loads(result.stdout)
    return len(issues) > 0


def has_pending_pr() -> bool:
    """Check if any issue is labeled pr-ready (awaiting human merge)."""
    result = run([
        "gh", "issue", "list",
        "--label", "pr-ready",
        "--state", "open",
        "--json", "number",
        "--limit", "1"
    ], cwd=str(REPO_ROOT))
    issues = json.loads(result.stdout)
    return len(issues) > 0


def fetch_needs_revision_issue() -> dict | None:
    """Fetch the oldest open issue labeled 'needs-revision'."""
    result = run([
        "gh", "issue", "list",
        "--label", "needs-revision",
        "--state", "open",
        "--search", "sort:created-asc",
        "--json", "number,title,body,labels,createdAt",
        "--limit", "1"
    ], cwd=str(REPO_ROOT))
    issues = json.loads(result.stdout)
    return issues[0] if issues else None


def find_issue_pr(issue_number: int) -> dict | None:
    """Find the open PR for an issue by searching for its branch prefix.

    Returns dict with 'number', 'headRefName', 'url' keys, or None.
    Uses regex to avoid false positives (e.g., issue-1- matching issue-12-).
    """
    result = run([
        "gh", "pr", "list",
        "--state", "open",
        "--json", "number,headRefName,url",
        "--limit", "20"
    ], cwd=str(REPO_ROOT), check=False)
    if result.returncode != 0:
        return None
    prs = json.loads(result.stdout)
    pattern = re.compile(rf'(?:^|/)issue-{issue_number}-')
    for pr in prs:
        if pattern.search(pr["headRefName"]):
            return pr
    return None


def set_label(issue_number: int, add: str, remove: str | None = None):
    """Add a label and optionally remove another."""
    if remove:
        run(["gh", "issue", "edit", str(issue_number),
             "--remove-label", remove, "--add-label", add],
            cwd=str(REPO_ROOT))
    else:
        run(["gh", "issue", "edit", str(issue_number),
             "--add-label", add],
            cwd=str(REPO_ROOT))


def comment_on_issue(issue_number: int, body: str):
    """Post a comment on the issue."""
    run(["gh", "issue", "comment", str(issue_number), "--body", body],
        cwd=str(REPO_ROOT))


def get_retry_count(issue_number: int) -> int:
    """Count [agent-retry] comments since the last [agent-reset] marker.

    When a user re-labels an issue from manual back to todo, we post an
    [agent-reset] comment to zero out the counter so old failures don't
    immediately re-trigger the manual threshold.
    """
    result = run([
        "gh", "issue", "view", str(issue_number),
        "--json", "comments",
        "-q", '.comments[].body'
    ], cwd=str(REPO_ROOT), check=False)
    if result.returncode != 0:
        return 0
    # Walk comments in order; reset counter on [agent-reset], increment on [agent-retry]
    count = 0
    for line in result.stdout.strip().splitlines():
        if line.startswith("[agent-reset]"):
            count = 0
        elif line.startswith("[agent-retry]"):
            count += 1
    return count


def get_revision_retry_count(issue_number: int) -> int:
    """Count [revision-retry] comments since the last [revision-reset] marker."""
    result = run([
        "gh", "issue", "view", str(issue_number),
        "--json", "comments",
        "-q", '.comments[].body'
    ], cwd=str(REPO_ROOT), check=False)
    if result.returncode != 0:
        return 0
    count = 0
    for line in result.stdout.strip().splitlines():
        if line.startswith("[revision-reset]"):
            count = 0
        elif line.startswith("[revision-retry]"):
            count += 1
    return count


def fetch_issue_comments(issue_number: int) -> str:
    """Fetch all comments on an issue, formatted for inclusion in prompts."""
    result = run([
        "gh", "issue", "view", str(issue_number),
        "--json", "comments",
        "-q", r'.comments[] | "**\(.author.login)** (\(.createdAt)):\n\(.body)\n"'
    ], cwd=str(REPO_ROOT), check=False)
    if result.returncode != 0 or not result.stdout.strip():
        return "(no comments)"
    return result.stdout.strip()


def fetch_pr_comments(pr_number: int) -> str:
    """Fetch PR comments (regular + review) for revision prompts.

    Excludes bot comments. Review comments include file path and line number.
    """
    comments = []

    # Regular PR comments (conversation)
    result = run([
        "gh", "api", f"repos/{REPO}/issues/{pr_number}/comments",
        "--jq", r'.[] | select(.user.type != "Bot") | "**\(.user.login)** (\(.created_at)):\n\(.body)\n"'
    ], cwd=str(REPO_ROOT), check=False)
    if result.returncode == 0 and result.stdout.strip():
        comments.append("### PR Comments\n")
        comments.append(result.stdout.strip())

    # Review comments (inline on code)
    result = run([
        "gh", "api", f"repos/{REPO}/pulls/{pr_number}/comments",
        "--jq", r'.[] | select(.user.type != "Bot") | "**\(.user.login)** (\(.created_at)) on `\(.path):\(.line // .original_line)`:\n\(.body)\n"'
    ], cwd=str(REPO_ROOT), check=False)
    if result.returncode == 0 and result.stdout.strip():
        comments.append("\n### Inline Review Comments\n")
        comments.append(result.stdout.strip())

    return "\n".join(comments) if comments else "(no PR comments)"


def slugify(text: str) -> str:
    """Convert issue title to branch-name-safe slug."""
    slug = text.lower()
    slug = re.sub(r'[^a-z0-9\s-]', '', slug)
    slug = re.sub(r'[\s]+', '-', slug)
    slug = slug.strip('-')[:40]
    return slug


def determine_issue_type(issue: dict) -> str:
    """Determine if issue is a bug or feature from labels."""
    label_names = [l["name"] for l in issue.get("labels", [])]
    if "bug" in label_names:
        return "bug"
    if "feature" in label_names or "enhancement" in label_names:
        return "feature"
    # Guess from title
    title = issue["title"].lower()
    if any(w in title for w in ["fix", "bug", "error", "crash", "broken", "wrong"]):
        return "bug"
    return "feature"


def build_prompt(issue: dict, branch_name: str) -> str:
    """Build the agent prompt from the template and issue data."""
    template = PROMPT_TEMPLATE.read_text()
    label_names = ", ".join(l["name"] for l in issue.get("labels", []))
    issue_type = determine_issue_type(issue)
    comments = fetch_issue_comments(issue["number"])

    prompt = (template
        .replace("{number}", str(issue["number"]))
        .replace("{title}", issue["title"])
        .replace("{labels}", label_names)
        .replace("{issue_type}", issue_type)
        .replace("{body}", issue.get("body", "(no description)") or "(no description)")
        .replace("{branch_name}", branch_name)
        .replace("{comments}", comments))
    return prompt


def create_worktree(issue_number: int, branch_name: str, revision: bool = False) -> Path:
    """Create an isolated git worktree for this issue.

    When revision=False (default): creates a new branch from origin/dev.
    When revision=True: checks out the existing branch (preserving PR commits).
    """
    worktree_path = WORKTREE_BASE / f"issue-{issue_number}"

    # Clean up if leftover from a previous attempt
    if worktree_path.exists():
        log(f"  Cleaning up stale worktree at {worktree_path}")
        run(["git", "worktree", "remove", "--force", str(worktree_path)],
            cwd=str(REPO_ROOT), check=False)
        if worktree_path.exists():
            shutil.rmtree(worktree_path)

    WORKTREE_BASE.mkdir(parents=True, exist_ok=True)

    if revision:
        # Revision mode: check out existing branch (preserves PR commits)
        # Fetch into local ref to ensure it exists and is up to date
        run(["git", "fetch", "origin", f"{branch_name}:{branch_name}"],
            cwd=str(REPO_ROOT), check=False)
        run(["git", "worktree", "add", str(worktree_path), branch_name],
            cwd=str(REPO_ROOT))
    else:
        # Implementation mode: new branch from origin/dev
        run(["git", "fetch", "origin", "dev"], cwd=str(REPO_ROOT))
        run(["git", "branch", "-D", branch_name], cwd=str(REPO_ROOT), check=False)
        run(["git", "worktree", "add", "-b", branch_name,
             str(worktree_path), "origin/dev"],
            cwd=str(REPO_ROOT))

    # Install frontend dependencies if node_modules doesn't exist.
    # Symlink from the main dev worktree if available (faster than npm install).
    frontend_nm = worktree_path / "frontend" / "node_modules"
    dev_nm = REPO_ROOT / "frontend" / "node_modules"
    if not frontend_nm.exists() and dev_nm.exists():
        log("  Symlinking node_modules from dev worktree...")
        frontend_nm.symlink_to(dev_nm)
    elif not frontend_nm.exists():
        log("  Installing frontend dependencies in worktree...")
        try:
            run(["npm", "install"], cwd=str(worktree_path / "frontend"), check=False)
        except FileNotFoundError:
            log("  WARN: npm not found — agent will need to install deps if needed")

    return worktree_path


def cleanup_worktree(issue_number: int):
    """Remove the worktree after processing."""
    worktree_path = WORKTREE_BASE / f"issue-{issue_number}"
    if worktree_path.exists():
        run(["git", "worktree", "remove", "--force", str(worktree_path)],
            cwd=str(REPO_ROOT), check=False)


def run_agent(worktree_path: Path, prompt: str) -> tuple[bool, str]:
    """Run Claude Code in headless mode. Returns (success, output)."""
    cmd = [
        "claude", "-p",
        "--permission-mode", "auto",
        "--max-budget-usd", str(MAX_BUDGET_USD),
        prompt
    ]

    log("  Running Claude Code agent...")
    result = run(
        cmd,
        cwd=str(worktree_path),
        check=False,
        timeout=MAX_TIMEOUT_SECONDS
    )

    output = result.stdout or ""
    if result.stderr:
        output += "\n" + result.stderr

    success = result.returncode == 0
    return success, output


def check_pr_created(issue_number: int, branch_name: str) -> bool:
    """Check if a PR was created for this issue's branch."""
    result = run([
        "gh", "pr", "list",
        "--state", "open",
        "--head", branch_name,
        "--json", "number",
        "--limit", "1"
    ], cwd=str(REPO_ROOT), check=False)
    if result.returncode != 0:
        return False
    prs = json.loads(result.stdout)
    return len(prs) > 0


def save_log(issue_number: int, output: str, revision: bool = False):
    """Save agent output to a log file."""
    LOG_DIR.mkdir(parents=True, exist_ok=True)
    ts = time.strftime("%Y%m%d-%H%M%S")
    kind = "revision" if revision else "impl"
    log_file = LOG_DIR / f"issue-{issue_number}-{kind}-{ts}.log"
    log_file.write_text(output)
    log(f"  Agent output saved to {log_file}")


def process_issue(issue: dict, dry_run: bool = False):
    """Process a single issue end-to-end."""
    number = issue["number"]
    title = issue["title"]
    issue_type = determine_issue_type(issue)
    prefix = "fix" if issue_type == "bug" else "feat"
    slug = slugify(title)
    branch_name = f"{prefix}/issue-{number}-{slug}"

    log(f"Processing issue #{number}: {title}")
    log(f"  Type: {issue_type}, Branch: {branch_name}")

    if dry_run:
        log("  [DRY RUN] Would process this issue. Skipping.")
        return

    # Check retry count — reset if the issue was manually re-labeled to todo
    retries = get_retry_count(number)
    if retries >= MAX_RETRIES:
        # There are old failures but the issue is labeled todo again — user reset it
        log(f"  Retry counter reset (was {retries}) — user re-queued the issue")
        comment_on_issue(number, "[agent-reset] Retry counter reset — issue re-queued.")
        retries = 0

    # Mark in-progress
    set_label(number, "in-progress", "todo")

    # Create worktree
    try:
        worktree_path = create_worktree(number, branch_name)
    except Exception as e:
        log(f"  Failed to create worktree: {e}")
        set_label(number, "needs-attention", "in-progress")
        comment_on_issue(number, f"[agent-retry] Failed to set up worktree:\n```\n{e}\n```")
        return

    # Build prompt and run agent
    prompt = build_prompt(issue, branch_name)
    try:
        success, output = run_agent(worktree_path, prompt)
    except subprocess.TimeoutExpired:
        log(f"  Agent timed out after {MAX_TIMEOUT_SECONDS}s")
        success = False
        output = f"Agent timed out after {MAX_TIMEOUT_SECONDS} seconds"

    save_log(number, output)

    # Determine outcome
    if success and check_pr_created(number, branch_name):
        log(f"  Success — PR created for issue #{number}")
        set_label(number, "pr-ready", "in-progress")
    elif success:
        # Agent exited cleanly but no PR — might have commented a question
        log(f"  Agent finished but no PR found — checking for clarification")
        # Check if agent left a comment asking for clarification
        if "clarification" in output.lower() or "unsure" in output.lower():
            set_label(number, "needs-clarification", "in-progress")
        else:
            set_label(number, "needs-attention", "in-progress")
            comment_on_issue(number,
                f"[agent-retry] Agent completed but did not create a PR. "
                f"Check the logs for details.")
    else:
        log(f"  Agent failed for issue #{number}")
        retries += 1
        if retries >= MAX_RETRIES:
            set_label(number, "manual", "in-progress")
            # Truncate output for the comment (GitHub has a 65536 char limit)
            truncated = output[-3000:] if len(output) > 3000 else output
            comment_on_issue(number,
                f"[agent-retry] Failed {retries}/{MAX_RETRIES} times — marking as `manual`.\n\n"
                f"Last output:\n```\n{truncated}\n```")
        else:
            set_label(number, "todo", "in-progress")
            comment_on_issue(number,
                f"[agent-retry] Attempt {retries}/{MAX_RETRIES} failed. "
                f"Will retry on next poll.")

    # Cleanup worktree (keep the branch for PR)
    cleanup_worktree(number)


def ensure_labels_exist():
    """Create required labels if they don't exist on the repo."""
    required = {
        "todo": "0E8A16",
        "in-progress": "FBCA04",
        "pr-ready": "1D76DB",
        "needs-attention": "D93F0B",
        "needs-revision": "C5DEF5",
        "needs-clarification": "F9D0C4",
        "manual": "B60205",
    }
    result = run(["gh", "label", "list", "--json", "name", "--limit", "100"],
                 cwd=str(REPO_ROOT))
    existing = {l["name"] for l in json.loads(result.stdout)}

    for name, color in required.items():
        if name not in existing:
            log(f"  Creating label: {name}")
            run(["gh", "label", "create", name, "--color", color,
                 "--description", f"Agent workflow: {name}"],
                cwd=str(REPO_ROOT))


def main():
    parser = argparse.ArgumentParser(description="Process GitHub issues with Claude Code agent")
    parser.add_argument("--dry-run", action="store_true",
                        help="Show what would happen without executing")
    parser.add_argument("--issue", type=int,
                        help="Process a specific issue by number")
    args = parser.parse_args()

    global REPO
    REPO = get_repo_name()
    log(f"Repository: {REPO}")

    # Ensure labels exist
    ensure_labels_exist()

    if args.issue:
        # Process a specific issue
        issue = fetch_issue(args.issue)
        if not issue:
            log(f"Issue #{args.issue} not found")
            sys.exit(1)
        if issue["state"] != "OPEN":
            log(f"Issue #{args.issue} is not open (state: {issue['state']})")
            sys.exit(1)
        process_issue(issue, dry_run=args.dry_run)
        return

    # Poll mode: only one active agent issue at a time.
    # Skip if anything is in-progress or if a PR is awaiting review.
    # This prevents merge conflicts from multiple PRs branching off the same dev.
    if is_in_progress():
        log("An issue is already in-progress — skipping this run")
        return

    if has_pending_pr():
        log("A pr-ready issue is awaiting merge — skipping to avoid conflicts")
        return


    # Fetch todo issues
    issues = fetch_todo_issues()
    if not issues:
        log("No issues labeled 'todo' — nothing to do")
        return

    log(f"Found {len(issues)} todo issue(s)")

    # Process the oldest one
    process_issue(issues[0], dry_run=args.dry_run)


if __name__ == "__main__":
    main()
