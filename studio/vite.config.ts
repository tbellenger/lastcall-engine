import { defineConfig } from "vite";

export default defineConfig({
  base: "./",
  build: { target: "es2022" },
  server: { host: "127.0.0.1", port: 5180 },
  preview: { host: "127.0.0.1", port: 5180 },
});
