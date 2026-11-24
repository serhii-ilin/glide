# Operating rules (read before the task)

You are a coding agent working on Glide, a small Rust desktop browser. An orchestrator
(Claude) wrote this task, will review your diff, re-run every gate itself, and merge.

1. Read `AGENTS.md` first and follow it. It is the source of truth for code style, tests,
   quality gates and the feature registry.
2. You are on branch `{{BRANCH}}` (based on `{{BASE}}`). Stay on it. Do not switch
   branches, rebase, merge, push, or open pull requests.
3. Stay in scope. If something outside the task needs changing, list it under
   OPEN ISSUES instead of doing it.
4. Write or update the tests first, then the code.
5. Shell commands: `cargo build`/`cargo nextest run` can take several minutes (V8, Stylo, Vello).
   Run them in the foreground with a long timeout (at least 900000 ms) and wait for the
   result. Do not leave them running in the background.
6. Before you finish, all of these must pass, and you must paste their final lines in the
   report:
   - `cargo fmt --check`
   - `cargo clippy --all-targets -- -D warnings`
   - `cargo nextest run --profile ci`
   - `cargo test --doc`
   - `cargo build --release` (only if the task changes user-facing behavior)
7. Update the registry slug files (`docs/features/<slug>.md`, `docs/requirements/<slug>.md`;
   choose a slug, never a number) for every behavior you add or change: status,
   requirement slugs, and the tests that cover it; run `node scripts/registry.mjs --check`. Update `README.md` tables when keys or
   features change.
   Fill the `mcp` field of every feature file you add or change (tool names,
   `planned <slug> (tool)` or `n/a (reason)`), following "Agent control (MCP)" in
   `AGENTS.md`. If the feature exposes state or an action an agent needs and `src/mcp/`
   exists, add or extend the tool with its `tests/mcp_stdio.rs` test; otherwise list it
   under OPEN ISSUES.
8. Do not weaken, delete or `#[ignore]` existing tests to make gates pass. Do not add
   `#[allow(...)]` to silence clippy unless the task says so; fix the code.
9. Commit on the branch in small logical commits with imperative subject lines. Leave the
   working tree clean.
10. End your final message with exactly this report:

```
STATUS: done | partial | blocked
SUMMARY: <what changed, 3-6 lines>
COMMITS: <git log --oneline {{BASE}}..HEAD>
GATES: <last line of each gate command>
DECISIONS: <choices you made that the task did not specify>
OPEN ISSUES: <anything left, or "none">
```

---

# Task
