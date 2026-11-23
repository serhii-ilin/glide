# Web platform gaps

Research and planning only. This document does not change engine behavior.

Base: `origin/main` at `2d9ad4e412b1f0ec303c400e408f3e51aa5fc315`
(`2d9ad4e`, "Merge origin/main into main"). `e151e75` is an ancestor.
Read against the tree at that commit: `src/js/`, `src/net.rs`, `src/http.rs`,
`src/custom_elements.rs`, `docs/features.md`, and Blitz 0.3.0-beta.4 as pinned
in `Cargo.toml`.

`docs/compat/REPORT.md` is from `504b190` (2026-11-20). It still lists
`fetch`, `MutationObserver`, `localStorage`, `history.pushState`, and
`crypto.getRandomValues` as missing. Those landed in F-051, F-055, F-056, and
F-057. Do not use that report as the current API inventory. Its WPT and Acid
numbers are still the latest measured rendering and `dom/nodes` scores.

## How to read status

- **works**: a page can use the feature for the common case, and a test in
  this tree covers that case.
- **partial**: the name exists or the engine does some of the work, and a
  real site will still misbehave.
- **missing**: no implementation. Feature detection sees `undefined` or the
  call logs `js.unsupported`.

Effort is the size of a Glide change, not a rewrite of Stylo or V8:
S is one module and its tests, M crosses two modules, L is a new subsystem
or a thread-boundary protocol, XL is a multi-spec engine feature.

## Benchmarks and suites

Tests in this repo must not touch the public network (`AGENTS.md`). Page
fixtures are `file://` or `data:`. `src/http.rs` already binds
`127.0.0.1:0` inside its own tests for redirects. That is the only loopback
precedent. A conformance gate should keep using vendored files and that
loopback pattern, not live hosts.

