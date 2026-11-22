Feature: F-066 MCP headless session navigation and page inspection
  As an agent driving Glide through the control layer
  I want a headless browsing session with navigation and inspection commands
  So that I can automate page reading without opening a window

  # Source / basis: Model Context Protocol specification (tools); HTML Living Standard (history traversal)
  # Registry: docs/features.md -> F-066
  # Tests: control_session::*; @mcp-stdio scenarios land in the next task
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