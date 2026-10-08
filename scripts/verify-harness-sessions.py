#!/usr/bin/env python3
"""Exercise real Pi and Claude sessions through CodeGotchi in a private xterm.

This is deliberately a session-behavior probe rather than a provider test. The
CLIs are genuine installed binaries, but the model is a local scripted endpoint
and no provider credentials are read or used. It covers real keyboard approval
input where a harness provides it, interruption while a response is streaming,
multiple sustained turns, and a persisted conversation resumed in a fresh
CodeGotchi process. Screenshots are retained for manual visual inspection.

Requires xterm, Xvfb, openbox, xauth, xdotool, ImageMagick, and the real Pi and
Claude binaries. The run owns only its private display, windows, processes, and
tmpfs home; cleanup is scoped and verified.
"""

import argparse
import hashlib
import json
import os
import pathlib
import re
import secrets
import shutil
import socket
import subprocess
import sys
import tempfile
import threading
import time
import urllib.request
from http.server import ThreadingHTTPServer, BaseHTTPRequestHandler

ROOT = pathlib.Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument(
    "--binary", type=pathlib.Path, default=ROOT / "target/debug/codegotchi"
)
parser.add_argument(
    "--output",
    type=pathlib.Path,
    default=ROOT / "docs/verification/multi-harness/native-sessions",
)
parser.add_argument("--harness", action="append", choices=["pi", "claude"])
parser.add_argument("--program", action="append", default=[], metavar="HARNESS=PATH")
parser.add_argument("--ui", choices=["terminal", "both"], default="terminal")
parser.add_argument("--background", choices=["light", "dark"], default="dark")
parser.add_argument(
    "--session-turns",
    type=int,
    default=12,
    help="Sustained turns before resume (the resumed turn is added separately)",
)
options = parser.parse_args()
OUT = options.output.resolve()
OUT.mkdir(parents=True, exist_ok=True)
TMP = pathlib.Path(
    tempfile.mkdtemp(
        prefix="codegotchi-interactive-",
        dir="/dev/shm" if pathlib.Path("/dev/shm").is_dir() else None,
    )
)
BINARY = options.binary.resolve()
BINARY_SHA256 = hashlib.sha256(BINARY.read_bytes()).hexdigest()
programs = {
    h: os.environ.get("CODEGOTCHI_REAL_" + h.upper()) or shutil.which(h)
    for h in ["pi", "claude"]
}
for spec in options.program:
    h, path = spec.split("=", 1)
    assert h in programs
    programs[h] = str(pathlib.Path(path).absolute())
selected = options.harness or list(programs)
if options.session_turns < 1:
    parser.error("--session-turns must be positive")
for h in selected:
    if not programs[h] or not os.access(programs[h], os.X_OK):
        parser.error(f"{h} is not installed; supply --program {h}=/path/to/cli")


# Only session-owned direct children are cleanup candidates; record identities before launch.
def stat(pid):
    try:
        f = pathlib.Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
        return int(f[1]), f[19]
    except (OSError, ValueError):
        return None


protected = set()
p = os.getpid()
while p and p not in protected:
    protected.add(p)
    s = stat(p)
    p = s[0] if s else 0
changed = True
while changed:
    changed = False
    for e in pathlib.Path("/proc").iterdir():
        if e.name.isdigit():
            p = int(e.name)
            s = stat(p)
            if s and s[0] in protected and p not in protected:
                protected.add(p)
                changed = True
owned = []


def launch(args, env, log, marker, cwd=None):
    proc = subprocess.Popen(
        args, env=env, cwd=cwd, stdout=open(log, "w"), stderr=subprocess.STDOUT
    )
    owned.append((proc, stat(proc.pid)[1], marker))
    return proc


def stop(proc):
    if proc.poll() is not None:
        return
    _, start, marker = next(x for x in owned if x[0] is proc)
    s = stat(proc.pid)
    assert proc.pid not in protected and s == (os.getpid(), start)
    assert (
        marker
        in pathlib.Path(f"/proc/{proc.pid}/cmdline")
        .read_bytes()
        .replace(b"\0", b" ")
        .decode()
    )
    print(
        subprocess.check_output(
            ["ps", "-o", "pid,ppid,stat,etime,cmd", "-p", str(proc.pid)], text=True
        ).strip(),
        flush=True,
    )
    proc.terminate()
    proc.wait(timeout=10)
    subprocess.check_call(["true"])


ENV = dict(os.environ)
for k in list(ENV):
    if any(t in k for t in ["API_KEY", "AUTH_TOKEN", "ACCESS_TOKEN"]) or k.startswith(
        ("CODEGOTCHI_", "PI_", "OMP_", "CLAUDE_", "HERMES_", "ANTHROPIC_", "OPENAI_")
    ):
        ENV.pop(k, None)
ENV.pop("NO_COLOR", None)
ENV.update(COLORTERM="truecolor", TERM="xterm-256color")
ENV["PATH"] = (
    ":".join(sorted({str(pathlib.Path(programs[h]).parent) for h in selected}))
    + ":"
    + ENV["PATH"]
)
for number in range(300, 400):
    probe = socket.socket()
    try:
        probe.bind(("127.0.0.1", 6000 + number))
        break
    except OSError:
        pass
    finally:
        probe.close()
