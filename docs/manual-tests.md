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
| M-008 | F-005, F-006 | Press Cmd/Ctrl+L | Address bar receives focus and its text is selected; the next typed character replaces the URL |
| M-009 | F-005 | Visit two URLs, press Alt+Left and Alt+Right (Option+arrows on macOS) | Back and forward navigate history |
| M-010 | F-005 | Press Cmd/Ctrl+[ then Cmd/Ctrl+] | Back then forward |
| M-011 | F-005 | Press Ctrl+R, F5, and on macOS Cmd+R | Page reloads each time |
| M-012 | F-005 | Press Cmd/Ctrl+W | Window closes |
| M-013 | F-005 | Press Cmd/Ctrl+Q | The process exits |
| M-014 | F-006 | Focus the address bar, type `hello`, Cmd/Ctrl+A, Cmd/Ctrl+C, move caret to end, Cmd/Ctrl+V | Field shows `hellohello` |
| M-031 | F-006 | Open a page with a paragraph, drag to select a word, press Cmd/Ctrl+C, focus the address bar, Cmd/Ctrl+V | The selected word is pasted into the address bar |
| M-030 | F-006 | Focus the address bar on a URL, press Cmd+A then Backspace. Separately, click at the end of the URL and press Cmd+Backspace | Both clear the address bar |
| M-015 | F-006 | On macOS: type `one two`, Option+Left/Right moves by word; Option+Backspace deletes the word before the caret | Caret and text follow macOS editing conventions |
| M-016 | F-007 | Open `tests/fixtures/input.html`, click the input (a first click on the page may only focus the iframe), type `x` | Characters appear in the page input, not the address bar |
| M-017 | F-022 | Open `tests/fixtures/js-toggle.html`. Press Cmd/Ctrl+Shift+J twice | After first toggle: toolbar **JS off**, page shows `static` and `no-js-fallback`. After second: **JS on**, page shows `scripted` |
| M-018 | F-022 | Open `tests/fixtures/counter.html`. Confirm `#count` shows `ready`. Toggle JS off then on | With JS off, count stays `0`; after turning JS on, count shows `ready` again |
| M-019 | F-023 | Open `https://nonexistent.invalid` | Error page shows the URL, a reason, and **Retry**; **Retry** requests the same URL again (no `glide://` navigation) |
| M-020 | F-023 | Open a slow `https://` page (or throttle locally) | Blue loading bar and **Stop** show only during the main document load; both clear when the load finishes |
| M-021 | F-023 | With a slow load in progress, press Esc or click Stop (■) | Load stops; previous page stays visible; history and address bar match the displayed page |
| M-022 | F-024 | Open a long local page (`tests/fixtures/find.html`). Press Cmd/Ctrl+= several times, then Cmd/Ctrl+-, then Cmd/Ctrl+0 | Page zooms in 10% steps between 30% and 300%; reset returns to 100%; toolbar stays the same visual size |
| M-023 | F-024 | Press Cmd/Ctrl+F, type `alpha` | Find bar opens with focus; matches highlight and scroll into view; count shows `n of m` |
| M-024 | F-024 | With find open, press Enter and Cmd/Ctrl+G, then Shift+Enter and Cmd/Ctrl+Shift+G | Selection moves to next/previous match and wraps around |
| M-025 | F-024 | With find open, press Esc | Find bar closes and page selection clears |
| M-026 | F-025 | Visit two `https://` pages, quit (Cmd/Ctrl+Q), relaunch with no URL argument | Same page as at quit; back returns to the previous page |
| M-027 | F-025 | Toggle JS off (toolbar or Cmd/Ctrl+Shift+J), quit, relaunch with no URL | JS stays off until toggled again |
| M-028 | F-025 | Bookmark a page (star or Cmd/Ctrl+D). In the address bar open `glide://bookmarks` and click a link | Bookmarks page lists the entry; the link navigates the page iframe |
| M-029 | F-025 | Log into a cookie-based site, navigate to another page on that host, confirm still logged in | Session cookies work for the lifetime of one Glide process (not across restarts) |