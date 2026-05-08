# SkyCodeOS CLI Reference

SkyCodeOS ships the `scos` binary. Commands are local-first and operate on the
current working directory unless a command exposes an explicit repository path.

| Command | Args | Description | Exit codes |
|---|---|---|---|
| `scos ask` | `"<task>" [--profile <p>]` | Run full pipeline; print diff ID | `0=ok`, `1=error` |
| `scos approve` | `<diff_id>` | Sign approval token (`TTL=300s`) | `0=ok`, `1=invalid` |
| `scos apply` | `<diff_id> [--repo <path>]` | Validate + apply + log | `0=ok`, `1=error` |
| `scos rollback` | `<ref> [--repo <path>]` | `git checkout` to ref | `0=ok`, `1=git error` |
| `scos diff` | `<path> <old> <new>` | Create `DiffProposal` from unified diff | `0=ok` |
| `scos scan` | `<path> [--force]` | Index project; `--force` clears graph first | `0=ok` |
| `scos graph impact` | `<symbol> [--project-id <p>] [--max-depth <n>]` | Show impact nodes | `0=ok` |
| `scos model load` | `<name>` | Load model from registry | `0=ok`, `1=not found` |
| `scos model verify` | `<name>` | Check model health | `0=ok`, `1=unhealthy` |
| `scos model bench` | `<name>` | Run timing benchmark | `0=ok` |
| `scos profile use` | `<name>` | Set active profile | `0=ok`, `1=invalid` |
| `scos profile show` | none | Print current profile | `0=ok` |
| `scos profile bench` | `"<task>" [--profile <p>] [--model <m>]` | Timed task run -> `tuning_run` | `0=ok` |
| `scos profile compare` | `<run_id1> <run_id2>` | Side-by-side comparison | `0=ok`, `1=not found` |
| `scos profile tune` | `[--model <m>] [--profile <p>]` | Benchmark sweep | `0=ok` |
| `scos profile export-results` | `[--format csv|json] [--limit N]` | Dump `tuning_runs` | `0=ok` |

All file writes remain gated by the approval-token pipeline. `ask`, `profile
bench`, and `profile tune` may create stored diff proposals, but they do not
apply patches.
