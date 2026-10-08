#!/usr/bin/env node
// Exercises the production launch, generated adapters, hook binary, and runtime.
import assert from "node:assert/strict";
import { readFileSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { pathToFileURL } from "node:url";
import { randomUUID } from "node:crypto";

const harness = process.env.CODEGOTCHI_HARNESS;
const executable = process.env.CODEGOTCHI_EXECUTABLE;
const args = process.argv.slice(2);
assert(!args.includes("--profile"), "no Codex profile passed to another harness");
const session = randomUUID();
const handlers = new Map();
const ctx = { sessionManager: { getSessionId: () => session } };
let settings;
if (harness === "pi" || harness === "omp") {
  assert.equal(args[0], "--extension");
  const extension = await import(pathToFileURL(args[1]).href);
  extension.default({ on: (name, handler) => handlers.set(name, handler) });
} else if (harness === "claude") {
  assert.equal(args[0], "--settings");
  settings = JSON.parse(readFileSync(args[1], "utf8"));
}

async function control(...arguments_) {
  let result;
  for (let attempt = 0; attempt < 4; attempt++) {
    result = spawnSync(executable, arguments_, { env: process.env, encoding: "utf8" });
    if (result.status === 0 || !result.stderr.includes("HTTP transport failed")) break;
    await new Promise(resolve => setTimeout(resolve, 100));
  }
  assert.equal(result.status, 0, result.stderr);
}
async function hook(name, fields = {}) {
  const payload = { hook_event_name: name, session_id: session, ...fields };
  if (harness === "pi" || harness === "omp") {
    const names = { SessionStart: "session_start", SessionEnd: "session_shutdown", UserPromptSubmit: "agent_start", Stop: "agent_end", PreToolUse: "tool_call", PostToolUse: "tool_result" };
    const response = await handlers.get(names[name])({ toolName: fields.tool_name, toolCallId: fields.tool_use_id, input: fields.tool_input, isError: fields.tool_response?.exit_code !== 0, details: { exitCode: fields.tool_response?.exit_code } }, ctx);
    return response || {};
  }
  let result;
  if (harness === "claude") {
    result = spawnSync(settings.hooks[name][0].hooks[0].command, { shell: true, input: JSON.stringify(payload), encoding: "utf8", env: process.env });
  } else {
    const names = { SessionStart: "on_session_start", UserPromptSubmit: "on_session_start", SessionEnd: "on_session_end", Stop: "on_session_end", PreToolUse: "pre_tool_call", PostToolUse: "post_tool_call" };
    payload.hook_event_name = names[name];
    if (payload.tool_name === "Bash") payload.tool_name = "terminal";
    payload.extra = { tool_call_id: fields.tool_use_id, result: JSON.stringify(fields.tool_response), duration_ms: 10 };
    result = spawnSync(executable, ["hook", "hermes"], { input: JSON.stringify(payload), encoding: "utf8", env: process.env });
  }
  assert.equal(result.status, 0, result.stderr);
  return JSON.parse(result.stdout);
}
function blocked(response) { return response.block || response.action === "block" || response.hookSpecificOutput?.permissionDecision === "deny"; }
function tool(name, id = randomUUID(), command = "cargo test") {
  return { tool_name: name, tool_use_id: id, tool_input: { command } };
}

const metadata = JSON.parse(readFileSync(process.env.CODEGOTCHI_SESSION_FILE, "utf8"));
async function state() {
  const response = await fetch(`${metadata.loopbackBaseUrl}/api/v1/state`, { headers: { Authorization: `Bearer ${metadata.bearerToken}` } });
  assert(response.ok);
  return response.json();
}
if (process.env.CODEGOTCHI_ADAPTER_VISUAL === "1") {
  let active;
  let chain = Promise.resolve();
  const acknowledgement = process.env.CODEGOTCHI_ADAPTER_ACK;
  function draw(label) {
    process.stdout.write(`\x1b[2J\x1b[HCodeGotchi ${harness} adapter verification fixture\r\n\r\nState: ${label}\r\n\r\nReal launcher / PTY / hook bridge / authoritative pet runtime\r\nKeys: t thinking, w work, s success, e error, v waiting, b blocked, q exit\r\n`);
    if (acknowledgement) writeFileSync(acknowledgement, label);
  }
  if (harness !== "hermes") await hook("SessionStart");
  draw("idle");
  process.stdin.setRawMode?.(true);
  process.stdin.resume();
  process.stdin.on("data", bytes => {
    for (const key of bytes.toString()) {
      chain = chain.then(async () => {
        if (key === "q" || key === "\x03") { await hook("SessionEnd"); process.exit(0); }
        if (key === "t") { await hook("UserPromptSubmit"); draw("thinking"); }
        if (key === "w") { active = tool("Bash"); await hook("PreToolUse", active); draw("working"); }
        if (key === "s" || key === "e") {
          if (!active) { active = tool("Bash"); await hook("PreToolUse", active); }
          await hook("PostToolUse", { ...active, tool_response: { exit_code: key === "s" ? 0 : 1 } });
          const snapshot = await state();
          assert.equal(snapshot.recentOutcome, key === "s" ? "Success" : "Failure");
          active = undefined; draw(key === "s" ? "success" : "failure");
        }
        if (key === "v") { await hook("Stop"); draw("waiting"); }
        if (key === "b") {
          await control("mode", "strict"); await control("debug", "neglect");
          assert(blocked(await hook("PreToolUse", tool("Bash")))); draw("blocked");
        }
      }).catch(error => { console.error(error); process.exit(1); });
    }
  });
  await new Promise(() => {});
}
await hook("SessionStart");
await hook("UserPromptSubmit");
const first = tool("Bash");
assert(!blocked(await hook("PreToolUse", first)));
const duringWork = await state();
await hook("PostToolUse", { ...first, tool_response: { exit_code: 0 } });
await hook("Stop");
await control("mode", "strict");
await control("debug", "neglect");
const denial = await hook("PreToolUse", tool("Bash"));
assert(blocked(denial), "Strict mode must block a real pre-tool request");
const editName = harness === "claude" ? "Write" : harness === "hermes" ? "patch" : "write";
assert(blocked(await hook("PreToolUse", tool(editName))), "Strict mode must block file edits");
assert(!blocked(await hook("PreToolUse", tool("Bash", randomUUID(), "codegotchi mode decorative"))), "care control stays allowed even when neglected");
const duringBlock = await state();
await control("mode", "decorative");
const second = tool(editName);
assert(!blocked(await hook("PreToolUse", second)));
await hook("PostToolUse", { ...second, tool_response: { exit_code: 1 } });
const duringError = await state();
await hook("Stop");
await hook("SessionEnd");
if (process.env.CODEGOTCHI_ADAPTER_REPORT) {
  writeFileSync(process.env.CODEGOTCHI_ADAPTER_REPORT, JSON.stringify({ harness, arguments: harness === "hermes" ? args : args.slice(2), strictBlocked: true, duringWork: duringWork.activity, duringBlock: duringBlock.activity, duringError: duringError.recentOutcome }));
}
console.log(`HARNESS ${harness}: work, Strict denial, care control, file edits, and error verified`);
process.exit(Number(process.env.CODEGOTCHI_ADAPTER_EXIT || 0));
