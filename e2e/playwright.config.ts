import { defineConfig } from "@playwright/test";
import { execSync } from "node:child_process";
import path from "node:path";
import fs from "node:fs";

// E2E uses synthetic staging under .staging/e2e-playwright, populated before server start.
// WebServer command is a PowerShell script that recreates staging and starts xingshu-server.
const E2E_STAGING = path.resolve("..", ".staging", "e2e-playwright");
const E2E_DB = path.join(E2E_STAGING, "xingshu-test.db");

// Ensure .playwright-cli output dir exists and is gitignored
const pwCliDir = path.resolve("..", ".playwright-cli");
if (!fs.existsSync(pwCliDir)) fs.mkdirSync(pwCliDir, { recursive: true });

export default defineConfig({
  testDir: "./tests",
  timeout: 45000,
  expect: { timeout: 8000 },
  fullyParallel: false, // server state is shared, avoid parallel mutation
  workers: 1,
  retries: 0,
  use: {
    baseURL: "http://127.0.0.1:12681",
    headless: true,
    viewport: { width: 1280, height: 800 },
    screenshot: "only-on-failure",
    trace: "retain-on-failure",
    video: "retain-on-failure",
  },
  webServer: {
    command: `pwsh -NoProfile -File "${path.resolve("./scripts/start-e2e-server.ps1").replace(/\\/g, "/")}"`,
    url: "http://127.0.0.1:12681/health",
    timeout: 120000,
    reuseExistingServer: false,
    cwd: path.resolve(".."),
    env: {
      XINGSHU_DB: E2E_DB,
      XINGSHU_PORT: "12681",
      XINGSHU_HOST: "127.0.0.1",
      RUST_LOG: "info",
    },
    stdout: "pipe",
    stderr: "pipe",
  },
  projects: [
    { name: "chromium", use: { browserName: "chromium" } },
  ],
});
