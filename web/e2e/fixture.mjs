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
const backendPort = new URL(backendInfo.baseUrl).port;
const proxy = createServer((request, response) => {
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
