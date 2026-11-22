// Domino page DOM: snapshot from Blitz, mirror mutations back, layout via RPC.
// Mutations queue until the end of the current task (script, timer, rAF, event)
// or until a layout read. Each inserted node carries its own parent, which may
// be a Blitz id or a client id from earlier in the same batch.
(function (global) {
  'use strict';

  const domino = global.__glide_domino || globalThis.__glide_domino;
  const MUTATE = { VALUE: 1, ATTR: 2, REMOVE_ATTR: 3, REMOVE: 4, MOVE: 5, INSERT: 6 };
  const XHTML = 'http://www.w3.org/1999/xhtml';

  let nextReply = 1;
  let nextCid = 1;
  let blitzToNode = new Map();
  let nodeToBlitz = new WeakMap();
  let nodeToCid = new WeakMap();
  let cidMap = new Map();
  let pendingBatch = [];
  let documentLoaded = false;
  let pageDocument = null;
  let mirroringEnabled = false;
  let openedThisTask = false;
  let writeCursor = null;
  let batchErrorLogged = false;
  let eventPathPatched = false;
  let scriptKind = '';
  let writingDocument = false;
  const reportedUnsupported = new Set();
  const skippedInserts = new Set();
  const pendingOnHandlers = [];
  const ranScripts = new WeakSet();
  const handlerCache = new Map();
  const layoutCache = new Map();

  function rpc(op, ...args) {
    const reply = nextReply++;
    const result = __glide_dom_sync(op, reply, ...args);
    if (result && result.__error) {
      throw new Error(result.__error);
    }
    return result;
  }

  // Report a page feature the runtime does not implement once per detail, so a
  // blank page still leaves a structured trace. Never let diagnostics throw.
  function diagUnsupported(feature, detail) {
    const key = feature + '\0' + detail;
    if (reportedUnsupported.has(key) || reportedUnsupported.size >= 20) return;
    reportedUnsupported.add(key);
    try {
      if (typeof __glide_diag === 'function') __glide_diag(String(feature), String(detail));
    } catch (_) { /* diagnostics must not break page scripts */ }
  }

  // Scripts inserted by a script are mirrored but never fetched or executed,
  // except inline scripts written by `document.write` (those run below).
  function reportInsertedScripts(root) {
    if (!root) return;
    if (root.nodeType !== 1 && root.nodeType !== 11) return;
    const stack = [root];
    let checked = 0;
    while (stack.length && checked < 64) {
      const node = stack.pop();
      checked += 1;
      if (node.nodeType === 1 && String(node.localName || '').toLowerCase() === 'script') {
        const attr = (name) => (node.getAttribute ? node.getAttribute(name) : null);
        const src = attr('src');
        if (src) {
          diagUnsupported('dynamic-script', String(src));
        } else if (!writingDocument) {
          const isModule = String(attr('type') || '').toLowerCase() === 'module';
          diagUnsupported(isModule ? 'module-script' : 'dynamic-script', '<inline>');
        }
        continue;
      }
      for (let c = node.firstChild; c; c = c.nextSibling) stack.push(c);
    }
  }

  // Well-known globals page apps feature-detect. A missing one makes the app take
  // a silent fallback path (no exception), which is why blank pages need this record.
  const CAPABILITY_PROBES = [
    ['requestIdleCallback', () => typeof global.requestIdleCallback === 'function'],
    ['requestAnimationFrame', () => typeof global.requestAnimationFrame === 'function'],
    ['fetch', () => typeof global.fetch === 'function'],
    ['XMLHttpRequest', () => typeof global.XMLHttpRequest === 'function'],
    ['localStorage', () => global.localStorage != null],
    ['sessionStorage', () => global.sessionStorage != null],
    ['WebAssembly', () => typeof global.WebAssembly === 'object'],
    ['IntersectionObserver', () => typeof global.IntersectionObserver === 'function'],
    ['ResizeObserver', () => typeof global.ResizeObserver === 'function'],
    ['customElements', () => global.customElements != null],
    ['Notification', () => typeof global.Notification === 'function'],
    ['WebSocket', () => typeof global.WebSocket === 'function'],
    ['Worker', () => typeof global.Worker === 'function'],
    ['EventSource', () => typeof global.EventSource === 'function'],
    ['Blob', () => typeof global.Blob === 'function'],
    ['crypto', () => global.crypto != null],
    ['matchMedia', () => typeof global.matchMedia === 'function'],
    ['CSS.supports', () => !!(global.CSS && typeof global.CSS.supports === 'function')],
    ['history.pushState', () => !!(global.history && typeof global.history.pushState === 'function')],
    ['performance.timing', () => !!(global.performance && global.performance.timing != null)],
    ['navigator.sendBeacon', () => !!(global.navigator && typeof global.navigator.sendBeacon === 'function')],
    ['navigator.languages', () => !!(global.navigator && global.navigator.languages != null)],
    ['queueMicrotask', () => typeof global.queueMicrotask === 'function'],
  ];

  function reportCapabilities() {
    const missing = [];
    for (let i = 0; i < CAPABILITY_PROBES.length; i++) {
      const [name, probe] = CAPABILITY_PROBES[i];
      let present = false;
      try { present = !!probe(); } catch (_) { present = false; }
      if (!present) missing.push(name);
    }
    if (!missing.length) return;
    try {
      if (typeof __glide_caps === 'function') __glide_caps(missing.join(','));
    } catch (_) { /* diagnostics must not break page scripts */ }
  }

  function consoleError(err) {
    try {
      let msg = err && err.stack ? String(err.stack) : String(err);
      const href = global.location && global.location.href ? String(global.location.href) : '';
      const script = global.__glideCurrentScript && global.__glideCurrentScript.src
        ? String(global.__glideCurrentScript.src)
        : '';
      if (href || script) msg += ' [page=' + href + (script ? ' script=' + script : '') + ']';
      __glide_console('error', msg);
    } catch (_) { /* console may be unavailable during early init */ }
  }

  function blitzId(node) {
    return nodeToBlitz.get(node);
  }

  function tagBlitz(node, id) {
    const n = Number(id);
    if (!Number.isFinite(n)) return;
    nodeToBlitz.set(node, n);
    blitzToNode.set(String(n), node);
  }

  function ensureCid(node) {
    let cid = nodeToCid.get(node);
    if (cid) return cid;
    cid = 'c' + (nextCid++);
    nodeToCid.set(node, cid);
    cidMap.set(cid, node);
    return cid;
  }

  function refOf(node) {
    if (!node || node.nodeType === 11) return null;
    const id = nodeToBlitz.get(node);
    if (id != null) return id;
    return nodeToCid.get(node) || null;
  }

  function followingRef(node) {
    for (let sib = node && node.nextSibling; sib; sib = sib.nextSibling) {
      const ref = refOf(sib);
      if (ref != null) return ref;
    }
    return null;
  }

  function forgetSubtree(node) {
    if (!node || node.nodeType === 11) return;
    const id = nodeToBlitz.get(node);
    if (id != null) {
      blitzToNode.delete(String(id));
      nodeToBlitz.delete(node);
    }
    const cid = nodeToCid.get(node);
    if (cid) {
      cidMap.delete(cid);
      nodeToCid.delete(node);
    }
    for (let c = node.firstChild; c; c = c.nextSibling) forgetSubtree(c);
  }

  function rooted(node) {
    let n = node;
    while (n) {
      if (n === pageDocument) return true;
      n = n.parentNode;
    }
    return false;
  }

  function remirrorSkipped() {
    if (!skippedInserts.size) return;
    const nodes = Array.from(skippedInserts);
    skippedInserts.clear();
    for (const node of nodes) {
      if (nodeToBlitz.get(node) != null) continue;
      if (!rooted(node)) continue;
      const before = pendingBatch.length;
      subtreeOps(node.parentNode, followingRef(node), node, pendingBatch);
      if (pendingBatch.length === before && nodeToBlitz.get(node) == null) {
        skippedInserts.add(node);
      }
    }
  }

  function flushMutations() {
    remirrorSkipped();
    if (!pendingBatch.length) return;
    const ops = pendingBatch;
    pendingBatch = [];
    layoutCache.clear();
    const batchCids = [];
    for (const op of ops) {
      if (op && typeof op.cid === 'string') batchCids.push(op.cid);
    }
    let ids = {};
    try {
      const resp = rpc('batchMutate', JSON.stringify(ops));
      ids = (resp && resp.ids) || {};
    } catch (err) {
      if (!batchErrorLogged) {
        batchErrorLogged = true;
        consoleError(err);
      }
      ids = {};
    }
    for (const cid of batchCids) {
      const node = cidMap.get(cid);
      cidMap.delete(cid);
      if (node) nodeToCid.delete(node);
      if (ids[cid] != null && node) {
        tagBlitz(node, ids[cid]);
        skippedInserts.delete(node);
      } else if (node && nodeToBlitz.get(node) == null && rooted(node)) {
        skippedInserts.add(node);
      }
    }
  }

  function beginTask() {
    layoutCache.clear();
  }

  function endTask() {
    layoutCache.clear();
    flushMutations();
    openedThisTask = false;
    writeCursor = null;
  }

  function cachedRpc(kind, id, extra) {
    const key = kind + '\0' + id + '\0' + (extra || '');
    if (layoutCache.has(key)) return layoutCache.get(key);
    const value = extra == null ? rpc(kind, String(id)) : rpc(kind, String(id), extra);
    layoutCache.set(key, value);
    return value;
  }

  function subtreeOps(parentNode, before, root, out) {
    if (!root || root.nodeType === 11 || root.nodeType === 9 || root.nodeType === 10) return;
    // A pending cid means this node is already queued in the batch (for example
    // remirrorSkipped walked it, then the INSERT handler saw the child).
    if (nodeToBlitz.get(root) != null || nodeToCid.has(root)) return;
    const parent = refOf(parentNode);
    if (parent == null) return;
    const cid = ensureCid(root);
    let kind = 'element';
    let name;
    let text;
    let ns = null;
    const attrs = [];
    if (root.nodeType === 3) {
      kind = 'text';
      text = root.data;
    } else if (root.nodeType === 8) {
      kind = 'comment';
      text = root.data;
    } else if (root.nodeType === 1) {
      name = root.localName || root.nodeName;
      ns = root.namespaceURI || null;
      for (const a of root.attributes || []) {
        attrs.push([a.localName || a.name, a.value, a.namespaceURI || '']);
      }
    } else {
      return;
    }
    const op = { t: 'insert', parent, cid, kind, name, text, attrs, ns };
    if (before != null) op.before = before;
    out.push(op);
    if (root.nodeType === 1) {
      for (let c = root.firstChild; c; c = c.nextSibling) {
        subtreeOps(root, null, c, out);
      }
    }
  }

  // Form controls built from the Blitz snapshot never go through domino's parser,
  // which is the only place that used to set `_form`. `input.form` stayed null,
  // so `form.q` threw.
  const FORM_ASSOCIATED = new Set([
    'button', 'fieldset', 'input', 'object', 'output', 'select', 'textarea',
  ]);

  function formOwner(el, detached) {
    if (!el || el.nodeType !== 1) return null;
    if (typeof el.hasAttribute === 'function' && el.hasAttribute('form')) {
      const id = el.getAttribute('form');
      const doc = el.ownerDocument;
      const byId = doc && id && typeof doc.getElementById === 'function'
        ? doc.getElementById(id) : null;
      return byId && byId.localName === 'form' ? byId : null;
    }
    if (detached) return null;
    for (let n = el.parentNode; n; n = n.parentNode) {
      if (n.nodeType === 1 && n.localName === 'form') return n;
    }
    return null;
  }

  function assignFormOwners(root, detached) {
    if (!root) return;
    const walk = (node) => {
      if (!node) return;
      if (node.nodeType === 1 && FORM_ASSOCIATED.has(node.localName)) {
        node._form = formOwner(node, detached);
      }
      for (let c = node.firstChild; c; c = c.nextSibling) walk(c);
    };
    walk(root);
  }

  function onMutation(rec) {
    if (!mirroringEnabled || !rec) return;
    remirrorSkipped();
    switch (rec.type) {
      case MUTATE.INSERT: {
        if (!rec.node || nodeToBlitz.get(rec.node) != null) return;
        reportInsertedScripts(rec.node);
        assignFormOwners(rec.node, false);
        subtreeOps(rec.target, followingRef(rec.node), rec.node, pendingBatch);
        break;
      }
      case MUTATE.REMOVE: {
        const node = rec.node;
        if (!node) return;
        assignFormOwners(node, true);
        const ref = refOf(node);
        forgetSubtree(node);
        if (ref != null) pendingBatch.push({ t: 'remove', id: ref });
        break;
      }
      case MUTATE.MOVE: {
        const node = rec.target;
        assignFormOwners(node, false);
        const id = refOf(node);
        const parent = refOf(node && node.parentNode);
        if (id == null || parent == null) return;
        const op = { t: 'move', id, parent };
        const before = followingRef(node);
        if (before != null) op.before = before;
        pendingBatch.push(op);
        break;
      }
      case MUTATE.ATTR: {
        const a = rec.attr;
        const name = a.localName || a.name;
        const aname = String(name || '').toLowerCase();
        if (aname === 'form') assignFormOwners(rec.target, false);
        const ref = refOf(rec.target);
        if (ref == null) return;
        pendingBatch.push({
          t: 'attr',
          id: ref,
          name,
          value: a.value,
          ns: a.namespaceURI || null,
        });
        if (aname.startsWith('on')) applyOnAttribute(rec.target, aname, a.value);
        break;
      }
      case MUTATE.REMOVE_ATTR: {
        const name = rec.attr.localName || rec.attr.name;
        const aname = String(name || '').toLowerCase();
        if (aname === 'form') assignFormOwners(rec.target, false);
        const ref = refOf(rec.target);
        if (ref == null) return;
        pendingBatch.push({ t: 'attr', id: ref, name, value: null, ns: rec.attr.namespaceURI || null });
        if (aname.startsWith('on')) {
          const type = aname.slice(2);
          const win = rec.target.ownerDocument && rec.target.ownerDocument.defaultView;
          if (rec.target.localName === 'body' && BODY_WINDOW_EVENTS.has(type) && win) {
            win._setEventHandler(type, null);
          } else if (rec.target._setEventHandler) {
            rec.target._setEventHandler(type, null);
          }
        }
        break;
      }
      case MUTATE.VALUE: {
        const ref = refOf(rec.target);
        if (ref == null) return;
        pendingBatch.push({ t: 'text', id: ref, value: rec.data });
        break;
      }
      default:
        break;
    }
  }

  function patchLayout(elProto) {
    elProto.getBoundingClientRect = function () {
      flushMutations();
      const id = blitzId(this);
      if (id == null) {
        return { x: 0, y: 0, width: 0, height: 0, top: 0, left: 0, right: 0, bottom: 0 };
      }
      return cachedRpc('boundingRect', id);
    };
    ['offsetWidth', 'offsetHeight', 'offsetTop', 'offsetLeft', 'clientWidth', 'clientHeight'].forEach((field) => {
      Object.defineProperty(elProto, field, {
        configurable: true,
        get() {
          flushMutations();
          const id = blitzId(this);
          if (id == null) return 0;
          return cachedRpc('layoutField', id, field);
        },
      });
    });
  }

  function styleFor(id) {
    const api = {
      getPropertyValue(prop) {
        flushMutations();
        return cachedRpc('computedStyle', id, String(prop));
      },
    };
    return new Proxy(api, {
      get(target, prop) {
        if (prop === 'getPropertyValue') return target.getPropertyValue;
        if (typeof prop !== 'string' || prop === 'then') return target[prop];
        flushMutations();
        return cachedRpc('computedStyle', id, prop);
      },
    });
  }

  function patchDataset(htmlProto) {
    if (!htmlProto || typeof Proxy !== 'function') return;
    if (global.DOMStringMap == null) {
      global.DOMStringMap = function DOMStringMap() {};
    }
    const maps = new WeakMap();
    const attrFor = (prop) => 'data-' + String(prop).replace(/[A-Z]/g, (ch) => '-' + ch.toLowerCase());
    const propFor = (attr) => attr.slice(5).replace(/-([a-z])/g, (_, ch) => ch.toUpperCase());
    const datasetFor = (el) => {
      const cached = maps.get(el);
      if (cached) return cached;
      const map = new Proxy(Object.create(global.DOMStringMap.prototype), {
        get(target, prop) {
          if (prop in target || typeof prop !== 'string') return target[prop];
          const value = el.getAttribute(attrFor(prop));
          return value == null ? undefined : value;
        },
        set(target, prop, value) {
          if (typeof prop === 'string') {
            el.setAttribute(attrFor(prop), String(value));
            return true;
          }
          target[prop] = value;
          return true;
        },
        deleteProperty(target, prop) {
          if (typeof prop === 'string') {
            el.removeAttribute(attrFor(prop));
            return true;
          }
          return delete target[prop];
        },
        has(target, prop) {
          return prop in target || (typeof prop === 'string' && el.hasAttribute(attrFor(prop)));
        },
        ownKeys(target) {
          const keys = [];
          const attrs = el.attributes || [];
          for (let i = 0; i < attrs.length; i += 1) {
            const name = attrs[i].name;
            if (name && name.indexOf('data-') === 0) keys.push(propFor(name));
          }
          return keys;
        },
        getOwnPropertyDescriptor(target, prop) {
          if (typeof prop === 'string' && el.hasAttribute(attrFor(prop))) {
            return {
              value: el.getAttribute(attrFor(prop)),
              writable: true,
              enumerable: true,
              configurable: true,
            };
          }
          return Reflect.getOwnPropertyDescriptor(target, prop);
        },
      });
      maps.set(el, map);
      return map;
    };
    try {
      Object.defineProperty(htmlProto, 'dataset', {
        configurable: true,
        get() { return datasetFor(this); },
      });
    } catch (_) { /* domino already exposes a dataset property */ }
  }

  function patchDomParser() {
    if (global.DOMParser) return;
    global.DOMParser = function DOMParser() {};
    global.DOMParser.prototype.parseFromString = function (str, mime) {
      const isXml = mime && String(mime).indexOf('xml') >= 0;
      if (isXml) {
        const p = new domino.impl.HTMLParser();
        p.parse(str, true);
        return p.document();
      }
      return domino.createDocument(str || '', true);
    };
  }

  const BODY_WINDOW_EVENTS = new Set([
    'afterprint', 'beforeprint', 'beforeunload', 'blur', 'error', 'focus', 'hashchange',
    'load', 'message', 'offline', 'online', 'pagehide', 'pageshow', 'popstate', 'resize',
    'scroll', 'storage', 'unload',
  ]);

  function mergeDominoWindowOntoGlobal(domWin) {
    const winProto = Object.getPrototypeOf(domWin);
    if (Object.getPrototypeOf(global) !== winProto) {
      Object.setPrototypeOf(global, winProto);
    }
    const skip = new Set([
      'document', 'self', 'window', 'parent', 'top', 'frames',
      'opener', 'frameElement', 'length', 'getComputedStyle',
      'location', 'navigator',
    ]);
    Object.getOwnPropertyNames(domWin).forEach((name) => {
      if (skip.has(name)) return;
      const desc = Object.getOwnPropertyDescriptor(domWin, name);
      if (!desc) return;
      try {
        Object.defineProperty(global, name, desc);
      } catch (_) { /* own property already frozen */ }
    });
    global.self = global;
    global.window = global;
    try {
      Object.defineProperty(global, Symbol.toStringTag, { value: 'Window', configurable: true });
    } catch (_) { /* ignore */ }
    global.toString = function () { return '[object Window]'; };
  }

  function compileInlineHandler(code) {
    const src = String(code);
    let fn = handlerCache.get(src);
    if (fn) return fn;
    // arguments[1..3] are document, form, and element. They are not named
    // parameters, so `element` / `form` / `defaultView` stay unresolved.
    try {
      fn = new Function(
        'event',
        'with (arguments[1]) with (arguments[2]) with (arguments[3]) {\n' + src + '\n}',
      );
    } catch (err) {
      fn = function () { throw err; };
    }
    handlerCache.set(src, fn);
    return fn;
  }

  function bindInlineHandler(el, inner) {
    return function (event) {
      const doc = (this && this.ownerDocument) || pageDocument || global.document;
      const form = (this && this.form) || {};
      const element = this && this.nodeType === 1 ? this : {};
      return inner.call(this, event, doc || {}, form, element);
    };
  }

  function applyOnAttribute(el, attrName, value) {
    if (!el || !value) return;
    const type = attrName.slice(2).toLowerCase();
    const handler = bindInlineHandler(el, compileInlineHandler(value));
    if (el.localName === 'body' && BODY_WINDOW_EVENTS.has(type)) {
      const win = (el.ownerDocument && el.ownerDocument.defaultView) || global;
      win._setEventHandler(type, handler);
    } else if (el._setEventHandler) {
      el._setEventHandler(type, handler);
    }
  }

  function listenerOptions(options) {
    if (options && typeof options === 'object') {
      return { capture: !!options.capture, once: !!options.once };
    }
    return { capture: !!options, once: false };
  }

  function removeListenerRecord(target, type, entry) {
    const list = target._listeners && target._listeners[type];
    if (!list) return;
    const index = list.indexOf(entry);
    if (index < 0) return;
    if (list.length === 1) target._listeners[type] = undefined;
    else list.splice(index, 1);
  }

  function callListener(entry, event) {
    const listener = entry.listener;
    if (typeof listener === 'function') return listener.call(event.currentTarget, event);
    if (listener && typeof listener.handleEvent === 'function') {
      return listener.handleEvent.call(listener, event);
    }
    return undefined;
  }

  // `once` lives on domino's listener record. The record is removed before the
  // call so removeEventListener(original) still matches, and a second add is a no-op.
  function armOnce(target, type, entry) {
    if (!entry || entry._glideOnce) return;
    entry._glideOnce = true;
    entry.once = true;
    const listener = entry.listener;
    entry.f = function (event) {
      removeListenerRecord(target, type, entry);
      if (typeof listener === 'function') return listener.call(this, event);
      if (listener && typeof listener.handleEvent === 'function') {
        return listener.handleEvent.call(listener, event);
      }
      return undefined;
    };
  }

  function invokeListenerEntry(target, type, entry, event) {
    if (entry.once || entry._glideOnce) removeListenerRecord(target, type, entry);
    if (entry.f && !entry._glideOnce) return entry.f.call(event.currentTarget, event);
    return callListener(entry, event);
  }

  function invokeWindowPhase(event, phase) {
    const Event = domino.impl.Event;
    event.eventPhase = phase;
    event.currentTarget = global;
    const type = event.type;
    if (phase !== Event.CAPTURING_PHASE && global._handlers && global._handlers[type]) {
      const handler = global._handlers[type];
      let rv;
      if (typeof handler === 'function') rv = handler.call(global, event);
      else if (handler && typeof handler.handleEvent === 'function') {
        rv = handler.handleEvent.call(handler, event);
      }
      if (rv === false) event.preventDefault();
    }
    const list = global._listeners && global._listeners[type];
    if (!list) return;
    const copy = list.slice();
    for (let i = 0; i < copy.length; i++) {
      if (event._immediatePropagationStopped) return;
      const entry = copy[i];
      if ((phase === Event.CAPTURING_PHASE && !entry.capture) ||
          (phase === Event.BUBBLING_PHASE && entry.capture)) continue;
      invokeListenerEntry(global, type, entry, event);
    }
  }

  function patchEventPath() {
    if (eventPathPatched) return;
    const nodeProto = domino.impl.Node && domino.impl.Node.prototype;
    if (!nodeProto) return;
    const etProto = Object.getPrototypeOf(nodeProto);
    const orig = etProto && etProto._dispatchEvent;
    const origAdd = etProto && etProto.addEventListener;
    const origRemove = etProto && etProto.removeEventListener;
    if (typeof orig !== 'function' || typeof origAdd !== 'function') return;
    eventPathPatched = true;
    etProto.addEventListener = function (type, listener, options) {
      const opts = listenerOptions(options);
      origAdd.call(this, type, listener, opts.capture);
      if (!opts.once || !this._listeners || !this._listeners[type]) return;
      const list = this._listeners[type];
      for (let i = list.length - 1; i >= 0; i--) {
        const entry = list[i];
        if (entry.listener === listener && entry.capture === opts.capture) {
          armOnce(this, type, entry);
          break;
        }
      }
    };
    if (typeof origRemove === 'function') {
      etProto.removeEventListener = function (type, listener, options) {
        const opts = listenerOptions(options);
        origRemove.call(this, type, listener, opts.capture);
      };
    }
    etProto._dispatchEvent = function (event, trusted) {
      const Event = domino.impl.Event;
      const targetIsWindow = this === global;
      const doc = this && this.nodeType === 9 ? this : this && this.ownerDocument;
      const inPage = targetIsWindow || (doc && doc === pageDocument);
      if (!inPage || !event) return orig.call(this, event, trusted);
      // Same check domino makes. A re-entrant dispatch of this event throws.
      if (!event._initialized || event._dispatching) return orig.call(this, event, trusted);
      if (typeof trusted !== 'boolean') trusted = false;
      if (!targetIsWindow) {
        event.isTrusted = trusted;
        event.target = this;
        event._dispatching = true;
        invokeWindowPhase(event, Event.CAPTURING_PHASE);
        if (event._propagationStopped) {
          event._dispatching = false;
          event.eventPhase = Event.AT_TARGET;
          event.currentTarget = null;
          return !event.defaultPrevented;
        }
        // orig sets `_dispatching` itself. Clear the flag only for that check.
        event._dispatching = false;
      }
      orig.call(this, event, trusted);
      let prevented = event.defaultPrevented;
      if (!targetIsWindow && event.bubbles && !event._propagationStopped) {
        event._dispatching = true;
        invokeWindowPhase(event, Event.BUBBLING_PHASE);
        event._dispatching = false;
        event.eventPhase = Event.AT_TARGET;
        event.currentTarget = null;
        prevented = event.defaultPrevented;
      }
      return !prevented;
    };
  }

  function htmlFragment(html) {
    const tpl = pageDocument.createElement('template');
    tpl.innerHTML = html == null ? '' : String(html);
    return tpl.content;
  }

  function scriptsIn(node, out) {
    if (!node) return;
    if (node.nodeType === 1 && node.localName === 'script') out.push(node);
    for (let c = node.firstChild; c; c = c.nextSibling) scriptsIn(c, out);
  }

  function runScriptElement(scriptEl) {
    if (!scriptEl || scriptEl.nodeType !== 1 || scriptEl.localName !== 'script') return;
    if (ranScripts.has(scriptEl)) return;
    ranScripts.add(scriptEl);
    if (scriptEl.getAttribute && scriptEl.getAttribute('src')) return;
    const code = scriptEl.textContent || '';
    if (!code) return;
    const prevScript = global.__glideCurrentScript;
    const prevCursor = writeCursor;
    global.__glideCurrentScript = scriptEl;
    writeCursor = scriptEl.nextSibling;
    try {
      (0, eval)(code);
    } catch (err) {
      consoleError(err);
    } finally {
      global.__glideCurrentScript = prevScript;
      writeCursor = prevCursor;
    }
  }

  function runScriptsIn(root) {
    const scripts = [];
    scriptsIn(root, scripts);
    for (let i = 0; i < scripts.length; i++) runScriptElement(scripts[i]);
  }

  function insertAfterScript(html) {
    const script = global.__glideCurrentScript;
    const parent = script && script.parentNode;
    if (!parent) {
      const host = (pageDocument && (pageDocument.body || pageDocument.documentElement)) || pageDocument;
      if (!host) return;
      const frag = htmlFragment(html);
      const scripts = [];
      scriptsIn(frag, scripts);
      host.appendChild(frag);
      for (let i = 0; i < scripts.length; i++) runScriptElement(scripts[i]);
      return;
    }
    if (writeCursor == null || writeCursor.parentNode !== parent) {
      writeCursor = script.nextSibling;
    }
    const cursor = writeCursor;
    const frag = htmlFragment(html);
    const scripts = [];
    scriptsIn(frag, scripts);
    parent.insertBefore(frag, cursor);
    for (let i = 0; i < scripts.length; i++) runScriptElement(scripts[i]);
  }

  function writeAfterLoad(html) {
    if (!pageDocument) return;
    if (!openedThisTask) {
      while (pageDocument.firstChild) pageDocument.removeChild(pageDocument.firstChild);
      const parsed = domino.createDocument(html || '', true);
      const kids = [];
      for (let c = parsed.firstChild; c; c = c.nextSibling) kids.push(c);
      for (let i = 0; i < kids.length; i++) pageDocument.appendChild(kids[i]);
      openedThisTask = true;
      runScriptsIn(pageDocument);
      return;
    }
    const host = pageDocument.body || pageDocument.documentElement || pageDocument;
    const frag = htmlFragment(html);
    const scripts = [];
    scriptsIn(frag, scripts);
    host.appendChild(frag);
    for (let i = 0; i < scripts.length; i++) runScriptElement(scripts[i]);
  }

  function warnIgnoredWrite() {
    try {
      __glide_console('warn', 'document.write ignored: insertion point is undefined');
    } catch (_) { /* console may be unavailable */ }
  }

  function doWrite(args, newline) {
    let html = Array.prototype.join.call(args, '');
    if (newline) html += '\n';
    if (scriptKind === 'async' || scriptKind === 'defer') {
      warnIgnoredWrite();
      return;
    }
    const script = global.__glideCurrentScript;
    if (!documentLoaded) {
      if (scriptKind === 'classic' && script && script.parentNode) {
        writingDocument = true;
        try { insertAfterScript(html); } finally { writingDocument = false; }
        return;
      }
      warnIgnoredWrite();
      return;
    }
    writingDocument = true;
    try { writeAfterLoad(html); } finally { writingDocument = false; }
  }

  function installDocumentWrite(doc) {
    // Prototype `write` is non-writable; an own property shadows it.
    Object.defineProperty(doc, 'write', {
      configurable: true,
      writable: true,
      value: function write() { doWrite(arguments, false); },
    });
    Object.defineProperty(doc, 'writeln', {
      configurable: true,
      writable: true,
      value: function writeln() { doWrite(arguments, true); },
    });
  }

  function buildFromSnapshot(tree, doc) {
    const arr = tree;
    const id = arr[0];
    const type = arr[1];
    const name = arr[2];
    const attrs = arr[3];
    const children = arr[4] || [];
    const ns = arr[5];
    let node;
    if (type === 9) {
      node = doc;
    } else if (type === 1) {
      if (ns && ns !== XHTML) {
        try { node = doc.createElementNS(ns, name); } catch (_) { node = doc.createElement(name); }
      } else {
        node = doc.createElement(name);
      }
      if (attrs) {
        for (const a of attrs) {
          const n = a[0];
          const v = a[1];
          if (n === 'id' || n === 'class' || !a[2] || String(a[2]).indexOf('html') >= 0 || a[2] === XHTML) {
            node.setAttribute(n, v);
          } else {
            try { node.setAttributeNS(a[2], n, v); } catch (_) { node.setAttribute(n, v); }
          }
          if (/^on/i.test(n)) pendingOnHandlers.push([node, n.toLowerCase(), v]);
        }
      }
    } else if (type === 3) {
      node = doc.createTextNode(name);
    } else if (type === 8) {
      node = doc.createComment(name);
    } else {
      node = doc.createDocumentFragment();
    }
    if (type !== 9) tagBlitz(node, id);
    for (const child of children) {
      const built = buildFromSnapshot(child, doc);
      if (built && built !== doc) node.appendChild(built);
    }
    return node;
  }

  function installGlobals(pageUrl) {
    global.document = pageDocument;
    if (global.navigator == null) global.navigator = { userAgent: 'Glide' };
    const impl = domino.impl;
    Object.keys(impl).forEach((k) => {
      if (k !== 'Window' && typeof impl[k] === 'function') global[k] = impl[k];
    });
    global.Node = impl.Node;
    global.Element = impl.Element;
    global.HTMLElement = impl.HTMLElement;
    global.Event = impl.Event;
    global.CustomEvent = impl.CustomEvent;
    global.MouseEvent = impl.MouseEvent;
    patchLayout(global.Element.prototype);
    patchDataset(global.HTMLElement.prototype);
    const gcsImpl = function (el) {
      flushMutations();
      const id = blitzId(el);
      if (id == null) return (el && el.style) || {};
      return styleFor(id);
    };
    try {
      Object.defineProperty(global, 'getComputedStyle', {
        value: gcsImpl,
        writable: true,
        configurable: true,
      });
    } catch (_) {
      global.getComputedStyle = gcsImpl;
    }
    patchDomParser();
    global.location = locationFrom(pageUrl || 'about:blank');
    try {
      Object.defineProperty(global.document, 'URL', { configurable: true, get: () => pageUrl });
      Object.defineProperty(global.document, 'documentURI', { configurable: true, get: () => pageUrl });
    } catch (_) { /* domino defines these as readonly */ }
  }

  function locationFrom(pageUrl) {
    let parsed = null;
    try { parsed = __glide_url_parse(String(pageUrl || 'about:blank')); } catch (_) { parsed = null; }
    parsed = parsed || {};
    return {
      href: parsed.href || pageUrl || 'about:blank',
      protocol: parsed.protocol || '',
      host: parsed.host || '',
      hostname: parsed.hostname || '',
      port: parsed.port || '',
      pathname: parsed.pathname || '',
      search: parsed.search || '',
      hash: parsed.hash || '',
      origin: parsed.origin || 'null',
      assign(u) { rpc('navigate', String(u)); },
      replace(u) { rpc('navigate', String(u)); },
      reload() {},
      toString() { return this.href; },
    };
  }

  global.__glide_init_page = function (snapshotJson, pageUrl) {
    pendingOnHandlers.length = 0;
    blitzToNode = new Map();
    nodeToBlitz = new WeakMap();
    nodeToCid = new WeakMap();
    cidMap = new Map();
    pendingBatch = [];
    documentLoaded = false;
    openedThisTask = false;
    writeCursor = null;
    mirroringEnabled = false;
    scriptKind = '';
    writingDocument = false;
    reportedUnsupported.clear();
    skippedInserts.clear();
    layoutCache.clear();
    const tree = typeof snapshotJson === 'string' ? JSON.parse(snapshotJson) : snapshotJson;
    const domWin = domino.createWindow('');
    pageDocument = domWin.document;
    while (pageDocument.lastChild) pageDocument.removeChild(pageDocument.lastChild);
    if (tree && tree.length) tagBlitz(pageDocument, tree[0]);
    if (tree && tree[6] && tree[6][0]) {
      try {
        const dt = pageDocument.implementation.createDocumentType(
          String(tree[6][0]),
          String(tree[6][1] || ''),
          String(tree[6][2] || ''),
        );
        pageDocument.insertBefore(dt, pageDocument.firstChild);
      } catch (err) {
        consoleError(err);
      }
    }
    const kids = (tree && tree[4]) || [];
    for (const child of kids) {
      const built = buildFromSnapshot(child, pageDocument);
      if (built && built !== pageDocument) pageDocument.appendChild(built);
    }
    assignFormOwners(pageDocument, false);
    mergeDominoWindowOntoGlobal(domWin);
    pageDocument._setMutationHandler(onMutation);
    Object.defineProperty(pageDocument, 'defaultView', {
      configurable: true,
      get: () => global,
    });
    installGlobals(pageUrl || 'about:blank');
    installDocumentWrite(pageDocument);
    patchEventPath();
    for (const [node, name, val] of pendingOnHandlers) applyOnAttribute(node, name, val);
    pendingOnHandlers.length = 0;
    mirroringEnabled = true;
    ['addEventListener', 'removeEventListener', 'dispatchEvent'].forEach((name) => {
      const fn = global[name];
      if (typeof fn === 'function') {
        try {
          Object.defineProperty(global, name, {
            value: fn.bind(global),
            writable: true,
            configurable: true,
          });
        } catch (_) { /* ignore */ }
      }
    });
    try {
      Object.defineProperty(pageDocument, 'currentScript', {
        configurable: true,
        get: () => global.__glideCurrentScript || null,
      });
    } catch (_) { /* ignore */ }
    pageDocument.readyState = 'loading';
    reportCapabilities();
  };

  global.__glide_set_current_script = function (id) {
    global.__glideCurrentScript = id ? blitzToNode.get(String(id)) || null : null;
    writeCursor = null;
  };

  global.__glide_set_script_kind = function (kind) {
    scriptKind = kind ? String(kind) : '';
  };

  global.__glide_begin_task = beginTask;

  global.__glide_mark_document_loaded = function () {
    documentLoaded = true;
  };

  global.__glide_fire_page_load = function () {
    beginTask();
    flushMutations();
    if (pageDocument) {
      pageDocument.readyState = 'interactive';
      pageDocument.dispatchEvent(new domino.impl.Event('DOMContentLoaded', { bubbles: true }));
      pageDocument.readyState = 'complete';
    }
    global.dispatchEvent(new domino.impl.Event('load', { bubbles: false }));
    flushMutations();
  };

  global.__glide_dispatch_bubble = function (path, type) {
    beginTask();
    flushMutations();
    if (!path || !path.length) return;
    const target = blitzToNode.get(String(path[0]));
    if (!target || !pageDocument) return;
    const evt = new domino.impl.Event(type, { bubbles: true, cancelable: true });
    target.dispatchEvent(evt);
    flushMutations();
  };

  global.__glide_flush_dom = endTask;
  if (global.__glide_debug_hooks) {
    global.__glide_test_skip_node = function (node) {
      if (!node) return;
      const id = nodeToBlitz.get(node);
      if (id != null) {
        blitzToNode.delete(String(id));
        nodeToBlitz.delete(node);
      }
      const cid = nodeToCid.get(node);
      if (cid) {
        cidMap.delete(cid);
        nodeToCid.delete(node);
      }
      skippedInserts.add(node);
    };
  }
  global.__glide_wrap_node = function (id) {
    return blitzToNode.get(String(id)) || null;
  };

  function installUrlGlobal() {
    global.URL = function URL(href, base) {
      const parsed = __glide_url_parse(String(href), base != null ? String(base) : undefined);
      if (!parsed) throw new TypeError('Invalid URL');
      Object.assign(this, parsed);
    };
    global.URL.prototype.toString = function () { return this.href; };
    Object.defineProperty(global.URL.prototype, 'searchParams', {
      configurable: true,
      get() { return new global.URLSearchParams(this.search || ''); },
    });
    global.URLSearchParams = class URLSearchParams {
      constructor(init) {
        this._pairs = [];
        if (typeof init === 'string') {
          const q = init.startsWith('?') ? init.slice(1) : init;
          q.split('&').forEach((part) => {
            if (!part) return;
            const eq = part.indexOf('=');
            const k = eq < 0 ? part : part.slice(0, eq);
            const v = eq < 0 ? '' : part.slice(eq + 1);
            this.append(decodeURIComponent(k.replace(/\+/g, ' ')), decodeURIComponent((v || '').replace(/\+/g, ' ')));
          });
        }
      }
      append(k, v) { this._pairs.push([String(k), String(v)]); }
      get(k) {
        const key = String(k);
        for (let i = 0; i < this._pairs.length; i++) {
          if (this._pairs[i][0] === key) return this._pairs[i][1];
        }
        return null;
      }
      toString() {
        return this._pairs.map(([k, v]) => encodeURIComponent(k) + '=' + encodeURIComponent(v)).join('&');
      }
    };
  }
  installUrlGlobal();
})(globalThis);
