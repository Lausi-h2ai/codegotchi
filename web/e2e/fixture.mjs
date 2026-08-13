/* global URL, console, process */

import { createServer, request as proxyRequest } from "node:http";
import { connect } from "node:net";
import { createInterface } from "node:readline";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";

const webRoot = path.resolve(
    path.dirname(fileURLToPath(import.meta.url)),
    "..",
);
const repositoryRoot = path.resolve(webRoot, "..");
const port = Number(process.env.CODEGOTCHI_PLAYWRIGHT_PORT ?? "4173");
const fixtureModes = new Set([
    "default",
    "affection",
    "snack",
    "poop",
    "snack-poop",
    "strict-happiness",
    "overdue",
]);

let backend;
let backendPort;
let restartPromise = Promise.resolve();

await startBackend("default");

const proxy = createServer((request, response) => {
    const requestUrl = new URL(request.url ?? "/", "http://127.0.0.1");
    if (
        request.method === "POST" &&
        requestUrl.pathname === "/__fixture/reset"
    ) {
        void handleReset(requestUrl, request, response);
        return;
    }

    const upstream = proxyRequest(
        {
            hostname: "127.0.0.1",
            port: backendPort,
            method: request.method,
            path: request.url,
            headers: {
                ...request.headers,
                host: `127.0.0.1:${backendPort}`,
            },
        },
        (upstreamResponse) => {
            response.writeHead(
                upstreamResponse.statusCode ?? 502,
                upstreamResponse.headers,
            );
            upstreamResponse.pipe(response);
        },
    );
    upstream.on("error", () => response.destroy());
    request.pipe(upstream);
});

proxy.on("upgrade", (request, socket, head) => {
    const upstream = connect(backendPort, "127.0.0.1");
    upstream.once("connect", () => {
        const headers = request.rawHeaders;
        const requestLines = [`${request.method} ${request.url} HTTP/1.1`];
        for (let index = 0; index < headers.length; index += 2) {
            const name = headers[index];
            const value = headers[index + 1];
            requestLines.push(
                `${name}: ${name.toLowerCase() === "host" ? `127.0.0.1:${backendPort}` : value}`,
            );
        }
        upstream.write(`${requestLines.join("\r\n")}\r\n\r\n`);
        if (head.length > 0) {
            upstream.write(head);
        }
        socket.pipe(upstream);
        upstream.pipe(socket);
    });
    const closeBoth = () => {
        socket.destroy();
        upstream.destroy();
    };
    upstream.on("error", closeBoth);
    socket.on("error", closeBoth);
});

await new Promise((resolve, reject) => {
    proxy.once("error", reject);
    proxy.listen(port, "127.0.0.1", resolve);
});
console.log(`TASK3_FIXTURE_WEB_READY http://127.0.0.1:${port}`);

let shuttingDown = false;
async function shutdown() {
    if (shuttingDown) {
        return;
    }
    shuttingDown = true;
    await new Promise((resolve) => proxy.close(resolve));
    await stopBackend();
}

process.once("SIGINT", () => void shutdown());
process.once("SIGTERM", () => void shutdown());
process.once("exit", () => {
    if (backend && !backend.killed) {
        backend.kill("SIGTERM");
    }
});

async function handleReset(requestUrl, request, response) {
    request.resume();
    const mode = requestUrl.searchParams.get("mode") ?? "default";
    if (!fixtureModes.has(mode)) {
        response.writeHead(400, { "content-type": "application/json" });
        response.end(
            JSON.stringify({ error: `unsupported fixture mode: ${mode}` }),
        );
        return;
    }

    try {
        await restartBackend(mode);
        response.writeHead(200, { "content-type": "application/json" });
        response.end(JSON.stringify({ mode }));
    } catch (error) {
        response.writeHead(500, { "content-type": "application/json" });
        response.end(
            JSON.stringify({
                error: error instanceof Error ? error.message : String(error),
            }),
        );
    }
}

function restartBackend(mode) {
    const next = restartPromise.then(async () => {
        await stopBackend();
        await startBackend(mode);
    });
    restartPromise = next.catch(() => {});
    return next;
}

async function startBackend(mode) {
    const child = spawn(
        "cargo",
        [
            "run",
            "--quiet",
            "--package",
            "codegotchi-cli",
            "--example",
            "task3_fixture",
            "--",
            mode,
        ],
        {
            cwd: repositoryRoot,
            env: {
                ...process.env,
                CODEGOTCHI_PLAYWRIGHT_MODE: mode,
            },
            stdio: ["ignore", "pipe", "inherit"],
        },
    );
    backend = child;
    const backendInfo = await waitForBackend(child);
    backendPort = new URL(backendInfo.baseUrl).port;
}

async function stopBackend() {
    const child = backend;
    backend = undefined;
    backendPort = undefined;
    if (!child || child.exitCode !== null) {
        return;
    }

    await new Promise((resolve) => {
        let settled = false;
        const timer = globalThis.setTimeout(() => {
            if (settled) {
                return;
            }
            child.kill("SIGKILL");
            finish();
        }, 5_000);
        const finish = () => {
            if (settled) {
                return;
            }
            settled = true;
            globalThis.clearTimeout(timer);
            resolve();
        };
        child.once("exit", finish);
        child.kill("SIGTERM");
    });
}

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