authority = TMP / "xauthority"
subprocess.check_call(
    [
        "xauth",
        "-f",
        str(authority),
        "add",
        f"localhost:{number}",
        ".",
        secrets.token_hex(16),
    ],
    stdout=subprocess.DEVNULL,
    stderr=subprocess.DEVNULL,
)
ENV["XAUTHORITY"] = str(authority)
ENV["DISPLAY"] = f"localhost:{number}"
xvfb = launch(
    [
        "Xvfb",
        f":{number}",
        "-screen",
        "0",
        "1800x1100x24",
        "-nolisten",
        "unix",
        "-listen",
        "tcp",
        "-auth",
        str(authority),
    ],
    ENV,
    TMP / "xvfb.log",
    f"Xvfb :{number}",
)
for _ in range(100):
    if (
        subprocess.run(
            ["xdotool", "getdisplaygeometry"],
            env=ENV,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            timeout=2,
        ).returncode
        == 0
    ):
        break
    time.sleep(0.1)
wm = launch(["openbox"], ENV, TMP / "wm.log", "openbox")
for _ in range(100):
    prop = subprocess.run(
        ["xprop", "-root", "_NET_SUPPORTING_WM_CHECK"],
        env=ENV,
        capture_output=True,
        text=True,
        timeout=2,
    )
    if "window id" in prop.stdout:
        break
    time.sleep(0.1)
else:
    raise RuntimeError("window manager did not become ready")


def x(args):
    return subprocess.check_output(
        ["xdotool", *args], env=ENV, text=True, timeout=10
    ).strip()


def wait(check, seconds=20):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        v = check()
        if v:
            return v
        time.sleep(0.1)
    raise RuntimeError("bounded wait expired")


