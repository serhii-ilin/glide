/**
 * Try alternate single-file bundles for domino and measure WPT pass rate (bundle-only bootstrap).
 */
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { bootstrapPage } from "./wpt-runner.mjs";
import { runWptFileForked } from "./wpt-fork.mjs";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(__dirname, "../../..");
const SPIKE_DIR = path.join(ROOT, "target/dom-spike");
const ENTRY = path.join(ROOT, "scripts/dom-spike/bundle-entries/domino.js");

const DOMINO_LIB = {
  id: "domino",
  globalName: "DomSpikeDomino",
  bootstrap: "bundle",
};

function spikeNm(sub) {
  return path.join(SPIKE_DIR, "node_modules", sub);
}

function tryCreatePage(bundleCode, loader) {
  try {
    const page = bootstrapPage(DOMINO_LIB, bundleCode, "<!DOCTYPE html><html><body><div id=x></div></body></html>", {
      stripScripts: true,
      loader,
    });
    return page?.document?.getElementById("x") != null;
  } catch (e) {
    return { ok: false, error: e.message };
  }
}

function esbuildDomino(opts, outName) {
  const outFile = path.join(SPIKE_DIR, "dist", outName);
  const esbuildBin = path.join(SPIKE_DIR, "node_modules/.bin/esbuild");
  const canvasStub = path.join(ROOT, "scripts/dom-spike/shims/canvas-stub.js");
  const args = [
    ENTRY,
    "--bundle",
    `--platform=${opts.platform ?? "node"}`,
    `--format=${opts.format ?? "iife"}`,
    `--outfile=${outFile}`,
    `--alias:domino=${spikeNm("domino")}`,
    `--alias:canvas=${canvasStub}`,
    "--banner:js=var global=globalThis;var setTimeout=globalThis.setTimeout;var setInterval=globalThis.setInterval;var clearTimeout=globalThis.clearTimeout;var clearInterval=globalThis.clearInterval;",
  ];
  if (opts.format === "iife") {
    args.splice(5, 0, `--global-name=${DOMINO_LIB.globalName}`);
  }
  if (opts.minify) {
    args.push("--minify");
  } else {
    args.push("--keep-names");
  }
  const run = spawnSync(esbuildBin, args, { cwd: ROOT, encoding: "utf8" });
  if (run.status !== 0) {
    return {
      ok: false,
      error: (run.stderr || run.stdout || "esbuild failed").trim(),
      path: outFile,
    };
  }
  const code = fs.readFileSync(outFile, "utf8");
  return {
    ok: true,
    path: outFile,
    bytes: Buffer.byteLength(code, "utf8"),
    code,
    format: opts.format ?? "iife",
  };
}

function browserifyDomino() {
  const outFile = path.join(SPIKE_DIR, "dist", "domino.browserify.js");
  const browserifyBin = path.join(SPIKE_DIR, "node_modules/.bin/browserify");
  const run = spawnSync(
    browserifyBin,
    [ENTRY, "-s", DOMINO_LIB.globalName, "-o", outFile],
    {
      cwd: ROOT,
      encoding: "utf8",
      env: { ...process.env, NODE_PATH: path.join(SPIKE_DIR, "node_modules") },
    },
  );
  if (run.status !== 0) {
    return {
      ok: false,
      error: (run.stderr || run.stdout || "browserify failed").trim(),
    };
  }
  const code = fs.readFileSync(outFile, "utf8");
  const banner =
    "var global=globalThis;var setTimeout=globalThis.setTimeout;var setInterval=globalThis.setInterval;var clearTimeout=globalThis.clearTimeout;var clearInterval=globalThis.clearInterval;\n";
  const bundled = banner + code;
  fs.writeFileSync(outFile, bundled);
  return {
    ok: true,
    path: outFile,
    bytes: Buffer.byteLength(bundled, "utf8"),
    code: bundled,
    format: "browserify-iife",
  };
}

