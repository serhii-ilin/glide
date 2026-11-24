Feature: timing-history-crypto Performance timing, history state and crypto
  As a page author building an app shell
  I want performance timing, history state APIs and Web Crypto randomness
  So that analytics, client-side routing and secure ids work without silent fallback

  # Source / basis: Performance Timeline / Navigation Timing Level 2;
  #                HTML Living Standard (history API);
  #                Web Crypto API (getRandomValues, randomUUID)
  # Registry: docs/features/timing-history-crypto.md
  # Tests: js::integration::performance_timing_and_entries_work,
  #        js::integration::performance_timing_snapshot_is_stable,
  #        js::integration::history_push_state_updates_location_and_state,
  #        js::integration::history_back_restores_initial_url,
  #        js::integration::history_go_coercion_and_bounds,
  #        js::integration::history_onpopstate_property_fires,
  #        js::integration::history_forward_and_push_truncates_forward_stack,
  #        js::integration::history_location_aliases_match_after_push_state,
  #        js::integration::history_push_state_invalid_url_throws_syntax_error,
  #        js::integration::history_and_performance_buffers_are_bounded,
  #        js::integration::crypto_get_random_values_and_uuid_work,
  #        js::integration::crypto_rejects_non_integer_views,
  #        js::integration::navigator_host_hints_work,
  #        headless_js::timing_history_crypto_work,
  #        headless_js::navigator_languages_work

  Background:
    Given a local page that runs page JavaScript

  Scenario: Performance exposes timing, navigation and user entries
    When a script reads performance.timeOrigin and performance.timing.navigationStart
    Then navigationStart is truthy and navigation.type is navigate
    And getEntriesByType("navigation") returns one entry
    And mark and measure add named performance entries
    And timing and navigation entry snapshots stay stable across reads and pushState

  Scenario: History pushState updates the visible URL without chrome navigation
    When a script calls pushState twice then replaceState on the same origin
    Then location, document.URL and history.state match the current entry
    And history.back dispatches popstate and restores the original document URL
    And a cross-origin pushState throws SecurityError without changing the URL
    And go(), invalid deltas and forward truncation behave per the history API

  Scenario: Crypto fills typed arrays and returns version 4 UUIDs
    When a script calls getRandomValues on a Uint8Array and randomUUID twice
    Then the array is filled with non-zero bytes and the same object is returned
    And requests above the byte quota throw QuotaExceededError
    And each UUID matches the version 4 UUID pattern and values differ
    And BigUint64Array works while DataView and Float32Array throw TypeError

  Scenario: Navigator exposes language and concurrency hints
    When a script reads navigator.languages and navigator.language
    Then languages lists en-US and en and language is en-US
    And navigator.languages is frozen with a non-empty platform and hardwareConcurrency at least 1