case = None


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def do_POST(self):
        try:
            body = json.loads(
                self.rfile.read(int(self.headers.get("Content-Length", 0)))
            )
            messages = body.get("messages", [])
            names = [
                t.get("name") or t.get("function", {}).get("name")
                for t in body.get("tools", [])
            ]
            tool = next((n for n in ["Bash", "bash", "terminal"] if n in names), None)
            users = [m.get("content") for m in messages if m.get("role") == "user"]

            def content_text(value):
                if isinstance(value, str):
                    return value
                if isinstance(value, list):
                    return "\n".join(
                        str(c.get("text", ""))
                        for c in value
                        if isinstance(c, dict)
                        and c.get("type") in ("text", "input_text")
                    )
                return ""

            text = content_text(users[-1]) if users else ""
            serialized = json.dumps(messages)
            assistant_context_text = "\n".join(
                content_text(message.get("content"))
                for message in messages
                if message.get("role") == "assistant"
            )
            result_blocks = []
            for message in messages:
                if message.get("role") == "tool":
                    result_blocks.append(
                        {
                            "tool_use_id": message.get("tool_call_id"),
                            "content": message.get("content"),
                            "is_error": False,
                        }
                    )
                if isinstance(message.get("content"), list):
                    result_blocks.extend(
                        block
                        for block in message["content"]
                        if isinstance(block, dict)
                        and block.get("type") == "tool_result"
                    )
            for block in result_blocks:
                tool_id = block.get("tool_use_id") or block.get("tool_use_id")
                for token, issued_id in case["issued"].items():
                    if tool_id == issued_id:
                        content = json.dumps(block.get("content", ""))
                        denied = any(
                            marker in content.lower()
                            for marker in [
                                "refuses this action",
                                "blocked until care",
                                "too exhausted",
                            ]
                        )
                        case["toolResults"][token] = {
                            "denied": denied,
                            "error": bool(block.get("is_error", False)),
                        }
                        case["toolResultIds"][token] = tool_id
                        case["approvalResultSeen"] = True
                        case["approvalAllowed"] = not denied

            case["requests"] += 1
            case["requestReceipts"].append(
                {
                    "path": self.path,
                    "userText": text,
                    "messageCount": len(messages),
                    "stream": bool(body.get("stream")),
                    "hasPriorApprovalToken": "CG_APPROVAL_DONE" in serialized,
                    "assistantPriorApprovalToken": "CG_APPROVAL_DONE"
                    in assistant_context_text,
                    "hasPriorLongToken": "CG_LONG_CHUNK" in serialized,
                }
            )
            title_request = "Write the title in the predominant language" in text
            if self.path.endswith("count_tokens"):
                self._json({"input_tokens": 64})
                return

            identifier = "call_" + secrets.token_hex(8)
            token = next(
                (
                    marker
                    for marker in [
                        "CG_APPROVAL",
                        "CG_LONG_RESPONSE",
                        "CG_INTERRUPT_RESUME",
                        "CG_RESUME_CHECK",
                    ]
                    if marker in text
                ),
                None,
            )
            if token is None:
                turn = re.search(r"\b(CG_TURN_(\d+))\b", text)
                token = turn.group(1) if turn else None
            if (
                token == "CG_RESUME_CHECK"
                and "CG_APPROVAL_DONE" in assistant_context_text
            ):
                case["resumeContextSeen"] = True

            is_tool = bool(
                tool
                and token == "CG_APPROVAL"
                and token not in case["issued"]
                and not title_request
            )
            if is_tool:
                case["issued"][token] = identifier
                case["prompts"].append(token)
                case["promptTexts"].append(text)
                if not title_request:
                    case["responded"].append(token)

            if is_tool:
                args = {
                    "command": f"printf CG_APPROVAL_DONE > '{case['root'] / 'CG_APPROVAL_DONE'}'"
                }
                self._reply(body, tool, identifier, args=args, reply="")
                return

            if token == "CG_LONG_RESPONSE" and not case["longCompleted"]:
                self._long_reply(body)
                return

            if token == "CG_RESUME_CHECK":
                reply = "CG_RESUME_ACK " + (
                    "CG_APPROVAL_DONE" if case["resumeContextSeen"] else "NO_CONTEXT"
                )
            elif token and token.startswith("CG_TURN_"):
                case["acknowledgedTurns"].add(token)
                case["turns"] = len(case["acknowledgedTurns"])
                reply = f"{token} acknowledged; sustained session turn {case['turns']}"
            elif token == "CG_INTERRUPT_RESUME":
                reply = "CG_INTERRUPT_RESUME_ACK"
            elif (
                case["approvalResultSeen"] and "CG_APPROVAL_DONE" not in case["replies"]
            ):
                case["approvalResultSeen"] = True
                reply = "CG_APPROVAL_DONE"
                case["replies"].append(reply)
            else:
                reply = "CG_SESSION_AUXILIARY"
            if token and not title_request:
                case["responded"].append(token)
            self._reply(body, tool, identifier, reply=reply)
        except Exception as e:
            case["serverErrors"].append(type(e).__name__ + ": " + str(e))
            try:
                self.send_error(500)
            except OSError:
                pass

    def _json(self, value):
        data = json.dumps(value).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def _reply(self, body, tool, identifier, args=None, reply=""):
        if "/messages" in self.path:
            is_tool = args is not None
            content = (
                [{"type": "tool_use", "id": identifier, "name": tool, "input": args}]
                if is_tool
                else [{"type": "text", "text": reply}]
            )
            value = {
                "id": "msg_" + identifier,
                "type": "message",
                "role": "assistant",
                "model": body.get("model", "test"),
                "content": content,
                "stop_reason": "tool_use" if is_tool else "end_turn",
                "stop_sequence": None,
                "usage": {"input_tokens": 64, "output_tokens": 16},
            }
            if not body.get("stream"):
                self._json(value)
                return
            events = [
                (
                    "message_start",
                    {
                        "type": "message_start",
                        "message": dict(value, content=[], stop_reason=None),
                    },
                )
            ]
            for index, block in enumerate(content):
                events.extend(
                    [
                        (
                            "content_block_start",
                            {
                                "type": "content_block_start",
                                "index": index,
                                "content_block": (
                                    dict(block, input={})
                                    if is_tool
                                    else dict(block, text="")
                                ),
                            },
                        ),
                        (
                            "content_block_delta",
                            {
                                "type": "content_block_delta",
                                "index": index,
                                "delta": (
                                    {
                                        "type": "input_json_delta",
                                        "partial_json": json.dumps(args),
                                    }
                                    if is_tool
                                    else {"type": "text_delta", "text": reply}
                                ),
                            },
                        ),
                        (
                            "content_block_stop",
                            {"type": "content_block_stop", "index": index},
                        ),
                    ]
                )
            events.extend(
                [
                    (
                        "message_delta",
                        {
                            "type": "message_delta",
                            "delta": {
                                "stop_reason": value["stop_reason"],
                                "stop_sequence": None,
                            },
                            "usage": {"output_tokens": 16},
                        },
                    ),
                    ("message_stop", {"type": "message_stop"}),
                ]
            )
            self._events(events)
            return

        is_tool = args is not None
        message = {"role": "assistant", "content": None if is_tool else reply}
        if is_tool:
            message["tool_calls"] = [
                {
                    "id": identifier,
                    "type": "function",
                    "function": {"name": tool, "arguments": json.dumps(args)},
                }
            ]
        value = {
            "id": identifier,
            "object": "chat.completion",
            "created": int(time.time()),
            "model": body.get("model", "test"),
            "choices": [
                {
                    "index": 0,
                    "message": message,
                    "finish_reason": "tool_calls" if is_tool else "stop",
                }
            ],
            "usage": {"prompt_tokens": 64, "completion_tokens": 16, "total_tokens": 80},
        }
        if not body.get("stream"):
            self._json(value)
            return
        delta = dict(message)
        if is_tool:
            delta["tool_calls"] = [dict(message["tool_calls"][0], index=0)]
        chunk = dict(
            value,
            object="chat.completion.chunk",
            choices=[{"index": 0, "delta": delta, "finish_reason": None}],
        )
        end = dict(
            chunk,
            choices=[
                {
                    "index": 0,
                    "delta": {},
                    "finish_reason": "tool_calls" if is_tool else "stop",
                }
            ],
        )
        self._events_openai([chunk, end])

    def _long_reply(self, body):
        case["longStarted"] = True
        if "/messages" not in self.path:
            self._long_openai_reply(body)
            return
        value = {
            "id": "msg_long_" + secrets.token_hex(4),
            "type": "message",
            "role": "assistant",
            "model": body.get("model", "test"),
            "content": [{"type": "text", "text": ""}],
            "stop_reason": None,
            "stop_sequence": None,
            "usage": {"input_tokens": 64, "output_tokens": 0},
        }
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Cache-Control", "no-cache")
        self.end_headers()

        def event(kind, payload):
            self.wfile.write(
                ("event: " + kind + "\ndata: " + json.dumps(payload) + "\n\n").encode()
            )
            self.wfile.flush()

        try:
            event("message_start", {"type": "message_start", "message": value})
            event(
                "content_block_start",
                {
                    "type": "content_block_start",
                    "index": 0,
                    "content_block": {"type": "text", "text": ""},
                },
            )
            for index in range(80):
                event(
                    "content_block_delta",
                    {
                        "type": "content_block_delta",
                        "index": 0,
                        "delta": {
                            "type": "text_delta",
                            "text": f"CG_LONG_CHUNK_{index} ",
                        },
                    },
                )
                time.sleep(0.12)
            event("content_block_stop", {"type": "content_block_stop", "index": 0})
            event(
                "message_delta",
                {
                    "type": "message_delta",
                    "delta": {"stop_reason": "end_turn", "stop_sequence": None},
                    "usage": {"output_tokens": 80},
                },
            )
            event("message_stop", {"type": "message_stop"})
            case["longCompleted"] = True
        except (BrokenPipeError, ConnectionResetError, OSError):
            case["longInterrupted"] = True

    def _long_openai_reply(self, body):
        response_id = "chatcmpl_long_" + secrets.token_hex(4)
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Cache-Control", "no-cache")
        self.end_headers()

        def chunk(delta, finish=None):
            value = {
                "id": response_id,
                "object": "chat.completion.chunk",
                "created": int(time.time()),
                "model": body.get("model", "test"),
                "choices": [{"index": 0, "delta": delta, "finish_reason": finish}],
            }
            self.wfile.write(("data: " + json.dumps(value) + "\n\n").encode())
            self.wfile.flush()

        try:
            chunk({"role": "assistant", "content": ""})
            for index in range(80):
                chunk({"content": f"CG_LONG_CHUNK_{index} "})
                time.sleep(0.12)
            chunk({}, "stop")
            self.wfile.write(b"data: [DONE]\n\n")
            self.wfile.flush()
            case["longCompleted"] = True
        except (BrokenPipeError, ConnectionResetError, OSError):
            case["longInterrupted"] = True

    def _events(self, events):
        data = "".join(
            "event: " + kind + "\ndata: " + json.dumps(payload) + "\n\n"
            for kind, payload in events
        ).encode()
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def _events_openai(self, payloads):
        data = (
            b"".join(
                (
                    b"data: " + json.dumps(payload).encode() + b"\n\n"
                    for payload in payloads
                )
            )
            + b"data: [DONE]\n\n"
        )
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)


