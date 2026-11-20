# Glide compatibility report

Date: 2026-11-20
Git: `a08317c`
OS: macos

## Summary

| Suite | Pass | Fail | Other | Total |
| --- | ---: | ---: | ---: | ---: |
| acid | 0 | 2 | 2 | 4 |
| wpt | 68 | 60 | 0 | 128 |
| probe | 21 | 37 | 0 | 58 |

## Super-critical

None recorded.

## Top failure categories

- probe:api — Web API missing (undefined typeof): 37
- wpt:reftest — rendering differs (pixel diff): 31
- wpt:testharness — harness did not load (ReferenceError: self is not defined): 26
- wpt:testharness — harness did not load (ReferenceError: async_test is not defined): 2
- wpt:testharness — harness did not load (ReferenceError: setup is not defined): 1
- acid:score — could not parse NN/100 score; TypeError: document.write is not a function: 1
- acid:reftest — Acid2 test page vs reference (expected differences): 1

## API probe

Rows marked **available** for language built-ins (e.g. `Map`, `Set`, `Proxy`, `Intl`, `Reflect`, `Symbol`, `WeakMap`) reflect ECMAScript globals from V8, not host Web APIs.

| API | Status |
| --- | --- |
| Blob | missing (undefined) |
| CSS.supports | missing (undefined) |
| CustomEvent | missing (undefined) |
| DOMParser | missing (undefined) |
| Element.prototype.attachShadow | missing (undefined) |
| Element.prototype.closest | missing (undefined) |
| Element.prototype.insertAdjacentHTML | missing (undefined) |
| FileReader | missing (undefined) |
| FormData | missing (undefined) |
| IntersectionObserver | missing (undefined) |
| MutationObserver | missing (undefined) |
| ResizeObserver | missing (undefined) |
| TextEncoder | missing (undefined) |
| URL | missing (undefined) |
| URLSearchParams | missing (undefined) |
| WebSocket | missing (undefined) |
| Worker | missing (undefined) |
| XMLHttpRequest | missing (undefined) |
| crypto.getRandomValues | missing (undefined) |
| customElements | missing (undefined) |
| document.documentElement.classList | missing (undefined) |
| document.documentElement.cloneNode | missing (undefined) |
| document.documentElement.dataset | missing (undefined) |
| document.documentElement.focus | missing (undefined) |
| document.documentElement.getBoundingClientRect | missing (undefined) |
| document.documentElement.innerHTML | missing (undefined) |
| document.documentElement.scrollIntoView | missing (undefined) |
| getComputedStyle | missing (undefined) |
| history.pushState | missing (undefined) |
| indexedDB | missing (undefined) |
| localStorage | missing (undefined) |
| location | missing (undefined) |
| matchMedia | missing (undefined) |
| navigator | missing (undefined) |
| navigator.userAgent | missing (undefined) |
| sessionStorage | missing (undefined) |
| structuredClone | missing (undefined) |
| Array.prototype.at | available (function) |
| Intl | available (object) |
| Map | available (function) |
| Object.hasOwn | available (function) |
| Promise | available (function) |
| Proxy | available (function) |
| Reflect | available (object) |
| Set | available (function) |
| Symbol | available (function) |
| WeakMap | available (function) |
| console | available (object) |
| document | available (object) |
| document.addEventListener | available (function) |
| document.dispatchEvent | available (function) |
| document.querySelector | available (function) |
| fetch | available (function) |
| performance | available (object) |
| queueMicrotask | available (function) |
| requestAnimationFrame | available (function) |
| window | available (object) |

## Acid scores

- `acid1/test5526c.htm` — **unreachable** 
- `acid2/compare` — **fail** {"diff_pct":3.7385167464114835}
- `acid3/with-js` — **fail** 
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
