# Glide compatibility report

Date: 2026-11-24
Git: `ed2cbb6`
OS: macos

## Summary

| Suite | Pass | Fail | Other | Total |
| --- | ---: | ---: | ---: | ---: |
| acid | 1 | 1 | 2 | 4 |
| wpt | 80 | 48 | 0 | 128 |
| probe | 71 | 3 | 0 | 74 |

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

- wpt:reftest — rendering differs (pixel diff): 31
- wpt:testharness — missing __glide_results JSON in dump-text: 3
- probe:api — Web API missing (undefined typeof): 3
- wpt:testharness — 2 failed, 0 timeout: 2
- wpt:testharness — 1 failed, 0 timeout: 2
- wpt:testharness — harness did not load (ReferenceError: async_test is not defined): 2
- wpt:testharness — 148 failed, 0 timeout: 1
- wpt:testharness — 3 failed, 0 timeout: 1
- wpt:testharness — 117 failed, 0 timeout: 1
- wpt:testharness — 47 failed, 0 timeout: 1
- wpt:testharness — harness did not load (INVALID_CHARACTER_ERR: INVALID_CHARACTER_ERR (5): the string contains invalid characters): 1
- wpt:testharness — harness did not load (ReferenceError: setup is not defined): 1
- wpt:testharness — harness did not load (Error: assert_throws_dom: function "() => document.createCDATASection("foo")" threw object "TypeError: document.createCDATASection is not a function" that is not a DOMException NotSupportedError: property "code" is equal to undefined, expected 9): 1
- wpt:testharness — 5 failed, 0 timeout: 1
- acid:reftest — Acid2 test page vs reference (expected differences): 1

## API probe

Rows marked **available** for language built-ins (e.g. `Map`, `Set`, `Proxy`, `Intl`, `Reflect`, `Symbol`, `WeakMap`) reflect ECMAScript globals from V8, not host Web APIs.

| API | Status |
| --- | --- |
| Worker | missing (undefined) |
| indexedDB | missing (undefined) |
| structuredClone | missing (undefined) |
| AbortController | available (function) |
| AbortSignal | available (function) |
| Array.prototype.at | available (function) |
| Blob | available (function) |
| CSS.supports | available (function) |
| CustomEvent | available (function) |
| DOMParser | available (function) |
| Element.prototype.attachShadow | available (function) |
| Element.prototype.closest | available (function) |
| Element.prototype.insertAdjacentHTML | available (function) |
| FileReader | available (function) |
| FormData | available (function) |
| Headers | available (function) |
| IntersectionObserver | available (function) |
| Intl | available (object) |
| Map | available (function) |
| MutationObserver | available (function) |
| Object.hasOwn | available (function) |
| Promise | available (function) |
| Proxy | available (function) |
| Reflect | available (object) |
| Request | available (function) |
| ResizeObserver | available (function) |
| Response | available (function) |
| Set | available (function) |
| Symbol | available (function) |
| TextDecoder | available (function) |
| TextEncoder | available (function) |
| URL | available (function) |
| URLSearchParams | available (function) |
| WeakMap | available (function) |
| WebSocket | available (function) |
| XMLHttpRequest | available (function) |
| atob | available (function) |
| btoa | available (function) |
| console | available (object) |
| console.info | available (function) |
| crypto.getRandomValues | available (function) |
| crypto.randomUUID | available (function) |
| customElements | available (object) |
| document | available (object) |
| document.addEventListener | available (function) |
| document.dispatchEvent | available (function) |
| document.documentElement.classList | available (object) |
| document.documentElement.cloneNode | available (function) |
| document.documentElement.dataset | available (object) |
| document.documentElement.focus | available (function) |
| document.documentElement.getBoundingClientRect | available (function) |
| document.documentElement.innerHTML | available (string) |
| document.documentElement.scrollIntoView | available (function) |
| document.querySelector | available (function) |
| fetch | available (function) |
| getComputedStyle | available (function) |
| history.pushState | available (function) |
| history.replaceState | available (function) |
| localStorage | available (object) |
| location | available (object) |
| matchMedia | available (function) |
| navigator | available (object) |
| navigator.languages | available (object) |
| navigator.sendBeacon | available (function) |
| navigator.userAgent | available (string) |
| performance | available (object) |
| performance.getEntriesByType | available (function) |
| performance.timing | available (object) |
| queueMicrotask | available (function) |
| requestAnimationFrame | available (function) |
| requestIdleCallback | available (function) |
| sessionStorage | available (object) |
| window | available (object) |

## Acid scores

- `acid1/test5526c.htm` — **manual** 
- `acid2/compare` — **fail** {"diff_pct":3.7385167464114835}
- `acid3/with-js` — **pass** {"acid3_score":36}
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
