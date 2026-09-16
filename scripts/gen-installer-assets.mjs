#!/usr/bin/env node
/**
 * 程序化生成安装器视觉资产（零二进制资产入库）：
 *   app-shell/src-tauri/installer/sidebar.bmp        164x314  NSIS 欢迎/完成页侧栏
 *   app-shell/src-tauri/installer/header.bmp         150x57   NSIS 页眉横幅
 *   app-shell/src-tauri/installer/dmg-background.png 660x400  DMG 拖放背景
 *
 * 视觉语言与设计令牌一致：深色底 + 品牌渐变（#6e8bff→#9b7bff）+ Z 条带。
 * 用法: node scripts/gen-installer-assets.mjs
 */

import { deflateSync } from "node:zlib";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const OUT = join(ROOT, "app-shell/src-tauri/installer");

const ACCENT = [110, 139, 255];
const ACCENT2 = [155, 123, 255];
const BASE0 = [11, 13, 19];
const BASE1 = [22, 26, 36];

const lerp = (a, b, t) => a + (b - a) * t;
const clamp01 = (t) => Math.min(1, Math.max(0, t));

/** 基础画布：竖向暗色渐变 + 两处品牌光晕。 */
function makeCanvas(w, h, { glowA = 0.55, glowB = 0.35 } = {}) {
  const px = new Uint8Array(w * h * 4);
  const glow = (x, y, cx, cy, r) => {
    const d = Math.hypot(x - cx, y - cy) / r;
    return clamp01(1 - d) ** 2;
  };
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const t = y / h;
      let r = lerp(BASE1[0], BASE0[0], t);
      let g = lerp(BASE1[1], BASE0[1], t);
      let b = lerp(BASE1[2], BASE0[2], t);
      const ga = glow(x, y, w * 0.5, h * 0.3, w * 0.75) * glowA;
      const gb = glow(x, y, w * 0.6, h * 0.85, w * 0.6) * glowB;
      r += ACCENT[0] * ga * 0.5 + ACCENT2[0] * gb * 0.4;
      g += ACCENT[1] * ga * 0.5 + ACCENT2[1] * gb * 0.4;
      b += ACCENT[2] * ga * 0.5 + ACCENT2[2] * gb * 0.4;
      const i = (y * w + x) * 4;
      px[i] = r;
      px[i + 1] = g;
      px[i + 2] = b;
      px[i + 3] = 255;
    }
  }
  return px;
}

/** 在画布上画居中的 Z 条带（与应用图标同形）。 */
function drawZ(px, w, h, box, stroke, colorA = ACCENT, colorB = ACCENT2) {
  const m = box.x + box.size * 0.18;
  const right = box.x + box.size - box.size * 0.18;
  const top = box.y + box.size * 0.2;
  const bottom = box.y + box.size - box.size * 0.2;
  const half = stroke / 2;
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      let inside = false;
      // 上横 / 下横
      if (x >= m && x <= right && ((y >= top && y <= top + stroke) || (y >= bottom - stroke && y <= bottom)))
        inside = true;
      // 对角线
      if (!inside) {
        const x1 = right, y1 = top + half;
        const x2 = m, y2 = bottom - half;
        const dx = x2 - x1, dy = y2 - y1;
        const t = clamp01(((x - x1) * dx + (y - y1) * dy) / (dx * dx + dy * dy));
        if (Math.hypot(x - (x1 + t * dx), y - (y1 + t * dy)) <= half) inside = true;
      }
      if (inside) {
        const i = (y * w + x) * 4;
        const t = (x + y) / (w + h);
        px[i] = lerp(colorA[0], colorB[0], t);
        px[i + 1] = lerp(colorA[1], colorB[1], t);
        px[i + 2] = lerp(colorA[2], colorB[2], t);
      }
    }
  }
}

/** 画圆环（DMG 图标光晕位）。 */
function drawRing(px, w, h, cx, cy, r, thickness, color, alpha = 0.8) {
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const d = Math.abs(Math.hypot(x - cx, y - cy) - r);
      if (d <= thickness / 2) {
        const i = (y * w + x) * 4;
        const a = alpha * (1 - d / (thickness / 2));
        px[i] = lerp(px[i], color[0], a);
        px[i + 1] = lerp(px[i + 1], color[1], a);
        px[i + 2] = lerp(px[i + 2], color[2], a);
      }
    }
  }
}

