/**
 * CSP 防回归：插件前端的 blob-import 装载机制要求
 * script-src 含 blob: 且 connect-src 覆盖 plugin-asset:。
 * 三处 CSP 来源（index.html meta、tauri.conf.json、tauri.dev.conf.json）必须一致满足。
 */

import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

const root = join(__dirname, "../..");

function cspOf(path: string): string {
  const text = readFileSync(join(root, path), "utf-8");
  if (path.endsWith(".json")) {
    return JSON.parse(text).app.security.csp as string;
  }
  const m = text.match(/http-equiv="Content-Security-Policy"\s+content="([^"]+)"/);
  if (!m) throw new Error(`CSP meta not found in ${path}`);
  return m[1];
}

describe("CSP allows plugin frontend loading", () => {
  for (const path of ["index.html", "src-tauri/tauri.conf.json", "src-tauri/tauri.dev.conf.json"]) {
    it(`${path}: script-src includes blob:`, () => {
      const csp = cspOf(path);
      const scriptSrc = csp.match(/script-src([^;]+)/)?.[1] ?? "";
      expect(scriptSrc).toContain("blob:");
    });
    it(`${path}: connect-src includes plugin-asset:`, () => {
      const csp = cspOf(path);
      const connectSrc = csp.match(/connect-src([^;]+)/)?.[1] ?? "";
      expect(connectSrc).toContain("plugin-asset:");
    });
  }
});
