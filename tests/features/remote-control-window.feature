Feature: remote-control-window Remote control of a live window
  As a Glide user
  I want to opt in to letting an MCP agent drive my open browser window
  So that an agent can inspect and operate pages only after I launch Glide with --remote-control

  # Source / basis: product security decision (opt-in per launch, user-only socket);
  #                MCP security best practices for local servers (explicit user consent,
  #                least-privilege local IPC); ssh-agent socket model (0700 directory,
  #                0600 socket, unguessable path under the profile)
  # Registry: docs/features/remote-control-window.md
  # Tests: remote_socket::tests::socket_path_is_under_run_dir,
  #        remote_socket::tests::request_and_response_lines_roundtrip,
  #        remote_socket::tests::parse_request_rejects_bad_version,
  #        remote_socket::tests::roundtrip_with_echo_handler,
  #        remote_socket::tests::handler_error_becomes_err_response,
  #        remote_socket::tests::handler_panic_returns_error_response,
  #        remote_socket::tests::socket_permissions_are_user_only,
  #        remote_socket::tests::existing_run_dir_is_tightened_to_0700,
  #        remote_socket::tests::symlinked_run_dir_is_refused,
  #        remote_socket::tests::overlong_socket_path_has_clear_error,
  #        remote_socket::tests::live_socket_refuses_second_server,
  #        remote_socket::tests::stale_socket_is_replaced,
  #        remote_socket::tests::regular_file_at_socket_path_is_not_removed,
  #        remote_socket::tests::symlink_at_socket_path_is_not_followed_or_removed,
  #        remote_socket::tests::oversize_line_rejected,
  #        remote_socket::tests::malformed_json_reports_error_and_keeps_connection,
  #        remote_socket::tests::socket_removed_on_shutdown_and_drop,
  #        remote_socket::tests::shutdown_does_not_remove_replaced_socket,
  #        remote_socket::tests::shutdown_closes_open_connections,
  #        remote_socket::tests::too_many_connections_are_refused,
  #        remote_socket::tests::rejected_request_record_is_structured_and_truncated,
  #        remote_socket::tests::client_call_times_out_when_handler_is_slow,
  #        remote_socket::tests::client_stays_in_sync_after_timeout,
  #        remote_socket::tests::call_deadline_holds_against_trickling_server,
  #        remote_socket::tests::large_response_over_one_mib_roundtrips,
  #        remote_socket::tests::oversize_response_poisons_client,
  #        remote_socket::tests::accept_error_action_classifies_transient_and_resource_errors,
  #        remote_socket::tests::requests_on_one_connection_answer_in_order
  #        @window scenarios: implemented with the window wiring and attach client (remote-control-window follow-up)
  # MCP: all mcp-headless-session-mcp-page-input tools via `glide --mcp --attach [socket]`

  Background:
    Given a profile directory with a run directory

  Scenario: The socket is readable only by the user
    Given a remote-control server bound at the profile socket path
    Then the run directory has mode 0700
    And the socket file has mode 0600

  Scenario: A second server refuses to start while one is live
    Given a remote-control server is live at the profile socket path
    When a second server tries to bind the same path
    Then binding fails with "remote control already active"

  Scenario: A stale socket file is replaced
    Given a socket file left behind by a dead Glide process
    When a new remote-control server binds the profile socket path
    Then the stale file is removed and the new server answers a request

  Scenario: A non-socket file at the socket path is never removed
    Given a regular file or symlink exists at the profile socket path
    When a remote-control server tries to bind that path
    Then binding fails with "exists and is not a socket"
    And the file or symlink is left untouched

  Scenario: A symlinked run directory is refused
    Given the profile run directory is a symlink to another directory
    When a remote-control server tries to bind the profile socket path
    Then binding fails with a clear error
    And the symlink target is not tightened or modified

  Scenario: An overlong socket path is refused with a hint
    Given a profile socket path longer than the platform sun_path limit
    When a server or client opens the socket
    Then it fails with a clear "path is too long" error and a shorter-path hint

  Scenario: A request gets a response with the same id
    Given a remote-control server with an echo handler
    When a client sends a request with id 7
    Then the response carries id 7 and the echoed request

  Scenario: A handler error becomes an error response
    Given a remote-control server whose handler fails
    When a client sends a request
    Then the response carries the handler's error message

  Scenario: A handler panic becomes an internal error
    Given a remote-control server whose handler panics
    When a client sends that request
    Then the response is "internal error"
    And the same connection still answers the next request

  Scenario: An oversize request is rejected and the connection closed
    Given a remote-control server
    When a client sends a line longer than 1 MiB
    Then the server answers with a null id and "request too large"
    And the server closes that connection

  Scenario: A malformed line gets an error and the connection keeps working
    Given a remote-control server is live
    When a client sends a line that is not a JSON request
    Then the server answers with a "bad request" error
    And the same connection still answers the next valid request

  Scenario: A client stays in sync after a timeout
    Given a handler that answers request 1 after the client's timeout
    When the client times out on request 1 and sends request 2
    Then the late answer for request 1 is discarded
    And request 2 receives its own response

  Scenario: A call respects one overall deadline
    Given a server that trickles bytes without finishing a line
    When the client calls with a timeout
    Then the call fails within that deadline rather than per read

  Scenario: A large response is accepted
    Given a handler that returns a 3 MiB payload
    When the client calls with the default response limit
    Then the full payload comes back

  Scenario: An oversize response poisons the connection
    Given a response limit smaller than the handler's payload
    When the client reads that response
    Then the call fails with "connection is broken; reconnect"
    And later calls fail the same way without reading the stream

  Scenario: Too many concurrent connections are refused
    Given 16 live connections
    When a seventeenth client connects
    Then it receives "too many connections" and is disconnected

  Scenario: The socket file is removed on shutdown
    Given a remote-control server is live
    When the server shuts down
    Then the socket file no longer exists

  Scenario: Shutdown closes open connections
    Given a client connected to a live remote-control server
    When the server shuts down
    Then the client's next call fails
    And the handler is not invoked

  Scenario: Shutdown does not remove a socket it no longer owns
    Given the socket path was replaced after the server bound it
    When the server shuts down
    Then the replacement file is left in place

  @window
  Scenario: Without --remote-control no socket is created
    Given Glide starts without --remote-control
    Then no socket file exists under the profile run directory

  @window
  Scenario: glide --mcp --attach drives the open window
    Given Glide is running with --remote-control
    When glide --mcp --attach navigates the window
    Then the window shows the requested page

  @window
  Scenario: Every remote command is logged and the toolbar shows Remote
    Given Glide is running with --remote-control and an attached agent
    When the agent sends a command
    Then the toolbar shows a Remote badge
    And one remote.control log record is written with source_len only
