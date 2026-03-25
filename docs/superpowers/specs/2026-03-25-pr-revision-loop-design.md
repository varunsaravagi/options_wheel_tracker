# PR Revision Loop for Agentic Issue Processor

**Date**: 2026-03-25
**Status**: Approved

## Problem

The agentic issue processor (`scripts/process_issues.py`) can create PRs from GitHub issues, but has no mechanism for the human reviewer to request changes and have the agent revise the PR. Once a PR is created and labeled `pr-ready`, the only options are to merge it as-is, manually fix it, or close and re-queue the entire issue from scratch.

## Design

### Labels

| Label | Meaning |
|-------|---------|
| `todo` | Queued for agent pickup |
| `in-progress` | Agent is actively working (implementation or revision) |
| `pr-ready` | PR open, waiting for human review |
| `needs-revision` | Human left PR feedback, agent should revise |
| `needs-attention` | Agent finished but something unexpected happened |
| `needs-clarification` | Agent needs human input on the issue |
| `manual` | Too many failures, human must intervene |

The only new label is `needs-revision`. All others already exist.

### Poll Priority Order

Each cron run (every 30 minutes) evaluates in this order:

1. **Any `in-progress`?** — skip entirely (agent already working)
2. **Any `needs-revision`?** — pick it up (priority over new work)
3. **Any `pr-ready`?** — skip entirely (blocks new work until human reviews)
4. **Pick oldest `todo`** — start fresh implementation

This ordering ensures one issue is fully finalized (merged or marked `manual`) before the agent starts new work, preventing merge conflicts from concurrent branches off `dev`.

**Invariant**: At most one issue should be in `pr-ready` or `needs-revision` state at any time. If multiple are found due to manual label manipulation, the agent processes the oldest `needs-revision` issue and ignores others.

### State Transitions

```
[todo] ───────────────────► [in-progress] ──┬──► [pr-ready] ◄─────────┐
  ▲                         (implementation) │        │                │
  │                                          ├──► [needs-attention]    │
  │                                          ├──► [needs-clarification]│
  │  (retry, attempts < 3)                   ├──► [todo]               │
  │◄─────────────────────────────────────────┘                         │
  │                                          └──► [manual]             │
  │                                            (3 impl failures)       │
  │                                                                    │
  │  (user re-labels)                                                  │
  └◄──── [manual] ◄────────────────┐   human merges ──► [closed]      │
              │                     │                                   │
              │ (user re-labels     │          human applies label      │
              │  for revision)      │   [pr-ready] ───► [needs-revision]│
              │                     │                        │          │
              └──► [needs-revision] │                        ▼          │
                        │           │  ┌──────────────────────────┐     │
                        ▼           │  │     [in-progress]        │     │
                  [in-progress] ────┘  │      (revision)          │     │
                   (revision)          └──────┬─────┬─────────────┘     │
                                              │     └───────────────────┘
                                              │       (revision success)
                                              ▼
                                          [manual]
                                    (2 revision failures)
```

### Agent Context by Workflow

| Trigger | Agent reads |
|---------|------------|
| `todo` → `in-progress` (fresh or re-queued) | Issue body + all issue comments |
| `needs-revision` → `in-progress` | Issue body + PR comments on the existing PR |

For fresh implementations and re-queued issues (`manual` → `todo`), the agent always reads the full issue comment history. This ensures human feedback left on the issue (e.g., guidance when re-queuing a `manual` issue) is seen by the agent.

For revisions, the agent reads PR comments only. This is where the human leaves revision feedback, and avoids noise from old agent retry logs on the issue.

### Revision Mechanics

**Branch reuse**: The revision workflow reuses the existing branch and PR. The existing `create_worktree()` is refactored with a `revision` flag — when `True`, it skips the branch delete and creates the worktree on the existing branch (`git worktree add <path> <branch>`, no `-b`). The agent reads the PR comments for feedback, makes changes, pushes, and the PR updates automatically.

**PR/branch lookup**: The agent finds the existing PR by querying GitHub (`gh pr list --search "issue-{number}"` filtering by branch prefix) rather than reconstructing the branch name. This is robust against issue title edits after the original PR was created.