| Suite | What it measures | Licence | Offline? | Vendoring effort | Recommendation |
| --- | --- | --- | --- | --- | --- |
| WPT (`web-platform-tests/wpt`), scored on [wpt.fyi](https://wpt.fyi) | Spec conformance. Testharness tests assert APIs. Reftests compare pixels. | 3-clause BSD. Confirmed in `tests/compat/wpt/LICENSE.md`. | The files are static. Many tests need `wptserve` (multiple origins, stash, websockets). Self-contained `dom/`, `html/syntax`, `url/`, and CSS reftests run from `file://` through the existing `compat` runner. | Already started: `scripts/vendor-wpt.sh`, ~100 CSS reftests, 30 `dom/nodes` harness files. | **Primary gate.** Grow the vendored subset. Do not point CI at wpt.fyi. |
| WPT subdirs that fit headless Glide today | `dom/nodes` (65.6% subtests at `504b190`), CSS2 box/float and css-backgrounds reftests (31/99 over the 0.1% tolerance, F-030). | Same BSD. | Yes, via `--dump-text` for harness JSON and `--screenshot` for reftests. | Low for the next slice: `dom/events`, `html/syntax/parsing`, `url/`. | Vendor those next. Skip `service-workers`, `websockets`, `webgl`, and `webcodecs` until the feature exists. |
| Acid2 | CSS2 visual reftest (floats, generated content, stacking). | The live page is the WaSP test. Confirm the header before copying it into `tests/`. Glide currently fetches it live. | The page is one document plus images. It can be vendored. | S, once the licence line is recorded. | Keep as a pixel gate. Last run: 3.7% diff (`docs/compat/REPORT.md`). Acid1 returned HTTP 403 to Glide's user agent; do not block on it. |
| Acid3 | DOM and JS score out of 100. | Same caveat: confirm before vendoring. | One HTML document. Yes. | S. | Keep. Last score with JS: 34/100 (R-014 floor is 15). Useful as a regression alarm, not as a feature list. |
| html5test.com | Feature-detection score. A missing name and a wrong implementation score the same. | The website is a service. Do not scrape it in CI. | No. It loads remote scripts. | Not worth it. | **Do not adopt.** Overlaps `tests/compat/probe.html`. |
| css3test.com | Same idea for CSS properties. | Online probe. | No. | Not worth it. | **Do not adopt.** `CSS.supports` in Glide is an allow-list (`src/js/bootstrap.js` around the `CSS_KNOWN_PROPERTIES` set) and would game this score. |
| Test262 | ECMAScript language conformance. | BSD-3-Clause (tc39/test262). | Yes, but it is a V8 suite. | Large, and it does not exercise host bindings. | **Do not re-run.** V8's published Test262 status is the language result. Add host cases to the probe and to `js::integration` instead. |
| Interop 2026 | Pass rate on a chosen WPT slice. Focus areas include container style queries, anchor positioning, `attr()`, `contrast-color()`, CSS `zoom`, custom highlights, dialogs and popovers, fetch uploads and ranges, IndexedDB `getAllRecords()`, Wasm JSPI, media pseudo-classes, Navigation API, scoped custom-element registries, scroll-driven animations, scroll snap, `shape()`, view transitions, a web-compat bucket, WebRTC, and WebTransport. | The tests are WPT (BSD). | The chosen tests can be vendored. Most need features Glide does not have. | L to vendor the whole slice, and it would be red. | Use as a **later** backlog, not a 2026 gate. The only near items are fetch `FormData` and dialogs, after the P0 list. |
| Speedometer 3 | Real-app JS and rendering throughput (TodoMVC-style). | BrowserBench, Apache-2.0. | The workload can be vendored, but it needs a broad DOM, layout, and framework surface. | L, and the number is not comparable until pages render. | Defer. Not a conformance suite. |
| JetStream | JS and Wasm throughput. | Mixed per subtest; BrowserBench distributes it. | Mostly offline. | M to vendor, little product signal beyond V8. | Defer. V8 already owns this. |
| MotionMark | Canvas, SVG, and animation frame rate. | BrowserBench / WebKit-derived benchmark. | Offline, but it needs canvas or SVG animation that we do not paint. | M, and it would not run. | Defer until canvas or SVG animation exists. |
| MDN browser-compat-data (BCD) | Feature checklist with spec URLs and browser versions. Not a test. | CC0. The npm package `@mdn/browser-compat-data` is the source. | Yes. The JSON is static. | M to import a filtered JSON and diff it against the probe. The full dataset is large. | **Checklist only.** Generate rows for this document's next revision. Do not treat a BCD "shipped in Chrome" flag as a test. |
| Servo | Tracks WPT on wpt.fyi (engine `servo`) plus in-tree unit tests. | MPL-2.0 for Servo code. Tests they share are WPT. | Their CI is not our CI. | Do not fork Servo tests. | Follow their WPT triage notes when choosing the next vendored directory. |
| Ladybird (LibWeb) | Own Layout/LibWeb tests plus imported WPT. | BSD-2-Clause for LibWeb. | Their harness expects Ladybird's test runner. | Do not vendor LibWeb tests. | Same as Servo: use their WPT expectations as a hint, not as fixtures. |
| Blitz | Pinned at 0.3.0-beta.4. No public conformance dashboard. Layout is Taffy (flex, grid, table) plus Blitz float and multicol code. | Blitz crates follow their own licences; we already depend on them. | Their unit tests run in the crates, not in Glide. | None. | When a CSS reftest fails, check whether Blitz already has a test for that case before writing a Glide-only workaround. |

### Recommendation

Keep one offline gate, the `compat` binary (F-027):

1. Vendored WPT, BSD, under `tests/compat/wpt/`. Next directories: `dom/events`, `html/syntax/parsing`, `url/`. Keep the `dom/nodes` threshold test (`tests/compat_dom_nodes.rs`). Re-run CSS reftests after layout fixes and update F-030; do not ignore failures to go green.
2. Acid2 as a pixel diff and Acid3 as a score, vendored once the licence line on the file is written into this doc. Stop fetching them live.
3. A host-API probe (`tests/compat/probe.html`) regenerated from this inventory, so F-032 stops listing APIs that already exist.
4. BCD as a quarterly checklist, not a CI run.
5. Leave Test262, html5test, css3test, Speedometer, JetStream, and MotionMark out of the merge gate.

Interop 2026 is a WPT filter we cannot pass yet. Revisit it after the P0 gaps below.

## Gap inventory

Evidence is from this tree unless it names a Blitz path. "Sites" are examples of who notices, not a claim that we loaded those sites here.

### DOM

| Gap | Status | Evidence | Who notices | Effort |
| --- | --- | --- | --- | --- |
| Node, Element, Document, querySelector, innerHTML, classList, dataset, CustomEvent, DOMParser | works | Domino embed (`src/js/vendor/domino.js`, `src/js/dom-glue.js`). `dom/nodes` 636/969 subtests at `504b190` (F-031). Dataset is F-051. | General DOM scripts. | — |
| DOM mutation mirror back to Blitz | works | F-036. One batch per task in `src/js/batch.rs`. | Any script that builds UI. | — |
| Events (listeners, capture on window, `once`) | partial | F-036 notes window capture and `once`. No full UI-event retargeting for page iframes. Dispatch is the domino event model, not the HTML event loop. | Drag-and-drop, focus delegation, React synthetic events mostly work; pointer-event details do not. | L |
| MutationObserver | partial | F-038, F-057. `oldValue` is null because domino does not store it (`docs/features.md` F-038 note). | Libraries that diff attributes. | S |
| TreeWalker, NodeIterator, NodeFilter | works | Domino exports them (`src/js/vendor/domino.js` TreeWalker module). Not separately tested in Glide. | Editors, sanitizers. | S to add a fixture |
| Shadow DOM (`attachShadow`, slots, closed mode) | missing | No `attachShadow` in `src/js`. Probe at `504b190` reported it missing; nothing since adds it. | Almost every modern design system and YouTube/GitHub web components. Light DOM custom elements still run without it. | L |
| Custom elements | partial | Registry, `define` / `get` / `whenDefined` in `src/js/bootstrap.js` (`customElements`, `upgrade` is empty). F-073 only forces un-upgraded markup visible because Stylo treats every element as `:not(:defined)`. `src/custom_elements.rs` header still says modules do not run; F-074 superseded that sentence. | Reddit faceplate, any `connectedCallback` widget. The page stays on server HTML. | L |
| `<template>` including streamed `template for` | partial | F-073 swaps `<suspense-placeholder>` in Rust. The HTML template element exists in domino. `import.meta` and declarative shadow DOM do not. | Reddit's streamed feed. Other template users work for inert content. | S for the rest of inert template; L for declarative shadow |
| Range, Selection, `getSelection` | missing | F-037 deferred. Needed for Acid3 and for selecting page text from script. | Rich text, Acid3 score, "share selection" buttons. | M |
| `document.write` | partial | F-034. Works during classic parser scripts and replaces the document after load. External scripts in the written markup do not run. Async and defer writes are ignored. | Legacy ads and a few analytics snippets. | S |

### JavaScript host APIs

V8 supplies the language (promises, modules syntax, `Intl`, `Proxy`, `WeakRef`, and WebAssembly unless we turn it off; nothing in `src/js/worker.rs` disables Wasm). The gaps are host objects and the HTML event loop.

| Gap | Status | Evidence | Who notices | Effort |
| --- | --- | --- | --- | --- |
| Timers, rAF, microtasks | works | `src/js/bootstrap.js` `setTimeout` / `requestAnimationFrame` / `queueMicrotask`. The worker pumps them (`src/js/worker.rs`). F-008. `queueMicrotask` runs before the next timer, which matches the HTML event loop for the cases we test. | Every app. | — |
| `requestIdleCallback` | partial | F-057. Implemented as a timeout, not idle-deadline scheduling tied to paint. | Analytics beacons. | S |
| `fetch`, `Request`, `Response`, `Headers` | partial | F-055. Bodies are buffered, not streamed. Redirects are followed for navigation in `src/http.rs` and reported on `fetch`. `Set-Cookie` is stripped before script (`src/js/apply.rs` `FORBIDDEN_RESPONSE_HEADERS`). No cache mode, no integrity, no upload stream. | APIs and SPAs that `fetch` JSON. Streaming uploads and cookie-authenticated fetches fail. | M for FormData and cookies; L for streams |
| XMLHttpRequest | partial | F-055 tests cover sync and async, abort, and headers. Sync XHR blocks the JS worker, not the UI thread, but it still stalls scripts. | Older stacks. | S to tighten |
| `AbortController` | partial | F-051 shim. `fetch` checks `aborted` only before the call (`bootstrap.js` `global.fetch`). It does not cancel an in-flight host request. XHR abort is implemented (F-055 test). | SPAs that cancel search requests. | M |
| URL, URLSearchParams | works | F-051. Domino URL plus a complete iterable `URLSearchParams`. | Routers. | — |
| TextEncoder, TextDecoder | works | Hand-rolled in `bootstrap.js`. F-055. | Every binary API. | — |
| Blob, File, FileReader | partial | F-055, in memory. No blob URLs that the network stack can load. | File inputs, image preview. | M |
| FormData | missing | F-032 still lists it, and `bootstrap.js` does not define it. | Any `fetch` of a form, including login. | M |
| structuredClone | missing | Not defined. F-032. | Sites that clone state. A small structured-clone subset is enough for postMessage later. | M |
| `crypto.getRandomValues`, `randomUUID` | works | F-056. `crypto.subtle` is not defined. | Login nonces work. WebCrypto (hash, sign, ECDH) does not. | L for `subtle` |
| performance.timing / entries | partial | F-056. A snapshot, not the full resource timing buffer. | RUM scripts. | S |
| IntersectionObserver, ResizeObserver | partial | F-057. Intersection uses the viewport and a timeout, not scroll or occlusion. Resize runs when re-measured, not on every layout. | Lazy-load and virtualized lists. | M |
| Workers, SharedWorker, Worklets | missing | Not defined. Would be another V8 isolate. | Webpack worker bundles, crypto in a worker. | XL |
| ES modules with a graph | partial | F-074. A module with no imports runs. `resolve_module` returns `None` (`src/js/worker.rs` around the `resolve_module` function) and logs `module-import`. No `import()`, no import maps. | Vite, webpack ESM, any `import` in an app bundle. The module does not run at all. | L |
| Dynamic classic scripts | works | F-074. `innerHTML` scripts do not run, which matches HTML. | Polyfill loaders. | — |
| WebAssembly | untested | V8 includes it. No Glide test. | Wasm decoders and games. Confirm with one fixture before calling it done. | S to probe |

### Networking

| Gap | Status | Evidence | Who notices | Effort |
| --- | --- | --- | --- | --- |
| Document HTTP: redirects, Refresh, non-2xx bodies | works | F-042. `src/http.rs`. 20 hops. | Every navigation. | — |
| HTTP/2 | works at the transport | `Cargo.toml` enables reqwest `http2`. No test that the negotiated protocol is h2. | Large sites. | S to record the protocol |
| HTTP/3 | missing | No quiche or quinn dependency. | A few large sites fall back to h2. | L |
| Cookies on the wire | partial | `src/http.rs` builds a reqwest client with `cookie_store(true)`. The jar is process-wide and not readable by script. | Any `Set-Cookie` login. | M (see plan 1) |
| Cookie persistence | missing | F-026 deferred. The jar dies with the process. | "Remember me". | M after the jar is ours |
| WebSocket | missing | Not defined. F-032. | Slack-like apps, live dashboards, GraphQL subscriptions. | L (see plan 5) |
| EventSource (SSE) | missing | Not defined. | Admin tails, some AI chat UIs. `fetch` streaming would cover part of this. | M |
| CORS | missing | The process fetches as itself. Page script is not blocked cross-origin, and there is no preflight. | This is a security hole and a compatibility lie: servers that require a preflight never see one. | L |
| CSP | missing | No policy parser, no violation reports. | Sites that depend on CSP for script nonces still run scripts. | L |
| Cache, service workers, Cache API | missing | Each load hits the network or `file://`. | Offline PWAs, some Next.js clients. | XL |
| Subresource loads (CSS, images, fonts, scripts) | partial | F-011. Script fetch failures are logged (F-050). No CORS, no cookie attachment from `document.cookie`. | Styled pages. | tied to cookies and CORS |

### Storage

| Gap | Status | Evidence | Who notices | Effort |
| --- | --- | --- | --- | --- |
| `localStorage` / `sessionStorage` | partial | `createStorage` in `bootstrap.js`. Both are in-memory maps. `on_navigation` drops the worker (`src/js/host.rs` `reset_page_js_state`), so both die on a real navigation. They are not per-origin on disk. Within one document they work, including property access. | Sites that stash a token and reload. They do not crash; they forget. | M |
| IndexedDB | missing | Not defined. | Figma-class apps, large offline caches. | XL |
| Cache API | missing | Not defined. | Service-worker sites. | XL, after service workers |
| Cookie jar shared with script | missing | F-058 planned. Wire jar exists. `document.cookie` does not. | Google's search interstitial (`SG_SS` in the F-058 note), most logins. | M (plan 1) |

### CSS and layout

Blitz 0.3.0-beta.4 lays out flex, grid, and table through Taffy
(`blitz-dom` `layout/mod.rs`: `Display::Flex`, `Display::Grid`,
`compute_grid_layout`) and has float and table code of its own. Stylo
computes styles. "The property exists in Stylo" is not "the page matches
Chrome". F-030: 31 of 99 vendored CSS reftests exceed the pixel tolerance.

| Gap | Status | Evidence | Who notices | Effort |
| --- | --- | --- | --- | --- |
| Flex, grid, floats, tables, positioned boxes | partial | Engine code above. F-063 reads used values for width, margin, padding. Reftests still fail (F-030). | Most layout. Bugs are edge cases (margin collapse, float paint order), not "no flex". | L, driven by reftests, one failure at a time |
| Block-in-inline, overflow, scroll | partial | Vendored CSS2 box tests. F-075 fixes wheel hit-testing of nested scrollers. | Long pages, nested panes. | M |
| Selectors, custom properties | partial | Stylo. No Glide test for `:has`, `:is`, or `@property`. | Modern CSS. `:has` is widely used for "parent" styling. | M to measure, then M per missing selector |
| `matchMedia` | partial | F-051. `matches` is always false (`bootstrap.js` `global.matchMedia`). Stylo's media queries for the document are a separate path. | Sites that swap a mobile stylesheet from script, and dark-mode script. | S |
| Animations, transitions, `@keyframes` | missing as a clock | No animation tick in Glide. Stylo may parse the properties. Nothing samples them per frame. | Motion, accordions that depend on `transitionend`. | L |
| Transforms, filters | partial | `transform` is in the `CSS.supports` allow-list. Filters are not. Paint is Vello CPU via Blitz. | Decorated UI. | M to measure with a reftest |
| `@font-face` and web fonts | partial | F-011 says fonts load. No subset test for unicode-range or font-display. | Branded pages fall back to a system font. | S to measure |
| Media queries in CSS | partial | Viewport is real (`src/headless.rs` 1100×760). `scripting` is patched by hand (F-053) because Stylo does not apply Blitz's `@media (scripting)` rule. | Print and scripting media. | S |
| Container queries, anchor positioning, scroll-driven animations, view transitions | missing | Interop 2026 items. No Glide code. | New CSS. Pages still render without them. | XL |
| `:defined` | wrong | F-073. Stylo reports every element undefined, so Glide injects a UA `visibility:visible !important` rule. | Custom-element shells. Fixing upgrade (plan 3) has to fix `:defined` or the UA hammer hides real `visibility` bugs. | part of plan 3 |

### Rendering and media

| Gap | Status | Evidence | Who notices | Effort |
| --- | --- | --- | --- | --- |
| Raster images | partial | Blitz paint. PNG is what Glide itself encodes (`src/shot.rs`). Other formats are whatever Blitz decodes. | Photos. | S to list supported decoders and add fixtures |
| SVG | partial | `blitz-dom` and `blitz-paint` are built with the `svg` feature (`Cargo.toml`). Domino has an SVG module. No Glide SVG reftest. | Icons, charts. | M |
| `<canvas>` 2D | missing | Blitz treats canvas as a replaced box (`blitz-dom` `layout/replaced.rs`). No `CanvasRenderingContext2D`. | Charts, captchas, some editors. | XL |
| WebGL, WebGPU | missing | No context. | Maps, Figma, games. | XL |
| `<video>`, `<audio>` | missing | Layout box only (`local_name!("video")` in Blitz replaced layout). No media player. | YouTube, any clip. | XL |
| `contenteditable` and rich text | missing | Text inputs in the page iframe receive keys (F-007). A contenteditable region is not an editing host. | Docs, comment boxes. | XL |

### Forms, history, frames, security, accessibility

| Gap | Status | Evidence | Who notices | Effort |
| --- | --- | --- | --- | --- |
| Form GET, `requestSubmit`, `onsubmit`, `location.assign` | works | F-072. Headless follows up to five script navigations. | Search forms that use GET. | — |
| Form POST, `multipart`, file fields | missing | F-072 note: Blitz iframe navigation is GET, and `file:` actions are not followed. | Login forms. | L (plan 4) |
| `input.value` / `checked` read-back from the user | missing | F-039 deferred. Script-set values can move to Blitz; typing in the control does not move back to domino. | Anything that reads the field in `onsubmit`. | M (plan 4) |
| Constraint validation (`checkValidity`, `pattern`, types other than text) | missing | No validation code in `src/js`. | Signup forms. | M |
| History API (`pushState`, `replaceState`, `popstate`) | partial | F-056. The shim updates the JS URL and fires `popstate`. The browser back stack is `src/history.rs` (F-003). The two stacks are not one stack. | Client-side routers. The URL in the address bar and the back button can disagree. | M |
| Hash routing | partial | If the location setter goes through F-072, a hash change can navigate. No dedicated `hashchange` test. | Older SPAs. | S |
| Page iframes (nested documents) | partial | The chrome hosts one page iframe. Blitz supports subdocuments. Glide does not build a JS `window` per nested frame or bridge `postMessage`. | Ads, embedded players, sandbox previews. | L |
| `window.open`, `target=_blank` | partial | Tabs exist (F-044). A page `window.open` is not wired to "open a tab". Modified clicks on links are (tabs tests). | "Open in new window" from script. | M |
| `postMessage` | missing | Not defined. | Frames and popups. | M, after frames |
| Same-origin and mixed content | missing | One process, one cookie jar, fetches allowed to any URL the process can reach. `file://` pages can `fetch` `https://` if the call is made. | Security boundary of a real browser. | L, with CORS |
| Sandbox (`iframe sandbox`, CSP sandbox) | missing | No flag plumbing. | Embeds. | L |
| Accessibility tree, ATK/NSAccessibility | missing | No accessibility backend in `src/`. Hit testing exists. | Screen readers. | XL |

### DevTools and MCP

DevTools (F-059 through F-065) inspects the page we already render. It does not fill platform gaps. MCP tools (`glide_navigate`, `glide_dom`, `glide_eval`, `glide_computed_style`, and the planned console and network tools) are the agent view of the same engine. A new web feature gets an MCP column of `n/a (observable via glide_eval / glide_snapshot_text)` unless the agent must drive a new action (a socket, a file upload, a popup). That matches `AGENTS.md`.

## Priority

Rank is impact on real pages, then effort, then what has to land first.

P0 is "a normal site is blank, stuck, or cannot log in". P1 is "the site paints, and a major interaction is wrong". P2 is "modern CSS and storage for apps we can already read". P3 is "media, GPU, and the service-worker platform".

Dependency order inside P0: the cookie jar is independent. Module loading and custom elements are independent of each other and both unblock JS-heavy sites. Form POST needs the control-value mirror, and it should share the cookie jar so a login POST stores the session. WebSocket should share that jar and the network log, so it comes after the cookie plan even though the code can start in parallel.

### P0

1. **`document.cookie` shares the navigation cookie jar** (F-058). Logins and the Google interstitial never proceed without it. The jar already exists. M.
2. **Module import graph and `import()`** (finish F-074). A single `import` drops the whole module. L.
3. **Custom element upgrade** (`connectedCallback`, attribute changes, disconnection) and a true `:defined`. Server HTML stays stuck. L.
4. **Form values and POST**. Script cannot read what the user typed, and POST does not navigate. L.
5. **WebSocket**. Live apps have no channel. L. After the cookie jar so the handshake can send `Cookie`.

### P1

6. Shadow DOM and slots. Custom elements that render into a shadow root still paint nothing useful after plan 3. L.
7. `localStorage` per origin, persisted; `sessionStorage` per document, memory only. M.
8. `FormData` and multipart `fetch` bodies. M. Shares plan 4's body encoder.
9. One history stack. `pushState` and the toolbar back button must move together. M.
10. `matchMedia` reports the viewport and `prefers-color-scheme`. S.
11. CORS and a same-origin check on `fetch`, XHR, and module loads. L. Security, and it changes who we can call.
12. Range and Selection. M. Moves Acid3 and unblocks "copy the selection" beyond F-006's browser shortcut.
13. CSS reftest burn-down (F-030), one cluster at a time, starting with the failures already vendored. L overall, S per cluster.
14. Nested iframes and `postMessage`. L.
15. `structuredClone`. M. Small, and `postMessage` wants it.
16. Abort in-flight `fetch`. M.
17. Hash change and `window.open` into a new tab. M.

### P2

18. IndexedDB on a Rust-native store (see crates below). XL.
19. EventSource. M.
20. CSS animations and transitions tied to rAF. L.
21. `:has`, container queries. L each, after the reftest burn-down shows Stylo's status.
22. Canvas 2D. XL.
23. CSP parser that at least enforces `script-src`. L.
24. Cookie persistence across restart (F-026). M, after plan 1.
25. `crypto.subtle` for SHA-256 and HMAC only. L.
26. Import maps. M, after plan 2.
27. Constraint validation. M.
28. Blob URLs that `<img>` and `fetch` can load. M.

### P3

29. Service workers and Cache API. XL.
30. `<video>` and `<audio>`. XL.
31. WebGL or WebGPU. XL. Do not start both.
32. `contenteditable`. XL.
33. Workers and worklets. XL.
34. HTTP/3. L, low user impact while h2 works.
35. Accessibility tree. XL.
36. Interop 2026 visual features (anchor positioning, view transitions, scroll-driven animations). XL.
37. Notifications, WebRTC, WebTransport. XL. Out of scope for a document browser.

## Top five plans

Each plan is a future `feat/` branch. It adds or updates a `docs/features.md` row, a `tests/features/F-xxx-*.feature` file, unit tests, and a manual row only where a real window is required. No `unwrap` or `expect` outside tests. No `unsafe` outside `src/js/`. New logs use `crate::logging` and the existing field names (`page_url`, `feature`, `detail`, `error_kind`, `hint`). Do not log cookie values, tokens, or script bodies.

`AGENTS.md` forbids the public network in tests. `src/http.rs` tests already bind `127.0.0.1:0`. Protocol tests may do that. Gherkin fixtures for pages stay `file://` and `data:`.

### 1. Cookie jar and `document.cookie` (F-058)

**Design.** One `cookie_store::CookieStore` owned by the network side, behind a mutex. Document loads, subresource loads, `fetch`, and XHR read and write that store. `document.cookie` is a getter and setter on the page document that sends a small RPC to the UI thread, which updates the same store. `Set-Cookie` stays hidden from script-visible headers (already true in `src/js/apply.rs`). The setter parses one cookie-attribute string. The getter returns the `name=value` serialization for the document URL, not `HttpOnly` cookies. Persistence across process restart stays F-026 and is not this branch.

**Threads.** The store lives where `src/http.rs` already runs, on the tokio side, with a synchronous mutex so the UI thread can serve `document.cookie` without awaiting. The glide-js worker never touches the store. It sends `cookie-get` / `cookie-set` DOM RPCs and applies the string result. That matches F-055: the UI thread dispatches network work.

**Files.** `src/http.rs` (stop building an opaque reqwest jar; install the shared store), `src/js/apply.rs` (two RPC variants), `src/js/bootstrap.js` (`document.cookie`), `src/js/dom-glue.js` if the accessor has to sit on domino's document, `docs/features.md` F-058.

**Crates.** No new crate. `cookie` 0.18.2 and `cookie_store` 0.22.1 are already in `Cargo.lock` because reqwest 0.13.5 depends on them. `cookie` is the Play Framework / Sergio Benitez cookie crate, widely used, pure Rust. Use those versions. If reqwest 0.13.5's `ClientBuilder` accepts a `cookie_store::CookieStore` handle, use that. If it does not, set the `Cookie` request header from the store inside `src/http.rs` and record `Set-Cookie` from the response there. Do not add a second parser.

Alternative considered: `tough-cookie` ports and parsing in JavaScript. Rejected. The wire jar and the script jar would diverge, which is the bug F-058 describes.

**MCP.** `n/a (engine behavior; observable via glide_eval)`. Do not add a tool that returns cookie values. `glide_navigate` already returns the final URL.

**Tests.** Unit: parse `document.cookie` setter strings, reject `HttpOnly` in the getter, scope by URL host and path, ignore a cookie for another host. Use recorded header strings, not a public server. One `http.rs`-style loopback on `127.0.0.1:0` that replies `Set-Cookie` and checks the next request's `Cookie` header. One `data:` or `file://` page that sets `document.cookie` and reads it back through the JS integration harness. Gherkin: "a script cookie is sent on the next same-origin fetch" and "an HttpOnly cookie is not visible to script". Manual: none.

**Size.** M. About one feature branch. The risky part is sharing the store with reqwest without cloning it per request.

### 2. Module graph and dynamic `import()`

**Design.** `resolve_module` in `src/js/worker.rs` currently returns `None`, and instantiation is abandoned. Replace that with a resolver that:

- resolves the specifier against the referrer's URL (`url` crate, already a dependency),
- fetches `http`, `https`, `file`, and `data` through the existing script channel (`src/js/scripts.rs`, tokio, not the JS thread),
- compiles each module once and caches by absolute URL in the worker,
- rejects a cycle and a failed fetch with a logged `js.unsupported` or `js.loader` record that includes `page_url`, `script`, and `hint`,
- implements dynamic `import()` by the same fetch path, queued as a task so it does not run inside `resolve_module`'s V8 callback.

Import maps stay P2. A graph that imports a bare specifier (`import 'react'`) fails with a clear unsupported record until import maps exist.

**Threads.** Fetch on tokio, same as F-074 external scripts. Compile and evaluate only on the glide-js worker. The UI thread only applies DOM RPCs that the module produces. Do not block the UI thread on a module download.

**Files.** `src/js/worker.rs` (`resolve_module`, module cache), `src/js/scripts.rs` (specifier resolution, a pure function), `src/js/apply.rs` only if a new fetch RPC is required (prefer the existing host fetch). F-074's row in `docs/features.md` changes from "done" to a note, and a new row F-076 (or the next free id) covers the graph. Update the stale sentence in `src/custom_elements.rs` that says modules do not run.

**Crates.** None. V8 already compiles modules. `url` 2 is already a direct dependency.

Alternative considered: a JavaScript import bundler in the browser, or `deno_core`'s module loader. Rejected. `deno_core` pulls an extension system we do not want, and the loader we need is a URL fetch plus V8's `instantiate_module`.

**MCP.** `n/a`. `glide_eval` and `glide_snapshot_text` see the side effects. `js.unsupported` with `feature=module-import` remains the diagnostic until the graph loads.

**Tests.** Fixtures under `tests/fixtures/`: an HTML page with `<script type="module" src="...">` and a sibling module file that imports `./dep.js`. `file://` only. A `data:` URL module that imports another `data:` URL. A fixture whose import 404s and asserts the `js.loader` record (the diagnostics subprocess pattern). A dynamic `import()` from a classic script. Gherkin scenarios for "a relative import runs", "a missing import logs and does not hang", and "dynamic import sees a binding". No public CDN.

**Size.** L. The cache and the "do not compile on the UI thread" rule are the whole design. Specifier resolution is a pure function with table tests.

### 3. Custom element upgrade

**Design.** When the parser or a script creates an element whose name is in `customElements`, construct the class, call `connectedCallback` when it is connected, `disconnectedCallback` when it is removed, and `attributeChangedCallback` for observed attributes. `upgrade(root)` walks a subtree. Construction runs on the glide-js worker, inside the existing task batch, so the constructor's DOM writes go through the F-036 mirror.

Do not run the constructor during Stylo's style pass. The UA `visibility` hammer in `src/custom_elements.rs` (`show_unupgraded`) stays until `:defined` is true for upgraded elements and false otherwise. Then delete that hammer in a follow-up if reftests and the Reddit fixture still paint. Doing both in one branch hides regressions.

`src/custom_elements.rs` keeps the streamed `<template for>` swap. That path is for markup that arrives before the definition. Once the definition runs, `connectedCallback` may also swap; the Rust swap must be idempotent.

**Threads.** All of this is on the glide-js worker. The UI thread only applies the mutation batch. No new network.

**Files.** `src/js/bootstrap.js` (registry and reaction queue), `src/js/dom-glue.js` (creation and connect hooks on domino), `src/custom_elements.rs` (stop claiming modules never run; keep the visibility rule for this branch), `docs/features.md` new row, and a fixture copied from the F-073 idea without depending on Reddit's network.

**Crates.** None.

Alternative considered: the JavaScript `custom-elements` polyfill (webcomponents/polyfills). Rejected as a runtime dependency. It expects a full native DOM and `MutationObserver` `oldValue`, which domino does not provide (F-038). A small reaction queue in `dom-glue.js` matches the mirror we have.

**MCP.** `n/a (observable via glide_dom and glide_snapshot_text)`.

**Tests.** `file://` fixture: define `x-greet`, append it, expect `connectedCallback` to set `textContent`. Remove it, expect `disconnectedCallback`. Change an observed attribute. Call `upgrade` on a server-rendered `<x-greet>` that was parsed before `define`. Assert `:defined` styling with a dump of computed `display` or text, not a screenshot, unless the visibility hammer is removed in the same branch. Gherkin for those four cases. Manual: none if the fixture covers the text.

**Size.** L. The reaction queue and the "constructor runs inside the task, not re-entrantly" rule are the risk. Domino's element factory is the file to extend, not a new DOM.

### 4. Form values and POST navigation

**Design.** Two pieces.

Value mirror: on submit, and when script reads `value` or `checked`, the UI thread copies the Blitz control's current value into the domino node before the script or the submit event runs. Script writes continue to flow Blitz-ward through the existing mutation batch (F-039's missing reverse direction).

