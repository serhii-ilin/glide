Feature: intersection-observer Real IntersectionObserver and native lazy images
  Page scripts need intersection callbacks when elements enter or leave the viewport,
  including after scroll, layout changes and DOM updates, and native lazy images should
  fetch only when they are near the viewport.
  Source / basis: Intersection Observer; HTML Living Standard (lazy loading attributes)

  # Registry: docs/features/intersection-observer.md

  # Tests: js::integration::{
  #          intersection_observer_fires_after_dom_moves_target_into_view,
  #          intersection_observer_fires_for_visible_and_hidden_elements,
  #          intersection_observer_threshold_crossing_fires,
  #          intersection_observer_reports_zero_size_target_inside_viewport,
  #          intersection_observer_zero_size_sentinel_fires_after_scroll,
  #          intersection_observer_take_records_buffers_entries,
  #          lazy_img_fetches_only_after_near_viewport_layout_change
  #        }

  Scenario: IntersectionObserver reports visibility after a layout change moves a target into view
    Given a page with a target below the viewport in a small window
    When page layout moves the target into the viewport
    Then the observer callback runs with isIntersecting true

  Scenario: IntersectionObserver exposes entry fields for visible and hidden targets
  # Source: Intersection Observer, 'compute the intersection' (zero-area targets intersect when
  # their edge-adjacent rectangle lies inside the root; intersectionRatio is 1) — Reddit's
  # next-page sentinel is a zero-size element.
  Scenario: A zero-size target inside the viewport intersects with ratio 1
    Given a page observes an element with width and height 0 that sits inside the viewport
    When the observer callback runs
    Then isIntersecting is true and intersectionRatio is 1

  Scenario: A zero-size sentinel fires when scrolling brings it into view
    Given a page observes a zero-size sentinel below the viewport
    When the viewport scrolls so the sentinel is inside it
    Then the callback runs with isIntersecting true

  Scenario: IntersectionObserver honors threshold crossings
  Scenario: takeRecords drains queued entries without waiting for the callback
  Scenario: A loading=lazy image requests its src after moving near the viewport