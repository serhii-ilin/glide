Feature: app-identity App name and icon
  As a Glide user
  I want the app to appear as "Glide" with the glider icon
  So that I can recognise it in the menu bar, Dock, Cmd-Tab and taskbar

  # Source / basis: Apple HIG (App icons; app menu named after the app); Apple Information
  #                Property List keys (CFBundleName, CFBundleIconFile); product decision
  # Registry: docs/features/app-identity.md
  # Tests: app_icon::tests::*, tests/app_identity.rs, manual app-identity#1
  # MCP: n/a (OS shell integration)

  Scenario: The embedded window icon decodes
    Given the embedded 256 px Glide icon PNG
    When it is decoded to RGBA
    Then the result is 256 by 256 pixels with 4 bytes per pixel

  Scenario: Corrupt icon bytes are rejected
    Given bytes that are not a PNG
    When they are decoded to RGBA
    Then decoding returns an error instead of panicking

  # Basis: PNG specification (colour types, tRNS, 16-bit samples)
  Scenario Outline: Any PNG colour type decodes to RGBA8
    Given a 1x1 <kind> PNG
    When it is decoded to RGBA
    Then the pixel is <rgba>

    Examples:
      | kind                      | rgba          |
      | RGB                       | 10,20,30,255  |
      | palette                   | 1,2,3,255     |
      | grayscale                 | 7,7,7,255     |
      | grayscale with alpha      | 7,7,7,128     |
      | palette with transparency | 1,2,3,64      |
      | 16-bit RGBA               | 18,86,154,222 |

  # Basis: AGENTS.md logging schema
  Scenario: A failed icon logs one schema-conformant warning
    Given the Dock icon cannot be created
    When Glide starts
    Then one warning with target "app.icon" carries feature "app-identity", error_kind, detail and hint

  Scenario: The Info.plist template names the app
    Given the Info.plist template shared by the build script and the bundle script
    Then CFBundleName is "Glide" and CFBundleIconFile is "AppIcon"
    And the version is a placeholder filled in at build time

  Scenario: The macOS binary embeds the Info.plist
    Given Glide is built on macOS
    Then the executable contains an embedded Info.plist whose CFBundleName is "Glide"

  Scenario: The app menu and Dock show Glide on macOS
    Given Glide is started on macOS with cargo run
    Then the bold app menu reads "Glide" and the Dock and Cmd-Tab show the glider icon

  Scenario: Glide.app can be built
    Given a macOS machine
    When scripts/bundle-macos.sh runs
    Then Glide.app contains the binary, Info.plist and AppIcon.icns
