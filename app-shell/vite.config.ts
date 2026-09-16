import { resolve } from "node:path";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

/**
 * 壳前端构建。
 *
 * 关键点：vendor/* 多入口以固定文件名输出，作为"共享模块 URL"，
 * 运行时供插件前端经 import 重写引用（react / sdk / framer-motion 等
 * 与壳保持同一模块实例，避免 Invalid hook call 类问题）。
 *
 * - 开发态：vite 原生服务 /src/vendor/*.ts
 * - 产物态：/assets/vendor/*.js（无 hash，便于壳计算固定 URL）
 */

const vendorEntries = [
  "vendor/react",
  "vendor/react-jsx-runtime",
  "vendor/react-dom",
  "vendor/zannen-plugin-sdk",
  "vendor/tauri-core",
  "vendor/tauri-events",
  "vendor/tauri-window",
  "vendor/tauri-plugin-dialog",
  "vendor/framer-motion",
] as const;

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  build: {
    target: "es2022",
    sourcemap: false,
    chunkSizeWarningLimit: 900,
    rollupOptions: {
      // vendor 入口在运行时被插件按名 import：禁用内部导出压缩，保留完整具名导出
      preserveEntrySignatures: "strict",
      input: {
        main: resolve(__dirname, "index.html"),
        ...Object.fromEntries(
          vendorEntries.map((name) => [name, resolve(__dirname, `src/${name}.ts`)]),
        ),
      },
      output: {
        minifyInternalExports: false,
        entryFileNames: (chunk) =>
          chunk.name.startsWith("vendor/") ? "assets/[name].js" : "assets/[name]-[hash].js",
      },
    },
  },
});
