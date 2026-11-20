import assert from "node:assert/strict";
import path from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

import { runWptFileForked } from "../lib/wpt-fork.mjs";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

test("fork worker enforces per-file wall clock budget", async () => {
  const fixture = path.join(__dirname, "../fixtures/sync-infinite-loop.html");
  const lib = {
    id: "domino",
    globalName: "DomSpikeDomino",
    bootstrap: "require",
  };
  const t0 = Date.now();
  const result = await runWptFileForked(lib, "", fixture);
  const elapsed = Date.now() - t0;
  assert.ok(elapsed < 20_000, `expected hang to finish quickly, took ${elapsed}ms`);
  assert.equal(result.hung, true);
  assert.match(result.error, /timeout|hung|wall-clock/i);
});