**Missing PR handling**: If the revision agent succeeds but the PR has been closed or is missing (e.g., human closed it between applying `needs-revision` and the agent running), the issue is labeled `needs-attention` with a comment explaining the PR was not found.

**Retry tracking**: Revision attempts are tracked separately from implementation attempts using `[revision-retry]` comment markers on the issue (distinct from `[agent-retry]` markers). A `[revision-reset]` marker zeros the revision counter, analogous to the existing `[agent-reset]` mechanism. The reset is posted automatically when the agent picks up a `needs-revision` issue whose revision retry count has reached the limit — indicating the human re-applied the label after a `manual` state.

**Retry limit**: 2 revision attempts. After 2 failed revision attempts, the issue is labeled `manual`. This is lower than the 3-attempt implementation limit because revision failures likely indicate the feedback is too complex for the agent.

**On success**: The agent pushes to the existing branch and the issue is labeled back to `pr-ready` for another round of human review.

### Human Workflows

| Goal | Action |
|------|--------|
| Request changes on a PR | Leave comments on the PR, then apply `needs-revision` label to the issue |
| Approve and merge | Merge the PR (issue auto-closes via `Closes #N` in PR body) |
| Re-queue a failed issue with feedback | Comment on the issue with guidance, change label from `manual` to `todo` |
| Re-queue a failed revision | Change label from `manual` to `needs-revision` |
| Pause the agent on an issue | Apply `needs-attention` or `needs-clarification` label |

## Changes Required

### `scripts/process_issues.py`

1. **New label**: Add `needs-revision` to `ensure_labels_exist()` with a distinct color.
2. **Poll priority**: Update `main()` to check for `needs-revision` issues before checking `todo`. The `needs-revision` check takes priority over the `pr-ready` gate — a `needs-revision` issue should be processed even though a PR exists.
3. **Revision detection**: New function `fetch_needs_revision_issue()` to find issues labeled `needs-revision`.
4. **Worktree creation refactor**: Add a `revision=False` parameter to `create_worktree()`. When `revision=True`, skip the branch delete and create the worktree on the existing branch (`git worktree add <path> <branch>`, no `-b` flag). When `revision=False`, keep the current behavior (delete branch, create new one from `origin/dev`).
5. **PR/branch lookup**: New function `find_issue_pr()` that queries GitHub for the open PR associated with an issue (e.g., `gh pr list` filtering by branch prefix `*/issue-{number}-*`). Returns the PR number, branch name, and URL. Do not reconstruct the branch name from the issue title.
6. **Revision processing**: New function `process_revision()` that:
   - Calls `find_issue_pr()` to locate the existing PR and branch
   - If PR is missing/closed: labels `needs-attention`, comments, and returns
   - Creates a revision worktree from the existing branch
   - Fetches PR comments and builds a revision-specific prompt
   - Runs the agent
   - On success: labels `pr-ready`, removes `needs-revision`
   - On failure: increments `[revision-retry]` counter, labels `manual` after 2 failures
7. **Revision retry tracking**: New function `get_revision_retry_count()` using `[revision-retry]` / `[revision-reset]` markers.
8. **Issue comments in prompt**: Update `build_prompt()` to fetch and include issue comments for implementation runs.
9. **`--issue` flag dispatch**: Update the `--issue` code path to detect the issue's current label and dispatch to `process_revision()` if labeled `needs-revision`, or `process_issue()` otherwise.
10. **Module docstring**: Update the label state machine documentation at the top of the file to include `needs-revision` and the revision workflow.

### `scripts/issue-prompt-template.md`

11. **Add comments section**: Include a `{comments}` placeholder for issue comments.

### New file: `scripts/revision-prompt-template.md`

12. **Revision prompt template**: A separate template for revision runs that includes PR comments/feedback and instructs the agent to address them on the existing branch. PR comments should include regular comments and review comments (with file path and line number for inline review comments), filtered to exclude bot/automated comments. Format each comment with author and timestamp.

### `scripts/setup-cron.sh`

13. **Add `needs-revision` label** to any label verification logic if present.

### Log file naming

14. **Distinguish revision logs**: Revision runs should be saved as `issue-{number}-revision-{timestamp}.log` to distinguish them from implementation logs (`issue-{number}-{timestamp}.log`).
