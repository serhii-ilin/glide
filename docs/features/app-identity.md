---
slug: "app-identity"
kind: "feature"
title: "App name and icon"
status: "done"
aliases:
  [
    "F-079"
  ]
requirements:
  [
    "app-presents-itself-glide-glide"
  ]
gherkin: "tests/features/app-identity.feature"
tests:
  [
    "`app_icon::tests::{embedded_window_icon_decodes_to_256_square_rgba,decode_png_rgba_rejects_bytes_that_are_not_a_png,decode_png_rgba_expands_rgb_and_palette_images,decode_png_rgba_expands_gray_transparent_palette_and_16_bit_images,icon_warning_uses_schema_fields,info_plist_template_names_the_app_and_icon_with_version_placeholder,info_plist_template_is_balanced_xml}`",
    "`app_identity::macos_binary_embeds_info_plist_naming_glide`",
    "manual app-identity#1"
  ]
mcp: "n/a (OS shell integration)"
manual:
  [
    {
      "steps": "macOS: `cargo run --release`. Look at the bold app menu, the Dock and Cmd-Tab. Then run `scripts/bundle-macos.sh`, `open` the printed `Glide.app`, check the same, look at the bundle in Finder and choose Glide > About Glide. Linux/Windows: run the binary and look at the window title bar and taskbar",
      "expected": "The bold app menu reads \"Glide\" (not \"glide\"); Dock and Cmd-Tab show the glider icon for both the bare binary and the bundle; Finder shows the icon on `Glide.app`; About Glide shows the icon; on Linux/Windows the window and taskbar show the glider",
      "aliases": [
        "M-060"
      ]
    }
  ]
---

The app shows as "Glide" with the Glide glider icon in the macOS menu bar, Dock and Cmd-Tab, and as the window icon on Windows and Linux; `scripts/bundle-macos.sh` builds `Glide.app`.

## Notes

`build.rs` embeds `assets/macos/Info.plist.in` (shared with the bundle script) via `-sectcreate`; `src/app_icon.rs` sets the Dock icon (the one `unsafe` call outside `src/js/`) and the winit window icon. Assets in `assets/icon/`.
