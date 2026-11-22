Feature: F-069 Remote control of a live window
  As a Glide user
  I want to opt in to letting an MCP agent drive my open browser window
  So that an agent can inspect and operate pages only after I launch Glide with --remote-control

  # Source / basis: product security decision (opt-in per launch, user-only socket);
  #                MCP security best practices for local servers (explicit user consent,
  #                least-privilege local IPC); ssh-agent socket model (0700 directory,
  #                0600 socket, unguessable path under the profile)
  # Registry: docs/features.md -> F-069
  # Tests: remote_socket::tests::socket_permissions_are_user_only,
  #        remote_socket::tests::existing_run_dir_is_tightened_to_0700,
  #        remote_socket::tests::live_socket_refuses_second_server,
  #        remote_socket::tests::stale_socket_is_replaced,
  #        remote_socket::tests::roundtrip_with_echo_handler,
  #        remote_socket::tests::requests_on_one_connection_answer_in_order,
  #        remote_socket::tests::handler_error_becomes_err_response,
  #        remote_socket::tests::oversize_line_rejected,
  #        remote_socket::tests::malformed_json_reports_error_and_keeps_connection,
  #        remote_socket::tests::socket_removed_on_shutdown_and_drop
  #        @window scenarios: implemented with the window wiring and attach client (F-069 follow-up)
  # MCP: all F-066-F-068 tools via `glide --mcp --attach [socket]`

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
    Then the stale file is removed and the new server is live

  Scenario: A request gets a response with the same id
    Given a remote-control server with an echo handler
    When a client sends a request with id 7
    Then the response carries id 7 and the echoed request

  Scenario: A handler error becomes an error response
    Given a remote-control server whose handler fails
    When a client sends a request
    Then the response carries the handler's error message

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

  Scenario: The socket file is removed on shutdown
    Given a remote-control server is live
    When the server shuts down
    Then the socket file no longer exists

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
