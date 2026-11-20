# Glide implementation decisions

## V8 script timeout vs. vendored `testharness.js` (2026-11-20)

**Question:** Should `run_script_with_timeout` stay at 5 s for a ~198 KiB `testharness.js`?

**Measurement:** With `GLIDE_MEASURE_SCRIPTS=1`, evaluating vendored `testharness.js` (`script-0`, 198 420 bytes) takes about **0.001 s** on a debug build after domino bootstrap (no RPC during snapshot; mutation mirroring deferred until the tree is built).

**Decision:** Keep the **5 s** watchdog (`50 × 100 ms`). The earlier 30 s raise was unnecessary; slowness came from mirroring every domino mutation during snapshot construction (synchronous RPC per node), not from JS parse/compile.

**Follow-up:** Mutation mirroring stays off until `__glide_init_page` finishes the snapshot. After that, domino records each mutation into one queue. The queue is flushed **once per task** — after a classic script, a timer callback, an animation-frame callback, an event dispatch, and the load / `DOMContentLoaded` firing — and also immediately before a layout read (`offsetWidth`, `getBoundingClientRect`, `getComputedStyle`). Nodes created in that queue carry a client id (`cid`) so later ops in the same batch can name them before Blitz has assigned a node id. One RPC round trip covers the whole task.

## Page `globalThis` is the window (2026-11-20)

**Decision:** Merge domino’s `Window` prototype onto V8’s `globalThis` (`Object.setPrototypeOf`), set `document.defaultView === globalThis`, and bind `addEventListener` / `dispatchEvent` to `globalThis`. Vendored WPT `testharness.js` stays at `})(self);` unchanged from upstream.