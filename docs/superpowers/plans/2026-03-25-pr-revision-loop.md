# PR Revision Loop Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `needs-revision` label workflow to the agentic issue processor so the human reviewer can request PR changes and the agent revises automatically.

**Architecture:** Extends `scripts/process_issues.py` with revision-aware poll priority, a `process_revision()` function that reuses the existing branch/PR, and a separate revision prompt template. Refactors `create_worktree()` to support checking out existing branches.

**Tech Stack:** Python 3, `gh` CLI, git worktrees, Claude Code headless mode

**Spec:** `docs/superpowers/specs/2026-03-25-pr-revision-loop-design.md`

---

### Task 1: Update module docstring and add `needs-revision` label

**Files:**
- Modify: `scripts/process_issues.py:1-23` (module docstring)
- Modify: `scripts/process_issues.py:412-431` (`ensure_labels_exist`)

- [ ] **Step 1: Update the module docstring**

Replace the label state machine section (lines 9-15) with the updated version:

```python
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
```

- [ ] **Step 2: Add `needs-revision` to `ensure_labels_exist()`**

Add to the `required` dict in `ensure_labels_exist()`:

```python
"needs-revision": "C5DEF5",
```

- [ ] **Step 3: Add `MAX_REVISION_RETRIES` constant**

Add after the existing `MAX_RETRIES = 3` line (line 48):

```python
MAX_REVISION_RETRIES = 2
```

- [ ] **Step 4: Add `REVISION_PROMPT_TEMPLATE` path constant**

Add after the existing `PROMPT_TEMPLATE` line (line 43):

```python
REVISION_PROMPT_TEMPLATE = REPO_ROOT / "scripts" / "revision-prompt-template.md"
```

- [ ] **Step 5: Run a quick syntax check**

Run: `python3 -c "import ast; ast.parse(open('scripts/process_issues.py').read()); print('OK')"`
Expected: `OK`

- [ ] **Step 6: Commit**

```bash
git add scripts/process_issues.py
git commit -m "feat: add needs-revision label and update docstring"
```

---

### Task 2: Add `fetch_needs_revision_issue()` and `find_issue_pr()`

**Files:**
- Modify: `scripts/process_issues.py` (add two new functions after `has_pending_pr()`)

- [ ] **Step 1: Add `fetch_needs_revision_issue()`**

Add after `has_pending_pr()` (after line 145):

```python
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
```

- [ ] **Step 2: Add `find_issue_pr()`**

Add after `fetch_needs_revision_issue()`:

```python
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
```

- [ ] **Step 3: Run syntax check**

Run: `python3 -c "import ast; ast.parse(open('scripts/process_issues.py').read()); print('OK')"`
Expected: `OK`

- [ ] **Step 4: Commit**

```bash
git add scripts/process_issues.py
git commit -m "feat: add fetch_needs_revision_issue and find_issue_pr functions"
```

---

### Task 3: Add `get_revision_retry_count()` and `fetch_issue_comments()`

**Files:**
- Modify: `scripts/process_issues.py` (add two new functions after `get_retry_count()`)

- [ ] **Step 1: Add `get_revision_retry_count()`**

Add after `get_retry_count()` (after line 188):

```python
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
```

- [ ] **Step 2: Add `fetch_issue_comments()`**

Add after `get_revision_retry_count()`:

```python
def fetch_issue_comments(issue_number: int) -> str:
    """Fetch all comments on an issue, formatted for inclusion in prompts."""
    result = run([
        "gh", "issue", "view", str(issue_number),
        "--json", "comments",
        "-q", '.comments[] | "**\(.author.login)** (\(.createdAt)):\n\(.body)\n"'
    ], cwd=str(REPO_ROOT), check=False)
    if result.returncode != 0 or not result.stdout.strip():
        return "(no comments)"
    return result.stdout.strip()
```

- [ ] **Step 3: Run syntax check**

Run: `python3 -c "import ast; ast.parse(open('scripts/process_issues.py').read()); print('OK')"`
Expected: `OK`

- [ ] **Step 4: Commit**

```bash
git add scripts/process_issues.py
git commit -m "feat: add revision retry counting and issue comment fetching"
```

---

### Task 4: Add `fetch_pr_comments()`

**Files:**
- Modify: `scripts/process_issues.py` (add new function after `fetch_issue_comments()`)

- [ ] **Step 1: Add `fetch_pr_comments()`**

This function fetches both regular PR comments and review comments, excludes bot comments, and formats them with author, timestamp, and file/line context for review comments.

