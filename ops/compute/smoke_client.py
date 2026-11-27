"""smoke_client.py ADDRESS: one action through REAPI v2 twice (a miss that runs, then an action-cache hit), protobuf by hand."""
import hashlib, json, sys, time
import grpc

V2 = "/build.bazel.remote.execution.v2."
INSTANCE = "main"


def varint(n):  # protobuf base-128 integer
    out = bytearray()
    while True:
        b, n = n & 0x7F, n >> 7
        out.append(b | (0x80 if n else 0))
        if not n:
            return bytes(out)


def field(num, value):  # one field: bytes/str/message as length-delimited, int/bool as varint
    if isinstance(value, (bool, int)):
        return varint(num << 3) + varint(int(value))
    if isinstance(value, str):
        value = value.encode()
    return varint(num << 3 | 2) + varint(len(value)) + value


def fields(data):  # {num: [values]} of a message: ints for varints, bytes for length-delimited
    out, i = {}, 0
    while i < len(data):
        key, shift = 0, 0
        while True:
            b = data[i]; i += 1; key |= (b & 0x7F) << shift; shift += 7
            if b < 0x80:
                break
        num, kind = key >> 3, key & 7
        if kind == 0:
            v, shift = 0, 0
            while True:
                b = data[i]; i += 1; v |= (b & 0x7F) << shift; shift += 7
                if b < 0x80:
                    break
        elif kind == 2:
            n, shift = 0, 0
            while True:
                b = data[i]; i += 1; n |= (b & 0x7F) << shift; shift += 7
                if b < 0x80:
                    break
            v, i = data[i:i + n], i + n
        else:
            raise ValueError(f"wire type {kind} not read here")
        out.setdefault(num, []).append(v)
    return out


def digest(blob):
    return hashlib.sha256(blob).hexdigest(), len(blob)


def digest_msg(d):
    return field(1, d[0]) + field(2, d[1])


def call(ch, method, req, stream=False):
    f = (ch.unary_stream if stream else ch.unary_unary)(method, request_serializer=None, response_deserializer=None)
    return f(req, timeout=120)


def run(ch, args, props):
    platform = b"".join(field(1, field(1, k) + field(2, v)) for k, v in sorted(props.items()))
    command = b"".join(field(1, a) for a in args) + field(5, platform)
    empty_dir = b""
    action = field(1, digest_msg(digest(command))) + field(2, digest_msg(digest(empty_dir))) + field(10, platform)
    blobs = b"".join(field(2, field(1, digest_msg(digest(b))) + field(2, b)) for b in (command, empty_dir, action))
    call(ch, V2 + "ContentAddressableStorage/BatchUpdateBlobs", field(1, INSTANCE) + blobs)
    t = time.monotonic()
    op = None
    for msg in call(ch, V2 + "Execution/Execute", field(1, INSTANCE) + field(6, digest_msg(digest(action))), stream=True):
        op = fields(msg)
    ms = round((time.monotonic() - t) * 1000)
    if not op or not op.get(3, [0])[0]:
        raise SystemExit("smoke: Execute ended without a finished operation")
    if 4 in op:
        raise SystemExit(f"smoke: Execute failed: {fields(op[4][0])}")
    resp = fields(fields(op[5][0])[2][0])  # Operation.response (Any) -> ExecuteResponse
    result = fields(resp[1][0])
    stdout = result.get(5, [b""])[0]
    if not stdout and 6 in result:
        d = fields(result[6][0])
        got = fields(call(ch, V2 + "ContentAddressableStorage/BatchReadBlobs", field(1, INSTANCE) + field(2, digest_msg((d[1][0].decode(), d.get(2, [0])[0])))))
        stdout = fields(got[1][0]).get(2, [b""])[0]
    cached = bool(resp.get(2, [0])[0])
    try:
        call(ch, V2 + "ActionCache/GetActionResult", field(1, INSTANCE) + field(2, digest_msg(digest(action))))
        in_ac = True
    except grpc.RpcError as e:
        if e.code() != grpc.StatusCode.NOT_FOUND:
            raise
        in_ac = False
    return {"exit": result.get(4, [0])[0], "cached": cached, "in_action_cache": in_ac, "ms": ms, "stdout": stdout.decode().strip()}


def main():
    ch = grpc.insecure_channel(sys.argv[1])
    grpc.channel_ready_future(ch).result(timeout=30)
    props = {"sandbox": sys.argv[2] if len(sys.argv) > 2 else "bwrap"}
    # the network check reads the action's own /proc: only `lo` when the sandbox gave it no network
    args = ["/bin/sh", "-c", "echo uid=$(id -u) host=$(uname -n) links=$(tail -n +3 /proc/net/dev | cut -d: -f1 | tr -d ' ' | tr '\\n' ,)"]
    first, second = run(ch, args, props), run(ch, args, props)
    print(json.dumps({"first": first, "second": second}))
    ok = first["exit"] == 0 and not first["cached"] and second["cached"] and first["in_action_cache"] is True and second["stdout"] == first["stdout"]
    ok = ok and first["stdout"].endswith("links=lo,")
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
