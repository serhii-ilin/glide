// Minimal DOM / event loop shims installed before page scripts run.
(function (global) {
  'use strict';

  let nextReply = 1;
  const pendingResolvers = new Map();
  const eventTargetData = new WeakMap();

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
    return {
      __id: id,
      appendChild(child) {
        rpc('appendChild', this.__id, child.__id);
        return child;
      },
      removeChild(child) {
        rpc('removeChild', this.__id, child.__id);
        return child;
      },
      insertBefore(child, before) {
        rpc('insertBefore', this.__id, child.__id, before ? before.__id : 0);
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
        const self = this;
        return {
          add(t) { rpc('classAdd', self.__id, t); },
          remove(t) { rpc('classRemove', self.__id, t); },
          contains(t) { return !!rpc('classContains', self.__id, t); },
        };
      },
      get style() {
        const self = this;
        return new Proxy({}, {
          set(_, prop, value) {
            rpc('styleSet', self.__id, String(prop), String(value));
            return true;
          },
        });
      },
      addEventListener(type, fn) {
        let bag = eventTargetData.get(this);
        if (!bag) {
          bag = {};
          eventTargetData.set(this, bag);
        }
        if (!bag[type]) bag[type] = [];
        bag[type].push(fn);
        rpc('listen', this.__id, type);
      },
      removeEventListener(type, fn) {
        const bag = eventTargetData.get(this);
        if (!bag || !bag[type]) return;
        bag[type] = bag[type].filter((f) => f !== fn);
      },
      dispatchEvent(evt) {
        const bag = eventTargetData.get(this);
        const list = bag && bag[evt.type];
        if (!list) return true;
        list.slice().forEach((fn) => {
          try { fn.call(this, evt); } catch (e) { console.error(e); }
        });
        return true;
      },
      click() {
        this.dispatchEvent({ type: 'click', target: this });
      },
    };
  }

  function wrapList(ids) {
    const nodes = ids.map(wrapNode);
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
      return wrapList(rpc('querySelectorAll', sel) || []);
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
      rpc('listen', 0, type);
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

  const timers = [];
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
    const id = timers.length;
    timers.push({ id, fn, at: Date.now() + (ms || 0) });
    return id;
  };
  global.clearTimeout = function (id) {
    timers[id] = null;
  };
  global.setInterval = function (fn, ms) {
    return global.setTimeout(function tick() {
      try { fn(); } catch (e) { console.error(e); }
      global.setTimeout(tick, ms || 0);
    }, ms || 0);
  };
  global.clearInterval = global.clearTimeout;

  global.requestAnimationFrame = function (fn) {
    const id = rafQueue.length;
    rafQueue.push(fn);
    return id;
  };

  global.__glide_pump_timers = function (now) {
    timers.forEach((t, i) => {
      if (t && t.at <= now) {
        timers[i] = null;
        try { t.fn(); } catch (e) { console.error(e); }
        runMicrotasks();
      }
    });
  };

  global.__glide_flush_raf = function () {
    const jobs = rafQueue.slice();
    rafQueue = [];
    jobs.forEach((fn) => {
      try { fn(performance.now()); } catch (e) { console.error(e); }
    });
    runMicrotasks();
  };

  global.__glide_dispatch_event = function (nodeId, type) {
    const node = wrapNode(nodeId);
    if (!node) return;
    node.dispatchEvent({ type, target: node });
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
  global.performance = { now: () => Date.now() };
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
})(globalThis);