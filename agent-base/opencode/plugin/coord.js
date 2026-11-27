// coord.js: an opencode agent's plugin (tool-scan, N-591 post 109; docket-core3's conditions, N-946 post 2).
//
// opencode loads it from the agent's config folder ($XDG_CONFIG_HOME/opencode/plugin/, which roster fills from
// agent-base/opencode). It does two things:
//   1. At load, it writes the ready mark into the agent's coord folder ($OPENCODE_COORD). opencode-rpc starts no
//      turn without it: with no plugin, coordd would see no tool call before it runs ("no hook, no work").
//   2. Before each tool call (`tool.execute.before`, the one hook whose failure fails the call), it asks coordd
//      through `claude/hook pre`, as a Claude Code agent's PreToolUse does, and throws when coordd refuses. The model
//      reads the thrown message as the tool's error.
//   3. It keeps each model request and its answer for the raw pane (owner m750: "I also want to see the raw that is
//      sent to the models"). opencode calls its providers itself, so no proxy logs them. `chat.headers` marks each
//      model request with the session it is for (MARK). The fetch opencode's providers call (the global one, looked
//      up at each call) is wrapped once at load, and the wrapper takes the mark off before the request goes: nothing
//      new is sent. A marked request's body and the answer as it came are written whole once the answer has ended,
//      one file per request in $OPENCODE_COORD/raw, in the sections of the model proxy's log that the raw pane reads
//      (yaaihh's proxylog crate). No header is read or written: they carry the account's key. The newest KEEP stay.
// Tool names and inputs are given as Claude Code's, so coordd's checks (guarded paths, the runner's rules, edits)
// read them unchanged: bash→Bash, read→Read, write→Write, edit→Edit (filePath→file_path), apply_patch→MultiEdit,
// once per file the patch names. Other tools keep their own name and input.
import { spawnSync } from "node:child_process"
import { writeFileSync, renameSync, mkdirSync, readdirSync, unlinkSync } from "node:fs"
import { join } from "node:path"

const coord = process.env.OPENCODE_COORD || ""
const base = process.env.AGENT_BASE || ""
// the header that marks a model request with its session, taken off before the request goes
const MARK = "x-coord-session"
// how many requests' files are kept in raw/: each repeats the conversation so far
const KEEP = 200
const TAPPED = Symbol.for("coord.raw")
let calls = 0

function patchFiles(text) {
  const files = []
  for (const line of String(text || "").split("\n")) {
    const m = /^\*\*\* (?:Add|Update|Delete) File: (.+)$/.exec(line.trim()) || /^\*\*\* Move to: (.+)$/.exec(line.trim())
    if (m) files.push(m[1].trim())
  }
  return files
}

// The calls coordd is asked about for one opencode tool call: [tool_name, tool_input] pairs.
export function asClaude(tool, args) {
  const a = args || {}
  const path = (p) => (p === undefined ? {} : { file_path: p })
  switch (tool) {
    case "bash":
      return [["Bash", { command: a.command, description: a.description, ...(a.workdir ? { cwd: a.workdir } : {}) }]]
    case "read":
      return [["Read", { ...path(a.filePath), offset: a.offset, limit: a.limit }]]
    case "write":
      return [["Write", { ...path(a.filePath), content: a.content }]]
    case "edit":
      return [["Edit", { ...path(a.filePath), old_string: a.oldString, new_string: a.newString, replace_all: a.replaceAll }]]
    case "apply_patch": {
      const files = patchFiles(a.patchText)
      // a patch that names no file is still asked about once, so coordd sees it
      return (files.length ? files : [undefined]).map((f) => ["MultiEdit", { ...path(f), patch: a.patchText }])
    }
    default:
      return [[tool, a]]
  }
}

// coordd's answer to one call: "" to go on, or the reason it refuses.
function refusal(sessionID, name, input) {
  if (!base) return "AGENT_BASE is not set, so coordd cannot be asked about this call; it is refused"
  const r = spawnSync(join(base, "claude", "hook"), ["pre"], {
    input: JSON.stringify({ hook_event_name: "PreToolUse", session_id: sessionID, tool_name: name, tool_input: input, cwd: process.cwd() }),
    encoding: "utf8",
  })
  if (r.error) return `coordd could not be asked about this call (${r.error.message}); it is refused`
  if (r.status === 2) return (r.stderr || "").trim() || "coordd refused this call"
  const out = (r.stdout || "").trim()
  if (!out) return ""
  let v
  try {
    v = JSON.parse(out)
  } catch {
    return ""
  }
  const h = v.hookSpecificOutput || {}
  if (h.permissionDecision === "deny") return h.permissionDecisionReason || "coordd refused this call"
  if (v.decision === "block") return v.reason || "coordd refused this call"
  return ""
}