function rollupDomino() {
  const outFile = path.join(SPIKE_DIR, "dist", "domino.rollup.js");
  const configFile = path.join(SPIKE_DIR, "rollup.domino.config.mjs");
  const config = `import resolve from "@rollup/plugin-node-resolve";
import commonjs from "@rollup/plugin-commonjs";
export default {
  input: ${JSON.stringify(ENTRY)},
  output: {
    file: ${JSON.stringify(outFile)},
    format: "iife",
    name: ${JSON.stringify(DOMINO_LIB.globalName)},
    exports: "named",
  },
  plugins: [resolve({ preferBuiltins: false }), commonjs()],
};
`;
  fs.writeFileSync(configFile, config);
  const rollupBin = path.join(SPIKE_DIR, "node_modules/.bin/rollup");
  const run = spawnSync(rollupBin, ["-c", configFile], {
    cwd: ROOT,
    encoding: "utf8",
  });
  if (run.status !== 0) {
    return {
      ok: false,
      error: (run.stderr || run.stdout || "rollup failed").trim(),
    };
  }
  const code = fs.readFileSync(outFile, "utf8");
  const banner =
    "var global=globalThis;var setTimeout=globalThis.setTimeout;var setInterval=globalThis.setInterval;var clearTimeout=globalThis.clearTimeout;var clearInterval=globalThis.clearInterval;\n";
  const bundled = banner + code;
  fs.writeFileSync(outFile, bundled);
  return {
    ok: true,
    path: outFile,
    bytes: Buffer.byteLength(bundled, "utf8"),
    code: bundled,
    format: "rollup-iife",
  };
}

async function summarizeWptForked(htmlPaths, bundleCode, loader) {
  let passed = 0;
  let total = 0;
  let filesFull = 0;
  const timeouts = [];
  const errors = [];
  const lib = { ...DOMINO_LIB, loader };
  for (const htmlPath of htmlPaths) {
    const r = await runWptFileForked(lib, bundleCode, htmlPath);
    if (r.hung || r.timedOut) {
      timeouts.push(r.file);
      continue;
    }
    if (r.error) {
      errors.push(`${r.file}: ${r.error}`);
      continue;
    }
    const tests = r.tests ?? [];
    if (tests.length === 0) {
      errors.push(`${r.file}: no subtests`);
      continue;
    }
    if (tests.every((t) => t.status === "PASS")) {
      filesFull += 1;
    }
    for (const t of tests) {
      total += 1;
      if (t.status === "PASS") {
        passed += 1;
      }
    }
  }
  return {
    passed,
    total,
    percent: total ? (100 * passed) / total : 0,
    filesFull,
    fileCount: htmlPaths.length,
    timeouts,
    errors: errors.slice(0, 8),
  };
}

export async function runDominoBundleExperiments(htmlPaths) {
  const results = [];
  const variants = [
    {
      id: "esbuild-iife-node-minify",
      build: () =>
        esbuildDomino(
          { platform: "node", format: "iife", minify: true },
          "domino.exp-min.js",
        ),
    },
    {
      id: "esbuild-iife-node-keepnames",
      build: () =>
        esbuildDomino(
          { platform: "node", format: "iife", minify: false },
          "domino.exp-nomin.js",
        ),
    },
    {
      id: "esbuild-iife-browser-keepnames",
      build: () =>
        esbuildDomino(
          { platform: "browser", format: "iife", minify: false },
          "domino.exp-browser.js",
        ),
    },
    {
      id: "esbuild-cjs-loader",
      build: () =>
        esbuildDomino({ platform: "node", format: "cjs", minify: false }, "domino.exp-cjs.js"),
      loader: "cjs",
    },
    {
      id: "browserify-standalone",
      build: () => browserifyDomino(),
    },
    {
      id: "rollup-iife-commonjs",
      build: () => rollupDomino(),
    },
  ];

  for (const v of variants) {
    console.log(`  domino bundle experiment: ${v.id}`);
    const built = v.build();
    if (!built.ok) {
      results.push({
        id: v.id,
        bundleOk: false,
        error: built.error,
        createPageOk: false,
        wptFull: null,
      });
      continue;
    }
    const createPageOk = tryCreatePage(built.code, v.loader ?? "iife");
    if (createPageOk !== true) {
      const err =
        typeof createPageOk === "object" && createPageOk?.error
          ? createPageOk.error
          : "createPage failed";
      results.push({
        id: v.id,
        bundleOk: true,
        bytes: built.bytes,
        createPageOk: false,
        createPageError: err,
        wptFull: null,
      });
      continue;
    }
    const wptFull = await summarizeWptForked(htmlPaths, built.code, v.loader ?? "iife");
    results.push({
      id: v.id,
      bundleOk: true,
      bytes: built.bytes,
      format: built.format,
      createPageOk: true,
      wptFull,
    });
    console.log(
      `    createPage OK; WPT ${wptFull.passed}/${wptFull.total} (${wptFull.percent.toFixed(1)}%), files full ${wptFull.filesFull}/${wptFull.fileCount}`,
    );
  }
  return results;
}