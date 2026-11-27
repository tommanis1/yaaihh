#!/usr/bin/env bash
# bump-check-test.sh: bump-check.sh refuses a harness or version that is not a plain name, and runs a staged
# package's install script with no home and no network; npm here is a stand-in whose package's script tries both.
set -u
root=$(cd "$(dirname "$0")/../../.." && pwd)
t=$(mktemp -d); trap 'rm -rf "$t"' EXIT; export t  # the stand-in npm names its files by it
export UNBOXED=1  # what the wire check takes for the pinned harness only: bump-check.sh boxes its candidate all the same
fails=0
no() { echo "FAIL: $*"; fails=$((fails + 1)); }

for bad in 'claude-code;id' 'Claude' '../x'; do
  bash "$root/ops/harness/bump-check.sh" "$bad" 9.9.9 "$t/out" 2>/dev/null; [ $? = 2 ] || no "harness '$bad' not refused"
done
for bad in '9.9.9/../x' '..' '9.9$(id)' "$(printf '9.9\n9')" '-9'; do
  bash "$root/ops/harness/bump-check.sh" claude-code "$bad" "$t/out" 2>/dev/null; [ $? = 2 ] || no "version '$bad' not refused"
done

# the stand-in npm: the package's install script writes into the home, calls out, and links a claude that answers
mkdir -p "$t/bin" "$t/home"
cat >"$t/bin/npm" <<'NPM'
#!/usr/bin/env bash
while [ $# -gt 0 ]; do [ "$1" = --prefix ] && d=$2; shift; done
p=$d/node_modules/@anthropic-ai/claude-code; mkdir -p "$p"
jq -n --arg t "$t" --arg v "$(basename "$d")" --arg uid "$(id -u)" '{$t, $v, $uid}' >"$p/stand-in.json"
cat >"$p/install.cjs" <<'JS'
const fs = require("fs"), path = require("path");
const { t, v, uid } = JSON.parse(fs.readFileSync(path.join(__dirname, "stand-in.json")));
const d = path.resolve(__dirname, "../../..");
try { fs.writeFileSync(process.env.HOME + "/pwned", "x"); } catch (e) {}
try { fs.writeFileSync(t + "/outside", "x"); } catch (e) {}
fs.writeFileSync(d + "/seen", ["/run/user/" + uid, "/mnt/c", "/home"].filter(p => fs.existsSync(p)).join(" "));
require("https").get("https://example.com", (r) => { r.resume(); fs.writeFileSync(d + "/net", "reached"); }).on("error", () => fs.writeFileSync(d + "/net", "none"));
const bus = require("net").connect("/run/user/" + uid + "/bus");
bus.on("connect", () => { fs.writeFileSync(d + "/bus", "reached"); bus.destroy(); }).on("error", () => fs.writeFileSync(d + "/bus", "none"));
setTimeout(() => process.exit(0), 10000).unref();
fs.mkdirSync(d + "/node_modules/.bin", { recursive: true });
const q = (s) => "'" + s.replace(/'/g, "'\\''") + "'";
fs.writeFileSync(d + "/node_modules/.bin/claude", "#!/bin/sh\n"
  + "touch " + q(t + "/version-ran-outside") + " 2>/dev/null\n"
  + "for p in /run/user/" + uid + " /mnt/c /home; do [ -e \"$p\" ] && echo seen: \"$p\"; done\n"
  + "echo " + q(v) + "\n", { mode: 0o755 });
JS
NPM
chmod +x "$t/bin/npm"
HOME=$t/home PATH=$t/bin:$PATH bash "$root/ops/harness/bump-check.sh" claude-code 9.9.9 "$t/out" >/dev/null 2>&1
[ -e "$t/home/pwned" ] && no "the install script wrote into the home"
[ -e "$t/outside" ] && no "the install script wrote outside its package's folder"
[ "$(cat "$t/out/stage/9.9.9/net" 2>/dev/null)" = none ] || no "the install script's network: $(cat "$t/out/stage/9.9.9/net" 2>/dev/null || echo 'never ran')"
grep -q '^Candidate-Staged claude-code claude-code 9.9.9$' "$t/out/lines" || no "no Candidate-Staged line with its launcher"
[ "$(cat "$t/out/stage/9.9.9/bus" 2>/dev/null)" = none ] || no "the install script reached the user bus: $(cat "$t/out/stage/9.9.9/bus" 2>/dev/null)"
[ -z "$(cat "$t/out/stage/9.9.9/seen" 2>/dev/null)" ] || no "the install script saw $(cat "$t/out/stage/9.9.9/seen")"
[ -e "$t/version-ran-outside" ] && no "the candidate's --version wrote outside its folder"
seen=$(cat "$t"/out/*.out 2>/dev/null | grep -h '^seen:' | sort -u | tr '\n' ' ')
[ -z "$seen" ] || no "the candidate's probe runs saw $seen"
ls "$t"/out/*.out >/dev/null 2>&1 || no "no probe run of the candidate was kept"
# an OUT whose name a shell would read as code: each hook's command still writes only its own moments file
odd="$t/o'x \$(touch $t/ran) y"
HOME=$t/home PATH=$t/bin:$PATH bash "$root/ops/harness/bump-check.sh" claude-code 9.9.9 "$odd" >/dev/null 2>&1
cmd=$(jq -r '[.hooks[]][0][0].hooks[0].command' "$odd/P.hooks.json" 2>/dev/null)
[ -n "$cmd" ] && sh -c "$cmd" && [ -s "$odd/P.moments" ] || no "a hook's command does not write into OUT: $cmd"
[ -e "$t/ran" ] && no "OUT's name ran as a command"
[ $fails = 0 ] && echo "bump-check-test: every case as it should be"
exit $fails
