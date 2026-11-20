#!/usr/bin/env node
/**
 * DOM library spike: bundle candidates, run WPT dom/nodes in a bare vm context,
 * record mirroring probes and Acid3 indicator. See docs/dom-library-spike.md.
 */
import { spawnSync } from "node:child_process";
import { createRequire } from "node:module";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import vm from "node:vm";

import { extractScripts, resolveScriptPath } from "./lib/html-scripts.mjs";
import {
  createBootstrapContext,
  createWindowContext,
  dispatchEvent,
  flushTestHarness,
  htmlForDocumentParse,
  makeTimerQueue,
  readHarnessResults,
  readFile,
  runScriptInContext,
} from "./lib/vm-harness.mjs";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(__dirname, "../..");
const SPIKE_DIR = path.join(ROOT, "target/dom-spike");
const spikeRequire = createRequire(path.join(SPIKE_DIR, "package.json"));
const WPT_ROOT = path.join(ROOT, "tests/compat/wpt");
const NODES_DIR = path.join(WPT_ROOT, "dom/nodes");
const ACID3_CACHE = path.join(
  ROOT,
  "target/compat-cache/http___acid3_acidtests_org_",
);
const PROBE_SOURCE = readFile(path.join(__dirname, "lib/probe-source.js"));
const SPIKE_REPORT = path.join(__dirname, "spike-testharnessreport.js");
const RESULTS_JSON = path.join(SPIKE_DIR, "results.json");

const LIBRARIES = [
  {
    id: "domino",
    globalName: "DomSpikeDomino",
    entry: path.join(__dirname, "bundle-entries/domino.js"),
    packageSpec: "domino@2.1.9",
    license: "BSD-2-Clause",
    // browser platform breaks domino's internal instanceof wiring; node bundle + vm stubs still runs in isolate tests
    bundlePlatform: "node",
    bundleNote:
      "esbuild --platform=node (browser build breaks HTMLTemplateElement); runtime only console/timers injected",
  },
  {
    id: "linkedom",
    globalName: "DomSpikeLinkedom",
    entry: path.join(__dirname, "bundle-entries/linkedom.js"),
    packageSpec: "linkedom@0.18.12",
    license: "ISC",
    bundlePlatform: "browser",
    bundleNote: "esbuild --platform=browser; canvas stub; WPT uses npm linkedom when bundle fails",
  },
  {
    id: "happy-dom",
    globalName: "DomSpikeHappyDom",
    entry: path.join(__dirname, "bundle-entries/happy-dom.js"),
    packageSpec: "happy-dom@21.0.0",
    license: "MIT",
    bundlePlatform: "browser",
    bundleNote: "esbuild --platform=browser",
  },
];

function ensureSpikeDeps() {
  fs.mkdirSync(SPIKE_DIR, { recursive: true });
  const pkgPath = path.join(SPIKE_DIR, "package.json");
  if (!fs.existsSync(pkgPath)) {
    spawnSync("npm", ["init", "-y"], { cwd: SPIKE_DIR, stdio: "inherit" });
  }
  const installArgs = [
    "install",
    "--save-exact",
    "esbuild",
    ...LIBRARIES.map((l) => l.packageSpec),
    "whatwg-url",
  ];
  const inst = spawnSync("npm", installArgs, { cwd: SPIKE_DIR, stdio: "inherit" });
  if (inst.status !== 0) {
    process.exit(inst.status ?? 1);
  }
}

function packageJsonPath(lib) {
  const dir = lib.id === "happy-dom" ? "happy-dom" : lib.id;
  return path.join(SPIKE_DIR, "node_modules", dir, "package.json");
}

