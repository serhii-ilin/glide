# Glide compatibility report

Date: 2026-11-20
Git: `fcfdecc`
OS: macos

## Summary

| Suite | Pass | Fail | Other | Total |
| --- | ---: | ---: | ---: | ---: |
| acid | 1 | 2 | 1 | 4 |
| wpt | 68 | 60 | 0 | 128 |
| probe | 21 | 37 | 0 | 58 |

## Super-critical

None recorded.

## Top failure categories

- probe:api: 38
- wpt:reftest: 31
- wpt:testharness: 29
- acid:reftest: 1
- acid:score: 1

## API probe

| API | typeof |
| --- | --- |
| Map | function |
| WebSocket | undefined |
| MutationObserver | undefined |
| document.documentElement.getBoundingClientRect | undefined |
| Reflect | object |
| FileReader | undefined |
| Proxy | function |
| document.documentElement.innerHTML | undefined |
| TextEncoder | undefined |
| matchMedia | undefined |
| Blob | undefined |
| Set | function |
| location | undefined |
| document.documentElement.cloneNode | undefined |
| document.querySelector | function |
| document.dispatchEvent | function |
| URLSearchParams | undefined |
| Symbol | function |
| document.documentElement.classList | undefined |
| FormData | undefined |
| Array.prototype.at | function |
| Element.prototype.insertAdjacentHTML | undefined |
| document.documentElement.scrollIntoView | undefined |
| Intl | object |
| indexedDB | undefined |
| document | object |
| Object.hasOwn | function |
| window | object |
| WeakMap | function |
| localStorage | undefined |
| document.documentElement.dataset | undefined |
| URL | undefined |
| sessionStorage | undefined |
| ResizeObserver | undefined |
| requestAnimationFrame | function |
| console | object |
| getComputedStyle | undefined |
| crypto.getRandomValues | undefined |
| navigator.userAgent | undefined |
| document.addEventListener | function |
| navigator | undefined |
| fetch | function |
| customElements | undefined |
| CSS.supports | undefined |
| structuredClone | undefined |
| Promise | function |
| history.pushState | undefined |
| Element.prototype.attachShadow | undefined |
| XMLHttpRequest | undefined |
| CustomEvent | undefined |
| DOMParser | undefined |
| document.documentElement.focus | undefined |
| queueMicrotask | function |
| Element.prototype.closest | undefined |
| performance | object |
| IntersectionObserver | undefined |
| Worker | undefined |

## Acid scores

- `acid1/test5526c.htm` — **unreachable** 
- `acid2/compare` — **fail** {"diff_pct":3.7385167464114835}
- `acid3/with-js` — **fail** 
- `acid3/no-js` — **pass** 

## ECMAScript

Language conformance is provided by embedded V8; see [V8 test262 status](https://v8.dev/docs/test262).
This run includes a short in-page ES smoke test only.

## Benchmarks considered

- **Acid1–3**: classic layout/DOM/JS regression targets; Acid1-3 URLs used live (not vendored).
- **WPT subset**: BSD-licensed reftests and testharness files vendored under `tests/compat/wpt/`.
- **css3test.com / html5test**: JS-heavy feature probes; deferred (overlap with probe + WPT).
- **test262**: V8 responsibility; not re-run here.
- **Speedometer / JetStream**: performance, not compatibility.
