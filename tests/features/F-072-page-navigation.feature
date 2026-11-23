Feature: F-072 Page-initiated navigation
  As a page author
  I want forms and location assignments to navigate the browsing context
  So that sites that submit forms or change location in scripts (JavaScript challenges, search forms) work

  # Source / basis: HTML Living Standard (form submission algorithm, form.requestSubmit(),
  #                form.submit(), location.assign/replace); product decision: a headless
  #                load follows page-initiated navigations for a bounded number of hops
  # Registry: docs/features.md -> F-072
  # Tests: js::integration::request_submit_fires_submit_event_and_navigates,
  #        js::integration::form_submit_does_not_fire_submit_event,
  #        js::integration::canceled_submit_event_prevents_navigation,
  #        js::integration::location_assign_requests_navigation,
  #        js::apply::tests::submit_form_rpc_navigates_with_the_entry_list,
  #        js::apply::tests::navigate_rpc_requests_the_target_url,
  #        headless_js::page_script_navigation_is_followed,
  #        headless_js::headless_post_form_navigation_submits_urlencoded_body,
  #        js::apply::tests::post_form_submit_navigates_with_post_method,
  #        http::tests::post_form_document_navigation_is_urlencoded
  # MCP: glide_navigate (the headless session follows page-initiated navigations)

  Background:
    Given a local page that runs page JavaScript

  Scenario: requestSubmit fires the submit event and navigates with the form fields
    Given a form with action /search, an input q=glide and a hidden token field
    And the page sets form.onsubmit to fill the token field
    When the page calls form.requestSubmit()
    Then the submit event handler runs
    And the browsing context navigates to /search?q=glide&token=t1

  Scenario: submit does not fire the submit event
    Given a form with an onsubmit handler
    When the page calls form.submit()
    Then the handler does not run
    And the browsing context still navigates with the form fields

  Scenario: A canceled submit event prevents navigation
    Given a form whose onsubmit handler calls preventDefault()
    When the page calls form.requestSubmit()
    Then no navigation is requested

  Scenario: location.assign requests a navigation
    When a script calls location.assign('/next?x=1')
    Then the browsing context navigates to https://example.test/next?x=1

  Scenario: A headless load follows a page-initiated navigation
    Given a file fixture whose script assigns a new location
    When the fixture is loaded headlessly
    Then the text dump comes from the target document
    And the target document sees the query string the script passed

  Scenario: A POST form submission navigates with an urlencoded body
    Given an http page whose script calls form.requestSubmit on a method=post form
    When the page is loaded headlessly
    Then the server receives an application/x-www-form-urlencoded POST body
    And the response document is shown

  Scenario: History back after POST does not resubmit the form
  # Source / basis: product decision — back/forward loads the URL with GET only
    Given the user submitted a POST form and navigated to the result
    When the user goes back and forward in history
    Then neither step repeats the POST body
