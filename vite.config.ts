import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

const apiPort = Number(process.env.PORT ?? 4317);

export default defineConfig({
  root: "web",
  plugins: [react(), tailwindcss()],
  server: {
    port: 5317,
    proxy: { "/api": { target: `http://127.0.0.1:${apiPort}`, changeOrigin: true } },
  },
  build: { outDir: "../dist-web", emptyOutDir: true },
  test: { root: ".", include: ["server/**/*.test.ts", "web/src/**/*.test.ts"] },
} as never);
