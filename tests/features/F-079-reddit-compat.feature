Feature: F-079 Reddit compatibility
  As a Reddit visitor
  I want Reddit's client bundle to run without missing DOM APIs or loader failures
  So that the page settles cleanly after challenge and polyfill scripts

  # Source / basis: HTML Living Standard (Node.getRootNode, ParentNode, ChildNode,
  #                custom elements); product decision (Reddit shell)
  # Registry: docs/features.md -> F-079
  # Tests: js::integration::reddit_dom_api_*, headless_js::reddit_*, diagnostics::reddit_compat_*
  # MCP: n/a (engine behavior; observable via glide_eval, glide_snapshot_text, structured logs)

  Background:
    Given a local file or loopback fixture under tests/fixtures/reddit-compat/

  Scenario: custom element connectedCallback can call getRootNode during define upgrade
  Scenario: dynamically injected script src on loopback fires load and runs
  Scenario: js_challenge flow sets cookie or query then navigates to home content
  Scenario: composed Reddit-like fixture reports clean page script diagnostics