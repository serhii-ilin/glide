# Manual checks

Behavior that automated tests cannot reach (a real window, OS input, focus). Run with
`cargo run --release` before marking a related feature `done` in `docs/features.md`.
Each row names the feature it covers.

| # | Feature | Steps | Expected |
| --- | --- | --- | --- |
| M-001 | F-004 | Open `https://example.com` | Window title reads "Example Domain — Glide" |
| M-002 | F-007 | `cargo run --release -- tests/fixtures/input.html`. Click the input, type `abc`, Backspace | Input shows `ab` |
| M-003 | F-010 | `cargo run --release -- --screenshot https://example.com /tmp/x.png` | PNG shows the page |
| M-004 | F-005 | Focus the address bar, type `example.com`, press Enter | Page navigates to Example Domain |
| M-005 | F-005 | Open a long page. Click the page (not the address bar). Press Space several times | Viewport scrolls down |
| M-006 | F-005 | Same page, press Shift+Space, PgUp, PgDn, Home, End, and arrow keys with no text field focused | Page scrolls in the matching direction |
| M-007 | F-005 | With the address bar focused, press Space | A space is typed; the page does not scroll |
| M-008 | F-005 | Press Cmd/Ctrl+L | Address bar receives focus |
| M-009 | F-005 | Visit two URLs, press Alt+Left and Alt+Right (Option+arrows on macOS) | Back and forward navigate history |
| M-010 | F-005 | Press Cmd/Ctrl+[ then Cmd/Ctrl+] | Back then forward |
| M-011 | F-005 | Press Ctrl+R, F5, and on macOS Cmd+R | Page reloads each time |
| M-012 | F-005 | Press Cmd/Ctrl+W | Window closes |
| M-013 | F-005 | Press Cmd/Ctrl+Q | The process exits |
| M-014 | F-006 | Focus the address bar, type `hello`, Cmd/Ctrl+A, Cmd/Ctrl+C, move caret to end, Cmd/Ctrl+V | Field shows `hellohello` |
| M-015 | F-006 | On macOS: type `one two`, Option+Left/Right moves by word; Option+Backspace deletes the word before the caret | Caret and text follow macOS editing conventions |
| M-016 | F-007 | Open `tests/fixtures/input.html`, click the input (a first click on the page may only focus the iframe), type `x` | Characters appear in the page input, not the address bar |
| M-017 | F-022 | Open `tests/fixtures/js-toggle.html`. Press Cmd/Ctrl+Shift+J twice | After first toggle: toolbar **JS off**, page shows `static` and `no-js-fallback`. After second: **JS on**, page shows `scripted` |
| M-018 | F-022 | Open `tests/fixtures/counter.html`. Confirm `#count` shows `ready`. Toggle JS off then on | With JS off, count stays `0`; after turning JS on, count shows `ready` again |