---
name: cursor-agents
description: Delegate Glide coding tasks to headless cursor-agent runs (composer-2.5 by default, grok-4.7-high on escalation), one task at a time on its own branch in the main checkout, then review with a Sonnet/Opus subagent, re-run the gates and merge --no-ff into main. Use when implementing any task from docs/features.md or when the user asks to delegate to Cursor.
---

# cursor agents (Glide)

Claude is the **orchestrator**. It writes the task prompt, launches `cursor-agent`,
reviews the diff, re-runs the gates, sends fix rounds and merges. The agent only writes
code on its branch. Runs are **sequential in the main checkout** (no worktrees).

`SKILL_DIR=.claude/skills/cursor-agents`

| File | Purpose |
| --- | --- |
| `scripts/ca-run.sh` | Check clean tree + lock, create/switch branch, run one headless agent, log stream-json, print summary |
| `scripts/ca-status.py` | Summarize a run log: chat id, state, tools, failed shell commands, edited files, tokens, last texts |
| `templates/preamble.md` | Operating rules prepended to every new task prompt |

## Model routing

| Step | Model |
| --- | --- |
| First run of any task | `composer-2.5` (default) |
| Fix rounds 1–2 | same chat, `-r <chat-id>` |
| After 2 failed fix rounds | fresh run with `-m grok-4.7-high` (new chat, preamble re-sent), or Claude fixes it directly |
| Review of S/M tasks | Agent subagent, `model: sonnet` |
| Review of L tasks | Agent subagent, `model: opus` |

## Verified facts (cursor-agent, macOS, 2026-11)

- Headless: `cursor-agent -p --force --trust --output-format stream-json --model <m> "<prompt>"`.
  `--force` auto-approves tools, `--trust` skips the workspace trust prompt.
- Every event carries `session_id`; that is the chat id for `--resume <id>`.
- Events: `system/init` (`model`), `user`, `thinking` deltas, `tool_call` `started`/`completed`
  with `tool_call.editToolCall` / `tool_call.shellToolCall` (`args.command`,
  `result.success.exitCode`), `assistant` (`message.content[].text`), and a final `result`
  (`subtype`, `is_error`, `duration_ms`, `usage`).
- The shell tool's default timeout is 30 s with `TIMEOUT_BEHAVIOR_BACKGROUND`, so long
  `cargo` commands get backgrounded unless the agent passes a long timeout. The preamble
  tells it to.
- macOS has no `timeout`; the script uses `perl -e 'alarm N; exec @ARGV'` (exit 142).
- `cursor-agent models` lists available models.

## Workflow

1. **Pick the task** from `docs/features.md` (or the user). Set its row to `in-progress`
   on `main` only if the user wants that tracked separately; otherwise the agent does it.
2. **Write the prompt** in the scratchpad (never inside the repo): goal, out of scope,
   files and existing functions to reuse (paths), tests to write first, acceptance
   criteria, the registry rows to update, and the feature's `MCP` exposure (tool to add or
   extend, or why it is `n/a`). The agent has no conversation context.
3. **Launch** as one background Bash call (`run_in_background: true`):
   ```bash
   .claude/skills/cursor-agents/scripts/ca-run.sh -n feat/<task> -p <scratchpad>/<task>.md
   ```
   Logs: `~/.claude/cursor-runs/glide/<branch-slug>/<stamp>.jsonl` (+ `latest.jsonl`, `.exit`).
   Do not edit the checkout while a run is live. You are notified when it exits.
4. **Progress** (read-only): `python3 .claude/skills/cursor-agents/scripts/ca-status.py ~/.claude/cursor-runs/glide/<slug> --texts 5`
5. **Review** every finished branch; do not trust the report:
   - `git log --oneline main..HEAD`, `git diff main...HEAD --stat`, read the diff;
   - re-run all gates yourself: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`
     (+ `cargo build --release` for user-facing changes);
   - spawn the review subagent (Sonnet or Opus per routing) with the diff range and the
     task's acceptance criteria; ask for correctness bugs, scope creep, weakened tests,
     missing registry update, and an `MCP` column / tool that ignores the
     "Agent control (MCP)" rules in `AGENTS.md`;
   - loop new tests 10× to catch flakes (`for i in $(seq 10); do cargo test <name> -q || break; done`).
6. **Fix rounds:** write concrete findings to `<task>-fix<N>.md` and run
   `ca-run.sh -n feat/<task> -p <fix-file> -r <chat-id>`. Escalate per the routing table.
7. **Merge** (user's policy: local, no PR):
   ```bash
   git switch main && git merge --no-ff feat/<task> -m "Merge feat/<task>: <summary>" && git branch -d feat/<task>
   ```
   Confirm the registry row is `done` (or `deferred` with a reason) on `main`.
8. **Report** to the user: what the agent did, gates you verified, review findings and
   fixes, rounds/model used, anything deferred.

## Gotchas

- `ca-run.sh` refuses to start on a dirty tree or while another run holds the lock.
- **Kill hung child processes by exact PID only.** `cursor-agent`'s command line contains the
  whole prompt text, so `pkill -f <pattern>` with any path or name mentioned in the prompt can
  match, and kill, the agent itself. Find the PID with `ps -eo pid,etime,command` and `kill <pid>`.
- **Hung commands stall the run.** Cursor's shell waits up to 24 h on a command. If a run goes
  quiet with a child at 100% CPU, kill that child by PID; the agent sees the failure and continues.
  Prompts for harness-style work should require hard per-script timeouts and an outer
  `perl -e 'alarm N; exec @ARGV'` limit.
- **Resuming after an interruption:** commit the WIP on the branch (the tree must be clean), then
  `ca-run.sh -n <branch> -p <resume-prompt> -r <chat-id>`.
- **Stale release binaries:** tests and runners that use `target/release/glide` can pick up a
  binary built on another branch. Rebuild before trusting a surprising result.
- An agent can claim gates passed when they did not. Step 5 is mandatory.
- If a run times out (142), check `git status` on the branch: partial uncommitted work may
  need to be committed or discarded before a fix round.
- `Cargo.lock` changes are expected when a task adds a dependency; check the dep is justified.