```python
def fetch_pr_comments(pr_number: int) -> str:
    """Fetch PR comments (regular + review) for revision prompts.

    Excludes bot comments. Review comments include file path and line number.
    """
    comments = []

    # Regular PR comments (conversation)
    result = run([
        "gh", "api", f"repos/{REPO}/issues/{pr_number}/comments",
        "--jq", '.[] | select(.user.type != "Bot") | "**\(.user.login)** (\(.created_at)):\n\(.body)\n"'
    ], cwd=str(REPO_ROOT), check=False)
    if result.returncode == 0 and result.stdout.strip():
        comments.append("### PR Comments\n")
        comments.append(result.stdout.strip())

    # Review comments (inline on code)
    result = run([
        "gh", "api", f"repos/{REPO}/pulls/{pr_number}/comments",
        "--jq", '.[] | select(.user.type != "Bot") | "**\(.user.login)** (\(.created_at)) on `\(.path):\(.line // .original_line)`:\n\(.body)\n"'
    ], cwd=str(REPO_ROOT), check=False)
    if result.returncode == 0 and result.stdout.strip():
        comments.append("\n### Inline Review Comments\n")
        comments.append(result.stdout.strip())

    return "\n".join(comments) if comments else "(no PR comments)"
```

- [ ] **Step 2: Run syntax check**

Run: `python3 -c "import ast; ast.parse(open('scripts/process_issues.py').read()); print('OK')"`
Expected: `OK`

- [ ] **Step 3: Commit**

```bash
git add scripts/process_issues.py
git commit -m "feat: add fetch_pr_comments for revision workflow"
```

---

### Task 5: Refactor `create_worktree()` with `revision` parameter

**Files:**
- Modify: `scripts/process_issues.py:231-270` (`create_worktree`)

- [ ] **Step 1: Refactor `create_worktree()`**

Replace the existing `create_worktree()` function with:

```python
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
```

- [ ] **Step 2: Run syntax check**

Run: `python3 -c "import ast; ast.parse(open('scripts/process_issues.py').read()); print('OK')"`
Expected: `OK`

- [ ] **Step 3: Commit**

```bash
git add scripts/process_issues.py
git commit -m "refactor: add revision parameter to create_worktree"
```

---

### Task 6: Update `save_log()` to distinguish revision logs

**Files:**
- Modify: `scripts/process_issues.py:321-327` (`save_log`)

- [ ] **Step 1: Add `revision` parameter to `save_log()`**

Replace the existing `save_log()` function:

```python
def save_log(issue_number: int, output: str, revision: bool = False):
    """Save agent output to a log file."""
    LOG_DIR.mkdir(parents=True, exist_ok=True)
    ts = time.strftime("%Y%m%d-%H%M%S")
    kind = "revision" if revision else "impl"
    log_file = LOG_DIR / f"issue-{issue_number}-{kind}-{ts}.log"
    log_file.write_text(output)
    log(f"  Agent output saved to {log_file}")
```

- [ ] **Step 2: Run syntax check**

Run: `python3 -c "import ast; ast.parse(open('scripts/process_issues.py').read()); print('OK')"`
Expected: `OK`

- [ ] **Step 3: Commit**

```bash
git add scripts/process_issues.py
git commit -m "feat: distinguish revision vs implementation log files"
```

---

### Task 7: Update `build_prompt()` to include issue comments

**Files:**
- Modify: `scripts/process_issues.py:214-228` (`build_prompt`)
- Modify: `scripts/issue-prompt-template.md`

- [ ] **Step 1: Add `{comments}` section to the issue prompt template**

Append before the `## Instructions` line in `scripts/issue-prompt-template.md`:

```markdown
## Comments

{comments}
```

So the full template becomes:

```markdown
# Issue #{number}: {title}

**Labels**: {labels}
**Type**: {issue_type}

## Description

{body}

## Comments

{comments}

## Instructions
...
```

- [ ] **Step 2: Update `build_prompt()` to fetch and include comments**

Replace the existing `build_prompt()` function. Uses `.replace()` instead of `.format()` to avoid crashes when comments/body contain curly braces (JSON, Rust code, etc.):

```python
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
```

- [ ] **Step 3: Run syntax check**

Run: `python3 -c "import ast; ast.parse(open('scripts/process_issues.py').read()); print('OK')"`
Expected: `OK`

- [ ] **Step 4: Commit**

```bash
git add scripts/process_issues.py scripts/issue-prompt-template.md
git commit -m "feat: include issue comments in agent prompt"
```

---

### Task 8: Create revision prompt template

**Files:**
- Create: `scripts/revision-prompt-template.md`

- [ ] **Step 1: Create the revision prompt template**

Create `scripts/revision-prompt-template.md`:

