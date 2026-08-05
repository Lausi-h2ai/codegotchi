/* global console, process */

import { createInterface } from "node:readline";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";

import { createServer } from "vite";

const webRoot = path.resolve(
    path.dirname(fileURLToPath(import.meta.url)),
    "..",
);
const repositoryRoot = path.resolve(webRoot, "..");
const port = Number(process.env.CODEGOTCHI_PLAYWRIGHT_PORT ?? "4173");

const backend = spawn(
    "cargo",
    [
        "run",
        "--quiet",
        "--package",
        "codegotchi-cli",
        "--example",
        "task3_fixture",
    ],
    {
        cwd: repositoryRoot,
        stdio: ["ignore", "pipe", "inherit"],
    },
);

const backendInfo = await waitForBackend(backend);
const vite = await createServer({
    root: webRoot,
    logLevel: "error",
    server: {
        host: "127.0.0.1",
        port,
        strictPort: true,
        proxy: {
            "/api": {
                target: backendInfo.baseUrl,
                changeOrigin: false,
                ws: true,
            },
        },
    },
});

await vite.listen();
console.log(`TASK3_FIXTURE_WEB_READY http://127.0.0.1:${port}`);

let shuttingDown = false;
async function shutdown() {
    if (shuttingDown) {
        return;
    }
    shuttingDown = true;
    await vite.close();
    backend.kill("SIGTERM");
}

process.once("SIGINT", () => void shutdown());
process.once("SIGTERM", () => void shutdown());
process.once("exit", () => {
    if (!backend.killed) {
        backend.kill("SIGTERM");
    }
});

async function waitForBackend(child) {
    return new Promise((resolve, reject) => {
        const lines = createInterface({ input: child.stdout });
        lines.on("line", (line) => {
            if (!line.startsWith("TASK3_FIXTURE_READY ")) {
                return;
            }
            lines.close();
            try {
                resolve(JSON.parse(line.slice("TASK3_FIXTURE_READY ".length)));
            } catch (error) {
                reject(error);
            }
        });
        child.once("error", reject);
        child.once("exit", (code) => {
            reject(
                new Error(
                    `Task 3 backend fixture exited before ready (${code})`,
                ),
            );
        });
    });
}
