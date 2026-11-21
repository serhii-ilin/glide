Feature: F-052 Hit testing after script-driven node removal
  As a user
  I want clicks to keep working after a page script removes elements
  So that a dynamic page does not crash the browser

  # Source / basis: DOM Standard (node removal); product robustness spec (R-013)
  # Registry: docs/features.md -> F-052
  # Tests: js::host::tests::stale_paint_child_is_pruned_before_hit_test

  Background:
    Given a page whose script removes a rendered element

  Scenario: Clicking after a removed node stays stable
    Given a parent paint list still names the removed node
    When the user clicks inside the parent
    Then Glide resolves and prunes the stale paint entry
    And the click completes without a panic
