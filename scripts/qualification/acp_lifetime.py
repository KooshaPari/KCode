"""Exercise original held operations and reload through a real isolated ACP actor."""
import json
import os
from pathlib import Path
import selectors
import subprocess
import sys
import tempfile
import time


class Client:
    def __init__(self, ready, env):
        self.process = subprocess.Popen(ready["acpCommand"], cwd=ready["cwd"], env=env,
                                        stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        stderr=subprocess.PIPE)
        self.pending = []
        self.buffer = b""

    def send(self, value):
        self.process.stdin.write(json.dumps(value).encode() + b"\n")
        self.process.stdin.flush()

    def wait(self, predicate):
        deadline = time.monotonic() + 30
        with selectors.DefaultSelector() as selector:
            selector.register(self.process.stdout, selectors.EVENT_READ)
            while time.monotonic() < deadline:
                for index, item in enumerate(self.pending):
                    if predicate(item):
                        return self.pending.pop(index)
                if selector.select(0.1):
                    chunk = os.read(self.process.stdout.fileno(), 65536)
                    if not chunk:
                        raise AssertionError("ACP closed before the expected response")
                    self.buffer += chunk
                    while b"\n" in self.buffer:
                        line, self.buffer = self.buffer.split(b"\n", 1)
                        self.pending.append(json.loads(line))
        raise AssertionError("ACP response timed out; buffered=" + str(self.pending[-8:]))

    def request(self, number, method, params):
        self.send({"jsonrpc": "2.0", "id": number, "method": method, "params": params})
        return self.result(number)

    def result(self, number):
        response = self.wait(lambda item: item.get("id") == number and "method" not in item)
        assert "result" in response, response
        return response["result"]

    def close(self):
        self.process.stdin.close()
        try:
            self.process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait()


