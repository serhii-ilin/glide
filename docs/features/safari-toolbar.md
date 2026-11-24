---
slug: "safari-toolbar"
kind: "feature"
title: "Safari-style toolbar, address field and tab row"
status: "done"
aliases:
  [
    "F-054"
  ]
requirements:
  [
    "window-shows-safari-like-macos"
  ]
gherkin: "tests/features/safari-toolbar.feature"
tests:
  [
    "`chrome::tests::toolbar_has_navigation_address_and_actions`",
    "`chrome::tests::address_field_shows_the_page_title`",
    "`chrome::tests::address_field_marks_the_focus_state`",
    "`chrome::tests::toolbar_uses_borderless_icon_buttons`",
    "`chrome::tests::chrome_uses_the_system_font_stack`",
    "`chrome::tests::navigation_buttons_use_svg_icons`",
    "`chrome::tests::stop_state_uses_an_svg_icon`",
    "`chrome::tests::bookmark_star_uses_svg_icons`",
    "`chrome::tests::reload_sits_inside_the_address_field`",
    "`chrome::tests::address_field_is_borderless_with_a_focus_ring`",
    "`chrome::tests::tabs_are_equal_width_segments`",
    "`chrome::tests::find_bar_has_chevrons_and_done`",
    "`chrome::tests::chrome_palette_is_neutral`",
    "`chrome::tests::devtools_console_input_fits_the_panel`",
    "`tabs::tests::ensure_one_tab_opens_a_default_tab`",
    "manual safari-toolbar#1"
  ]
mcp: "n/a (chrome UI)"
manual:
  [
    {
      "steps": "Open a window with two or three tabs; hover the toolbar buttons, a tab and a menu item; click the address field; press Cmd/Ctrl+F; open the Tabs menu",
      "expected": "The menu bar and toolbar read as one neutral grey surface in the system font with a single hairline above the tab row and one between chrome and page. Back/forward chevrons sit on the left; the borderless filled address field is centered, shows the page title with a magnifier and reload inside it; clicking it turns it white with a blue focus ring and reveals the URL left-aligned. Tabs are equal-width flat segments below the toolbar; the active tab is lighter, the close × appears at the leading edge of the hovered and active tab. Icon buttons show a light grey fill on hover. The find bar shows a filled search field, chevrons and Done. Menu shortcuts show ⇧, ⌘ and ⇥ without tofu boxes and hovered items turn accent blue. The tab row never shows an empty strip",
      "aliases": [
        "M-040"
      ]
    }
  ]
---

The window looks like Safari on macOS (light): one neutral grey chrome surface in the system font, thin line icons for back and forward on the left, a centered borderless address field that shows the page title with reload at its trailing edge and turns white with a blue focus ring while editing the URL, and the zoom, JavaScript and bookmark controls on the right. Tabs sit below as equal-width segments, the find bar is a Safari-style banner with chevrons and Done, and DevTools uses the same neutral palette. The tab row always shows at least one tab.

## Notes

Colours are CSS custom properties on `html`; every length is `calc(Npx * var(--ui))`. All icons (back, forward, reload, stop ✕, star, filled star, plus, close, magnifier, find chevrons) are inline SVG line icons stroked in `currentColor`, so the CSS colour tokens drive them. Hover states are verified by CSS review and safari-toolbar#1, since `--screenshot` cannot hover.
