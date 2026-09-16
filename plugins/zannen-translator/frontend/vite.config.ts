import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

/**
 * 插件前端构建：单文件 ESM，react/sdk/tauri/framer-motion 声明 external
 * （运行时由壳的 vendor URL 注入，保证模块实例唯一）。
 */
export default defineConfig({
  plugins: [react()],
  build: {
    target: "es2022",
    sourcemap: false,
    minify: "esbuild",
    cssCodeSplit: false,
    lib: {
      entry: "src/index.tsx",
      formats: ["es"],
      fileName: () => "index.js",
      cssFileName: "style",
    },
    rollupOptions: {
      external: [
        "react",
        "react/jsx-runtime",
        "react-dom",
        "react-dom/client",
        "@zannen/plugin-sdk",
        "@tauri-apps/api/core",
        "@tauri-apps/api/event",
        "@tauri-apps/api/window",
        "framer-motion",
      ],
    },
  },
});
