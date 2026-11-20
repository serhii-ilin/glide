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
5. Shell commands: `cargo build`/`cargo test` can take several minutes (V8, Stylo, Vello).
   Run them in the foreground with a long timeout (at least 900000 ms) and wait for the
   result. Do not leave them running in the background.
6. Before you finish, all of these must pass, and you must paste their final lines in the
   report:
   - `cargo fmt --check`
   - `cargo clippy --all-targets -- -D warnings`
   - `cargo test`
   - `cargo build --release` (only if the task changes user-facing behavior)
7. Update `docs/features.md` (registry) for every behavior you add or change: row status,
   requirement IDs, and the tests that cover it. Update `README.md` tables when keys or
   features change.
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