```markdown
# Revision Request — Issue #{number}: {title}

**PR**: #{pr_number} ({pr_url})
**Branch**: `{branch_name}`

## Original Issue

{body}

## PR Feedback to Address

{pr_comments}

## Instructions

You are an autonomous agent revising an existing PR on the Options Wheel Tracker project.
Read CLAUDE.md at the project root for architecture, conventions, and rules.

### Your workflow

1. **Read the feedback above** — understand exactly what the reviewer wants changed.
2. **You are already on the branch** — `{branch_name}`. Do NOT create a new branch.
3. **Read the relevant files** before making changes. Understand the existing code.
4. **Make the requested changes** — address each piece of feedback. Keep changes minimal and focused.
5. **Verify your work**:
   - If you changed backend code: run `cargo check` and `cargo test` in `backend/`
   - If you changed frontend code: run `npm run build` in `frontend/`
   - If you changed migrations: run `scripts/test-migration.sh`
6. **Commit your changes** with a message like `fix: address PR feedback for #{number}`.
7. **Push the branch** — `git push origin {branch_name}`. Do NOT create a new PR.

### Constraints

- Do NOT create a new branch — stay on `{branch_name}`.
- Do NOT open a new PR — push to the existing branch to update the existing PR.
- Do NOT modify more than 5 files unless the feedback explicitly requires it.
- Do NOT delete any existing files.
- Do NOT install new dependencies unless the feedback explicitly requests it.
- Do NOT modify: Makefile, .env*, next.config.ts, .claude/ settings files.
- If the feedback is unclear or contradictory, comment on the PR asking for clarification, then stop.

### On failure

- If tests fail after 2 fix attempts, stop and comment the error output on the PR.
- If you cannot understand the feedback, comment on the PR explaining what you need.
```

- [ ] **Step 2: Commit**

```bash
git add scripts/revision-prompt-template.md
git commit -m "feat: add revision prompt template for PR feedback workflow"
```

---

### Task 9: Add `build_revision_prompt()` and `process_revision()`

**Files:**
- Modify: `scripts/process_issues.py` (add two new functions)

- [ ] **Step 1: Add `build_revision_prompt()`**

Add after `build_prompt()`:

```python
def build_revision_prompt(issue: dict, pr: dict) -> str:
    """Build the agent prompt for a PR revision.

    Uses .replace() instead of .format() to avoid crashes when PR comments
    or issue body contain curly braces (JSON, Rust code, etc.).
    """
    template = REVISION_PROMPT_TEMPLATE.read_text()
    pr_comments = fetch_pr_comments(pr["number"])

    prompt = (template
        .replace("{number}", str(issue["number"]))
        .replace("{title}", issue["title"])
        .replace("{body}", issue.get("body", "(no description)") or "(no description)")
        .replace("{pr_number}", str(pr["number"]))
        .replace("{pr_url}", pr["url"])
        .replace("{branch_name}", pr["headRefName"])
        .replace("{pr_comments}", pr_comments))
    return prompt
```

- [ ] **Step 2: Add `process_revision()`**

Add after `process_issue()`:

```python
def process_revision(issue: dict, dry_run: bool = False):
    """Process a revision request for an existing PR."""
    number = issue["number"]
    title = issue["title"]

    log(f"Processing revision for issue #{number}: {title}")

    # Find the existing PR
    pr = find_issue_pr(number)
    if not pr:
        log(f"  No open PR found for issue #{number}")
        set_label(number, "needs-attention", "needs-revision")
        comment_on_issue(number,
            "[revision-retry] No open PR found for this issue. "
            "The PR may have been closed. Marking as needs-attention.")
        return

    branch_name = pr["headRefName"]
    log(f"  Found PR #{pr['number']} on branch {branch_name}")

    if dry_run:
        log("  [DRY RUN] Would process this revision. Skipping.")
        return

    # Check revision retry count — reset if re-queued from manual
    retries = get_revision_retry_count(number)
    if retries >= MAX_REVISION_RETRIES:
        log(f"  Revision retry counter reset (was {retries}) — user re-queued")
        comment_on_issue(number, "[revision-reset] Revision retry counter reset — re-queued.")
        retries = 0

    # Mark in-progress
    set_label(number, "in-progress", "needs-revision")

    # Create worktree on existing branch
    try:
        worktree_path = create_worktree(number, branch_name, revision=True)
    except Exception as e:
        log(f"  Failed to create revision worktree: {e}")
        set_label(number, "needs-attention", "in-progress")
        comment_on_issue(number,
            f"[revision-retry] Failed to set up revision worktree:\n```\n{e}\n```")
        return

    # Build revision prompt and run agent
    prompt = build_revision_prompt(issue, pr)
    try:
        success, output = run_agent(worktree_path, prompt)
    except subprocess.TimeoutExpired:
        log(f"  Agent timed out after {MAX_TIMEOUT_SECONDS}s")
        success = False
        output = f"Agent timed out after {MAX_TIMEOUT_SECONDS} seconds"

    save_log(number, output, revision=True)

    # Determine outcome
    if success:
        # Verify PR still exists
        if find_issue_pr(number):
            log(f"  Revision successful for issue #{number}")
            set_label(number, "pr-ready", "in-progress")
        else:
            log(f"  Revision completed but PR is missing")
            set_label(number, "needs-attention", "in-progress")
            comment_on_issue(number,
                "[revision-retry] Revision completed but the PR appears to be "
                "closed or missing.")
    else:
        log(f"  Revision failed for issue #{number}")
        retries += 1
        if retries >= MAX_REVISION_RETRIES:
            set_label(number, "manual", "in-progress")
            truncated = output[-3000:] if len(output) > 3000 else output
            comment_on_issue(number,
                f"[revision-retry] Revision failed {retries}/{MAX_REVISION_RETRIES} "
                f"times — marking as `manual`.\n\n"
                f"Last output:\n```\n{truncated}\n```")
        else:
            set_label(number, "needs-revision", "in-progress")
            comment_on_issue(number,
                f"[revision-retry] Revision attempt {retries}/{MAX_REVISION_RETRIES} "
                f"failed. Will retry on next poll.")

    # Cleanup worktree (keep the branch for PR)
    cleanup_worktree(number)
