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

A bare host gets `https://`. A phrase with spaces is sent to DuckDuckGo's HTML frontend (`https://duckduckgo.com/html/`), which does not need page JavaScript. `file://` URLs open local files.

| Key | Action |
| --- | --- |
| Enter | Submit the address bar |
| Alt+Left / Alt+Right (Option on macOS) | Back / forward |
| Cmd/Ctrl+[ / Cmd/Ctrl+] | Back / forward |
| Ctrl+R or F5 (Cmd+R on macOS) | Reload |
| Esc (when no text field is focused) | Stop the in-flight page load |
| Cmd/Ctrl+Shift+J | Turn page JavaScript on or off (reloads the page) |
| Ctrl+L or Cmd+L | Focus the address bar |
| Cmd/Ctrl+W | Close the window |
| Cmd/Ctrl+Q | Quit |
| Cmd/Ctrl+F | Find in page |
| Enter / Cmd/Ctrl+G (find bar open) | Next match |
| Shift+Enter / Cmd/Ctrl+Shift+G (find bar open) | Previous match |
| Esc (in the find field) | Close find and clear selection |
| Cmd/Ctrl+= / Cmd/Ctrl++ / Numpad+ | Zoom page in (10% steps, also with Shift held) |
| Cmd/Ctrl+- / Numpad- | Zoom page out |
| Cmd/Ctrl+0 | Reset zoom to 100% (non-100% zoom shows a **%** label in the toolbar) |
| Cmd/Ctrl+D | Bookmark or unbookmark the current page (toolbar **☆** / **★**) |
| Space / Shift+Space / PgUp / PgDn / Home / End / arrows | Scroll the page when no text field is focused |

Text fields (address bar and in-page inputs) use normal editing shortcuts: Backspace and
Delete, arrows, Home/End, Shift+selection, Cmd/Ctrl+C/X/V, and Cmd/Ctrl+A. On macOS,
Option+Left/Right moves by word; Glide synthesizes Apple's standard keybindings for
Backspace because Blitz's text editor expects them on that platform. Keys are forwarded
into the page `<iframe>` when it has focus.

The first release build compiles Stylo and Vello, so it takes a while. Later builds are incremental.

### Profile and persistence

Glide stores settings, the back/forward session, a navigation log, and bookmarks under a profile directory (typically the OS app data location for **Glide**). Override it with `GLIDE_PROFILE_DIR`. JavaScript on/off and page zoom persist across restarts; `--no-js` wins for that run only and does not overwrite the saved JS setting. With no URL on the command line, the last session is restored; passing a URL starts there without replacing the saved session until you navigate elsewhere. In-memory cookies are enabled for the network stack; cookie persistence to disk is not implemented yet. Open `glide://bookmarks` (or type it in the address bar) for a list of bookmarks, newest first.

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

Globals: `self` / `window` / `globalThis` (`String(window)` is `[object Window]`), domino constructors (`Node`, `HTMLElement`, `Event`, …), `URL` / `URLSearchParams` (resolution via Rust `url`; `URL.prototype.searchParams` is a `URLSearchParams`), `DOMParser`, `document.createTreeWalker` / `document.createNodeIterator`, and `document.write` / `writeln`. `location.protocol`, `host`, `hostname`, `port`, `pathname`, `search`, `hash`, and `origin` come from the page URL. Timers, `fetch`, and `console` remain in `src/js/bootstrap.js`.

`document.write` while a classic parser-inserted script is running inserts the parsed fragment immediately after `document.currentScript` (and runs inline scripts in that fragment, in order). Writes from `async` or `defer` scripts, and writes before load when no parser script is current, are ignored. `document.write` after load, from a timer or event handler, uses `document.open` semantics and replaces the document's children. External scripts referenced from written markup are not fetched. There is no `TreeWalker` or `NodeIterator` constructor on the global object; use `document.createTreeWalker` and `document.createNodeIterator`.

Layout reads go to Blitz by node id. `getBoundingClientRect` and `offsetWidth` / `offsetHeight` / `offsetTop` / `offsetLeft` use Blitz's border box (`offset*` is relative to the offset parent). `clientWidth` / `clientHeight` are that same border-box size, not the CSS padding box. `getComputedStyle` returns Blitz's resolved value for the property (used values for layout-dependent properties such as `width`). No `MutationObserver` or `Range`. No modules, workers, or service workers. Five-second per-script watchdog. `tests/fixtures/counter.html` and `js-toggle.html` are smoke tests; WPT `dom/nodes` and Acid3 scores are in `docs/compat/REPORT.md`.

The first `--release` build downloads and compiles V8 prebuilts in addition to Stylo and Vello, so allow extra time and disk.

## What works

- A native window on Linux and macOS, drawn with Vello's CPU rasterizer.
- HTML and CSS, including flex, floats, and SVG (`blitz-dom` features `floats`, `svg`, and `woff`).
- http, https, and file pages, plus stylesheets, images, and web fonts.
- Link clicks, in-page fragment jumps, scrolling, text selection, and form controls that Blitz implements (the address bar is one of those).
- Back, forward, and reload. Redirects replace the current history entry. A link click pushes a new one.
- Loading indicator, stop (Esc or the toolbar **Stop** button while a page is loading), and a readable error page when a page fails to load. **Retry** on the error page reloads the same URL.
- Page zoom (30%–300%) and find-in-page with match highlight and scroll-into-view. Zoom level persists across restarts.
- Settings (JS on/off, zoom), session restore, bookmark star, `glide://bookmarks`, and an append-only history log on disk. Cookie jar for the current run only.
- System fonts through Parley.
- Experimental page JavaScript via V8 (see above). Pass `--no-js` to turn it off.

## What does not

- **Full browser JavaScript.** The bootstrap is intentionally tiny; Boa in Kitesurf implements far more of the Web platform. DuckDuckGo's JS-heavy UI may still break; the HTML frontend is the default search target.
- **Many browser features.** No multiple tabs, persistent cookies, downloads, or extensions. Blitz's own gaps apply too: incomplete CSS, limited form controls, and no full navigation of frames other than the single page iframe Glide uses as the viewport.
- **Find scroll target.** Find scrolls the inline root that contains the match into view, not the exact line box; tall paragraphs and scrollable subcontainers may not place the caret optimally.
- **Stop vs. subresources.** Stopping a navigation aborts the in-flight main document load, but Blitz also aborts the previous page's subresource fetches when a new navigation starts, so images or scripts on the page you return to may stay missing until you reload.
- **Kitesurf's Workers runtime.** No Chrome DevTools Protocol, no Dynamic Workers, no PDF export. Those live in Kitesurf's TypeScript and Wasm layers.

## Layout

The chrome is an HTML document drawn by Blitz. Back, forward, and reload are links (`glide://back` and similar). The address field is a form whose submit is intercepted. The page is an `<iframe>`; clicks inside it stay in Blitz, and Glide records the iframe's URL as history.

```
src/lib.rs        crate modules and library entry points
src/main.rs       CLI dispatch (window, screenshot, help)
src/app.rs        window, history sync, keyboard shortcuts, JS pump
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
