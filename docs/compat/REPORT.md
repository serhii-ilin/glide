# Glide compatibility report

Date: 2026-11-20
Git: `504b190`
OS: macos

## Summary

| Suite | Pass | Fail | Other | Total |
| --- | ---: | ---: | ---: | ---: |
| acid | 1 | 1 | 2 | 4 |
| wpt | 79 | 49 | 0 | 128 |
| probe | 36 | 22 | 0 | 58 |

## Testharness (`dom/nodes`)

Files with no subtest results count as failures (one failed subtest each). The pass rate includes those files; it is not computed only over files that produced a harness report.

| | Pass | Total | Rate |
| --- | ---: | ---: | ---: |
| Files fully passing | 12 | 28 | 42.9% |
| Subtests | 636 | 969 | 65.6% |

Files with no results: 8.

## Super-critical

None recorded.

## Top failure categories

- wpt:reftest — rendering differs (pixel diff): 32
- probe:api — Web API missing (undefined typeof): 22
- wpt:testharness — missing __glide_results JSON in dump-text: 3
- wpt:testharness — 1 failed, 0 timeout: 2
- wpt:testharness — 2 failed, 0 timeout: 2
- wpt:testharness — harness did not load (ReferenceError: async_test is not defined): 2
- wpt:testharness — harness did not load (ReferenceError: setup is not defined): 1
- acid:reftest — Acid2 test page vs reference (expected differences): 1
- wpt:testharness — 117 failed, 0 timeout: 1
- wpt:testharness — 47 failed, 0 timeout: 1
- wpt:testharness — harness did not load (INVALID_CHARACTER_ERR: INVALID_CHARACTER_ERR (5): the string contains invalid characters): 1
- wpt:testharness — 148 failed, 0 timeout: 1
- wpt:testharness — 5 failed, 0 timeout: 1
- wpt:testharness — 3 failed, 0 timeout: 1
- wpt:testharness — harness did not load (Error: assert_throws_dom: function "() => document.createCDATASection("foo")" threw object "TypeError: document.createCDATASection is not a function" that is not a DOMException NotSupportedError: property "code" is equal to undefined, expected 9): 1

## API probe

Rows marked **available** for language built-ins (e.g. `Map`, `Set`, `Proxy`, `Intl`, `Reflect`, `Symbol`, `WeakMap`) reflect ECMAScript globals from V8, not host Web APIs.

| API | Status |
| --- | --- |
| Blob | missing (undefined) |
| CSS.supports | missing (undefined) |
| Element.prototype.attachShadow | missing (undefined) |
| FileReader | missing (undefined) |
| FormData | missing (undefined) |
| IntersectionObserver | missing (undefined) |
| MutationObserver | missing (undefined) |
| ResizeObserver | missing (undefined) |
| TextEncoder | missing (undefined) |
| WebSocket | missing (undefined) |
| Worker | missing (undefined) |
| XMLHttpRequest | missing (undefined) |
| crypto.getRandomValues | missing (undefined) |
| customElements | missing (undefined) |
| document.documentElement.dataset | missing (undefined) |
| document.documentElement.scrollIntoView | missing (undefined) |
| history.pushState | missing (undefined) |
| indexedDB | missing (undefined) |
| localStorage | missing (undefined) |
| matchMedia | missing (undefined) |
| sessionStorage | missing (undefined) |
| structuredClone | missing (undefined) |
| Array.prototype.at | available (function) |
| CustomEvent | available (function) |
| DOMParser | available (function) |
| Element.prototype.closest | available (function) |
| Element.prototype.insertAdjacentHTML | available (function) |
| Intl | available (object) |
| Map | available (function) |
| Object.hasOwn | available (function) |
| Promise | available (function) |
| Proxy | available (function) |
| Reflect | available (object) |
| Set | available (function) |
| Symbol | available (function) |
| URL | available (function) |
| URLSearchParams | available (function) |
| WeakMap | available (function) |
| console | available (object) |
| document | available (object) |
| document.addEventListener | available (function) |
| document.dispatchEvent | available (function) |
| document.documentElement.classList | available (object) |
| document.documentElement.cloneNode | available (function) |
| document.documentElement.focus | available (function) |
| document.documentElement.getBoundingClientRect | available (function) |
| document.documentElement.innerHTML | available (string) |
| document.querySelector | available (function) |
| fetch | available (function) |
| getComputedStyle | available (function) |
| location | available (object) |
| navigator | available (object) |
| navigator.userAgent | available (string) |
| performance | available (object) |
| queueMicrotask | available (function) |
| requestAnimationFrame | available (function) |
| window | available (object) |

## Acid scores

- `acid1/test5526c.htm` — **unreachable** 
- `acid2/compare` — **fail** {"diff_pct":3.7385167464114835}
- `acid3/with-js` — **pass** {"acid3_score":34}
- `acid3/no-js` — **info** 

## ECMAScript

Language conformance is provided by embedded V8; see [V8 test262 status](https://v8.dev/docs/test262).
This run includes a short in-page ES smoke test only.

## Benchmarks considered

- **Acid1–3**: classic layout/DOM/JS regression targets; Acid1-3 URLs used live (not vendored).
- **WPT subset**: BSD-licensed reftests and testharness files vendored under `tests/compat/wpt/`.
- **css3test.com / html5test**: JS-heavy feature probes; deferred (overlap with probe + WPT).
- **test262**: V8 responsibility; not re-run here.
- **Speedometer / JetStream**: performance, not compatibility.

### Acid1 fetch note

Acid1 was unreachable in this run. If the reason includes HTTP 403/406, w3.org may be rejecting requests that lack a typical browser `User-Agent` (Glide does not spoof one).
