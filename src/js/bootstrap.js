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

  // HTML timers normalize the delay: NaN and negative become 0, and the delay is
  // capped. Keep it numeric here so `at` never becomes a concatenated string or
  // a non-finite value that would confuse the idle-wake calculation.
  const MAX_TIMER_DELAY = 2147483647;
  function timerDelay(ms) {
    const n = Number(ms);
    if (Number.isNaN(n) || n <= 0) return 0;
    return Math.min(n, MAX_TIMER_DELAY);
  }

  global.setTimeout = function (fn, ms) {
    const id = nextTimerId++;
    timers.set(id, { fn, at: nowMs + timerDelay(ms), kind: 'timeout' });
    return id;
  };
  global.clearTimeout = function (id) {
    timers.delete(id);
  };
  global.setInterval = function (fn, ms) {
    const id = nextTimerId++;
    const delay = timerDelay(ms);
    timers.set(id, { fn, at: nowMs + delay, interval: delay, kind: 'interval' });
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

  global.__glide_next_wake = function (now) {
    // -1 means "no pending work". Deadlines are normalized to finite numbers at
    // creation; a deadline <= now is due immediately (not idle). Use an
    // Infinity minimum so an overdue timer cannot be replaced by a later one.
    let earliest = Infinity;
    let hasWork = false;
    timers.forEach((t) => {
      if (!t) return;
      hasWork = true;
      const at = Number(t.at);
      const due = Number.isFinite(at) ? at : 0;
      if (due < earliest) earliest = due;
    });
    if (rafQueue.length) {
      hasWork = true;
      const rafAt = now + 16;
      if (rafAt < earliest) earliest = rafAt;
    }
    if (!hasWork) return -1;
    return Math.ceil(Math.max(0, earliest));
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

  const performanceTimeOrigin = Date.now() - nowMs;
  const perfEntries = [];
  const perfMarks = new Map();
  const PERF_ENTRY_MAX = 250;
  const PERF_MARK_MAX = 250;
  let cachedTiming = null;
  let cachedNavigationEntry = null;

  function capturePerformanceSnapshot() {
    if (cachedTiming) return;
    const nav = performanceTimeOrigin;
    const stamp = nav + Math.max(0, nowMs);
    cachedTiming = Object.freeze({
      navigationStart: nav,
      unloadEventStart: nav,
      unloadEventEnd: nav,
      redirectStart: nav,
      redirectEnd: nav,
      fetchStart: nav,
      domainLookupStart: nav,
      domainLookupEnd: nav,
      connectStart: nav,
      connectEnd: nav,
      secureConnectionStart: nav,
      requestStart: nav,
      responseStart: stamp,
      responseEnd: stamp,
      domLoading: stamp,
      domInteractive: stamp,
      domContentLoadedEventStart: stamp,
      domContentLoadedEventEnd: stamp,
      domComplete: stamp,
      loadEventStart: stamp,
      loadEventEnd: stamp,
    });
    const href = pageHref();
    cachedNavigationEntry = Object.freeze({
      name: href,
      entryType: 'navigation',
      startTime: 0,
      duration: nowMs,
    });
  }

  global.__glide_capture_performance_navigation = capturePerformanceSnapshot;

  function pushPerfEntry(entry) {
    perfEntries.push(entry);
    while (perfEntries.length > PERF_ENTRY_MAX) perfEntries.shift();
  }

  function capPerfMarks() {
    while (perfMarks.size > PERF_MARK_MAX) {
      const oldest = perfMarks.keys().next().value;
      if (oldest === undefined) break;
      perfMarks.delete(oldest);
      for (let i = perfEntries.length - 1; i >= 0; i -= 1) {
        const e = perfEntries[i];
        if (e.entryType === 'mark' && e.name === oldest) {
          perfEntries.splice(i, 1);
          break;
        }
      }
    }
  }

  global.performance = {
    get timeOrigin() { return performanceTimeOrigin; },
    get timing() {
      capturePerformanceSnapshot();
      return cachedTiming;
    },
    navigation: { type: 0, redirectCount: 0 },
    now: () => nowMs,
    getEntries() {
      capturePerformanceSnapshot();
      const nav = cachedNavigationEntry;
      return nav ? [nav, ...perfEntries.slice()] : perfEntries.slice();
    },
    getEntriesByType(type) {
      const want = String(type);
      return global.performance.getEntries().filter((e) => e.entryType === want);
    },
    getEntriesByName(name, type) {
      const want = String(name);
      const kind = type == null ? null : String(type);
      return global.performance.getEntries().filter((e) => {
        if (e.name !== want) return false;
        if (kind != null && e.entryType !== kind) return false;
        return true;
      });
    },
    mark(name) {
      const label = String(name);
      perfMarks.set(label, nowMs);
      capPerfMarks();
      pushPerfEntry({ name: label, entryType: 'mark', startTime: nowMs, duration: 0 });
    },
    measure(name, startMark, endMark) {
      const label = String(name);
      let start = 0;
      let end = nowMs;
      if (startMark != null) {
        const key = String(startMark);
        if (perfMarks.has(key)) start = perfMarks.get(key);
      }
      if (endMark != null) {
        const key = String(endMark);
        if (perfMarks.has(key)) end = perfMarks.has(key) ? perfMarks.get(key) : end;
      }
      pushPerfEntry({
        name: label,
        entryType: 'measure',
        startTime: start,
        duration: Math.max(0, end - start),
      });
    },
    clearMarks(name) {
      if (name == null) {
        perfMarks.clear();
        for (let i = perfEntries.length - 1; i >= 0; i -= 1) {
          if (perfEntries[i].entryType === 'mark') perfEntries.splice(i, 1);
        }
        return;
      }
      const label = String(name);
      perfMarks.delete(label);
      for (let i = perfEntries.length - 1; i >= 0; i -= 1) {
        const e = perfEntries[i];
        if (e.entryType === 'mark' && e.name === label) perfEntries.splice(i, 1);
      }
    },
    clearMeasures(name) {
      if (name == null) {
        for (let i = perfEntries.length - 1; i >= 0; i -= 1) {
          if (perfEntries[i].entryType === 'measure') perfEntries.splice(i, 1);
        }
        return;
      }
      const label = String(name);
      for (let i = perfEntries.length - 1; i >= 0; i -= 1) {
        const e = perfEntries[i];
        if (e.entryType === 'measure' && e.name === label) perfEntries.splice(i, 1);
      }
    },
    clearResourceTimings() {},
    setResourceTimingBufferSize() {},
  };
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

  let nextIdleId = 1;
  const idleHandles = new Map();

  global.requestIdleCallback = function (cb, options) {
    if (typeof cb !== 'function') throw new TypeError('Callback must be a function');
    const id = nextIdleId++;
    const timerId = setTimeout(function () {
      idleHandles.delete(id);
      const start = performance.now();
      const budget = 50;
      try {
        cb({
          didTimeout: !!(options && options.timeout != null),
          timeRemaining() {
            return Math.max(0, budget - (performance.now() - start));
          },
        });
      } catch (e) { console.error(e); }
    }, 1);
    idleHandles.set(id, timerId);
    return id;
  };

  global.cancelIdleCallback = function (id) {
    const timerId = idleHandles.get(id);
    if (timerId != null) {
      clearTimeout(timerId);
      idleHandles.delete(id);
    }
  };

  function intersectRect(a, b) {
    const left = Math.max(a.left, b.left);
    const top = Math.max(a.top, b.top);
    const right = Math.min(a.right, b.right);
    const bottom = Math.min(a.bottom, b.bottom);
    const w = Math.max(0, right - left);
    const h = Math.max(0, bottom - top);
    return { x: left, y: top, top, left, right, bottom, width: w, height: h };
  }

  function rectsOverlap(a, b) {
    return a.width > 0 && a.height > 0 && b.width > 0 && b.height > 0
      && a.left < b.right && a.right > b.left && a.top < b.bottom && a.bottom > b.top;
  }

  function overlapRatio(rect, root) {
    if (!rectsOverlap(rect, root)) return 0;
    const box = intersectRect(rect, root);
    const area = rect.width * rect.height;
    return area > 0 ? (box.width * box.height) / area : 0;
  }

  function viewportRootBounds() {
    const docEl = document.documentElement;
    const body = document.body;
    let w = (docEl && docEl.clientWidth) || 0;
    let h = (docEl && docEl.clientHeight) || 0;
    if (!w && body && body.clientWidth) w = body.clientWidth;
    if (!h && body && body.clientHeight) h = body.clientHeight;
    if (!w && docEl) {
      const r = docEl.getBoundingClientRect();
      if (r.width > 0) w = r.width;
    }
    if (!h && docEl) {
      const r = docEl.getBoundingClientRect();
      if (r.height > 0) h = r.height;
    }
    if (!w) w = 800;
    if (!h) h = 600;
    return { x: 0, y: 0, top: 0, left: 0, width: w, height: h, right: w, bottom: h };
  }

  const ioObservers = new Set();
  let ioCheckScheduled = false;
  let ioResizeHooked = false;
  let ioScrollHooked = false;
  let ioDeliveryScheduled = false;
  const ioPendingDelivery = new Set();
  const ioScrollRootListeners = new WeakMap();

  function parseRootMarginSide(side, refSize) {
    const raw = String(side || '0px').trim();
    const m = raw.match(/^(-?\d+(?:\.\d+)?)(px|%)$/);
    if (!m) return 0;
    const n = Number(m[1]);
    if (!Number.isFinite(n)) return 0;
    if (m[2] === '%') return n * refSize / 100;
    return n;
  }

  function parseRootMargin(marginStr) {
    const parts = String(marginStr == null ? '0px' : marginStr).trim().split(/\s+/).filter(Boolean);
  let top;
  let right;
  let bottom;
  let left;
    if (!parts.length) {
      top = right = bottom = left = '0px';
    } else if (parts.length === 1) {
      top = right = bottom = left = parts[0];
    } else if (parts.length === 2) {
      top = bottom = parts[0];
      right = left = parts[1];
    } else if (parts.length === 3) {
      top = parts[0];
      right = left = parts[1];
      bottom = parts[2];
    } else {
      top = parts[0];
      right = parts[1];
      bottom = parts[2];
      left = parts[3];
    }
    return { top, right, bottom, left };
  }

  function applyRootMargin(bounds, marginStr) {
    const sides = parseRootMargin(marginStr);
    const top = parseRootMarginSide(sides.top, bounds.height);
    const right = parseRootMarginSide(sides.right, bounds.width);
    const bottom = parseRootMarginSide(sides.bottom, bounds.height);
    const left = parseRootMarginSide(sides.left, bounds.width);
    const w = bounds.width + left + right;
    const h = bounds.height + top + bottom;
    const x = bounds.left - left;
    const y = bounds.top - top;
    return {
      x, y, top: y, left: x, width: w, height: h, right: x + w, bottom: y + h,
    };
  }

  function rootBoundsForObserver(obs) {
    const root = obs.root;
    if (root && root.nodeType === 1) {
      const rect = root.getBoundingClientRect();
      return applyRootMargin({
        x: rect.x, y: rect.y, top: rect.top, left: rect.left,
        width: rect.width, height: rect.height, right: rect.right, bottom: rect.bottom,
      }, obs.rootMargin);
    }
    const viewport = viewportRootBounds();
    return applyRootMargin(viewport, obs.rootMargin);
  }

  function elementIsHidden(el) {
    if (!el || el.nodeType !== 1) return true;
    const rect = el.getBoundingClientRect();
    if (rect.width === 0 && rect.height === 0) {
      let inline = String((el.getAttribute && el.getAttribute('style')) || '');
      if (!inline && el.attributes) {
        for (let ai = 0; ai < el.attributes.length; ai += 1) {
          const a = el.attributes[ai];
          const name = String(a.name || a.localName || '').toLowerCase();
          if (name === 'style') {
            inline = String(a.value || '');
            break;
          }
        }
      }
      if (/display\s*:\s*none/i.test(inline)) return true;
    }
    if (el.style && String(el.style.display).toLowerCase() === 'none') return true;
    if (typeof global.getComputedStyle === 'function') {
      try {
        if (global.getComputedStyle(el).display === 'none') return true;
      } catch (_) { /* layout may be unavailable */ }
    }
    return false;
  }

  function thresholdsCrossed(prevRatio, ratio, thresholds) {
    if (prevRatio == null) return true;
    const was = prevRatio > 0;
    const now = ratio > 0;
    if (was !== now) return true;
    for (let i = 0; i < thresholds.length; i += 1) {
      const t = thresholds[i];
      if ((prevRatio < t && ratio >= t) || (prevRatio >= t && ratio < t)) return true;
    }
    return false;
  }

  let nativeLazyObserver = null;

  function maybeLoadLazyImage(el, rootBounds) {
    if (!el || String(el.localName || '').toLowerCase() !== 'img') return;
    const loading = String((el.getAttribute && el.getAttribute('loading')) || '').toLowerCase();
    if (loading !== 'lazy' || el.__glide_lazy_loaded) return;
    const rect = el.getBoundingClientRect();
    if (!rectsOverlap(rect, rootBounds)) return;
    el.__glide_lazy_loaded = true;
    if (typeof global.__glide_load_lazy_image === 'function') {
      global.__glide_load_lazy_image(el);
    }
  }

  function scheduleIntersectionChecks() {
    if (ioCheckScheduled) return;
    ioCheckScheduled = true;
    requestAnimationFrame(function () {
      ioCheckScheduled = false;
      runIntersectionChecks();
    });
  }

  function deliverIntersectionObservers() {
    if (ioDeliveryScheduled) return;
    ioDeliveryScheduled = true;
    queueMicrotask(function () {
      ioDeliveryScheduled = false;
      const pending = Array.from(ioPendingDelivery);
      ioPendingDelivery.clear();
      for (let i = 0; i < pending.length; i += 1) {
        const obs = pending[i];
        const entries = obs._drainPendingEntries();
        if (!entries.length) continue;
        for (let j = 0; j < entries.length; j += 1) {
          obs._records.push(entries[j]);
        }
        try { obs._callback(entries, obs); } catch (e) { console.error(e); }
      }
    });
  }

  function runIntersectionChecks() {
    if (typeof global.__glide_flush_dom === 'function') global.__glide_flush_dom();
    const time = performance.now();
    observeNativeLazyImages();
    for (const obs of ioObservers) {
      if (!obs._targets.size) continue;
      const rootBounds = rootBoundsForObserver(obs);
      const pending = [];
      for (const [el, state] of obs._targets) {
        if (!el || el.nodeType !== 1) continue;
        const hidden = elementIsHidden(el);
        const rect = el.getBoundingClientRect();
        const ratio = hidden ? 0 : overlapRatio(rect, rootBounds);
        const isect = !hidden && ratio > 0;
        const prevRatio = state == null ? null : state.ratio;
        if (!thresholdsCrossed(prevRatio, ratio, obs.thresholds)) continue;
        obs._targets.set(el, { isIntersecting: isect, ratio });
        const intersectionRect = hidden
          ? { x: 0, y: 0, top: 0, left: 0, right: 0, bottom: 0, width: 0, height: 0 }
          : intersectRect(rect, rootBounds);
        pending.push({
          target: el,
          isIntersecting: isect,
          intersectionRatio: isect ? Math.min(1, ratio) : 0,
          boundingClientRect: {
            x: rect.x, y: rect.y, top: rect.top, left: rect.left,
            right: rect.right, bottom: rect.bottom, width: rect.width, height: rect.height,
          },
          rootBounds: { ...rootBounds },
          intersectionRect,
          time,
        });
      }
      if (pending.length) {
        if (!obs._pendingEntries) obs._pendingEntries = [];
        obs._pendingEntries.push.apply(obs._pendingEntries, pending);
        ioPendingDelivery.add(obs);
        deliverIntersectionObservers();
      }
    }
  }

  function ioScrollTarget(node) {
    let el = node;
    while (el && el.nodeType === 1) {
      let overflowY = '';
      let overflowX = '';
      try {
        if (typeof global.getComputedStyle === 'function') {
          const cs = global.getComputedStyle(el);
          overflowY = String(cs.overflowY || cs.overflow || '');
          overflowX = String(cs.overflowX || cs.overflow || '');
        }
      } catch (_) { /* layout may be unavailable */ }
      const scrollable = /auto|scroll|overlay/i.test(overflowY) || /auto|scroll|overlay/i.test(overflowX);
      if (scrollable && !ioScrollRootListeners.has(el)) {
        const handler = scheduleIntersectionChecks;
        el.addEventListener('scroll', handler, { passive: true });
        ioScrollRootListeners.set(el, handler);
      }
      if (el === document.documentElement || el === document.body) break;
      el = el.parentNode;
    }
  }

  function trackIoScrollRoots(target) {
    ioScrollTarget(target);
    const root = document.documentElement;
    if (root && root !== target) ioScrollTarget(root);
  }

  function ensureIoResizeListener() {
    if (ioResizeHooked) return;
    ioResizeHooked = true;
    global.addEventListener('resize', scheduleIntersectionChecks);
  }

  function ensureIoScrollListener() {
    if (ioScrollHooked) return;
    ioScrollHooked = true;
    global.addEventListener('scroll', scheduleIntersectionChecks, true);
    const root = document.documentElement;
    if (root) root.addEventListener('scroll', scheduleIntersectionChecks, { passive: true });
  }

  global.IntersectionObserver = class IntersectionObserver {
    constructor(callback, options) {
      if (typeof callback !== 'function') throw new TypeError('Callback must be a function');
      options = options || {};
      this._callback = callback;
      const root = options.root;
      if (root != null && root.nodeType !== 1) {
        throw new TypeError('Root must be an Element or null');
      }
      this.root = root || null;
      this.rootMargin = options.rootMargin == null ? '0px' : String(options.rootMargin);
      const raw = options.threshold;
      let thresholds;
      if (raw == null) thresholds = [0];
      else if (Array.isArray(raw)) thresholds = raw.map(Number);
      else thresholds = [Number(raw)];
      thresholds = thresholds.filter((t) => Number.isFinite(t)).sort((a, b) => a - b);
      if (!thresholds.length) thresholds = [0];
      this.thresholds = thresholds;
      this._targets = new Map();
      this._records = [];
      this._pendingEntries = [];
      ioObservers.add(this);
      ensureIoResizeListener();
      ensureIoScrollListener();
    }
    observe(target) {
      if (!target || target.nodeType !== 1) throw new TypeError('Target must be an Element');
      if (this.root && this.root !== target) {
        let parent = target.parentNode;
        let inRoot = false;
        while (parent) {
          if (parent === this.root) {
            inRoot = true;
            break;
          }
          parent = parent.parentNode;
        }
        if (!inRoot) {
          throw new TypeError('Target must be a descendant of the observer root');
        }
      }
      this._targets.set(target, null);
      trackIoScrollRoots(target);
      scheduleIntersectionChecks();
    }
    unobserve(target) {
      this._targets.delete(target);
    }
    disconnect() {
      this._targets.clear();
      this._records.length = 0;
      this._pendingEntries.length = 0;
    }
    takeRecords() {
      runIntersectionChecks();
      const pending = this._drainPendingEntries();
      if (pending.length) {
        for (let i = 0; i < pending.length; i += 1) this._records.push(pending[i]);
        ioPendingDelivery.delete(this);
      }
      const batch = this._records.slice();
      this._records.length = 0;
      return batch;
    }
    _drainPendingEntries() {
      const batch = this._pendingEntries.slice();
      this._pendingEntries.length = 0;
      return batch;
    }
  };

  function ensureNativeLazyObserver() {
    if (nativeLazyObserver) return nativeLazyObserver;
    nativeLazyObserver = new IntersectionObserver(function (entries) {
      const rootBounds = rootBoundsForObserver({ root: null, rootMargin: '0px' });
      for (let i = 0; i < entries.length; i += 1) {
        maybeLoadLazyImage(entries[i].target, rootBounds);
      }
    }, { root: null, rootMargin: '200px' });
    return nativeLazyObserver;
  }

  function observeNativeLazyImages() {
    if (!document || !document.getElementsByTagName) return;
    const obs = ensureNativeLazyObserver();
    const imgs = document.getElementsByTagName('img');
    for (let i = 0; i < imgs.length; i += 1) {
      const img = imgs[i];
      const loading = String((img.getAttribute && img.getAttribute('loading')) || '').toLowerCase();
      if (loading !== 'lazy' || img.__glide_lazy_observed) continue;
      img.__glide_lazy_observed = true;
      obs.observe(img);
    }
  }

  function scrollViewportTo(x, y) {
    if (typeof global.__glide_begin_task === 'function') global.__glide_begin_task();
    if (typeof global.__glide_dom_sync === 'function') {
      try {
        global.__glide_dom_sync('scrollViewport', 0, String(Number(x) || 0), String(Number(y) || 0));
      } catch (_) { /* scroll may be unavailable in this build */ }
    }
    if (typeof global.__glide_flush_dom === 'function') global.__glide_flush_dom();
    scheduleIntersectionChecks();
    const root = document && document.documentElement;
    if (root) {
      try {
        root.dispatchEvent(new Event('scroll', { bubbles: false }));
      } catch (_) {
        const evt = document.createEvent('Event');
        evt.initEvent('scroll', false, false);
        root.dispatchEvent(evt);
      }
    }
    global.dispatchEvent(new Event('scroll', { bubbles: false }));
  }

  global.scrollTo = function scrollTo(x, y) {
    const nx = typeof x === 'object' && x != null ? Number(x.left) || 0 : Number(x) || 0;
    const ny = typeof x === 'object' && x != null ? Number(x.top) || 0 : Number(y) || 0;
    scrollViewportTo(nx, ny);
  };
  global.scroll = global.scrollTo;

  function observersOrLazyImagesPending() {
    if (ioObservers.size) return true;
    if (!document || !document.querySelector) return false;
    return !!document.querySelector('img[loading="lazy"]');
  }

  if (typeof global.__glide_flush_dom === 'function') {
    const origFlushDom = global.__glide_flush_dom;
    global.__glide_flush_dom = function glideFlushDomForObservers() {
      origFlushDom();
      if (observersOrLazyImagesPending()) scheduleIntersectionChecks();
    };
  }
  if (typeof global.__glide_flush_raf === 'function') {
    const origFlushRaf = global.__glide_flush_raf;
    global.__glide_flush_raf = function glideFlushRafForObservers() {
      origFlushRaf();
      if (observersOrLazyImagesPending()) scheduleIntersectionChecks();
    };
  }

  const roObservers = new Set();
  let roCheckScheduled = false;

  function scheduleResizeChecks() {
    if (roCheckScheduled) return;
    roCheckScheduled = true;
    setTimeout(function () {
      roCheckScheduled = false;
      runResizeChecks();
    }, 0);
  }

  function sizeKey(w, h) {
    return String(w) + '\0' + String(h);
  }

  function runResizeChecks() {
    if (typeof global.__glide_flush_dom === 'function') global.__glide_flush_dom();
    for (const obs of roObservers) {
      if (!obs._targets.size) continue;
      const entries = [];
      for (const [el, last] of obs._targets) {
        if (!el || el.nodeType !== 1) continue;
        const rect = el.getBoundingClientRect();
        const w = rect.width;
        const h = rect.height;
        const key = sizeKey(w, h);
        if (last === key) continue;
        obs._targets.set(el, key);
        const contentRect = {
          x: rect.x, y: rect.y, width: w, height: h,
          top: rect.top, right: rect.right, bottom: rect.bottom, left: rect.left,
        };
        entries.push({
          target: el,
          contentRect,
          borderBoxSize: [{ inlineSize: w, blockSize: h }],
          contentBoxSize: [{ inlineSize: w, blockSize: h }],
        });
      }
      if (entries.length) {
        try { obs._callback(entries, obs); } catch (e) { console.error(e); }
      }
    }
  }

  global.ResizeObserver = class ResizeObserver {
    constructor(callback) {
      if (typeof callback !== 'function') throw new TypeError('Callback must be a function');
      this._callback = callback;
      this._targets = new Map();
      roObservers.add(this);
    }
    observe(target) {
      if (!target || target.nodeType !== 1) throw new TypeError('Target must be an Element');
      if (!this._targets.has(target)) this._targets.set(target, null);
      scheduleResizeChecks();
    }
    unobserve(target) {
      this._targets.delete(target);
    }
    disconnect() {
      this._targets.clear();
    }
  };

  const CSS_KNOWN_PROPERTIES = new Set([
    'display', 'position', 'float', 'clear', 'width', 'height', 'min-width', 'min-height',
    'max-width', 'max-height', 'margin', 'margin-top', 'margin-right', 'margin-bottom', 'margin-left',
    'padding', 'padding-top', 'padding-right', 'padding-bottom', 'padding-left',
    'border', 'border-width', 'border-style', 'border-color', 'border-radius',
    'color', 'background', 'background-color', 'background-image', 'opacity',
    'font', 'font-family', 'font-size', 'font-weight', 'font-style', 'line-height',
    'text-align', 'text-decoration', 'text-transform', 'white-space', 'overflow', 'overflow-x', 'overflow-y',
    'flex', 'flex-direction', 'flex-wrap', 'justify-content', 'align-items', 'align-content', 'flex-grow',
    'transform', 'visibility', 'z-index', 'top', 'right', 'bottom', 'left', 'box-sizing',
    'fill', 'stroke', 'vector-effect',
  ]);

  function cssValueLooksValid(prop, value) {
    const v = String(value).trim().toLowerCase();
    if (!v) return false;
    if (prop === 'display') {
      return ['none', 'block', 'inline', 'inline-block', 'flex', 'grid', 'contents'].includes(v);
    }
    if (prop === 'position') return ['static', 'relative', 'absolute', 'fixed', 'sticky'].includes(v);
    if (prop === 'float') return ['none', 'left', 'right'].includes(v);
    if (v === 'initial' || v === 'inherit' || v === 'unset' || v === 'revert') return true;
    if (/^#[0-9a-f]{3,8}$/i.test(v) || /^rgb|^hsl|^[a-z]+$/.test(v)) return true;
    if (/^-?\d+(\.\d+)?(px|em|rem|%|vh|vw|fr|s|ms)?$/.test(v)) return true;
    return v.length < 64;
  }

  function cssEscape(value) {
    const s = String(value);
    let out = '';
    for (let i = 0; i < s.length; i++) {
      const code = s.charCodeAt(i);
      if (code === 0x0) {
        out += '\uFFFD';
        continue;
      }
      const first = i === 0;
      const secondDigit = i === 1 && s.charCodeAt(0) === 0x30 && code >= 0x30 && code <= 0x39;
      if ((first && code >= 0x30 && code <= 0x39) || secondDigit
        || (code >= 0x1 && code <= 0x1f) || code === 0x7f
        || (first && code === 0x2d)) {
        out += '\\' + code.toString(16) + ' ';
        continue;
      }
      if (code === 0x2d && secondDigit) {
        out += '\\' + code.toString(16) + ' ';
        continue;
      }
      if (code === 0x40 || code === 0x2d || code === 0x5f
        || (code >= 0x30 && code <= 0x39) || (code >= 0x41 && code <= 0x5a) || (code >= 0x61 && code <= 0x7a)
        || code >= 0x80) {
        out += s[i];
      } else {
        out += '\\' + code.toString(16) + ' ';
      }
    }
    return out;
  }

  function cssSupports(property, value) {
    const prop = String(property).trim().toLowerCase();
    if (!CSS_KNOWN_PROPERTIES.has(prop)) return false;
    return cssValueLooksValid(prop, value);
  }

  function cssSupportsCondition(condition) {
    const match = /^\(\s*([^:)]+)\s*:\s*([^)]+)\s*\)$/i.exec(String(condition).trim());
    if (!match) return false;
    return cssSupports(match[1], match[2]);
  }

  global.CSS = global.CSS || {};
  global.CSS.supports = function (a, b) {
    if (arguments.length === 1) return cssSupportsCondition(a);
    return cssSupports(a, b);
  };
  global.CSS.escape = cssEscape;

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

  function normalizeHeaderName(name) {
    return String(name).trim().toLowerCase();
  }

  const FORBIDDEN_REQUEST_HEADERS = new Set([
    'cookie', 'host', 'content-length', 'connection', 'user-agent', 'referer', 'origin',
  ]);

  function isValidHeaderName(name) {
    const s = String(name).trim();
    if (!s) return false;
    for (let i = 0; i < s.length; i += 1) {
      const c = s.charCodeAt(i);
      const ok = (c >= 0x30 && c <= 0x39)
        || (c >= 0x41 && c <= 0x5a)
        || (c >= 0x61 && c <= 0x7a)
        || c === 0x21 || c === 0x23 || c === 0x24 || c === 0x26 || c === 0x27
        || c === 0x2a || c === 0x2b || c === 0x2d || c === 0x2e
        || c === 0x5e || c === 0x5f || c === 0x60 || c === 0x7c || c === 0x7e;
      if (!ok) return false;
    }
    return true;
  }

  function isValidHeaderValue(value) {
    const s = String(value);
    return !/[\r\n]/.test(s);
  }

  function validateHeaderNameValue(name, value) {
    if (!isValidHeaderName(name) || !isValidHeaderValue(value)) {
      const err = new TypeError('invalid header');
      throw err;
    }
  }

  function bytesFromBase64(b64) {
    const bin = atob(String(b64));
    const out = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i += 1) out[i] = bin.charCodeAt(i);
    return out;
  }

  function base64FromBytes(bytes) {
    let bin = '';
    for (let i = 0; i < bytes.length; i += 1) bin += String.fromCharCode(bytes[i]);
    return btoa(bin);
  }

  function cloneHeaders(h) {
    const copy = new Headers();
    if (h) h.forEach((v, k) => copy.append(k, v));
    return copy;
  }

  class Headers {
    constructor(init) {
      this._map = new Map();
      if (init) Headers.appendAll(this, init);
    }
    append(name, value) {
      validateHeaderNameValue(name, value);
      const key = normalizeHeaderName(name);
      const prev = this._map.get(key);
      this._map.set(key, prev == null ? String(value) : `${prev}, ${value}`);
    }
    delete(name) { this._map.delete(normalizeHeaderName(name)); }
    get(name) {
      const v = this._map.get(normalizeHeaderName(name));
      return v == null ? null : v;
    }
    has(name) { return this._map.has(normalizeHeaderName(name)); }
    set(name, value) {
      validateHeaderNameValue(name, value);
      this._map.set(normalizeHeaderName(name), String(value));
    }
    forEach(cb, thisArg) {
      for (const [k, v] of this._map) cb.call(thisArg, v, k, this);
    }
    *entries() { yield* this._map.entries(); }
    *keys() { yield* this._map.keys(); }
    *values() { yield* this._map.values(); }
    [Symbol.iterator]() { return this.entries(); }
    static appendAll(target, init) {
      if (!init) return;
      if (init instanceof Headers) {
        init.forEach((v, k) => target.append(k, v));
        return;
      }
      if (Array.isArray(init)) {
        init.forEach((pair) => target.append(pair[0], pair[1]));
        return;
      }
      Object.keys(init).forEach((k) => target.append(k, init[k]));
    }
  }

  function headersFromRecord(record) {
    const h = new Headers();
    if (record && typeof record === 'object') {
      Object.keys(record).forEach((k) => h.append(k, record[k]));
    }
    return h;
  }

  function hostFetch(op, url, optionsJson) {
    if (optionsJson == null) {
      return __glide_dom_sync(op, 0, String(url));
    }
    return __glide_dom_sync(op, 0, String(url), optionsJson);
  }

  function normalizeHostPayload(raw, fallbackUrl) {
    if (raw && raw.__error) {
      const err = new TypeError(String(raw.__error));
      throw err;
    }
    if (raw && typeof raw === 'object' && raw.status != null && raw.body != null) {
      return {
        status: Number(raw.status) || 0,
        ok: raw.ok == null
          ? ((Number(raw.status) || 0) >= 200 && (Number(raw.status) || 0) < 300)
          : !!raw.ok,
        statusText: raw.statusText != null ? String(raw.statusText) : '',
        url: raw.url ? String(raw.url) : String(fallbackUrl),
        redirected: !!raw.redirected,
        headers: headersFromRecord(raw.headers),
        body: String(raw.body),
        bodyBase64: raw.bodyBase64 != null ? String(raw.bodyBase64) : '',
      };
    }
    const body = raw == null ? '' : String(raw);
    return {
      status: 200,
      ok: true,
      statusText: 'OK',
      url: String(fallbackUrl),
      redirected: false,
      headers: new Headers(),
      body,
    };
  }

  function toRpcOptions(method, headers, body, credentials) {
    const headerObj = {};
    if (headers) {
      headers.forEach((v, k) => {
        if (!FORBIDDEN_REQUEST_HEADERS.has(normalizeHeaderName(k))) headerObj[k] = v;
      });
    }
    const opt = { method: method || 'GET', headers: headerObj };
    if (body != null && body !== '') opt.body = String(body);
    if (credentials) opt.credentials = String(credentials);
    return JSON.stringify(opt);
  }

  function payloadBytes(data) {
    if (data && data.bodyBase64) return bytesFromBase64(data.bodyBase64);
    return utf8Encode(data && data.body != null ? data.body : '');
  }

  class Request {
    constructor(input, init) {
      init = init || {};
      const req = input instanceof Request ? input : null;
      const url = req ? req.url : String(input);
      this.url = url;
      this.method = String((init.method != null ? init.method : (req && req.method) || 'GET')).toUpperCase();
      this.headers = new Headers();
      if (req) {
        if (init.headers !== undefined) Headers.appendAll(this.headers, init.headers);
        else Headers.appendAll(this.headers, req.headers);
      } else {
        Headers.appendAll(this.headers, init.headers || null);
      }
      if (init.body !== undefined) this._body = init.body == null ? null : String(init.body);
      else if (req) this._body = req._body;
      else this._body = null;
      const cred = init.credentials != null ? init.credentials : (req && req.credentials) || 'same-origin';
      this.credentials = String(cred);
      this._signal = init.signal !== undefined ? init.signal : (req && req._signal) || null;
    }
    get body() { return this._body; }
    get signal() { return this._signal; }
    clone() {
      return new Request(this.url, {
        method: this.method,
        headers: cloneHeaders(this.headers),
        body: this._body,
        credentials: this.credentials,
        signal: this._signal,
      });
    }
  }

  class Response {
    constructor(body, init) {
      init = init || {};
      this.status = init.status == null ? 200 : Number(init.status);
      this.statusText = init.statusText != null ? String(init.statusText) : '';
      this.ok = init.ok != null ? !!init.ok : this.status >= 200 && this.status < 300;
      this.url = init.url != null ? String(init.url) : '';
      this.redirected = !!init.redirected;
      this.type = 'basic';
      this.headers = init.headers instanceof Headers ? init.headers : headersFromRecord(init.headers);
      this._body = body == null ? '' : String(body);
      this._bytes = init.bodyBase64 ? bytesFromBase64(init.bodyBase64) : utf8Encode(this._body);
    }
    clone() {
      return new Response(this._body, {
        status: this.status,
        statusText: this.statusText,
        ok: this.ok,
        url: this.url,
        redirected: this.redirected,
        headers: cloneHeaders(this.headers),
        bodyBase64: base64FromBytes(this._bytes),
      });
    }
    text() { return Promise.resolve(this._body); }
    json() {
      return new Promise((resolve, reject) => {
        try { resolve(JSON.parse(this._body)); } catch (e) { reject(e); }
      });
    }
    arrayBuffer() {
      const copy = this._bytes.slice();
      return Promise.resolve(copy.buffer.slice(copy.byteOffset, copy.byteOffset + copy.byteLength));
    }
    blob() {
      const type = this.headers.get('content-type') || '';
      return Promise.resolve(new Blob([this._bytes], { type }));
    }
  }

  function utf8Encode(str) {
    const s = String(str);
    const bytes = [];
    for (let i = 0; i < s.length; i += 1) {
      let c = s.charCodeAt(i);
      if (c < 0x80) bytes.push(c);
      else if (c < 0x800) {
        bytes.push(0xc0 | (c >> 6), 0x80 | (c & 0x3f));
      } else if (c >= 0xd800 && c <= 0xdbff) {
        if (i + 1 < s.length) {
          const c2 = s.charCodeAt(i + 1);
          if (c2 >= 0xdc00 && c2 <= 0xdfff) {
            const cp = ((c - 0xd800) << 10) + (c2 - 0xdc00) + 0x10000;
            bytes.push(
              0xf0 | (cp >> 18),
              0x80 | ((cp >> 12) & 0x3f),
              0x80 | ((cp >> 6) & 0x3f),
              0x80 | (cp & 0x3f),
            );
            i += 1;
            continue;
          }
        }
        bytes.push(0xef, 0xbf, 0xbd);
      } else if (c >= 0xdc00 && c <= 0xdfff) {
        bytes.push(0xef, 0xbf, 0xbd);
      } else {
        bytes.push(0xe0 | (c >> 12), 0x80 | ((c >> 6) & 0x3f), 0x80 | (c & 0x3f));
      }
    }
    return new Uint8Array(bytes);
  }

  global.Headers = Headers;
  global.Request = Request;
  global.Response = Response;

  global.fetch = function (input, init) {
    return new Promise((resolve, reject) => {
      queueMicrotask(() => {
        try {
          const req = new Request(input, init || {});
          const signal = req.signal;
          if (signal && signal.aborted) {
            const err = new Error('The operation was aborted');
            err.name = 'AbortError';
            reject(err);
            return;
          }
          const raw = hostFetch('fetch', req.url, toRpcOptions(req.method, req.headers, req._body, req.credentials));
          const payload = normalizeHostPayload(raw, req.url);
          resolve(new Response(payload.body, payload));
        } catch (e) {
          reject(e instanceof Error ? e : new TypeError(String(e)));
        }
      });
    });
  };

  const XHR_UNSENT = 0;
  const XHR_OPENED = 1;
  const XHR_HEADERS_RECEIVED = 2;
  const XHR_LOADING = 3;
  const XHR_DONE = 4;

  function dispatchXhrEvent(xhr, type) {
    const event = { type, target: xhr, currentTarget: xhr };
    const set = xhr._listeners.get(type);
    if (set) Array.from(set).forEach((fn) => { try { fn(event); } catch (e) { console.error(e); } });
    const handler = xhr['on' + type];
    if (typeof handler === 'function') {
      try { handler.call(xhr, event); } catch (e) { console.error(e); }
    }
  }

  function setXhrState(xhr, state) {
    xhr.readyState = state;
    dispatchXhrEvent(xhr, 'readystatechange');
  }

  class XMLHttpRequest {
    constructor() {
      this.UNSENT = XHR_UNSENT;
      this.OPENED = XHR_OPENED;
      this.HEADERS_RECEIVED = XHR_HEADERS_RECEIVED;
      this.LOADING = XHR_LOADING;
      this.DONE = XHR_DONE;
      this.readyState = XHR_UNSENT;
      this.status = 0;
      this.statusText = '';
      this._responseText = '';
      this.response = null;
      this.responseType = '';
      this.responseURL = '';
      this.timeout = 0;
      this.withCredentials = false;
      this._method = '';
      this._url = '';
      this._async = true;
      this._headers = new Headers();
      this._aborted = false;
      this._sent = false;
      this._dispatched = false;
      this._sendTimerId = null;
      this._listeners = new Map();
      this.onreadystatechange = null;
      this.onload = null;
      this.onerror = null;
      this.onabort = null;
      this.ontimeout = null;
      this.onloadstart = null;
      this.onloadend = null;
    }
    addEventListener(type, listener) {
      if (typeof listener !== 'function') return;
      const set = this._listeners.get(type) || new Set();
      set.add(listener);
      this._listeners.set(type, set);
    }
    removeEventListener(type, listener) {
      const set = this._listeners.get(type);
      if (set) set.delete(listener);
    }
    dispatchEvent(event) {
      if (!event || !event.type) return false;
      dispatchXhrEvent(this, event.type);
      return true;
    }
    open(method, url, async, user, password) {
      const _ = user; const __ = password;
      if (this.readyState === XHR_DONE || this._aborted) {
        this._reset();
      }
      this._aborted = false;
      this._sent = false;
      this._dispatched = false;
      this._method = String(method || 'GET').toUpperCase();
      this._url = String(url);
      this._async = async !== false;
      this.responseType = '';
      setXhrState(this, XHR_OPENED);
    }
    setRequestHeader(name, value) {
      if (this.readyState !== XHR_OPENED) throw new Error('InvalidStateError');
      this._headers.set(name, value);
    }
    overrideMimeType() {}
    abort() {
      if (this._sendTimerId != null) {
        clearTimeout(this._sendTimerId);
        this._sendTimerId = null;
      }
      this._aborted = true;
      if (!this._sent || this.readyState === XHR_DONE) {
        this.readyState = XHR_UNSENT;
        return;
      }
      this.readyState = XHR_DONE;
      dispatchXhrEvent(this, 'readystatechange');
      dispatchXhrEvent(this, 'abort');
      dispatchXhrEvent(this, 'loadend');
    }
    getResponseHeader(name) {
      if (this.readyState < XHR_HEADERS_RECEIVED || !this._responseHeaders) return null;
      return this._responseHeaders.get(name);
    }
    getAllResponseHeaders() {
      if (this.readyState < XHR_HEADERS_RECEIVED || !this._responseHeaders) return '';
      const lines = [];
      this._responseHeaders.forEach((v, k) => lines.push(`${k}: ${v}`));
      return lines.length ? `${lines.join('\r\n')}\r\n` : '';
    }
    send(body) {
      if (this.readyState !== XHR_OPENED) return;
      if (this._aborted) return;
      if (!this._async && this.timeout > 0) {
        const err = new Error('InvalidAccessError');
        err.name = 'InvalidAccessError';
        throw err;
      }
      const payload = body == null ? '' : String(body);
      this._sent = true;
      this._dispatched = false;
      if (!this._async) {
        this._run(payload);
        return;
      }
      const self = this;
      this._sendTimerId = setTimeout(() => {
        self._sendTimerId = null;
        if (self._aborted) return;
        beginTask();
        try { self._run(payload); } catch (_) { self._onNetworkError(); }
        runMicrotasks();
      }, 0);
    }
    _fillBody(data) {
      const bytes = payloadBytes(data);
      if (this.responseType === 'json') {
        this.response = JSON.parse(data.body);
        this._responseText = '';
      } else if (this.responseType === 'arraybuffer') {
        const copy = bytes.slice();
        this.response = copy.buffer.slice(copy.byteOffset, copy.byteOffset + copy.byteLength);
        this._responseText = '';
      } else if (this.responseType === 'blob') {
        const type = data.headers.get('content-type') || '';
        this.response = new Blob([bytes], { type });
        this._responseText = '';
      } else if (this.responseType === '' || this.responseType === 'text') {
        this._responseText = data.body;
        this.response = data.body;
      } else {
        this._responseText = data.body;
        this.response = data.body;
      }
    }
    _onTimeout() {
      if (this._aborted) return;
      this.status = 0;
      this._responseText = '';
      this.response = null;
      this.readyState = XHR_DONE;
      dispatchXhrEvent(this, 'readystatechange');
      dispatchXhrEvent(this, 'timeout');
      dispatchXhrEvent(this, 'loadend');
    }
    _onNetworkError() {
      if (this._aborted) return;
      this.readyState = XHR_DONE;
      dispatchXhrEvent(this, 'readystatechange');
      dispatchXhrEvent(this, 'error');
      dispatchXhrEvent(this, 'loadend');
    }
    _run(body) {
      if (this._aborted) return;
      this._dispatched = true;
      dispatchXhrEvent(this, 'loadstart');
      const started = performance.now();
      try {
        const creds = this.withCredentials ? 'include' : 'same-origin';
        const raw = hostFetch('xhr', this._url, toRpcOptions(this._method, this._headers, body, creds));
        if (this._aborted) return;
        if (this.timeout > 0 && (performance.now() - started) > this.timeout) {
          this._onTimeout();
          return;
        }
        const data = normalizeHostPayload(raw, this._url);
        this.status = data.status;
        this.statusText = data.statusText;
        this.responseURL = data.url;
        this._responseHeaders = data.headers;
        setXhrState(this, XHR_HEADERS_RECEIVED);
        setXhrState(this, XHR_LOADING);
        if (this._aborted) return;
        this._fillBody(data);
        setXhrState(this, XHR_DONE);
        dispatchXhrEvent(this, 'load');
        dispatchXhrEvent(this, 'loadend');
      } catch (_) {
        if (this._aborted) return;
        this._onNetworkError();
      }
    }
    _reset() {
      this._aborted = false;
      this._sent = false;
      this._dispatched = false;
      this.status = 0;
      this.statusText = '';
      this._responseText = '';
      this.response = null;
      this.responseURL = '';
      this._responseHeaders = null;
      this._headers = new Headers();
    }
    get responseText() {
      if (this.responseType !== '' && this.responseType !== 'text') {
        const err = new Error('The object is in an invalid state.');
        err.name = 'InvalidStateError';
        throw err;
      }
      return this._responseText || '';
    }
    set responseText(value) { this._responseText = value; }
  }
  XMLHttpRequest.UNSENT = XHR_UNSENT;
  XMLHttpRequest.OPENED = XHR_OPENED;
  XMLHttpRequest.HEADERS_RECEIVED = XHR_HEADERS_RECEIVED;
  XMLHttpRequest.LOADING = XHR_LOADING;
  XMLHttpRequest.DONE = XHR_DONE;
  global.XMLHttpRequest = XMLHttpRequest;

  function cloneHistoryState(state) {
    if (state === undefined) return null;
    try {
      return JSON.parse(JSON.stringify(state));
    } catch (_) {
      return null;
    }
  }

  function pageHref() {
    if (typeof global.__glide_get_page_url === 'function') {
      return global.__glide_get_page_url();
    }
    return (global.location && global.location.href) || '';
  }

  function resolvePageUrl(url) {
    const raw = String(url);
    const base = pageHref();
    const parsed = __glide_url_parse(raw, base);
    if (!parsed) {
      const err = new Error('The string did not match the expected pattern.');
      err.name = 'SyntaxError';
      throw err;
    }
    const current = __glide_url_parse(base);
    if (!current) {
      const err = new Error('The operation is insecure.');
      err.name = 'SecurityError';
      throw err;
    }
    if (parsed.origin !== current.origin) {
      const err = new Error('The operation is insecure.');
      err.name = 'SecurityError';
      throw err;
    }
    return parsed.href;
  }

  function coerceHistoryUrl(url) {
    if (url == null || url === '') return null;
    return resolvePageUrl(url);
  }

  const HISTORY_ENTRY_MAX = 100;
  const historyEntries = [];
  let historyIndex = 0;

  function activeHistoryUrl() {
    const entry = historyEntries[historyIndex];
    return (entry && entry.url) || pageHref();
  }

  function trimHistoryEntries() {
    while (historyEntries.length > HISTORY_ENTRY_MAX) {
      historyEntries.shift();
      historyIndex -= 1;
    }
    if (historyIndex < 0) historyIndex = 0;
  }

  function applyHistoryUrl(href) {
    if (href != null && typeof global.__glide_set_page_url === 'function') {
      global.__glide_set_page_url(href);
    }
  }

  function firePopState(state) {
    const EventCtor = global.Event || function () {};
    const ev = new EventCtor('popstate');
    ev.state = state;
    global.dispatchEvent(ev);
    if (typeof global.onpopstate === 'function') {
      try {
        global.onpopstate.call(global, ev);
      } catch (e) {
        console.error(e);
      }
    }
  }

  function historyGo(target) {
    if (!Number.isInteger(target) || target < 0 || target >= historyEntries.length) return;
    historyIndex = target;
    const entry = historyEntries[historyIndex];
    applyHistoryUrl(entry.url);
    firePopState(cloneHistoryState(entry.state));
  }

  global.__glide_reset_page_history = function () {
    historyEntries.length = 0;
    historyEntries.push({ url: pageHref(), state: null });
    historyIndex = 0;
  };

  global.history = {
    get state() {
      const entry = historyEntries[historyIndex];
      return cloneHistoryState(entry ? entry.state : null);
    },
    get length() {
      return Math.max(1, historyEntries.length);
    },
    scrollRestoration: 'auto',
    pushState(state, _unused, url) {
      const resolved = coerceHistoryUrl(url);
      const clone = cloneHistoryState(state);
      historyEntries.length = historyIndex + 1;
      const href = resolved || activeHistoryUrl();
      historyEntries.push({ url: href, state: clone });
      historyIndex = historyEntries.length - 1;
      trimHistoryEntries();
      if (resolved) applyHistoryUrl(href);
    },
    replaceState(state, _unused, url) {
      const resolved = coerceHistoryUrl(url);
      const clone = cloneHistoryState(state);
      const entry = historyEntries[historyIndex];
      if (resolved) {
        entry.url = resolved;
        applyHistoryUrl(resolved);
      }
      entry.state = clone;
    },
    back() { historyGo(historyIndex - 1); },
    forward() { historyGo(historyIndex + 1); },
    go(delta) {
      const n = Number(delta);
      const step = Number.isFinite(n) ? Math.trunc(n) : 0;
      if (step === 0) return;
      historyGo(historyIndex + step);
    },
  };

  const CRYPTO_BYTE_QUOTA = 65536;

  function hostRandomBytes(size) {
    const n = Number(size);
    if (!Number.isFinite(n) || n < 0) throw new TypeError('Invalid random byte size');
    const raw = __glide_dom_sync('randomBytes', 0, String(Math.floor(n)));
    if (raw && raw.__error) {
      const err = new Error(String(raw.__error));
      err.name = raw.__error === 'QuotaExceededError' ? 'QuotaExceededError' : 'Error';
      throw err;
    }
    return raw;
  }

  const RANDOM_TYPED_ARRAYS = new Set([
    'Int8Array', 'Uint8Array', 'Uint8ClampedArray',
    'Int16Array', 'Uint16Array', 'Int32Array', 'Uint32Array',
    'BigInt64Array', 'BigUint64Array',
  ]);

  function cryptoGetRandomValues(view) {
    if (!view || !ArrayBuffer.isView(view)) {
      throw new TypeError('Expected an ArrayBuffer view');
    }
    const name = view.constructor && view.constructor.name;
    if (!name || !RANDOM_TYPED_ARRAYS.has(name)) {
      throw new TypeError('Expected a typed array');
    }
    const len = view.byteLength;
    if (len > CRYPTO_BYTE_QUOTA) {
      const err = new Error('The requested length exceeds the quota.');
      err.name = 'QuotaExceededError';
      throw err;
    }
    const bytes = hostRandomBytes(len);
    const u8 = new Uint8Array(view.buffer, view.byteOffset, view.byteLength);
    for (let i = 0; i < len; i += 1) u8[i] = bytes[i];
    return view;
  }

  function cryptoRandomUUID() {
    const bytes = hostRandomBytes(16);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    const hex = bytes.map((b) => b.toString(16).padStart(2, '0')).join('');
    return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
  }

  global.crypto = {
    getRandomValues: cryptoGetRandomValues,
    randomUUID: cryptoRandomUUID,
    get subtle() { return undefined; },
  };

  function ensureNavigator() {
    if (global.navigator == null) global.navigator = {};
    return global.navigator;
  }
  const navigatorShim = ensureNavigator();
  if (navigatorShim.userAgent == null) navigatorShim.userAgent = 'Glide';
  if (navigatorShim.languages == null) {
    navigatorShim.languages = Object.freeze(['en-US', 'en']);
  }
  if (navigatorShim.language == null) navigatorShim.language = 'en-US';
  if (navigatorShim.platform == null) {
    navigatorShim.platform = global.__glide_platform || '';
  }
  if (navigatorShim.onLine == null) navigatorShim.onLine = true;
  if (navigatorShim.hardwareConcurrency == null) {
    const cores = global.__glide_hardware_concurrency;
    navigatorShim.hardwareConcurrency = typeof cores === 'number' && cores >= 1 ? cores : 1;
  }
  navigatorShim.sendBeacon = function (url, data) {
    const body = data == null ? '' : (typeof data === 'string' ? data : String(data));
    setTimeout(() => {
      try {
        hostFetch('fetch', String(url), JSON.stringify({ method: 'POST', body, headers: { 'Content-Type': 'text/plain;charset=UTF-8' } }));
      } catch (_) { /* fire-and-forget */ }
    }, 0);
    return true;
  };

  function partsToBytes(parts) {
    const chunks = [];
    let size = 0;
    (parts || []).forEach((part) => {
      if (part == null) return;
      if (part instanceof Blob) {
        chunks.push(part._bytes);
        size += part._bytes.length;
      } else if (part instanceof ArrayBuffer) {
        const view = new Uint8Array(part);
        chunks.push(view);
        size += view.length;
      } else if (ArrayBuffer.isView(part)) {
        const view = new Uint8Array(part.buffer, part.byteOffset, part.byteLength);
        chunks.push(view);
        size += view.length;
      } else {
        const bytes = utf8Encode(String(part));
        chunks.push(bytes);
        size += bytes.length;
      }
    });
    const out = new Uint8Array(size);
    let off = 0;
    chunks.forEach((chunk) => { out.set(chunk, off); off += chunk.length; });
    return out;
  }

  class Blob {
    constructor(parts, options) {
      options = options || {};
      this._bytes = partsToBytes(parts);
      this.size = this._bytes.length;
      this.type = options.type != null ? String(options.type) : '';
    }
    slice(start, end, type) {
      const s = start == null ? 0 : Number(start);
      const e = end == null ? this._bytes.length : Number(end);
      const slice = this._bytes.subarray(s, e);
      return new Blob([slice.buffer.slice(slice.byteOffset, slice.byteOffset + slice.byteLength)], { type: type || this.type });
    }
    text() {
      return Promise.resolve(new TextDecoder().decode(this._bytes));
    }
    arrayBuffer() {
      const copy = this._bytes.slice();
      return Promise.resolve(copy.buffer);
    }
  }
  global.Blob = Blob;

  global.File = function File(parts, name, options) {
    const blob = new Blob(parts, options || {});
    blob.name = String(name);
    blob.lastModified = (options && options.lastModified) || Date.now();
    return blob;
  };

  const FR_EMPTY = 0;
  const FR_LOADING = 1;
  const FR_DONE = 2;

  class FileReader {
    constructor() {
      this.EMPTY = FR_EMPTY;
      this.LOADING = FR_LOADING;
      this.DONE = FR_DONE;
      this.readyState = FR_EMPTY;
      this.result = null;
      this.error = null;
      this.onload = null;
      this.onerror = null;
      this.onloadend = null;
      this.onabort = null;
      this._listeners = new Map();
      this._aborted = false;
    }
    addEventListener(type, listener) {
      if (typeof listener !== 'function') return;
      const set = this._listeners.get(type) || new Set();
      set.add(listener);
      this._listeners.set(type, set);
    }
    removeEventListener(type, listener) {
      const set = this._listeners.get(type);
      if (set) set.delete(listener);
    }
    abort() {
      if (this.readyState !== FR_LOADING) return;
      this._aborted = true;
      this.readyState = FR_DONE;
      this._fire('abort');
      this._fire('loadend');
    }
    readAsText(blob, encoding) {
      if (!(blob instanceof Blob)) throw new TypeError('parameter 1 is not of type Blob');
      const _ = encoding;
      this._read(blob, () => new TextDecoder().decode(blob._bytes));
    }
    readAsArrayBuffer(blob) {
      if (!(blob instanceof Blob)) throw new TypeError('parameter 1 is not of type Blob');
      this._read(blob, () => {
        const copy = blob._bytes.slice();
        return copy.buffer;
      });
    }
    readAsDataURL(blob) {
      if (!(blob instanceof Blob)) throw new TypeError('parameter 1 is not of type Blob');
      this._read(blob, () => {
        const type = blob.type || 'application/octet-stream';
        let binary = '';
        for (let i = 0; i < blob._bytes.length; i += 1) {
          binary += String.fromCharCode(blob._bytes[i]);
        }
        return `data:${type};base64,${btoa(binary)}`;
      });
    }
    _read(blob, produce) {
      this._aborted = false;
      this.readyState = FR_LOADING;
      this._fire('loadstart');
      queueMicrotask(() => {
        if (this._aborted) return;
        try {
          this.result = produce();
          this.readyState = FR_DONE;
          this._fire('load');
        } catch (e) {
          this.error = e;
          this.readyState = FR_DONE;
          this._fire('error');
        }
        this._fire('loadend');
      });
    }
    _fire(type) {
      const event = { type, target: this };
      const set = this._listeners.get(type);
      if (set) Array.from(set).forEach((fn) => { try { fn(event); } catch (e) { console.error(e); } });
      const handler = this['on' + type];
      if (typeof handler === 'function') {
        try { handler.call(this, event); } catch (e) { console.error(e); }
      }
    }
  }
  FileReader.EMPTY = FR_EMPTY;
  FileReader.LOADING = FR_LOADING;
  FileReader.DONE = FR_DONE;
  global.FileReader = FileReader;

  class TextEncoder {
    constructor() { this.encoding = 'utf-8'; }
    encode(input) { return utf8Encode(input == null ? '' : input); }
    encodeInto(input, dest) {
      const bytes = this.encode(input);
      const n = Math.min(bytes.length, dest.length);
      dest.set(bytes.subarray(0, n));
      return { read: String(input).length, written: n };
    }
  }
  global.TextEncoder = TextEncoder;

  function utf8DecodeFatal(fatal) {
    if (fatal) throw new TypeError('Decoding failed');
    return '\uFFFD';
  }

  function isContinuation(b) {
    return (b & 0xc0) === 0x80;
  }

  function utf8DecodeBytes(arr, fatal, partial) {
    let out = '';
    let i = 0;
    const need = (n) => {
      if (i + n > arr.length) {
        if (partial) return false;
        utf8DecodeFatal(fatal);
        out += '\uFFFD';
        i += 1;
        return 'skip';
      }
      return true;
    };
    while (i < arr.length) {
      const b0 = arr[i];
      if (b0 < 0x80) {
        out += String.fromCharCode(b0);
        i += 1;
        continue;
      }
      if ((b0 & 0xe0) === 0xc0) {
        const ok = need(2);
        if (ok === false) break;
        if (ok === 'skip') continue;
        const b1 = arr[i + 1];
        if (!isContinuation(b1) || b0 < 0xc2) {
          out += utf8DecodeFatal(fatal);
          i += 1;
          continue;
        }
        out += String.fromCharCode(((b0 & 0x1f) << 6) | (b1 & 0x3f));
        i += 2;
        continue;
      }
      if ((b0 & 0xf0) === 0xe0) {
        const ok = need(3);
        if (ok === false) break;
        if (ok === 'skip') continue;
        const b1 = arr[i + 1];
        const b2 = arr[i + 2];
        if (!isContinuation(b1) || !isContinuation(b2)) {
          out += utf8DecodeFatal(fatal);
          i += 1;
          continue;
        }
        let cp = ((b0 & 0x0f) << 12) | ((b1 & 0x3f) << 6) | (b2 & 0x3f);
        if (cp >= 0xd800 && cp <= 0xdfff) {
          out += utf8DecodeFatal(fatal);
          i += 1;
          continue;
        }
        out += String.fromCharCode(cp);
        i += 3;
        continue;
      }
      if ((b0 & 0xf8) === 0xf0) {
        const ok = need(4);
        if (ok === false) break;
        if (ok === 'skip') continue;
        const b1 = arr[i + 1];
        const b2 = arr[i + 2];
        const b3 = arr[i + 3];
        if (!isContinuation(b1) || !isContinuation(b2) || !isContinuation(b3) || b0 > 0xf4) {
          out += utf8DecodeFatal(fatal);
          i += 1;
          continue;
        }
        const cp = ((b0 & 0x07) << 18)
          | ((b1 & 0x3f) << 12)
          | ((b2 & 0x3f) << 6)
          | (b3 & 0x3f);
        if (cp > 0x10ffff || cp < 0x10000) {
          out += utf8DecodeFatal(fatal);
          i += 1;
          continue;
        }
        const adj = cp - 0x10000;
        out += String.fromCharCode(0xd800 + (adj >> 10), 0xdc00 + (adj & 0x3ff));
        i += 4;
        continue;
      }
      out += utf8DecodeFatal(fatal);
      i += 1;
    }
    return { text: out, rest: arr.slice(i) };
  }

  class TextDecoder {
    constructor(label, options) {
      const name = String(label == null ? 'utf-8' : label).toLowerCase();
      if (name !== 'utf-8' && name !== 'utf8') {
        throw new RangeError('Encoding not supported');
      }
      options = options || {};
      this.encoding = 'utf-8';
      this.fatal = !!options.fatal;
      this.ignoreBOM = !!options.ignoreBOM;
      this._stream = new Uint8Array(0);
    }
    decode(input, options) {
      options = options || {};
      let bytes;
      if (input == null) bytes = new Uint8Array(0);
      else if (input instanceof ArrayBuffer) bytes = new Uint8Array(input);
      else if (ArrayBuffer.isView(input)) bytes = new Uint8Array(input.buffer, input.byteOffset, input.byteLength);
      else bytes = new Uint8Array(0);
      const merged = new Uint8Array(this._stream.length + bytes.length);
      merged.set(this._stream);
      merged.set(bytes, this._stream.length);
      let source = merged;
      if (!this.ignoreBOM && source.length >= 3
        && source[0] === 0xef && source[1] === 0xbb && source[2] === 0xbf) {
        source = source.subarray(3);
      }
      const decoded = utf8DecodeBytes(source, this.fatal, !!options.stream);
      if (options.stream) {
        this._stream = new Uint8Array(decoded.rest);
        return decoded.text;
      }
      this._stream = new Uint8Array(0);
      return decoded.text;
    }
  }
  global.TextDecoder = TextDecoder;

  const WS_CONNECTING = 0;
  const WS_OPEN = 1;
  const WS_CLOSING = 2;
  const WS_CLOSED = 3;
  let nextWsId = 1;
  const wsRegistry = new Map();

  function wsDomError(name, message) {
    const e = new Error(message || name);
    e.name = name;
    return e;
  }

  function utf8ByteLength(value) {
    return new TextEncoder().encode(String(value)).length;
  }

  function bytesToBase64(bytes) {
    let binary = '';
    const chunk = 0x8000;
    for (let i = 0; i < bytes.length; i += chunk) {
      const sub = bytes.subarray(i, i + chunk);
      binary += String.fromCharCode.apply(null, sub);
    }
    return btoa(binary);
  }

  function base64ToBytes(b64) {
    const raw = atob(String(b64 || ''));
    const bytes = new Uint8Array(raw.length);
    for (let i = 0; i < raw.length; i++) bytes[i] = raw.charCodeAt(i);
    return bytes;
  }

  if (typeof global.Event !== 'function') {
    global.Event = function Event(type, init) {
      this.type = String(type);
      const options = init || {};
      this.bubbles = !!options.bubbles;
      this.cancelable = !!options.cancelable;
    };
  }

  function validateCloseCode(code) {
    if (code !== 1000 && !(code >= 3000 && code <= 4999)) {
      throw wsDomError('InvalidAccessError', 'close code out of range');
    }
    if (code === 1005) {
      throw wsDomError('InvalidAccessError', 'close code 1005 is reserved');
    }
  }

  class MessageEvent extends global.Event {
    constructor(type, init) {
      super(type);
      init = init || {};
      this.data = init.data;
      this.origin = init.origin || '';
      this.lastEventId = init.lastEventId || '';
      this.source = init.source || null;
      this.ports = init.ports || [];
    }
  }

  class CloseEvent extends global.Event {
    constructor(type, init) {
      super(type);
      init = init || {};
      this.code = init.code == null ? 1006 : Number(init.code);
      this.reason = init.reason == null ? '' : String(init.reason);
      this.wasClean = !!init.wasClean;
    }
  }

  function dispatchWsEvent(ws, type, eventInit) {
    let event;
    if (type === 'message') {
      event = new MessageEvent('message', Object.assign({ target: ws, currentTarget: ws }, eventInit || {}));
    } else if (type === 'close') {
      event = new CloseEvent('close', Object.assign({ target: ws, currentTarget: ws }, eventInit || {}));
    } else {
      event = Object.assign(new global.Event(type), { target: ws, currentTarget: ws }, eventInit || {});
    }
    const set = ws._listeners.get(type);
    if (set) Array.from(set).forEach((fn) => { try { fn(event); } catch (e) { console.error(e); } });
    const handler = ws['on' + type];
    if (typeof handler === 'function') {
      try { handler.call(ws, event); } catch (e) { console.error(e); }
    }
  }

  global.__glide_ws_deliver = function (json) {
    let ev;
    try { ev = JSON.parse(String(json)); } catch (_) { return; }
    const ws = wsRegistry.get(ev.id);
    if (!ws) return;
    const kind = ev.type;
    if (kind === 'open') {
      ws.readyState = WS_OPEN;
      if (typeof ev.protocol === 'string') ws.protocol = ev.protocol;
      dispatchWsEvent(ws, 'open');
    } else if (kind === 'message') {
      let data;
      if (ev.binary) {
        const bytes = base64ToBytes(ev.data);
        data = ws.binaryType === 'arraybuffer'
          ? bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength)
          : new Blob([bytes]);
      } else {
        data = String(ev.data || '');
      }
      dispatchWsEvent(ws, 'message', { data });
    } else if (kind === 'error') {
      dispatchWsEvent(ws, 'error');
    } else if (kind === 'close') {
      ws.readyState = WS_CLOSED;
      wsRegistry.delete(ev.id);
      dispatchWsEvent(ws, 'close', {
        code: typeof ev.code === 'number' ? ev.code : 1006,
        reason: String(ev.reason || ''),
        wasClean: !!ev.was_clean,
      });
    }
    runMicrotasks();
  };

  class WebSocket {
    constructor(url, protocols) {
      this.CONNECTING = WS_CONNECTING;
      this.OPEN = WS_OPEN;
      this.CLOSING = WS_CLOSING;
      this.CLOSED = WS_CLOSED;
      this.readyState = WS_CONNECTING;
      this.url = String(url);
      this.protocol = '';
      this.extensions = '';
      this.binaryType = 'blob';
      this._bufferedAmount = 0;
      this._id = nextWsId++;
      this._listeners = new Map();
      this.onopen = null;
      this.onmessage = null;
      this.onerror = null;
      this.onclose = null;
      Object.defineProperty(this, 'bufferedAmount', {
        configurable: true,
        enumerable: true,
        get: () => {
          if (this.readyState === WS_CLOSED) return 0;
          const raw = __glide_dom_sync('wsBuffered', 0, String(this._id));
          if (typeof raw === 'number' && Number.isFinite(raw)) this._bufferedAmount = raw;
          return this._bufferedAmount;
        },
      });
      wsRegistry.set(this._id, this);
      let protocolsJson = '[]';
      if (protocols != null) {
        protocolsJson = JSON.stringify(Array.isArray(protocols) ? protocols : [String(protocols)]);
      }
      const raw = __glide_dom_sync('wsConnect', 0, String(this._id), this.url, protocolsJson);
      if (raw && typeof raw === 'object' && raw.__error != null) {
        wsRegistry.delete(this._id);
        this.readyState = WS_CLOSED;
        const msg = String(raw.__error);
        let name = 'SyntaxError';
        if (msg.includes('mixed content')) name = 'SecurityError';
        throw wsDomError(name, msg);
      }
      if (typeof raw === 'string' && raw) this.url = raw;
    }
    addEventListener(type, listener) {
      if (typeof listener !== 'function') return;
      const set = this._listeners.get(type) || new Set();
      set.add(listener);
      this._listeners.set(type, set);
    }
    removeEventListener(type, listener) {
      const set = this._listeners.get(type);
      if (set) set.delete(listener);
    }
    dispatchEvent(event) {
      if (!event || !event.type) return false;
      dispatchWsEvent(this, event.type, event);
      return true;
    }
    send(data) {
      if (this.readyState === WS_CONNECTING || this.readyState === WS_CLOSING || this.readyState === WS_CLOSED) {
        throw wsDomError('InvalidStateError', 'WebSocket is not open');
      }
      let payload;
      let binary = false;
      if (typeof data === 'string') {
        payload = data;
      } else if (data instanceof Blob) {
        binary = true;
        payload = bytesToBase64(data._bytes || new Uint8Array());
      } else if (data instanceof ArrayBuffer) {
        binary = true;
        payload = bytesToBase64(new Uint8Array(data));
      } else if (ArrayBuffer.isView(data)) {
        binary = true;
        const view = new Uint8Array(data.buffer, data.byteOffset, data.byteLength);
        payload = bytesToBase64(view);
      } else {
        payload = String(data);
      }
      const raw = __glide_dom_sync('wsSend', 0, String(this._id), payload, binary ? '1' : '0');
      if (raw && typeof raw === 'object' && raw.__error != null) {
        throw new Error(String(raw.__error));
      }
      if (typeof raw === 'number') this._bufferedAmount = raw;
    }
    close(code, reason) {
      if (this.readyState === WS_CLOSING || this.readyState === WS_CLOSED) return;
      const c = code == null ? 1000 : Number(code);
      const r = reason == null ? '' : String(reason);
      if (code != null) validateCloseCode(c);
      if (utf8ByteLength(r) > 123) {
        throw wsDomError('SyntaxError', 'close reason is too long');
      }
      this.readyState = WS_CLOSING;
      __glide_dom_sync('wsClose', 0, String(this._id), String(c), r);
    }
  }
  WebSocket.CONNECTING = WS_CONNECTING;
  WebSocket.OPEN = WS_OPEN;
  WebSocket.CLOSING = WS_CLOSING;
  WebSocket.CLOSED = WS_CLOSED;
  global.WebSocket = WebSocket;

  function fireLoaded() {
    if (global.__glide_mark_document_loaded) global.__glide_mark_document_loaded();
    if (global.__glide_dispatch_dom_content_loaded) {
      global.__glide_dispatch_dom_content_loaded();
    } else {
      document.dispatchEvent(new Event('DOMContentLoaded'));
    }
    runMicrotasks();
  }

  global.__glide_fire_dom_content_loaded = fireLoaded;
  global.__glide_drain_microtasks = function () {
    return runMicrotasks();
  };
})(globalThis);