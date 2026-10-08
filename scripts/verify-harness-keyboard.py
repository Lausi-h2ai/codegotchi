#!/usr/bin/env python3
"""Real CLI keyboard and visual acceptance on a private Xvfb display.

Requires xterm, Xvfb, openbox, xauth, xdotool, xclip, ImageMagick, and installed
CLIs. Uses disposable homes and a scripted loopback model (no provider keys).
Images are evidence for manual inspection, not automatic visual pass claims.
"""

import argparse
import hashlib
import json
import os
import pathlib
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
    default=ROOT / "docs/verification/multi-harness/keyboard",
)
parser.add_argument(
    "--harness", action="append", choices=["pi", "omp", "claude", "hermes"]
)
parser.add_argument("--program", action="append", default=[], metavar="HARNESS=PATH")
parser.add_argument("--ui", choices=["terminal", "both"], default="terminal")
parser.add_argument("--background", choices=["light", "dark"], default="dark")
parser.add_argument(
    "--terminal-theme",
    choices=["auto", "mono", "soft-green", "amber", "night"],
    default="auto",
)
parser.add_argument(
    "--interactive",
    action="store_true",
    help="Read JSON actions from stdin instead of running assertions",
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
    for h in ["pi", "omp", "claude", "hermes"]
}
for spec in options.program:
    h, path = spec.split("=", 1)
    assert h in programs
    programs[h] = str(pathlib.Path(path).absolute())
