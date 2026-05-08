# Tool System and Safety

## Required Tools (V1)

- read_file
- list_dir
- search_project
- create_diff
- apply_diff
- rollback
- git_status

## Safety Rules

- No write path without approval token.
- Every tool action is logged.
- Risky actions require protected branch workflow.

## Safe Edit Flow

1. Propose change
2. Generate diff
3. Request approval
4. Apply patch
5. Verify
6. Log decision and tool events
