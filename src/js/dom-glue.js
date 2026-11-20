// Domino page DOM: snapshot from Blitz, mirror mutations back, layout via RPC.
(function (global) {
  'use strict';

  const domino = global.__glide_domino || globalThis.__glide_domino;
  const MUTATE = { VALUE: 1, ATTR: 2, REMOVE_ATTR: 3, REMOVE: 4, MOVE: 5, INSERT: 6 };

  let nextReply = 1;
  let nextCid = 1;
  let blitzToNode = new Map();
  let nodeToBlitz = new WeakMap();
  let pendingBatch = [];
  let documentLoaded = false;
  let pageWindow = null;
  let pageDocument = null;
  const pendingOnHandlers = [];

  function rpc(op, ...args) {
    const reply = nextReply++;
    const result = __glide_dom_sync(op, reply, ...args);
    if (result && result.__error) {
      throw new Error(result.__error);
    }
    return result;
  }

  function blitzId(node) {
    return nodeToBlitz.get(node);
  }

  function tagBlitz(node, id) {
    nodeToBlitz.set(node, id);
    blitzToNode.set(String(id), node);
    try {
      Object.defineProperty(node, '__glideBlitzId', { value: id, configurable: true });
    } catch (_) { /* noop */ }
  }

  function ensureCid(node) {
    if (node.__glideCid) return node.__glideCid;
    const cid = 'c' + (nextCid++);
    node.__glideCid = cid;
    return cid;
  }

  function scheduleFlush() {
    flushMutations();
  }

  function flushMutations() {
    if (!pendingBatch.length) return;
    const ops = pendingBatch;
    pendingBatch = [];
    const resp = rpc('batchMutate', JSON.stringify(ops));
    const ids = resp && resp.ids;
    if (!ids) return;
    for (const [cid, id] of Object.entries(ids)) {
      const node = findByCid(cid);
      if (node) tagBlitz(node, Number(id));
    }
  }

  const cidMap = new Map();
  function findByCid(cid) {
    return cidMap.get(cid) || null;
  }

  function rememberCid(node, cid) {
    cidMap.set(cid, node);
    node.__glideCid = cid;
  }

  function buildFromSnapshot(tree, doc) {
    const arr = tree;
    const id = arr[0];
    const type = arr[1];
    const name = arr[2];
    const attrs = arr[3];
    const children = arr[4] || [];
    let node;
    if (type === 9) {
      node = doc;
    } else if (type === 1) {
      node = doc.createElement(name);
      if (attrs) {
        for (const a of attrs) {
          const n = a[0];
          const v = a[1];
          if (n === 'id' || n === 'class' || !a[2] || a[2].indexOf('html') >= 0) {
            node.setAttribute(n, v);
          } else {
            try { node.setAttributeNS(a[2], n, v); } catch (_) { node.setAttribute(n, v); }
          }
          if (/^on/i.test(n)) {
            pendingOnHandlers.push([node, n.toLowerCase(), v]);
          }
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

  function subtreeOps(parentBlitz, beforeBlitz, root, out) {
    const cid = ensureCid(root);
    rememberCid(root, cid);
    let kind = 'element';
    let name;
    let text;
    const attrs = [];
    if (root.nodeType === 3) {
      kind = 'text';
      text = root.data;
    } else if (root.nodeType === 8) {
      kind = 'comment';
      text = root.data;
    } else if (root.nodeType === 1) {
      name = root.localName;
      for (const a of root.attributes || []) {
        attrs.push([a.name, a.value]);
      }
    } else {
      return;
    }
    out.push({
      t: 'insert',
      parent: parentBlitz,
      before: beforeBlitz || null,
      cid,
      kind,
      name,
      text,
      attrs,
    });
    if (root.nodeType === 1) {
      let beforeChild = null;
      for (let c = root.firstChild; c; c = c.nextSibling) {
        subtreeOps(parentBlitz, beforeChild, c, out);
        beforeChild = null;
      }
    }
  }

  function onMutation(rec) {
    switch (rec.type) {
      case MUTATE.INSERT: {
        const parent = blitzId(rec.target);
        if (parent == null) return;
        const before = rec.node.nextSibling ? blitzId(rec.node.nextSibling) : null;
        if (blitzId(rec.node) != null) return;
        subtreeOps(parent, before, rec.node, pendingBatch);
        scheduleFlush();
        break;
      }
      case MUTATE.REMOVE: {
        const id = blitzId(rec.node);
        if (id == null) return;
        pendingBatch.push({ t: 'remove', id });
        blitzToNode.delete(String(id));
        nodeToBlitz.delete(rec.node);
        scheduleFlush();
        break;
      }
      case MUTATE.MOVE: {
        const id = blitzId(rec.target);
        const parent = blitzId(rec.target.parentNode);
        if (id == null || parent == null) return;
        const before = rec.target.nextSibling ? blitzId(rec.target.nextSibling) : null;
        pendingBatch.push({ t: 'move', id, parent, before });
        scheduleFlush();
        break;
      }
      case MUTATE.ATTR: {
        const id = blitzId(rec.target);
        if (id == null) return;
        const a = rec.attr;
        pendingBatch.push({ t: 'attr', id, name: a.localName, value: a.value });
        scheduleFlush();
        const aname = String(a.localName || a.name || '').toLowerCase();
        if (aname.startsWith('on')) {
          applyOnAttribute(rec.target, aname, a.value);
        }
        break;
      }
      case MUTATE.REMOVE_ATTR: {
        const id = blitzId(rec.target);
        if (id == null) return;
        pendingBatch.push({ t: 'attr', id, name: rec.attr.localName, value: null });
        scheduleFlush();
        const aname = String(rec.attr.localName || rec.attr.name || '').toLowerCase();
        if (aname.startsWith('on')) {
          const type = aname.slice(2);
          const win = rec.target.ownerDocument && rec.target.ownerDocument.defaultView;
          if (rec.target.localName === 'body' && BODY_WINDOW_EVENTS.has(type) && win) {
            win._setEventHandler(type, null);
          } else {
            rec.target._setEventHandler(type, null);
          }
        }
        break;
      }
      case MUTATE.VALUE: {
        const id = blitzId(rec.target);
        if (id == null) return;
        pendingBatch.push({ t: 'text', id, value: rec.data });
        scheduleFlush();
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
      if (id == null) return new domino.impl.Element().getBoundingClientRect.call(this);
      return rpc('boundingRect', String(id));
    };
    ['offsetWidth', 'offsetHeight', 'offsetTop', 'offsetLeft', 'clientWidth', 'clientHeight'].forEach((f) => {
      Object.defineProperty(elProto, f, {
        get: function () {
          flushMutations();
          const id = blitzId(this);
          if (id == null) return 0;
          return rpc('layoutField', String(id), f);
        },
      });
    });
  }

  function patchDomParser() {
    if (global.DOMParser) return;
    global.DOMParser = function DOMParser() {};
    global.DOMParser.prototype.parseFromString = function (str, mime) {
      const isXml = mime && mime.indexOf('xml') >= 0;
      const d = isXml
        ? domino.createDOMImplementation().createDocument(null, 'root', null)
        : domino.createDocument(str || '', true);
      if (isXml) {
        const p = new domino.impl.HTMLParser();
        p.parse(str, true);
        return p.document();
      }
      return d;
    };
  }

  const BODY_WINDOW_EVENTS = new Set([
    'afterprint', 'beforeprint', 'beforeunload', 'blur', 'error', 'focus', 'hashchange',
    'load', 'message', 'offline', 'online', 'pagehide', 'pageshow', 'popstate', 'resize',
    'scroll', 'storage', 'unload',
  ]);

  function defineRo(w, name, get) {
    try {
      Object.defineProperty(w, name, { get, configurable: true });
    } catch (_) { /* noop */ }
  }

  function installBrowsingContext(win) {
    defineRo(global, 'self', () => win);
    defineRo(global, 'window', () => win);
    defineRo(global, 'parent', () => win);
    defineRo(global, 'top', () => win);
    defineRo(global, 'frames', () => win);
    defineRo(win, 'parent', () => win);
    defineRo(win, 'top', () => win);
    defineRo(win, 'frames', () => win);
    defineRo(win, 'self', () => win);
    defineRo(win, 'window', () => win);
    defineRo(win, 'opener', () => null);
    defineRo(win, 'frameElement', () => null);
    defineRo(win, 'length', () => 0);
  }

  function compileInlineHandler(code, doc, el) {
    const defaultView = doc.defaultView || pageWindow || global;
    const form = (el.form) || Object.create(null);
    const element = el;
    try {
      const fn = new Function(
        'defaultView', 'document', 'form', 'element', 'event',
        'with(defaultView)with(document)with(form)with(element){' + String(code) + '};',
      );
      return fn.bind(el, defaultView, doc, form, element);
    } catch (err) {
      return function () { throw err; };
    }
  }

  function applyOnAttribute(el, attrName, value) {
    if (!value) return;
    const type = attrName.slice(2).toLowerCase();
    const handler = compileInlineHandler(value, el.ownerDocument, el);
    if (el.localName === 'body' && BODY_WINDOW_EVENTS.has(type)) {
      const win = el.ownerDocument.defaultView || pageWindow || global;
      win._setEventHandler(type, handler);
    } else {
      el._setEventHandler(type, handler);
    }
  }

  function activateAttributeHandlers(root) {
    if (!root) return;
    if (root.nodeType === 1) {
      for (const attr of root.attributes || []) {
        if (/^on/i.test(attr.name) && attr.value) {
          applyOnAttribute(root, attr.name.toLowerCase(), attr.value);
        }
      }
      for (let c = root.firstChild; c; c = c.nextSibling) {
        activateAttributeHandlers(c);
      }
    } else if (root.nodeType === 9) {
      for (let c = root.firstChild; c; c = c.nextSibling) {
        activateAttributeHandlers(c);
      }
    }
  }

  function installGlobals(win, pageUrl) {
    installBrowsingContext(win);
    global.document = win.document;
    global.navigator = win.navigator;
    const impl = domino.impl;
    Object.keys(impl).forEach((k) => {
      if (k !== 'Window' && typeof impl[k] === 'function') {
        global[k] = impl[k];
      }
    });
    global.Node = impl.Node;
    global.Element = impl.Element;
    global.HTMLElement = impl.HTMLElement;
    global.Event = impl.Event;
    global.CustomEvent = impl.CustomEvent;
    global.MouseEvent = impl.MouseEvent;
    patchLayout(global.Element.prototype);
    const gcs = win.getComputedStyle.bind(win);
    try {
      Object.defineProperty(win, 'getComputedStyle', {
        value: function (el) {
          flushMutations();
          const id = blitzId(el);
          if (id == null) return gcs(el);
          return {
            getPropertyValue(prop) {
              return rpc('computedStyle', String(id), prop);
            },
          };
        },
      });
    } catch (_) {
      win.getComputedStyle = function (el) {
      flushMutations();
      const id = blitzId(el);
      if (id == null) return gcs(el);
      return {
        getPropertyValue(prop) {
          return rpc('computedStyle', String(id), prop);
        },
      };
      };
    }
    global.getComputedStyle = win.getComputedStyle;
    patchDomParser();
    global.location = {
      href: pageUrl || 'about:blank',
      assign(u) { rpc('navigate', String(u)); },
      replace(u) { rpc('navigate', String(u)); },
      reload() {},
    };
    try {
      Object.defineProperty(global.document, 'URL', { get: () => pageUrl });
      Object.defineProperty(global.document, 'documentURI', { get: () => pageUrl });
    } catch (_) { /* readonly in domino */ }
  }

  global.__glide_init_page = function (snapshotJson, pageUrl) {
    pendingOnHandlers.length = 0;
    const tree = typeof snapshotJson === 'string' ? JSON.parse(snapshotJson) : snapshotJson;
    pageWindow = domino.createWindow('');
    pageDocument = pageWindow.document;
    while (pageDocument.lastChild) {
      pageDocument.removeChild(pageDocument.lastChild);
    }
    const kids = tree[4] || [];
    for (const child of kids) {
      pageDocument.appendChild(buildFromSnapshot(child, pageDocument));
    }
    pageDocument._setMutationHandler(onMutation);
    Object.defineProperty(pageDocument, 'defaultView', {
      get: () => pageWindow,
      configurable: true,
    });
    installGlobals(pageWindow, pageUrl || 'about:blank');
    activateAttributeHandlers(pageDocument);
    for (const [node, name, val] of pendingOnHandlers) {
      applyOnAttribute(node, name, val);
    }
    pendingOnHandlers.length = 0;
    global.addEventListener = pageDocument.addEventListener.bind(pageDocument);
    global.removeEventListener = pageDocument.removeEventListener.bind(pageDocument);
    global.dispatchEvent = pageDocument.dispatchEvent.bind(pageDocument);
    Object.defineProperty(pageDocument, 'currentScript', {
      get: () => global.__glideCurrentScript || null,
    });
  };

  global.__glide_set_current_script = function (id) {
    global.__glideCurrentScript = id ? blitzToNode.get(String(id)) || null : null;
  };

  global.__glide_mark_document_loaded = function () {
    documentLoaded = true;
  };

  global.__glide_fire_page_load = function () {
    flushMutations();
  pageDocument.dispatchEvent(new domino.impl.Event('DOMContentLoaded', { bubbles: true }));
    const win = pageWindow || global;
    win.dispatchEvent(new domino.impl.Event('load', { bubbles: false }));
    flushMutations();
  };

  global.__glide_dispatch_bubble = function (path, type) {
    flushMutations();
    if (!path || !path.length) return;
    const target = blitzToNode.get(String(path[0]));
    if (!target || !pageDocument) return;
    const evt = new domino.impl.Event(type, { bubbles: true, cancelable: true });
    Object.defineProperty(evt, 'target', { value: target });
    target.dispatchEvent(evt);
    flushMutations();
  };

  global.__glide_flush_dom = flushMutations;

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
    global.URLSearchParams = class URLSearchParams {
      constructor(init) {
        this._m = new Map();
        if (typeof init === 'string') {
          const q = init.startsWith('?') ? init.slice(1) : init;
          q.split('&').forEach((p) => {
            if (!p) return;
            const [k, v] = p.split('=');
            this.append(decodeURIComponent(k), decodeURIComponent(v || ''));
          });
        }
      }
      append(k, v) { this._m.set(k, String(v)); }
      get(k) { return this._m.has(k) ? this._m.get(k) : null; }
      toString() {
        return Array.from(this._m.entries()).map(([k, v]) => `${encodeURIComponent(k)}=${encodeURIComponent(v)}`).join('&');
      }
    };
  }
  installUrlGlobal();
})(globalThis);