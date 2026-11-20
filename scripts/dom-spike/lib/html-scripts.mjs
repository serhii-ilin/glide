/**
 * Extract <script> blocks from HTML source in document order.
 * @returns {{ src: string | null, code: string, line: number }[]}
 */
export function extractScripts(html) {
  const scripts = [];
  const re = /<script\b([^>]*)>([\s\S]*?)<\/script\s*>|<script\b([^>]*)\/>/gi;
  let m;
  while ((m = re.exec(html)) !== null) {
    const attrs = m[1] ?? m[3] ?? "";
    const inline = m[2] ?? "";
    const srcMatch = /\bsrc\s*=\s*(?:"([^"]*)"|'([^']*)'|(\S+))/i.exec(attrs);
  const src = srcMatch ? srcMatch[1] ?? srcMatch[2] ?? srcMatch[3] : null;
    const before = html.slice(0, m.index);
    const line = before.split("\n").length;
    scripts.push({ src, code: inline.trim(), line });
  }
  return scripts;
}

/** Map WPT script src values to paths under tests/compat/wpt/. */
export function resolveScriptPath(src, testFileDir, wptRoot) {
  if (src.startsWith("/resources/")) {
    return `${wptRoot}/resources/${src.slice("/resources/".length)}`;
  }
  if (src.startsWith("resources/")) {
    return `${wptRoot}/${src}`;
  }
  if (src.startsWith("../../resources/")) {
    return `${wptRoot}/resources/${src.slice("../../resources/".length)}`;
  }
  if (src.startsWith("../")) {
    return `${testFileDir}/${src}`;
  }
  if (!src.includes("/")) {
    return `${testFileDir}/${src}`;
  }
  return `${testFileDir}/${src.replace(/^\.\//, "")}`;
}