def main():
    binary = str(Path(sys.argv[1]).resolve(strict=True))
    with tempfile.TemporaryDirectory(prefix="kc-", dir="/tmp") as directory:
        root = Path(directory).resolve()
        fixture = subprocess.Popen([sys.executable, str(Path(__file__).with_name("acp_actor.py")),
                                    "--binary", binary, "--root", str(root), "--duration", "300"],
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        client = None
        try:
            deadline = time.monotonic() + 40
            while not (root / "ready.json").exists():
                if fixture.poll() is not None or time.monotonic() > deadline:
                    if fixture.poll() is None:
                        fixture.terminate()
                    try:
                        _, stderr = fixture.communicate(timeout=15)
                    except subprocess.TimeoutExpired:
                        fixture.kill()
                        _, stderr = fixture.communicate()
                    raise AssertionError("fixture failed: " + stderr.decode(errors="replace"))
                time.sleep(0.05)
            ready = json.loads((root / "ready.json").read_text())
            env = os.environ.copy()
            env.update(json.loads(Path(ready["environmentFile"]).read_text()))
            client = Client(ready, env)
            client.request(1, "initialize", {"protocolVersion": 1, "clientCapabilities": {"elicitation": {"form": {}}}})
            params = {"sessionId": ready["sessionId"], "cwd": ready["cwd"], "mcpServers": [],
                      "_meta": {"jcode.interactionController": True}}
            assert client.request(2, "session/load", params)["_meta"]["jcode.interactionController"] is True
            marker = Path(ready["sideEffect"])
            original_ids = []
            for number, prompt, method in [(10, "HARNESSDESK_QUESTION", "elicitation/create"),
                                           (20, "HARNESSDESK_PERMISSION", "session/request_permission")]:
                client.send({"jsonrpc": "2.0", "id": number, "method": "session/prompt",
                             "params": {"sessionId": ready["sessionId"], "prompt": [{"type": "text", "text": prompt}]}})
                callback = client.wait(lambda item: item.get("method") == method)
                assert callback["params"]["sessionId"] == ready["sessionId"]
                original_ids.append(callback["id"])
                assert not marker.exists(), "held tool ran before approval"
                reloaded = client.request(number + 1, "session/load", params)
                assert reloaded["_meta"]["jcode.interactionController"] is True
                if method == "elicitation/create":
                    result = {"action": "accept", "content": {"value": "original-witness-token"}}
                    tool_id = callback["params"]["toolCallId"]
                else:
                    result = {"outcome": {"outcome": "selected", "optionId": "allow-once"}}
                    tool_id = callback["params"]["toolCall"]["toolCallId"]
                client.send({"jsonrpc": "2.0", "id": callback["id"], "result": result})
                assert client.result(number)["stopReason"] == "end_turn"
                records = [entry for line in Path(ready["originalToolResults"]).read_text().splitlines()
                           for entry in json.loads(line)]
                matching = [entry for entry in records if entry.get("tool_call_id") == tool_id]
                assert matching, (tool_id, records)
                if method == "elicitation/create":
                    assert any("original-witness-token" in str(entry) for entry in matching)
                else:
                    assert marker.read_text() == "approved", "operation was not executed exactly once"
            marker.unlink()
            client.send({"jsonrpc": "2.0", "id": 30, "method": "session/prompt",
                         "params": {"sessionId": ready["sessionId"], "prompt": [{"type": "text", "text": "HARNESSDESK_PERMISSION"}]}})
            held = client.wait(lambda item: item.get("method") == "session/request_permission")
            client.request(31, "session/cancel", {"sessionId": ready["sessionId"]})
            client.result(30)
            assert not marker.exists(), "cancelled permission performed its side effect"
            client.send({"jsonrpc": "2.0", "id": held["id"], "result": {"outcome": {"outcome": "selected", "optionId": "allow-once"}}})
            client.request(32, "session/list", {})
            assert not marker.exists(), "late approval resurrected a cancelled operation"
            client.send({"jsonrpc": "2.0", "id": 40, "method": "session/prompt",
                         "params": {"sessionId": ready["sessionId"], "prompt": [{"type": "text", "text": "HARNESSDESK_QUESTION"}]}})
            disconnected = client.wait(lambda item: item.get("method") == "elicitation/create")
            client.close()
            client = None
            time.sleep(5.3)
            client = Client(ready, env)
            client.request(1, "initialize", {"protocolVersion": 1, "clientCapabilities": {"elicitation": {"form": {}}}})
            assert client.request(2, "session/load", params)["_meta"]["jcode.interactionController"] is True
            recovered = client.wait(lambda item: item.get("method") == "elicitation/create")
            assert recovered["id"] == disconnected["id"], "disconnect replaced the original held operation"
            client.send({"jsonrpc": "2.0", "id": recovered["id"], "result": {"action": "accept", "content": {"value": "reconnected-witness-token"}}})
            deadline = time.monotonic() + 20
            while True:
                records = [entry for line in Path(ready["originalToolResults"]).read_text().splitlines()
                           for entry in json.loads(line)]
                if any(entry.get("tool_call_id") == recovered["params"]["toolCallId"] and "reconnected-witness-token" in str(entry) for entry in records):
                    break
                assert time.monotonic() < deadline, "reconnected reply did not reach its original tool"
                time.sleep(0.05)
            print(json.dumps({"sessionId": ready["sessionId"], "runtimeId": ready["runtimeId"],
                              "callbackIds": original_ids, "checks": ["real_actor", "held_question_reload",
                              "held_permission_reload", "original_tool_result", "exactly_once_marker",
                              "cancel_without_side_effect", "late_approval_rejected", "disconnect_recovers_original_question"]}))
        finally:
            if client:
                client.close()
            if fixture.poll() is None:
                fixture.terminate()
                try:
                    fixture.wait(timeout=15)
                except subprocess.TimeoutExpired:
                    fixture.kill()
                    fixture.wait()


if __name__ == "__main__":
    main()
