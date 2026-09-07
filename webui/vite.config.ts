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
    port: Number(process.env.XINGSHU_WEB_PORT ?? 12680),
    strictPort: true,
    proxy: {
      "/api": `http://127.0.0.1:${process.env.XINGSHU_PORT ?? 12681}`,
      "/health": `http://127.0.0.1:${process.env.XINGSHU_PORT ?? 12681}`,
    },
  },
});
