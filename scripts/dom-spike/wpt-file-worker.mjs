#!/usr/bin/env node
import { runWptFile } from "./lib/wpt-runner.mjs";

const FILE_WALL_MS = 15_000;

let finished = false;
const watchdog = setTimeout(() => {
  if (!finished) {
    process.send?.({
      ok: false,
      hung: true,
      error: "worker wall-clock budget exceeded",
    });
    process.exit(2);
  }
}, FILE_WALL_MS);

process.on("message", (msg) => {
  try {
    const result = runWptFile(msg.lib, msg.bundleCode ?? "", msg.htmlPath);
    finished = true;
    clearTimeout(watchdog);
    process.send?.({ ok: true, result });
    process.exit(0);
  } catch (e) {
    finished = true;
    clearTimeout(watchdog);
    process.send?.({ ok: false, error: e.message, hung: false });
    process.exit(1);
  }
});