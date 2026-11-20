import { parseHTML } from "linkedom";

export function createPage(html) {
  const parsed = parseHTML(
    html || "<!DOCTYPE html><html><head></head><body></body></html>",
  );
  return { window: parsed.window ?? parsed, document: parsed.document };
}