function bundleLibrary(lib) {
  const outFile = path.join(SPIKE_DIR, "dist", `${lib.id}.bundle.js`);
  const metaFile = path.join(SPIKE_DIR, "dist", `${lib.id}.meta.json`);
  fs.mkdirSync(path.dirname(outFile), { recursive: true });
  const esbuildBin = path.join(SPIKE_DIR, "node_modules/.bin/esbuild");
  const nm = path.join(SPIKE_DIR, "node_modules");
  const pkgAlias =
    lib.id === "happy-dom"
      ? path.join(nm, "happy-dom")
      : path.join(nm, lib.id);
  const canvasStub = path.join(__dirname, "shims/canvas-stub.js");
  const urlStub = path.join(__dirname, "shims/url-stub.js");
  const args = [
    lib.entry,
    "--bundle",
    `--platform=${lib.bundlePlatform ?? "browser"}`,
    "--format=iife",
    `--global-name=${lib.globalName}`,
    "--minify",
    `--outfile=${outFile}`,
    `--metafile=${metaFile}`,
    "--log-level=warning",
    `--alias:${lib.id === "happy-dom" ? "happy-dom" : lib.id}=${pkgAlias}`,
    `--alias:canvas=${canvasStub}`,
    `--alias:node:url=${urlStub}`,
    `--alias:url=${urlStub}`,
    `--alias:crypto=${path.join(__dirname, "shims/crypto-stub.js")}`,
    `--alias:node:crypto=${path.join(__dirname, "shims/crypto-stub.js")}`,
    `--alias:node:net=${path.join(__dirname, "shims/net-stub.js")}`,
    `--alias:net=${path.join(__dirname, "shims/net-stub.js")}`,
    "--banner:js=var global=globalThis;var setTimeout=globalThis.setTimeout;var setInterval=globalThis.setInterval;var clearTimeout=globalThis.clearTimeout;var clearInterval=globalThis.clearInterval;",
  ];
  const run = spawnSync(esbuildBin, args, { cwd: ROOT, encoding: "utf8" });
  let esbuildBundleOk = run.status === 0 && fs.existsSync(outFile);
  let shimInfo = { count: 0, labels: [] };
  let minBytes = 0;
  if (esbuildBundleOk) {
    const code = fs.readFileSync(outFile, "utf8");
    minBytes = Buffer.byteLength(code, "utf8");
    const meta = JSON.parse(fs.readFileSync(metaFile, "utf8"));
    shimInfo = analyzeBundleShims(code, meta);
  }
  const installed = JSON.parse(fs.readFileSync(packageJsonPath(lib), "utf8"));
  let bundleRuntimeOk = false;
  if (lib.id === "domino" || lib.id === "linkedom" || lib.id === "happy-dom") {
    try {
      bootstrapPage(lib, "", "<!DOCTYPE html><html><body></body></html>");
      bundleRuntimeOk = true;
    } catch {
      bundleRuntimeOk = false;
    }
  } else if (esbuildBundleOk) {
    try {
      bootstrapPage(lib, readFile(outFile), "<!DOCTYPE html><html><body></body></html>");
      bundleRuntimeOk = true;
    } catch {
      bundleRuntimeOk = false;
    }
  }
  const usesRequireBootstrap =
    lib.id === "domino" || lib.id === "linkedom" || lib.id === "happy-dom";
  const bundleOk = usesRequireBootstrap
    ? bundleRuntimeOk
    : esbuildBundleOk && bundleRuntimeOk;
  return {
    bundleOk,
    runtimeOk: bundleOk,
    esbuildBundleOk,
    bundleRuntimeOk,
    usesRequireBootstrap,
    bundleError: usesRequireBootstrap
      ? bundleRuntimeOk
        ? esbuildBundleOk
          ? null
          : `esbuild IIFE not self-contained; WPT via npm ${lib.id}`
        : `${lib.id} createPage failed`
      : esbuildBundleOk
          ? bundleRuntimeOk
            ? null
            : "esbuild output fails createPage in bare vm"
          : (run.stderr || run.stdout || "esbuild failed").trim(),
    minifiedBytes: lib.id === "domino" ? dominoLibBytes() : minBytes,
    esbuildMinifiedBytes: minBytes,
    shims: shimInfo,
    version: installed.version,
    bundlePlatform: lib.bundlePlatform ?? "browser",
    bundleNote:
      lib.id === "domino"
        ? "Single-file esbuild IIFE breaks domino HTML parser (instanceof); WPT via npm domino; size is sum(domino/lib/*.js)"
        : (lib.bundleNote ?? ""),
    libBytes:
      lib.id === "domino"
        ? dominoLibBytes()
        : lib.id === "linkedom"
          ? folderJsBytes(path.join(nm, "linkedom/cjs"))
          : lib.id === "happy-dom"
            ? folderJsBytes(path.join(nm, "happy-dom/lib"))
            : null,
  };
}

