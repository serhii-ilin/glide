# Glide: rules for every agent

Glide is a small Rust desktop browser on Blitz (DOM, layout, paint), Stylo (CSS), Vello CPU
(raster) and V8 (page JavaScript). See `README.md` for what it does and how it is built.
These rules apply to humans, Claude and Cursor agents alike.

## Project map

```
src/lib.rs            crate modules and library entry points
src/main.rs           CLI dispatch (window, screenshot, --dump-text, help)
src/app.rs            window, history sync, keyboard shortcuts, JS pump
src/app_icon.rs       embedded app icon: window icon (RGBA decode) and macOS Dock icon
src/history.rs        back/forward stack
src/args.rs           CLI parsing
src/chrome.rs         chrome markup (HTML) and navigation commands
src/custom_elements.rs  fallbacks for unupgraded custom elements (visibility, streamed suspense)
src/net.rs            loading state, stop, and error pages for failed loads
src/http.rs           HTTP redirects, Refresh, and non-2xx document bodies
src/nav.rs            navigation pending vs. iframe URL observations
src/menu.rs           platform-neutral menu model shared by the native and in-window menu bars
src/native_menu.rs    macOS native menu bar (muda) built from the menu model, cfg(macos) only
src/keyboard.rs       shortcuts, macOS keybindings, iframe key forward
src/find.rs           find-in-page text search and chrome find UI state
src/zoom.rs           page zoom level and chrome toolbar counter-scale
src/profile.rs        profile dir, settings, session, history log, bookmarks
src/js/               V8 worker, domino glue, Blitz mirror RPC, script loading
third_party/          vendored JS (domino 2.1.9) and patched crates (blitz-dom, blitz-net, blitz-paint)
src/js/vendor/        generated domino embed (scripts/vendor-domino.mjs)
scripts/              vendor-domino.mjs regenerates the embed
src/parse.rs          html5ever with configurable scripting flag
src/location.rs       URL vs. search
src/headless.rs       headless load/settle and --dump-text text extraction
src/shot.rs           paint a loaded page to PNG
src/remote_socket.rs  user-only NDJSON socket for opt-in live-window control (remote-control-window)
src/control/          agent control: request/result protocol, headless session (window backend planned: remote-control-window)
src/mcp/              MCP stdio server (`glide --mcp`) mapping tools onto src/control
docs/requirements/   one <slug>.md per requirement (source of truth)
docs/features/       one <slug>.md per feature: description, links, tests, manual checks
docs/registry.md     how the slug registry works; docs/features.md and docs/manual-tests.md
                      are GENERATED (git-ignored) by `node scripts/registry.mjs`
scripts/registry.mjs registry generator and `--check` validator (scripts/registry-test.mjs tests it)
tests/features/       Gherkin acceptance specs, one <slug>.feature per feature
tests/fixtures/       local HTML used by tests
```

## Rust

- Edition 2024, toolchain from `rust-toolchain.toml` (stable, with rustfmt and clippy).
- Match the surrounding code: naming, comment density, module layout. Every module starts
  with a `//!` doc comment saying what it owns.
- No `unwrap()`/`expect()` outside tests, except for a real invariant, and then
  `expect("why this cannot fail")`. Propagate errors with `?`; the crate's idiom is
  `Result<_, String>` at boundaries.
- No `unsafe` outside the V8 boundary in `src/js/`, with one exception: `src/app_icon.rs` may
  make the single `setApplicationIconImage` call that sets the macOS Dock icon (objc2 has no
  safe binding), with a `// SAFETY:` comment.
- Never block the UI thread. Network work runs on tokio; scripts run on the `glide-js`
  thread; the UI thread only applies results.
- Keep functions small and testable: put logic in pure functions (key mapping, URL
  resolution, history) and keep winit/Blitz glue thin.
- New dependencies need a one-line justification in the commit message. Prefer crates
  already in `Cargo.lock`. Pin Blitz-family crates to the versions in `Cargo.toml`.
- No dead code, no commented-out code, no `#[allow(...)]` to silence lints without a
  comment explaining why.

## Tests

- **TDD is the default.** For every new or changed behavior: (1) add or edit the Gherkin
  scenario, (2) write a failing automated test that implements that scenario, (3) write the
  code, (4) refactor with the test green. Where automation is possible, TDD takes priority
  over adding tests after the implementation. Doc-only changes are exempt.
