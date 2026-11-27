// relay.js: opencode's plugin for a Docket v2 agent. Each hook and event the driver names goes to the relay as
// {input, output, directory}; its answer is applied as given, and nothing is decided here.
//   {}                          go on as opencode would
//   {"deny": REASON}            the hook fails with REASON, which the model reads as the tool's error
//   {"modify": OBJECT}          OBJECT's keys are set on the hook's output
//   {"reply": once|reject}      the permission request in `input` is answered through opencode's own server
import { spawn } from "node:child_process"

const relay = process.env.DOCKET_HOOK_RELAY || ""
const HOOKS = ["chat.message", "tool.execute.before", "tool.execute.after", "experimental.session.compacting"]
const EVENTS = ["permission.asked", "session.idle", "session.error"]

function ask(name, input, output, directory) {
  return new Promise((resolve) => {
    if (!relay) return resolve({})
    const p = spawn(relay, [name], { stdio: ["pipe", "pipe", "inherit"] })
    let out = ""
    p.stdout.on("data", (d) => (out += d))
    p.on("error", () => resolve({}))
    p.on("close", () => {
      try {
        resolve(out.trim() ? JSON.parse(out) : {})
      } catch {
        resolve({})
      }
    })
    p.stdin.end(JSON.stringify({ input, output, directory }))
  })
}

export default {
  id: "docket-relay",
  server: async ({ serverUrl, directory }) => {
    const apply = (a, output, input) => {
      if (a.deny !== undefined) throw new Error(String(a.deny))
      if (a.modify && typeof a.modify === "object") Object.assign(output, a.modify)
      if (a.reply && input?.id) {
        fetch(new URL(`/permission/${input.id}/reply`, serverUrl), {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ reply: a.reply }),
        }).catch(() => {})
      }
    }
    const hooks = Object.fromEntries(HOOKS.map((h) => [h, async (input, output) => apply(await ask(h, input, output, directory), output, input)]))
    hooks.event = async ({ event }) => {
      if (EVENTS.includes(event?.type)) apply(await ask(event.type, event.properties || {}, {}, directory), {}, event.properties)
    }
    return hooks
  },
}
