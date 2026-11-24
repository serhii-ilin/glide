---
slug: "redirects"
kind: "feature"
title: "Follow HTTP redirects and render other status codes"
status: "done"
aliases:
  [
    "F-042"
  ]
requirements:
  [
    "navigate-back-forward-reload-history",
    "render-html-css-flex-floats",
    "user-see-page-loading-failed"
  ]
gherkin: "tests/features/redirects.feature"
tests:
  [
    "`http::tests::follows_redirect_chain_and_keeps_error_body`",
    "`http::tests::meta_refresh_reads_head_and_noscript_fallback`"
  ]
mcp: "planned mcp-headless-session (`glide_navigate` final URL)"
manual: []
---

HTTP redirects and Refresh are followed, and non-2xx documents with bodies are rendered.

## Notes

301/302/303/307/308, a `Refresh` header, and a zero-delay `<meta http-equiv="refresh">` are followed (20 hops). A refresh that exists only inside `<noscript>` is followed only when scripting is disabled and the page has no visible content (`display:none` boxes do not count); with scripting enabled the `<noscript>` contents are inert, so Google's `enablejs` retry page is not opened. The address bar uses the final URL. 4xx/5xx with a body render that document; an empty error status or a transport failure still shows Glide's error page. `fetch()` reports `status`, `ok`, `url`, and `redirected`
