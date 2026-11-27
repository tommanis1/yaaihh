#!/usr/bin/env python3
"""A stand-in for a model API on 127.0.0.1, for the harness wire check: it records each request it gets, whole, and
answers a messages call with one short reply, so the harness ends its turn. It signs in nowhere and dials out to
nothing. Run inside a network namespace that has only loopback (harness-wire-check.sh does), so the harness it serves
cannot reach anything else either.

    api-standin.py LOG PORTFILE    serves on a free loopback port, writes the port to PORTFILE, and appends one JSON
                                   line per request to LOG: method, path, and the body as the harness sent it

With STANDIN_TOOL_COMMAND set, a messages call that offers tools and holds no tool result yet is answered with one
call of the Bash tool running that command, so the harness's own checks on a command can be seen: the next request
carries the tool's result, which says whether the command ran or what refused it.
"""
import json
import os
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

LOG, PORTFILE = sys.argv[1], sys.argv[2]
REPLY = "ok"
TOOL_COMMAND = os.environ.get("STANDIN_TOOL_COMMAND")


def sse(kind, data):
    return f"event: {kind}\ndata: {json.dumps(data)}\n\n".encode()


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def record(self, body):
        with open(LOG, "a") as f:
            f.write(json.dumps({"method": self.command, "path": self.path, "body": body.decode("utf-8", "replace")}) + "\n")

    def send_json(self, code, obj):
        out = json.dumps(obj).encode()
        self.send_response(code)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(out)))
        self.end_headers()
        self.wfile.write(out)

    def do_GET(self):
        self.record(b"")
        self.send_json(404, {"type": "error", "error": {"type": "not_found_error", "message": "stand-in"}})

    def do_POST(self):
        body = self.rfile.read(int(self.headers.get("content-length") or 0))
        self.record(body)
        path = self.path.split("?")[0]
        if path.endswith("/count_tokens"):
            return self.send_json(200, {"input_tokens": 1})
        if not path.endswith("/v1/messages"):
            return self.send_json(404, {"type": "error", "error": {"type": "not_found_error", "message": "stand-in"}})
        try:
            req = json.loads(body)
        except ValueError:
            req = {}
        model = req.get("model", "stand-in")
        usage = {"input_tokens": 1, "output_tokens": 1}
        has_result = any(isinstance(m.get("content"), list) and any(c.get("type") == "tool_result" for c in m["content"])
                         for m in req.get("messages") or [])
        tool_names = [t.get("name") for t in req.get("tools") or []]
        if TOOL_COMMAND and "Bash" in tool_names and not has_result:
            return self.tool_call(model, usage, bool(req.get("stream")))
        if not req.get("stream"):
            return self.send_json(200, {"id": "msg_standin", "type": "message", "role": "assistant", "model": model,
                                        "content": [{"type": "text", "text": REPLY}], "stop_reason": "end_turn",
                                        "stop_sequence": None, "usage": usage})
        self.send_response(200)
        self.send_header("content-type", "text/event-stream")
        self.end_headers()
        self.wfile.write(sse("message_start", {"type": "message_start", "message": {
            "id": "msg_standin", "type": "message", "role": "assistant", "model": model, "content": [],
            "stop_reason": None, "stop_sequence": None, "usage": usage}}))
        self.wfile.write(sse("content_block_start", {"type": "content_block_start", "index": 0,
                                                     "content_block": {"type": "text", "text": ""}}))
        self.wfile.write(sse("content_block_delta", {"type": "content_block_delta", "index": 0,
                                                     "delta": {"type": "text_delta", "text": REPLY}}))
        self.wfile.write(sse("content_block_stop", {"type": "content_block_stop", "index": 0}))
        self.wfile.write(sse("message_delta", {"type": "message_delta", "delta": {"stop_reason": "end_turn",
                                                                                   "stop_sequence": None},
                                               "usage": {"output_tokens": 1}}))
        self.wfile.write(sse("message_stop", {"type": "message_stop"}))


    def tool_call(self, model, usage, stream):
        call = {"type": "tool_use", "id": "toolu_standin", "name": "Bash",
                "input": {"command": TOOL_COMMAND, "description": "the wire check's command"}}
        if not stream:
            return self.send_json(200, {"id": "msg_standin", "type": "message", "role": "assistant", "model": model,
                                        "content": [call], "stop_reason": "tool_use", "stop_sequence": None,
                                        "usage": usage})
        self.send_response(200)
        self.send_header("content-type", "text/event-stream")
        self.end_headers()
        self.wfile.write(sse("message_start", {"type": "message_start", "message": {
            "id": "msg_standin", "type": "message", "role": "assistant", "model": model, "content": [],
            "stop_reason": None, "stop_sequence": None, "usage": usage}}))
        self.wfile.write(sse("content_block_start", {"type": "content_block_start", "index": 0,
                                                     "content_block": {**call, "input": {}}}))
        self.wfile.write(sse("content_block_delta", {"type": "content_block_delta", "index": 0, "delta": {
            "type": "input_json_delta", "partial_json": json.dumps(call["input"])}}))
        self.wfile.write(sse("content_block_stop", {"type": "content_block_stop", "index": 0}))
        self.wfile.write(sse("message_delta", {"type": "message_delta", "delta": {"stop_reason": "tool_use",
                                                                                   "stop_sequence": None},
                                               "usage": {"output_tokens": 1}}))
        self.wfile.write(sse("message_stop", {"type": "message_stop"}))


server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
with open(PORTFILE + ".tmp", "w") as f:
    f.write(str(server.server_address[1]))
os.replace(PORTFILE + ".tmp", PORTFILE)
server.serve_forever()
