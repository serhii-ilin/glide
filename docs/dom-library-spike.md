# DOM library spike (T9a)

Glide’s V8 isolate has no Node `require`, so a future page-DOM integration (T9b) needs a
third-party library that (1) scores well on vendored WPT `dom/nodes` in a bare `vm` context,
(2) supports mirroring mutations to Blitz, and (3) ideally ships as one self-contained script.

This spike measures **domino** 2.1.9, **linkedom** 0.18.12, and **happy-dom** 21.0.0. Results
below are from the committed harness; numbers also land in `target/dom-spike/results.json`
(gitignored).

## Reproduce

```sh
# 15-minute outer wall clock (recommended)
perl -e 'alarm 900; exec @ARGV' node scripts/dom-spike/run.mjs

# Optional: one library only
SPIKE_ONLY=domino node scripts/dom-spike/run.mjs
```

First run installs packages under `target/dom-spike/` (`npm init`, `esbuild`, the three
libraries, `browserify`, `rollup` plugins). WPT inputs are the 30 HTML files in
`tests/compat/wpt/dom/nodes/`. Each file runs in a **`child_process.fork` worker** with a
**15 s** wall clock; VM scripts use `{ timeout: 5000 }`.

Acid3 uses `target/compat-cache/http___acid3_acidtests_org_` when present (compat runner
cache). Acid3 is layout-heavy; scores here are indicative only (no layout stubs).

Measured: **2026-11-20** (harness after fork timeouts, `productions.js` vendored).

## WPT `dom/nodes` (require bootstrap, library `window` as VM realm)

| Library | Bundle OK (esbuild IIFE) | Size (notes) | Subtests pass | Files 100% | Timeouts |
| --- | --- | --- | --- | --- | --- |
| **domino** | esbuild builds but **not** used for WPT | ~798 KiB raw `lib/*.js`; esbuild min IIFE ~336 KiB | **353 / 545 (64.8%)** | **13 / 30** | `domino:DOMImplementation-createDocument.html` |
| **linkedom** | Yes (browser IIFE ~256 KiB min) | same | **140 / 424 (33.0%)** | **1 / 30** | `linkedom:ChildNode-after.html`, `linkedom:ChildNode-before.html` |
| **happy-dom** | esbuild IIFE fails in some environments; WPT via npm | `happy-dom/lib` tree | **319 / 593 (53.8%)** | **3 / 30** | `happy-dom:DOMImplementation-createDocument.html` |

Common harness issues (all libs): iframe / browsing-context crash tests expect `contentDocument`;
`Document-URL` and character-set helpers capture 0 subtests without full encoding stack; several
domino files hit `instanceof` errors on `CDATASection` / attribute edges.

Subtests with status **ERROR** in a file still count toward totals when the harness reported
them before failure; files with **no captured subtests** (errors, timeouts) are excluded from
the pass % denominator.

## Mirroring readiness (T9b)

| Capability | domino | linkedom | happy-dom |
| --- | --- | --- | --- |
| Mutation observation | `_setMutationHandler` on document (no `MutationObserver`) | `MutationObserver` | `MutationObserver` |
| Id tagging on nodes | yes | yes | yes |
| Override `getBoundingClientRect` | yes | yes | yes |
| Override `getComputedStyle` | **no** (not on window) | no | **yes** |
| Range / TreeWalker / NodeIterator / DOMParser | no / no / no / no | no / no / no / **yes** | **yes** / **yes** / **yes** / **yes** |
| `document.write` | yes (limited) | no | yes |
| Acid3 (no layout) | unparsed (page text not `N/100`) | fails early (`document.write`) | unparsed |

## Domino single-file bundling (T9b blocker)

Glide cannot `require()` domino inside V8. We tried shipping one script:

| Variant | createPage in bare VM | WPT match npm domino? |
| --- | --- | --- |
| esbuild IIFE `platform=node`, minify | **fail** (`instanceof` / parser) | — |
| esbuild IIFE `platform=node`, `--keep-names` | **fail** (parser error `Expected /^[^ ?*+#{]+/`) | — |
| esbuild IIFE `platform=browser`, `--keep-names` | **fail** (same) | — |
| esbuild CJS + hand `module.exports` loader | **fail** (same) | — |
| browserify `--standalone` | **fail** | — |
| rollup IIFE + commonjs | **fail** (`domino is not defined` / tiny broken bundle) | — |

**None** of the bundled variants reached the npm **~65%** WPT score. Root cause: esbuild/browserify
flatten domino’s circular `require` graph; internal `instanceof` checks between `HTMLTemplateElement`
and related prototypes break before `createWindow()` can parse even a trivial document.

**Implication for T9b:** vendoring **unbundled** `domino/lib/*.js` with a small CJS loader in
the Rust embedder (multiple scripts, not one IIFE) is the realistic path if domino is chosen.
A single minified IIFE is not feasible without upstream domino changes.

## Recommendation

**Primary: domino (npm, multi-file vendoring)** for WPT coverage and the explicit
`_setMutationHandler` hook aligned with Glide’s RPC mirror design.

- Meets the ≥15% subtests bar by a wide margin (64.8%).
- Best fit for “observe every mutation and forward to Blitz” without patching the library.
- Trade-offs: no `MutationObserver`, no `DOMParser` / `Range` (Acid3 / modern APIs), weak
  `getComputedStyle`, and **no working single-file bundle**.

**Secondary: happy-dom** if the project prioritizes standards surface area (`MutationObserver`,
`DOMParser`, layout hook overrides) over raw WPT score (53.8% here). Still needs a deliberate
bundle story (esbuild IIFE + shims; size larger than linkedom).

**Not recommended: linkedom** for this milestone — lowest WPT score, two wall-clock timeouts on
common DOM mutation tests, no `document.write`.

## Risks for T9b

1. **Bundle vs. multi-file:** domino likely ships as many files + loader, not one IIFE.
2. **Mirroring:** domino uses per-document hooks; happy-dom/linkedom prefer `MutationObserver`
   (extra work in the worker thread).
3. **Layout proxies:** only happy-dom exposes an override-friendly `getComputedStyle`; Glide
   must still route both APIs to Blitz by node id.
4. **Timeouts:** some WPT files sync-loop or spin; production harness must keep the fork + 5 s
   VM caps used here.
5. **Glide shim gap:** today’s hand-written DOM is far below all three libraries; replacing it
   is a large embed + snapshot + mirror project (out of scope for T9a).

## See also

- Harness: `scripts/dom-spike/run.mjs`
- Spike report script: `scripts/dom-spike/spike-testharnessreport.js`
- Fork timeout test: `scripts/dom-spike/tests/fork-timeout.test.mjs`, `tests/dom_spike.rs`