POST: when a form's method is POST, build an `application/x-www-form-urlencoded` body from the controls and navigate with that body. Multipart and file fields wait for `FormData` (P1) unless the branch is already touching the encoder; URL-encoded POST is enough for a password form. `file:` actions stay rejected (F-072). The navigation uses `src/http.rs` so redirects and the cookie jar apply. The address bar shows the final URL. History records the POST as a non-replayable entry: back returns to the form page and does not POST again.

**Threads.** Value read is UI-thread, then a DOM RPC into the worker before the submit listener. The POST itself is tokio, same as other document loads. The worker does not open a socket.

**Files.** `src/js/dom-glue.js` (form submit already patched for F-072), `src/js/apply.rs` (submit RPC carries method and body), `src/http.rs` (POST document fetch), `src/history.rs` (do not replay POST), `docs/features.md` extending F-039 and F-072 or a new row if the scope is clearer as F-077.

**Crates.** None. `url` encoding can use `url` or `form_urlencoded` if it is already in the lockfile via `url`. Check `Cargo.lock` before adding anything. A new crate is not justified for percent-encoding.

**MCP.** `glide_navigate` stays the agent entry. No new tool. A test can `glide_eval` a submit only after the value mirror exists; until then the headless harness sets the value through the same RPC the UI uses.

