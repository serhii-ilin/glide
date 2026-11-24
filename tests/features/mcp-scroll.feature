Feature: mcp-scroll Headless scrolling, scroll events and infinite scroll
  As an agent driving Glide through the control layer
  I want to scroll the headless page and have page scripts observe it
  So that infinite-scroll sites append content and I can read and screenshot it

  # Source / basis: CSSOM View Module (Window.scrollX/scrollY/scrollTo/scrollBy/innerWidth/innerHeight,
  #                Element.scrollIntoView, Document.scrollingElement); HTML Living Standard (scroll event,
  #                "run the scroll steps"); Intersection Observer; product decision (infinite scroll)
  # Registry: docs/features/mcp-scroll.md
  # Tests: see the feature file's tests list (control_session::scroll_*, js::integration::*scroll*,
  #        mcp_stdio::scroll_*)
  # MCP: glide_scroll, glide_screenshot (full_page)

  Background:
    Given a headless control session with JavaScript enabled

  Scenario: Scroll by a delta moves the viewport and reports offsets
    Given a page taller than the viewport
    When the agent scrolls by dy 300
    Then the result reports y 300, a scroll height above the viewport height and the viewport size

  Scenario: Scroll is clamped to the document
    Given a page taller than the viewport
    When the agent scrolls by dy 1000000
    Then the reported y equals scroll height minus viewport height

  Scenario: Scroll to a CSS selector brings the element into view
    Given a page with an element far below the fold
    When the agent scrolls to that selector
    Then the element's bounding rect intersects the viewport

  Scenario: Scroll to a missing selector is an error
    When the agent scrolls to a selector that matches nothing
    Then the control layer returns an error

  Scenario: Page JavaScript sees the real viewport
    Given a scrolled page
    Then window.scrollY, pageYOffset, scrollX, pageXOffset, innerWidth and innerHeight match the viewport
    And document.scrollingElement is the document element

  Scenario: A scroll event reaches window and document after the viewport moves
    Given a page with scroll listeners on window and document
    When the viewport moves, from the agent or from page scrollTo and scrollBy
    Then each listener runs for the move, with scrollY already updated

  Scenario: A page scrollTo to the offset the host just set fires exactly one scroll event
    Given a page with a scroll listener whose viewport the host moved without the page knowing
    When the page calls scrollTo with that same offset in the same task
    Then exactly one scroll event fires

  Scenario: An IntersectionObserver sentinel loads more content after a scroll
    Given an infinite-scroll page observing a sentinel at the bottom
    When the agent scrolls to the bottom
    Then the next batch of items fetched from a loopback server is in the page text

  Scenario: A scroll listener loads more content after a scroll
    Given an infinite-scroll page that checks scrollY in a scroll event
    When the agent scrolls to the bottom
    Then the next batch of items fetched from a loopback server is in the page text

  Scenario: A scroll result waits for a slow page fetch started by the scroll
    Given an infinite-scroll page whose items endpoint answers after a delay
    When the agent scrolls to the bottom
    Then the result is settled and the next batch of items is already in the page text

  Scenario: A page that never goes quiet returns within the timeout and is reported unsettled
    Given a page that mutates the document forever once it scrolls
    When the agent scrolls with a one second timeout
    Then the call returns within about the timeout and `settled` is false

  Scenario: Assigning scrollTop on an overflow container scrolls it and clamps
    Given a page with a 100px overflow:auto container holding 1000px of content
    When page script assigns scrollTop, scrollLeft and increments scrollTop in a loop
    Then reading them back returns the clamped offsets and the loop terminates

  Scenario: Screenshot honors the scroll offset and can capture the full page
    Given a page with a red block above a blue block below the fold
    When the agent scrolls past the red block and takes a screenshot
    Then the top pixel is blue
    When the agent takes a full-page screenshot
    Then the image is taller than the viewport and shows both colors
