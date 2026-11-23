Feature: F-077 WebSocket
  As a page author
  I want the WebSocket API
  So that realtime pages can open a bidirectional socket

  # Source / basis: RFC 6455; HTML mixed content; product decision (one tokio task per socket)
  # Registry: docs/features.md -> F-077 (R-037)
  # Tests: js::websocket::tests::*,
  #        js::integration::{websocket_loopback_echo_and_close,websocket_close_validation_leaves_socket_open,
  #        websocket_binary_blob_echo,websocket_url_fragment_and_default_port,websocket_protocol_negotiation,
  #        websocket_rejects_mixed_content_and_bad_url}; Manual: M-057

  Background:
    Given a local page that runs page JavaScript on https://example.test

  Scenario: a loopback socket echoes and closes cleanly
    # Manual: M-056
    When a script opens a WebSocket to a local echo server
    Then an open event fires
    And a sent text frame is echoed as a message event
    And close reports code 1000 and wasClean true

  Scenario: mixed content and invalid URLs throw
    When a script on an HTTPS page opens ws:// or a non-WebSocket URL
    Then mixed content raises SecurityError
    And an invalid scheme raises SyntaxError

  Scenario: close codes, binaryType, and URL rules
    # Manual: M-057
    When a script opens a socket and calls close with invalid code or reason
    Then InvalidAccessError or SyntaxError is raised and the socket stays open
    And binaryType blob and arraybuffer echo binary payloads
    And a URL with a fragment throws SyntaxError