server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
threading.Thread(target=server.serve_forever, daemon=True).start()
base = f"http://127.0.0.1:{server.server_port}"
terminal = None
window = None
harness = None
records = []
stage = "initializing"


def api(path, data=None):
    meta = json.loads(
        next((case["root"] / "runtime/codegotchi").glob("session-*.json")).read_text()
    )
    req = urllib.request.Request(
        meta["loopbackBaseUrl"] + path,
        data=json.dumps(data).encode() if data is not None else None,
        headers={
            "Authorization": "Bearer " + meta["bearerToken"],
            "Content-Type": "application/json",
            "X-CodeGotchi-Debug": "1",
        },
    )
    return json.load(urllib.request.urlopen(req, timeout=5))


def capture(name):
    path = OUT / f"{harness}-{name}.png"
    subprocess.check_call(["import", "-window", window, str(path)], env=ENV, timeout=10)
    records.append({"harness": harness, "action": "capture", "artifact": path.name})
    return str(path)


def summary():
    state = api("/api/v1/state")
    return {
        "harness": harness,
        "requests": case["requests"],
        "prompts": case["prompts"],
        "promptTexts": case["promptTexts"],
        "approvalResultSeen": case["approvalResultSeen"],
        "approvalAllowed": case["approvalAllowed"],
        "longStarted": case["longStarted"],
        "longInterrupted": case["longInterrupted"],
        "longCompleted": case["longCompleted"],
        "resumeContextSeen": case["resumeContextSeen"],
        "turns": case["turns"],
        "acknowledgedTurnMarkers": sorted(case["acknowledgedTurns"]),
        "replies": list(case["replies"]),
        "responded": list(case["responded"]),
        "requestReceipts": list(case["requestReceipts"]),
        "serverErrors": list(case["serverErrors"]),
        "sentinels": [p.name for p in case["root"].glob("CG_*")],
        "activity": state["activity"],
        "workPoints": state["workPoints"],
        "eventCount": len(state["processedEventIds"]),
        "sessionCount": len(state["sessionActivities"]),
        "toolResults": dict(case["toolResults"]),
        "issuedToolCallIds": dict(case["issued"]),
        "toolResultIds": dict(case["toolResultIds"]),
        "needs": state["needs"],
        "careCount": len(state["processedCareIds"]),
    }


