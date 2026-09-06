import { defineConfig } from "vite";
import tailwindcss from "@tailwindcss/vite";
import vue from "@vitejs/plugin-vue";

export default defineConfig({
  plugins: [vue(), tailwindcss()],
  resolve: {
    alias: {
      "@": `${import.meta.dirname}/src`,
    },
  },
  server: {
    host: "127.0.0.1",
    port: 12680,
    strictPort: true,
    proxy: {
      "/api": "http://127.0.0.1:12681",
      "/health": "http://127.0.0.1:12681",
    },
  },
});
