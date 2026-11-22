Feature: F-065 DevTools Network tab
  As a page author
  I want to see completed network requests for the active page
  So that I can debug loads, subresources and fetch/XHR traffic

  # Source / basis: Chrome DevTools Network panel; product decision
  # Registry: docs/features.md -> F-065
  # Tests: devtools::network::tests::*,
  #        net::tests::network_log_records_subresource,
  #        js::apply::tests::start_fetch_records_network_entry,
  #        chrome::tests::classifies_devtools_network_clear,
  #        manual M-050
  # MCP: planned F-071 (`glide_network_requests`)

  Background:
    Given DevTools is open on the Network tab

  Scenario: Completed requests appear in the table
  # Automated: devtools::network::tests::journal_stores_entries_in_order,
  #            devtools::network::tests::render_table_includes_columns

  Scenario: Clear empties the log without navigating
  # Automated: devtools::network::tests::clear_resets_journal,
  #            chrome::tests::classifies_devtools_network_clear

  Scenario: Filter hides non-matching rows
  # Automated: devtools::network::tests::filter_matches_name_and_url

  Scenario: Selecting a row shows request details
  # Automated: devtools::network::tests::detail_html_includes_headers,
  #            devtools::network::tests::row_links_use_glide_select_command

  Scenario: Navigation clears the log for a new document
  # Automated: devtools::network::tests::clear_on_navigation_epoch,
  #            devtools::network::tests::link_navigation_prune_keeps_document_drops_subresources,
  #            devtools::network::tests::completed_observation_leaves_journal_unchanged