def action(a):
    global case, terminal, window, harness
    op = a["op"]
    if op == "start":
        if terminal:
            stop(terminal)
        harness = a["harness"]
        if a.get("resume"):
            if case is None or case["harness"] != harness:
                raise RuntimeError(
                    "resume requested without the first session for this harness"
                )
            run = case["root"]
            case["phase"] = "resumed"
        else:
            run = TMP / f"{harness}-{len(records)}"
            run.mkdir()
            case = {
                "root": run,
                "harness": harness,
                "startedAt": time.monotonic(),
                "phase": "initial",
                "issued": {},
                "toolResultIds": {},
                "prompts": [],
                "promptTexts": [],
                "requests": 0,
                "toolResults": {},
                "approvalResultSeen": False,
                "approvalAllowed": None,
                "longStarted": False,
                "longInterrupted": False,
                "longCompleted": False,
                "resumeContextSeen": False,
                "turns": 0,
                "acknowledgedTurns": set(),
                "replies": [],
                "responded": [],
                "requestReceipts": [],
                "serverErrors": [],
            }
        env = dict(
            ENV,
            HOME=str(run),
            XDG_STATE_HOME=str(run / "state"),
            XDG_RUNTIME_DIR=str(run / "runtime"),
            CODEGOTCHI_BROWSER="none",
            CODEGOTCHI_ENABLE_DEBUG="1",
            CODEGOTCHI_LIVE_HARNESS="1",
            CODEGOTCHI_LIVE_ARGUMENTS_ROOT=str(run),
            CODEGOTCHI_LIVE_PROTOCOL_FILE=str(run / "protocol.txt"),
            CLAUDE_CONFIG_DIR=str(run / "claude"),
            HERMES_HOME=str(run / "hermes"),
            ANTHROPIC_API_KEY="codegotchi-test-only",
            ANTHROPIC_BASE_URL=base,
            CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC="1",
        )
        env["CODEGOTCHI_REAL_" + harness.upper()] = programs[harness]
        if harness == "pi":
            agent = run / ".pi" / "agent"
            agent.mkdir(parents=True, exist_ok=True)
            env["PI_CODING_AGENT_DIR"] = str(agent)
            session_dir = run / "sessions"
            session_dir.mkdir(exist_ok=True)
            (agent / "models.json").write_text(
                json.dumps(
                    {
                        "providers": {
                            "codegotchi-local": {
                                "baseUrl": base + "/v1",
                                "api": "openai-completions",
                                "apiKey": "codegotchi-test-only",
                                "models": [
                                    {
                                        "id": "test",
                                        "name": "CodeGotchi local test",
                                        "reasoning": False,
                                        "input": ["text"],
                                        "cost": {
                                            "input": 0,
                                            "output": 0,
                                            "cacheRead": 0,
                                            "cacheWrite": 0,
                                        },
                                        "contextWindow": 32768,
                                        "maxTokens": 4096,
                                    }
                                ],
                            }
                        }
                    }
                )
            )
            args = [
                "--provider",
                "codegotchi-local",
                "--model",
                "test",
                "--session-dir",
                str(session_dir),
            ]
            if a.get("resume"):
                args.insert(0, "--continue")
        elif harness == "claude":
            args = ["--model", "claude-sonnet-4-6", "--permission-mode", "manual"]
            if a.get("resume"):
                session_id = a.get("sessionId")
                if not session_id:
                    raise RuntimeError("Claude resume requires sessionId")
                args[0:0] = ["--resume", session_id]
        title = f"codegotchi-interactive-{harness}-{os.getpid()}"
        child_command = [
            str(BINARY),
            "run",
            "--ui",
            a.get("ui", "terminal"),
            "--",
            harness,
            *args,
        ]
        terminal = launch(
            [
                "xterm",
                "-bg",
                "#101218" if options.background == "dark" else "white",
                "-fg",
                "#e8e8e8" if options.background == "dark" else "black",
                "-title",
                title,
                "-name",
                title,
                "-fa",
                "DejaVu Sans Mono",
                "-fs",
                "10",
                "-geometry",
                "120x45+0+0",
                "-e",
                *child_command,
            ],
            env,
            run / "xterm.log",
            title,
            cwd=run,
        )
        window = wait(
            lambda: subprocess.run(
                ["xdotool", "search", "--classname", title],
                env=ENV,
                capture_output=True,
                text=True,
            )
            .stdout.strip()
            .split("\n")[0]
        )
        x(["windowactivate", "--sync", window])
        time.sleep(6)
        return capture("resumed-startup" if a.get("resume") else "startup")
    if op == "key":
        x(["key", "--window", window, "--clearmodifiers", *a["keys"]])
        time.sleep(a.get("wait", 0.2))
    elif op == "text":
        x(["type", "--window", window, "--delay", "8", "--", a["text"]])
        time.sleep(0.2)
    elif op == "paste":
        subprocess.run(
            ["xclip", "-selection", "primary"],
            input=a["text"],
            text=True,
            env=ENV,
            check=True,
        )
        x(["key", "--window", window, "shift+Insert"])
        time.sleep(0.3)
    elif op == "capture":
        return capture(a["name"])
    elif op == "resize":
        x(
            [
                "windowsize",
                "--sync",
                "--usehints",
                window,
                str(a["cols"]),
                str(a["rows"]),
            ]
        )
        time.sleep(0.8)
        return capture(a.get("name", f'{a["cols"]}x{a["rows"]}'))
    elif op == "state":
        return summary()
    elif op == "strict":
        api("/api/v1/mode", {"mode": "strict"})
        api("/api/v1/debug/neglect", {})
        return summary()
    elif op == "protocol":
        return (case["root"] / "protocol.txt").read_text()
    elif op == "wait":
        time.sleep(a.get("seconds", 2))
        return summary()
    elif op == "focus":
        title = f"codegotchi-interactive-focus-{os.getpid()}"
        peer = launch(
            ["xterm", "-title", title, "-name", title, "-e", "sleep", "20"],
            ENV,
            TMP / "focus.log",
            title,
        )
        peer_window = wait(
            lambda: subprocess.run(
                ["xdotool", "search", "--classname", title],
                env=ENV,
                capture_output=True,
                text=True,
            )
            .stdout.strip()
            .split("\n")[0]
        )
        x(["windowactivate", "--sync", peer_window])
        assert x(["getactivewindow"]) == peer_window
        time.sleep(0.3)
        x(["windowactivate", "--sync", window])
        assert x(["getactivewindow"]) == window
        time.sleep(0.3)
        stop(peer)
    elif op in ["click", "feed"]:
        # xterm has a two-pixel border; derive cell centers from its current grid.
        geom = dict(
            line.split("=", 1)
            for line in x(["getwindowgeometry", "--shell", window]).splitlines()
            if "=" in line
        )
        cw = (int(geom["WIDTH"]) - 4) // a["cols"]
        ch = (int(geom["HEIGHT"]) - 4) // a["rows"]
        x(
            [
                "mousemove",
                "--window",
                window,
                str(2 + int((a["col"] + 0.5) * cw)),
                str(2 + int((a["row"] + 0.5) * ch)),
            ]
        )
        if op == "feed":
            x(["mousedown", "1"])
            time.sleep(0.1)
            x(
                [
                    "mousemove",
                    "--window",
                    window,
                    str(2 + int(3.5 * cw)),
                    str(2 + int((a["row"] + 0.5) * ch)),
                ]
            )
            time.sleep(0.1)
            x(["mouseup", "1"])
        else:
            x(["click", "1"])
        time.sleep(0.4)
    elif op == "save":
        (OUT / "actions.json").write_text(json.dumps(records, indent=2) + "\n")
        return str(TMP)
    elif op == "quit":
        return "quit"
    return {"exitCode": terminal.poll()}


