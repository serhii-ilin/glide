/** Minimal URL for happy-dom browser bundles (no node:url). */
export class URL {
  constructor(input, base) {
    let href = String(input ?? "");
    if (base) {
      try {
        const b = new URL(base);
        if (!href.includes("://") && !href.startsWith("//")) {
          href = b.origin + (href.startsWith("/") ? href : `/${href}`);
        }
      } catch {
        /* ignore */
      }
    }
    this.href = href;
    const m = /^([^:]+):\/\/([^/]+)(.*)$/.exec(href);
    this.protocol = m ? `${m[1]}:` : "";
    this.host = m ? m[2] : "";
    const hp = this.host.split(":");
    this.hostname = hp[0] ?? "";
    this.port = hp[1] ?? "";
    this.pathname = m ? m[3].split("?")[0].split("#")[0] || "/" : href;
    this.search = href.includes("?") ? `?${href.split("?")[1].split("#")[0]}` : "";
    this.hash = href.includes("#") ? `#${href.split("#").pop()}` : "";
    this.origin = this.protocol && this.host ? `${this.protocol}//${this.host}` : "";
  }
  static canParse() {
    return true;
  }
  toString() {
    return this.href;
  }
}

export default { URL };