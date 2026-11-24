Feature: reddit-compat Reddit compatibility
  Reddit's bundle expects DOM tree APIs and polyfill loaders that behave like a browser.

  # Registry: docs/features/reddit-compat.md

  Background:
    Given Glide loads tests from tests/fixtures/reddit-compat or loopback 127.0.0.1:0

  Scenario: getRootNode works during custom element upgrade
    Given a page defines a custom element whose connectedCallback calls getRootNode
    When the element is connected to the document
    Then the callback reports the document root without throwing

  Scenario: Shadow root stub composed traversal
    Given a DocumentFragment child whose host is an attached body element
    When the child calls getRootNode and getRootNode with composed true
    Then getRootNode returns the fragment and composed returns the document
    And isConnected is true for the attached host and false for a detached host

  Scenario: ParentNode append validates before mutating
    Given a parent with a child element
    When append is called with an inclusive ancestor
    Then HierarchyRequestError is thrown and the tree is unchanged

  Scenario: Polyfill loader receives load only after script runs
    Given a loopback page loads loader.js that sets a data attribute
    When the polyfill script is injected dynamically
    Then the script element fires load and the marker attribute is 1 before the callback runs

  Scenario: Missing polyfill src fires error
    Given a loopback 404 script URL
    When the loader injects that script
    Then the script element fires error and the polyfill marker stays unset

  Scenario: js_challenge gate and redirect
    Given a challenge page sets js_challenge cookie and location.replace to home
    When Glide follows the navigation with the cookie or query token
    Then the home document renders HOME-CHALLENGE-OK
    And a plain home GET without the token shows the failure page

  Scenario: Constructable stylesheets are available
    Given a page constructs a CSSStyleSheet and calls replaceSync
    When it adopts the sheet with document.adoptedStyleSheets
    Then the stylesheet reports its rules and the adopted CSS applies to the page

  # Source: DOM Standard, Element.attachShadow / ShadowRoot; Lit's createRenderRoot calls it.
  Scenario: Custom elements can attach a shadow root
    Given a custom element calls this.attachShadow({mode: "open"}) and renders into it
    When the element connects
    Then no TypeError is raised and the rendered content is visible in the page
    And host.shadowRoot returns the root with host and mode set, null for a closed root
    And a second attachShadow on the same host throws InvalidStateError
    And getRootNode on a node inside returns the shadow root, or the document when composed

  Scenario: Shadow roots accept adopted stylesheets
    Given a host with an attached shadow root containing an element
    When the page assigns shadowRoot.adoptedStyleSheets with a constructed sheet
    Then the CSS applies to the element and a non-CSSStyleSheet value throws TypeError

  Scenario: Composed Reddit-like fixture settles cleanly
    Given reddit-composed.html is loaded with JavaScript enabled
    When page scripts settle
    Then dump text contains REDDIT-COMPOSED-OK
    And structured diagnostics report page scripts settled with status clean