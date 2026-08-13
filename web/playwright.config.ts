import { defineConfig } from "@playwright/test";

export default defineConfig({
    testDir: "./e2e",
    fullyParallel: false,
    forbidOnly: Boolean(process.env.CI),
    retries: process.env.CI ? 1 : 0,
    reporter: "list",
    timeout: 30_000,
    use: {
        baseURL: "http://127.0.0.1:4173",
        headless: true,
        trace: "retain-on-failure",
    },
    webServer: {
        command: "node e2e/fixture.mjs",
        url: "http://127.0.0.1:4173",
        reuseExistingServer: false,
        gracefulShutdown: {
            signal: "SIGTERM",
            timeout: 10_000,
        },
        timeout: 120_000,
    },
});