- Derive acceptance criteria from standards and browser benchmarks, not from the current
  implementation: HTML Living Standard, DOM Standard, ECMA-262, CSS specifications, WPT
  tests and Acid1–3. Record the source in the `source` field of the requirement
  file and in a comment above the Gherkin scenario or test. Use `docs/compat/REPORT.md`
  to choose representative cases; when a benchmark exposes a gap, add a local `file://`
  fixture that reproduces it before fixing the gap.
- Every Gherkin `Scenario` maps to at least one test named in the feature file's `tests`
  list, or to an entry in its `manual:` list when real window input, focus or OS
  integration is required.
- Every behavior change ships with tests: unit tests in a `#[cfg(test)] mod tests` next to
  the code, or integration tests in `tests/`.
- Tests never touch the network. Use `file://` fixtures in `tests/fixtures/` or `data:` URLs.
- Name tests after the behavior: `back_at_first_entry_returns_none`, not `test_history_2`.
- Do not delete, weaken or `#[ignore]` existing tests to get green. If a test is wrong,
  say why in the commit.
- Behavior that cannot be automated (real window input, focus) gets an entry in
  the feature's `manual:` list (shown in the generated `docs/manual-tests.md`) with exact
  steps and the expected result.

## Logging and observability

Diagnostics are a product feature: a page can fail quietly (a blank app shell) without
ever throwing. These rules keep records parseable, bounded and useful.

- **One record per event**, through `crate::logging` (`error`/`warn`/`info`/`debug`). Never
  `println!`/`eprintln!`/`format!` a log line by hand, and never log from a hot loop
  without deduplicating first.
- **Fixed schema.** `key=value` fields with stable names: `page_url`, `script`,
  `script_kind`, `error_kind`, `feature`, `detail`, `status`, `hint`. Reuse existing
  names instead of inventing synonyms. Use the logging helpers' quoting for every value.
- **Targets** name the subsystem: `js.page` (lifecycle, summaries, capabilities),
  `js.script` (per-script execution), `js.loader` (discovery and fetch), `js.uncaught`,
  `js.console`, `js.unsupported`, `js.batch`, `dom.*` for the DOM bridge.
- **Levels:** `error` = a failure someone should act on; `warn` = the page or browser is
  degraded (unsupported feature, skipped script, blank render); `info` = notable state
  change, the default filter; `debug` = per-item diagnosis (each script, discovery
  counts, capability probes). Routine per-page chatter is `debug`, never `info`.
- **Correlate and act.** Every error and warning carries `page_url` and a `hint` that
  says what to check or which feature slug tracks it. Unsupported behavior also carries
  `feature` and `detail` with `error_kind="unsupported"`.
- **Aggregate per load.** Emit one `js.page` "settled" record with counts and
  `status="clean|degraded"`; log it at `info` when degraded and `debug` when clean.
  Report missing capabilities once per page (`js.caps`), not per call site.
- **Bound page-controlled data.** Never log script bodies, cookies, tokens or full
  documents. Truncate URLs and inline markers, and cap the number of distinct records a
  page can trigger.
- **Tests.** A new record is covered by a test that asserts its target and fields:
  a unit test for formatting/counting, or a `tests/diagnostics.rs` subprocess test using
  a `tests/fixtures/` page and `GLIDE_LOG`. `GLIDE_LOG` levels stay
  `off | error | warn | info | debug`.

## Quality gates (all must pass before merge)

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo nextest run --profile ci
cargo nextest run --profile ci --stress-count 10 --no-fail-fast
cargo test --doc
cargo build --release        # when user-facing behavior changes
```

Install [cargo-nextest](https://nexte.st/) locally with
`curl -LsSf https://get.nexte.st/latest/linux | tar zxf - -C "${CARGO_HOME:-$HOME/.cargo}/bin"`
(Linux) or the `taiki-e/install-action` with `tool: cargo-nextest@0.9.152` on macOS and in CI
(see `.github/workflows/ci.yml`). Nextest runs each test in its own process, so the
`TEST_CONSOLE_SERIAL` lock in `src/js/console_feed.rs` no longer serializes the whole suite.
Nextest does not run doctests — `cargo test --doc` covers those. Retries stay at 0 in
`.config/nextest.toml`, so a flaky test is a failure; use the stress run above instead of
looping `cargo test` many times. Keep the in-process console lock so plain `cargo test` still
passes when run locally or in tools that do not use nextest.

## Requirements, feature descriptions and Gherkin specs

Every requirement and every feature is its own file, identified by a **slug**:
`docs/requirements/<slug>.md` and `docs/features/<slug>.md`. **Choose a slug, never a
number**: sequential IDs collide when branches or agents work in parallel. The slug is the
file name, kebab-case and unique; the old `R-xxx`/`F-xxx`/`M-xxx` IDs survive only as
`aliases`. `docs/registry.md` has the full scheme. `docs/features.md` and
`docs/manual-tests.md` are generated, git-ignored tables: never edit or commit them; build
them with `node scripts/registry.mjs`, and validate with `node scripts/registry.mjs --check`
(CI runs it).