def do(op, **args):
    a = {"op": op, **args}
    records.append({"harness": harness, **a})
    return action(a)


def wait_responded(token, seconds=30):
    return wait(lambda: token in case["responded"], seconds)


def claude_session_id():
    """Find the persisted session id from Claude's disposable JSONL history."""
    candidates = sorted(case["root"].rglob("*.jsonl"), key=lambda p: p.stat().st_mtime)
    for path in reversed(candidates):
        try:
            for line in reversed(path.read_text(errors="replace").splitlines()):
                record = json.loads(line)
                value = record.get("sessionId") or record.get("session_id")
                if value:
                    return value
        except (OSError, json.JSONDecodeError):
            continue
    raise RuntimeError("Claude did not persist a resumable session id")


def persisted_aborted_response():
    """Confirm the harness persisted an assistant response with an abort stop reason."""
    for path in case["root"].rglob("*.jsonl"):
        try:
            for line in path.read_text(errors="replace").splitlines():
                record = json.loads(line)
                message = record.get("message", record)
                if (
                    isinstance(message, dict)
                    and message.get("role") == "assistant"
                    and (
                        record.get("stopReason") == "aborted"
                        or message.get("stopReason") == "aborted"
                        or record.get("isAbortedMidStream") is True
                    )
                ):
                    return True
        except (OSError, json.JSONDecodeError):
            continue
    return False


def finish_process():
    wait(lambda: terminal.poll() is not None, 20)
    return terminal.returncode


def exit_process(h):
    do("text", text="/exit" if h == "claude" else "/quit")
    do("key", keys=["Return"], wait=2)
    code = finish_process()
    assert code == 0, f"{h} exited with {code}"
    runtime = case["root"] / "runtime/codegotchi"
    assert not list(runtime.glob("session-*.json"))
    assert not list(runtime.glob("integration-*"))
    return code


