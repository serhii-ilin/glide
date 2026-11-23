Feature: F-081 Origin-scoped persistent Web Storage
  As a page author
  I want localStorage and sessionStorage to follow the Web Storage specification
  So that client-side state survives navigation and restart where the spec requires

  # Source / basis: HTML Living Standard (Web Storage); product decision: JSON profile persistence (Turso/SQLite considered overkill)
  # Registry: docs/features.md -> F-081
  # Tests: web_storage::tests::*, js::integration::web_storage_*, js::storage_events::tests::*, headless_js::local_storage_persists_in_profile_across_headless_loads, manual M-060

  Background:
    Given a page loaded over HTTP on 127.0.0.1 or https://example.test

  Scenario: localStorage is partitioned by tuple origin
    When a script calls localStorage.setItem on one path of the origin
    Then localStorage.getItem on another path of the same origin returns the value

  Scenario: sessionStorage survives same-tab navigation
    When a script sets sessionStorage on the first document of a tab
    And the tab navigates to another same-origin document
    Then sessionStorage still exposes the value

  Scenario: data: URLs use a non-persistent per-document opaque store
    When a script sets localStorage on a data: document
    And a new document generation loads another data: URL in the same tab
    Then the previous key is not present

  Scenario: localStorage enforces a per-origin quota
    When a script sets localStorage to more than about five mebibytes for an origin
    Then setItem throws QuotaExceededError

  Scenario: Storage supports the standard API surface
    When a script uses getItem, setItem, removeItem, clear, key, length and property access
    Then each operation behaves per the Web Storage spec

  Scenario: localStorage persists to the profile directory
    When localStorage is written under a temporary profile directory
  And the browser process loads storage again from that profile
    Then the value is still readable

  Scenario: storage events reach another same-origin document
    When two tuple-origin documents are registered
    And one document changes localStorage
    Then the other document receives a storage event