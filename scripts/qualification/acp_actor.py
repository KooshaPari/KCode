"""Isolated real KCode actor with a loopback synthetic chat-completions provider."""
import argparse
import json
import os
from pathlib import Path
import shlex
import signal
import socket
import subprocess
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


def content(message):
    value = message.get("content", "")
    if isinstance(value, list):
        return "\n".join(block.get("text", "") for block in value if isinstance(block, dict))
    return value or ""


def provider(root):
    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_GET(self):
            payload = json.dumps({"object": "list", "data": [{"id": "fixture-model", "object": "model"}]}).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)

        def do_POST(self):
            request = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
            messages = request.get("messages", [])
            last_user = max((index for index, message in enumerate(messages) if message.get("role") == "user"), default=-1)
            prompt = content(messages[last_user]) if last_user >= 0 else ""
            results = [message for message in messages[last_user + 1:] if message.get("role") == "tool"]
            offered = [tool.get("function", {}).get("name") for tool in request.get("tools", [])]
            call = None
            text = "Synthetic actor ready"
            if results:
                with (root / "tool-results.jsonl").open("a") as stream:
                    stream.write(json.dumps(results) + "\n")
                text = "ORIGINAL_TOOL_RESULT " + "\n".join(content(result) for result in results)
            elif "HARNESSDESK_QUESTION" in prompt:
                call = ("elicitate_mcp", {"intent": "Synthetic integration question", "title": "Synthetic witness", "question": "Return the witness token", "field": {"kind": "text", "label": "Witness token"}, "timeout_secs": 120})
            elif "HARNESSDESK_PERMISSION" in prompt:
                call = ("bash", {"intent": "Write the isolated permission witness", "command": "printf approved >> " + shlex.quote(str(root / "approved.marker"))})
            if call and call[0] not in offered:
                text = "FIXTURE_ERROR tool not offered: " + call[0]
                call = None
            if call:
                delta = {"role": "assistant", "tool_calls": [{"index": 0, "id": "fixture-" + str(time.time_ns()), "type": "function", "function": {"name": call[0], "arguments": json.dumps(call[1])}}]}
                reason = "tool_calls"
            else:
                delta = {"role": "assistant", "content": text}
                reason = "stop"
            chunks = [{"id": "fixture-response", "object": "chat.completion.chunk", "model": "fixture-model", "choices": [{"index": 0, "delta": delta, "finish_reason": None}]},
                      {"id": "fixture-response", "object": "chat.completion.chunk", "model": "fixture-model", "choices": [{"index": 0, "delta": {}, "finish_reason": reason}]}]
            payload = ("".join("data: " + json.dumps(chunk) + "\n\n" for chunk in chunks) + "data: [DONE]\n\n").encode()
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)
    return ThreadingHTTPServer(("127.0.0.1", 0), Handler)