def prepare_claude_onboarding():
    # Fresh disposable home: matching theme, dummy API key, notes, trust.
    for keys in [
        ["Down", "Return"] if options.background == "light" else ["Return"],
        ["Up", "Return"],
        ["Return"],
        ["Down", "Return"],
    ]:
        do("key", keys=keys, wait=2)
        do("capture", name="onboarding-" + str(len(records)))


def verify(h):
    global stage
    stage = f"{h}: initial launch"
    do("start", harness=h, ui=options.ui)
    if h == "claude":
        stage = f"{h}: onboarding"
        prepare_claude_onboarding()
    else:
        wait(lambda: summary()["sessionCount"] > 0, 60)
        time.sleep(0.5)

    # Approval behavior: Pi documents that it has no permission popups; its
    # native approval coverage is therefore explicitly not applicable. Claude
    # must show its real permission prompt because no allowedTools bypass is used.
    stage = f"{h}: native approval"
    do("text", text="CG_APPROVAL: run the requested shell command once")
    do("key", keys=["Return"], wait=1)
    wait(lambda: "CG_APPROVAL" in case["issued"], 30)
    if h == "claude":
        assert not (
            case["root"] / "CG_APPROVAL_DONE"
        ).exists(), "Claude ran the command before its native permission prompt"
        do("capture", name="native-approval-prompt")
        do("key", keys=["Return"], wait=1)
        wait(lambda: (case["root"] / "CG_APPROVAL_DONE").exists(), 40)
        wait(lambda: case["approvalResultSeen"], 20)
        approval = "approved_with_native_prompt"
    else:
        wait(lambda: (case["root"] / "CG_APPROVAL_DONE").exists(), 40)
        approval = "not_applicable_pi_has_no_native_permission_popups"
    assert (case["root"] / "CG_APPROVAL_DONE").read_text() == "CG_APPROVAL_DONE"
    do("capture", name="approval-result")

    # A real streamed response remains in flight long enough for a physical
    # Escape to interrupt it. Claude should close the request; Pi may cancel at
    # its own documented keyboard boundary, but both must accept the next turn.
    stage = f"{h}: interrupted long response"
    do("text", text="CG_LONG_RESPONSE: stream a long answer")
    do("key", keys=["Return"], wait=2)
    wait(lambda: case["longStarted"], 20)
    do("capture", name="long-response-streaming")
    do("key", keys=["Escape"], wait=2)
    wait(lambda: case["longInterrupted"] or case["longCompleted"], 15)
    long_interrupted_at_escape = case["longInterrupted"]
    long_completed_at_escape = case["longCompleted"]
    backend_after_escape = summary()
    backend_escape_activity = backend_after_escape["activity"]
    backend_escape_active = (
        backend_escape_activity in ("Thinking", "Working")
        or isinstance(backend_escape_activity, dict)
        and "Active" in backend_escape_activity
    )
    do("capture", name="after-interrupt")
    assert case[
        "longInterrupted"
    ], f"{h} did not terminate the streamed response after Escape"

    stage = f"{h}: post-interruption response"
    do("text", text="CG_INTERRUPT_RESUME: continue after cancellation")
    do("key", keys=["Return"], wait=1)
    wait_responded("CG_INTERRUPT_RESUME")
    backend_after_followup = summary()
    do("capture", name="interrupt-resumed")

    # Five additional turns create a materially longer session while retaining
    # real xterm input and screenshots for each visible state.
    stage = f"{h}: sustained turns before resume"
    for index in range(1, options.session_turns + 1):
        token = f"CG_TURN_{index}"
        do("text", text=f"{token}: sustained session turn {index}")
        do("key", keys=["Return"], wait=1)
        wait_responded(token)
        do("capture", name=f"sustained-turn-{index}")
    assert case["turns"] >= 5
    first_summary = summary()
    first_session = claude_session_id() if h == "claude" else None
    do("capture", name="before-resume")
    first_exit = exit_process(h)
    aborted_record = persisted_aborted_response()

    # Launch a fresh CodeGotchi process against the persisted harness session.
    # Claude receives an explicit id; Pi's documented --continue chooses the
    # most recent session in the isolated session directory.
    stage = f"{h}: persisted session resume launch"
    do("start", harness=h, ui=options.ui, resume=True, sessionId=first_session)
    if h == "claude":
        # Claude asks the workspace trust question again when a resumed
        # conversation is opened by a fresh wrapper process. Answer it with
        # real navigation before sending the resumed prompt.
        time.sleep(12)
        do("key", keys=["Down", "Return"], wait=4)
        do("capture", name="resumed-ready")
    else:
        wait(lambda: summary()["sessionCount"] > 0, 60)
    do("capture", name="resumed-startup")
    stage = f"{h}: resumed context turn"
    do("text", text="CG_RESUME_CHECK: what approval marker was created earlier?")
    do("key", keys=["Return"], wait=2)
    wait_responded("CG_RESUME_CHECK")
    assert case[
        "resumeContextSeen"
    ], "resumed model request did not contain prior conversation context"
    do("capture", name="resumed-context")
    stage = f"{h}: final resumed turn"
    resumed_turn_token = f"CG_TURN_{options.session_turns + 1}"
    do("text", text=f"{resumed_turn_token}: final sustained resumed turn")
    do("key", keys=["Return"], wait=1)
    wait_responded(resumed_turn_token)
    do("capture", name="resumed-final-turn")
    resumed_summary = summary()
    second_exit = exit_process(h)
    protocol = case["root"] / "protocol.txt"
    if protocol.exists():
        (OUT / f"{h}-protocol.txt").write_text(protocol.read_text())
    (OUT / f"{h}-receipts.json").write_text(
        json.dumps(
            {
                "requests": case["requestReceipts"],
                "issuedToolCallIds": case["issued"],
                "toolResultIds": case["toolResultIds"],
                "toolResults": case["toolResults"],
                "acknowledgedTurnMarkers": sorted(case["acknowledgedTurns"]),
                "persistedAbortedAssistantResponse": aborted_record,
            },
            indent=2,
        )
        + "\n"
    )
    return {
        "harness": h,
        "ui": options.ui,
        "background": options.background,
        "binarySha256": BINARY_SHA256,
        "elapsedSeconds": round(time.monotonic() - case["startedAt"], 2),
        "model": "scripted loopback; no authenticated provider request",
        "nativeApproval": approval,
        "approvalPromptCaptured": h == "claude",
        "approvalResultSeen": case["approvalResultSeen"] if h == "claude" else None,
        "interruption": {
            "longResponseStarted": case["longStarted"],
            "longResponseInterrupted": long_interrupted_at_escape,
            "longResponseCompletedAtEscape": long_completed_at_escape,
            "physicalEscapeSent": True,
            "persistedAbortedAssistantResponse": aborted_record,
            "backendAfterEscape": {
                "activity": backend_after_escape["activity"],
                "eventCount": backend_after_escape["eventCount"],
                "workPoints": backend_after_escape["workPoints"],
            },
            "backendAfterFollowup": {
                "activity": backend_after_followup["activity"],
                "eventCount": backend_after_followup["eventCount"],
                "workPoints": backend_after_followup["workPoints"],
            },
            "nativeStopHookSettled": not backend_escape_active,
            "nativeStopHookCoverageGap": (
                "The harness abort record is real, but CodeGotchi activity remained "
                + json.dumps(backend_escape_activity, sort_keys=True)
                + " until the follow-up; the native interrupt has no directly observed "
                "stop hook in this probe."
                if backend_escape_active
                else None
            ),
        },
        "sustainedTurnsRequested": options.session_turns,
        "sustainedTurnsBeforeResume": first_summary["turns"],
        "sustainedTurnsAfterResume": resumed_summary["turns"],
        "acknowledgedTurnMarkers": resumed_summary["acknowledgedTurnMarkers"],
        "resumeContextSeen": case["resumeContextSeen"],
        "resumeSessionId": first_session,
        "requestCount": case["requests"],
        "workPoints": resumed_summary["workPoints"],
        "eventCount": resumed_summary["eventCount"],
        "firstExitCode": first_exit,
        "secondExitCode": second_exit,
        "runtimeCleaned": True,
        "serverErrors": case["serverErrors"],
        "passed": (
            (h == "pi" or case["approvalResultSeen"])
            and case["longStarted"]
            and case["longInterrupted"]
            and aborted_record
            and case["resumeContextSeen"]
            and first_summary["turns"] >= options.session_turns
            and resumed_summary["turns"] >= options.session_turns
            and first_exit == 0
            and second_exit == 0
            and not case["serverErrors"]
        ),
    }


