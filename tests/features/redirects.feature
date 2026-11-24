Feature: redirects Follow HTTP redirects and render other status codes
  As a user
  I want links and searches to land on the page the server intends
  So that navigation does not stop on an interstitial or a blank shell

  # Source / basis: RFC 9110 section 15 (redirection); HTML Standard, the noscript
  #                element in a document with a browsing context (contents are parsed
  #                only when scripting is disabled)
  # Registry: docs/features/redirects.md
  # Tests: http::tests::follows_redirect_chain_and_keeps_error_body,
  # http::tests::meta_refresh_reads_head_and_noscript_fallback

  Background:
    Given a local HTTP server that answers with redirects and refresh pages

  Scenario: An HTTP redirect chain is followed
    Given a response that redirects 302 to one path and 301 to another
    When Glide loads the first URL
    Then the final page is the last path in the chain

  Scenario: A zero-delay Refresh header is followed
    Given a response with "Refresh: 0;url=..."
    When Glide loads that URL
    Then the page at the refresh target is shown

  Scenario: A zero-delay meta refresh is followed
    Given a document whose head contains an immediate meta refresh
    When Glide loads that URL
    Then the page at the refresh target is shown

  Scenario: A noscript-only refresh is followed when scripting is disabled
    Given a script shell whose only refresh sits inside noscript
    And scripting is disabled
    When Glide loads that URL
    Then the noscript fallback page is followed

  Scenario: A noscript-only refresh is ignored when scripting is enabled
    Given a script shell whose only refresh sits inside noscript
    And scripting is enabled
    When Glide loads that URL
    Then Glide stays on the shell instead of the fallback
    And Google's "Turn on JavaScript" retry page is not opened
