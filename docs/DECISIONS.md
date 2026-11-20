# Glide implementation decisions

## V8 script timeout vs. vendored `testharness.js` (2026-11-20)

**Question:** Should `run_script_with_timeout` stay at 5 s for a ~198 KiB `testharness.js`?

**Measurement:** With `GLIDE_MEASURE_SCRIPTS=1`, evaluating vendored `testharness.js` (`script-0`, 198 420 bytes) takes about **0.001 s** on a debug build after domino bootstrap (no RPC during snapshot; mutation mirroring deferred until the tree is built).

**Decision:** Keep the **5 s** watchdog (`50 × 100 ms`). The earlier 30 s raise was unnecessary; slowness came from mirroring every domino mutation during snapshot construction (synchronous RPC per node), not from JS parse/compile.

**Follow-up:** Mutation mirroring stays off until `__glide_init_page` finishes the snapshot. Runtime mirroring uses a synchronous `scheduleFlush()` (RPC batch per mutation batch); disabling mirroring during bootstrap avoids flooding the UI thread while the domino tree is built.

## Page `globalThis` is the window (2026-11-20)

**Decision:** Merge domino’s `Window` prototype onto V8’s `globalThis` (`Object.setPrototypeOf`), set `document.defaultView === globalThis`, and bind `addEventListener` / `dispatchEvent` to `globalThis`. Vendored WPT `testharness.js` stays at `})(self);` unchanged from upstream.