```

- [ ] **Step 3: Run syntax check**

Run: `python3 -c "import ast; ast.parse(open('scripts/process_issues.py').read()); print('OK')"`
Expected: `OK`

- [ ] **Step 4: Commit**

```bash
git add scripts/process_issues.py
git commit -m "feat: add build_revision_prompt and process_revision functions"
```

---

### Task 10: Update `main()` poll logic and `--issue` dispatch

**Files:**
- Modify: `scripts/process_issues.py:434-486` (`main`)

- [ ] **Step 1: Update `main()` with new poll priority and `--issue` dispatch**

Replace the existing `main()` function:

```python
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
        # Process a specific issue — detect label to dispatch correctly
        issue = fetch_issue(args.issue)
        if not issue:
            log(f"Issue #{args.issue} not found")
            sys.exit(1)
        if issue["state"] != "OPEN":
            log(f"Issue #{args.issue} is not open (state: {issue['state']})")
            sys.exit(1)
        label_names = [l["name"] for l in issue.get("labels", [])]
        if "needs-revision" in label_names:
            process_revision(issue, dry_run=args.dry_run)
        else:
            process_issue(issue, dry_run=args.dry_run)
        return

    # Poll mode: only one active agent issue at a time.
    # Priority: in-progress (skip) > needs-revision > pr-ready (skip) > todo
    if is_in_progress():
        log("An issue is already in-progress — skipping this run")
        return

    # Check for revision requests (priority over new work)
    revision_issue = fetch_needs_revision_issue()
    if revision_issue:
        log(f"Found revision request: issue #{revision_issue['number']}")
        process_revision(revision_issue, dry_run=args.dry_run)
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
```

- [ ] **Step 2: Run syntax check**

Run: `python3 -c "import ast; ast.parse(open('scripts/process_issues.py').read()); print('OK')"`
Expected: `OK`

- [ ] **Step 3: Commit**

```bash
git add scripts/process_issues.py
git commit -m "feat: update poll priority for needs-revision and --issue dispatch"
```

---

### Task 11: End-to-end dry-run test

**Files:**
- No files modified — validation only

- [ ] **Step 1: Run full syntax check**

Run: `python3 -c "import ast; ast.parse(open('scripts/process_issues.py').read()); print('OK')"`
Expected: `OK`

- [ ] **Step 2: Verify dry-run still works**

Run: `python3 scripts/process_issues.py --dry-run`
Expected: Script runs without errors. Should print "No issues labeled 'todo' — nothing to do" or process an issue in dry-run mode.

- [ ] **Step 3: Verify the revision prompt template renders**

Run: `python3 -c "from pathlib import Path; t = Path('scripts/revision-prompt-template.md').read_text(); print(t.format(number=1, title='test', body='desc', pr_number=10, pr_url='http://example.com', branch_name='feat/test', pr_comments='feedback here')[:200])"`
Expected: Prints the first 200 chars of a rendered revision prompt without errors.

- [ ] **Step 4: Verify the issue prompt template still renders with comments**

Run: `python3 -c "from pathlib import Path; t = Path('scripts/issue-prompt-template.md').read_text(); print(t.format(number=1, title='test', labels='todo', issue_type='feature', body='desc', branch_name='feat/test', comments='some comments')[:200])"`
Expected: Prints the first 200 chars of a rendered issue prompt without errors.

- [ ] **Step 5: Final commit if any fixes were needed**

Only if previous steps required fixes. Otherwise skip.
