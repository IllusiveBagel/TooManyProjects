import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    port: 5173,
    // The Rust server runs on 8080 in development (`cargo run` in ../server).
    proxy: { "/api": "http://localhost:8080" },
    fs: { allow: [".."] },
  },
  build: { outDir: "dist", sourcemap: true },
});
