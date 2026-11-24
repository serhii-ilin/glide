---
slug: "glide-exposes-dom-tree-script"
kind: "requirement"
title: "Glide exposes the DOM tree and script-loading behavior Reddit's shell expects: `getRootNode` (including `{composed:true}` across shadow-root stubs), spec-conforming ParentNode methods, loopback dynamic `<script src>` load/error events, JavaScript challenge redirects that reach the final document, constructable stylesheets (`new CSSStyleSheet()`, `replaceSync`, `document.adoptedStyleSheets`) whose rules apply to the page, and `Element.attachShadow` / `ShadowRoot` so custom elements (Lit) can render"
status: "active"
milestone: "M3"
aliases:
  [
    "R-045"
  ]
features:
  [
    "reddit-compat"
  ]
source: "HTML Living Standard (Node, Element, ParentNode, ChildNode, scripts); CSSOM (Constructable Stylesheets); DOM Standard (Shadow DOM: attachShadow, ShadowRoot); product decision (Reddit)"
---

Glide exposes the DOM tree and script-loading behavior Reddit's shell expects: `getRootNode` (including `{composed:true}` across shadow-root stubs), spec-conforming ParentNode methods, loopback dynamic `<script src>` load/error events, JavaScript challenge redirects that reach the final document, constructable stylesheets (`new CSSStyleSheet()`, `replaceSync`, `document.adoptedStyleSheets`) whose rules apply to the page, and `Element.attachShadow` / `ShadowRoot` so custom elements (Lit) can render