**Tests.** Unit: encode controls, skip disabled fields, include the submitter name. `data:` document with a form whose `action` is `http://127.0.0.1:<port>/login` using the `http.rs` loopback pattern; the server reads the body and responds 200. Assert the dump contains the response text and that back does not POST again. A separate test that the typed value is what script sees: set the Blitz value through the test harness, read `input.value` in JS. Gherkin: "POST body reaches the action" and "back does not resubmit". Manual row only for "typing in the window and pressing Enter", because synthetic Blitz values do not prove keyboard input (M-new, next free id in `docs/manual-tests.md`).

**Size.** L. The history rule (do not replay POST) is the part that will corrupt session history if it is wrong.

### 5. WebSocket

**Design.** `new WebSocket(url)` on the page. Support `ws:` and `wss:` only. `blob` binary type can wait; `arraybuffer` is enough. Events: `open`, `message`, `error`, `close`. `send` accepts a string or an `ArrayBuffer`. `close(code, reason)` sends a close frame. Ready state is the spec enum.

The handshake and frames run on tokio. The worker holds the JS object and a channel of events. Incoming messages are queued and delivered on the next JS pump, not on the network thread. A per-page cap (for example 16 sockets and 1 MiB of queued message bytes) drops the oldest message and logs `js.unsupported` or a new `js.net` target with `feature=websocket`, `page_url`, and `hint`. Do not log the payload.

