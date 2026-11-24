---
slug: "browser-page-script-failures-logged"
kind: "requirement"
title: "Browser and page-script failures are logged as structured records with page, script, line, root-cause kind and hint; pages that degrade silently (unsupported features, missing capability probes, blank renders) leave a record too"
status: "active"
milestone: "M3"
aliases:
  [
    "R-020"
  ]
features:
  [
    "structured-diagnostics"
  ]
source: "Product observability spec; ECMA-262 (uncaught exceptions); V8 message listener API; `AGENTS.md` logging rules"
---

Browser and page-script failures are logged as structured records with page, script, line, root-cause kind and hint; pages that degrade silently (unsupported features, missing capability probes, blank renders) leave a record too
