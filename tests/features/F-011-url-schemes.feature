Feature: F-011 URL schemes (file, data, http, about)
  As a user
  I want built-in and local URLs to load without the network stack
  So that blank tabs and local fixtures work like in other browsers

  # Source / basis: HTML Living Standard (about:blank), WHATWG URL (about scheme);
  #                product: new tabs default to about:blank
  # Registry: docs/features.md -> F-011
  # Tests: net::tests::about_blank_delivers_empty_document,
  #        headless::tests::fetch_about_blank_without_network,
  #        tabs::tests::new_tab_starts_on_about_blank
  # MCP: glide_navigate with about:blank

  Scenario: Navigating to about:blank shows an empty document
    Given Glide is asked to load the URL about:blank
    When the page bytes are delivered
    Then the document is an empty HTML page
    And no network fetch is attempted

  Scenario: A new tab uses about:blank locally
  When the user opens a new tab
  Then the active tab URL is about:blank
  And loading about:blank does not use the HTTP stack