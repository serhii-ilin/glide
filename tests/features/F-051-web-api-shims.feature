Feature: F-051 Common Web API compatibility shims
  As a page author
  I want common Web APIs available in page scripts
  So that third-party scripts do not crash on missing globals

  # Source / basis: HTML Living Standard (base64 utilities, Window, storage);
  #                DOM Standard (MediaQueryList, AbortSignal, MutationObserver);
  #                product decision: in-memory/no-op shims where full support is deferred
  # Registry: docs/features.md -> F-051
  # Tests: headless_js::common_web_api_shims_work,
  #        diagnostics::glide_log_warn_suppresses_informational_console_messages, manual M-038

  Background:
    Given a local page that runs page JavaScript

  Scenario: Base64 helpers are available
    When a script calls atob and btoa
    Then the decoded and encoded strings follow the standard base64 alphabet

  Scenario: Media queries return a non-matching list
    When a script calls matchMedia("(min-width: 1px)")
    Then it receives a MediaQueryList-like object with matches set to false
    And listener registration does not throw

  Scenario: Console info and debug do not throw
    When a script calls console.info and console.debug
    Then both calls succeed
    And both are logged through the browser console bridge

  Scenario: Storage APIs work in memory
    When a script uses localStorage and sessionStorage
    Then getItem returns the value previously passed to setItem
    And removeItem and clear remove entries

  Scenario: AbortController exposes an abort signal
    When a script constructs an AbortController and calls abort
    Then signal.aborted is true
    And registered abort listeners run once

  Scenario: MutationObserver can be constructed without throwing
    When a script constructs a MutationObserver and calls observe
    Then neither call throws
    And takeRecords returns an empty array

  Scenario: Data attributes expose a DOMStringMap
    When a script sets element.dataset.fooBar to "baz"
    Then the element has data-foo-bar="baz"
    And element.dataset is an instance of window.DOMStringMap

  # Added with F-072: Reddit's JavaScript challenge iterates its own query string.
  Scenario: URLSearchParams iterates and mutates pairs
    When a script parses URLSearchParams from a query string
    Then forEach and entries visit the pairs in order
    And set and delete update the pair list
    And size and has reflect the change
    And the object is iterable
