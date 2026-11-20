import vm from "node:vm";
import fs from "node:fs";

const TIMER_CAP_MS = 10_000;
export const VM_SCRIPT_TIMEOUT_MS = 5000;

const VM_BUILTINS = [
  "Object",
  "Array",
  "Number",
  "String",
  "Boolean",
  "Symbol",
  "BigInt",
  "Math",
  "JSON",
  "Date",
  "RegExp",
  "Error",
  "TypeError",
  "ReferenceError",
  "SyntaxError",
  "RangeError",
  "EvalError",
  "URIError",
  "Map",
  "Set",
  "WeakMap",
  "WeakSet",
  "Promise",
  "Reflect",
  "Proxy",
  "Intl",
  "Int8Array",
  "Uint8Array",
  "Uint8ClampedArray",
  "Int16Array",
  "Uint16Array",
  "Int32Array",
  "Uint32Array",
  "Float32Array",
  "Float64Array",
  "ArrayBuffer",
  "DataView",
  "parseInt",
  "parseFloat",
  "isNaN",
  "isFinite",
  "encodeURI",
  "decodeURI",
  "encodeURIComponent",
  "decodeURIComponent",
];

function attachVmBuiltins(target) {
  for (const name of VM_BUILTINS) {
    if (name in globalThis) {
      target[name] = globalThis[name];
    }
  }
}

export function makeTimerQueue() {
  let now = 0;
  let nextId = 1;
  const timers = new Map();
  const microtasks = [];

  function runMicrotasks() {
    while (microtasks.length > 0) {
      const fn = microtasks.shift();
      try {
        fn();
      } catch (e) {
        throw e;
      }
    }
  }

  function setTimeout(fn, delay = 0, ...args) {
    const id = nextId++;
    const fireAt = now + Math.max(0, Number(delay) || 0);
    timers.set(id, { fn, fireAt, args, kind: "timeout" });
    return id;
  }

  function clearTimeout(id) {
    timers.delete(id);
  }

  function setInterval(fn, delay = 0, ...args) {
    const id = nextId++;
    const period = Math.max(0, Number(delay) || 0);
    timers.set(id, {
      fn,
      fireAt: now + period,
      args,
      kind: "interval",
      period,
    });
    return id;
  }

  function clearInterval(id) {
    timers.delete(id);
  }

  function queueMicrotask(fn) {
    if (typeof fn !== "function") {
      throw new TypeError("queueMicrotask: callback is not a function");
    }
    microtasks.push(fn);
  }

  function drainUntilIdle(maxMs = TIMER_CAP_MS) {
    const deadline = now + maxMs;
    let iterations = 0;
    const maxIterations = 500_000;
    while (iterations++ < maxIterations) {
      runMicrotasks();
      if (timers.size === 0) {
        break;
      }
      let earliest = null;
      for (const [id, t] of timers) {
        if (!earliest || t.fireAt < earliest.fireAt) {
          earliest = { id, ...t };
        }
      }
      if (!earliest) {
        break;
      }
      if (earliest.fireAt > deadline) {
        return { timedOut: true, now };
      }
      now = earliest.fireAt;
      try {
        earliest.fn(...earliest.args);
      } catch (_) {
        /* testharness captures errors */
      }
      if (earliest.kind === "interval") {
        timers.set(earliest.id, {
          ...earliest,
          fireAt: now + earliest.period,
        });
      } else {
        timers.delete(earliest.id);
      }
    }
    runMicrotasks();
    return { timedOut: now >= deadline && timers.size > 0, now, pending: timers.size };
  }

  return {
    setTimeout,
    clearTimeout,
    setInterval,
    clearInterval,
    queueMicrotask,
    drainUntilIdle,
    hasPending() {
      return microtasks.length > 0 || timers.size > 0;
    },
  };
}

/** Strip scripts so the parser only builds markup we execute ourselves. */
export function htmlForDocumentParse(html) {
  return html
    .replace(/<script\b[^>]*>[\s\S]*?<\/script\s*>/gi, "")
    .replace(/<script\b[^>]*\/>/gi, "");
}

/**
 * Vm context backed by the library global object so `instanceof` matches node creation.
 */
