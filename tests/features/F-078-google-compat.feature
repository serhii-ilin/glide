Feature: F-078 Google Search compatibility
  Page navigations and scripts behave well enough for Google Search's client-side
  checks and POST flow to succeed in Glide.

  # Source / basis: product decision (Google Search interstitial); HTML Living Standard
  # (form entry list, script processing, document.write); Fetch Metadata Request Headers

  Background:
    Given Glide is built from this repository

  Scenario: Top-level navigation sends an honest Glide user agent and Fetch Metadata
    When an HTTP document navigation is sent to 127.0.0.1
    Then the User-Agent names Glide and the host OS, not Firefox/81
    And Accept-Language and Sec-Fetch-* headers are present

  Scenario: Hidden gbv field reflects script-updated value on POST
    Given a page with hidden gbv=1 that sets gbv=2 when XMLHttpRequest exists
    When the form POSTs to 127.0.0.1
    Then the body includes gbv=2

  Scenario: External scripts from document.write run before load
  # Tests: headless_js::{onload_runs_external_challenge_before_load,document_write_fetches_external_script_before_load}
    Given a challenge script that defines challenge()
    When document.write inserts its script src before window load
    Then onload can call challenge() without ReferenceError

  Scenario: Missing host APIs log as missing_api; page function names do not
  # Tests: js::diagnostics::tests::{classify_known_fetch_missing_is_missing_api,classify_page_function_missing_is_other_not_missing_api}
    When a script throws ReferenceError for fetch
    Then the error kind is missing_api
    When a script throws ReferenceError for solveSimpleChallenge
    Then the error kind is other with a page-script hint