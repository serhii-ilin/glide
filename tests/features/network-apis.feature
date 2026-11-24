Feature: network-apis Network and binary Web APIs
  As a page author
  I want fetch, XMLHttpRequest, Blob helpers, and sendBeacon available
  So that modern app shells can load data without silent fallback paths

  # Source / basis: XMLHttpRequest Living Standard; Fetch Living Standard;
  #                File API; Encoding Standard
  # Registry: docs/features/network-apis.md
  # Tests: js::integration::xhr_sync_and_async_requests_work,
  #        js::integration::fetch_request_and_response_objects_work,
  #        js::integration::fetch_and_xhr_base64_data_urls_work,
  #        js::integration::xhr_abort_stops_callbacks,
  #        js::integration::xhr_headers_available_at_headers_received,
  #        js::integration::fetch_binary_body_round_trips,
  #        js::integration::fetch_request_init_overrides_input,
  #        js::integration::send_beacon_returns_true_without_throwing,
  #        js::integration::text_and_blob_helpers_work,
  #        js::apply::tests::start_fetch_returns_before_the_network_completes,
  #        js::apply::tests::start_fetch_answers_data_urls_inline,
  #        headless_js::network_apis_work

  Background:
    Given a local page that runs page JavaScript

  Scenario: XMLHttpRequest reports status and body for data URLs
    When a script performs sync and async GET requests to a data URL
    Then readyState reaches DONE with status 200 and the payload as responseText
    And readyState progresses through HEADERS_RECEIVED, LOADING, and DONE

  Scenario: fetch exposes Request, Response, and Headers
    When a script calls fetch with method, headers, and body
    Then the Response resolves with status, headers, and body accessors

  Scenario: base64 data URLs decode for fetch and XHR
    When a script loads data:text/plain;base64 payloads
    Then fetch and XMLHttpRequest return the decoded bytes as text
    And malformed base64 fails the request

  Scenario: abort stops queued XMLHttpRequest callbacks
    When a script aborts an async XMLHttpRequest before completion
    Then load and loadstart do not run after abort

  Scenario: binary bodies round-trip through arrayBuffer
    When a script reads a binary data URL through fetch and XHR
    Then every byte matches the original octets

  Scenario: Blob and encoding helpers round-trip text
    When a script uses Blob, FileReader, TextEncoder, and TextDecoder
    Then UTF-8 text round-trips and data URLs are produced for blob reads

  Scenario: sendBeacon queues a POST without blocking
    When a script calls navigator.sendBeacon with a data URL and payload
    Then the call returns true immediately without throwing

  # Source / basis: Fetch Living Standard (fetch is asynchronous; the networking
  #                task delivers the response); product decision: the UI thread
  #                never performs network I/O.
  Scenario: a slow fetch does not freeze the window
    Given a page script fetches a URL whose response is delayed
    When the host dispatches the request
    Then control returns to the browser before the server answers
    And the Response resolves when the server answers
