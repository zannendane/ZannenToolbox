#!/usr/bin/env node
/**
 * 合并各平台清单片段为完整插件更新清单（files 按 target 归并）：
 * 读入全部 fragment-*.json，按插件 id 分组写出 manifest-<id>.json。
 *
 * 用法: node scripts/merge-plugin-manifests.mjs <片段目录> <输出目录>
 */
import fs from "node:fs";
import path from "node:path";

const [fragDir, outDir] = process.argv.slice(2);
if (!fragDir || !outDir) {
  console.error("usage: merge-plugin-manifests.mjs <fragDir> <outDir>");
  process.exit(1);
}

const byId = new Map();
for (const name of fs.readdirSync(fragDir)) {
  if (!name.startsWith("fragment-") || !name.endsWith(".json")) continue;
  const frag = JSON.parse(fs.readFileSync(path.join(fragDir, name), "utf-8"));
  const entry = byId.get(frag.id) ?? { version: frag.version, files: {} };
  if (entry.version !== frag.version) {
    console.error(`version mismatch for ${frag.id}: ${entry.version} vs ${frag.version}`);
    process.exit(1);
  }
  entry.files[frag.target] = frag.file;
  byId.set(frag.id, entry);
}

fs.mkdirSync(outDir, { recursive: true });
for (const [id, manifest] of byId) {
  const out = path.join(outDir, `manifest-${id}.json`);
  fs.writeFileSync(out, JSON.stringify(manifest, null, 2) + "\n");
  console.log(`written: ${out}`);
}
