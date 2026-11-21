# Glide requirements and feature registry

This file is the single record of what Glide must do (requirements, `R-xxx`) and what it
does or will do (features, `F-xxx`). Rules for keeping it current are in `AGENTS.md`.

Status: `planned` · `in-progress` · `done` · `partial` · `deferred` (with reason) · `removed`

## Milestones

| Milestone | Goal | State |
| --- | --- | --- |
| M0 | First window, navigation, experimental JS (PRs #1, #2) | done |
| M1 | Minimal browser: hardening, keyboard, JS toggle, loading/error UX, zoom/find, persistence, compatibility report | done (manual checks M-001–M-028 and first CI run pending) |

## Requirements

| ID | Requirement | Milestone |
| --- | --- | --- |
| R-001 | Open a URL or search phrase typed in the address bar | M0 |
| R-002 | Navigate back, forward and reload, with history that follows redirects and link clicks | M0 |
| R-003 | Render HTML and CSS (flex, floats, SVG, web fonts) with the Blitz engine | M0 |
| R-004 | Run page JavaScript, and allow turning it off | M0 |
| R-005 | Every documented keyboard shortcut and text-editing key works on macOS and Linux | M0 |
| R-006 | Render a page to PNG without a window | M0 |
| R-007 | Quality gates (fmt, clippy -D warnings, tests) pass on every merge and in CI | M1 |
| R-008 | JavaScript can be turned on and off at runtime, and the page reloads in the new mode | M1 |
| R-009 | The user can see that a page is loading, and a failed load shows an error page with retry | M1 |
| R-010 | Page zoom and find-in-page | M1 |
| R-011 | Settings, session, history and bookmarks survive a restart | M1 |
| R-012 | A reproducible HTML/CSS/JS compatibility report against public benchmarks | M1 |
| R-013 | No crash, hang or panic on common pages; super-critical compatibility issues are fixed | M1 |
| R-014 | WPT `dom/nodes` ≥15% subtests and ≥3 full files; Acid3 with JS ≥15/100 | M1 |

## Features

| ID | Feature | Status | Req | Tests | Notes |
| --- | --- | --- | --- | --- | --- |
| F-001 | Address bar: URL vs. search resolution (bare host → https, phrase → DuckDuckGo HTML) | done | R-001 | `location::tests::*`, `chrome::tests::reads_the_address_form` | |
| F-002 | Chrome as an HTML document, page in an `<iframe>`; `glide://` commands | done | R-001, R-002 | `chrome::tests::classifies_chrome_actions` | |
| F-003 | Back / forward / reload with redirect-aware history | done | R-002 | `history::tests::*` | `History` in `src/history.rs` |
| F-004 | Window title follows page `<title>` | done | R-002 | manual | |
| F-005 | Browser shortcuts (Cmd/Ctrl+L/R/W/Q/[/], Alt+arrows, F5, scroll keys) | done | R-005 | `keyboard::tests::readme_shortcut_table`, manual M-004–M-013 | |
| F-006 | Text editing with macOS standard keybindings | done | R-005 | `keyboard::tests::{macos_standard_keybinding_table,headless_address_bar_typing_and_backspace,headless_word_delete_in_address_bar}`, manual M-014–M-015 | |
| F-007 | Keyboard forwarding into the page iframe | done | R-005 | `keyboard::tests::headless_iframe_input_receives_keys`, manual M-002, M-016 | |
| F-008 | V8 page JavaScript: worker thread, DOM RPC, timers, rAF, fetch, events | done | R-004 | `js::apply::tests::*`, `js::integration::*`, `js::scripts::tests::*` | experimental; see README limits |
| F-009 | `--no-js` CLI flag | done | R-004 | `args::tests::*`, `headless_js::*` | |
| F-010 | Headless `--screenshot` to PNG (with and without JS) | done | R-006 | manual | |
| F-011 | `file://`, `data:`, http(s) loading through blitz-net | done | R-003 | indirect | |
| F-020 | T1 Hardening: lib/bin split, `History` module + tests, tested CLI args, clippy clean, CI | done | R-007, R-002 | `history::tests::*`, `args::tests::*`, CI workflow | lib/bin split, `src/app.rs` |
| F-021 | T2 Keyboard verification: table tests for every README shortcut, manual checklist | done | R-005 | `keyboard::tests::*`, `docs/manual-tests.md` M-004–M-016 | |
| F-022 | T3 Runtime JS toggle (toolbar + Cmd/Ctrl+Shift+J) and `--dump-text` headless mode | done | R-008, R-004 | `args::tests::dump_text_*`, `headless_js::*`, `html::tests::set_js_runtime_mode_updates_shared_flags`, `chrome::tests::classifies_chrome_actions`, `keyboard::tests::*` | manual M-017–M-018 |
| F-029 | Headless `--dump-text` (compatibility runner hook) | done | R-004, R-012 | `headless_js::*`, `args::tests::dump_text_*` | shares `headless::load` with screenshot |
| F-023 | T4 Loading indicator, error page with retry, stop button | done | R-009 | `net::tests::*`, `nav::tests::*`, `app::tests::initial_iframe_load_uses_glide_net_error_page`, `chrome::tests::*`, `keyboard::tests::*`, manual M-019–M-021 | Retry is the failed URL; `NavTracker` fixes stop/history |
| F-024 | T5 Page-only zoom (Cmd/Ctrl +/-/0) and find in page (Cmd/Ctrl+F) | done | R-010 | `zoom::tests::*`, `find::tests::*`, `keyboard::tests::readme_shortcut_table`, `chrome::tests::toolbar_height_stable_across_page_zoom`, manual M-022–M-025 | Chrome viewport zoom; toolbar counter-scaled with `--ui` |
| F-025 | T6 Profile dir, settings, session restore, history file, bookmarks, in-memory cookies | done | R-011 | `profile::tests::*`, `history::tests::from_parts_restores_position`, `args::tests::*`, `keyboard::tests::readme_shortcut_table`, `net::tests::{provider_with_cookies_feature_builds,glide_bookmarks_page_delivers_html}`, manual M-026–M-028 | F-026 deferred; profile under platform data dir or `GLIDE_PROFILE_DIR` |
| F-026 | Cookies persisted to disk | deferred | R-011 | | blitz-net builds its own reqwest client; needs a custom NetProvider |
| F-027 | T7 Compatibility runner (`compat` bin) and `docs/compat/REPORT.md` | done | R-012 | `compat::*`, `tests/compat_runner.rs`, `docs/compat/results.json` | Acid1-3 live URLs, ~100 WPT reftests + 30 `dom/nodes` harness; `scripts/vendor-wpt.sh` |
| F-028 | T8 Super-critical compatibility fixes | done | R-013 | `docs/compat/REPORT.md` | T7 run (191 checks) found no crash, panic or hang; nothing to fix. Non-critical gaps are F-030–F-033 |
| F-030 | WPT CSS reftest pixel diffs | deferred | R-012 | `docs/compat/REPORT.md` | 31/99 reftests exceed 0.1% / channel 16 tolerance |
| F-031 | WPT testharness `dom/nodes` measurable | done | R-014 | `docs/compat/REPORT.md`, `docs/DECISIONS.md`, `tests/compat_dom_nodes.rs`, `tests/compat_runner.rs`, `js::integration::*` | Release at `504b190`: 12/28 files fully pass; 636/969 subtests (65.6%). The 8 files with no subtest results each count as one failed subtest. `tests/compat_dom_nodes.rs` gates the same set at ≥10 files and ≥50% |
| F-036 | Domino page DOM + Blitz mirroring | done | R-014 | `src/js/snapshot.rs`, `src/js/batch.rs`, `src/js/dom-glue.js`, `src/js/vendor/domino.js`, `js::integration::*`, `tests/headless_js.rs`, `tests/compat_dom_nodes.rs` | One mutation batch per task (client ids for nodes created in the batch); SVG/MathML namespaces round-trip; `globalThis` is the window. Window capture sees `event.target`; `once` is stored on the listener record; a skipped insert is not queued twice |
| F-034 | `document.write` | done | R-014 | `src/js/dom-glue.js`, `tests/headless_js.rs::{document_write_inserts_after_current_script,async_document_write_does_not_wipe_the_page}`, `docs/compat/REPORT.md` | Inserts after `document.currentScript` during a classic parser script. Async and defer writes are ignored. After load, writes from timers and event handlers replace the document. Inline scripts in the written markup run; external `src` scripts do not |
| F-032 | Missing Web APIs and DOM bindings | deferred | R-012 | `tests/compat/probe.html`, `docs/compat/REPORT.md` | Probe 36/58 available, including `DOMParser`, `URL`, `URLSearchParams`, and `getComputedStyle`. 22 still missing, including `localStorage`, `MutationObserver`, observers, and storage/network host APIs. `Range` is also absent (F-037) |
| F-033 | Acid2/Acid3 layout and JS test gaps | partial | R-014 | `docs/compat/REPORT.md`, `tests/compat_dom_nodes.rs` | Acid3 with JS is 34/100 (R-014 floor is 15). Acid2 reftest still differs (~3.7% pixel diff). Acid1 stays unreachable (HTTP 403) |
| F-035 | T9a DOM library selection (domino / linkedom / happy-dom spike) | done | R-012 | `docs/dom-library-spike.md`, `scripts/dom-spike/run.mjs`, `scripts/dom-spike/tests/fork-timeout.test.mjs`, `tests/dom_spike.rs` | Recommends **domino** (multi-file vendoring); T9b in F-036 |
| F-037 | `Range` for Acid3 | deferred | R-014 | | Not required for first domino mirror milestone |
| F-038 | `MutationObserver` | deferred | R-014 | | domino uses `_setMutationHandler` instead |
| F-039 | Form control value read-back (Blitz → domino) | deferred | R-014 | | `input.value` / `checked` RPC not wired yet |
| F-040 | Hide the raw `__glide_dom_sync` RPC from page code | deferred | R-014 | | `navigate`, `fetch`, and `batchMutate` are reachable as a plain global. Move the binding into a closure the page cannot call |
| F-041 | Watchdog for work outside a script run | deferred | R-014 | | The 5 s limit covers `run_script` only. An endless promise chain keeps the `glide-js` thread spinning with no watchdog |
