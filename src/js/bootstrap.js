// Event loop / fetch shims. Domino + page DOM live in dom-glue.js (included before this file).
(function (global) {
  'use strict';
  global.self = global.window = global;

  const console = {
    log(...args) { __glide_console('log', args.map(String).join(' ')); },
    info(...args) { __glide_console('info', args.map(String).join(' ')); },
    debug(...args) { __glide_console('debug', args.map(String).join(' ')); },
    trace(...args) { __glide_console('debug', args.map(String).join(' ')); },
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

  global.Promise = Promise;
  global.performance = { now: () => nowMs };
  global.console = console;

  function invalidCharacter() {
    const err = new Error('InvalidCharacterError');
    err.name = 'InvalidCharacterError';
    return err;
  }

  const B64 = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
  global.btoa = function (value) {
    const input = String(value);
    let out = '';
    for (let i = 0; i < input.length; i += 3) {
      const a = input.charCodeAt(i);
      const hasB = i + 1 < input.length;
      const hasC = i + 2 < input.length;
      const b = hasB ? input.charCodeAt(i + 1) : 0;
      const c = hasC ? input.charCodeAt(i + 2) : 0;
      if (a > 255 || (hasB && b > 255) || (hasC && c > 255)) throw invalidCharacter();
      out += B64[a >> 2];
      out += B64[((a & 3) << 4) | (b >> 4)];
      out += hasB ? B64[((b & 15) << 2) | (c >> 6)] : '=';
      out += hasC ? B64[c & 63] : '=';
    }
    return out;
  };

  global.atob = function (value) {
    const input = String(value).replace(/[\t\n\f\r ]/g, '');
    if (input.length % 4 === 1) throw invalidCharacter();
    let out = '';
    for (let i = 0; i < input.length; i += 4) {
      const chunk = input.slice(i, i + 4);
      const trailing = /=+$/.exec(chunk);
      let pad = trailing ? trailing[0].length : 0;
      const body = trailing ? chunk.slice(0, -pad) : chunk;
      if (/[=]/.test(body)) throw invalidCharacter();
      if (i + 4 >= input.length && pad === 0) pad = 4 - body.length;
      if (pad > 2 || body.length + pad !== 4) throw invalidCharacter();
      const values = [0, 0, 0, 0];
      for (let j = 0; j < body.length; j += 1) {
        const index = B64.indexOf(body[j]);
        if (index < 0) throw invalidCharacter();
        values[j] = index;
      }
      const bits = (values[0] << 18) | (values[1] << 12) | (values[2] << 6) | values[3];
      out += String.fromCharCode((bits >> 16) & 255);
      if (pad < 2) out += String.fromCharCode((bits >> 8) & 255);
      if (pad < 1) out += String.fromCharCode(bits & 255);
    }
    return out;
  };

  global.matchMedia = function (query) {
    const media = String(query);
    return {
      media,
      matches: false,
      onchange: null,
      addListener() {},
      removeListener() {},
      addEventListener() {},
      removeEventListener() {},
      dispatchEvent() { return false; },
    };
  };

  function createStorage() {
    const data = new Map();
    const store = {
      get length() { return data.size; },
      key(index) {
        const keys = Array.from(data.keys());
        return index >= 0 && index < keys.length ? keys[index] : null;
      },
      getItem(key) {
        const name = String(key);
        return data.has(name) ? data.get(name) : null;
      },
      setItem(key, value) { data.set(String(key), String(value)); },
      removeItem(key) { data.delete(String(key)); },
      clear() { data.clear(); },
    };
    return new Proxy(store, {
      get(target, prop) {
        if (prop in target) return target[prop];
        return typeof prop === 'string' && data.has(prop) ? data.get(prop) : undefined;
      },
      set(target, prop, value) {
        if (typeof prop === 'string' && !(prop in target)) {
          data.set(prop, String(value));
          return true;
        }
        target[prop] = value;
        return true;
      },
      deleteProperty(target, prop) {
        if (typeof prop === 'string' && data.delete(prop)) return true;
        return delete target[prop];
      },
      has(target, prop) {
        return prop in target || (typeof prop === 'string' && data.has(prop));
      },
      ownKeys(target) {
        return Array.from(new Set(Reflect.ownKeys(target).concat(Array.from(data.keys()))));
      },
      getOwnPropertyDescriptor(target, prop) {
        if (typeof prop === 'string' && data.has(prop)) {
          return { value: data.get(prop), writable: true, enumerable: true, configurable: true };
        }
        return Reflect.getOwnPropertyDescriptor(target, prop);
      },
    });
  }
  global.localStorage = createStorage();
  global.sessionStorage = createStorage();

  class AbortSignalShim {
    constructor() {
      this.aborted = false;
      this.reason = undefined;
      this.onabort = null;
      this._listeners = new Map();
    }
    addEventListener(type, listener) {
      if (type !== 'abort' || typeof listener !== 'function') return;
      const set = this._listeners.get(type) || new Set();
      set.add(listener);
      this._listeners.set(type, set);
    }
    removeEventListener(type, listener) {
      const set = this._listeners.get(type);
      if (set) set.delete(listener);
    }
    dispatchEvent(event) {
      if (!event || event.type !== 'abort') return false;
      this._fire(event.reason);
      return true;
    }
    throwIfAborted() {
      if (this.aborted) throw this.reason || new Error('This operation was aborted');
    }
    _fire(reason) {
      if (this.aborted) return;
      this.aborted = true;
      this.reason = reason;
      const event = { type: 'abort', target: this, reason };
      const set = this._listeners.get('abort');
      if (set) Array.from(set).forEach((fn) => fn(event));
      if (typeof this.onabort === 'function') this.onabort(event);
    }
  }
  global.AbortSignal = AbortSignalShim;
  global.AbortController = class AbortController {
    constructor() { this.signal = new AbortSignalShim(); }
    abort(reason) { this.signal._fire(reason); }
  };

  global.MutationRecord = function MutationRecord() {};
  global.MutationObserver = function MutationObserver(callback) {
    this.observe = function () {};
    this.disconnect = function () {};
    this.takeRecords = function () { return []; };
  };

  function Image(width, height) {
    const img = document.createElement('img');
    if (arguments.length >= 1 && width != null) img.width = width;
    if (arguments.length >= 2 && height != null) img.height = height;
    return img;
  }
  global.Image = Image;
  try {
    const ctor = document.createElement('img').constructor;
    if (ctor) global.HTMLImageElement = ctor;
  } catch (_) { /* document may not be an HTML document yet */ }

  global.fetch = function (url) {
    return new Promise((resolve, reject) => {
      queueMicrotask(() => {
        try {
          const raw = __glide_dom_sync('fetch', 0, String(url));
          if (raw && raw.__error) throw new Error(raw.__error);
          let status = 200;
          let ok = true;
          let finalUrl = String(url);
          let redirected = false;
          let body;
          if (raw && typeof raw === 'object' && raw.status != null && raw.body != null) {
            status = Number(raw.status) || 0;
            ok = raw.ok !== false && status >= 200 && status < 300;
            finalUrl = raw.url ? String(raw.url) : finalUrl;
            redirected = !!raw.redirected;
            body = String(raw.body);
          } else {
            body = raw == null ? '' : String(raw);
          }
          resolve({
            ok,
            status,
            url: finalUrl,
            redirected,
            text() { return Promise.resolve(body); },
            json() { return Promise.resolve(JSON.parse(body)); },
          });
        } catch (e) {
          reject(e);
        }
      });
    });
  };

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