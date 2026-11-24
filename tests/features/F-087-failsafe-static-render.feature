Feature: F-087 Fail-safe static rendering
  As a person browsing a script-heavy site
  I want Glide to show the page's server-rendered content when its scripts leave the page blank
  So that I can still read something meaningful even if the page's app fails

  # Source / basis: product decision (rendering must fail safe); HTML Living Standard
  #                (`noscript` element, parsing with scripting disabled)
  # Registry: docs/features.md -> F-087
  # Tests: headless_js::{failsafe_blank_body_falls_back_to_static_content,
  #        failsafe_hidden_then_throw_falls_back_to_static_content,
  #        failsafe_keeps_scripted_content_when_it_renders},
  #        control_session::blank_scripted_page_falls_back_to_static_text,
  #        diagnostics::failsafe_fallback_is_logged
  # MCP: n/a (applies to every headless load; observable via glide_snapshot_text and glide_screenshot)

  Background:
    Given a local file:// fixture is loaded with JavaScript enabled

  Scenario: Scripts wipe the server-rendered shell
    Given a page whose script empties the body and renders nothing
    When the load settles
    Then the server-rendered text and the noscript text are shown
    And a js.page warning with status "fallback" records the switch to static content

  Scenario: Hydration hides the shell and then throws
    Given a page whose script hides the shell with CSS and throws
    When the load settles
    Then the server-rendered text is shown

  Scenario: A page that renders is left alone
    Given a page whose script replaces the body with visible content
    When the load settles
    Then the scripted content is shown and the static content is not

  Scenario: The MCP headless session shows the static content too
    Given a headless session with JavaScript on
    When it navigates to the page whose script empties the body
    Then glide_snapshot_text returns the server-rendered and noscript text