/** 画右箭头（DMG 拖放引导）：矩形箭杆 + 三角箭头。 */
function drawArrow(px, w, h, x0, x1, cy, thickness, color, alpha = 0.75) {
  const headLen = 26;
  const headHalf = 17;
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      let inside = false;
      if (x >= x0 && x <= x1 - headLen && Math.abs(y - cy) <= thickness / 2) inside = true;
      if (x > x1 - headLen && x <= x1) {
        const t = (x - (x1 - headLen)) / headLen;
        if (Math.abs(y - cy) <= headHalf * (1 - t)) inside = true;
      }
      if (inside) {
        const i = (y * w + x) * 4;
        px[i] = lerp(px[i], color[0], alpha);
        px[i + 1] = lerp(px[i + 1], color[1], alpha);
        px[i + 2] = lerp(px[i + 2], color[2], alpha);
      }
    }
  }
}

// ---------- PNG 编码（8bit RGBA + deflate） ----------
function crc32(buf) {
  let c;
  if (!crc32.table) {
    crc32.table = new Int32Array(256);
    for (let n = 0; n < 256; n++) {
      c = n;
      for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
      crc32.table[n] = c;
    }
  }
  c = -1;
  for (let i = 0; i < buf.length; i++) c = (c >>> 8) ^ crc32.table[(c ^ buf[i]) & 0xff];
  return (c ^ -1) >>> 0;
}

function pngChunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([len, body, crc]);
}

function encodePng(px, w, h) {
  const raw = Buffer.alloc((w * 4 + 1) * h);
  for (let y = 0; y < h; y++) {
    raw[y * (w * 4 + 1)] = 0;
    Buffer.from(px.buffer, y * w * 4, w * 4).copy(raw, y * (w * 4 + 1) + 1);
  }
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(w, 0);
  ihdr.writeUInt32BE(h, 4);
  ihdr[8] = 8;
  ihdr[9] = 6;
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    pngChunk("IHDR", ihdr),
    pngChunk("IDAT", deflateSync(raw, { level: 9 })),
    pngChunk("IEND", Buffer.alloc(0)),
  ]);
}

// ---------- BMP 编码（24-bit，bottom-up，行 4 字节对齐） ----------
function encodeBmp24(px, w, h) {
  const rowSize = Math.ceil((w * 3) / 4) * 4;
  const pixelSize = rowSize * h;
  const file = Buffer.alloc(54 + pixelSize);
  file.write("BM");
  file.writeUInt32LE(54 + pixelSize, 2);
  file.writeUInt32LE(54, 10);
  file.writeUInt32LE(40, 14); // BITMAPINFOHEADER
  file.writeInt32LE(w, 18);
  file.writeInt32LE(h, 22);
  file.writeUInt16LE(1, 26); // planes
  file.writeUInt16LE(24, 28); // bpp
  file.writeUInt32LE(pixelSize, 34);
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const src = (y * w + x) * 4;
      const dst = 54 + (h - 1 - y) * rowSize + x * 3; // bottom-up, BGR
      file[dst] = px[src + 2];
      file[dst + 1] = px[src + 1];
      file[dst + 2] = px[src];
    }
  }
  return file;
}

// ---------- 生成 ----------
mkdirSync(OUT, { recursive: true });

// NSIS 侧栏 164x314：暗色渐变 + 居中 Z
{
  const w = 164, h = 314;
  const px = makeCanvas(w, h);
  drawZ(px, w, h, { x: w * 0.14, y: h * 0.32, size: w * 0.72 }, w * 0.11);
  writeFileSync(join(OUT, "sidebar.bmp"), encodeBmp24(px, w, h));
  console.log(`sidebar.bmp ${w}x${h}`);
}

// NSIS 页眉 150x57：暗色渐变 + 右下微光
{
  const w = 150, h = 57;
  const px = makeCanvas(w, h, { glowA: 0.3, glowB: 0.25 });
  writeFileSync(join(OUT, "header.bmp"), encodeBmp24(px, w, h));
  console.log(`header.bmp ${w}x${h}`);
}

// DMG 背景 660x400：渐变底 + 左右图标光晕环 + 中央箭头
{
  const w = 660, h = 400;
  const px = makeCanvas(w, h, { glowA: 0.2, glowB: 0.18 });
  const cy = 190;
  drawRing(px, w, h, 180, cy, 74, 3, ACCENT, 0.5); // 应用图标位
  drawRing(px, w, h, 480, cy, 74, 3, [255, 255, 255], 0.25); // Applications 位
  drawArrow(px, w, h, 270, 392, cy, 4, [235, 238, 248], 0.7);
  writeFileSync(join(OUT, "dmg-background.png"), encodePng(px, w, h));
  console.log(`dmg-background.png ${w}x${h}`);
}

console.log(`installer assets written to ${OUT}`);
