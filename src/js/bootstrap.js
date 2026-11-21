// Event loop / fetch shims. Domino + page DOM live in dom-glue.js (included before this file).
(function (global) {
  'use strict';
  global.self = global.window = global;

  const console = {
    log(...args) { __glide_console('log', args.map(String).join(' ')); },
    error(...args) { __glide_console('error', args.map(String).join(' ')); },
    warn(...args) { __glide_console('warn', args.map(String).join(' ')); },
  };

  const timers = new Map();
  let nextTimerId = 1;
  let nowMs = 0;
  let rafQueue = [];
  let microtaskQueue = [];

  function runMicrotasks() {
    let ran = false;
    while (microtaskQueue.length) {
      ran = true;
      const jobs = microtaskQueue.slice();
      microtaskQueue = [];
      jobs.forEach((j) => {
        try { j(); } catch (e) { console.error(e); }
      });
    }
    return ran;
  }

  function beginTask() {
    if (typeof global.__glide_begin_task === 'function') global.__glide_begin_task();
  }

  global.queueMicrotask = function (fn) {
    microtaskQueue.push(fn);
  };

  global.setTimeout = function (fn, ms) {
    const id = nextTimerId++;
    timers.set(id, { fn, at: nowMs + (ms || 0), kind: 'timeout' });
    return id;
  };
  global.clearTimeout = function (id) {
    timers.delete(id);
  };
  global.setInterval = function (fn, ms) {
    const id = nextTimerId++;
    timers.set(id, { fn, at: nowMs + (ms || 0), interval: ms || 0, kind: 'interval' });
    return id;
  };
  global.clearInterval = function (id) {
    timers.delete(id);
  };

  global.requestAnimationFrame = function (fn) {
    const id = rafQueue.length;
    rafQueue.push(fn);
    return id;
  };

  global.__glide_pump_timers = function (now) {
    nowMs = now;
    timers.forEach((t, id) => {
      if (!t || t.at > now) return;
      beginTask();
      try { t.fn(); } catch (e) { console.error(e); }
      if (t.kind === 'interval') {
        t.at = now + t.interval;
      } else {
        timers.delete(id);
      }
      runMicrotasks();
      if (typeof global.__glide_flush_dom === 'function') global.__glide_flush_dom();
    });
  };

  global.__glide_flush_raf = function () {
    const jobs = rafQueue.slice();
    rafQueue = [];
    jobs.forEach((fn) => {
      beginTask();
      try { fn(nowMs); } catch (e) { console.error(e); }
      runMicrotasks();
      if (typeof global.__glide_flush_dom === 'function') global.__glide_flush_dom();
    });
  };

  global.fetch = function (url) {
    return new Promise((resolve, reject) => {
      queueMicrotask(() => {
        try {
          const body = __glide_dom_sync('fetch', 0, String(url));
          if (body && body.__error) throw new Error(body.__error);
          resolve({
            ok: true,
            text() { return Promise.resolve(body); },
            json() { return Promise.resolve(JSON.parse(body)); },
          });
        } catch (e) {
          reject(e);
        }
      });
    });
  };

  global.Promise = Promise;
  global.performance = { now: () => nowMs };
  global.console = console;

  function fireLoaded() {
    if (global.__glide_mark_document_loaded) global.__glide_mark_document_loaded();
    if (global.__glide_fire_page_load) {
      global.__glide_fire_page_load();
    } else {
      document.dispatchEvent(new Event('DOMContentLoaded'));
      global.dispatchEvent(new Event('load'));
    }
    runMicrotasks();
  }

  global.__glide_fire_dom_content_loaded = fireLoaded;
  global.__glide_drain_microtasks = function () {
    return runMicrotasks();
  };
})(globalThis);