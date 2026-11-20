import { Window } from "happy-dom";

export function createPage(html) {
  const window = new Window({
    url: "http://localhost/",
    width: 1024,
    height: 768,
  });
  const markup = html || "<!DOCTYPE html><html><head></head><body></body></html>";
  window.document.open();
  window.document.write(markup);
  window.document.close();
  return { window, document: window.document };
}