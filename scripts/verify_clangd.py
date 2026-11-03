#!/usr/bin/env python3
"""Probe real clangd on private, unmaterialized C++ documents (no IDE mocks).

This checks the protocol/toolchain independently; native-provider tests and UI
checks remain necessary. Output is a small evidence JSON, never source files.
"""
import argparse
import json
from pathlib import Path
import queue
import shutil
import subprocess
import tempfile
import threading
import time


class Client:
    def __init__(self, process):
        self.process = process
        self.incoming = queue.Queue()
        self.sequence = 0
        threading.Thread(target=self.read, daemon=True).start()

    def read(self):
        while line := self.process.stdout.readline():
            if line.startswith(b"Content-Length:"):
                length = int(line.split(b":")[1])
                self.process.stdout.readline()
                self.incoming.put(json.loads(self.process.stdout.read(length)))

    def send(self, method, params, identity=None):
        packet = dict(jsonrpc="2.0", method=method, params=params)
        if identity is not None:
            packet["id"] = identity
        content = json.dumps(packet).encode()
        self.process.stdin.write(f"Content-Length: {len(content)}\r\n\r\n".encode() + content)
        self.process.stdin.flush()

    def request(self, method, params):
        self.sequence += 1
        self.send(method, params, self.sequence)
        deadline = time.monotonic() + 20
        while time.monotonic() < deadline:
            response = self.incoming.get(timeout=max(0.01, deadline - time.monotonic()))
            if response.get("id") == self.sequence:
                if "error" in response:
                    raise RuntimeError(response["error"])
                return response["result"]
        raise TimeoutError(method)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    compiler, clangd = shutil.which("clang++") or shutil.which("g++"), shutil.which("clangd")
    if not compiler or not clangd:
        raise RuntimeError("A real C++ compiler and clangd must be on PATH")
    with tempfile.TemporaryDirectory(prefix="altaed hints 中文 ") as directory:
        root = Path(directory).resolve()
        source = root / "private-id" / "document.cpp"
        document = {"uri": source.as_uri()}
        process = subprocess.Popen([clangd, "--header-insertion=never", "--log=error", "--completion-parse=always",
                                    "--completion-style=detailed", "--function-arg-placeholders=true", f"--query-driver={compiler}"],
                                   stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
        client = Client(process)
        try:
            initialized = client.request("initialize", {"processId": None, "rootUri": root.as_uri(), "capabilities": {
                "general": {"positionEncodings": ["utf-16"]}, "textDocument": {
                    "completion": {"completionItem": {"snippetSupport": True, "insertReplaceSupport": True,
                        "labelDetailsSupport": True, "resolveSupport": {"properties": ["documentation", "detail"]}}},
                    "signatureHelp": {"signatureInformation": {"parameterInformation": {"labelOffsetSupport": True},
                        "activeParameterSupport": True}}}}})
            client.send("initialized", {})
            client.send("workspace/didChangeConfiguration", {"settings": {"compilationDatabaseChanges": {
                str(source): {"workingDirectory": str(root), "compilationCommand": [compiler, "-std=c++20", str(source)]}}}})
            prefix = "#include <vector>\n/// Scales a count by a weight.\ndouble combine(int count, double weight) { return count * weight; }\n"
            text = prefix + "int main() { /*🙂*/ combi; }"
            client.send("textDocument/didOpen", {"textDocument": dict(document, languageId="cpp", version=1, text=text)})

            def at(text, marker):
                before = text[:text.rindex(marker) + len(marker)]
                return dict(textDocument=document, position={"line": before.count("\n"),
                    "character": len(before.rsplit("\n", 1)[-1].encode("utf-16-le")) // 2})

            def items(response):
                return response.get("items", []) if isinstance(response, dict) else response or []

            candidates = items(client.request("textDocument/completion", at(text, "combi;"[:-1])))
            function = next(item for item in candidates if "combine" in item["label"])
            can_resolve = initialized.get("capabilities", {}).get("completionProvider", {}).get("resolveProvider", False)
            resolved = client.request("completionItem/resolve", function) if can_resolve else function
            assert "Scales a count" in str(resolved.get("documentation")), resolved
            text = prefix + "int main() { /*🙂*/ combine(1, ); }"
            client.send("textDocument/didChange", {"textDocument": dict(document, version=2), "contentChanges": [{"text": text}]})
            signature = client.request("textDocument/signatureHelp", at(text, "combine(1, "))
            assert signature and signature["signatures"], signature
            active = signature["signatures"][signature.get("activeSignature", 0)]
            assert active.get("activeParameter", signature.get("activeParameter")) == 1, signature
            text = prefix + "int main() { std::vector<int> values; /*🙂*/ values.pu; }"
            client.send("textDocument/didChange", {"textDocument": dict(document, version=3), "contentChanges": [{"text": text}]})
            members = items(client.request("textDocument/completion", at(text, "values.pu")))
            push = next(item for item in members if "push_back" in item["label"])
            assert push.get("insertTextFormat") == 2 and "${" in str(push), push
            assert not push.get("additionalTextEdits"), push
            client.request("shutdown", None)
            client.send("exit", None)
            evidence = {"server": initialized.get("serverInfo"), "unmaterialized_source": not source.exists(),
                "function_completion": function, "resolve_supported": can_resolve, "documentation": resolved.get("documentation"),
                "signature_help": signature, "member_snippet": push}
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_text(json.dumps(evidence, ensure_ascii=False, indent=2) + "\n", encoding="utf-8", newline="\n")
            print("Real clangd: Unicode completion, documentation, second-parameter signature and member snippet passed.")
        finally:
            process.terminate()
            try:
                process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()


if __name__ == "__main__":
    main()
