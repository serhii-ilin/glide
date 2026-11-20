import domino from "domino";

export function createPage(html) {
  const window = domino.createWindow(html || "<!DOCTYPE html><html><head></head><body></body></html>");
  return { window, document: window.document };
}

export { domino };