#!/usr/bin/env bash
# Run one headless cursor-agent task in the main checkout, on its own branch.
#
# Usage:
#   ca-run.sh -n <branch> -p <prompt-file> [-m <model>] [-r <chat-id>]
#             [-b <base>] [-t <timeout-secs>] [-P none]
#
#   -n  branch name, also the log folder (e.g. feat/t1-hardening)
#   -p  file holding the task prompt (markdown, kept outside the repo)
#   -m  model (default: $CA_MODEL or composer-2.5; escalation: grok-4.7-high)
#   -r  resume an existing chat (fix round); the preamble is not re-sent
#   -b  base branch for a new branch (default: main)
#   -t  hard timeout in seconds (default: 5400)
#   -P  "none" to skip templates/preamble.md
#
# Runs are sequential: the checkout must be clean and no other run may be live.
# Exit code is cursor-agent's; 142 = hard timeout.
set -euo pipefail

SKILL_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NAME="" PROMPT="" BASE="main" CHAT="" PREAMBLE="yes"
MODEL="${CA_MODEL:-composer-2.5}"
TIMEOUT=5400

while getopts "n:p:m:r:b:t:P:" opt; do
  case "$opt" in
    n) NAME="$OPTARG" ;;
    p) PROMPT="$OPTARG" ;;
    m) MODEL="$OPTARG" ;;
    r) CHAT="$OPTARG" ;;
    b) BASE="$OPTARG" ;;
    t) TIMEOUT="$OPTARG" ;;
    P) [ "$OPTARG" = "none" ] && PREAMBLE="no" ;;
    *) sed -n '2,17p' "$0"; exit 2 ;;
  esac
done
[ -n "$NAME" ] && [ -n "$PROMPT" ] || { sed -n '2,17p' "$0"; exit 2; }
[ -f "$PROMPT" ] || { echo "prompt file not found: $PROMPT" >&2; exit 2; }

REPO="$(git rev-parse --show-toplevel)"
cd "$REPO"
SLUG="${NAME//\//-}"
LOG_ROOT="${CA_LOG_DIR:-$HOME/.claude/cursor-runs}/$(basename "$REPO")"
LOCK="$LOG_ROOT/.lock"
mkdir -p "$LOG_ROOT"

if [ -e "$LOCK" ] && kill -0 "$(cat "$LOCK")" 2>/dev/null; then
  echo "another run is live (pid $(cat "$LOCK")); runs are sequential" >&2
  exit 3
fi
if [ -n "$(git status --porcelain)" ]; then
  echo "checkout is dirty; commit or stash first" >&2
  git status --short >&2
  exit 3
fi

if git show-ref --verify --quiet "refs/heads/$NAME"; then
  git switch -q "$NAME"
else
  git switch -q -c "$NAME" "$BASE"
fi

LOG_DIR="$LOG_ROOT/$SLUG"
mkdir -p "$LOG_DIR"
STAMP="$(date +%Y%m%d-%H%M%S)"
LOG="$LOG_DIR/$STAMP.jsonl"
ERR="$LOG_DIR/$STAMP.err"
ln -sf "$LOG" "$LOG_DIR/latest.jsonl"

MESSAGE="$(cat "$PROMPT")"
if [ "$PREAMBLE" = "yes" ] && [ -z "$CHAT" ]; then
  MESSAGE="$(sed -e "s|{{BRANCH}}|$NAME|g" -e "s|{{BASE}}|$BASE|g" \
    "$SKILL_DIR/templates/preamble.md")

$MESSAGE"
fi

ARGS=(-p --force --trust --output-format stream-json --model "$MODEL")
[ -n "$CHAT" ] && ARGS+=(--resume "$CHAT")

echo "branch: $NAME (base $BASE)"
echo "model:  $MODEL"
echo "log:    $LOG"

echo $$ > "$LOCK"
trap 'rm -f "$LOCK"' EXIT

set +e
perl -e 'alarm shift; exec @ARGV' "$TIMEOUT" \
  cursor-agent "${ARGS[@]}" "$MESSAGE" >"$LOG" 2>"$ERR"
CODE=$?
set -e
echo "$CODE" > "${LOG%.jsonl}.exit"

echo "exit:   $CODE$([ "$CODE" = 142 ] && echo ' (timeout)')"
python3 "$SKILL_DIR/scripts/ca-status.py" "$LOG" --texts 2 || true
exit "$CODE"
