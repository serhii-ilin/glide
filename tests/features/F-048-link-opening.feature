Feature: F-048 Link opening with modifier keys
  As a user
  I want modified clicks on links to open tabs
  So that I can keep reading the current page while following a link

  # Source / basis: HTML Living Standard (link activation behaviour, the target
  #                attribute and modifier-key processing); browser convention
  # Registry: docs/features.md -> F-048
  # Tests: tabs::tests::link_target_from_modifiers, manual M-036
  # Integration is deferred: Blitz's NavigateIframe event carries only node_id and
  # url; its NavigationOptions has no modifier/target fields (see F-048 notes).

  Background:
    Given a local page with a link to another local fixture
    And two tabs are open with the linking page active

  Scenario: A plain click navigates the active tab
    When I click the link without modifiers
    Then the active tab shows the linked page
    And the number of tabs is unchanged

  Scenario: Command or Control click opens a background tab
    When I click the link while holding Cmd (Ctrl on Linux)
    Then a new tab opens with the linked page
    And the original tab stays active

  Scenario: Command or Control and Shift click opens a foreground tab
    When I click the link while holding Cmd and Shift (Ctrl and Shift on Linux)
    Then a new tab opens with the linked page
    And the new tab becomes active
