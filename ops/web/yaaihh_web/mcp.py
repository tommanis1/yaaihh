"""MCP over stdio: one JSON-RPC 2.0 message per line in, one per line out (the 2025-06-18 transport)."""
import json, sys, traceback

PROTOCOLS = ("2025-06-18", "2025-03-26", "2024-11-05")


class ToolError(Exception):
    """A refusal the caller should read (isError: true), not a protocol error."""


class Server:
    def __init__(self, name, version, tools):
        self.name, self.version = name, version
        self.tools = {t["name"]: t for t in tools}  # name: {name, description, inputSchema, run}

    def answer(self, msg):
        """The reply to MSG, or None for a notification."""
        mid, method, params = msg.get("id"), msg.get("method"), msg.get("params") or {}
        if mid is None:
            return None
        try:
            if method == "initialize":
                want = params.get("protocolVersion")
                result = {"protocolVersion": want if want in PROTOCOLS else PROTOCOLS[0],
                          "capabilities": {"tools": {"listChanged": False}},
                          "serverInfo": {"name": self.name, "version": self.version}}
            elif method == "ping":
                result = {}
            elif method == "tools/list":
                result = {"tools": [{k: t[k] for k in ("name", "description", "inputSchema")}
                                    for t in self.tools.values()]}
            elif method == "tools/call":
                result = self.call(params.get("name"), params.get("arguments") or {})
            else:
                return {"jsonrpc": "2.0", "id": mid, "error": {"code": -32601, "message": f"no method {method}"}}
        except Exception as e:  # a bug in a tool is its caller's error text, never the server's end
            traceback.print_exc(file=sys.stderr)
            return {"jsonrpc": "2.0", "id": mid, "error": {"code": -32603, "message": str(e)[:500]}}
        return {"jsonrpc": "2.0", "id": mid, "result": result}

    def call(self, name, args):
        tool = self.tools.get(name)
        if tool is None:
            return text_result({"error": f"no tool {name}"}, True)
        try:
            check_args(tool["inputSchema"], args)
            return text_result(tool["run"](args), False)
        except ToolError as e:
            return text_result({"error": str(e)}, True)

    def serve(self, rin=sys.stdin, rout=sys.stdout):
        for line in rin:
            if not line.strip():
                continue
            try:
                msg = json.loads(line)
            except ValueError:
                out = {"jsonrpc": "2.0", "id": None, "error": {"code": -32700, "message": "not JSON"}}
            else:
                out = self.answer(msg) if isinstance(msg, dict) else None
            if out is not None:
                rout.write(json.dumps(out, ensure_ascii=False) + "\n")
                rout.flush()


def text_result(obj, is_error):
    return {"content": [{"type": "text", "text": json.dumps(obj, ensure_ascii=False, indent=1)}],
            "structuredContent": obj, "isError": is_error}


def check_args(schema, args):
    """ARGS against the tool's own flat schema: required keys, no others, each of its type, enum and bounds."""
    if not isinstance(args, dict):
        raise ToolError("arguments must be an object")
    props = schema.get("properties", {})
    for k in schema.get("required", []):
        if k not in args:
            raise ToolError(f"{k} is required")
    types = {"string": str, "integer": int, "boolean": bool, "array": list, "object": dict}
    for k, v in args.items():
        p = props.get(k)
        if p is None:
            raise ToolError(f"{k}: no such argument")
        t = types[p["type"]]
        if not isinstance(v, t) or (t is int and isinstance(v, bool)):
            raise ToolError(f"{k}: a {p['type']}")
        if "enum" in p and v not in p["enum"]:
            raise ToolError(f"{k}: one of {p['enum']}")
        if t is list and "enum" in p.get("items", {}):
            bad = [x for x in v if x not in p["items"]["enum"]]
            if bad:
                raise ToolError(f"{k}: {bad} not in {p['items']['enum']}")
        if t is int and not p.get("minimum", v) <= v <= p.get("maximum", v):
            raise ToolError(f"{k}: from {p.get('minimum')} to {p.get('maximum')}")
        if t is str and len(v) > p.get("maxLength", 1 << 16):
            raise ToolError(f"{k}: at most {p.get('maxLength', 1 << 16)} characters")
