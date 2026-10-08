#!/usr/bin/env python3
"""Opt-in live-provider acceptance for the Pi and Claude Code adapters.

This check is deliberately separate from ``verify-harness-keyboard.py``.  It
uses a real provider request and therefore must never start by accident in CI.
The Pi OAuth file is copied into a disposable home, while Claude receives an
API key only from the named environment variable.  No credential bytes are
written under the output directory and the generated transcript stays in a
tmpfs directory that is removed on normal exit.

The check exercises a small live conversation, a tool request with the native
permission UI, cancellation while a response is in flight, a follow-up in the
same conversation, and a process restart with the CLI's continue/resume flag.
The provider model is allowed to answer naturally; assertions only use unique
probe markers and sanitized process/runtime facts.

Example (Pi, explicitly opting into the stored OAuth copy):

    python3 scripts/verify-harness-provider.py --harness pi \
      --pi-auth-source "$HOME/.pi/agent/auth.json"

Claude requires an explicitly supplied ``ANTHROPIC_API_KEY`` (or a different
variable passed with ``--claude-api-key-env``), or an opt-in
``--claude-auth-source`` pointing at a real ``.credentials.json``. If neither
is available the script records a concrete blocker and exits without starting
Claude.
"""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
import pathlib
import secrets
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parents[1]
DEFAULT_OUTPUT = ROOT / "docs/verification/multi-harness/provider-live"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--binary", type=pathlib.Path, default=ROOT / "target/release/codegotchi"
    )
    parser.add_argument("--output", type=pathlib.Path, default=DEFAULT_OUTPUT)
    parser.add_argument(
        "--harness", action="append", choices=["pi", "claude"], default=None
    )
    parser.add_argument(
        "--pi-program",
        type=pathlib.Path,
        default=None,
        help="Pi executable (defaults to pi resolved from PATH)",
    )
    parser.add_argument(
        "--claude-program",
        type=pathlib.Path,
        default=None,
        help="Claude executable (defaults to claude resolved from PATH)",
    )
    parser.add_argument(
        "--pi-auth-source",
        type=pathlib.Path,
        help="Explicitly opt into copying this Pi auth file into a private home",
    )
    parser.add_argument(
        "--claude-api-key-env",
        default="ANTHROPIC_API_KEY",
        metavar="NAME",
        help="Environment variable whose value is passed as ANTHROPIC_API_KEY (never logged)",
    )
    parser.add_argument(
        "--claude-auth-source",
        type=pathlib.Path,
        help="Optional .credentials.json file or directory to copy into a private Claude config",
    )
    parser.add_argument(
        "--pi-model", default="gpt-5.5", help="Pi OpenAI Codex model id"
    )
    parser.add_argument(
        "--claude-model", default="claude-haiku-4-5", help="Claude model alias"
    )
    parser.add_argument(
        "--turn-timeout", type=float, default=120.0, help="Provider response timeout"
    )
    return parser.parse_args()


OPTIONS = parse_args()
OUT = OPTIONS.output.resolve()
OUT.mkdir(parents=True, exist_ok=True)
TMP = pathlib.Path(
    tempfile.mkdtemp(
        prefix="codegotchi-provider-live-",
        dir="/dev/shm" if pathlib.Path("/dev/shm").is_dir() else None,
    )
)


