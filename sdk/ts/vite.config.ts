import { defineConfig } from "vite";
import wasm from "vite-plugin-wasm";
import topLevelAwait from "vite-plugin-top-level-await";
import { resolve } from "path";

export default defineConfig({
  test: {
    globals: true,
    environment: "jsdom",
    include: ["src/**/*.test.ts"]
  },
  resolve: {
    alias: {
      "olayer-wasm": resolve(__dirname, "./wasm/pkg/olayer_wasm.js")
    }
  },
  plugins: [
    wasm(),
    topLevelAwait()
  ],
  build: {
    lib: {
      entry: resolve(__dirname, "src/index.ts"),
      name: "Olayer",
      fileName: "olayer",
      formats: ["es", "umd"]
    },
    rollupOptions: {
      external: [],
      output: {
        globals: {}
      }
    }
  },
  server: {
    port: 3000,
    fs: {
      allow: [
        resolve(__dirname, "../../")
      ]
    }
  }
});
