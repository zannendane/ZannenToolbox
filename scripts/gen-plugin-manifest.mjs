#!/usr/bin/env node
/**
 * 生成插件更新清单片段（per-target）：读取 .znplugin 与其 .sig，
 * 产出 {version, target, file:{url, sha256, signature}} JSON。
 *
 * 用法: node scripts/gen-plugin-manifest.mjs <pluginId> <target> <pkg.znplugin> <baseUrl>
 *   <baseUrl> 为分发仓库滚动 Release 的下载基址，如
 *   https://github.com/<owner>/<dist-repo>/releases/download/plugins-latest
 */
import { createHash } from "node:crypto";
import fs from "node:fs";
import path from "node:path";

const [pluginId, target, pkgPath, baseUrl] = process.argv.slice(2);
if (!pluginId || !target || !pkgPath || !baseUrl) {
  console.error(
    "usage: gen-plugin-manifest.mjs <pluginId> <target> <pkg.znplugin> <baseUrl>",
  );
  process.exit(1);
}

const pkg = fs.readFileSync(pkgPath);
const sha256 = createHash("sha256").update(pkg).digest("hex");
const signature = fs.readFileSync(`${pkgPath}.sig`, "utf-8").trim();
const fileName = path.basename(pkgPath);
// 版本号取自包名约定：<id>-<version>-<os>-<arch>.znplugin
const m = fileName.match(new RegExp(`^${pluginId.replace(".", "\\.")}-(.+)-[^-]+-[^-]+\\.znplugin$`));
if (!m) {
  console.error(`package name does not carry version: ${fileName}`);
  process.exit(1);
}

const fragment = {
  id: pluginId,
  version: m[1],
  target,
  file: {
    url: `${baseUrl.replace(/\/$/, "")}/${fileName}`,
    sha256,
    signature,
    bytes: pkg.length,
  },
};
process.stdout.write(JSON.stringify(fragment, null, 2) + "\n");
