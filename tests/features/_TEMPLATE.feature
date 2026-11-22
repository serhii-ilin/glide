# Copy to F-xxx-<slug>.feature. One file per feature ID in docs/features.md.
# Keep the Feature line, Source / basis comment and Registry comment in sync with the row.
Feature: F-xxx Short feature name
  As a <user role>
  I want <behavior>
  So that <value>

  # Source / basis: <HTML Living Standard section, ECMA-262 clause, CSS spec,
  #                WPT test path, or Acid1-3 case>
  # Registry: docs/features.md -> F-xxx
  # Tests: <test names from the feature row, or manual check M-xxx>
  # MCP: <glide_* tools that expose this | planned F-xxx (tool) | n/a (reason)>

  Background:
    Given <shared precondition, e.g. a fresh profile or a local file:// fixture>

  Scenario: <observable behavior>
    Given <starting state>
    When <action>
    Then <observable result>
