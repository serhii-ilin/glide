---
slug: "mcp-scroll"
kind: "feature"
title: "Headless scroll, scroll events and infinite scroll over MCP"
status: "done"
aliases: []
requirements:
  [
    "headless-scroll-observable-by-page-scripts",
    "screenshot-honors-scroll-offset"
  ]
gherkin: "tests/features/mcp-scroll.feature"
tests:
  [
    "`control_session::{scroll_by_delta_moves_viewport_and_reports_offsets,scroll_is_clamped_to_document_extent,scroll_to_selector_brings_element_into_view,scroll_to_missing_selector_is_error,scroll_without_page_is_error,scroll_moves_viewport_when_javascript_is_disabled,page_script_sees_viewport_and_scroll_events_after_scroll,screenshot_honors_scroll_offset,full_page_screenshot_covers_document_height_and_keeps_scroll,infinite_scroll_intersection_observer_appends_items_after_scroll,infinite_scroll_scroll_event_appends_items_after_scroll,scroll_waits_for_slow_page_fetch_started_by_scroll_handler,scroll_scroll_result_is_not_settled_while_page_mutates_forever}`",
    "`js::integration::{scroll_event_reaches_window_and_document_after_viewport_moves,window_scroll_properties_match_viewport_after_scroll,page_scroll_to_and_by_move_viewport_and_fire_scroll_events,intersection_observer_sentinel_fires_after_viewport_scroll,element_scroll_top_and_left_setters_scroll_overflow_container_and_clamp,viewport_reads_and_scroll_do_not_panic_after_document_element_is_removed,page_scroll_to_the_offset_the_host_just_set_fires_exactly_one_scroll_event}`; `js::viewport::tests::viewport_helpers_return_neutral_state_without_a_root_element`",
    "`control::page_ops::tests::{apply_scroll_clamps_and_reports_new_state,apply_scroll_rejects_non_finite_delta}`; `control::protocol::tests::{scroll_request_json_roundtrip_with_optional_fields,bare_screenshot_request_without_params_still_deserializes}`; `js::viewport::tests::scroll_to_selector_on_display_none_element_is_an_error`; `js::host::tests::page_scroll_offset_ignores_chrome_scroll_in_window_mode_without_a_page`; `mcp::tools::tests::scroll_info_formats_as_json_with_offsets_and_sizes`; `js::host` outstanding-fetch tracking via the slow-fetch tests",
    "`mcp_stdio::{scroll_then_snapshot_text_loads_more_items_over_stdio,scroll_waits_for_slow_fetch_and_reports_settled_over_stdio,scroll_to_missing_selector_is_tool_error_over_stdio,full_page_screenshot_is_taller_than_viewport_over_stdio,initialize_and_list_tools}`",
    "manual mcp-scroll#1"
  ]
mcp: "`glide_scroll`, `glide_screenshot` (`full_page`)"
manual:
  [
    {
      "steps": "Run `cargo run --release`, open `file://` `tests/fixtures/scroll-state.html` (a 3000 px page that prints scroll state into a fixed paragraph on each `scroll` event), then scroll with the mouse wheel and with Page Down; then open DevTools and run `scrollBy(0, 100); scrollY` in the console",
      "expected": "The paragraph updates after each wheel or key scroll with `win=` and `doc=` counts increasing by one per scroll turn, `y`/`pyo` equal to the page offset and `ih` equal to the window's page height; the console `scrollBy` returns the offset moved by 100 and fires one more scroll event",
      "aliases": []
    }
  ]
---

An agent can scroll the headless page by delta or to a CSS selector with `glide_scroll`; page scripts see the new `window.scrollY`, a `scroll` event and IntersectionObserver callbacks, so infinite-scroll pages append more content, and screenshots follow the scroll offset or capture the full page.

## Notes

One code path: `glide_scroll` maps to `ControlRequest::Scroll`, which moves the Blitz viewport offset through `js::viewport` (`BaseDocument::scroll_to`/`scroll_by` on the root element, the primitive the wheel uses; clamped to the document), then pumps the JS host with `headless::settle` (with `wait_for_fetches`) plus `flush_dom_rpc_until_fetches_idle` so scroll listeners, IntersectionObserver callbacks and the `fetch` they start finish before the result is returned. Page HTTP fetches bypass the Blitz `Provider`, so `JsHost` counts outstanding `DomRequest::Fetch` replies and the settle and flush loops treat them as not done. The whole call is bounded by `timeout_ms` (default 3000); the result carries `settled`, false when the page was still busy at the deadline (for example a `setInterval` that mutates forever) so the agent knows content may still be arriving. `JsHost::pump` compares the page viewport offset on every pump and tells the page runtime (`__glide_viewport_scrolled`) when it changed for any reason, so the window wheel and keyboard scrolling now also run page `scroll` events and IntersectionObserver checks; the runtime fires one `scroll` event per animation-frame turn at the document (bubbling to the window) and compares each new offset only with the last offset the page runtime saw, so a move reaching the page twice (host notice plus its own `scrollTo`) fires once. There is no live-window control backend yet (`remote-control-window`), so there is no "needs a live window" path; the window itself already scrolls by wheel (`wheel-scroll-target`) and gets the same events, which only the manual check covers. Page JS reads `scrollX/scrollY/pageXOffset/pageYOffset/innerWidth/innerHeight/outerWidth/outerHeight`, `document.scrollingElement` (domino), element `scrollWidth/scrollHeight/scrollTop/scrollLeft` (root element values are the document's; assigning root `scrollTop`/`scrollLeft` scrolls the viewport, and assigning on any other element scrolls it through the `ScrollElement` RPC, clamped to its overflow, so `el.scrollTop = el.scrollHeight` and `el.scrollTop += d` loops work), `scrollTo`/`scroll`/`scrollBy` with `(x, y)` or `{left, top}` (`behavior` is treated as instant) and `Element.scrollIntoView` (`block` honored, instant). `getBoundingClientRect` was already viewport-relative; the root element's `clientWidth`/`clientHeight` now report the viewport (they reported the document size, which made IntersectionObserver treat the whole document as visible). Layout-only RPCs (`boundingRect`, `computedStyle`, `layoutField`, `viewportInfo`) no longer count as page activity, so a page with an IntersectionObserver settles instead of timing out. `glide_screenshot` paints at the current offset; `full_page` resizes the viewport to the document height (capped at 16384 px), paints from the top, and restores size and offset (through a drop guard, so a panic while painting still restores them). `ControlRequest::Screenshot` now carries `ScreenshotParams` (it was a unit variant); a bare `{"method":"screenshot"}` request with no `params` still parses. A `to_selector` that matches a `display: none` element (or one inside such a subtree) is an error, since it has no layout box. The host only reports chrome-document scroll as page scroll for headless hosts (`JsHost::in_window` marks the window app). Known gap: CSS `scroll-behavior`/`behavior: smooth` animation is not run; nested scroll containers are scrolled by page script (`el.scrollTop`, `scrollIntoView`), not by `glide_scroll`, which only moves the viewport.
