# Glide: rules for every agent

Glide is a small Rust desktop browser on Blitz (DOM, layout, paint), Stylo (CSS), Vello CPU
(raster) and V8 (page JavaScript). See `README.md` for what it does and how it is built.
These rules apply to humans, Claude and Cursor agents alike.

## Project map

```
src/lib.rs        crate modules and library entry points
src/main.rs       CLI dispatch (window, screenshot, --dump-text, help)
src/app.rs        window, history sync, keyboard shortcuts, JS pump
src/history.rs    back/forward stack
src/args.rs       CLI parsing
src/chrome.rs     chrome markup (HTML) and navigation commands
src/net.rs        loading state, stop, and error pages for failed loads
src/http.rs       HTTP redirects, Refresh, and non-2xx document bodies
src/nav.rs        navigation pending vs. iframe URL observations
src/keyboard.rs   shortcuts, macOS keybindings, iframe key forward
src/find.rs       find-in-page text search and chrome find UI state
src/zoom.rs       page zoom level and chrome toolbar counter-scale
src/profile.rs  profile dir, settings, session, history log, bookmarks
src/js/           V8 worker, domino glue, Blitz mirror RPC, script loading
third_party/      vendored JS (domino 2.1.9)
src/js/vendor/    generated domino embed (scripts/vendor-domino.mjs)
scripts/          vendor-domino.mjs regenerates the embed
src/parse.rs      html5ever with configurable scripting flag
src/location.rs   URL vs. search
src/headless.rs   headless load/settle and --dump-text text extraction
src/shot.rs       paint a loaded page to PNG
src/control/      agent control: request/result protocol, headless session, window socket (F-069)
src/mcp/          MCP stdio server (`glide --mcp`) mapping tools onto src/control (planned F-066)
docs/features.md  requirements + feature registry + spec links (keep current)
tests/features/   Gherkin acceptance specs, one F-xxx-<slug>.feature per feature
tests/fixtures/   local HTML used by tests
```

## Rust

- Edition 2024, toolchain from `rust-toolchain.toml` (stable, with rustfmt and clippy).
- Match the surrounding code: naming, comment density, module layout. Every module starts
  with a `//!` doc comment saying what it owns.
- No `unwrap()`/`expect()` outside tests, except for a real invariant, and then
  `expect("why this cannot fail")`. Propagate errors with `?`; the crate's idiom is
  `Result<_, String>` at boundaries.
- No `unsafe` outside the V8 boundary in `src/js/`.
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
  tests and Acid1–3. Record the source in the `Source / basis` column of the requirement
  row and in a comment above the Gherkin scenario or test. Use `docs/compat/REPORT.md`
  to choose representative cases; when a benchmark exposes a gap, add a local `file://`
  fixture that reproduces it before fixing the gap.
- Every Gherkin `Scenario` maps to at least one test named in the feature row's `Tests`
  column, or to a row in `docs/manual-tests.md` when real window input, focus or OS
  integration is required.
- Every behavior change ships with tests: unit tests in a `#[cfg(test)] mod tests` next to
  the code, or integration tests in `tests/`.
- Tests never touch the network. Use `file://` fixtures in `tests/fixtures/` or `data:` URLs.
- Name tests after the behavior: `back_at_first_entry_returns_none`, not `test_history_2`.
- Do not delete, weaken or `#[ignore]` existing tests to get green. If a test is wrong,
  say why in the commit.
- Behavior that cannot be automated (real window input, focus) gets a row in
  `docs/manual-tests.md` with exact steps and the expected result.

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
  says what to check or which feature ID tracks it. Unsupported behavior also carries
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
cargo test
cargo build --release        # when user-facing behavior changes
```

## Requirements, feature descriptions and Gherkin specs

`docs/features.md` is the single registry of requirements (`R-xxx`) and features (`F-xxx`).

**Requirements are structured.** Every `R-xxx` row has an ID, a testable requirement
statement, a `Source / basis` (the HTML/DOM/ECMAScript/CSS spec, WPT or Acid benchmark, or an
explicit product decision), the `Features` that satisfy it, and a milestone. Do not add a
requirement that cannot be expressed as a Gherkin scenario.

**Features are structured.** Every `F-xxx` row has:

- `Description`: one or two sentences of user-visible behavior, written from the user's
  point of view, not implementation detail.
- `Gherkin`: the path to `tests/features/F-xxx-<slug>.feature`, the feature's acceptance
  spec. One file per feature ID; use `Background` and `Scenario Outline` where they reduce
  duplication. Copy `tests/features/_TEMPLATE.feature` when adding a feature.
- `MCP`: how an agent reaches the feature over MCP (see "Agent control (MCP)").
- `Status`, `Req`, `Tests` and `Notes` as before.

Update rules:

- Any change that adds, changes or removes behavior updates the requirement row(s), the
  feature row (description, status, requirement IDs, Gherkin path, covering tests, MCP, notes)
  and the `.feature` file in the same branch.
- Add or edit the Gherkin scenario **before** writing code, then write the failing test,
  then implement. TDD takes priority over post-hoc test writing wherever automation is
  possible.
- Status values: `planned`, `in-progress`, `done`, `partial`, `deferred` (with a reason), `removed`.
- A feature is `done` only when its description and Gherkin spec exist, its tests exist and
  pass (or its manual check is listed in `docs/manual-tests.md`), and the compatibility
  report shows no regression for that behavior.
- New ideas or out-of-scope findings become `planned` or `deferred` rows instead of
  drive-by code; when promoted, add the description and Gherkin spec in the same branch.
- Existing features that still lack a `.feature` file are backfilled on the next change to
  that feature; new or changed behavior must include its script immediately.
- A branch that changes behavior without a registry + Gherkin update is not merged.

## Agent control (MCP)

Glide is also a browser for agents: `glide --mcp` serves the Model Context Protocol on
stdio so another agent can navigate, inspect pages and read DevTools data. Every feature is
designed with that second user in mind.

- **Consider MCP for every feature.** Each `F-xxx` row has an `MCP` value:
  - the `glide_*` tool names that expose it;
  - `planned F-xxx (tool)` when a later feature adds the tool;
  - `n/a (reason)`: chrome-only UI, engine behavior already observable through
    `glide_eval`/`glide_snapshot_text`, process/tooling, or deferred.
- **Expose agent-relevant behavior.** A feature that adds page state, DevTools data,
  navigation, tab or settings behavior that an agent would need to read or drive adds or
  extends a tool in the same branch once `src/mcp/` exists. If that is out of scope, add a
  `planned` row and point the `MCP` column at it. Never leave the column empty.
- **One code path.** Tools are thin: they map onto `control::ControlRequest` and share
  `control::page_ops` with both backends (headless session and live window). A request a
  backend cannot serve returns a clear "needs a live window" / "unsupported" error, never a
  panic or a silent empty result.
- **Tests.** Each tool has a Gherkin scenario and an automated test: an in-process
  `tests/control_session.rs` test plus a `tests/mcp_stdio.rs` subprocess test that speaks
  JSON-RPC to `glide --mcp` against a `file://` fixture. Window-only behavior adds a
  `docs/manual-tests.md` row.
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