results = []
print(json.dumps({"ready": True, "privateRoot": str(TMP)}), flush=True)
try:
    for h in selected:
        try:
            result = verify(h)
            results.append(result)
            print(json.dumps(result), flush=True)
        except Exception as e:
            partial = {
                "harness": h,
                "stage": stage,
                "passed": False,
                "error": type(e).__name__ + ": " + str(e),
            }
            if case is not None and case.get("harness") == h:
                partial["requests"] = case["requests"]
                partial["requestReceipts"] = case["requestReceipts"]
                partial["issuedToolCallIds"] = case["issued"]
                partial["toolResultIds"] = case["toolResultIds"]
                partial["toolResults"] = case["toolResults"]
                partial["longStarted"] = case["longStarted"]
                partial["longInterrupted"] = case["longInterrupted"]
                partial["serverErrors"] = case["serverErrors"]
                (OUT / f"{h}-partial.json").write_text(
                    json.dumps(partial, indent=2) + "\n"
                )
            results.append(partial)
            print(json.dumps(results[-1]), flush=True)
            try:
                do("capture", name="failed")
            except Exception:
                pass
            break
finally:
    (OUT / "actions.json").write_text(json.dumps(records, indent=2) + "\n")
    (OUT / "results.json").write_text(json.dumps(results, indent=2) + "\n")
    for proc, _, _ in reversed(owned):
        try:
            stop(proc)
        except (AssertionError, subprocess.SubprocessError, OSError):
            pass
    server.shutdown()
if results and not all(r["passed"] for r in results):
    sys.exit(1)