Cookie: the handshake request includes `Cookie` from the store in plan 1. If plan 1 has not landed, ship WebSocket without cookies and say so in the feature note rather than reading reqwest's private jar.

Network tab: append a row to the F-065 log when the handshake completes or fails (URL, status, no payload).

**Threads.** Tokio task per socket. UI thread only moves bytes between the task and the worker queue, the same way F-055 delivers `fetch`. The glide-js worker runs event listeners. Closing the page (`reset_page_js_state`) aborts the task.

**Files.** `src/js/bootstrap.js` (`WebSocket` class), `src/js/apply.rs` or a small `src/js/websocket.rs` for the RPC, `src/net.rs` for the task, `src/devtools/network.rs` for the row, `docs/features.md` new row.

**Crate.** `tokio-tungstenite` 0.30.0 (published 2026-07-11, MIT, MSRV 1.85, crates.io shows on the order of 3×10^8 downloads and thousands of dependents, maintained by Daniel Abramov and Alexey Galakhov at snapview). The tokio wrapper has no `unsafe` of its own; `tungstenite` may use `unsafe` internally for UTF-8, which stays inside the dependency. Our code does not add `unsafe`. Features: default plus whatever `tokio` we already enable (`rt-multi-thread` is already on).

Justification for the new dependency, to put in the commit message: WebSocket framing and the HTTP upgrade are easy to get wrong, and this crate is the maintained tokio binding already used across the ecosystem.

Alternative considered: `soketto` (Rust, less used) and writing the upgrade by hand on `hyper` (already in the tree via reqwest). Hand-rolling fails the "mature library" rule. `soketto` is smaller but has a much thinner user base. `tungstenite` without tokio would block a runtime thread.

**MCP.** `n/a` for the socket itself. The network row is visible later through planned `glide_network_requests` (F-071). Do not add a tool that reads socket payloads.

**Tests.** In-process: a `tokio-tungstenite` server on `127.0.0.1:0`, the same loopback shape as `src/http.rs` tests. Script opens a socket, sends `hello`, receives `hello`, closes. A refused port fires `error` and `close` and does not hang the worker (the F-008 watchdog rule). A navigation mid-socket does not leave the task running (assert the server sees the TCP close). Gherkin for those three. No public echo server. Manual: none.

**Size.** L. Most of the code is the queue and the shutdown path, not the frame parser.

## What this document does not ask for

No implementation in the branch that adds this file. No merge to `main`. No Speedometer run. No new dependency until the feature branch that names it in the commit message. F-032's "remaining gaps" sentence in `docs/features.md` is stale and should be edited when the probe is regenerated, not in this research commit.
