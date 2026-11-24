---
slug: "site-reddit-infinite-scroll"
kind: "feature"
title: "Reddit feed loads more posts on scroll"
status: "planned"
requirements: []
gherkin: "planned (spec added when promoted)"
tests: []
mcp: "`glide_scroll`, `glide_snapshot_text`"
manual: []
---

Scrolling the Reddit home feed to its end makes the page request and append the next page of posts, as in other browsers.

## Notes

Investigation on 2026-11-24 (`glide --mcp`, `glide_scroll` to the bottom): the feed paints and the viewport scrolls and settles, but `scroll_height` stays 2214 and no more posts load. Findings so far:

- The next-page trigger is `<faceplate-partial id="feed-next-page-partial" slot="load-after" loading="programmatic" src="/svc/shreddit/feeds/popular-feed?after=...">`, a zero-size element at the end of `shreddit-feed`.
- The page does not observe it with `IntersectionObserver` (only Glide's own lazy-image observers exist). It registers `scroll` listeners on `window`, `document`, `html`, `body` and `#right-sidebar-container`, and no next-page `fetch` follows a scroll. The cause is still unknown: look at what the scroll handler or `faceplate-partial` checks (`scrollY`, `scrollHeight`, `getBoundingClientRect`, visibility, an upgraded custom element).
- A related engine gap was fixed on `feat/headless-scroll`: zero-area `IntersectionObserver` targets never intersected (`intersection-observer` feature).
- Next step: read the feed and `faceplate-partial` bundles (loaded from `redditstatic.com/js/concat?...`) and add an offline snapshot fixture that reproduces the missing trigger.
