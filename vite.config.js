/// <reference types="vitest" />
import { defineConfig } from "vite";

export default defineConfig({
  base: "./",
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
  build: {
    target: "es2022",
    minify: "esbuild",
  },
  test: {
    // jsdom matches the browser's HTML parser closely enough that DOMPurify
    // produces the same structural output as in the Tauri webview. happy-dom
    // mangles <pre>/<table> wrappers on reparse so the security assertions
    // pass but structural ones don't.
    environment: "jsdom",
    include: ["src/**/*.test.js"],
  },
});
