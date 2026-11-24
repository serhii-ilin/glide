---
slug: "parser-discovered-script-type-importmap"
kind: "requirement"
title: "Parser-discovered `<script type=\"importmap\">` JSON merges in document order and remaps bare module specifiers for static `import` and dynamic `import()`; invalid maps log an error and are ignored; unmapped bare specifiers fail with a clear diagnostic; `HTMLScriptElement.supports('importmap')` is true"
status: "active"
milestone: "M3"
aliases:
  [
    "R-044"
  ]
features:
  [
    "import-maps"
  ]
source: "HTML Living Standard (import maps, resolve a module specifier, import map merge); product decision"
---

Parser-discovered `<script type="importmap">` JSON merges in document order and remaps bare module specifiers for static `import` and dynamic `import()`; invalid maps log an error and are ignored; unmapped bare specifiers fail with a clear diagnostic; `HTMLScriptElement.supports('importmap')` is true
