Feature: F-055 Network and binary Web APIs
  As a page author
  I want fetch, XMLHttpRequest, Blob helpers, and sendBeacon available
  So that modern app shells can load data without silent fallback paths

  # Source / basis: XMLHttpRequest Living Standard; Fetch Living Standard;
  #                File API; Encoding Standard
  # Registry: docs/features.md -> F-055
  # Tests: js::integration::xhr_sync_and_async_requests_work,
  #        js::integration::fetch_request_and_response_objects_work,
  #        js::integration::text_and_blob_helpers_work,
  #        headless_js::network_apis_work

  Background:
    Given a local page that runs page JavaScript

  Scenario: XMLHttpRequest reports status and body for data URLs
    When a script performs sync and async GET requests to a data URL
    Then readyState reaches DONE with status 200 and the payload as responseText

  Scenario: fetch exposes Request, Response, and Headers
    When a script calls fetch with method, headers, and body
    Then the Response resolves with status, headers, and body accessors

  Scenario: Blob and encoding helpers round-trip text
    When a script uses Blob, FileReader, TextEncoder, and TextDecoder
    Then UTF-8 text round-trips and data URLs are produced for blob reads

  Scenario: sendBeacon queues a POST without blocking
    When a script calls navigator.sendBeacon with a data URL and payload
    Then the call returns true immediately