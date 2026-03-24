# Issue #{number}: {title}

**Labels**: {labels}
**Type**: {issue_type}

## Description

{body}

## Instructions

You are an autonomous agent working on the Options Wheel Tracker project.
Read CLAUDE.md at the project root for architecture, conventions, and rules.

### Your workflow

1. **Understand the issue** — read the description carefully. Read relevant source files before changing anything.
2. **Create a branch** — `{branch_name}` (already created for you, you are on it).
3. **Implement the fix or feature** with minimal, focused changes.
4. **Write tests** — regression test for bugs, feature tests for new model/handler logic.
5. **Verify your work**:
   - If you changed backend code: run `cargo check` and `cargo test` in `backend/`
   - If you changed frontend code: run `npm run build` in `frontend/`
   - If you changed migrations: run `scripts/test-migration.sh`
6. **Commit your changes** with a descriptive message referencing the issue.
7. **Push the branch** and **open a PR** targeting `dev` with `Closes #{number}` in the body.

### Constraints

- Do NOT modify more than 5 files unless the issue explicitly requires it.
- Do NOT delete any existing files.
- Do NOT install new dependencies (modify Cargo.toml or package.json) unless the issue explicitly requests it.
- Do NOT modify: Makefile, .env*, next.config.ts, .claude/ settings files.
- If the change requires more than ~200 lines of new code, stop and comment on the issue asking for guidance.
- If you are unsure about the approach or need clarification, comment on the issue explaining what you need, then stop.

### On failure

- If tests fail after 2 fix attempts, stop and comment the error output on the issue.
- If you cannot understand or reproduce the issue, comment on the issue explaining what you found.
