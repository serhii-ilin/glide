# Manual checks

Behavior that automated tests cannot reach (a real window, OS input, focus). Run with
`cargo run --release` before marking a related feature `done` in `docs/features.md`.
Each row names the feature it covers.

| # | Feature | Steps | Expected |
| --- | --- | --- | --- |
| M-001 | F-004 | Open `https://example.com` | Window title reads "Example Domain — Glide" |
| M-002 | F-007 | Open a page with a text input, click it, type `abc`, Backspace | Input shows `ab` |
| M-003 | F-010 | `cargo run --release -- --screenshot https://example.com /tmp/x.png` | PNG shows the page |
