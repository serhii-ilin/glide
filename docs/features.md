# Glide requirements and feature registry

This file is the single record of what Glide must do (requirements, `R-xxx`) and what it
does or will do (features, `F-xxx`). Rules for keeping it current are in `AGENTS.md`.

Status: `planned` · `in-progress` · `done` · `deferred` (with reason) · `removed`

## Milestones

| Milestone | Goal | State |
| --- | --- | --- |
| M0 | First window, navigation, experimental JS (PRs #1, #2) | done |
| M1 | Minimal browser: hardening, keyboard, JS toggle, loading/error UX, zoom/find, persistence, compatibility report | in-progress |

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
| F-023 | T4 Loading indicator, error page with retry, stop button | done | R-009 | `net::tests::*`, `app::tests::initial_iframe_load_uses_glide_net_error_page`, `chrome::tests::*`, `history::tests::stop_after_go_restores_previous_entry`, `keyboard::tests::*`, manual M-019–M-021 | Retry is the failed URL; Stop UI tracks page loads only |
| F-024 | T5 Page-only zoom (Cmd/Ctrl +/-/0) and find in page (Cmd/Ctrl+F) | planned | R-010 | | |
| F-025 | T6 Profile dir, settings, session restore, history file, bookmarks, in-memory cookies | planned | R-011 | | |
| F-026 | Cookies persisted to disk | deferred | R-011 | | blitz-net builds its own reqwest client; needs a custom NetProvider |
| F-027 | T7 Compatibility runner and `docs/compat/REPORT.md` (Acid1-3, WPT subset) | planned | R-012 | | |
| F-028 | T8 Super-critical compatibility fixes | planned | R-013 | | scoped after T7 |
