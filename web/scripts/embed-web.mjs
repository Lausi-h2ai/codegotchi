/* global console */

import { cp, rm } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const webRoot = path.resolve(
    path.dirname(fileURLToPath(import.meta.url)),
    "..",
);
const source = path.join(webRoot, "dist");
const destination = path.resolve(
    webRoot,
    "..",
    "crates",
    "codegotchi-cli",
    "web-dist",
);

await rm(destination, { recursive: true, force: true });
await cp(source, destination, { recursive: true });
console.log(`Copied ${source} to ${destination}`);
