import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";

export default defineConfig({
  plugins: [vue()],
  server: {
    port: 12680,
    strictPort: true,
    proxy: {
      "/api": "http://127.0.0.1:12681",
      "/health": "http://127.0.0.1:12681",
    },
  },
});
