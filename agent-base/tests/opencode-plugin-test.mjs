// opencode-plugin-test: agent-base/opencode/plugin/coord.js under node, with a stand-in claude/hook (tool-scan, N-591
// post 109). It checks the ready mark at load, the names and inputs coordd is asked about, and that a refusal (JSON
// deny, block, or exit 2) fails the call while an empty answer lets it run. Then the raw calls (3. in the plugin),
// against a stand-in provider on loopback with a made-up key: the mark is taken off and nothing else changed, each
// marked request is kept whole with no header, an unmarked one is not, and only the newest KEEP files stay.
// Run: node opencode-plugin-test.mjs
import { mkdtempSync, writeFileSync, readFileSync, chmodSync, mkdirSync, rmSync, existsSync, statSync, readdirSync } from "node:fs"
import { createServer } from "node:http"
import { join, dirname } from "node:path"
import { homedir } from "node:os"
import { fileURLToPath, pathToFileURL } from "node:url"

const here = dirname(fileURLToPath(import.meta.url))
const S = mkdtempSync(join(homedir(), "agent-coord/docket-try-saved/tool-scan/oc-plugin-test."))
let failed = 0
const check = (what, ok, more = "") => {
  console.log((ok ? "ok   " : "FAIL ") + what + (!ok && more ? ` (${more})` : ""))
  if (!ok) failed++
}
try {
  const coord = join(S, "coord"), base = join(S, "base"), log = join(S, "asked.jsonl")
  mkdirSync(coord, { mode: 0o700 })
  mkdirSync(join(base, "claude"), { recursive: true })
  // the stand-in hook: logs what it is asked; refuses Bash rm (JSON deny), Write to /guarded (block), Read of /two (exit 2)
  writeFileSync(join(base, "claude", "hook"), `#!/usr/bin/env python3
import json, sys
call = json.load(sys.stdin)
open(${JSON.stringify(log)}, "a").write(json.dumps({"event": sys.argv[1], **call}) + "\\n")
i = call.get("tool_input") or {}
if call["tool_name"] == "Bash" and "rm " in (i.get("command") or ""):
    print(json.dumps({"hookSpecificOutput": {"hookEventName": "PreToolUse", "permissionDecision": "deny", "permissionDecisionReason": "no rm here"}}))
elif call["tool_name"] in ("Write", "MultiEdit") and (i.get("file_path") or "").startswith("/guarded"):
    print(json.dumps({"decision": "block", "reason": "guarded path"}))
elif call["tool_name"] == "Read" and i.get("file_path") == "/two":
    sys.stderr.write("refused by exit 2"); sys.exit(2)
`)
  chmodSync(join(base, "claude", "hook"), 0o755)
  process.env.OPENCODE_COORD = coord
  process.env.AGENT_BASE = base
  const realFetch = globalThis.fetch
  const mod = await import(pathToFileURL(join(here, "..", "opencode", "plugin", "coord.js")).href)
  check("the default export is opencode's plugin form (id and server)", mod.default.id === "coord" && typeof mod.default.server === "function")
  const hooks = await mod.default.server({})
  check("loading writes the ready mark, mode 600", existsSync(join(coord, "ready")) && (statSync(join(coord, "ready")).mode & 0o777) === 0o600)
  const before = hooks["tool.execute.before"]
  const call = async (tool, args) => {
    try {
      await before({ tool, sessionID: "ses_1", callID: "c1" }, { args })
      return ""
    } catch (e) {
      return e.message
    }
  }
  const asked = () => (existsSync(log) ? readFileSync(log, "utf8").trim().split("\n").map((l) => JSON.parse(l)) : [])
  check("a call coordd lets through runs", (await call("bash", { command: "ls", description: "list" })) === "")
  let a = asked().at(-1)
  check("bash is asked as Bash, with its command, as pre", a.event === "pre" && a.tool_name === "Bash" && a.tool_input.command === "ls" && a.session_id === "ses_1" && a.hook_event_name === "PreToolUse", JSON.stringify(a))
  check("a JSON deny fails the call with its reason", (await call("bash", { command: "rm -rf x" })) === "no rm here")
  check("a block fails the call with its reason", (await call("write", { filePath: "/guarded/a", content: "x" })) === "guarded path")
  a = asked().at(-1)
  check("write is asked as Write with file_path", a.tool_name === "Write" && a.tool_input.file_path === "/guarded/a", JSON.stringify(a))
  check("an exit 2 fails the call with its words", (await call("read", { filePath: "/two" })) === "refused by exit 2")
  await call("edit", { filePath: "/w/a.rs", oldString: "a", newString: "b" })
  a = asked().at(-1)
  check("edit is asked as Edit with file_path, old_string, new_string", a.tool_name === "Edit" && a.tool_input.file_path === "/w/a.rs" && a.tool_input.old_string === "a" && a.tool_input.new_string === "b", JSON.stringify(a))
  const patch = "*** Begin Patch\n*** Update File: /w/ok.rs\n@@\n-a\n+b\n*** Add File: /guarded/new.rs\n+x\n*** End Patch"
  const n = asked().length
  check("a patch touching a guarded file fails", (await call("apply_patch", { patchText: patch })) === "guarded path")
  const pa = asked().slice(n)
  check("a patch is asked as MultiEdit once per file, in order", pa.length === 2 && pa.every((x) => x.tool_name === "MultiEdit") && pa[0].tool_input.file_path === "/w/ok.rs" && pa[1].tool_input.file_path === "/guarded/new.rs", JSON.stringify(pa))
  await call("webfetch", { url: "https://example.org" })
  a = asked().at(-1)
  check("another tool keeps its own name and input", a.tool_name === "webfetch" && a.tool_input.url === "https://example.org", JSON.stringify(a))

  // 3. raw calls, against a stand-in provider: a stream of two pieces on /v1/chat/completions, a 401 on /fail
  const got = []
  const provider = createServer((req, res) => {
    let body = ""
    req.on("data", (c) => (body += c))
    req.on("end", () => {
      got.push({ url: req.url, headers: req.headers, body })
      if (req.url === "/fail") {
        res.writeHead(401, { "content-type": "application/json" })
        res.end('{"error":{"message":"made-up key refused"}}')
        return
      }
      res.writeHead(200, { "content-type": "text/event-stream" })
      res.write('data: {"choices":[{"delta":{"content":"Hel"}}]}\n\n')
      setTimeout(() => res.end('data: {"choices":[{"delta":{"content":"lo"},"finish_reason":"stop"}]}\n\ndata: [DONE]\n\n'), 20)
    })
  })
  await new Promise((r) => provider.listen(0, "127.0.0.1", r))
  try {
    const url = `http://127.0.0.1:${provider.address().port}`
    const raw = join(coord, "raw")
    const calls = async (n) => {
      for (let i = 0; i < 200 && readdirSync(raw).filter((x) => x.startsWith("call-")).length < n; i++) await new Promise((r) => setTimeout(r, 10))
      return readdirSync(raw).filter((x) => x.startsWith("call-")).sort()
    }
    check("loading makes raw/, mode 700, and taps the global fetch", existsSync(raw) && (statSync(raw).mode & 0o777) === 0o700 && globalThis.fetch !== realFetch)
    const tapped = globalThis.fetch
    await mod.default.server({})
    check("a second load does not tap it again", globalThis.fetch === tapped)
    const out = { headers: { "content-type": "application/json", authorization: "Bearer sk-made-up-4711" } }
    await hooks["chat.headers"]({ sessionID: "ses_9", agent: "build", model: {}, provider: {}, message: {} }, out)
    check("chat.headers marks the request with its session", out.headers["x-coord-session"] === "ses_9")
    globalThis.fetch = realFetch
    const bare = { headers: {} }
    await hooks["chat.headers"]({ sessionID: "ses_9" }, bare)
    globalThis.fetch = tapped
    check("and does not while the fetch is not ours (the mark would reach the provider)", bare.headers["x-coord-session"] === undefined)
    const body = JSON.stringify({ model: "kimi-k3", messages: [{ role: "user", content: "fix mean" }], stream: true })
    const res = await fetch(`${url}/v1/chat/completions`, { method: "POST", headers: out.headers, body })
    const text = await res.text()
    check("the answer reaches opencode whole", res.status === 200 && text.includes('"Hel"') && text.includes("[DONE]"), text)
    const sent = got.at(-1)
    check("the mark is taken off before it goes; the rest goes as it came", sent.headers["x-coord-session"] === undefined && sent.headers.authorization === "Bearer sk-made-up-4711" && sent.body === body, JSON.stringify(sent.headers))
    let f = await calls(1)
    const kept = f.length === 1 ? readFileSync(join(raw, f[0]), "utf8") : ""
    check("one file for the request, mode 600", f.length === 1 && (statSync(join(raw, f[0])).mode & 0o777) === 0o600, f.join(" "))
    check("it holds the time, the session, the body and the answer as it came, then the end", /^=== REQUEST INFO ===\nTimestamp: \d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.\d{9}Z\nSession: ses_9\n/.test(kept) && kept.includes(`Body:\n${body}\n`) && kept.includes("Status: 200") && kept.includes('"content":"Hel"') && kept.includes("[DONE]") && kept.endsWith("=== END ===\n"), kept)
    check("no header is written, nor the key", !/sk-made-up|authorization|content-type|x-coord-session/i.test(kept), kept)
    await (await fetch(`${url}/models`, { headers: { authorization: "Bearer sk-made-up-4711" } })).text()
    check("a request with no mark goes as it came and is not kept", got.at(-1).url === "/models" && (await calls(2)).length === 1)
    const marked = { headers: {} }
    await hooks["chat.headers"]({ sessionID: "ses_9" }, marked)
    const refused = await fetch(`${url}/fail`, { method: "POST", headers: new Headers({ ...marked.headers, authorization: "Bearer sk-made-up-4711" }), body })
    check("a refusal reaches opencode as it came", refused.status === 401 && (await refused.text()).includes("made-up key refused"))
    f = await calls(2)
    const no = f.length === 2 ? readFileSync(join(raw, f[1]), "utf8") : ""
    check("and is kept with its status and its error (headers given as Headers)", no.includes("Status: 401") && no.includes('{"error":{"message":"made-up key refused"}}') && !no.includes("sk-made-up"), no)
    check("pairs keep their form without the mark", JSON.stringify(mod.unmarked({ headers: [["a", "1"], ["X-Coord-Session", "s"]] })) === JSON.stringify({ session: "s", headers: [["a", "1"]] }))
    for (let i = 0; i < 200; i++) await (await fetch(`${url}/fail`, { method: "POST", headers: marked.headers, body })).text()
    for (let i = 0; i < 200 && readdirSync(raw).includes(f[0]); i++) await new Promise((r) => setTimeout(r, 10))
    const last = readdirSync(raw).filter((x) => x.startsWith("call-"))
    check("only the newest 200 are kept: the first is gone", last.length === 200 && !last.includes(f[0]), `${last.length}`)
  } finally {
    provider.close()
    globalThis.fetch = realFetch
  }
  delete process.env.AGENT_BASE
} finally {
  rmSync(S, { recursive: true, force: true })
}
console.log(failed ? `${failed} failed` : "all passed")
process.exit(failed ? 1 : 0)