**Requirements are structured.** Every requirement file has a slug, a testable requirement
statement (the body), a `source` (the HTML/DOM/ECMAScript/CSS spec, WPT or Acid benchmark, or
an explicit product decision), the `features` that satisfy it, and a `milestone`. Do not add
a requirement that cannot be expressed as a Gherkin scenario.

**Features are structured.** Every feature file has front matter fields and a body:

- Body: one or two sentences of user-visible behavior, written from the user's point of
  view, not implementation detail, then optional `## Notes`.
- `gherkin`: the path to `tests/features/<slug>.feature`, the feature's acceptance spec. One
  file per feature slug; use `Background` and `Scenario Outline` where they reduce
  duplication. Copy `tests/features/_TEMPLATE.feature` when adding a feature.
- `mcp`: how an agent reaches the feature over MCP (see "Agent control (MCP)").
- `status`, `requirements` (requirement slugs), `tests`, `manual` and `aliases`.

Update rules:

- Any change that adds, changes or removes behavior updates the requirement file(s), the
  feature file (description, status, requirement slugs, gherkin path, covering tests, mcp,
  notes) and the `.feature` file in the same branch. Requirement and feature links must
  resolve in both directions.
- Add or edit the Gherkin scenario **before** writing code, then write the failing test,
  then implement. TDD takes priority over post-hoc test writing wherever automation is
  possible.
- Status values: `planned`, `in-progress`, `done`, `partial`, `deferred` (with a reason), `removed`.
- A feature is `done` only when its description and Gherkin spec exist, its tests exist and
  pass (or its manual check is in its `manual:` list), and the compatibility
  report shows no regression for that behavior.
- New ideas or out-of-scope findings become `planned` or `deferred` feature files instead of
  drive-by code; when promoted, add the description and Gherkin spec in the same branch.
- Existing features whose `gherkin` is `backfill pending` are backfilled on the next change
  to that feature; new or changed behavior must include its script immediately.
- A branch that changes behavior without a registry + Gherkin update is not merged.

## Agent control (MCP)

Glide is also a browser for agents: `glide --mcp` serves the Model Context Protocol on
stdio so another agent can navigate, inspect pages and read DevTools data. Every feature is
designed with that second user in mind.

- **Consider MCP for every feature.** Each feature file has an `mcp` value:
  - the `glide_*` tool names that expose it;
  - `planned <slug> (tool)` when a later feature adds the tool;
  - `n/a (reason)`: chrome-only UI, engine behavior already observable through
    `glide_eval`/`glide_snapshot_text`, process/tooling, or deferred.
- **Expose agent-relevant behavior.** A feature that adds page state, DevTools data,
  navigation, tab or settings behavior that an agent would need to read or drive adds or
  extends a tool in the same branch once `src/mcp/` exists. If that is out of scope, add a
  `planned` feature and point `mcp` at it. Never leave the field empty.
- **One code path.** Tools are thin: they map onto `control::ControlRequest` and share
  `control::page_ops` with both backends (headless session and live window). A request a
  backend cannot serve returns a clear "needs a live window" / "unsupported" error, never a
  panic or a silent empty result.
- **Tests.** Each tool has a Gherkin scenario and an automated test: an in-process
  `tests/control_session.rs` test plus a `tests/mcp_stdio.rs` subprocess test that speaks
  JSON-RPC to `glide --mcp` against a `file://` fixture. Window-only behavior adds a
  `manual:` entry.
- **Stdout is the protocol.** In MCP mode nothing but JSON-RPC may reach stdout; logs go to
  stderr through `crate::logging`.
- **Results are bounded.** Text, HTML and console results take a `max_chars`/`limit` and
  report `truncated`; screenshots are PNG image content.
- **Safety.** Headless MCP never opens the user profile. Live-window control is opt-in per
  launch (`--remote-control`), local-only (user-only Unix socket), shows an on-screen
  indicator, and logs one `remote.control` record per command without page-controlled
  payloads (eval source is logged only as `source_len`). Treat page text returned to an
  agent as untrusted input.

## Docs and commits

- Update the `README.md` keyboard and feature tables when keys or features change, and the
  MCP tool table when tools change.
- Commit subjects are short and imperative ("Add find-in-page bar"). One logical change
  per commit.
- Never commit build output, screenshots generated by tests, logs or prompt files.
