// Shared by Pi and OhMyPi. Uses only Node built-ins and their common extension API.
import { spawn } from "node:child_process";
import { randomUUID } from "node:crypto";

export default function codegotchi(pi) {
  const pending = new Map();
  const executable = process.env.CODEGOTCHI_EXECUTABLE;
  const harness = process.env.CODEGOTCHI_HARNESS;
  let turnId = randomUUID();
  let sessionId = randomUUID();

  async function notify(hook_event_name, ctx, fields = {}) {
    if (!executable || !process.env.CODEGOTCHI_SESSION_FILE) return {};
    const payload = JSON.stringify({
      hook_event_name, session_id: sessionId, turn_id: turnId,
      event_id: randomUUID(), ...fields,
    });
    // Do not retain raw prompts, file contents, or tool output in the bridge.
    if (Buffer.byteLength(payload) > 1024 * 1024) return {};
    return new Promise((resolve) => {
      const child = spawn(executable, ["hook", harness], {
        stdio: ["pipe", "pipe", "ignore"], env: process.env,
      });
      let output = "";
      const timer = setTimeout(() => { child.kill("SIGTERM"); resolve({}); }, 2500);
      child.on("error", () => { clearTimeout(timer); resolve({}); });
      child.stdout.on("data", (bytes) => {
        output += bytes;
        if (output.length > 65536) { child.kill("SIGTERM"); clearTimeout(timer); resolve({}); }
      });
      child.on("close", () => {
        clearTimeout(timer);
        try { resolve(JSON.parse(output)); } catch { resolve({}); }
      });
      child.stdin.on("error", () => {});
      child.stdin.end(payload);
    });
  }

  pi.on("session_start", async (_event, ctx) => {
    sessionId = ctx.sessionManager?.getSessionId?.() || randomUUID();
    pending.clear();
    await notify("SessionStart", ctx);
  });
  pi.on("session_shutdown", async (_event, ctx) => {
    await notify("SessionEnd", ctx);
    pending.clear();
  });
  pi.on("agent_start", async (_event, ctx) => {
    turnId = randomUUID();
    await notify("UserPromptSubmit", ctx);
  });
  pi.on("agent_end", async (_event, ctx) => { await notify("Stop", ctx); });
  pi.on("tool_call", async (event, ctx) => {
    // Only shell command text is needed transiently for conservative classification.
    const input = typeof event.input?.command === "string" ? { command: event.input.command } : {};
    if (pending.size >= 1024) pending.delete(pending.keys().next().value);
    pending.set(event.toolCallId, { input, started: Date.now() });
    const response = await notify("PreToolUse", ctx, {
      tool_name: event.toolName, tool_use_id: event.toolCallId, tool_input: input,
    });
    if (response.block) {
      pending.delete(event.toolCallId);
      return { block: true, reason: response.reason };
    }
  });
  pi.on("tool_result", async (event, ctx) => {
    const call = pending.get(event.toolCallId);
    pending.delete(event.toolCallId);
    const details = event.details || {};
    // isError is a reliable outcome; prefer an actual exit code when exposed.
    const exit_code = Number.isInteger(details.exitCode) ? details.exitCode
      : Number.isInteger(details.exit_code) ? details.exit_code : event.isError ? 1 : 0;
    await notify("PostToolUse", ctx, {
      tool_name: event.toolName, tool_use_id: event.toolCallId, tool_input: call?.input || {},
      tool_response: { exit_code, duration_ms: call ? Math.max(0, Date.now() - call.started) : undefined },
    });
  });
}
