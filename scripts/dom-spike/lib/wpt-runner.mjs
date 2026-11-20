/**
 * Run one WPT dom/nodes file in-process (used by fork worker and domino bundle tests).
 */
import { createRequire } from "node:module";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { extractScripts, resolveScriptPath } from "./html-scripts.mjs";
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
} from "./vm-harness.mjs";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(__dirname, "../../..");
const SPIKE_DIR = path.join(ROOT, "target/dom-spike");
const WPT_ROOT = path.join(ROOT, "tests/compat/wpt");
const SPIKE_REPORT = path.join(ROOT, "scripts/dom-spike/spike-testharnessreport.js");

const spikeRequire = createRequire(path.join(SPIKE_DIR, "package.json"));

export function bootstrapPage(
  lib,
  bundleCode,
  html,
  { stripScripts = true, loader = "iife" } = {},
) {
  const parseHtml = stripScripts ? htmlForDocumentParse(html) : html;
  const empty =
    "<!DOCTYPE html><html><head></head><body></body></html>";

  const useRequire =
    lib.bootstrap === "require" ||
    (lib.bootstrap !== "bundle" &&
      (lib.id === "domino" || lib.id === "linkedom" || lib.id === "happy-dom") &&
      !bundleCode);

  if (useRequire) {
    if (lib.id === "domino") {
      const domino = spikeRequire("domino");
      const window = domino.createWindow(parseHtml || empty);
      return { window, document: window.document };
    }
    if (lib.id === "linkedom") {
      const { parseHTML } = spikeRequire("linkedom");
      const parsed = parseHTML(parseHtml || empty);
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
      window.document.write(parseHtml || empty);
      window.document.close();
      return { window, document: window.document };
    }
    throw new Error(`unknown library for require bootstrap: ${lib.id}`);
  }

  const boot = createBootstrapContext(makeTimerQueue());
  if (loader === "cjs") {
    runScriptInContext(
      boot,
      "var module = { exports: {} }; var exports = module.exports;",
      "cjs-prelude",
    );
    runScriptInContext(boot, bundleCode, "bundle.cjs");
    runScriptInContext(
      boot,
      `globalThis.__PAGE__ = module.exports.createPage(${JSON.stringify(parseHtml)});`,
      "<createPage>",
    );
  } else {
    runScriptInContext(boot, bundleCode, "bundle.js");
    runScriptInContext(
      boot,
      `globalThis.__PAGE__ = ${lib.globalName}.createPage(${JSON.stringify(parseHtml)});`,
      "<createPage>",
    );
  }
  return boot.__PAGE__;
}

/**
 * @param {object} lib - { id, globalName, bootstrap?: 'require'|'bundle' }
 * @param {string} bundleCode - IIFE source (empty when bootstrap require)
 * @param {string} htmlPath - absolute path
 */
export function runWptFile(lib, bundleCode, htmlPath) {
  const html = readFile(htmlPath);
  const testDir = path.dirname(htmlPath);
  const scripts = extractScripts(html);

  let page;
  try {
    page = bootstrapPage(lib, bundleCode, html, {
      loader: lib.loader ?? "iife",
    });
  } catch (e) {
    return {
      file: path.basename(htmlPath),
      error: `createPage: ${e.message}`,
      tests: [],
      timedOut: false,
      hung: false,
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
        runScriptInContext(
          sandbox,
          spec.code,
          `${path.basename(htmlPath)}:${spec.line}`,
        );
      }
    } catch (e) {
      const msg = e.message || String(e);
      if (/Script execution timed out/i.test(msg)) {
        return {
          file: path.basename(htmlPath),
          error: `vm timeout: ${spec.src || "inline"}`,
          tests: [],
          timedOut: true,
          hung: true,
        };
      }
      harnessError = `script error (${spec.src || "inline"}): ${msg}`;
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

  return {
    file: path.basename(htmlPath),
    error: harnessError,
    timedOut: drain.timedOut,
    hung: drain.timedOut,
    pendingTimers: drain.pending ?? 0,
    summary,
    tests: summary?.tests ?? [],
    harnessStatus: summary?.status ?? (harnessError ? "ERROR" : "NO_RESULTS"),
  };
}

export { ROOT, SPIKE_DIR, WPT_ROOT };