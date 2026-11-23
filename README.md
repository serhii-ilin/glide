# Glide

Glide is a small desktop web browser. One window, one address bar, back, forward, and reload. Type a URL or a search, and the page is laid out with the same engine [Kitesurf](https://github.com/cloudflare/kitesurf) uses.

![Glide showing Hacker News](docs/glide.png)

## Run

Rust 1.91 or newer (edition 2024):

```sh
cargo run --release -- --help
cargo run --release                        # restores the last session when no URL is given
cargo run --release -- https://example.com
cargo run --release -- news.ycombinator.com
cargo run --release -- --dump-text file://$PWD/tests/fixtures/js-toggle.html
```

A bare host gets `https://`. A phrase with spaces is sent to DuckDuckGo's HTML frontend (`https://duckduckgo.com/html/`), which does not need page JavaScript. `file://` URLs open local files. `about:blank` is an empty document (new tabs, or typing it in the address bar); other `about:` URLs show a local error page.

| Key | Action |
| --- | --- |
| Enter | Submit the address bar |
| Alt+Left / Alt+Right (Option on macOS) | Back / forward |
| Cmd/Ctrl+[ / Cmd/Ctrl+] | Back / forward |
| Ctrl+R or F5 (Cmd+R on macOS) | Reload |
| Esc (menu open) | Close the open menu |
| Esc (when no text field is focused) | Stop the in-flight page load |
| Cmd/Ctrl+Shift+J | Turn page JavaScript on or off (reloads the page) |
| Ctrl+L or Cmd+L | Focus the address bar |
| Cmd/Ctrl+T | New tab |
| Cmd/Ctrl+W | Close the active tab (the window closes with the last tab) |
| Cmd/Ctrl+Tab / Cmd/Ctrl+Shift+Tab | Next / previous tab |
| Cmd+Shift+] / Cmd+Shift+[ | Next / previous tab on macOS |
| Cmd/Ctrl+1…9 | Select a tab by position |
| Cmd/Ctrl+Q | Quit |
| Cmd/Ctrl+F | Find in page |
| F12 or Meta+Alt+I / Ctrl+Shift+I | Toggle developer tools |
| Cmd/Ctrl+Shift+C | Open DevTools on Elements (if closed) and toggle element inspect mode |
| Esc (focus in the DevTools panel) | Close developer tools |
| Up / Down (Elements tree focused) | Select previous / next DOM row |
| Left / Right (Elements tree focused) | Collapse or parent / expand node |
| Enter (Console input focused) | Evaluate JavaScript in the active page |
| Up / Down (Console input focused) | Previous / next entry in per-tab Console history |
| Click a row (Network tab) | Show request URL, method, status and response headers in the detail pane (does not navigate) |
| Enter / Cmd/Ctrl+G (find bar open) | Next match |
| Shift+Enter / Cmd/Ctrl+Shift+G (find bar open) | Previous match |
| Esc (in the find field) | Close find and clear selection |
| Cmd/Ctrl+= / Cmd/Ctrl++ / Numpad+ | Zoom page in (10% steps, also with Shift held) |
| Cmd/Ctrl+- / Numpad- | Zoom page out |
| Cmd/Ctrl+0 | Reset zoom to 100% (non-100% zoom shows a **%** label in the toolbar) |
| Cmd/Ctrl+D | Bookmark or unbookmark the current page (toolbar star outline / filled) |
| Space / Shift+Space / PgUp / PgDn / Home / End / arrows | Scroll the page when no text field is focused |

The in-window **DevTools** panel (F12) has Console, Elements, and Network tabs. The Network tab lists completed document loads, subresources, and page `fetch` / `XHR` for the active tab (filter box, **Clear**, and per-row detail); the per-tab log clears on navigation.

Text fields (address bar and in-page inputs) use normal editing shortcuts: Backspace and
Delete, arrows, Home/End, Shift+selection, Cmd/Ctrl+C/X/V, and Cmd/Ctrl+A (select all).
Selecting text on the page and pressing Cmd/Ctrl+C copies that selection.
Cmd+L focuses the address bar and selects its text, so the next key replaces the URL.
On macOS, Cmd+Backspace deletes back to the start of the line and Cmd+Delete (forward
delete) deletes to the end; Option+Left/Right moves by word. Glide synthesizes Apple's
standard keybindings because Blitz's text editor expects them on that platform. Keys are
forwarded into the page `<iframe>` when it has focus.

The first release build compiles Stylo and Vello, so it takes a while. Later builds are incremental.

### Profile and persistence

Glide stores settings, all open tabs with their back/forward stacks and per-tab find/zoom state, a navigation log, and bookmarks under a profile directory (typically the OS app data location for **Glide**). Override it with `GLIDE_PROFILE_DIR`. Older one-tab session files are migrated on load. JavaScript on/off is global and persists across restarts; page zoom and find state are per tab. `--no-js` wins for that run only and does not overwrite the saved JS setting. With no URL on the command line, the last session is restored; passing a URL starts one new tab there without replacing the saved session until you navigate elsewhere. Cookies share one profile-wide jar across tabs, navigation, `fetch`/XHR, and `document.cookie`; they persist in `cookies.json` under the profile directory when persistence is enabled. Open `glide://bookmarks` (or type it in the address bar) for a list of bookmarks, newest first.

### Linux packages

Glide links winit, fontconfig (system fonts), and reqwest's native-tls backend, which uses OpenSSL on Linux:

```sh
sudo apt install pkg-config libssl-dev libxkbcommon-dev libxkbcommon-x11-0 \
  libxcb1-dev libx11-dev libfontconfig1-dev
```

`libxkbcommon-x11-0` is needed at runtime on X11. No GPU is required.

macOS uses the system security framework and Core Text. The Xcode command-line tools are enough.

### Screenshot without a window

```sh
cargo run --release -- --screenshot https://example.com example.png
cargo run --release -- --screenshot --no-js https://example.com example-nojs.png
```

This fetches the page, waits for stylesheets and images, runs page scripts unless `--no-js` is set, and writes a PNG with Vello's CPU rasterizer.

### Agent control (MCP)

`glide --mcp` serves the [Model Context Protocol](https://modelcontextprotocol.io) on stdio
with a dedicated headless session (never opens your profile). Build a release binary, then wire
it into your MCP client:

```sh
cargo build --release
claude mcp add glide -- /absolute/path/to/target/release/glide --mcp
```

Generic `mcpServers` snippet:

```json
{
  "mcpServers": {
    "glide": {
      "command": "/absolute/path/to/target/release/glide",
      "args": ["--mcp"]
    }
  }
}
```

Add `--no-js` to the args array to disable page JavaScript for that server. Page text, HTML, and
other tool output are **untrusted input** for the calling agent. Local `file://` paths the
process can read are loadable.

**Shipped tools:** `glide_navigate`, `glide_back`, `glide_forward`, `glide_reload`,
`glide_page_info`, `glide_snapshot_text`, `glide_dom`, `glide_query`, `glide_computed_style`,
`glide_screenshot`.

**Planned:** `glide_console_messages`, `glide_eval`, click/type tools, tab helpers,
`glide --mcp --attach` to drive a live window (`glide --remote-control`, F-069).

### Compatibility report

```sh
cargo build --release --bin glide --bin compat
cargo run --release --bin compat -- --suite all
```

Writes `docs/compat/REPORT.md` and `docs/compat/results.json` (Acid1–3, vendored WPT subset, API probe). Re-vendor WPT files with `./scripts/vendor-wpt.sh`.

## Why this is not a Kitesurf crate dependency

Kitesurf (Apache-2.0, opened November 2026) is a browser engine built to run on Cloudflare Workers. Its Rust workspace members are `kitesurf-core`, `renderer-core`, and `scene-host-wasm`. They are the right code for a Worker, and a poor fit for a native window:

- `packages/core/src/lib.rs` imports `wasm_bindgen` unconditionally and exposes the document as a Wasm API. `renderer-core` is a `cdylib` that `#[path]`-includes that code.
- The workspace pins `getrandom` 0.2 with the `js` feature and `getrandom` 0.3 with `wasm_js`. Those backends do not build for a desktop target.
- `[patch.crates-io]` rewrites Blitz, Stylo, Boa, Parley, and Vello onto `packages/core/target/vendor-patched/...`. That tree is produced by Bazel when Kitesurf is built. It is not in the git checkout (`packages/core` contains `src`, tests, and build files, not `target/`), so a git dependency cannot resolve.

Glide therefore links the native crates Kitesurf is built on, at the versions Kitesurf pins:

| Piece | Crate | Role |
| --- | --- | --- |
| HTML, DOM, layout, paint | `blitz-html`, `blitz-dom`, `blitz-paint` 0.3.0-beta.4 | The engine Kitesurf embeds |
| CSS | Stylo 0.22, via Blitz | Same Servo style engine |
| Window and input | `blitz-shell` + winit 0.31 | Native event loop. Kitesurf has no desktop shell; its local client is a Wasm scene host |
| Pixels | `anyrender_vello_cpu` 0.18 (`vello_cpu` 0.3) | The CPU rasterizer Kitesurf uses. The window presents it with softbuffer, so Glide does not need Vulkan or Metal |
| Network | `blitz-net` | http, https, file, and data URLs |

Kitesurf patches those crates (box-shadow performance, gradient LUTs, iframe scene recording, a smaller `NodeId`, and others) before compiling them to Wasm. Glide uses the published crates, so it does not include those patches. The painting model is the same: Blitz builds a display list, Vello rasterizes it.

## Page JavaScript

Glide runs page scripts with [V8](https://v8.dev/) on a dedicated `glide-js` thread. The **Blitz** document on the UI thread is authoritative for layout and paint; page scripts see a **[domino](https://github.com/fgnass/domino) 2.1.9** in-memory DOM (vendored under `third_party/domino/`, embedded as `src/js/vendor/domino.js`). On each navigation the worker snapshots Blitz into domino, mirrors mutations back in batches over RPC, and proxies geometry (`getBoundingClientRect`, `getComputedStyle`, …) by Blitz node id. See `src/js/dom-glue.js` and `docs/dom-library-spike.md`.

Globals: `self` / `window` / `globalThis` (`String(window)` is `[object Window]`), domino constructors (`Node`, `HTMLElement`, `Event`, …), `URL` / `URLSearchParams` (resolution via Rust `url`; `URL.prototype.searchParams` is a `URLSearchParams`), `DOMParser`, `document.createTreeWalker` / `document.createNodeIterator`, and `document.write` / `writeln`. `location.protocol`, `host`, `hostname`, `port`, `pathname`, `search`, `hash`, and `origin` come from the page URL. Timers, `fetch` (with `Request`/`Response`/`Headers`), `XMLHttpRequest`, `navigator.sendBeacon`, `Blob`/`File`/`FileReader`, `TextEncoder`/`TextDecoder`, `console`, `atob`/`btoa`, `matchMedia`, in-memory `localStorage`/`sessionStorage`, `AbortController`/`AbortSignal`, `requestIdleCallback`, `IntersectionObserver`, `ResizeObserver`, `CSS.supports` / `CSS.escape` live in `src/js/bootstrap.js`. `MutationObserver` and `customElements` (registry, upgrade, lifecycle reactions, and `:defined` mirroring) are implemented in `src/js/dom-glue.js` (domino mutation fan-out). `HTMLElement.dataset` is a live `DOMStringMap` over `data-*` attributes. Host fetches run synchronously on the UI thread; `AbortSignal` does not cancel them after they start. CORS and credentials mode are not enforced; `Set-Cookie` is hidden from script-visible response headers, but navigation, `fetch`/XHR, and `document.cookie` (in `dom-glue.js`) share one profile-wide jar. HttpOnly cookies are omitted from the `document.cookie` getter.

`<noscript>` fallback content is hidden while page scripts are enabled and shown when they are disabled, matching the HTML rendering rules. `document.write` while a classic parser-inserted script is running inserts the parsed fragment immediately after `document.currentScript` (and runs inline scripts in that fragment, in order). Writes from `async` or `defer` scripts, and writes before load when no parser script is current, are ignored. `document.write` after load, from a timer or event handler, uses `document.open` semantics and replaces the document's children. External scripts referenced from written markup are not fetched. There is no `TreeWalker` or `NodeIterator` constructor on the global object; use `document.createTreeWalker` and `document.createNodeIterator`.

Form controls mirror Blitz on `input`/`textarea` `.value` and `.checked` getters and sync before `requestSubmit`/`submit`. Headless and MCP loads follow `POST` navigations as `application/x-www-form-urlencoded` (multipart document posts are unsupported); history back loads the previous URL with `GET` only.

Layout reads go to Blitz by node id. `getBoundingClientRect` and `offsetWidth` / `offsetHeight` / `offsetTop` / `offsetLeft` use Blitz's border box (`offset*` is relative to the offset parent). `clientWidth` / `clientHeight` are that same border-box size, not the CSS padding box. `getComputedStyle` returns Blitz's resolved value for the property (used values for layout-dependent properties such as `width`). There is no `Range`. ES modules resolve `file://`, `data:`, and `http(s)` imports (no import maps or bare specifiers). No workers or service workers. Five-second per-script watchdog. `tests/fixtures/counter.html`, `js-toggle.html`, `web-api-shims.html`, and `observers.html` are smoke tests; WPT `dom/nodes` and Acid3 scores are in `docs/compat/REPORT.md`.

### Diagnostics

Failures from page scripts, script loading, DOM mutation batches and profile I/O are emitted as structured `key=value` records on stderr:

```text
level=ERROR target="js.uncaught" msg="Uncaught TypeError: ... " page_url="file:///page.html" script="script-0" line="2" error_kind="missing_api" hint="..."
```

Uncaught V8 exceptions are captured through a message listener with a stack trace, so the raw `<unknown>:N: Uncaught …` line no longer loses the page and script identity. `error_kind` classifies the root cause (`missing_api`, `invalid_url`, `incompatible_descriptor`, `dom_rpc`, `script_timeout`, `other`) and `hint` explains known compatibility gaps. Page `console.error`/`warn` records use the same fields.

Set `GLIDE_LOG=off|error|warn|info|debug` to control verbosity (default `info`). For example, `GLIDE_LOG=error cargo run --release` prints only failures.

Pages can also fail quietly — a JS app that probes an API Glide does not provide takes a fallback path without ever throwing. Those cases leave their own records: `js.unsupported` names a skipped or unexecutable script (`feature="dynamic-script"`, `module-import`), `js.caps` lists the capability probes that failed on the page, and `js.page` logs script discovery plus one summary per load (`status="clean|degraded"`, error counts) and a warning when a page renders no visible text. Per-script timing is `debug`; the schema and level rules are in `AGENTS.md`.

Before hit testing a click, Glide resolves the page layout and prunes paint entries that name a node already removed by script. If pruning is needed it logs a `dom.hit` warning; the click continues instead of panicking inside Blitz.

The first `--release` build downloads and compiles V8 prebuilts in addition to Stylo and Vello, so allow extra time and disk.

## What works

- A native window on Linux and macOS, drawn with Vello's CPU rasterizer.
- HTML and CSS, including flex, floats, and SVG (`blitz-dom` features `floats`, `svg`, and `woff`).
- http, https, and file pages, plus stylesheets, images, and web fonts.
- Link clicks, in-page fragment jumps, scrolling, text selection, and form controls that Blitz implements (the address bar is one of those).
- Tabs with a tab strip, new-tab button, per-tab close controls, and an in-window File/Edit/View/History/Bookmarks/Tabs/Help menu bar.
- A Safari-like (macOS light) look: one neutral grey chrome surface in the system font, thin line icons for back and forward, a centered borderless address field with reload inside it that shows the page title and switches to the URL (with a blue focus ring) when you click it, and the zoom, JavaScript and bookmark controls on the right. Tabs sit below the toolbar as equal-width segments with the close button at the leading edge, and the find bar has chevrons and Done. The window always shows at least one tab.
- Back, forward, and reload. Redirects replace the current history entry. A link click pushes a new one. Each tab keeps its own history, address, title, loading state, find state and zoom.
- Loading indicator, stop (Esc or the toolbar **Stop** button while a page is loading), and a readable error page when a page fails to load. **Retry** on the error page reloads the same URL. HTTP redirects (301, 302, 303, 307, 308, a `Refresh` header, and a zero-delay `<meta http-equiv="refresh">`) are followed and the address bar shows the final URL. A refresh that sits only inside `<noscript>` is followed when that is the page's only content. A 4xx or 5xx response with a body is shown as that page; an empty error status or a host that cannot be reached still uses Glide's error page.
- Per-tab page zoom (30%–300%) and find-in-page with match highlight and scroll-into-view. Both persist per tab across restarts.
- Settings (JS on/off and default zoom), multi-tab session restore, bookmark star for the active tab, `glide://bookmarks` and `glide://help`, and an append-only history log on disk. Cookie jar for the current run only.
- System fonts through Parley.
- Experimental page JavaScript via V8 (see above). Pass `--no-js` to turn it off.

## What does not

- **Full browser JavaScript.** The bootstrap is intentionally tiny; the new Web API shims are in-memory or no-op. Boa in Kitesurf implements far more of the Web platform. DuckDuckGo's JS-heavy UI may still break; the HTML frontend is the default search target.
- **Many browser features.** No native macOS menu bar, modified-click link opening in new tabs, durable Web Storage, real media queries or mutation observation, downloads, or extensions. Only one page iframe is live at a time; switching tabs reloads its document while per-tab state is restored. Blitz's own gaps apply too: incomplete CSS and limited form controls.
- **Find scroll target.** Find scrolls the inline root that contains the match into view, not the exact line box; tall paragraphs and scrollable subcontainers may not place the caret optimally.
- **Stop vs. subresources.** Stopping a navigation aborts the in-flight main document load, but Blitz also aborts the previous page's subresource fetches when a new navigation starts, so images or scripts on the page you return to may stay missing until you reload.
- **Kitesurf's Workers runtime.** No Chrome DevTools Protocol, no Dynamic Workers, no PDF export. Those live in Kitesurf's TypeScript and Wasm layers.

## Layout

The chrome is an HTML document drawn by Blitz. It contains, top to bottom, a menu bar, toolbar, tab strip and find bar. Back, forward, tab and menu actions are links (`glide://back`, `glide://new-tab`, `glide://menu-open?menu=file`, and similar). The address field is a form whose submit is intercepted. The page is an `<iframe>`; clicks inside it stay in Blitz, and Glide records the iframe's URL as history for the active tab. Switching tabs reloads the single iframe and restores the selected tab's state.

```
src/lib.rs        crate modules and library entry points
src/main.rs       CLI dispatch (window, screenshot, help)
src/app.rs        window, tab sync, history sync, keyboard shortcuts, JS pump
src/tabs.rs       per-tab state, tab selection and session v2
src/history.rs    back/forward stack
src/args.rs       CLI parsing
src/chrome.rs     chrome markup and navigation commands
src/net.rs        loading state, stop, and error pages for failed loads
src/keyboard.rs   shortcuts, macOS keybindings, iframe key forward
src/js/           V8 worker, domino glue, Blitz mirror RPC
third_party/      domino 2.1.9 (page JS DOM)
src/parse.rs      html5ever with configurable scripting flag
src/location.rs   URL vs. search
src/profile.rs    on-disk settings, session, history log, bookmarks
src/shot.rs       headless PNG
```

## License

Apache-2.0. See `LICENSE` and `NOTICE`. Kitesurf, Blitz, Stylo, and the Linebender crates keep their own licenses; Glide does not vendor their source.