selected = options.harness or list(programs)
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
            result = any(
                m.get("role") == "tool"
                or any(
                    isinstance(c, dict) and c.get("type") == "tool_result"
                    for c in (
                        m.get("content", [])
                        if isinstance(m.get("content"), list)
                        else []
                    )
                )
                for m in messages
            )
            names = [
                t.get("name") or t.get("function", {}).get("name")
                for t in body.get("tools", [])
            ]
            tool = next((n for n in ["Bash", "bash", "terminal"] if n in names), None)
            # Each new user turn is distinguished by its marker, even in retained history.
            users = [m.get("content") for m in messages if m.get("role") == "user"]
            prompt = next(
                (
                    u
                    for u in reversed(users)
                    if isinstance(u, str)
                    or isinstance(u, list)
                    and any(c.get("type") == "text" for c in u if isinstance(c, dict))
                ),
                None,
            )
            text = (
                prompt
                if isinstance(prompt, str)
                else "\n".join(
                    c.get("text", "") for c in prompt or [] if isinstance(c, dict)
                )
            )
            identifier = "call_" + secrets.token_hex(8)
            token = next(
                (
                    t
                    for t in [
                        "CG_INPUT_EDIT_OK",
                        "CG_PASTE_LINE_ONE",
                        "CG_STRICT_PROBE",
                        "CG_CARE_PROBE",
                    ]
                    if t in text
                ),
                None,
            )
            is_tool = bool(tool and token and token not in case["issued"])
            if is_tool:
                case["issued"][token] = identifier
                case["prompts"].append(token)
                case["promptTexts"].append(text)

            for m in messages:
                blocks = (
                    [
                        {
                            "tool_use_id": m.get("tool_call_id"),
                            "content": m.get("content"),
                        }
                    ]
                    if m.get("role") == "tool"
                    else (
                        [
                            c
                            for c in m.get("content", [])
                            if isinstance(c, dict) and c.get("type") == "tool_result"
                        ]
                        if isinstance(m.get("content"), list)
                        else []
                    )
                )
                for block in blocks:
                    tool_id = block.get("tool_use_id")
                    if tool_id in case["issued"].values():
                        tool_token = next(
                            k for k, v in case["issued"].items() if v == tool_id
                        )
                        content = json.dumps(block.get("content", ""))
                        denied = "refuses this action" in content
                        case["toolResults"][tool_token] = {
                            "denied": denied,
                            "error": bool(block.get("is_error", False)),
                        }
                        if denied:
                            case["denialSeen"] = True
            case["requests"] += 1
            if self.path.endswith("count_tokens"):
                value = {"input_tokens": 64}
                data = json.dumps(value).encode()
                ctype = "application/json"
            else:
                time.sleep(1.2)
                args = {
                    "command": f'printf {token} > {case["root"] / (token or "untagged")}'
                }
                reply = "CG_LOCAL_DONE " + (token or "auxiliary")
                if "/messages" in self.path:
                    content = (
                        [
                            {
                                "type": "tool_use",
                                "id": identifier,
                                "name": tool,
                                "input": args,
                            }
                        ]
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
                    if body.get("stream"):
                        events = [
                            (
                                "message_start",
                                {
                                    "type": "message_start",
                                    "message": dict(
                                        value, content=[], stop_reason=None
                                    ),
                                },
                            )
                        ]
                        for i, b in enumerate(content):
                            events += [
                                (
                                    "content_block_start",
                                    {
                                        "type": "content_block_start",
                                        "index": i,
                                        "content_block": (
                                            dict(b, input={})
                                            if is_tool
                                            else dict(b, text="")
                                        ),
                                    },
                                ),
                                (
                                    "content_block_delta",
                                    {
                                        "type": "content_block_delta",
                                        "index": i,
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
                                    {"type": "content_block_stop", "index": i},
                                ),
                            ]
                        events += [
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
                        data = "".join(
                            "event: " + k + "\ndata: " + json.dumps(e) + "\n\n"
                            for k, e in events
                        ).encode()
                        ctype = "text/event-stream"
                    else:
                        data = json.dumps(value).encode()
                        ctype = "application/json"
                else:
                    msg = {"role": "assistant", "content": None if is_tool else reply}
                    if is_tool:
                        msg["tool_calls"] = [
                            {
                                "id": identifier,
                                "type": "function",
                                "function": {
                                    "name": tool,
                                    "arguments": json.dumps(args),
                                },
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
                                "message": msg,
                                "finish_reason": "tool_calls" if is_tool else "stop",
                            }
                        ],
                        "usage": {
                            "prompt_tokens": 64,
                            "completion_tokens": 16,
                            "total_tokens": 80,
                        },
                    }
                    if body.get("stream"):
                        delta = dict(msg)
                        if is_tool:
                            delta["tool_calls"] = [dict(msg["tool_calls"][0], index=0)]
                        chunk = dict(
                            value,
                            object="chat.completion.chunk",
                            choices=[
                                {"index": 0, "delta": delta, "finish_reason": None}
                            ],
                        )
                        end = dict(
                            chunk,
                            choices=[
                                {
                                    "index": 0,
                                    "delta": {},
                                    "finish_reason": (
                                        "tool_calls" if is_tool else "stop"
                                    ),
                                }
                            ],
                        )
                        data = (
                            "data: "
                            + json.dumps(chunk)
                            + "\n\ndata: "
                            + json.dumps(end)
                            + "\n\ndata: [DONE]\n\n"
                        ).encode()
                        ctype = "text/event-stream"
                    else:
                        data = json.dumps(value).encode()
                        ctype = "application/json"
            self.send_response(200)
            self.send_header("Content-Type", ctype)
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)
        except Exception as e:
            print("model error: " + str(e), flush=True)
            self.send_error(500)


server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
threading.Thread(target=server.serve_forever, daemon=True).start()
base = f"http://127.0.0.1:{server.server_port}"
terminal = None
window = None
harness = None
records = []


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
        "denialSeen": case["denialSeen"],
        "sentinels": [p.name for p in case["root"].glob("CG_*")],
        "activity": state["activity"],
        "workPoints": state["workPoints"],
        "eventCount": len(state["processedEventIds"]),
        "sessionCount": len(state["sessionActivities"]),
        "toolResults": dict(case["toolResults"]),
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
        run = TMP / f"{harness}-{len(records)}"
        run.mkdir()
        case = {
            "root": run,
            "issued": {},
            "prompts": [],
            "promptTexts": [],
            "requests": 0,
            "toolResults": {},
            "denialSeen": False,
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
        if harness in ["pi", "omp"]:
            agent = run / (".pi" if harness == "pi" else ".omp") / "agent"
            agent.mkdir(parents=True)
            env["PI_CODING_AGENT_DIR"] = str(agent)
            if a.get("skip_setup", True):
                env["OMP_SKIP_SETUP"] = "1"
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
            args = ["--provider", "codegotchi-local", "--model", "test", "--no-session"]
        elif harness == "claude":
            args = ["--model", "claude-sonnet-4-6", "--allowedTools", "Bash"]
        else:
            home = run / "hermes"
            home.mkdir()
            env.update(
                OPENAI_API_KEY="codegotchi-test-only", OPENAI_BASE_URL=base + "/v1"
            )
            command = "'" + str(BINARY) + "' hook hermes"
            events = [
                "on_session_start",
                "on_session_end",
                "pre_tool_call",
                "post_tool_call",
            ]
            (home / "config.yaml").write_text(
                json.dumps(
                    {
                        "hooks": (
                            {e: [{"command": command, "timeout": 5}] for e in events}
                            if a.get("hermes_hooks", "approved") != "none" else {}
                        ),
                        "model": {
                            "provider": "openai",
                            "default": "test",
                            "base_url": base + "/v1",
                        },
                        "terminal": {"backend": "local"},
                        "agent": {"max_turns": 3},
                        "compression": {"enabled": False},
                    }
                )
            )
            # Only these reviewed generated hooks are approved, in a disposable home.
            if a.get("hermes_hooks", "approved") == "approved":
                (home / "shell-hooks-allowlist.json").write_text(
                    json.dumps(
                        {"approvals": [{"event": e, "command": command} for e in events]}
                    )
                )
            args = [
                "chat",
                "--provider",
                "openai",
                "--model",
                "test",
                "--toolsets",
                "terminal",
                "--max-turns",
                "3",
            ]
        title = f"codegotchi-interactive-{harness}-{os.getpid()}"
        child_command = (
            [programs[harness], *args]
            if a.get("direct")
            else [
                str(BINARY),
                "run",
                "--ui",
                a.get("ui", "terminal"),
                "--terminal-theme",
                a.get("terminal_theme", options.terminal_theme),
                "--",
                harness,
                *args,
            ]
        )
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
        return capture("startup")
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


def finish_turn(token):
    def settled():
        state = summary()
        return (
            state
            if token in case["toolResults"] and state["activity"] == "WaitingForUser"
            else False
        )

    return wait(settled, 30)


def verify(h):
    do("start", harness=h, ui=options.ui)
    if h in ["pi", "omp"]:
        wait(lambda: summary()["sessionCount"] > 0, 60)
        time.sleep(0.5)
    if h == "claude":
        # Fresh disposable home: matching theme, test API key, notes, workspace trust.
        for keys in [
            ["Down", "Return"] if options.background == "light" else ["Return"],
            ["Up", "Return"],
            ["Return"],
            ["Down", "Return"],
        ]:
            do("key", keys=keys, wait=2)
            do("capture", name="onboarding-" + str(len(records)))
    elif h == "hermes":
        do(
            "key", keys=["Return"], wait=12
        )  # decline telemetry; allow classic REPL initialization
    do("text", text="CG_INPUT_EDIT_OX")
    do("key", keys=["Left"])
    do("text", text="K")
    do("key", keys=["End", "BackSpace"])
    do("capture", name="edited")
    do("key", keys=["Return"], wait=1)
    first = finish_turn("CG_INPUT_EDIT_OK")
    assert (case["root"] / "CG_INPUT_EDIT_OK").read_text() == "CG_INPUT_EDIT_OK"
    assert case["promptTexts"][0].endswith("CG_INPUT_EDIT_OK")
    assert not case["toolResults"]["CG_INPUT_EDIT_OK"]["denied"]
    do("capture", name="response")
    do("key", keys=["Up"])
    do("capture", name="history")
    do("key", keys=["Down"])
    do("paste", text="CG_PASTE_LINE_ONE\nCG_PASTE_LINE_TWO")
    assert "CG_PASTE_LINE_ONE" not in case["issued"], "paste submitted before Enter"
    for name, cols, rows in [
        ("compact", 100, 30),
        ("minimal", 80, 21),
        ("narrow", 64, 45),
        ("full", 120, 45),
    ]:
        do("resize", cols=cols, rows=rows, name=name + "-composer")
        # Check live key forwarding at every size while preserving the pasted text.
        do("key", keys=["End", "BackSpace"])
        do("text", text="O")
        do("capture", name=name + "-edited")
    do("focus")
    do("capture", name="focus-return")
    do("key", keys=["Return"], wait=1)
    pasted = finish_turn("CG_PASTE_LINE_ONE")
    assert case["promptTexts"][1].endswith("CG_PASTE_LINE_ONE\nCG_PASTE_LINE_TWO")
    assert (case["root"] / "CG_PASTE_LINE_ONE").read_text() == "CG_PASTE_LINE_ONE"
    do("capture", name="paste-response")
    do("strict")
    do("text", text="CG_STRICT_PROBE")
    do("key", keys=["Return"], wait=1)
    strict = finish_turn("CG_STRICT_PROBE")
    assert case["toolResults"]["CG_STRICT_PROBE"]["denied"]
    assert not (case["root"] / "CG_STRICT_PROBE").exists()
    if h == "claude":
        do("key", keys=["ctrl+o"])
    do("capture", name="strict-response")
    if h == "claude":
        do("key", keys=["ctrl+o"])
    # A real food drag must reach the server and must leave the composer usable.
    do("resize", cols=80, rows=21, name="care-before")
    before = summary()
    do("feed", col=11, row=19, cols=80, rows=21)
    after = summary()
    assert after["needs"]["hunger"] < before["needs"]["hunger"]
    assert after["careCount"] > before["careCount"]
    do("capture", name="care-after")
    do("text", text="CG_CARE_PROBE")
    do("key", keys=["Return"], wait=1)
    care = finish_turn("CG_CARE_PROBE")
    assert case["toolResults"]["CG_CARE_PROBE"]["denied"]
    # Care lowered hunger, but other neglected needs still block this tool.
    do("capture", name="care-keyboard-response")
    protocol = (case["root"] / "protocol.txt").read_text()
    (OUT / f"{h}-protocol.txt").write_text(protocol)
    assert "event=paste bracketed=true delivered=true" in protocol
    for dimensions in ["120x45", "100x30", "80x21", "64x45"]:
        assert f"resize physical={dimensions}" in protocol
    do("text", text="/exit" if h in ["claude", "hermes"] else "/quit")
    do("key", keys=["Return"], wait=1)
    wait(lambda: terminal.poll() is not None, 10)
    assert terminal.returncode == 0
    assert not list((case["root"] / "runtime/codegotchi").glob("session-*.json"))
    assert not list((case["root"] / "runtime/codegotchi").glob("integration-*"))
    return {
        "harness": h,
        "binarySha256": BINARY_SHA256,
        "program": programs[h],
        "ui": options.ui,
        "background": options.background,
        "terminalTheme": options.terminal_theme,
        "editedPromptExact": True,
        "multilinePasteExact": True,
        "editingAtAllSizes": True,
        "focusReturn": True,
        "strictDenialInNativeToolResult": True,
        "strictSentinelAbsent": True,
        "foodDragChangedAuthoritativeState": True,
        "keyboardAfterCare": True,
        "normalExitCode": terminal.returncode,
        "runtimeCleaned": True,
        "workPoints": care["workPoints"],
        "events": care["eventCount"],
        "passed": True,
    }


results = []
print(json.dumps({"ready": True, "privateRoot": str(TMP)}), flush=True)
try:
    if options.interactive:
        for line in sys.stdin:
            try:
                a = json.loads(line)
                r = do(**a)
                print(json.dumps({"result": r}), flush=True)
                if a["op"] == "quit":
                    break
            except Exception as e:
                print(
                    json.dumps({"error": type(e).__name__ + ": " + str(e)}), flush=True
                )
    else:
        for h in selected:
            try:
                result = verify(h)
                results.append(result)
                print(json.dumps(result), flush=True)
            except Exception as e:
                results.append(
                    {
                        "harness": h,
                        "passed": False,
                        "error": type(e).__name__ + ": " + str(e),
                    }
                )
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
        stop(proc)
    server.shutdown()
if results and not all(r["passed"] for r in results):
    sys.exit(1)
