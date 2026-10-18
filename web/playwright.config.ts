import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "e2e",
  use: { baseURL: "http://127.0.0.1:4173" },
  webServer: [
    {
      command: "npm run preview -- --host 127.0.0.1 --port 4173",
      port: 4173,
      reuseExistingServer: false,
    },
    {
      command: "node e2e/host-server.mjs",
      port: 4174,
      reuseExistingServer: false,
    },
  ],
});