export function createWindowContext(window, timerQueue) {
  const ctx = window;
  try {
    ctx.console = console;
    ctx.setTimeout = timerQueue.setTimeout.bind(timerQueue);
    ctx.clearTimeout = timerQueue.clearTimeout.bind(timerQueue);
    ctx.setInterval = timerQueue.setInterval.bind(timerQueue);
    ctx.clearInterval = timerQueue.clearInterval.bind(timerQueue);
    ctx.queueMicrotask = timerQueue.queueMicrotask.bind(timerQueue);
    attachVmBuiltins(ctx);
    ctx.globalThis = ctx;
    ctx.global = ctx;
    ctx.self = ctx;
    ctx.window = ctx;
    ctx.top = ctx;
    ctx.parent = ctx;
    vm.createContext(ctx);
    return ctx;
  } catch {
    const harness = {
      console,
      setTimeout: timerQueue.setTimeout.bind(timerQueue),
      clearTimeout: timerQueue.clearTimeout.bind(timerQueue),
      setInterval: timerQueue.setInterval.bind(timerQueue),
      clearInterval: timerQueue.clearInterval.bind(timerQueue),
      queueMicrotask: timerQueue.queueMicrotask.bind(timerQueue),
    };
    attachVmBuiltins(harness);
    const sandbox = new Proxy(harness, {
      get(target, prop, receiver) {
        if (Object.prototype.hasOwnProperty.call(target, prop)) {
          return Reflect.get(target, prop, receiver);
        }
      if (prop === "globalThis" || prop === "global" || prop === "self") {
        return sandbox;
      }
      if (prop === "__SPIKE_HARNESS__") {
        return target.__SPIKE_HARNESS__;
      }
        if (prop === "document") {
          return window.document;
        }
        if (prop === "window" || prop === "top" || prop === "parent") {
          return window;
        }
        const v = window[prop];
        if (typeof v !== "function") {
          return v;
        }
        // Do not .bind constructors — it breaks `instanceof` in WPT tests.
        if (/^[A-Z]/.test(String(prop))) {
          return v;
        }
        return v.bind(window);
      },
      set(target, prop, value) {
        if (prop === "__SPIKE_HARNESS__") {
          target.__SPIKE_HARNESS__ = value;
          return true;
        }
        if (Object.prototype.hasOwnProperty.call(target, prop)) {
          target[prop] = value;
          return true;
        }
        window[prop] = value;
        return true;
      },
      has(_target, prop) {
        return prop in harness || prop in window;
      },
      getPrototypeOf() {
        return Object.getPrototypeOf(window);
      },
    });
    vm.createContext(sandbox);
    return sandbox;
  }
}

/** Minimal context used only to evaluate the library bundle before a document exists. */
export function createBootstrapContext(timerQueue) {
  const sandbox = {
    console,
    setTimeout: timerQueue.setTimeout,
    clearTimeout: timerQueue.clearTimeout,
    setInterval: timerQueue.setInterval,
    clearInterval: timerQueue.clearInterval,
    queueMicrotask: timerQueue.queueMicrotask,
  };
  attachVmBuiltins(sandbox);
  vm.createContext(sandbox);
  try {
    vm.runInContext(
      [
        "var globalThis = this;",
        "var global = globalThis;",
        "var self = this;",
        "var window = this;",
        "var top = this;",
        "var parent = this;",
      ].join(" "),
      sandbox,
      { timeout: VM_SCRIPT_TIMEOUT_MS },
    );
  } catch (e) {
    throw new Error(`bootstrap globals: ${e.message}`);
  }
  return sandbox;
}

export function runScriptInContext(sandbox, code, filename) {
  try {
    const script = new vm.Script(code, { filename, displayErrors: true });
    script.runInContext(sandbox, { timeout: VM_SCRIPT_TIMEOUT_MS });
  } catch (e) {
    throw e;
  }
}

export function readFile(path) {
  return fs.readFileSync(path, "utf8");
}

export function dispatchEvent(sandbox, type) {
  runScriptInContext(
    sandbox,
    `
      (function() {
        var e = document.createEvent("Event");
        e.initEvent(${JSON.stringify(type)}, true, true);
        document.dispatchEvent(e);
        if (typeof window !== "undefined" && typeof window.dispatchEvent === "function") {
          window.dispatchEvent(e);
        }
      })();
    `,
    `<event ${type}>`,
  );
}

/** Flush testharness completion when \`load\` timers do not run. */
export function flushTestHarness(sandbox) {
  runScriptInContext(
    sandbox,
    `
      (function() {
        if (typeof test_environment !== "undefined") {
          test_environment.all_loaded = true;
        }
        if (typeof tests !== "undefined" && typeof tests.complete === "function") {
          tests.complete();
        }
      })();
    `,
    "<flush harness>",
  );
}

export function readHarnessResults(sandbox) {
  try {
    if (sandbox.__SPIKE_HARNESS__) {
      return sandbox.__SPIKE_HARNESS__;
    }
    const doc = sandbox.document;
    const pre = doc.getElementById("__glide_results");
    if (!pre || !pre.textContent) {
      return null;
    }
    const raw = pre.textContent.trim();
    const prefix = "__GLIDE_RESULTS__:";
    const json = raw.startsWith(prefix) ? raw.slice(prefix.length) : raw;
    return JSON.parse(json);
  } catch {
    return null;
  }
}