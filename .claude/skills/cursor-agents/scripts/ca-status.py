#!/usr/bin/env python3
"""Summarize cursor-agent `--output-format stream-json` logs.

Usage:
  ca-status.py <log.jsonl | run-dir> [...] [--texts N]

A run-dir (e.g. ~/.claude/cursor-runs/glide/feat-t1-hardening) resolves to its
latest.jsonl. Prints chat id, model, state, tool usage, failed shell commands,
files edited, token usage and the last N assistant texts (default 3).
"""

import json
import os
import sys
import time
from collections import Counter


def resolve(path):
    if os.path.isdir(path):
        return os.path.join(path, "latest.jsonl")
    return path


def summarize(path, n_texts):
    chat, model, result = None, None, None
    texts, failed = [], []
    tools, files = Counter(), set()
    last_ms = None
    bad = 0

    with open(path, encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            try:
                e = json.loads(line)
            except json.JSONDecodeError:
                bad += 1
                continue
            chat = e.get("session_id") or chat
            last_ms = e.get("timestamp_ms") or last_ms
            kind = e.get("type")
            if kind == "system" and e.get("subtype") == "init":
                model = e.get("model")
            elif kind == "assistant":
                for part in (e.get("message") or {}).get("content") or []:
                    if part.get("type") == "text" and part.get("text", "").strip():
                        texts.append(part["text"].strip())
            elif kind == "tool_call" and e.get("subtype") == "completed":
                call = e.get("tool_call") or {}
                for name, body in call.items():
                    if not name.endswith("ToolCall") or not isinstance(body, dict):
                        continue
                    tool = name[: -len("ToolCall")]
                    tools[tool] += 1
                    args = body.get("args") or {}
                    res = body.get("result") or {}
                    if tool == "edit" and args.get("path"):
                        files.add(args["path"])
                    if tool == "shell":
                        ok = res.get("success") or {}
                        code = ok.get("exitCode") if ok else None
                        if not ok or code not in (0, None):
                            failed.append(f"exit {code}: {str(args.get('command'))[:140]}")
                    elif "error" in res:
                        failed.append(f"{tool}: {str(res['error'])[:160]}")
            elif kind == "result":
                result = e

    exit_file = os.path.splitext(os.path.realpath(path))[0] + ".exit"
    if os.path.exists(exit_file):
        code = open(exit_file, encoding="utf-8").read().strip()
        state = f"exited {code}" + (" (timeout)" if code == "142" else "")
    elif result:
        state = "finished (result event, no .exit file)"
    else:
        state = "running (no .exit file yet)"
    if last_ms:
        state += f", last event {int(time.time() - last_ms / 1000)}s ago"

    print(f"== {path}")
    print(f"chat:   {chat}")
    print(f"model:  {model}")
    print(f"state:  {state}")
    if result:
        usage = result.get("usage") or {}
        print(
            f"result: {result.get('subtype')} error={result.get('is_error')} "
            f"{int((result.get('duration_ms') or 0) / 1000)}s "
            f"in={usage.get('inputTokens')} out={usage.get('outputTokens')} "
            f"cache_read={usage.get('cacheReadTokens')}"
        )
    print("tools:  " + ", ".join(f"{k}×{v}" for k, v in tools.most_common()))
    if files:
        print("edited: " + ", ".join(sorted(files)))
    if failed:
        print(f"failed ({len(failed)}):")
        for f in failed[-10:]:
            print(f"  - {f}")
    if bad:
        print(f"unparsable lines: {bad}")
    for t in texts[-n_texts:]:
        print("--- text")
        print(t[:2000])


def main(argv):
    n_texts = 3
    paths = []
    it = iter(argv)
    for a in it:
        if a == "--texts":
            n_texts = int(next(it))
        else:
            paths.append(a)
    if not paths:
        print(__doc__)
        return 2
    for p in paths:
        summarize(resolve(p), n_texts)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
