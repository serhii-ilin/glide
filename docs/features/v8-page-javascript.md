---
slug: "v8-page-javascript"
kind: "feature"
title: "V8 page JavaScript: worker thread, DOM RPC, timers, rAF, fetch, events"
status: "done"
aliases:
  [
    "F-008"
  ]
requirements:
  [
    "run-page-javascript-allow-turning"
  ]
gherkin: "tests/features/v8-page-javascript.feature"
tests:
  [
    "`js::apply::tests::*`",
    "`js::integration::*`",
    "`js::scripts::tests::*`",
    "`js::host::tests::{stale_dom_rpc_is_rejected,current_dom_rpc_is_applied,maybe_start_without_navigation_bumps_generation,page_with_timer_reports_next_wake,page_without_timers_reports_no_wake,settimeout_zero_reports_immediate_wake,settimeout_negative_delay_reports_immediate_wake,settimeout_non_numeric_and_infinite_delays_stay_numeric,worker_wake_on_dom_rpc_without_host_pump,earlier_timer_shortens_next_wake_and_wakes_the_host}`",
    "`js::integration::timers_fixture_reports_wake_and_advances_with_pump`",
    "`app::tests::pump_wait_*`",
    "manual v8-page-javascript#1",
    "v8-page-javascript#2"
  ]
mcp: "planned mcp-console-eval (`glide_eval`)"
manual:
  [
    {
      "steps": "Open `tests/fixtures/timers.html` in a window with page JavaScript enabled. Do not move the mouse, type, or reload for at least two seconds",
      "expected": "`#tick` advances above zero and `document.title` shows `tick:` with a rising count; `#timeout` becomes `fired` within about one second without user input",
      "aliases": [
        "M-051"
      ]
    },
    {
      "steps": "Open `tests/fixtures/timers.html`, wait until the tick advances, then navigate away via the address bar, a link click, or a page `location.assign` (for example to `tests/fixtures/counter.html`). Watch stderr and DevTools Console",
      "expected": "No `dom channel closed` error and no stale `dynamic-script` or retired-worker console error from the timer page after navigation",
      "aliases": [
        "M-052"
      ]
    }
  ]
---

Page scripts run on V8 against a domino DOM, with timers, animation frames, fetch and events mirrored to Blitz. Timers keep firing while the window is idle; retiring a page rejects stale DOM RPCs and suppresses diagnostics from the old worker. A new page without an explicit navigation still bumps the DOM generation. The worker wakes the window for DOM RPCs, earlier timer deadlines, and console/diagnostic output.

## Notes

One DOM RPC channel per host with a generation counter; `__glide_next_wake` uses `-1` for idle; full timer deadlines arm winit `WaitUntil` (no 250 ms cap).
