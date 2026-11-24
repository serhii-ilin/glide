---
slug: "js-cli-flag"
kind: "feature"
title: "`--no-js` CLI flag"
status: "done"
aliases:
  [
    "F-009"
  ]
requirements:
  [
    "run-page-javascript-allow-turning"
  ]
gherkin: "backfill pending"
tests:
  [
    "`args::tests::*`",
    "`headless_js::*`"
  ]
mcp: "planned mcp-headless-session (`glide --mcp --no-js`)"
manual: []
---

`--no-js` disables page JavaScript for the current run without changing the saved setting.
