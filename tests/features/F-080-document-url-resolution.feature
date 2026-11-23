Feature: F-080 Document URL and reflected attributes

  Page scripts that resolve URLs through the DOM and the URL constructor
  (including webpack/axios bundle init on meduza.io) see a document address,
  location.origin, and reflected src/href attributes resolved against the
  active page URL.

  Background:
    Given Glide runs page scripts against a domino mirror of Blitz

  Scenario: location exposes absolute href and origin
    Given a page loaded at "https://meduza.test/page"
    When an inline script reads window.location.href and location.origin
    Then both values are non-empty absolute URLs with a scheme and host

  Scenario: document URL and baseURI match the page address
    Given a page loaded at "https://meduza.test/page"
    When a script reads document.URL documentURI and baseURI
    Then each value equals "https://meduza.test/page"

  Scenario: parser-inserted async script src resolves
    Given a page at "https://meduza.test/" with a parser async script src="/assets/app.js"
    When the parser has inserted the script element
    Then script.src is an absolute URL under "https://meduza.test/"

  Scenario: link and anchor href attributes resolve
    Given a page at "https://meduza.test/"
    When a link href="/style.css" and anchor href="/path" are read
    Then the resolved hrefs are "https://meduza.test/style.css" and "https://meduza.test/path"

  Scenario: URL constructor rejects undefined href
  # Source: URL Standard; meduza.io chunk 860 axios init
    Given the URL constructor is available
    When new URL(undefined) is evaluated
    Then a TypeError is thrown

  Scenario: webpack manifest and chunk loader init without Invalid URL
    Given a minimal manifest and loadable chunk fixture on 127.0.0.1
    When an async app script loads the manifest and runs the chunk
    Then the page text includes ok with the loopback origin and no error prefix