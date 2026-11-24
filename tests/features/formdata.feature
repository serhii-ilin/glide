Feature: formdata FormData and multipart request bodies
  As a page script author
  I want FormData, Blob, and File entries and correct fetch/XHR encoding
  So I can upload forms and binary data like other browsers

  # Registry: docs/features/formdata.md

  # Source / basis: HTML Living Standard (constructing form data set, multipart/form-data);
  # XMLHttpRequest Living Standard (send()); Fetch Living Standard (body).

  Background:
    Given a Glide page with scripting enabled

  Scenario: FormData supports append, set, get, delete, and iteration
    When a script builds FormData, mutates entries, and walks them
    Then get and getAll return the expected values
    And delete and has behave correctly

  Scenario: FormData constructor fills from an HTMLFormElement
    When a script constructs FormData from a form with controls and a submitter
    Then the entry list matches native form POST rules

  Scenario: fetch with FormData sends multipart/form-data
    When a script POSTs FormData to a loopback server
    Then the request has a multipart Content-Type with boundary
    And parts include escaped names, filenames, and UTF-8 bodies

  Scenario: fetch and XMLHttpRequest encode URLSearchParams as urlencoded
    When a script sends URLSearchParams as the body
    Then the Content-Type is application/x-www-form-urlencoded

  Scenario: Blob body uses the blob type as Content-Type
    When a script sends a Blob with a type
    Then the request Content-Type matches the blob type
