Feature: F-056 Performance timing, history state and crypto
  As a page author building an app shell
  I want performance timing, history state APIs and Web Crypto randomness
  So that analytics, client-side routing and secure ids work without silent fallback

  # Source / basis: Performance Timeline / Navigation Timing Level 2;
  #                HTML Living Standard (history API);
  #                Web Crypto API (getRandomValues, randomUUID)
  # Registry: docs/features.md -> F-056
  # Tests: js::integration::performance_timing_and_entries_work,
  #        js::integration::history_push_state_updates_location_and_state,
  #        js::integration::crypto_get_random_values_and_uuid_work,
  #        headless_js::timing_history_crypto_work,
  #        headless_js::navigator_languages_work

  Background:
    Given a local page that runs page JavaScript

  Scenario: Performance exposes timing, navigation and user entries
    When a script reads performance.timeOrigin and performance.timing.navigationStart
    Then navigationStart is truthy and navigation.type is navigate
    And getEntriesByType("navigation") returns one entry
    And mark and measure add named performance entries

  Scenario: History pushState updates the visible URL without chrome navigation
    When a script calls pushState twice then replaceState on the same origin
    Then location, document.URL and history.state match the current entry
    And history.back dispatches popstate with the previous state
    And a cross-origin pushState throws SecurityError without changing the URL

  Scenario: Crypto fills typed arrays and returns version 4 UUIDs
    When a script calls getRandomValues on a Uint8Array and randomUUID twice
    Then the array is filled with non-zero bytes and the same object is returned
    And requests above the byte quota throw QuotaExceededError
    And each UUID matches the version 4 UUID pattern and values differ

  Scenario: Navigator exposes language and concurrency hints
    When a script reads navigator.languages and navigator.language
    Then languages lists en-US and en and language is en-US