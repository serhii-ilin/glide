/* Mirroring-readiness probe; evaluated inside the library's window context. */
(function () {
  const out = {
    mutationObserver: false,
    mutationHandlerHook: false,
    idTagging: false,
    overrideGetBoundingClientRect: false,
    overrideGetComputedStyle: false,
    range: false,
    treeWalker: false,
    nodeIterator: false,
    domParser: false,
    documentWrite: false,
    documentWriteNote: "",
  };

  try {
    out.mutationObserver = typeof MutationObserver === "function";
  } catch (_) {}

  try {
    if (typeof document !== "undefined" && document._setMutationHandler) {
      out.mutationHandlerHook = true;
    }
    if (typeof domino !== "undefined" && domino && domino.impl) {
      out.mutationHandlerHook = true;
    }
  } catch (_) {}

  try {
    const el = document.createElement("div");
    el.__glideId = 42;
    out.idTagging = el.__glideId === 42;
    const parent = document.createElement("div");
    parent.appendChild(el);
    out.idTagging = out.idTagging && parent.firstChild === el;
  } catch (e) {
    out.idTagging = false;
  }

  try {
    const el = document.createElement("div");
    const orig = el.getBoundingClientRect;
    el.getBoundingClientRect = function () {
      return { x: 1, y: 2, width: 3, height: 4, top: 2, left: 1, right: 4, bottom: 6 };
    };
    const r = el.getBoundingClientRect();
    out.overrideGetBoundingClientRect = r && r.width === 3;
    el.getBoundingClientRect = orig;
  } catch (_) {}

  try {
    if (typeof getComputedStyle === "function") {
      const before = getComputedStyle(document.documentElement);
      window.getComputedStyle = function () {
        return { getPropertyValue: () => "red" };
      };
      const after = getComputedStyle(document.documentElement);
      out.overrideGetComputedStyle =
        after && typeof after.getPropertyValue === "function" &&
        after.getPropertyValue("color") === "red";
      window.getComputedStyle = function (elt) {
        return before;
      };
    }
  } catch (_) {}

  try {
    out.range = typeof Range !== "undefined" && typeof document.createRange === "function";
  } catch (_) {}
  try {
    out.treeWalker =
      typeof document.createTreeWalker === "function" &&
      typeof TreeWalker !== "undefined";
  } catch (_) {}
  try {
    out.nodeIterator =
      typeof document.createNodeIterator === "function" &&
      typeof NodeIterator !== "undefined";
  } catch (_) {}
  try {
    out.domParser = typeof DOMParser !== "undefined";
  } catch (_) {}

  try {
    out.documentWrite = typeof document.write === "function";
    if (out.documentWrite) {
      const body = document.body;
      const n = body ? body.childNodes.length : 0;
      document.write("<p id='dw-probe'>x</p>");
      const el = document.getElementById("dw-probe");
      out.documentWriteNote = el ? "writes-into-document" : `childDelta=${(body?.childNodes.length ?? 0) - n}`;
    }
  } catch (e) {
    out.documentWriteNote = String(e);
  }

  if (typeof globalThis.__SPIKE_PROBE__ !== "undefined") {
    globalThis.__SPIKE_PROBE__ = out;
  } else {
    this.__SPIKE_PROBE__ = out;
  }
  return out;
})();