def bootstrap(path, root):
    stream = socket.socket(socket.AF_UNIX)
    stream.settimeout(20)
    stream.connect(str(path))
    reader = stream.makefile("rb")
    request = {"type": "subscribe", "id": 1, "working_dir": str(root), "continue_on_disconnect": True, "crash_on_disconnect": False, "client_instance_id": "isolated-fixture", "client_has_local_history": False, "allow_session_takeover": False}
    stream.sendall(json.dumps(request).encode() + b"\n")
    while True:
        event = json.loads(reader.readline())
        if event.get("type") == "error":
            raise RuntimeError(event)
        if event.get("type") == "done" and event.get("id") == 1:
            break
    stream.sendall(b'{"type":"get_history","id":2}\n')
    while True:
        event = json.loads(reader.readline())
        if event.get("type") == "history" and event.get("id") == 2:
            session = event["session_id"]
            break
    stream.sendall(json.dumps({"type": "message", "id": 3, "content": "HARNESSDESK_BOOTSTRAP", "images": [], "no_reply": False}).encode() + b"\n")
    while True:
        event = json.loads(reader.readline())
        if event.get("type") == "error" and event.get("id") == 3:
            raise RuntimeError(event)
        if event.get("type") == "done" and event.get("id") == 3:
            break
    stream.settimeout(None)
    def drain():
        try:
            for _ in reader:
                pass
        except OSError:
            pass
    threading.Thread(target=drain, daemon=True).start()
    return stream, session


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--duration", type=int, default=600)
    args = parser.parse_args()
    def terminate(*_):
        raise SystemExit(0)
    signal.signal(signal.SIGTERM, terminate)
    binary = args.binary.resolve(strict=True)
    root = args.root.resolve()
    root.mkdir(mode=0o700, parents=True, exist_ok=True)
    if any(root.iterdir()):
        raise RuntimeError("fixture root must be empty")
    home = root / "home"
    home.mkdir(mode=0o700)
    runtime = root / "run"
    runtime.mkdir(mode=0o700)
    path = runtime / "daemon.sock"
    if len(str(path).encode()) + len(".interactions") >= 100:
        raise RuntimeError("use a short /tmp fixture root for Unix socket paths")
    gate = root / "gate.py"
    gate.write_text('import json, sys\npayload=json.load(sys.stdin)\nif "approved.marker" in payload.get("command", ""):\n print("Approve the isolated marker operation", file=sys.stderr)\n sys.exit(3)\n')
    server = provider(root)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    overrides = {"JCODE_HOME": str(home), "JCODE_RUNTIME_DIR": str(runtime), "JCODE_SOCKET": str(path),
                 "JCODE_NO_TELEMETRY": "1", "JCODE_TEMP_SERVER": "1", "JCODE_SERVER_OWNER_PID": str(os.getpid()),
                 "JCODE_OPENAI_COMPAT_API_BASE": "http://127.0.0.1:" + str(server.server_port) + "/v1",
                 "JCODE_OPENAI_COMPAT_API_KEY_NAME": "KCODE_FIXTURE_KEY", "KCODE_FIXTURE_KEY": "synthetic-not-secret",
                 "JCODE_OPENAI_COMPAT_DEFAULT_MODEL": "fixture-model", "JCODE_HOOK_PRE_TOOL": shlex.join([sys.executable, str(gate)])}
    env = {key: value for key, value in os.environ.items() if not key.startswith("JCODE_")}
    env.update(overrides)
    arguments = [str(binary), "--provider", "openai-compatible", "--model", "fixture-model", "--no-update", "--no-selfdev"]
    (root / "environment.json").write_text(json.dumps(overrides, indent=2) + "\n")
    log = (root / "daemon.log").open("wb")
    daemon = subprocess.Popen(arguments + ["serve", "--socket", str(path)], cwd=root, env=env, stdin=subprocess.DEVNULL, stdout=log, stderr=log)
    connection = None
    try:
        deadline = time.monotonic() + 30
        while not path.exists():
            if daemon.poll() is not None or time.monotonic() > deadline:
                raise RuntimeError("isolated daemon did not start; inspect " + str(root / "daemon.log"))
            time.sleep(0.05)
        connection, session = bootstrap(path, root)
        ready = {"sessionId": session, "runtimeId": None, "daemonPid": daemon.pid, "socket": str(path), "cwd": str(root),
                 "acpCommand": arguments + ["acp"], "environmentFile": str(root / "environment.json"),
                 "controllerMeta": {"jcode.interactionController": True}, "questionPrompt": "HARNESSDESK_QUESTION",
                 "permissionPrompt": "HARNESSDESK_PERMISSION", "sideEffect": str(root / "approved.marker"),
                 "originalToolResults": str(root / "tool-results.jsonl")}
        (root / "ready.json").write_text(json.dumps(ready, indent=2) + "\n")
        print(json.dumps(ready), flush=True)
        time.sleep(args.duration)
    finally:
        if connection:
            connection.close()
        if daemon.poll() is None:
            daemon.send_signal(signal.SIGINT)
            try:
                daemon.wait(timeout=10)
            except subprocess.TimeoutExpired:
                daemon.kill()
                daemon.wait()
        server.shutdown()
        log.close()


if __name__ == "__main__":
    main()
