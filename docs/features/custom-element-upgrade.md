---
slug: "custom-element-upgrade"
kind: "feature"
title: "Custom element upgrade and `:defined`"
status: "done"
aliases:
  [
    "F-076"
  ]
requirements:
  [
    "page-scripts-register-autonomous-custom"
  ]
gherkin: "tests/features/custom-element-upgrade.feature"
tests:
  [
    "`js::integration::custom_element_*`",
    "`headless_js::custom_element_upgrade_fixture`",
    "`custom_elements::tests::built_in_elements_match_defined_before_upgrade_hammer`",
    "`tests/diagnostics::custom_element_is_extension_logs_unsupported`"
  ]
mcp: "n/a (engine behavior; observable via `glide_eval`, `glide_dom`, `glide_snapshot_text`)"
manual: []
---

Autonomous custom elements upgrade on `define`, `upgrade`, and insertion; connected, disconnected, and attribute lifecycle callbacks run on the JS worker inside the DOM mutation task; Blitz honors `:defined` after upgrade.

## Notes

Reaction queue in `dom-glue.js` with index-based drain; ES class `super()` patched via `HTMLElement` upgrade target + `Reflect.construct`; legacy constructors use `call`. Constructor reuse and invalid `whenDefined` names throw per spec. Attribute `oldValue` from domino `mutateAttr`. `is=` rejects with `NotSupportedError` and `js.unsupported`. `adoptedCallback` not wired. Direct `stylo_dom` for `ElementState::DEFINED` batch sync.