def file_digest(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def stat_identity(pid: int) -> tuple[int, str] | None:
    try:
        fields = pathlib.Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
        return int(fields[1]), fields[19]
    except (OSError, ValueError, IndexError):
        return None


def process_tree() -> set[int]:
    """Return our session tree before any test process is launched."""

    protected: set[int] = set()
    pid = os.getpid()
    while pid and pid not in protected:
        protected.add(pid)
        identity = stat_identity(pid)
        pid = identity[0] if identity else 0
    changed = True
    while changed:
        changed = False
        for entry in pathlib.Path("/proc").iterdir():
            if not entry.name.isdigit():
                continue
            candidate = int(entry.name)
            identity = stat_identity(candidate)
            if identity and identity[0] in protected and candidate not in protected:
                protected.add(candidate)
                changed = True
    return protected


PROTECTED = process_tree()
OWNED: list[tuple[subprocess.Popen[bytes], str, str]] = []


def launch(
    args: list[str],
    env: dict[str, str],
    log: pathlib.Path,
    marker: str,
    cwd: pathlib.Path | None = None,
) -> subprocess.Popen[bytes]:
    stream = log.open("wb")
    process = subprocess.Popen(
        args,
        cwd=str(cwd) if cwd else None,
        env=env,
        stdout=stream,
        stderr=subprocess.STDOUT,
    )
    stream.close()
    identity = stat_identity(process.pid)
    if identity is None or identity[0] != os.getpid():
        raise RuntimeError(
            f"owned process {process.pid} did not retain this launcher as parent"
        )
    OWNED.append((process, identity[1], marker))
    return process


def stop(process: subprocess.Popen[bytes] | None) -> None:
    if process is None or process.poll() is not None:
        return
    item = next((entry for entry in OWNED if entry[0] is process), None)
    if item is None:
        raise RuntimeError("refusing to stop an untracked process")
    _, start, marker = item
    if process.pid in PROTECTED:
        raise RuntimeError(f"refusing to stop protected process {process.pid}")
    identity = stat_identity(process.pid)
    if identity is None:
        # The verified child may have exited between poll() and /proc lookup.
        # There is no live process to signal in this case.
        return
    cmdline = pathlib.Path(f"/proc/{process.pid}/cmdline")
    if not cmdline.exists() or marker.encode() not in cmdline.read_bytes():
        # A just-exited child can briefly remain as a zombie, and a PID can be
        # reused. In either case the unique marker is absent, so there is no
        # verified target to signal.
        return
    if identity != (os.getpid(), start):
        raise RuntimeError(f"owned process identity changed for {process.pid}")
    print(
        subprocess.check_output(
            ["ps", "-o", "pid,ppid,stat,etime,cmd", "-p", str(process.pid)],
            text=True,
        ).strip(),
        file=sys.stderr,
    )
    process.terminate()
    try:
        process.wait(timeout=10)
    except subprocess.TimeoutExpired:
        # This is still a verified child with the same kernel start time.  TERM
        # is preferred; a second TERM gives wrappers a chance to forward it.
        process.send_signal(signal.SIGTERM)
        process.wait(timeout=10)


def wait_for(check, seconds: float, description: str):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        value = check()
        if value:
            return value
        time.sleep(0.1)
    raise TimeoutError(f"timed out waiting for {description}")


def activity_label(value: object) -> str | None:
    """Normalize the JSON shape of AgentActivityState without logging it."""

    if isinstance(value, str):
        return value.lower()
    if isinstance(value, dict) and len(value) == 1:
        key, nested = next(iter(value.items()))
        if isinstance(nested, str):
            return nested.lower()
        if nested is None:
            return str(key).lower()
        return activity_label(nested)
    return None


def clean_env() -> dict[str, str]:
    env = dict(os.environ)
    prefixes = (
        "ANTHROPIC_",
        "OPENAI_",
        "PI_",
        "CLAUDE_",
        "CODEGOTCHI_",
        "XDG_",
    )
    sensitive_words = ("API_KEY", "AUTH_TOKEN", "ACCESS_TOKEN", "SECRET")
    for key in list(env):
        if key.startswith(prefixes) or any(word in key for word in sensitive_words):
            env.pop(key, None)
    env.update(
        TERM="xterm-256color",
        COLORTERM="truecolor",
        NO_COLOR="",
    )
    return env


def auth_shape(path: pathlib.Path) -> dict[str, object]:
    """Validate Pi auth without returning any credential material."""

    if not path.is_file():
        return {"available": False, "reason": f"auth source is not a file: {path}"}
    try:
        parsed = json.loads(path.read_text())
    except Exception as exc:  # pragma: no cover - diagnostic path
        return {
            "available": False,
            "reason": f"could not parse auth JSON: {type(exc).__name__}",
        }
    provider = parsed.get("openai-codex") if isinstance(parsed, dict) else None
    if not isinstance(provider, dict):
        return {"available": False, "reason": "auth JSON has no openai-codex record"}
    required = ["access", "refresh", "expires", "accountId"]
    missing = [key for key in required if not provider.get(key)]
    if missing:
        return {
            "available": False,
            "reason": f"openai-codex record lacks {','.join(missing)}",
        }
    expiry = provider.get("expires")
    expiry_seconds = (
        float(expiry) / 1000
        if isinstance(expiry, (int, float)) and expiry > 10**11
        else float(expiry)
    )
    remaining = expiry_seconds - time.time()
    return {
        "available": remaining > 0,
        "provider": "openai-codex",
        "auth_type": "oauth",
        "expires_utc": dt.datetime.fromtimestamp(
            expiry_seconds, dt.timezone.utc
        ).isoformat(),
        "remaining_seconds": round(max(remaining, 0), 1),
        "reason": None if remaining > 0 else "openai-codex OAuth record is expired",
    }


class Display:
    def __init__(self, env: dict[str, str]) -> None:
        self.env = env
        self.number = self._choose_display()
        self.authority = TMP / "xauthority"
        self.authority.parent.mkdir(parents=True, exist_ok=True)
        subprocess.check_call(
            [
                "xauth",
                "-f",
                str(self.authority),
                "add",
                f"localhost:{self.number}",
                ".",
                secrets.token_hex(16),
            ],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        self.env.update(
            DISPLAY=f"localhost:{self.number}", XAUTHORITY=str(self.authority)
        )
        self.xvfb = launch(
            [
                "Xvfb",
                f":{self.number}",
                "-screen",
                "0",
                "1800x1100x24",
                "-nolisten",
                "unix",
                "-listen",
                "tcp",
                "-auth",
                str(self.authority),
            ],
            self.env,
            TMP / "xvfb.log",
            f"Xvfb :{self.number}",
        )
        wait_for(
            lambda: subprocess.run(
                ["xdotool", "getdisplaygeometry"],
                env=self.env,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            ).returncode
            == 0,
            20,
            "Xvfb",
        )
        self.wm = launch(["openbox"], self.env, TMP / "openbox.log", "openbox")
        wait_for(
            lambda: "window id"
            in subprocess.run(
                ["xprop", "-root", "_NET_SUPPORTING_WM_CHECK"],
                env=self.env,
                capture_output=True,
                text=True,
            ).stdout,
            20,
            "openbox",
        )

    @staticmethod
    def _choose_display() -> int:
        for number in range(300, 400):
            probe = socket.socket()
            try:
                probe.bind(("127.0.0.1", 6000 + number))
                return number
            except OSError:
                pass
            finally:
                probe.close()
        raise RuntimeError("no free private X display")

    def x(self, args: list[str]) -> str:
        return subprocess.check_output(
            ["xdotool", *args], env=self.env, text=True, timeout=10
        ).strip()

    def close(self) -> None:
        stop(self.wm)
        stop(self.xvfb)


def token_in(path: pathlib.Path, token: str) -> bool:
    try:
        return token in path.read_text(errors="replace")
    except OSError:
        return False


PERMISSION_MARKERS = (
    "permission",
    "allow",
    "approve",
    "do you want",
    "run this command",
    "execute",
)


class LiveRun:
    def __init__(
        self,
        harness: str,
        program: pathlib.Path,
        args: list[str],
        env: dict[str, str],
        display: Display,
        run_dir: pathlib.Path,
    ):
        self.harness = harness
        self.program = program
        self.args = args
        self.env = env
        self.display = display
        self.run_dir = run_dir
        self.transcript = run_dir / "terminal.log"
        self.terminal: subprocess.Popen[bytes] | None = None
        self.window: str | None = None
        self.frames: list[str] = []
        self.permission_observed = False
        self.approval_sent = False

    def start(self, resume: bool = False) -> None:
        title = f"codegotchi-provider-{self.harness}-{os.getpid()}"
        child_args = list(self.args)
        if resume:
            child_args.append("--continue")
        command = [
            str(OPTIONS.binary.resolve()),
            "run",
            "--ui",
            "terminal",
            "--terminal-theme",
            "night",
            "--",
            self.harness,
            *child_args,
        ]
        self.env[f"CODEGOTCHI_REAL_{self.harness.upper()}"] = str(self.program)
        self.terminal = launch(
            [
                "xterm",
                "-l",
                "-lf",
                str(self.transcript),
                "-bg",
                "#101218",
                "-fg",
                "#e8e8e8",
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
                *command,
            ],
            self.env,
            self.run_dir / "xterm-process.log",
            title,
            self.run_dir,
        )
        self.window = wait_for(
            lambda: self.find_window(title),
            20,
            "xterm window",
        )
        self.display.x(["windowactivate", "--sync", self.window])
        if self.harness == "pi":
            # Pi may download fd and finish extension discovery after the
            # xterm is already visible. Typing into the pre-ready composer can
            # silently lose the first prompt, so wait for the real model row.
            wait_for(
                lambda: token_in(self.transcript, "(openai-codex)")
                and token_in(self.transcript, "fd installed to"),
                75,
                "Pi startup and extension registration",
            )
            time.sleep(1)
        else:
            time.sleep(8)

    def find_window(self, title: str) -> str | None:
        for option in ("--name", "--classname", "--title"):
            found = subprocess.run(
                ["xdotool", "search", option, title],
                env=self.display.env,
                capture_output=True,
                text=True,
            ).stdout.strip()
            if found:
                return found.split("\n")[0]
        return None

    def capture(self, name: str) -> pathlib.Path:
        if not self.window:
            raise RuntimeError("cannot capture before start")
        path = OUT / f"{self.harness}-{name}.png"
        subprocess.run(
            ["import", "-window", self.window, str(path)],
            env=self.display.env,
            check=True,
            timeout=10,
        )
        self.frames.append(path.name)
        return path

    def session_files(self) -> list[pathlib.Path]:
        # Pi stores one JSONL session under its private agent directory. Claude
        # stores project sessions under its config/home tree. Searching only
        # this run directory keeps the evidence isolated and avoids touching
        # the user's real history.
        return sorted(self.run_dir.rglob("*.jsonl"))

    def session_ids(self) -> set[str]:
        ids: set[str] = set()
        for path in self.session_files():
            try:
                lines = path.read_text(errors="replace").splitlines()
            except OSError:
                continue
            for line in lines:
                try:
                    record = json.loads(line)
                except json.JSONDecodeError:
                    continue
                if isinstance(record, dict) and record.get("type") == "session":
                    session_id = record.get("id")
                    if isinstance(session_id, str):
                        ids.add(session_id)
        return ids

    def session_messages(self) -> list[tuple[str, object, dict[str, object]]]:
        messages: list[tuple[str, object, dict[str, object]]] = []
        for path in self.session_files():
            try:
                lines = path.read_text(errors="replace").splitlines()
            except OSError:
                continue
            for line in lines:
                try:
                    record = json.loads(line)
                except json.JSONDecodeError:
                    continue
                if not isinstance(record, dict):
                    continue
                message = record.get("message")
                if not isinstance(message, dict):
                    message = record
                role = message.get("role")
                if isinstance(role, str):
                    messages.append((role, message.get("content"), record))
        return messages

    @staticmethod
    def content_text(content: object) -> str:
        if isinstance(content, str):
            return content
        if isinstance(content, list):
            parts: list[str] = []
            for block in content:
                if isinstance(block, dict):
                    for key in ("text", "content", "output"):
                        value = block.get(key)
                        if isinstance(value, str):
                            parts.append(value)
                        elif isinstance(value, list):
                            parts.append(LiveRun.content_text(value))
            return "\n".join(parts)
        if isinstance(content, dict):
            return LiveRun.content_text(content.get("content", content.get("text", "")))
        return ""

    def user_prompt_contains(self, marker: str) -> bool:
        return any(
            role == "user" and marker in self.content_text(content)
            for role, content, _ in self.session_messages()
        )

    def assistant_marker(self, marker: str) -> bool:
        return any(
            role == "assistant" and marker in self.content_text(content)
            for role, content, _ in self.session_messages()
        )

    def tool_result_marker(self, marker: str) -> bool:
        for role, content, record in self.session_messages():
            if role in {
                "tool",
                "toolResult",
                "tool_result",
            } and marker in self.content_text(content):
                return True
            # Pi records a tool result as role=toolResult. Claude's JSONL
            # history can put tool_result blocks in a user-shaped message.
            message = record.get("message")
            if isinstance(message, dict) and isinstance(message.get("content"), list):
                for block in message["content"]:
                    if isinstance(block, dict) and block.get("type") in {
                        "tool_result",
                        "toolResult",
                    }:
                        if marker in self.content_text(block):
                            return True
        return False

    def assistant_text_contains(self, marker: str) -> bool:
        return self.assistant_marker(marker)

    def assistant_aborted_after(self, marker: str) -> bool:
        started = False
        for role, content, record in self.session_messages():
            if role == "user" and marker in self.content_text(content):
                started = True
                continue
            if started and role == "assistant":
                message = record.get("message")
                if not isinstance(message, dict):
                    message = record
                if (
                    message.get("stopReason") == "aborted"
                    or message.get("errorMessage") == "Operation aborted"
                ):
                    return True
        return False

    def assistant_thinking_after(self, marker: str) -> bool:
        started = False
        for role, content, _ in self.session_messages():
            if role == "user" and marker in self.content_text(content):
                started = True
                continue
            if started and role == "assistant":
                if isinstance(content, list) and any(
                    isinstance(block, dict)
                    and block.get("type") in {"thinking", "reasoning"}
                    for block in content
                ):
                    return True
        return False

    def transcript_contains(self, *markers: str) -> bool:
        try:
            text = self.transcript.read_text(errors="replace").lower()
        except OSError:
            return False
        return any(marker.lower() in text for marker in markers)

    def api_state(self) -> dict[str, object] | None:
        metadata_files = sorted(
            (self.run_dir / "xdg-runtime" / "codegotchi").glob("session-*.json")
        )
        if not metadata_files:
            return None
        try:
            metadata = json.loads(metadata_files[-1].read_text())
            request = urllib.request.Request(
                metadata["loopbackBaseUrl"] + "/api/v1/state",
                headers={
                    "Authorization": "Bearer " + metadata["bearerToken"],
                    "X-CodeGotchi-Debug": "1",
                },
            )
            with urllib.request.urlopen(request, timeout=2) as response:
                value = json.load(response)
            return value if isinstance(value, dict) else None
        except (OSError, KeyError, json.JSONDecodeError, urllib.error.URLError):
            return None

    def backend_settled(self) -> bool:
        state = self.api_state()
        label = activity_label(state.get("activity")) if state else None
        return label in {"idle", "waitingforuser", "blocked"}

    def wait_backend_settled(self, description: str) -> None:
        wait_for(lambda: self.backend_settled(), 20, description)

    def wait_assistant(self, marker: str, timeout: float) -> bool:
        try:
            wait_for(
                lambda: self.assistant_marker(marker),
                timeout,
                f"assistant marker {marker}",
            )
            return True
        except TimeoutError:
            return False

    def wait_tool_result(self, marker: str, timeout: float) -> bool:
        try:
            wait_for(
                lambda: self.tool_result_marker(marker),
                timeout,
                f"tool result {marker}",
            )
            return True
        except TimeoutError:
            return False

    def keys(self, *keys: str) -> None:
        if not self.window:
            raise RuntimeError("cannot send keys before start")
        self.display.x(["key", "--window", self.window, "--clearmodifiers", *keys])
        time.sleep(0.25)

    def text(self, value: str) -> None:
        if not self.window:
            raise RuntimeError("cannot type before start")
        self.display.x(["type", "--window", self.window, "--delay", "8", "--", value])
        time.sleep(0.25)

    def prompt(self, value: str) -> None:
        self.text(value)
        self.keys("Return")

    def permission_seen(self, start: int = 0) -> bool:
        try:
            text = self.transcript.read_text(errors="replace")[start:].lower()
        except OSError:
            return False
        return any(marker in text for marker in PERMISSION_MARKERS)

    def approve_if_needed(self, start: int = 0) -> bool:
        if self.permission_seen(start):
            self.permission_observed = True
            self.capture("approval-prompt")
            self.keys("Return")
            self.approval_sent = True
            return True
        return False

    def close_cli(self) -> None:
        if self.terminal is None or self.terminal.poll() is not None:
            return
        self.prompt("/exit" if self.harness == "claude" else "/quit")
        wait_for(
            lambda: self.terminal and self.terminal.poll() is not None, 20, "CLI exit"
        )

    def output_digest(self) -> str:
        return file_digest(self.transcript) if self.transcript.exists() else ""


def resolve_claude_credentials(source: pathlib.Path | None) -> pathlib.Path | None:
    if source is None:
        return None
    path = source.expanduser().resolve()
    if path.is_file():
        return path if path.name == ".credentials.json" else None
    if path.is_dir():
        candidate = path / ".credentials.json"
        return candidate if candidate.is_file() else None
    return None


def claude_credentials_shape(source: pathlib.Path | None) -> dict[str, object]:
    path = resolve_claude_credentials(source)
    if path is None:
        if source is None:
            return {
                "available": False,
                "reason": "no .credentials.json source supplied",
            }
        return {
            "available": False,
            "reason": "credential source must be a directory containing .credentials.json or that file",
        }
    try:
        parsed = json.loads(path.read_text())
    except Exception as exc:  # pragma: no cover - diagnostic path
        return {
            "available": False,
            "reason": f"could not parse Claude credentials: {type(exc).__name__}",
        }
    oauth = parsed.get("claudeAiOauth") if isinstance(parsed, dict) else None
    if not isinstance(oauth, dict):
        return {
            "available": False,
            "reason": "Claude credentials have no claudeAiOauth record",
        }
    access = oauth.get("accessToken") or oauth.get("access_token")
    refresh = oauth.get("refreshToken") or oauth.get("refresh_token")
    if (
        not isinstance(access, str)
        or not access
        or not isinstance(refresh, str)
        or not refresh
    ):
        return {
            "available": False,
            "reason": "Claude claudeAiOauth record lacks access and refresh tokens",
        }
    expiry = oauth.get("expiresAt") or oauth.get("expires_at")
    expires_utc: str | None = None
    if isinstance(expiry, (int, float)):
        expiry_seconds = float(expiry) / 1000 if expiry > 10**11 else float(expiry)
        expires_utc = dt.datetime.fromtimestamp(
            expiry_seconds, dt.timezone.utc
        ).isoformat()
        if expiry_seconds <= time.time():
            return {
                "available": False,
                "reason": "Claude OAuth credentials are expired",
                "expires_utc": expires_utc,
            }
    result: dict[str, object] = {
        "available": True,
        "auth_type": "oauth",
        "source": str(path),
    }
    if expires_utc is not None:
        result["expires_utc"] = expires_utc
    return result


def claude_auth_available(
    key_name: str, credential_source: pathlib.Path | None
) -> dict[str, object]:
    value = os.environ.get(key_name, "")
    if value:
        return {"available": True, "auth_type": "api-key-env"}
    oauth = claude_credentials_shape(credential_source)
    if oauth.get("available"):
        return oauth
    return {
        "available": False,
        "reason": f"{key_name} is not set and no valid Claude OAuth credentials were supplied",
        "oauth_reason": oauth.get("reason"),
    }


def prepare_env(
    harness: str,
    run_dir: pathlib.Path,
    pi_auth: pathlib.Path | None,
    claude_key_name: str,
    claude_auth_source: pathlib.Path | None,
) -> dict[str, str]:
    env = clean_env()
    home = run_dir / "home"
    home.mkdir(parents=True, exist_ok=True)
    env.update(
        HOME=str(home),
        XDG_CONFIG_HOME=str(run_dir / "xdg-config"),
        XDG_STATE_HOME=str(run_dir / "xdg-state"),
        XDG_CACHE_HOME=str(run_dir / "xdg-cache"),
        XDG_RUNTIME_DIR=str(run_dir / "xdg-runtime"),
        CODEGOTCHI_BROWSER="none",
        CODEGOTCHI_ENABLE_DEBUG="1",
        PI_TELEMETRY="0",
        CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC="1",
    )
    (run_dir / "xdg-runtime").mkdir(mode=0o700, parents=True)
    if harness == "pi":
        if pi_auth is None:
            raise RuntimeError(
                "Pi requires --pi-auth-source for an explicit credential copy"
            )
        agent = home / ".pi" / "agent"
        agent.mkdir(parents=True, exist_ok=True)
        # Only authentication and the already cached provider catalog are
        # copied. User settings, packages, extensions, and default providers
        # must not leak into a live acceptance run.
        for name in ["auth.json", "models-store.json"]:
            source = pi_auth.parent / name
            if source.exists():
                shutil.copy2(source, agent / name)
        # Avoid the user's configured provider default; the copied OAuth record
        # is selected explicitly on the child command line.
        env["PI_CODING_AGENT_DIR"] = str(agent)
    else:
        config = run_dir / "claude-config"
        config.mkdir(mode=0o700, parents=True, exist_ok=True)
        env["CLAUDE_CONFIG_DIR"] = str(config)
        # Claude Code only reads ANTHROPIC_API_KEY. The option names the
        # caller's source variable; it must not be forwarded as an unknown
        # provider variable.
        if os.environ.get(claude_key_name):
            env["ANTHROPIC_API_KEY"] = os.environ[claude_key_name]
            if claude_key_name != "ANTHROPIC_API_KEY":
                env.pop(claude_key_name, None)
        credential = resolve_claude_credentials(claude_auth_source)
        if credential is not None:
            destination = config / ".credentials.json"
            shutil.copy2(credential, destination)
            try:
                destination.chmod(0o600)
            except OSError:
                pass
    return env


def run_live(
    harness: str,
    display: Display,
    pi_auth: pathlib.Path | None,
    claude_auth_source: pathlib.Path | None,
) -> dict[str, object]:
    run_dir = TMP / harness
    run_dir.mkdir()
    if harness == "pi":
        requested = OPTIONS.pi_program or pathlib.Path(shutil.which("pi") or "")
        program = requested.expanduser().resolve()
        env = prepare_env(
            harness,
            run_dir,
            pi_auth,
            OPTIONS.claude_api_key_env,
            claude_auth_source,
        )
        args = [
            "--provider",
            "openai-codex",
            "--model",
            OPTIONS.pi_model,
            "--thinking",
            "off",
            "--no-extensions",
            "--no-context-files",
        ]
    else:
        requested = OPTIONS.claude_program or pathlib.Path(shutil.which("claude") or "")
        program = requested.expanduser().resolve()
        env = prepare_env(
            harness,
            run_dir,
            pi_auth,
            OPTIONS.claude_api_key_env,
            claude_auth_source,
        )
        args = ["--model", OPTIONS.claude_model]
    env.update(DISPLAY=display.env["DISPLAY"], XAUTHORITY=display.env["XAUTHORITY"])
    if not program.is_file() or not os.access(program, os.X_OK):
        return {
            "harness": harness,
            "status": "blocked",
            "reason": f"program unavailable: {program}",
        }

    live = LiveRun(harness, program, args, env, display, run_dir)
    evidence: dict[str, object] = {
        "harness": harness,
        "status": "failed",
        "model": OPTIONS.pi_model if harness == "pi" else OPTIONS.claude_model,
        "frames": live.frames,
        "actions": [],
    }
    minimum_turn_passed = False
    minimum_tool_passed = False
    try:
        live.start()
        live.capture("startup")
        # Claude's first-run screens are real native prompts.  The API key is
        # already in the environment, but current releases still ask for a
        # theme, notes, and workspace trust in a new config directory.
        if harness == "claude":
            for keys in [["Return"], ["Up", "Return"], ["Return"], ["Down", "Return"]]:
                live.keys(*keys)
                time.sleep(1.5)
            live.capture("onboarding")

        turn1 = f"LIVE_AUTH_{harness.upper()}_TURN_1_OK"
        resume_secret = "LIVE_SECRET_" + secrets.token_hex(6).upper()
        live.prompt(
            f"Remember this private token for later in our conversation: {resume_secret}. Reply with exactly {turn1} and no other text. Do not use tools."
        )
        evidence["actions"].append("live_turn_1")
        if not live.wait_assistant(turn1, OPTIONS.turn_timeout):
            raise RuntimeError(
                "live provider did not record an assistant turn-one marker"
            )
        minimum_turn_passed = True
        live.wait_backend_settled("backend settle after first live assistant turn")
        live.capture("turn-1")

        tool = f"LIVE_AUTH_{harness.upper()}_TOOL_OK"
        permission_cursor = (
            live.transcript.stat().st_size if live.transcript.exists() else 0
        )
        live.prompt(
            f"Use your shell tool to run exactly `printf {tool}` and then report its output. Do not run any other command."
        )
        evidence["actions"].append("native_tool_request")
        if harness == "claude":
            # A permission prompt should be visible before approval. Poll only
            # the bytes written after this tool request; startup help often
            # contains the word "permission" too.
            try:
                wait_for(
                    lambda: live.permission_seen(permission_cursor),
                    35,
                    "native permission prompt",
                )
            except TimeoutError:
                pass
            if not live.approve_if_needed(permission_cursor):
                raise RuntimeError(
                    "Claude tool request had no observable native approval prompt"
                )
            evidence["actions"].append("native_tool_approved")
        else:
            evidence["actions"].append("native_tool_permission_not_applicable")
        if not live.wait_tool_result(tool, OPTIONS.turn_timeout):
            raise RuntimeError(
                "live provider did not record the native-tool result marker"
            )
        minimum_tool_passed = True
        live.wait_backend_settled("backend settle after native tool result")
        live.capture("tool-result")

        long_marker = f"LIVE_AUTH_{harness.upper()}_INTERRUPTED"
        live.prompt(
            f"Write a very detailed explanation of how a shell command starts, using at least 1200 words, and end with {long_marker}."
        )
        evidence["actions"].append("interruption_request")
        active_state: dict[str, object] = {}

        def backend_thinking() -> bool:
            state = live.api_state()
            activity = activity_label(state.get("activity")) if state else None
            if activity == "thinking":
                active_state["activity"] = activity
                return True
            return False

        stream_started = wait_for(
            backend_thinking,
            25,
            "backend Thinking activity for interruption",
        )
        time.sleep(0.5)
        live.keys("Escape")
        evidence["interruption_sent"] = True
        live.capture("interrupted")
        if live.terminal is None or live.terminal.poll() is not None:
            raise RuntimeError("CLI exited after interruption")

        follow = f"LIVE_AUTH_{harness.upper()}_FOLLOW_UP_OK"
        live.prompt(f"Reply with exactly {follow} and no other text. Do not use tools.")
        evidence["actions"].append("follow_up_after_interrupt")
        if not live.wait_assistant(follow, OPTIONS.turn_timeout):
            raise RuntimeError(
                "follow-up after interruption did not record an assistant marker"
            )
        if not live.assistant_aborted_after(long_marker):
            raise RuntimeError(
                "interruption had no assistant stopReason=aborted record"
            )
        live.capture("after-interruption")

        # A few short turns make this a real retained conversation rather than
        # a single health check, while keeping provider usage bounded.
        for index in range(2, 5):
            marker = f"LIVE_AUTH_{harness.upper()}_LONG_{index}_OK"
            live.prompt(
                f"Reply with exactly {marker} and no other text. Do not use tools."
            )
            if not live.wait_assistant(marker, OPTIONS.turn_timeout):
                raise RuntimeError(
                    f"long-session turn {index} did not record an assistant marker"
                )
            evidence["actions"].append(f"long_session_turn_{index}")
        live.capture("long-session")

        live.close_cli()
        evidence["process_exit_code"] = (
            live.terminal.returncode if live.terminal else None
        )
        if live.terminal is None or live.terminal.returncode != 0:
            raise RuntimeError("first CLI process did not exit successfully")

        first_frames = list(live.frames)
        first_permission_observed = live.permission_observed
        first_approval_sent = live.approval_sent
        first_transcript_digest = live.output_digest()
        first_session_ids = live.session_ids()

        # Re-enter through the production launcher and use each CLI's native
        # continue flag. This exercises persisted conversation state plus a new
        # CodeGotchi integration directory.
        resumed = LiveRun(harness, program, args, env, display, run_dir)
        resumed.start(resume=True)
        live = resumed
        resumed_session_ids = live.session_ids()
        shared_session_ids = first_session_ids.intersection(resumed_session_ids)
        if not shared_session_ids:
            raise RuntimeError("continue/resume did not reopen the original session id")
        resume_marker = f"LIVE_AUTH_{harness.upper()}_RESUME_OK"
        if harness == "claude":
            # Continue should skip first-run prompts; give the TUI a moment to
            # restore the conversation before typing into the composer.
            time.sleep(3)
        live.prompt(
            f"We are continuing the previous conversation. Recall the private token I asked you to remember earlier. Reply with exactly {resume_marker} followed by that token and no other text. Do not use tools."
        )
        evidence["actions"].append("native_continue_resume")
        if not live.wait_assistant(resume_marker, OPTIONS.turn_timeout):
            raise RuntimeError(
                "native continue/resume did not record an assistant marker"
            )
        if not live.assistant_text_contains(resume_secret):
            raise RuntimeError("resume answer did not recall the earlier private token")
        live.capture("resumed")
        live.close_cli()
        evidence["resume_exit_code"] = (
            live.terminal.returncode if live.terminal else None
        )
        if live.terminal is None or live.terminal.returncode != 0:
            raise RuntimeError("resumed CLI process did not exit successfully")

        evidence.update(
            {
                "status": "passed",
                "minimum_provider_conversation": True,
                "minimum_native_tool_result": True,
                "native_permission_prompt_observed": bool(
                    first_permission_observed or live.permission_observed
                ),
                "native_permission_approval_sent": bool(
                    first_approval_sent or live.approval_sent
                ),
                "native_permission_status": (
                    "not_applicable" if harness == "pi" else "observed"
                ),
                "interruption_stream_observed": bool(stream_started),
                "interruption_backend_activity": active_state.get("activity"),
                "interruption_abort_indication": True,
                "shared_session_id": True,
                "interruption_kept_process_alive": True,
                "frames": first_frames + list(live.frames),
                "initial_transcript_sha256": first_transcript_digest,
                "transcript_sha256": live.output_digest(),
            }
        )
        if harness == "claude" and not evidence["native_permission_prompt_observed"]:
            evidence["status"] = "incomplete"
            evidence["reason"] = "tool ran without an observable native approval prompt"
    except Exception as exc:
        evidence.update(
            {
                "status": (
                    "passed"
                    if minimum_turn_passed and minimum_tool_passed
                    else "failed"
                ),
                "reason": f"{type(exc).__name__}: {exc}",
                "transcript_sha256": live.output_digest(),
                "frames": list(live.frames),
            }
        )
        if minimum_turn_passed and minimum_tool_passed:
            evidence["extended_status"] = "incomplete"
            evidence["minimum_provider_conversation"] = True
            evidence["minimum_native_tool_result"] = True
        try:
            live.capture("failed")
        except Exception:
            pass
    finally:
        try:
            stop(live.terminal)
        except Exception as exc:
            evidence["cleanup_error"] = f"{type(exc).__name__}: {exc}"
    return evidence


def write_result(name: str, result: dict[str, object]) -> None:
    (OUT / f"{name}.json").write_text(
        json.dumps(result, indent=2, sort_keys=True) + "\n"
    )


def main() -> int:
    selected = OPTIONS.harness or ["pi", "claude"]
    binary = OPTIONS.binary.resolve()
    if not binary.is_file() or not os.access(binary, os.X_OK):
        print(f"codegotchi binary unavailable: {binary}", file=sys.stderr)
        return 2

    previous_availability: dict[str, object] = {}
    availability_file = OUT / "availability.json"
    if availability_file.is_file():
        try:
            loaded = json.loads(availability_file.read_text())
            if isinstance(loaded, dict):
                previous_availability = loaded
        except json.JSONDecodeError:
            pass
    availability: dict[str, object] = {
        **previous_availability,
        "verified_utc": dt.datetime.now(dt.timezone.utc).isoformat(),
        "binary_sha256": file_digest(binary),
        "output_directory": str(OUT),
        "private_tmpfs": str(TMP),
    }
    pi_auth: pathlib.Path | None = None
    pi_auth_digest_before: str | None = None
    claude_auth_source = resolve_claude_credentials(OPTIONS.claude_auth_source)
    claude_auth_digest_before: str | None = None
    if "pi" in selected:
        if OPTIONS.pi_auth_source is None:
            result = {
                "harness": "pi",
                "status": "blocked",
                "reason": "pass --pi-auth-source to opt into a read-only credential copy",
            }
            write_result("pi", result)
            availability["pi_auth"] = result
        else:
            pi_auth = OPTIONS.pi_auth_source.expanduser().resolve()
            shape = auth_shape(pi_auth)
            shape["source"] = str(pi_auth)
            if pi_auth.is_file():
                # Keep the digest in memory only. The public artifact records
                # a boolean after the run instead of a credential fingerprint.
                pi_auth_digest_before = file_digest(pi_auth)
            availability["pi_auth"] = shape
            if not shape.get("available"):
                write_result(
                    "pi",
                    {
                        "harness": "pi",
                        "status": "blocked",
                        "reason": shape.get("reason"),
                    },
                )
    if "claude" in selected:
        availability["claude_auth"] = claude_auth_available(
            OPTIONS.claude_api_key_env, OPTIONS.claude_auth_source
        )
        if claude_auth_source is not None:
            claude_auth_digest_before = file_digest(claude_auth_source)
        if not availability["claude_auth"]["available"]:
            write_result(
                "claude",
                {
                    "harness": "claude",
                    "status": "blocked",
                    "reason": availability["claude_auth"]["reason"],
                },
            )

    (OUT / "availability.json").write_text(
        json.dumps(availability, indent=2, sort_keys=True) + "\n"
    )

    env = clean_env()
    display = Display(env)
    results: dict[str, dict[str, object]] = {}
    try:
        if (
            "pi" in selected
            and pi_auth
            and availability.get("pi_auth", {}).get("available")
        ):
            results["pi"] = run_live("pi", display, pi_auth, None)
            write_result("pi", results["pi"])
        if "claude" in selected and availability.get("claude_auth", {}).get(
            "available"
        ):
            results["claude"] = run_live("claude", display, None, claude_auth_source)
            write_result("claude", results["claude"])
    finally:
        try:
            display.close()
        except Exception as exc:
            availability["display_cleanup_error"] = f"{type(exc).__name__}: {exc}"
        # Keep the tmpfs path in availability.json for debugging, but remove
        # all private homes, transcripts, copied credentials, and runtime state.
        shutil.rmtree(TMP, ignore_errors=True)

    if pi_auth is not None and isinstance(availability.get("pi_auth"), dict):
        availability["pi_auth"]["source_unchanged"] = bool(
            pi_auth_digest_before
            and pi_auth.is_file()
            and file_digest(pi_auth) == pi_auth_digest_before
        )
    if (
        isinstance(availability.get("claude_auth"), dict)
        and claude_auth_source is not None
    ):
        availability["claude_auth"]["source_unchanged"] = bool(
            claude_auth_digest_before
            and claude_auth_source.is_file()
            and file_digest(claude_auth_source) == claude_auth_digest_before
        )
    (OUT / "availability.json").write_text(
        json.dumps(availability, indent=2, sort_keys=True) + "\n"
    )

    print(
        json.dumps({"results": results, "availability": availability}, sort_keys=True)
    )
    statuses = [
        results.get(h, {}).get("status")
        or availability.get(f"{h}_auth", {}).get("status")
        for h in selected
    ]
    return (
        0
        if statuses and all(status in {"passed", "blocked"} for status in statuses)
        else 1
    )


if __name__ == "__main__":
    raise SystemExit(main())
