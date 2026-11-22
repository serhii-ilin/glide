# Glide implementation decisions

## V8 script timeout vs. vendored `testharness.js` (2026-11-20)

**Question:** Should `run_script_with_timeout` stay at 5 s for a ~198 KiB `testharness.js`?

**Measurement:** With `GLIDE_MEASURE_SCRIPTS=1`, evaluating vendored `testharness.js` (`script-0`, 198 420 bytes) takes about **0.001 s** on a debug build after domino bootstrap (no RPC during snapshot; mutation mirroring deferred until the tree is built).

**Decision:** Keep the **5 s** watchdog (`50 × 100 ms`). The earlier 30 s raise was unnecessary; slowness came from mirroring every domino mutation during snapshot construction (synchronous RPC per node), not from JS parse/compile.

**Follow-up:** Mutation mirroring stays off until `__glide_init_page` finishes the snapshot. After that, domino records each mutation into one queue. The queue is flushed **once per task** — after a classic script, a timer callback, an animation-frame callback, an event dispatch, and the load / `DOMContentLoaded` firing — and also immediately before a layout read (`offsetWidth`, `getBoundingClientRect`, `getComputedStyle`). Nodes created in that queue carry a client id (`cid`) so later ops in the same batch can name them before Blitz has assigned a node id. One RPC round trip covers the whole task.

**Update (2026-11-22):** Console eval is another task that ends with a flush: `eval_console_expression` runs `settle_task` before the worker sends its reply, so an agent's next read sees the mutations and the REPL watchdog still bounds a runaway microtask chain.

## Page `globalThis` is the window (2026-11-20)

**Decision:** Merge domino’s `Window` prototype onto V8’s `globalThis` (`Object.setPrototypeOf`), set `document.defaultView === globalThis`, and bind `addEventListener` / `dispatchEvent` to `globalThis`. Vendored WPT `testharness.js` stays at `})(self);` unchanged from upstream.

## Click activation stays inside domino’s dispatch (2026-11-20)

**Decision:** A window bubble listener can cancel `dispatchEvent` (`preventDefault` makes it return false). Domino’s click activation (`HTMLElement._doClick` on the mouseup path) still runs inside `_dispatchEvent`, before that window bubble phase. A window listener cannot stop the activation that domino already performed.

## Agent control over MCP (2026-11-22)

**Decision:** Agents control Glide through the Model Context Protocol, using the official
Rust SDK (`rmcp`, stdio transport) rather than a hand-rolled JSON-RPC server. Tools are thin
adapters over a transport-agnostic `src/control` layer with two backends:

- **Headless session** (`glide --mcp`): a long-lived page on a dedicated `glide-session`
  thread (Blitz's blocking fetch cannot run inside a tokio task), never touching the user
  profile.
- **Live window** (`glide --remote-control`, then `glide --mcp --attach`): the window listens
  on `<profile>/run/remote-control.sock` (directory 0700, socket 0600). Like ssh-agent, file
  permissions are the authentication; there is no TCP listener and no token. Commands are
  queued for the UI thread's `sync()` and never block it; navigation replies resolve when the
  load finishes.

Eval results come back through a reply channel on the existing REPL `WorkerMessage::Eval`,
so DevTools and MCP share one evaluation path and its watchdog. Every feature row records
its MCP exposure (`MCP` column) so new behavior is designed for agents as well as people.