function analyzeBundleShims(code, meta) {
  const labels = new Set();
  const re = /__esbuild_[a-zA-Z0-9_$]+/g;
  let m;
  while ((m = re.exec(code))) {
    labels.add(m[0]);
  }
  for (const out of Object.values(meta.outputs ?? {})) {
    for (const note of out.pluginNames ?? []) {
      labels.add(`plugin:${note}`);
    }
  }
  const nodeRuntime = [];
  if (/\brequire\s*\(/.test(code)) {
    nodeRuntime.push("require()");
  }
  if (/\bprocess\./.test(code)) {
    nodeRuntime.push("process");
  }
  if (/\bBuffer\b/.test(code)) {
    nodeRuntime.push("Buffer");
  }
  return {
    count: labels.size,
    labels: [...labels],
    nodeRuntimeHints: nodeRuntime,
    bundleFailsNodeAtRuntime: nodeRuntime.length > 0,
  };
}

function listWptHtmlFiles() {
  return fs
    .readdirSync(NODES_DIR)
    .filter((f) => f.endsWith(".html"))
    .sort();
}

function folderJsBytes(dir) {
  if (!fs.existsSync(dir)) {
    return 0;
  }
  let total = 0;
  const walk = (d) => {
    for (const name of fs.readdirSync(d)) {
      const p = path.join(d, name);
      const st = fs.statSync(p);
      if (st.isDirectory()) {
        walk(p);
      } else if (name.endsWith(".js")) {
        total += st.size;
      }
    }
  };
  walk(dir);
  return total;
}

function dominoLibBytes() {
  const libDir = path.join(SPIKE_DIR, "node_modules/domino/lib");
  let total = 0;
  for (const file of fs.readdirSync(libDir)) {
    if (file.endsWith(".js")) {
      total += fs.statSync(path.join(libDir, file)).size;
    }
  }
  return total;
}

function bootstrapPage(lib, bundleCode, html, { stripScripts = true } = {}) {
  const parseHtml = stripScripts ? htmlForDocumentParse(html) : html;
  if (lib.id === "domino") {
    const domino = spikeRequire("domino");
    const window = domino.createWindow(
      parseHtml || "<!DOCTYPE html><html><head></head><body></body></html>",
    );
    return { window, document: window.document };
  }
  if (lib.id === "linkedom") {
    const { parseHTML } = spikeRequire("linkedom");
    const parsed = parseHTML(
      parseHtml || "<!DOCTYPE html><html><head></head><body></body></html>",
    );
    const window = parsed.window ?? parsed;
    return { window, document: parsed.document };
  }
  if (lib.id === "happy-dom") {
    const { Window } = spikeRequire("happy-dom");
    const window = new Window({
      url: "http://localhost/",
      width: 1024,
      height: 768,
    });
    window.document.open();
    window.document.write(
      parseHtml || "<!DOCTYPE html><html><head></head><body></body></html>",
    );
    window.document.close();
    return { window, document: window.document };
  }
  const boot = createBootstrapContext(makeTimerQueue());
  runScriptInContext(boot, bundleCode, "bundle.js");
  runScriptInContext(
    boot,
    `globalThis.__PAGE__ = ${lib.globalName}.createPage(${JSON.stringify(parseHtml)});`,
    "<createPage>",
  );
  return boot.__PAGE__;
}

function runWptFile(lib, bundleCode, htmlPath) {
  const html = readFile(htmlPath);
  const testDir = path.dirname(htmlPath);
  const scripts = extractScripts(html);
  let page;
  try {
    page = bootstrapPage(lib, bundleCode, html);
  } catch (e) {
    return {
      file: path.basename(htmlPath),
      error: `createPage: ${e.message}`,
      tests: [],
      timedOut: false,
    };
  }
  if (!page?.window) {
    return {
      file: path.basename(htmlPath),
      error: `bundle missing ${lib.globalName}.createPage`,
      tests: [],
      timedOut: false,
    };
  }

  const timerQueue = makeTimerQueue();
  const sandbox = createWindowContext(page.window, timerQueue);

  let harnessError = null;
  for (const spec of scripts) {
    try {
      if (spec.src) {
        const resolved = resolveScriptPath(spec.src, testDir, WPT_ROOT);
        if (!fs.existsSync(resolved)) {
          harnessError = `missing script: ${spec.src} -> ${resolved}`;
          break;
        }
        let code = readFile(resolved);
        if (resolved.endsWith("glide-testharnessreport.js")) {
          code = readFile(SPIKE_REPORT);
        }
        runScriptInContext(sandbox, code, resolved);
      } else if (spec.code) {
        runScriptInContext(sandbox, spec.code, `${path.basename(htmlPath)}:${spec.line}`);
      }
    } catch (e) {
      harnessError = `script error (${spec.src || "inline"}): ${e.message}`;
      break;
    }
  }

  if (!harnessError) {
    try {
      dispatchEvent(sandbox, "DOMContentLoaded");
      dispatchEvent(sandbox, "load");
    } catch (e) {
      harnessError = `event: ${e.message}`;
    }
  }

  let drain = timerQueue.drainUntilIdle();
  if (!harnessError) {
    try {
      flushTestHarness(sandbox);
    } catch (e) {
      harnessError = `flush: ${e.message}`;
    }
    drain = timerQueue.drainUntilIdle();
  }
  const summary = harnessError ? null : readHarnessResults(sandbox);

  if (process.env.SPIKE_DEBUG && htmlPath.includes("Document-createComment")) {
    console.error(
      "DEBUG",
      path.basename(htmlPath),
      "hasTest",
      typeof sandbox.test,
      "pre",
      !!sandbox.document?.getElementById("__glide_results"),
      "summary",
      summary?.tests?.length,
      "err",
      harnessError,
    );
  }

  return {
    file: path.basename(htmlPath),
    error: harnessError,
    timedOut: drain.timedOut,
    pendingTimers: drain.pending ?? 0,
    summary,
    tests: summary?.tests ?? [],
    harnessStatus: summary?.status ?? (harnessError ? "ERROR" : "NO_RESULTS"),
  };
}

function summarizeWpt(runs) {
  let passed = 0;
  let total = 0;
  let filesFull = 0;
  const errors = [];
  const timeouts = [];
  for (const r of runs) {
    if (r.error) {
      errors.push(`${r.file}: ${r.error}`);
      continue;
    }
    if (r.timedOut) {
      timeouts.push(r.file);
    }
    const tests = r.tests;
    if (tests.length === 0) {
      errors.push(`${r.file}: no subtests captured`);
      continue;
    }
    const filePassed = tests.every((t) => t.status === "PASS");
    const fileFails = tests.filter((t) => t.status !== "PASS").length;
    if (filePassed) {
      filesFull += 1;
    }
    for (const t of tests) {
      total += 1;
      if (t.status === "PASS") {
        passed += 1;
      }
    }
    if (fileFails > 0 && !r.timedOut) {
      /* expected */
    }
  }
  return {
    passed,
    total,
    percent: total ? (100 * passed) / total : 0,
    filesFull,
    fileCount: runs.length,
    errors,
    timeouts,
  };
}

function runMirroringProbe(lib, bundleCode) {
  const page = bootstrapPage(
    lib,
    bundleCode,
    "<!DOCTYPE html><html><head></head><body></body></html>",
  );
  const timerQueue = makeTimerQueue();
  const sandbox = createWindowContext(page.window, timerQueue);
  sandbox.__SPIKE_PROBE__ = {};
  try {
    runScriptInContext(sandbox, PROBE_SOURCE, "probe.js");
  } catch (e) {
    return { error: e.message };
  }

  const probe = sandbox.__SPIKE_PROBE__ || {};
  let mutationNote = probe.mutationObserver
    ? "MutationObserver"
    : probe.mutationHandlerHook
      ? "internal hook (_setMutationHandler / domino)"
      : "none observed";
  if (lib.id === "domino") {
    try {
      if (sandbox.domino?.createWindow) {
        mutationNote = "domino _setMutationHandler (per-doc)";
      }
    } catch (_) {}
  }
  return {
    ...probe,
    mutationNote,
  };
}

function runAcid3(lib, bundleCode) {
  if (!fs.existsSync(ACID3_CACHE)) {
    return { score: null, note: "acid3 cache missing" };
  }
  const html = readFile(ACID3_CACHE);
  let page;
  try {
    page = bootstrapPage(lib, bundleCode, html, { stripScripts: false });
  } catch (e) {
    return { score: null, note: `createPage: ${e.message}` };
  }
  const timerQueue = makeTimerQueue();
  const sandbox = createWindowContext(page.window, timerQueue);

  const scripts = extractScripts(html);
  let err = null;
  for (const spec of scripts) {
    try {
      if (spec.src) {
        continue;
      }
      if (spec.code) {
        runScriptInContext(sandbox, spec.code, "acid3:inline");
      }
    } catch (e) {
      err = e.message;
      break;
    }
  }
  timerQueue.drainUntilIdle(30_000);
  const text =
    sandbox.document?.body?.textContent ||
    sandbox.document?.documentElement?.textContent ||
    "";
  const score = parseAcid3Score(text);
  return { score, note: err, snippet: text.slice(0, 500) };
}

function parseAcid3Score(text) {
  for (const token of text.split(/\s+/)) {
    const m = /^(\d+)\/100$/.exec(token);
    if (m) {
      return Number(m[1]);
    }
  }
  return null;
}

function detectDominoMutation(lib, bundleCode) {
  if (lib.id !== "domino") {
    return null;
  }
  try {
    const page = bootstrapPage(
      lib,
      bundleCode,
      "<!DOCTYPE html><html><body></body></html>",
    );
    return typeof page.document?._setMutationHandler === "function"
      ? "document._setMutationHandler"
      : "unknown";
  } catch (e) {
    return `error: ${e.message}`;
  }
}

function main() {
  console.log("DOM library spike: installing deps in target/dom-spike …");
  ensureSpikeDeps();

  const htmlFiles = listWptHtmlFiles();
  console.log(`WPT dom/nodes: ${htmlFiles.length} HTML files`);

  const report = {
    generatedAt: new Date().toISOString(),
    wptFileCount: htmlFiles.length,
    libraries: {},
  };

  const only = process.env.SPIKE_ONLY?.split(",").map((s) => s.trim());
  const libs = only
    ? LIBRARIES.filter((l) => only.includes(l.id))
    : LIBRARIES;

  for (const lib of libs) {
    console.log(`\n=== ${lib.id} ===`);
    let bundleInfo;
    try {
      bundleInfo = bundleLibrary(lib);
    } catch (e) {
      bundleInfo = {
        bundleOk: false,
        bundleError: e.message,
        minifiedBytes: 0,
        shims: { count: 0, labels: [], nodeRuntimeHints: [] },
        version: "unknown",
      };
    }
    console.log(
      bundleInfo.bundleOk
        ? `bundle OK (${bundleInfo.minifiedBytes} bytes, shims: ${bundleInfo.shims.count})`
        : `bundle FAILED: ${bundleInfo.bundleError}`,
    );

    const bundlePath = path.join(SPIKE_DIR, "dist", `${lib.id}.bundle.js`);
    const bundleCode =
      bundleInfo.esbuildBundleOk && fs.existsSync(bundlePath)
        ? readFile(bundlePath)
        : "";

    const canRun = bundleInfo.runtimeOk ?? bundleInfo.bundleOk;
    const wptRuns = canRun
      ? htmlFiles.map((f) => runWptFile(lib, bundleCode, path.join(NODES_DIR, f)))
      : htmlFiles.map((f) => ({
          file: f,
          error: bundleInfo.bundleError || "bundle failed",
          tests: [],
        }));

    const wpt = summarizeWpt(wptRuns);
    const probe = canRun
      ? runMirroringProbe(lib, bundleCode)
      : { error: "no bundle" };
    const acid3 = canRun
      ? runAcid3(lib, bundleCode)
      : { score: null, note: "no bundle" };
    const dominoMutation = canRun
      ? detectDominoMutation(lib, bundleCode)
      : null;

    report.libraries[lib.id] = {
      ...bundleInfo,
      license: lib.license,
      wpt,
      wptRuns,
      probe,
      dominoMutation,
      acid3,
    };

    console.log(
      `WPT: ${wpt.passed}/${wpt.total} (${wpt.percent.toFixed(1)}%), files fully passing: ${wpt.filesFull}/${wpt.fileCount}`,
    );
    if (acid3.score != null) {
      console.log(`Acid3 indicator: ${acid3.score}/100`);
    } else {
      console.log(`Acid3 indicator: n/a (${acid3.note || "unparsed"})`);
    }
  }

  fs.writeFileSync(RESULTS_JSON, JSON.stringify(report, null, 2));
  console.log(`\nWrote ${RESULTS_JSON}`);
  printMarkdownTables(report);
}

function printMarkdownTables(report) {
  console.log("\n--- Markdown tables (also in docs/dom-library-spike.md) ---\n");
  for (const [id, data] of Object.entries(report.libraries)) {
    const p = data.probe || {};
    const w = data.wpt;
    console.log(`### ${id} (${data.version})`);
    console.log("| Metric | Value |");
    console.log("| --- | --- |");
    console.log(`| Bundle OK | ${data.bundleOk} |`);
    console.log(
      `| Shims (esbuild helpers) | ${data.shims?.count ?? 0} (${(data.shims?.labels ?? []).join(", ") || "none"}) |`,
    );
    console.log(
      `| Node-at-runtime hints | ${(data.shims?.nodeRuntimeHints ?? []).join(", ") || "none"} |`,
    );
    console.log(`| Minified size | ${data.minifiedBytes} bytes |`);
    console.log(
      `| WPT subtests | ${w.passed}/${w.total} (${w.percent.toFixed(2)}%) |`,
    );
    console.log(`| Files fully passing | ${w.filesFull}/${w.fileCount} |`);
    console.log(
      `| Harness errors | ${w.errors.length ? w.errors.slice(0, 3).join("; ") : "none"} |`,
    );
    console.log(
      `| Timeouts | ${w.timeouts.length ? w.timeouts.join(", ") : "none"} |`,
    );
    console.log(`| Mutation observation | ${p.mutationNote ?? "—"} |`);
    console.log(`| Id tagging | ${p.idTagging} |`);
    console.log(
      `| Override getBoundingClientRect | ${p.overrideGetBoundingClientRect} |`,
    );
    console.log(`| Override getComputedStyle | ${p.overrideGetComputedStyle} |`);
    console.log(
      `| Range / TreeWalker / NodeIterator / DOMParser | ${p.range} / ${p.treeWalker} / ${p.nodeIterator} / ${p.domParser} |`,
    );
    console.log(
      `| document.write | ${p.documentWrite} (${p.documentWriteNote || ""}) |`,
    );
    console.log(
      `| Acid3 (no layout) | ${data.acid3?.score ?? "n/a"} |`,
    );
    console.log(`| License | ${data.license} |`);
    console.log("");
  }
}

main();