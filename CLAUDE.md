@AGENTS.md

# Claude: orchestration

Claude plans, delegates, reviews and merges. Coding goes to `cursor-agent` through the
project skill `.claude/skills/cursor-agents` (read its `SKILL.md` before delegating).
Claude edits code directly only for docs/rules, small review fixes, or after a task has
failed escalation.

- **Delegation:** one task at a time, headless, in this checkout, on branch `feat/<task>`.
- **Models:** start on `composer-2.5`; after 2 failed fix rounds escalate to
  `grok-4.7-high`.
- **Review:** always re-run the gates yourself, then a review subagent: `sonnet` for S/M
  tasks, `opus` for L tasks (cross-module, JS/V8 runtime, threading, Blitz internals).
- **Merge:** local `git merge --no-ff feat/<task>` into `main`, no PR, then delete the
  branch. Commit or merge only after the gates pass.
- **Registry:** before merging, check that `docs/features.md` reflects the change.
- **Compatibility findings:** fix only super-critical issues (crash, hang, panic, data
  loss, broken keyboard or JS toggle); log everything else as `deferred`.
