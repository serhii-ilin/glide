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

  function normalizeHeaderName(name) {
    return String(name).trim().toLowerCase();
  }

  class Headers {
    constructor(init) {
      this._map = new Map();
      if (init) Headers.appendAll(this, init);
    }
    append(name, value) {
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
    set(name, value) { this._map.set(normalizeHeaderName(name), String(value)); }
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

  function hostFetch(url, optionsJson) {
    if (optionsJson == null) {
      return __glide_dom_sync('fetch', 0, String(url));
    }
    return __glide_dom_sync('fetch', 0, String(url), optionsJson);
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

  function toRpcOptions(method, headers, body) {
    const headerObj = {};
    if (headers) {
      headers.forEach((v, k) => { headerObj[k] = v; });
    }
    const opt = { method: method || 'GET', headers: headerObj };
    if (body != null && body !== '') opt.body = String(body);
    return JSON.stringify(opt);
  }

  class Request {
    constructor(input, init) {
      init = init || {};
      const req = input instanceof Request ? input : null;
      const url = req ? req.url : String(input);
      this.url = url;
      this.method = String((init.method || (req && req.method) || 'GET')).toUpperCase();
      this.headers = new Headers();
      Headers.appendAll(this.headers, req ? req.headers : null);
      Headers.appendAll(this.headers, init.headers || null);
      if (init.body !== undefined) this._body = init.body == null ? null : String(init.body);
      else this._body = req ? req._body : null;
      this._signal = init.signal || (req && req._signal) || null;
    }
    clone() {
      const copy = new Request(this.url, {
        method: this.method,
        headers: this.headers,
        body: this._body,
        signal: this._signal,
      });
      return copy;
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
    }
    clone() {
      return new Response(this._body, {
        status: this.status,
        statusText: this.statusText,
        ok: this.ok,
        url: this.url,
        redirected: this.redirected,
        headers: this.headers,
      });
    }
    text() { return Promise.resolve(this._body); }
    json() {
      return new Promise((resolve, reject) => {
        try { resolve(JSON.parse(this._body)); } catch (e) { reject(e); }
      });
    }
    arrayBuffer() {
      const bytes = utf8Encode(this._body);
      return Promise.resolve(bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength));
    }
    blob() {
      const type = this.headers.get('content-type') || '';
      return Promise.resolve(new Blob([this._body], { type }));
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
      } else if (c >= 0xd800 && c <= 0xdbff && i + 1 < s.length) {
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
        } else {
          bytes.push(0xef, 0xbf, 0xbd);
        }
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
          const req = input instanceof Request ? input : new Request(input, init || {});
          const signal = (init && init.signal) || req._signal;
          if (signal && signal.aborted) {
            const err = new Error('The operation was aborted');
            err.name = 'AbortError';
            reject(err);
            return;
          }
          const raw = hostFetch(req.url, toRpcOptions(req.method, req.headers, req._body));
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
      this.responseText = '';
      this.response = null;
      this.responseType = '';
      this.responseURL = '';
      this.timeout = 0;
      this._method = '';
      this._url = '';
      this._async = true;
      this._headers = new Headers();
      this._aborted = false;
      this._timeoutId = null;
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
      if (this.readyState === XHR_DONE) {
        this._reset();
      }
      this._method = String(method || 'GET').toUpperCase();
      this._url = String(url);
      this._async = async !== false;
      setXhrState(this, XHR_OPENED);
    }
    setRequestHeader(name, value) {
      if (this.readyState !== XHR_OPENED) throw new Error('InvalidStateError');
      this._headers.set(name, value);
    }
    overrideMimeType() {}
    abort() {
      this._aborted = true;
      if (this._timeoutId != null) clearTimeout(this._timeoutId);
      this.readyState = XHR_UNSENT;
      dispatchXhrEvent(this, 'abort');
      this.readyState = XHR_DONE;
      dispatchXhrEvent(this, 'readystatechange');
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
      return lines.join('\r\n');
    }
    send(body) {
      if (this.readyState !== XHR_OPENED || this._aborted) return;
      const payload = body == null ? '' : String(body);
      const run = () => this._perform(payload);
      if (!this._async) {
        run();
        return;
      }
      if (this.timeout > 0) {
        this._timeoutId = setTimeout(() => {
          if (this.readyState < XHR_DONE) dispatchXhrEvent(this, 'timeout');
        }, this.timeout);
      }
      setTimeout(() => {
        beginTask();
        try { run(); } catch (e) { dispatchXhrEvent(this, 'error'); dispatchXhrEvent(this, 'loadend'); }
        runMicrotasks();
      }, 0);
    }
    _perform(body) {
      dispatchXhrEvent(this, 'loadstart');
      setXhrState(this, XHR_HEADERS_RECEIVED);
      setXhrState(this, XHR_LOADING);
      try {
        const raw = hostFetch(this._url, toRpcOptions(this._method, this._headers, body));
        const data = normalizeHostPayload(raw, this._url);
        this.status = data.status;
        this.statusText = data.statusText;
        this.responseURL = data.url;
        this._responseHeaders = data.headers;
        if (this.responseType === 'json') {
          this.response = JSON.parse(data.body);
          this.responseText = '';
        } else if (this.responseType === 'arraybuffer') {
          const bytes = utf8Encode(data.body);
          this.response = bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength);
          this.responseText = '';
        } else if (this.responseType === '' || this.responseType === 'text') {
          this.responseText = data.body;
          this.response = data.body;
        } else {
          this.responseText = data.body;
          this.response = data.body;
        }
        setXhrState(this, XHR_DONE);
        dispatchXhrEvent(this, 'load');
      } catch (e) {
        dispatchXhrEvent(this, 'error');
        this.readyState = XHR_DONE;
        dispatchXhrEvent(this, 'readystatechange');
      }
      if (this._timeoutId != null) clearTimeout(this._timeoutId);
      dispatchXhrEvent(this, 'loadend');
    }
    _reset() {
      this._aborted = false;
      this.status = 0;
      this.statusText = '';
      this.responseText = '';
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

  function ensureNavigator() {
    if (global.navigator == null) global.navigator = {};
    return global.navigator;
  }
  const navigatorShim = ensureNavigator();
  if (navigatorShim.userAgent == null) navigatorShim.userAgent = 'Glide';
  navigatorShim.sendBeacon = function (url, data) {
    const body = data == null ? '' : (typeof data === 'string' ? data : String(data));
    setTimeout(() => {
      try {
        hostFetch(String(url), JSON.stringify({ method: 'POST', body, headers: { 'Content-Type': 'text/plain;charset=UTF-8' } }));
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
      this._aborted = true;
      this.readyState = FR_DONE;
      this._fire('abort');
      this._fire('loadend');
    }
    readAsText(blob, encoding) {
      const _ = encoding;
      this._read(blob, () => new TextDecoder().decode(blob._bytes));
    }
    readAsArrayBuffer(blob) {
      this._read(blob, () => {
        const copy = blob._bytes.slice();
        return copy.buffer;
      });
    }
    readAsDataURL(blob) {
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

  function utf8DecodeBytes(arr, partial) {
    let out = '';
    let i = 0;
    while (i < arr.length) {
      const b0 = arr[i];
      if (b0 < 0x80) {
        out += String.fromCharCode(b0);
        i += 1;
        continue;
      }
      if ((b0 & 0xe0) === 0xc0 && i + 1 < arr.length) {
        out += String.fromCharCode(((b0 & 0x1f) << 6) | (arr[i + 1] & 0x3f));
        i += 2;
        continue;
      }
      if ((b0 & 0xf0) === 0xe0 && i + 2 < arr.length) {
        out += String.fromCharCode(
          ((b0 & 0x0f) << 12) | ((arr[i + 1] & 0x3f) << 6) | (arr[i + 2] & 0x3f),
        );
        i += 3;
        continue;
      }
      if ((b0 & 0xf8) === 0xf0 && i + 3 < arr.length) {
        const cp = ((b0 & 0x07) << 18)
          | ((arr[i + 1] & 0x3f) << 12)
          | ((arr[i + 2] & 0x3f) << 6)
          | (arr[i + 3] & 0x3f);
        const adj = cp - 0x10000;
        out += String.fromCharCode(0xd800 + (adj >> 10), 0xdc00 + (adj & 0x3ff));
        i += 4;
        continue;
      }
      if (partial && i >= arr.length - 3) break;
      out += '\uFFFD';
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
      const decoded = utf8DecodeBytes(merged, !!options.stream);
      if (options.stream) {
        this._stream = new Uint8Array(decoded.rest);
        return decoded.text;
      }
      this._stream = new Uint8Array(0);
      return decoded.text;
    }
  }
  global.TextDecoder = TextDecoder;

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