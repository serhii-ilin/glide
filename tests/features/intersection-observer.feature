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
  #          intersection_observer_take_records_buffers_entries,
  #          lazy_img_fetches_only_after_near_viewport_layout_change
  #        }

  Scenario: IntersectionObserver reports visibility after a layout change moves a target into view
    Given a page with a target below the viewport in a small window
    When page layout moves the target into the viewport
    Then the observer callback runs with isIntersecting true

  Scenario: IntersectionObserver exposes entry fields for visible and hidden targets
  Scenario: IntersectionObserver honors threshold crossings
  Scenario: takeRecords drains queued entries without waiting for the callback
  Scenario: A loading=lazy image requests its src after moving near the viewport