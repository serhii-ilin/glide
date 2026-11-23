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
  const CE_RESERVED = new Set([
    'annotation-xml', 'color-profile', 'font-face', 'font-face-src', 'font-face-uri',
    'font-face-format', 'font-face-name', 'missing-glyph',
  ]);
  const ceRegistry = new Map();
  const cePending = new Map();
  const ceConstructors = new Set();
  const ceUpgraded = new WeakSet();
  const ceFailed = new WeakSet();
  const ceReactionQueue = [];
  let ceQueueHead = 0;
  let ceDraining = false;
  let ceUpgradeTarget = null;
  let ceHTMLElementPatched = false;

  // Potential custom element name (HTML Living Standard, valid element local name).
  const PCEN_CONTINUATION =
    /^[a-z0-9._\-\u00B7\u00C0-\u00D6\u00D8-\u00F6\u00F8-\u037D\u037F-\u1FFF\u200C-\u200D\u203F-\u2040\u2070-\u218F\u2C00-\u2FEF\u3001-\uD7FF\uF900-\uFDCF\uFDF0-\uFFFD][a-z0-9._\-\u00B7\u00C0-\u00D6\u00D8-\u00F6\u00F8-\u037D\u037F-\u1FFF\u200C-\u200D\u203F-\u2040\u2070-\u218F\u2C00-\u2FEF\u3001-\uD7FF\uF900-\uFDCF\uFDF0-\uFFFD]*$/u;

  function validCustomElementName(name) {
    const n = String(name);
    const lower = n.toLowerCase();
    if (!n.includes('-')) return false;
    if (/[A-Z]/.test(n)) return false;
    if (CE_RESERVED.has(lower)) return false;
    const i = n.indexOf('-');
    if (i <= 0) return false;
    const before = n.slice(0, i);
    const after = n.slice(i + 1);
    if (!/^[a-z]/.test(before)) return false;
    if (!PCEN_CONTINUATION.test(after)) return false;
    return PCEN_CONTINUATION.test(before);
  }

  function resetCustomElementState() {
    ceRegistry.clear();
    cePending.clear();
    ceConstructors.clear();
    ceReactionQueue.length = 0;
    ceQueueHead = 0;
    ceDraining = false;
    ceUpgradeTarget = null;
  }

  function setElementDefinedState(el, defined) {
    const ref = refOf(el);
    if (ref == null) return;
    pendingBatch.push({ t: 'state', id: ref, defined: !!defined });
  }

  function observedAttributesOf(ctor) {
    if (typeof ctor.observedAttributes === 'function') {
      try {
        const v = ctor.observedAttributes();
        if (Array.isArray(v)) return v.map(String);
      } catch (err) {
        consoleError(err);
      }
    }
    if (Array.isArray(ctor.observedAttributes)) {
      return ctor.observedAttributes.map(String);
    }
    return [];
  }

  function enqueueCeReaction(el, callbackName, args) {
    if (!el) return;
    ceReactionQueue.push({ el, callbackName, args });
  }

  function invokeCeCallback(el, ctor, callbackName, args) {
    const fn = ctor && ctor.prototype && ctor.prototype[callbackName];
    if (typeof fn !== 'function') return;
    try {
      if (args && args.length) fn.apply(el, args);
      else fn.call(el);
    } catch (err) {
      consoleError(err);
    }
  }

  function drainCeReactions() {
    if (ceDraining) return;
    ceDraining = true;
    try {
      while (ceQueueHead < ceReactionQueue.length) {
        const { el, callbackName, args } = ceReactionQueue[ceQueueHead];
        ceQueueHead += 1;
        if (!el || el.nodeType !== 1 || !ceUpgraded.has(el)) continue;
        const name = String(el.localName || el.nodeName).toLowerCase();
        const ctor = ceRegistry.get(name);
        if (!ctor) continue;
        invokeCeCallback(el, ctor, callbackName, args);
      }
    } finally {
      ceDraining = false;
      ceReactionQueue.length = 0;
      ceQueueHead = 0;
    }
  }

  function isElementConnected(el) {
    if (!el) return false;
    let n = el;
    while (n) {
      if (n.nodeType === 11) return false;
      if (n === pageDocument) return true;
      n = n.parentNode;
    }
    return false;
  }

  function walkUpgradedInSubtree(root, visitor) {
    walkElementTree(root, (el) => {
      if (ceUpgraded.has(el)) visitor(el);
    });
  }

  function installHTMLElementConstructorPatch() {
    if (ceHTMLElementPatched) return;
    const Orig = domino.impl.HTMLElement;
    if (!Orig) return;
    function HTMLElement() {
      if (new.target === undefined) {
        throw new TypeError('Illegal constructor');
      }
      if (ceUpgradeTarget != null) {
        const existing = ceUpgradeTarget;
        return existing;
      }
      return Reflect.construct(Orig, arguments, new.target);
    }
    Object.setPrototypeOf(HTMLElement, Orig);
    HTMLElement.prototype = Orig.prototype;
    domino.impl.HTMLElement = HTMLElement;
    global.HTMLElement = HTMLElement;
    ceHTMLElementPatched = true;
  }

  function customElementExtendsHTMLElement(ctor) {
    const root = global.HTMLElement && global.HTMLElement.prototype;
    if (!root) return false;
    let p = ctor.prototype;
    while (p && p !== Object.prototype) {
      if (p === root) return true;
      p = Object.getPrototypeOf(p);
    }
    return false;
  }

  function customElementConstructorIsClass(ctor) {
    try {
      const src = Function.prototype.toString.call(ctor);
      return /^\s*class[\s{]/.test(src) || src.startsWith('class ');
    } catch (_) {
      return false;
    }
  }

  function runCustomElementConstructor(el, ctor) {
    if (ceFailed.has(el)) return false;
    const prevProto = Object.getPrototypeOf(el);
    Object.setPrototypeOf(el, ctor.prototype);
    setElementDefinedState(el, true);
    ceUpgradeTarget = el;
    try {
      const mustNew =
        customElementExtendsHTMLElement(ctor) && customElementConstructorIsClass(ctor);
      if (mustNew) {
        const result = Reflect.construct(ctor, [], ctor);
        if (result !== el) {
          ceFailed.add(el);
          Object.setPrototypeOf(el, prevProto);
          setElementDefinedState(el, false);
          return false;
        }
        return true;
      }
      const ret = ctor.call(el);
      if (ret && typeof ret === 'object' && ret !== el) {
        ceFailed.add(el);
        Object.setPrototypeOf(el, prevProto);
        setElementDefinedState(el, false);
        return false;
      }
      return true;
    } catch (err) {
      ceFailed.add(el);
      Object.setPrototypeOf(el, prevProto);
      setElementDefinedState(el, false);
      consoleError(err);
      return false;
    } finally {
      ceUpgradeTarget = null;
    }
  }

  function upgradeCustomElement(el) {
    if (!el || el.nodeType !== 1 || ceUpgraded.has(el) || ceFailed.has(el)) return;
    const name = String(el.localName || el.nodeName).toLowerCase();
    if (!validCustomElementName(name)) return;
    const ctor = ceRegistry.get(name);
    if (!ctor) return;
    if (!runCustomElementConstructor(el, ctor)) return;
    ceUpgraded.add(el);
    const observed = observedAttributesOf(ctor);
    for (let i = 0; i < observed.length; i += 1) {
      const attr = observed[i];
      if (el.hasAttribute(attr)) {
        invokeCeCallback(el, ctor, 'attributeChangedCallback', [attr, null, el.getAttribute(attr)]);
      }
    }
    if (isElementConnected(el)) {
      invokeCeCallback(el, ctor, 'connectedCallback', null);
    }
  }

  function walkElementTree(root, visitor) {
    if (!root) return;
    if (root.nodeType === 1) visitor(root);
    for (let c = root.firstChild; c; c = c.nextSibling) walkElementTree(c, visitor);
  }

  function upgradeInSubtree(root) {
    if (!root) return;
    const stack = [];
    walkElementTree(root, (el) => {
      const name = String(el.localName || el.nodeName).toLowerCase();
      if (validCustomElementName(name) && !ceUpgraded.has(el) && !ceFailed.has(el)) stack.push(el);
    });
    for (let i = 0; i < stack.length; i += 1) {
      upgradeCustomElement(stack[i]);
    }
  }

  function patchDocumentCreateElement(doc) {
    if (!doc || doc.__glide_ce_create_patched) return;
    const orig = doc.createElement.bind(doc);
    doc.createElement = function createElementPatched(tag, options) {
      const el = orig(tag, options);
      const name = String(tag).toLowerCase();
      if (validCustomElementName(name) && ceRegistry.has(name)) upgradeCustomElement(el);
      return el;
    };
    doc.__glide_ce_create_patched = true;
  }

  function resolveWhenDefined(name) {
    const pending = cePending.get(name);
    if (pending) {
      cePending.delete(name);
      pending.resolve(ceRegistry.get(name));
    }
  }

  function installCustomElementsRegistry() {
    installFormControlMirror();
    installHTMLElementConstructorPatch();
    const registry = {
      define(name, ctor, options) {
        const n = String(name);
        if (!validCustomElementName(n)) {
          throw new SyntaxError('Invalid custom element name');
        }
        if (ceRegistry.has(n)) {
          const err = typeof DOMException !== 'undefined' && DOMException.NOT_SUPPORTED_ERR != null
            ? new DOMException(DOMException.NOT_SUPPORTED_ERR)
            : new Error('Custom element already defined');
          err.name = 'NotSupportedError';
          throw err;
        }
        if (ceConstructors.has(ctor)) {
          const err = typeof DOMException !== 'undefined' && DOMException.NOT_SUPPORTED_ERR != null
            ? new DOMException(DOMException.NOT_SUPPORTED_ERR)
            : new Error('Constructor already used with another custom element name');
          err.name = 'NotSupportedError';
          throw err;
        }
        if (typeof ctor !== 'function') throw new TypeError('Constructor must be a function');
        if (options && options.extends) {
          diagUnsupported('custom-elements', 'is= customized built-ins');
          const err = new Error('customized built-ins are not supported');
          err.name = 'NotSupportedError';
          throw err;
        }
        ceRegistry.set(n, ctor);
        ceConstructors.add(ctor);
        resolveWhenDefined(n);
        if (pageDocument) upgradeInSubtree(pageDocument);
        flushMutations();
      },
      get(name) {
        return ceRegistry.get(String(name)) || undefined;
      },
      whenDefined(name) {
        const n = String(name);
        if (!validCustomElementName(n)) {
          return Promise.reject(new SyntaxError('Invalid custom element name'));
        }
        if (ceRegistry.has(n)) return Promise.resolve(ceRegistry.get(n));
        if (!cePending.has(n)) {
          let resolve;
          const p = new Promise((r) => { resolve = r; });
          cePending.set(n, { promise: p, resolve });
          return p;
        }
        return cePending.get(n).promise;
      },
      upgrade(root) {
        const node = root || pageDocument;
        if (node) upgradeInSubtree(node);
        drainCeReactions();
        flushMutations();
      },
    };
    try {
      Object.defineProperty(global, 'customElements', {
        value: registry,
        writable: true,
        configurable: true,
      });
    } catch (_) {
      global.customElements = registry;
    }
  }

  let currentPageUrl = 'about:blank';
  let pageLocation = null;
  let formSubmissionInstalled = false;
  let formControlMirrorInstalled = false;

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

  // Scripts inserted by a script run once, after the inserting task mirrors its
  // DOM. Parser-inserted nodes (document markup, innerHTML fragments, and
  // document.write markup, which runs its own inline scripts) are marked by
  // domino and never re-run. External sources are fetched through the same
  // synchronous host channel as fetch().
  const pendingDynamicScripts = [];
  const executedScripts = new WeakSet();
  let dynamicScriptsRunning = false;

  function queueInsertedScripts(root) {
    if (!root || writingDocument) return;
    if (root.nodeType !== 1 && root.nodeType !== 11) return;
    const stack = [root];
    let checked = 0;
    while (stack.length && checked < 64) {
      const node = stack.pop();
      checked += 1;
      if (node.nodeType === 1 && String(node.localName || '').toLowerCase() === 'script') {
        if (node._parser_inserted || node._already_started) continue;
        if (executedScripts.has(node)) continue;
        if (!rooted(node)) continue;
        executedScripts.add(node);
        pendingDynamicScripts.push(node);
        continue;
      }
      for (let c = node.firstChild; c; c = c.nextSibling) stack.push(c);
    }
  }

  function scriptKindOf(node) {
    const type = String((node.getAttribute && node.getAttribute('type')) || '')
      .trim()
      .toLowerCase();
    if (!type) return 'classic';
    if (type === 'module') return 'module';
    if (type.indexOf('javascript') >= 0) return 'classic';
    return null;
  }

  function dynamicScriptSource(node) {
    const src = node.getAttribute && node.getAttribute('src');
    if (!src) return { source: node.textContent || '', name: '<dynamic>' };
    let resolved = String(src);
    try {
      const parsed = __glide_url_parse(String(src), currentPageUrl);
      if (parsed && parsed.href) resolved = parsed.href;
    } catch (_) { /* keep the raw src */ }
    let raw;
    try {
      raw = rpc('fetch', resolved);
    } catch (err) {
      return { error: String((err && err.message) || err || 'fetch failed') };
    }
    if (!raw || raw.__error) {
      return { error: String((raw && raw.__error) || 'fetch failed') };
    }
    const status = Number(raw.status);
    if (!(status >= 200 && status < 300)) return { error: 'HTTP ' + status };
    return { source: String(raw.body || ''), name: resolved };
  }

  function runDynamicScript(node) {
    if (!rooted(node)) return;
    const kind = scriptKindOf(node);
    if (kind == null) return;
    const src = String((node.getAttribute && node.getAttribute('src')) || '<inline>');
    const fetched = dynamicScriptSource(node);
    if (fetched.error) {
      diagUnsupported('dynamic-script', src + ' (' + fetched.error + ')');
      notifyScriptElement(node, false);
      return;
    }
    flushMutations();
    const id = blitzId(node);
    const setCurrent = global.__glide_set_current_script;
    if (id != null && typeof setCurrent === 'function') setCurrent(String(id));
    let ok = true;
    try {
      if (typeof global.__glide_execute_script === 'function') {
        const result = global.__glide_execute_script(fetched.source, kind, fetched.name);
        ok = result !== false;
      }
    } catch (_) {
      ok = false;
      // Compilation and runtime errors are reported by the worker.
    }
    if (id != null && typeof setCurrent === 'function') setCurrent('');
    flushMutations();
    notifyScriptElement(node, ok);
  }

  function runDynamicScripts() {
    if (dynamicScriptsRunning) return;
    dynamicScriptsRunning = true;
    let ran = 0;
    try {
      while (pendingDynamicScripts.length && ran < 64) {
        ran += 1;
        runDynamicScript(pendingDynamicScripts.shift());
      }
    } finally {
      dynamicScriptsRunning = false;
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

  let moDeliverScheduled = false;
  let moSnapshotBuilding = false;
  const moRegistry = new Set();

  function nodeListLike(nodes) {
    const arr = Array.from(nodes);
    arr.item = function (i) { return arr[i] || null; };
    return arr;
  }

  function attrNameMatchesFilter(name, filter, target) {
    if (!filter || !filter.length) return true;
    const n = String(name);
    const html = target && target.namespaceURI === XHTML;
    for (let i = 0; i < filter.length; i++) {
      const f = String(filter[i]);
      if (html ? f.toLowerCase() === n.toLowerCase() : f === n) return true;
    }
    return false;
  }

  function isDescendant(node, ancestor) {
    let n = node;
    while (n) {
      if (n === ancestor) return true;
      n = n.parentNode;
    }
    return false;
  }

  function recordMatchesTarget(target, options, recordTarget) {
    if (recordTarget === target) return true;
    return !!options.subtree && recordTarget && isDescendant(recordTarget, target);
  }

  // Domino's mutation hooks do not expose previous attribute/character values;
  // attributeOldValue/characterDataOldValue options are accepted but oldValue stays null.
  function dominoToMutationRecords(rec) {
    const out = [];
    if (!rec) return out;
    switch (rec.type) {
      case MUTATE.INSERT: {
        const parent = rec.target;
        const node = rec.node;
        if (!parent || !node) break;
        out.push({
          type: 'childList',
          target: parent,
          addedNodes: nodeListLike([node]),
          removedNodes: nodeListLike([]),
          previousSibling: node.previousSibling,
          nextSibling: node.nextSibling,
        });
        break;
      }
      case MUTATE.REMOVE: {
        const parent = rec.target;
        const node = rec.node;
        if (!parent || !node) break;
        out.push({
          type: 'childList',
          target: parent,
          addedNodes: nodeListLike([]),
          removedNodes: nodeListLike([node]),
          previousSibling: node.previousSibling,
          nextSibling: node.nextSibling,
        });
        break;
      }
      case MUTATE.MOVE: {
        const node = rec.target;
        const parent = node && node.parentNode;
        if (!parent || !node) break;
        out.push({
          type: 'childList',
          target: parent,
          addedNodes: nodeListLike([node]),
          removedNodes: nodeListLike([]),
          previousSibling: node.previousSibling,
          nextSibling: node.nextSibling,
        });
        break;
      }
      case MUTATE.ATTR: {
        const a = rec.attr;
        if (!rec.target || !a) break;
        out.push({
          type: 'attributes',
          target: rec.target,
          attributeName: a.localName || a.name,
          attributeNamespace: a.namespaceURI || null,
          oldValue: rec.oldValue != null ? rec.oldValue : null,
        });
        break;
      }
      case MUTATE.REMOVE_ATTR: {
        const a = rec.attr;
        if (!rec.target || !a) break;
        out.push({
          type: 'attributes',
          target: rec.target,
          attributeName: a.localName || a.name,
          attributeNamespace: a.namespaceURI || null,
          oldValue: rec.oldValue != null ? rec.oldValue : null,
        });
        break;
      }
      case MUTATE.VALUE: {
        if (!rec.target) break;
        out.push({
          type: 'characterData',
          target: rec.target,
          oldValue: null,
        });
        break;
      }
      default:
        break;
    }
    return out;
  }

  function observerWantsRecord(options, record) {
    if (record.type === 'childList' && !options.childList) return false;
    if (record.type === 'attributes' && !options.attributes) return false;
    if (record.type === 'characterData' && !options.characterData) return false;
    if (record.type === 'attributes' && options.attributeFilter) {
      if (!attrNameMatchesFilter(record.attributeName, options.attributeFilter, record.target)) {
        return false;
      }
    }
    return true;
  }

  function scheduleMutationObserverDelivery() {
    if (moDeliverScheduled) return;
    moDeliverScheduled = true;
    const deliver = function () {
      moDeliverScheduled = false;
      if (moSnapshotBuilding) return;
      let reschedule = false;
      for (const obs of moRegistry) {
        if (!obs._queue.length) continue;
        const batch = obs._queue.slice();
        obs._queue.length = 0;
        try {
          obs._callback.call(obs, batch, obs);
        } catch (err) {
          consoleError(err);
        }
        if (obs._queue.length) reschedule = true;
      }
      if (reschedule) scheduleMutationObserverDelivery();
    };
    if (typeof global.queueMicrotask === 'function') global.queueMicrotask(deliver);
    else Promise.resolve().then(deliver);
  }

  function enqueueMutationRecords(rec) {
    if (moSnapshotBuilding || !rec) return;
    const records = dominoToMutationRecords(rec);
    if (!records.length) return;
    let added = false;
    for (const obs of moRegistry) {
      if (!obs._targets.size) continue;
      for (const [target, options] of obs._targets) {
        for (const record of records) {
          if (!recordMatchesTarget(target, options, record.target)) continue;
          if (!observerWantsRecord(options, record)) continue;
          obs._queue.push(record);
          added = true;
        }
      }
    }
    if (added) scheduleMutationObserverDelivery();
  }

  function normalizeMoOptions(options) {
    options = options || {};
    return {
      childList: !!options.childList,
      attributes: !!options.attributes,
      characterData: !!options.characterData,
      subtree: !!options.subtree,
      attributeOldValue: !!options.attributeOldValue,
      characterDataOldValue: !!options.characterDataOldValue,
      attributeFilter: Array.isArray(options.attributeFilter)
        ? options.attributeFilter.map(String) : null,
    };
  }

  function resetMutationObservers() {
    for (const obs of moRegistry) {
      obs._targets.clear();
      obs._queue.length = 0;
    }
  }

  global.MutationObserver = class MutationObserver {
    constructor(callback) {
      if (typeof callback !== 'function') {
        throw new TypeError('MutationObserver callback must be a function');
      }
      this._callback = callback;
      this._targets = new Map();
      this._queue = [];
      moRegistry.add(this);
    }
    observe(target, options) {
      if (!target || typeof target.nodeType !== 'number') {
        throw new TypeError('MutationObserver.observe: target must be a Node');
      }
      const moOpts = normalizeMoOptions(options);
      if (moOpts.characterData) {
        const nt = target.nodeType;
        if (nt === 3 || nt === 8 || nt === 7) {
          installCharacterDataInstanceHook(target);
        } else if (moOpts.subtree) {
          hookCharacterDataSubtree(target);
        }
      }
      this._targets.set(target, moOpts);
    }
    disconnect() {
      this._targets.clear();
      this._queue.length = 0;
    }
    takeRecords() {
      const batch = this._queue.slice();
      this._queue.length = 0;
      return batch;
    }
  };
  global.MutationRecord = function MutationRecord() {};

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
    drainCeReactions();
    flushMutations();
    runDynamicScripts();
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
    enqueueMutationRecords(rec);
    if (!mirroringEnabled || !rec) return;
    remirrorSkipped();
    switch (rec.type) {
      case MUTATE.INSERT: {
        if (!rec.node || nodeToBlitz.get(rec.node) != null) return;
        queueInsertedScripts(rec.node);
        assignFormOwners(rec.node, false);
        subtreeOps(rec.target, followingRef(rec.node), rec.node, pendingBatch);
        if (rec.node) {
          const upgradedBefore = new WeakSet();
          walkElementTree(rec.node, (el) => {
            if (ceUpgraded.has(el)) upgradedBefore.add(el);
          });
          upgradeInSubtree(rec.node);
          walkUpgradedInSubtree(rec.node, (el) => {
            if (upgradedBefore.has(el) && isElementConnected(el)) {
              enqueueCeReaction(el, 'connectedCallback', null);
            }
          });
          drainCeReactions();
        }
        break;
      }
      case MUTATE.REMOVE: {
        const node = rec.node;
        if (!node) return;
        walkUpgradedInSubtree(node, (el) => {
          enqueueCeReaction(el, 'disconnectedCallback', null);
        });
        drainCeReactions();
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
        if (node && node.nodeType === 1) {
          walkUpgradedInSubtree(node, (el) => {
            enqueueCeReaction(el, 'disconnectedCallback', null);
            if (isElementConnected(el)) enqueueCeReaction(el, 'connectedCallback', null);
          });
          drainCeReactions();
        }
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
        if (rec.target && rec.target.nodeType === 1 && ceUpgraded.has(rec.target)) {
          const ctor = ceRegistry.get(String(rec.target.localName || '').toLowerCase());
          if (ctor) {
            const observed = observedAttributesOf(ctor);
            const lname = String(name || '').toLowerCase();
            if (observed.some((n) => String(n).toLowerCase() === lname)) {
              const prev = rec.oldValue != null ? rec.oldValue : null;
              enqueueCeReaction(rec.target, 'attributeChangedCallback', [
                name,
                prev,
                a.value,
              ]);
              drainCeReactions();
            }
          }
        }
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
        if (rec.target && rec.target.nodeType === 1 && ceUpgraded.has(rec.target)) {
          const ctor = ceRegistry.get(String(rec.target.localName || '').toLowerCase());
          if (ctor) {
            const observed = observedAttributesOf(ctor);
            const lname = String(name || '').toLowerCase();
            if (observed.some((n) => String(n).toLowerCase() === lname)) {
              const prev = rec.oldValue != null ? rec.oldValue : null;
              enqueueCeReaction(rec.target, 'attributeChangedCallback', [name, prev, null]);
              drainCeReactions();
            }
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

  // Domino only emits value mutations for rooted nodes. Page scripts may observe
  // detached Text/Comment nodes (the classic microtask-queue polyfill does), so
  // wrap the node's `_data` slot and emit for unrooted changes ourselves.
  function hookCharacterDataSubtree(root) {
    if (!root) return;
    const stack = [root.firstChild];
    let walked = 0;
    while (stack.length && walked < 4096) {
      const node = stack.pop();
      if (!node) continue;
      walked += 1;
      installCharacterDataInstanceHook(node);
      if (node.nextSibling) stack.push(node.nextSibling);
      if (node.firstChild) stack.push(node.firstChild);
    }
  }

  function installCharacterDataInstanceHook(node) {
    if (!node || node.__glideCharDataDelivery) return;
    const nt = node.nodeType;
    if (nt !== 3 && nt !== 8 && nt !== 7) return;
    if (!Object.prototype.hasOwnProperty.call(node, '_data')) return;
    const prior = Object.getOwnPropertyDescriptor(node, '_data');
    if (prior && prior.get) return;
    let data = node._data;
    Object.defineProperty(node, '_data', {
      configurable: true,
      enumerable: true,
      get() { return data; },
      set(v) {
        if (v === null || v === undefined) v = '';
        else v = String(v);
        if (v === data) return;
        data = v;
        if (!node.rooted) {
          const doc = pageDocument || node.ownerDocument;
          if (doc) doc.mutateValue(node);
        }
      },
    });
    node.__glideCharDataDelivery = true;
  }

  function patchPageDocumentCharacterDataCreates(doc) {
    if (!doc || doc.__glideCharDataCreates) return;
    doc.__glideCharDataCreates = true;
    for (const name of ['createTextNode', 'createComment', 'createProcessingInstruction']) {
      const orig = doc[name];
      if (typeof orig !== 'function') continue;
      const wrapped = function (...args) {
        const node = orig.apply(this, args);
        installCharacterDataInstanceHook(node);
        return node;
      };
      try {
        Object.defineProperty(doc, name, {
          configurable: true,
          writable: true,
          value: wrapped,
        });
      } catch (_) { /* observe-time hooks still cover detached observers */ }
    }
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
    if (scriptEl.getAttribute && scriptEl.getAttribute('src')) {
      if (!documentLoaded) {
        runDynamicScript(scriptEl);
      }
      return;
    }
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

  function parsedPageUrl(pageUrl) {
    let parsed = null;
    try { parsed = __glide_url_parse(String(pageUrl || 'about:blank')); } catch (_) { parsed = null; }
    return parsed || {};
  }

  function fillLocationFields(loc, parsed, pageUrl) {
    const href = parsed.href || pageUrl || 'about:blank';
    loc.href = href;
    loc.protocol = parsed.protocol || '';
    loc.host = parsed.host || '';
    loc.hostname = parsed.hostname || '';
    loc.port = parsed.port || '';
    loc.pathname = parsed.pathname || '';
    loc.search = parsed.search || '';
    loc.hash = parsed.hash || '';
    loc.origin = parsed.origin || 'null';
  }

  function syncLocationObject(parsed, pageUrl) {
    if (!pageLocation) return;
    fillLocationFields(pageLocation, parsed, pageUrl);
  }

  function setPageUrl(href) {
    currentPageUrl = String(href);
    syncLocationObject(parsedPageUrl(currentPageUrl), currentPageUrl);
  }

  function installGlobals(pageUrl) {
    currentPageUrl = pageUrl || 'about:blank';
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
    installEventHandlerProperties(impl.HTMLElement.prototype);
    installScriptElementExtras(impl.HTMLScriptElement);
    installFormSubmission();
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
    resetCustomElementState();
    installCustomElementsRegistry();
    pageLocation = locationFrom(currentPageUrl);
    global.location = pageLocation;
    try {
      Object.defineProperty(global.document, 'URL', {
        configurable: true,
        get: () => currentPageUrl,
      });
      Object.defineProperty(global.document, 'documentURI', {
        configurable: true,
        get: () => currentPageUrl,
      });
      Object.defineProperty(global.document, 'location', {
        configurable: true,
        get: () => pageLocation,
      });
      Object.defineProperty(global.document, 'cookie', {
        configurable: true,
        enumerable: true,
        get() {
          return String(rpc('cookieGet') || '');
        },
        set(value) {
          rpc('cookieSet', String(value));
        },
      });
    } catch (_) { /* domino defines these as readonly */ }
  }

  const EVENT_HANDLER_TYPES = [
    'load', 'error', 'click', 'change', 'input', 'reset', 'focus', 'blur',
    'keydown', 'keyup', 'keypress', 'scroll', 'resize', 'select', 'toggle',
    'readystatechange', 'message', 'popstate', 'hashchange',
  ];

  // Domino only maps on* properties for elements that declare `events`; most
  // elements keep `el.onload = fn` as a plain property that dispatch never
  // calls. Register the common ones as real listeners.
  function installEventHandlerProperties(proto) {
    if (!proto) return;
    for (const type of EVENT_HANDLER_TYPES) {
      const prop = 'on' + type;
      if (Object.prototype.hasOwnProperty.call(proto, prop)) continue;
      const key = '__glide_handler_' + type;
      Object.defineProperty(proto, prop, {
        configurable: true,
        enumerable: true,
        get() {
          return this[key] || null;
        },
        set(fn) {
          const previous = this[key];
          if (previous) this.removeEventListener(type, previous);
          const handler = typeof fn === 'function' ? fn : null;
          if (handler) this.addEventListener(type, handler);
          this[key] = handler;
        },
      });
    }
  }

  function installScriptElementExtras(Script) {
    if (!Script || !Script.prototype) return;
    if (!Object.prototype.hasOwnProperty.call(Script.prototype, 'noModule')) {
      Object.defineProperty(Script.prototype, 'noModule', {
        configurable: true,
        enumerable: true,
        get() {
          return !!this.hasAttribute('nomodule');
        },
        set(value) {
          if (value) this.setAttribute('nomodule', '');
          else this.removeAttribute('nomodule');
        },
      });
    }
  }

  function notifyScriptElement(node, ok) {
    try {
      node.dispatchEvent(new domino.impl.Event(ok ? 'load' : 'error', { bubbles: false }));
    } catch (_) { /* a handler error must not break the queue */ }
  }

  // Form submission (HTML Living Standard, form submission algorithm).
  //
  // Domino mirrors Blitz's element tree; Blitz builds the entry list, resolves
  // the action and calls the document's navigation provider. Domino's
  // `HTMLFormElement.submit` is a no-op and `requestSubmit` does not exist, so
  // pages that submit forms (Reddit's JavaScript challenge, search boxes) never
  // navigate. `requestSubmit` fires the cancelable `submit` event first, which
  // is where pages add fields to the form.
  function mirrorControlField(node, field) {
    const id = blitzId(node);
    if (id == null) return null;
    const raw = rpc('controlField', String(id), field);
    if (raw == null) return null;
    if (field === 'checked') {
      const on = raw === 'true' || raw === true;
      try { node.checked = on; } catch (_) { /* domino may reject on wrong type */ }
      return on;
    }
    if (field === 'value') {
      try { node.value = String(raw); } catch (_) { /* textarea/input */ }
      return String(raw);
    }
    return raw;
  }

  const controlValueCache = new WeakMap();

  function syncFormControlsFromBlitz(form) {
    const formId = blitzId(form);
    if (formId == null) return;
    const rows = rpc('syncFormControls', String(formId));
    if (!Array.isArray(rows)) return;
    for (const row of rows) {
      const node = blitzToNode.get(String(row.id));
      if (!node) continue;
      if (row.checked != null) {
        try { node.checked = Boolean(row.checked); } catch (_) { /* type */ }
      }
      if (row.value != null) {
        try { node.value = String(row.value); } catch (_) { /* type */ }
      }
    }
  }

  function pushFormControlsToBlitz(form) {
    if (!form || form.nodeType !== 1) return;
    const pairs = [];
    const stack = [form];
    while (stack.length) {
      const node = stack.pop();
      if (!node || node.nodeType !== 1) continue;
      const tag = String(node.localName || '').toLowerCase();
      if (tag === 'input' || tag === 'textarea') {
        const id = blitzId(node);
        if (id != null) {
          let value = '';
          try {
            value = String(node.value);
          } catch (_) {
            value = (node.getAttribute && node.getAttribute('value')) || '';
          }
          if (controlValueCache.has(node)) value = String(controlValueCache.get(node));
          pairs.push([id, value]);
        }
      }
      for (let c = node.firstChild; c; c = c.nextSibling) stack.push(c);
    }
    if (pairs.length) {
      try {
        rpc('pushFormValues', JSON.stringify(pairs));
      } catch (_) { /* navigation must not throw into the page */ }
    }
  }

  function installFormControlMirror() {
    if (formControlMirrorInstalled) return;
    const Input = domino.impl.HTMLInputElement;
    const TextArea = domino.impl.HTMLTextAreaElement;
    if (!Input || !Input.prototype) return;
    formControlMirrorInstalled = true;
    const inputProto = Object.create(Input.prototype);
    Object.defineProperty(Input, 'prototype', {
      value: inputProto,
      writable: true,
      enumerable: false,
      configurable: false,
    });
    Object.defineProperty(inputProto, 'value', {
      configurable: true,
      get() {
        if (controlValueCache.has(this)) return controlValueCache.get(this);
        return mirrorControlField(this, 'value') ?? '';
      },
      set(v) {
        const text = String(v);
        controlValueCache.set(this, text);
        this.setAttribute('value', text);
      },
    });
    Object.defineProperty(inputProto, 'checked', {
      configurable: true,
      get() {
        const raw = mirrorControlField(this, 'checked');
        return raw === true || raw === 'true';
      },
      set(on) {
        if (on) this.setAttribute('checked', '');
        else this.removeAttribute('checked');
      },
    });
    if (TextArea && TextArea.prototype) {
      const taProto = Object.create(TextArea.prototype);
      Object.defineProperty(TextArea, 'prototype', {
        value: taProto,
        writable: true,
        enumerable: false,
        configurable: false,
      });
      Object.defineProperty(taProto, 'value', {
        configurable: true,
        get() {
          if (controlValueCache.has(this)) return controlValueCache.get(this);
          return mirrorControlField(this, 'value') ?? '';
        },
        set(v) {
          const text = String(v);
          controlValueCache.set(this, text);
          this.setAttribute('value', text);
        },
      });
    }
  }

  function installFormSubmission() {
    if (formSubmissionInstalled) return;
    const Form = domino.impl.HTMLFormElement;
    if (!Form || !Form.prototype) return;
    formSubmissionInstalled = true;
    // Domino installs `submit` as a non-configurable, non-writable no-op on
    // HTMLFormElement.prototype. Forms created later inherit from whatever
    // `Form.prototype` points at, so swap in a child prototype that adds the
    // real methods without trying to redefine the locked property.
    const proto = Object.create(Form.prototype);
    Object.defineProperty(Form, 'prototype', {
      value: proto,
      writable: true,
      enumerable: false,
      configurable: false,
    });

    // Domino has no event-handler IDL properties. `form.onsubmit = fn` is a
    // plain property there, so dispatching `submit` would not call it.
    Object.defineProperty(proto, 'onsubmit', {
      configurable: true,
      get() {
        return this.__glide_onsubmit || null;
      },
      set(fn) {
        const previous = this.__glide_onsubmit;
        if (previous) this.removeEventListener('submit', previous);
        const handler = typeof fn === 'function' ? fn : null;
        if (handler) this.addEventListener('submit', handler);
        this.__glide_onsubmit = handler;
      },
    });

    function submitFormElement(form, submitter, skipBlitzPull) {
      const formId = blitzId(form);
      if (formId == null) return;
      if (!skipBlitzPull) syncFormControlsFromBlitz(form);
      pushFormControlsToBlitz(form);
      // Fields added or changed in this task must exist in Blitz first.
      flushMutations();
      const submitterId = submitter ? blitzId(submitter) : null;
      try {
        rpc('submitForm', String(formId), String(submitterId == null ? formId : submitterId));
      } catch (_) {
        // A rejected navigation must not throw into the page's handler.
      }
    }

    Object.defineProperty(proto, 'submit', {
      configurable: true,
      writable: true,
      value: function submit() {
        submitFormElement(this, null);
      },
    });

    Object.defineProperty(proto, 'requestSubmit', {
      configurable: true,
      writable: true,
      value: function requestSubmit(submitter) {
        if (submitter != null && submitter.form !== this) {
          const DOMException = domino.impl.DOMException;
          throw new DOMException(
            'The submitter is not owned by this form element.',
            DOMException.NOT_FOUND_ERR,
          );
        }
        const evt = new domino.impl.Event('submit', { bubbles: true, cancelable: true });
        if (!this.dispatchEvent(evt)) return;
        flushMutations();
        submitFormElement(this, submitter || null, true);
      },
    });
  }

  function locationFrom(pageUrl) {
    const loc = {
      assign(u) {
        const base = currentPageUrl;
        const resolved = __glide_url_parse(String(u), base);
        const href = (resolved && resolved.href) || String(u);
        rpc('navigate', href);
      },
      replace(u) {
        const base = currentPageUrl;
        const resolved = __glide_url_parse(String(u), base);
        const href = (resolved && resolved.href) || String(u);
        rpc('navigate', href);
      },
      reload() {},
      toString() { return this.href; },
    };
    fillLocationFields(loc, parsedPageUrl(pageUrl), pageUrl);
    return loc;
  }

  global.__glide_set_page_url = setPageUrl;
  global.__glide_get_page_url = function () { return currentPageUrl; };

  global.__glide_init_page = function (snapshotJson, pageUrl) {
    // Must run before the snapshot creates elements, so they inherit the
    // prototype that carries `submit` / `requestSubmit`.
    installFormSubmission();
    moSnapshotBuilding = true;
    resetMutationObservers();
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
    resetCustomElementState();
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
    patchDocumentCreateElement(pageDocument);
    pageDocument._setMutationHandler(onMutation);
    patchPageDocumentCharacterDataCreates(pageDocument);
    Object.defineProperty(pageDocument, 'defaultView', {
      configurable: true,
      get: () => global,
    });
    installGlobals(pageUrl || 'about:blank');
    ceHTMLElementPatched = false;
    installHTMLElementConstructorPatch();
    if (typeof global.__glide_reset_page_history === 'function') {
      global.__glide_reset_page_history();
    }
    if (typeof global.__glide_capture_performance_navigation === 'function') {
      global.__glide_capture_performance_navigation();
    }
    installDocumentWrite(pageDocument);
    patchEventPath();
    for (const [node, name, val] of pendingOnHandlers) applyOnAttribute(node, name, val);
    pendingOnHandlers.length = 0;
    mirroringEnabled = true;
    moSnapshotBuilding = false;
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

  global.__glide_notify_script_element = function (id, ok) {
    const node = blitzToNode.get(String(id));
    if (node) notifyScriptElement(node, !!ok);
  };

  global.__glide_begin_task = beginTask;

  global.__glide_mark_document_loaded = function () {
    documentLoaded = true;
  };

  global.__glide_dispatch_dom_content_loaded = function () {
    beginTask();
    flushMutations();
    if (pageDocument) {
      pageDocument.readyState = 'interactive';
      pageDocument.dispatchEvent(new domino.impl.Event('DOMContentLoaded', { bubbles: true }));
    }
    flushMutations();
  };

  global.__glide_fire_page_load = function () {
    beginTask();
    flushMutations();
    if (pageDocument) {
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
      get size() { return this._pairs.length; }
      append(k, v) { this._pairs.push([String(k), String(v)]); }
      delete(k) {
        const key = String(k);
        this._pairs = this._pairs.filter(([name]) => name !== key);
      }
      getAll(k) {
        const key = String(k);
        return this._pairs.filter(([name]) => name === key).map(([, value]) => value);
      }
      get(k) {
        const key = String(k);
        for (let i = 0; i < this._pairs.length; i++) {
          if (this._pairs[i][0] === key) return this._pairs[i][1];
        }
        return null;
      }
      has(k) {
        const key = String(k);
        return this._pairs.some(([name]) => name === key);
      }
      set(k, v) {
        const key = String(k);
        const value = String(v);
        const at = this._pairs.findIndex(([name]) => name === key);
        if (at < 0) {
          this._pairs.push([key, value]);
          return;
        }
        this._pairs[at] = [key, value];
        this._pairs = this._pairs.filter((pair, i) => i <= at || pair[0] !== key);
      }
      sort() {
        this._pairs.sort((a, b) => (a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : 0));
      }
      forEach(callback, thisArg) {
        this._pairs.slice().forEach(([name, value]) => {
          callback.call(thisArg, value, name, this);
        });
      }
      *entries() { yield* this._pairs; }
      *keys() { yield* this._pairs.map(([name]) => name); }
      *values() { yield* this._pairs.map(([, value]) => value); }
      [Symbol.iterator]() { return this.entries(); }
      toString() {
        return this._pairs.map(([k, v]) => encodeURIComponent(k) + '=' + encodeURIComponent(v)).join('&');
      }
    };
  }
  installCustomElementsRegistry();
  installUrlGlobal();
})(globalThis);
