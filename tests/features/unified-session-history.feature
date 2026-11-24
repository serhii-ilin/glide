Feature: unified-session-history Unified session history stack
  As a user navigating a single-page app
  I want pushState entries on the same Rust history stack as full page loads
  So that chrome back/forward and history.back() stay in sync without reloading SPA entries

  # Source / basis: HTML Living Standard (session history and navigation);
  #                WHATWG HTML § traverse history / popstate / hashchange
  # Registry: docs/features/unified-session-history.md
  # Tests: nav::tests::chrome_back_after_push_state_is_same_document_traversal,
  #        js::integration::history_unified_stack_length_matches_rust,
  #        js::integration::history_hashchange_fires_on_fragment_only_push,
  #        headless_js::unified_history_fixture_works

  Background:
    Given a local page that runs page JavaScript

  Scenario: pushState entries share the chrome history stack
    When a script calls pushState and the Rust session history records the entry
    Then chrome back traverses the same-document entry with popstate instead of reloading

  Scenario: Fragment-only pushState fires hashchange
    When a script pushes a hash-only URL on the same origin
    Then hashchange fires and the session entry stays same-document

  Scenario: Cross-origin pushState still throws SecurityError
    When a script attempts pushState to another origin
    Then the URL is unchanged and the stack is not mutated