# Vendored Web Platform Tests subset

Pinned WPT commit: `3acacfcec497956f0634a3ada2fcd54c568773e0`

Reproduce vendored files:

```sh
./scripts/vendor-wpt.sh
```

## Contents

- `MANIFEST.txt` — every vendored path (reftests, harness HTML, shared resources).
- `resources/testharness.js` — WPT harness (BSD-3-Clause, see `LICENSE.md`).
- `resources/glide-testharnessreport.js` — Glide-specific completion reporter writing JSON for `--dump-text`.
- `fonts/Ahem.ttf` — metric font used by several CSS reftests.
- `integration/` — small offline fixtures used by `tests/compat_runner.rs`.
- ~100 CSS reftests from `css/css-flexbox`, `css/CSS2/floats`, `css/css-backgrounds`, `css/CSS2/box-display`, `css/css-text`, and `html/rendering`.
- ~30 `dom/nodes` testharness tests.

Absolute `/resources/`, `/fonts/`, and `/css/support/` URLs in vendored HTML are rewritten to relative paths for `file://` loading.