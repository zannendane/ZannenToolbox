#!/usr/bin/env node
// 插件包（.znplugin）签名工具：插件目录 → zip → ed25519 签名 → .znplugin + .sig
//
// 用法：
//   node scripts/sign-plugin.mjs <插件目录|包.zip> [输出路径]
//   node scripts/sign-plugin.mjs --print-pubkey-hex   # 打印验签公钥 hex（嵌入壳内）
//   node scripts/sign-plugin.mjs --help
//
// 密钥（首次运行自动生成并提示）：
//   ~/.zannen/keys/plugin-ed25519.pem      私钥（PKCS8 PEM，权限 0600，务必备份、勿入库）
//   ~/.zannen/keys/plugin-ed25519.pub.pem  公钥（SPKI PEM）
// 公钥 hex 需嵌入 crates/zannen-core/src/plugin_installer.rs 的 PLUGIN_SIGNING_PUBKEY_HEX。
//
// 签名约定：对 .znplugin（zip）文件的完整字节做 ed25519 签名；
// .sig 为签名的 hex 文本。成功后 stdout 输出一行 JSON（供更新清单使用）：
//   {"sha256":"…","signature":"…","bytes":N}
// 其余提示信息一律走 stderr，便于脚本解析 stdout。

import { execFileSync } from "node:child_process";
import {
  createHash,
  createPrivateKey,
  createPublicKey,
  generateKeyPairSync,
  sign,
} from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

const KEY_DIR = path.join(os.homedir(), ".zannen", "keys");
const PRIV_PATH = path.join(KEY_DIR, "plugin-ed25519.pem");
const PUB_PATH = path.join(KEY_DIR, "plugin-ed25519.pub.pem");

// ed25519 的 SPKI DER 头固定为这 12 字节，其后 32 字节即原始公钥
const SPKI_ED25519_PREFIX = Buffer.from("302a300506032b6570032100", "hex");

const USAGE = `用法：
  node scripts/sign-plugin.mjs <插件目录|包.zip> [输出路径]   打包并签名
  node scripts/sign-plugin.mjs --print-pubkey-hex             打印验签公钥 hex
  node scripts/sign-plugin.mjs --help                         显示本说明

密钥：~/.zannen/keys/plugin-ed25519.pem（不存在则自动生成）。
输出：<名称>.znplugin 与 <名称>.znplugin.sig（hex 签名），
stdout 最后一行输出 JSON：{"sha256":"…","signature":"…","bytes":N}`;

/** 读取密钥对，不存在则生成（目录 0700，私钥 0600）。 */
function loadOrCreateKeys() {
  if (!fs.existsSync(PRIV_PATH)) {
    fs.mkdirSync(KEY_DIR, { recursive: true, mode: 0o700 });
    const { privateKey, publicKey } = generateKeyPairSync("ed25519");
    fs.writeFileSync(
      PRIV_PATH,
      privateKey.export({ type: "pkcs8", format: "pem" }),
      { mode: 0o600 },
    );
    fs.writeFileSync(PUB_PATH, publicKey.export({ type: "spki", format: "pem" }));
    console.error(`[sign-plugin] 已生成新密钥对：${PRIV_PATH}`);
    console.error(
      `[sign-plugin] 公钥 hex（嵌入 crates/zannen-core/src/plugin_installer.rs 的 PLUGIN_SIGNING_PUBKEY_HEX）：\n  ${pubkeyHex()}`,
    );
  }
  return {
    privateKey: createPrivateKey(fs.readFileSync(PRIV_PATH)),
    publicKey: createPublicKey(fs.readFileSync(PUB_PATH)),
  };
}

/** 从 SPKI 公钥导出 32 字节原始公钥的 hex。 */
function pubkeyHex() {
  const der = createPublicKey(fs.readFileSync(PUB_PATH)).export({
    type: "spki",
    format: "der",
  });
  if (!der.subarray(0, 12).equals(SPKI_ED25519_PREFIX) || der.length !== 44) {
    throw new Error(`公钥不是 ed25519 SPKI 格式：${PUB_PATH}`);
  }
  return der.subarray(12).toString("hex");
}

/** 递归收集目录内文件（相对路径），排除 .DS_Store，按路径排序保证可复现。 */
function collectFiles(dir, prefix = "") {
  const out = [];
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const rel = prefix ? `${prefix}/${entry.name}` : entry.name;
    if (entry.name === ".DS_Store") continue;
    if (entry.isDirectory()) {
      out.push(...collectFiles(path.join(dir, entry.name), rel));
    } else if (entry.isFile()) {
      out.push(rel);
    } else {
      console.error(`[sign-plugin] 跳过非常规文件：${rel}`);
    }
  }
  return out.sort();
}

/** 用系统 zip 打包目录：文件顺序已排序，-X 去除平台额外属性。 */
function zipDir(srcDir, zipPath) {
  const files = collectFiles(srcDir);
  if (!files.includes("plugin.toml")) {
    throw new Error(`插件目录缺少 plugin.toml：${srcDir}`);
  }
  fs.rmSync(zipPath, { force: true });
  execFileSync("zip", ["-q", "-X", zipPath, "--", ...files], { cwd: srcDir });
}

/** 归一化输出路径：保证以 .znplugin 结尾。 */
function znpluginPath(outPath, fallbackBase) {
  if (!outPath) return path.resolve(`${fallbackBase}.znplugin`);
  const abs = path.resolve(outPath);
  if (abs.endsWith(".znplugin")) return abs;
  if (abs.endsWith(".zip")) return `${abs.slice(0, -4)}.znplugin`;
  return `${abs}.znplugin`;
}

function main() {
  const args = process.argv.slice(2);
  if (args.includes("--help") || args.includes("-h")) {
    console.log(USAGE);
    return;
  }
  if (args[0] === "--print-pubkey-hex") {
    loadOrCreateKeys();
    console.log(pubkeyHex());
    return;
  }

  const input = args[0];
  if (!input) {
    console.error(USAGE);
    process.exit(1);
  }
  const inputAbs = path.resolve(input);
  if (!fs.existsSync(inputAbs)) {
    throw new Error(`输入不存在：${inputAbs}`);
  }

  const finalPath = znpluginPath(
    args[1],
    path.basename(inputAbs).replace(/\.(zip|znplugin)$/i, ""),
  );
  fs.mkdirSync(path.dirname(finalPath), { recursive: true });

  // 得到 zip 字节：目录先打包（临时文件，避免把产物打进包内），zip 文件直接复制
  let zipPath;
  if (fs.statSync(inputAbs).isDirectory()) {
    zipPath = path.join(
      os.tmpdir(),
      `zannen-sign-${process.pid}-${Date.now()}.zip`,
    );
    zipDir(inputAbs, zipPath);
  } else {
    zipPath = inputAbs;
  }

  try {
    const bytes = fs.readFileSync(zipPath);
    const { privateKey } = loadOrCreateKeys();
    const signature = sign(null, bytes, privateKey).toString("hex");
    const sha256 = createHash("sha256").update(bytes).digest("hex");

    fs.copyFileSync(zipPath, finalPath);
    fs.writeFileSync(`${finalPath}.sig`, `${signature}\n`);

    console.error(`[sign-plugin] 产物：${finalPath}`);
    console.error(`[sign-plugin] 签名：${finalPath}.sig`);
    console.log(JSON.stringify({ sha256, signature, bytes: bytes.length }));
  } finally {
    if (zipPath !== inputAbs) fs.rmSync(zipPath, { force: true });
  }
}

try {
  main();
} catch (e) {
  console.error(`[sign-plugin] 失败：${e.message}`);
  process.exit(1);
}
