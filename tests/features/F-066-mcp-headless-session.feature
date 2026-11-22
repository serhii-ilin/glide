Feature: F-066 MCP headless session navigation and page inspection
  As an agent driving Glide through the control layer
  I want a headless browsing session with navigation and inspection commands
  So that I can automate page reading without opening a window

  # Source / basis: Model Context Protocol specification (tools); HTML Living Standard (history traversal)
  # Registry: docs/features.md -> F-066
  # Tests: control_session::*; control::protocol::tests::*; control::page_ops::tests::*; control::session::tests::*; headless::tests::fetch_page_decodes_percent_encoded_file_path; js::console_feed::tests::drain_for_tab_keeps_other_tabs; shot::tests::encode_png_roundtrip; mcp_stdio::*; mcp::tests::*; mcp::tools::tests::*; args::tests::mcp_*
  # MCP: glide_navigate, glide_back, glide_forward, glide_reload, glide_page_info, glide_snapshot_text, glide_dom, glide_query, glide_screenshot

  Background:
    Given a headless control session with JavaScript enabled

  Scenario: Navigate to a local fixture and read its title and visible text
    When the session navigates to the MCP page A fixture
    Then the page info reports title "Page A" and status settled
    And a text snapshot includes "MCP Page A" and "Paragraph on page A"

  Scenario: Back and forward walk the session history
    Given the session has visited MCP page A and then page B
    When the session goes back
    Then the page title is "Page A"
    When the session goes forward
    Then the page title is "Page B"

  Scenario: Back at the first page is an error
    Given the session is on the first history entry only
    When the session goes back
    Then the control layer returns an error

  Scenario: Reload re-runs the page's scripts
    Given the session is on MCP page A
    When the session reloads the page
    Then the inline script marker element is present again

  Scenario: DOM by selector returns the element's HTML
    Given the session is on MCP page A
    When the session requests DOM for selector "#box"
    Then the HTML includes id="box" and "Box"

  Scenario: A selector with no match is an error
    Given the session is on MCP page A
    When the session requests DOM for selector "#missing"
    Then the control layer returns an error

  Scenario: Query returns element boxes with visibility
    Given the session is on MCP page A
    When the session queries "#box"
    Then a visible box about 120 by 40 is returned
    And the hidden element is not visible

  Scenario: Screenshot returns a 1100x760 PNG
    Given the session is on MCP page A
    When the session takes a screenshot
    Then the PNG dimensions are 1100 by 760

  Scenario: Navigating to a missing local file is an error and keeps the current page
    Given the session is on MCP page A
    When the session navigates to a missing local file path
    Then navigation fails
    And the page title remains "Page A"

  Scenario: With JavaScript disabled, static text is still readable
    Given a headless control session with JavaScript disabled
    When the session navigates to the MCP page A fixture
    Then a text snapshot includes "MCP Page A"
    And the snapshot does not include the random script output prefix

  @mcp-stdio
  Scenario: The MCP server lists the page tools
    Given glide is started in MCP mode
    When the client lists tools
    Then the tool list includes navigation and inspection tools

  @mcp-stdio
  Scenario: Every stdout line is a JSON-RPC message
    Given glide is started in MCP mode
    When the client exchanges several JSON-RPC messages
    Then every stdout line parses as JSON-RPC

  @mcp-stdio
  Scenario: Back and forward over stdio
    Given glide is started in MCP mode
    When the client navigates MCP page A then page B, goes back, then forward
    Then page titles are "Page A" then "Page B"

  @mcp-stdio
  Scenario: Reload over stdio re-runs inline scripts
    Given glide is started in MCP mode on MCP page A
    When the client reloads the page
    Then DOM for "#run" is present again

  @mcp-stdio
  Scenario: Page info over stdio reports settled state
    Given glide is started in MCP mode on MCP page A
    When the client requests page info
    Then the page info JSON reports title "Page A" and status settled

  @mcp-stdio
  Scenario: DOM and query over stdio
    Given glide is started in MCP mode on MCP page A
    When the client requests DOM for "#box" and queries "#box"
    Then the DOM includes the box and query returns a visible element

  @mcp-stdio
  Scenario: Missing DOM selector is a tool error over stdio
    Given glide is started in MCP mode on MCP page A
    When the client requests DOM for "#missing"
    Then the tool result is an error naming the selector

  @mcp-stdio
  Scenario: Missing file navigation is a tool error over stdio
    Given glide is started in MCP mode on MCP page A
    When the client navigates to a missing local file
    Then navigation is a tool error and page title remains "Page A"

  @mcp-stdio
  Scenario: Screenshot over stdio returns 1100x760 PNG
    Given glide is started in MCP mode on MCP page A
    When the client takes a screenshot
    Then the PNG dimensions are 1100 by 760

  @mcp-stdio
  Scenario: MCP with JavaScript disabled still reads static text
    Given glide is started in MCP mode with JavaScript disabled
    When the client navigates to MCP page A and snapshots text
    Then the snapshot includes "MCP Page A" and not the script marker prefix

  @mcp-stdio
  Scenario: Missing required tool arguments are JSON-RPC errors
    Given glide is started in MCP mode
    When the client calls glide_navigate without a url
    Then the response is a JSON-RPC error and a later call still works

  @mcp-stdio
  Scenario: Stdin EOF exits cleanly
    Given glide is started in MCP mode
    When the client closes stdin
    Then glide exits with code 0 within 10 seconds

  @mcp-stdio
  Scenario: Attach is not supported yet
    When glide is started as glide --mcp --attach
    Then it exits with code 2 and a clear stderr message