// A request's time in RFC 3339 to the nanosecond: its millisecond, then a count, so no two requests of this process
// share one (the raw pane reads on from a request's time).
function stamp() {
  calls = (calls % 999999) + 1
  return new Date().toISOString().replace(/Z$/, `${String(calls).padStart(6, "0")}Z`)
}

// INIT's headers without the mark, and the session the mark named; {} for a request with no mark (not a model's).
// The headers keep their form (a Headers, pairs, or an object); none of them is read but the mark.
export function unmarked(init) {
  const h = init?.headers
  if (!h) return {}
  if (h instanceof Headers) {
    const session = h.get(MARK)
    if (session === null) return {}
    const headers = new Headers(h)
    headers.delete(MARK)
    return { session, headers }
  }
  const pairs = Array.isArray(h) ? h : Object.entries(h)
  const at = pairs.findIndex(([k]) => String(k).toLowerCase() === MARK)
  if (at < 0) return {}
  const rest = pairs.filter((_, i) => i !== at)
  return { session: String(pairs[at][1]), headers: Array.isArray(h) ? rest : Object.fromEntries(rest) }
}

// STREAM's text once it has ended, or what came before it broke off, and why.
async function said(stream) {
  if (!stream) return ""
  const words = new TextDecoder()
  let text = ""
  try {
    for await (const piece of stream) text += words.decode(piece, { stream: true })
    return text + words.decode()
  } catch (e) {
    return `${text}\n(the answer broke off: ${e?.message || e})`
  }
}

// One request's file in DIR, written whole (a hidden file renamed), then the oldest past KEEP removed. A failure to
// write loses only this request's file: the request itself is never held up by it.
function keep(dir, at, session, body, status, answer) {
  try {
    const name = `call-${at.replace(/[-:.]/g, "")}.log`
    const text = `=== REQUEST INFO ===\nTimestamp: ${at}\nSession: ${session}\n\n=== API REQUEST 1 ===\nBody:\n${body}\n\n=== API RESPONSE 1 ===\nStatus: ${status}\n\n${answer}\n\n=== END ===\n`
    writeFileSync(join(dir, `.${name}`), text, { mode: 0o600 })
    renameSync(join(dir, `.${name}`), join(dir, name))
    const all = readdirSync(dir).filter((n) => n.startsWith("call-") && n.endsWith(".log")).sort()
    for (const n of all.slice(0, Math.max(0, all.length - KEEP))) unlinkSync(join(dir, n))
  } catch {}
}

// FETCH, with a marked request's body and answer kept in DIR (3. above); any other request goes as it came.
export function tap(fetch, dir) {
  const tapped = async (input, init) => {
    const { session, headers } = unmarked(init)
    if (session === undefined) return fetch(input, init)
    const at = stamp()
    const body = typeof init.body === "string" ? init.body : ""
    let res
    try {
      res = await fetch(input, { ...init, headers })
    } catch (e) {
      keep(dir, at, session, body, "none", `(no answer: ${e?.message || e})`)
      throw e
    }
    if (!res.body) {
      keep(dir, at, session, body, res.status, "")
      return res
    }
    const [mine, theirs] = res.body.tee()
    said(mine).then((answer) => keep(dir, at, session, body, res.status, answer))
    return new Response(theirs, { status: res.status, statusText: res.statusText, headers: res.headers })
  }
  tapped[TAPPED] = true
  return tapped
}

export default {
  id: "coord",
  server: async () => {
    if (coord) {
      const mark = join(coord, "ready")
      writeFileSync(`${mark}.${process.pid}`, `${process.pid}\n`, { mode: 0o600 })
      renameSync(`${mark}.${process.pid}`, mark)
      // raw/ is there from the start, so the raw pane reads an empty list before the first request
      mkdirSync(join(coord, "raw"), { recursive: true, mode: 0o700 })
      if (!globalThis.fetch[TAPPED]) globalThis.fetch = tap(globalThis.fetch, join(coord, "raw"))
    }
    return {
      // marked only while the fetch is ours, which takes the mark off: it never reaches a provider
      "chat.headers": async (input, output) => {
        if (globalThis.fetch[TAPPED]) output.headers[MARK] = input.sessionID
      },
      "tool.execute.before": async (input, output) => {
        for (const [name, args] of asClaude(input.tool, output.args)) {
          const why = refusal(input.sessionID, name, args)
          if (why) throw new Error(why)
        }
      },
    }
  },
}
