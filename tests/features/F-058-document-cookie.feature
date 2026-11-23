Feature: F-058 document.cookie / cookie-jar bridge
  As a page script
  I want to read and write cookies through document.cookie
  So that client-side state stays in sync with HTTP requests

  # Source / basis: HTML Living Standard (document.cookie); RFC 6265
  # Registry: docs/features.md -> F-058
  # Tests: cookies::tests::*, http::tests::set_cookie_echoes_on_follow_up_request,
  #        js::apply::tests::cookie_rpc_get_hides_httponly,
  #        js::integration::document_cookie_get_set_visible_to_scripts
  # MCP: n/a (engine behavior; observable via glide_eval)

  Background:
    Given the browser uses one profile-wide cookie jar for every tab

  Scenario: HttpOnly response cookies are hidden from document.cookie
  # Source / basis: RFC 6265 § 4.1.2.6; HTML document.cookie
    When the server sets "hidden=1" with the HttpOnly attribute on a navigation
    And the page reads document.cookie
    Then the string does not contain "hidden"

  Scenario: Set-Cookie on a response is sent on the next request to the same host
  # Source / basis: RFC 6265 § 5.2; product decision (shared jar)
    When the server answers the first request with Set-Cookie sid=alpha; Path=/
    And the browser requests another URL on the same host
    Then the follow-up request includes a Cookie header containing sid=alpha

  Scenario: Page scripts can set and read a non-HttpOnly cookie
  # Source / basis: HTML Living Standard document.cookie setter/getter
    Given a page at https://example.test/ with scripting enabled
    When the script runs document.cookie = "a=1; path=/"
    And the script reads document.cookie
    Then the returned string includes a=1