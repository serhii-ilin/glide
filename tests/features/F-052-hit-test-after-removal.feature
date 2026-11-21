Feature: F-052 Hit testing after script-driven node removal
  As a user
  I want clicks and window input to keep working after a page script or the chrome
  rebuild removes elements
  So that a dynamic page or a toolbar update does not crash the browser

  # Source / basis: DOM Standard (node removal); product robustness spec (R-013)
  # Registry: docs/features.md -> F-052
  # Tests: js::host::tests::stale_paint_child_is_pruned_before_hit_test,
  # js::host::tests::paint_after_removed_positioned_node_does_not_panic,
  # js::paint_prune::tests::prune_drops_only_dead_ids,
  # js::integration::removed_positioned_child_stays_paintable,
  # chrome::tests::menu_rebuild_keeps_hit_testing_safe

  Background:
    Given a page whose script removes a rendered element

  Scenario: Clicking after a removed node stays stable
    Given a parent paint list still names the removed node
    When the user clicks inside the parent
    Then Glide resolves and prunes the stale paint entry
    And the click completes without a panic

  Scenario: Painting after a removed positioned node stays stable
    Given a hoisted stacking-context entry still names the removed node
    When Glide resolves the layout for display
    Then the stale hoisted entry is pruned first
    And painting completes without a panic

  Scenario: Rebuilding the menu bar keeps hover hit-testing safe
    Given the chrome menu bar is rebuilt while a dropdown is displayed
    When the window hit-tests the pointer before the next resolve
    Then the stale hoisted dropdown entry is pruned first
    And the hit test completes without a panic
