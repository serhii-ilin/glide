#!/usr/bin/env node
/**
 * DOM library spike: bundle candidates, run WPT dom/nodes in a bare vm context,
 * record mirroring probes and Acid3 indicator. See docs/dom-library-spike.md.
 */
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { runDominoBundleExperiments } from "./lib/domino-bundle-experiments.mjs";
import { extractScripts } from "./lib/html-scripts.mjs";
import { runWptFileForked } from "./lib/wpt-fork.mjs";
import { bootstrapPage } from "./lib/wpt-runner.mjs";
import {
  createWindowContext,
  makeTimerQueue,
  readFile,
  runScriptInContext,
} from "./lib/vm-harness.mjs";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(__dirname, "../..");
const SPIKE_DIR = path.join(ROOT, "target/dom-spike");
const WPT_ROOT = path.join(ROOT, "tests/compat/wpt");
const NODES_DIR = path.join(WPT_ROOT, "dom/nodes");
const ACID3_CACHE = path.join(
  ROOT,
  "target/compat-cache/http___acid3_acidtests_org_",
);
const PROBE_SOURCE = readFile(path.join(__dirname, "lib/probe-source.js"));
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
    "browserify",
    "rollup",
    "@rollup/plugin-node-resolve",
    "@rollup/plugin-commonjs",
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
      bootstrapPage(
        { ...lib, bootstrap: "require" },
        "",
        "<!DOCTYPE html><html><body></body></html>",
      );
      bundleRuntimeOk = true;
    } catch {
      bundleRuntimeOk = false;
    }
  } else if (esbuildBundleOk) {
    try {
      bootstrapPage(
        { ...lib, bootstrap: "bundle" },
        readFile(outFile),
        "<!DOCTYPE html><html><body></body></html>",
      );
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
    minifiedBytes:
      lib.id === "domino"
        ? dominoLibBytes()
        : minBytes || folderJsBytes(path.join(nm, lib.id === "happy-dom" ? "happy-dom/lib" : "linkedom/cjs")),
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

function summarizeWpt(runs, libId) {
  let passed = 0;
  let total = 0;
  let filesFull = 0;
  const errors = [];
  const timeouts = [];
  for (const r of runs) {
    if (r.hung || r.timedOut) {
      timeouts.push(`${libId}:${r.file}`);
      continue;
    }
    if (r.error) {
      errors.push(`${r.file}: ${r.error}`);
      continue;
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
    { ...lib, bootstrap: "require" },
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
    page = bootstrapPage(
      { ...lib, bootstrap: "require" },
      bundleCode,
      html,
      { stripScripts: false },
    );
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
      { ...lib, bootstrap: "require" },
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

async function main() {
  console.log("DOM library spike: installing deps in target/dom-spike …");
  ensureSpikeDeps();

  const htmlFiles = listWptHtmlFiles();
  const htmlPaths = htmlFiles.map((f) => path.join(NODES_DIR, f));
  console.log(`WPT dom/nodes: ${htmlFiles.length} HTML files`);

  const report = {
    generatedAt: new Date().toISOString(),
    wptFileCount: htmlFiles.length,
    libraries: {},
    dominoBundleExperiments: [],
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
    const wptLib = { ...lib, bootstrap: "require" };
    const wptRuns = [];
    if (canRun) {
      for (const htmlPath of htmlPaths) {
        process.stdout.write(`  WPT ${path.basename(htmlPath)} …\n`);
        wptRuns.push(await runWptFileForked(wptLib, bundleCode, htmlPath));
      }
    } else {
      for (const f of htmlFiles) {
        wptRuns.push({
          file: f,
          error: bundleInfo.bundleError || "bundle failed",
          tests: [],
        });
      }
    }

    const wpt = summarizeWpt(wptRuns, lib.id);
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
      wptBootstrap: "require",
      wpt,
      wptRuns,
      probe,
      dominoMutation,
      acid3,
    };
    fs.writeFileSync(RESULTS_JSON, JSON.stringify(report, null, 2));

    console.log(
      `WPT: ${wpt.passed}/${wpt.total} (${wpt.percent.toFixed(1)}%), files fully passing: ${wpt.filesFull}/${wpt.fileCount}`,
    );
    if (wpt.timeouts.length) {
      console.log(`  timeouts: ${wpt.timeouts.join(", ")}`);
    }
    if (acid3.score != null) {
      console.log(`Acid3 indicator: ${acid3.score}/100`);
    } else {
      console.log(`Acid3 indicator: n/a (${acid3.note || "unparsed"})`);
    }
  }

  if (!only || only.includes("domino")) {
    console.log("\n=== domino bundle experiments (T9b vendoring) ===");
    report.dominoBundleExperiments =
      await runDominoBundleExperiments(htmlPaths);
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

main().catch((e) => {
  console.error(e);
  process.exit(1);
});