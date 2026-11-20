// Minimal DOM / event loop shims installed before page scripts run.
(function (global) {
  'use strict';

  let nextReply = 1;
  const eventTargetData = new WeakMap();
  const nodeCache = new Map();

  function rpc(op, ...args) {
    const reply = nextReply++;
    const result = __glide_dom_sync(op, reply, ...args);
    if (result && result.__error) {
      throw new Error(result.__error);
    }
    return result;
  }

  function wrapNode(id) {
    if (id === null || id === undefined) return null;
    const key = String(id);
    const cached = nodeCache.get(key);
    if (cached) return cached;
    const self = {
      __id: key,
      parentNode: null,
      get parentElement() {
        const p = rpc('parentId', this.__id);
        return p ? wrapNode(p) : null;
      },
      appendChild(child) {
        rpc('appendChild', this.__id, child.__id);
        return child;
      },
      removeChild(child) {
        rpc('removeChild', this.__id, child.__id);
        return child;
      },
      insertBefore(child, before) {
        rpc('insertBefore', this.__id, child.__id, before ? before.__id : '0');
        return child;
      },
      get textContent() {
        return rpc('getTextContent', this.__id) || '';
      },
      set textContent(v) {
        rpc('setTextContent', this.__id, String(v));
      },
      set innerHTML(html) {
        rpc('setInnerHtml', this.__id, String(html));
      },
      getAttribute(name) {
        return rpc('getAttribute', this.__id, name);
      },
      setAttribute(name, value) {
        rpc('setAttribute', this.__id, name, String(value));
      },
      removeAttribute(name) {
        rpc('removeAttribute', this.__id, name);
      },
      get classList() {
        const node = this;
        return {
          add(t) { rpc('classAdd', node.__id, t); },
          remove(t) { rpc('classRemove', node.__id, t); },
          contains(t) { return !!rpc('classContains', node.__id, t); },
        };
      },
      get style() {
        const node = this;
        return new Proxy({}, {
          set(_, prop, value) {
            rpc('styleSet', node.__id, String(prop), String(value));
            return true;
          },
        });
      },
      addEventListener(type, fn) {
        let bag = eventTargetData.get(self);
        if (!bag) {
          bag = {};
          eventTargetData.set(self, bag);
        }
        if (!bag[type]) bag[type] = [];
        bag[type].push(fn);
        rpc('listen', self.__id, type);
      },
      removeEventListener(type, fn) {
        const bag = eventTargetData.get(self);
        if (!bag || !bag[type]) return;
        bag[type] = bag[type].filter((f) => f !== fn);
      },
      dispatchEvent(evt) {
        const bag = eventTargetData.get(self);
        const list = bag && bag[evt.type];
        if (!list) return true;
        list.slice().forEach((fn) => {
          try { fn.call(self, evt); } catch (e) { console.error(e); }
        });
        return true;
      },
      click() {
        this.dispatchEvent({ type: 'click', target: this, bubbles: true });
      },
    };
    nodeCache.set(key, self);
    return self;
  }

  function wrapList(ids) {
    const nodes = (ids || []).map(wrapNode);
    nodes.item = (i) => nodes[i] || null;
    return nodes;
  }

  const document = {
    getElementById(id) {
      return wrapNode(rpc('getElementById', id));
    },
    querySelector(sel) {
      return wrapNode(rpc('querySelector', sel));
    },
    querySelectorAll(sel) {
      return wrapList(rpc('querySelectorAll', sel));
    },
    createElement(tag) {
      return wrapNode(rpc('createElement', tag));
    },
    createTextNode(text) {
      return wrapNode(rpc('createTextNode', text));
    },
    addEventListener(type, fn) {
      let bag = eventTargetData.get(document);
      if (!bag) {
        bag = {};
        eventTargetData.set(document, bag);
      }
      if (!bag[type]) bag[type] = [];
      bag[type].push(fn);
      rpc('listen', '0', type);
    },
    removeEventListener(type, fn) {
      const bag = eventTargetData.get(document);
      if (!bag || !bag[type]) return;
      bag[type] = bag[type].filter((f) => f !== fn);
    },
    dispatchEvent(evt) {
      const bag = eventTargetData.get(document);
      const list = bag && bag[evt.type];
      if (!list) return true;
      list.slice().forEach((fn) => {
        try { fn.call(document, evt); } catch (e) { console.error(e); }
      });
      return true;
    },
  };

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
    while (microtaskQueue.length) {
      const jobs = microtaskQueue.slice();
      microtaskQueue = [];
      jobs.forEach((j) => {
        try { j(); } catch (e) { console.error(e); }
      });
    }
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
      try { t.fn(); } catch (e) { console.error(e); }
      if (t.kind === 'interval') {
        t.at = now + t.interval;
      } else {
        timers.delete(id);
      }
      runMicrotasks();
    });
  };

  global.__glide_flush_raf = function () {
    const jobs = rafQueue.slice();
    rafQueue = [];
    jobs.forEach((fn) => {
      try { fn(nowMs); } catch (e) { console.error(e); }
    });
    runMicrotasks();
  };

  function dispatchOnPath(path, type) {
    if (!path || !path.length) return;
    const target = wrapNode(path[0]);
    const evt = { type, target, bubbles: true, cancelable: true };
    for (const id of path) {
      const node = wrapNode(id);
      const bag = eventTargetData.get(node);
      const list = bag && bag[type];
      if (list) {
        list.slice().forEach((fn) => {
          try { fn.call(node, evt); } catch (e) { console.error(e); }
        });
      }
    }
  }

  global.__glide_dispatch_bubble = function (path, type) {
    dispatchOnPath(path, type);
  };

  global.fetch = function (url) {
    return new Promise((resolve, reject) => {
      queueMicrotask(() => {
        try {
          const body = rpc('fetch', String(url));
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
  global.window = global;
  global.document = document;
  global.console = console;

  function fireLoaded() {
    document.dispatchEvent({ type: 'DOMContentLoaded' });
    global.dispatchEvent({ type: 'load' });
  }

  global.addEventListener = document.addEventListener.bind(document);
  global.dispatchEvent = document.dispatchEvent.bind(document);

  global.__glide_fire_dom_content_loaded = fireLoaded;
  global.__glide_drain_microtasks = runMicrotasks;
})(globalThis);