---
slug: "mcp-headless-session"
kind: "feature"
title: "MCP headless session: navigation and page inspection"
status: "done"
aliases:
  [
    "F-066"
  ]
requirements:
  [
    "agent-drive-headless-glide-session"
  ]
gherkin: "tests/features/mcp-headless-session.feature"
tests:
  [
    "`control_session::*`; `control::protocol::tests::*`; `control::page_ops::tests::*`; `control::session::tests::*`; `headless::tests::fetch_page_decodes_percent_encoded_file_path`; `js::console_feed::tests::drain_for_tab_keeps_other_tabs`; `shot::tests::encode_png_roundtrip`; `mcp_stdio::*`; `mcp::tests::*`; `mcp::tools::tests::*`; `args::tests::mcp_*`"
  ]
mcp: "`glide_navigate`, `glide_back`, `glide_forward`, `glide_reload`, `glide_page_info`, `glide_snapshot_text`, `glide_dom`, `glide_query`, `glide_computed_style`, `glide_screenshot`"
manual:
  [
    {
      "steps": "Build release (`cargo build --release`). Run `claude mcp add glide -- $(pwd)/target/release/glide --mcp` (or add the JSON `mcpServers` entry from README). In Claude Code, call `glide_navigate` with `file://` path to `tests/fixtures/mcp-page-a.html`, then `glide_snapshot_text`, then `glide_screenshot`",
      "expected": "Tools list includes the nine navigation/inspection tools; navigation returns title \"Page A\"; text includes \"MCP Page A\"; screenshot returns a PNG",
      "aliases": [
        "M-046"
      ]
    }
  ]
---

`glide --mcp` runs an MCP server on stdio with its own headless page; an agent can navigate, go back, forward and reload, and read page info, visible text, DOM by selector, element boxes and a PNG screenshot.

## Notes

Served by `glide --mcp` (rmcp 3.11). Never opens the user profile. Scrolling and the full-page screenshot option are in `mcp-scroll`. Known gaps: `glide_snapshot_text` reuses the `--dump-text` DOM walk, so `display:none` text is included (planned: layout-aware visible text); pipelined tool calls are not ordered (clients should wait for each result).
