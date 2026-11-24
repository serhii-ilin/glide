---
slug: "page-hides-custom-element-shell"
kind: "requirement"
title: "A page that hides its custom-element shell until upgrade, or whose content streams in inside `<template for>` suspense slots, still shows its server-rendered markup even though custom elements are not upgraded"
status: "active"
milestone: "M3"
aliases:
  [
    "R-033"
  ]
features:
  [
    "custom-element-rendering"
  ]
source: "HTML Living Standard (custom elements, `:defined`, `template`); CSS Selectors Level 4 (`:defined`); product decision (unupgraded custom elements render as plain elements, Reddit/faceplate streaming suspense)"
---

A page that hides its custom-element shell until upgrade, or whose content streams in inside `<template for>` suspense slots, still shows its server-rendered markup even though custom elements are not upgraded
