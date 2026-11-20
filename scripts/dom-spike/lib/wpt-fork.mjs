import { fork } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const WORKER = path.join(__dirname, "../wpt-file-worker.mjs");
export const FILE_WALL_MS = 15_000;

/**
 * Run one WPT file in a child process so synchronous infinite loops cannot wedged the harness.
 */
export function runWptFileForked(lib, bundleCode, htmlPath) {
  return new Promise((resolve) => {
    const child = fork(WORKER, [], {
      stdio: ["ignore", "ignore", "ignore", "ipc"],
      env: { ...process.env, NODE_OPTIONS: "" },
    });
    let settled = false;
    const basename = path.basename(htmlPath);
    const finish = (value) => {
      if (settled) {
        return;
      }
      settled = true;
      clearTimeout(timer);
      try {
        child.kill("SIGKILL");
      } catch {
        /* already dead */
      }
      resolve(value);
    };
    const timer = setTimeout(() => {
      finish({
        file: basename,
        error: `wall-clock timeout (${FILE_WALL_MS}ms)`,
        tests: [],
        timedOut: true,
        hung: true,
        library: lib.id,
      });
    }, FILE_WALL_MS);
    child.on("message", (msg) => {
      if (msg.hung) {
        finish({
          file: basename,
          error: msg.error || "worker hung",
          tests: [],
          timedOut: true,
          hung: true,
          library: lib.id,
        });
        return;
      }
      if (msg.ok) {
        finish({ ...msg.result, library: lib.id });
        return;
      }
      finish({
        file: basename,
        error: msg.error || "worker failed",
        tests: [],
        hung: false,
        library: lib.id,
      });
    });
    child.on("error", (e) => {
      finish({
        file: basename,
        error: e.message,
        tests: [],
        hung: false,
        library: lib.id,
      });
    });
    child.send({
      lib: {
        id: lib.id,
        globalName: lib.globalName,
        bootstrap: lib.bootstrap ?? "require",
        loader: lib.loader,
      },
      bundleCode: bundleCode ?? "",
      htmlPath,
    });
  });
}