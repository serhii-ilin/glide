---
slug: "document-address-reflected-url-attributes"
kind: "requirement"
title: "The document address and reflected URL attributes resolve against the active page URL so bundle loaders do not throw `Invalid URL` on sites like meduza.io"
status: "active"
milestone: "M3"
aliases:
  [
    "R-040"
  ]
features:
  [
    "document-url-resolution",
    "profile-dir-settings-session-history"
  ]
source: "HTML Living Standard (document address; reflected `src`/`href`); URL Standard; meduza.io axios/webpack regression"
---

The document address and reflected URL attributes resolve against the active page URL so bundle loaders do not throw `Invalid URL` on sites like meduza.io
