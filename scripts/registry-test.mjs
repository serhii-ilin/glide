#!/usr/bin/env node
// Asserts `registry.mjs --check` passes on the valid fixture and fails with the expected message on each negative one.
import { spawnSync } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const cases = [
  ['valid', 0, null],
  ['duplicate-slug', 1, "duplicate slug 'alpha'"],
  ['missing-gherkin', 1, "gherkin file 'tests/features/nope.feature' does not exist"],
  ['unknown-requirement', 1, "unknown requirement 'ghost'"],
];
let failed = 0;
for (const [name, code, message] of cases) {
  const r = spawnSync('node', [path.join(here, 'registry.mjs'), '--check', '--root', path.join(here, 'registry-fixtures', name)], { encoding: 'utf8' });
  const out = r.stdout + r.stderr;
  const ok = r.status === code && (message === null || out.includes(message));
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${name} (exit ${r.status})`);
  if (!ok) { failed++; console.log(out); }
}
process.exit(failed ? 1 : 0);
