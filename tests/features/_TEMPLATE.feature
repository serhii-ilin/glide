# Copy to tests/features/<slug>.feature. One file per feature slug (docs/features/<slug>.md).
# Choose a slug, never a number. See docs/registry.md.
# Keep the Feature line, Source / basis comment and Registry comment in sync with the feature file.
Feature: <slug> Short feature name
  As a <user role>
  I want <behavior>
  So that <value>

  # Source / basis: <HTML Living Standard section, ECMA-262 clause, CSS spec,
  #                WPT test path, or Acid1-3 case>
  # Registry: docs/features/<slug>.md
  # Tests: <test names from the feature file's tests list, or manual <slug>#<n>>
  # MCP: <glide_* tools that expose this | planned <slug> (tool) | n/a (reason)>

  Background:
    Given <shared precondition, e.g. a fresh profile or a local file:// fixture>

  Scenario: <observable behavior>
    Given <starting state>
    When <action>
    Then <observable result>
