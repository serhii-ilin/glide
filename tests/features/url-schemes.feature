Feature: url-schemes URL schemes (file, data, http, about)
  As a user
  I want built-in and local URLs to load without the network stack
  So that blank tabs and local fixtures work like in other browsers

  # Source / basis: HTML Living Standard (about:blank), WHATWG URL (about scheme);
  #                product: new tabs default to about:blank
  # Registry: docs/features/url-schemes.md
  # Tests: net::tests::about_blank_delivers_empty_document,
  #        net::tests::unsupported_about_urls_serve_local_error_page_without_provider,
  #        headless::tests::fetch_about_blank_without_network,
  #        headless::tests::fetch_unsupported_about_urls_local_error_without_provider,
  #        tabs::tests::new_tab_starts_on_about_blank,
  #        control_session::navigate_about_blank_reports_empty_document
  # MCP: glide_navigate with about:blank

  Background:
    Given Glide is asked to load the URL about:blank

  Scenario: Navigating to about:blank shows an empty document
    When the page bytes are delivered
    Then the document is an empty HTML page
    And no network fetch is attempted

  Scenario: A new tab uses about:blank locally
    When the user opens a new tab
    Then the active tab URL is about:blank
    And loading about:blank does not use the HTTP stack
