#!/usr/bin/env python3
"""machines-sample-test.py: the devices on Home's sampler (agent-base/bin/machines-sample; the owner's N-1104 p1070,
step 1 of docs/specs/devices.md, paper-research N-1446 p2804; the fields agreed with docket-edge8, p2814).

1. one device's READ answer, parsed: the hottest sensor with its own crit, a card's numbers with "[N/A]" left out,
   one disk per mount, our services with the window's word and a failed one off, an unknown unit left out; nothing
   read gives no field; an answer cut before `==end` is no answer;
2. a whole run against a stand-in runner folder and a stand-in `ssh` that records its argv and answers from a file:
   - the engine's own device first (`here`, roles server), read on this machine;
   - the build machine asked with the runner's own ssh line (no config, the key by path, agent off, known hosts
     only, no prompt), its read naming only the runner's unit;
   - when it answers: on, its numbers, `runner`, and `build_machine` for the engine's jobs reader;
   - when it does not, or its answer is cut: on false, `why`, `last_on` from the sample replaced;
   - no `machines` file: the engine's device alone;
   - the file holds no host name, address, path or temp file.
Nothing is started but the sampler, `sh` and the stand-in; nothing reaches a network or a key.
"""
import importlib.machinery, importlib.util, json, os, re, socket, struct, subprocess, sys, tempfile

here = os.path.dirname(os.path.abspath(__file__))
sampler = os.path.join(here, "..", "bin", "machines-sample")
t = tempfile.mkdtemp()
fails = 0


def ok(name, got, want):
    global fails
    if got == want:
        print("ok  ", name)
    else:
        print(f"FAIL {name}: got {got!r}, want {want!r}")
        fails += 1


# its yaaihh.toml is the one beside the machines folder: the test's own, never his
os.environ["MACHINES_OUT"] = os.path.join(t, "machines", "machines.json")
loader = importlib.machinery.SourceFileLoader("machines_sample", sampler)
spec = importlib.util.spec_from_loader("machines_sample", loader)
ms = importlib.util.module_from_spec(spec)
loader.exec_module(ms)

TOWER = """==virt
kvm
==load
2.25 1.90 1.50 3/411 9999
==cores
24
==cpu
model name	: AMD Ryzen 9 7900X 12-Core Processor
==mem
MemTotal:       20971520 kB
MemAvailable:   15728640 kB
==temps
==gpu
==disks
Filesystem     1024-blocks      Used Available Capacity Mounted on
/dev/sda1        205375464  92417460 102463412      48% /
/dev/sda1        205375464  92417460 102463412      48% /
/dev/sdb1        514937088 104857600 410079488      21% /home
==units
build-runner.service loaded active running build-runner
==end
"""

# 1. parsing
p = ms.device(ms.sections(TOWER))
smart = json.dumps({"model_name": "Made-up SATA SSD", "smart_status": {"passed": True}, "temperature": {"current": 35}, "power_on_time": {"hours": 900},
                    "ata_smart_attributes": {"table": [{"id": 5, "value": 100, "raw": {"value": 0}}, {"id": 177, "value": 92, "raw": {"value": 40}}]}})
nvme = json.dumps({"name": "Made-up NVMe 1TB", "temp_c": 41.9, "wear_pct": 3, "critical": 0, "media_errors": 0, "power_on_hours": 1204, "odd": 1})
ok("each disk's health: NVMe's log, SATA's through smartctl", ms.health(nvme + '\n{"name": "Unprivileged NVMe", "temp_c": 38.0}\n', smart + "\nnot json\n"),
   [{"name": "Made-up NVMe 1TB", "temp_c": 41.9, "wear_pct": 3, "critical": 0, "media_errors": 0, "power_on_hours": 1204}, {"name": "Unprivileged NVMe", "temp_c": 38.0},
    {"name": "Made-up SATA SSD", "critical": 0, "temp_c": 35, "power_on_hours": 900, "media_errors": 0, "wear_pct": 8}])
ok("no smartctl run unless pinned", ms.SMART, "")
fmt = re.search(r'struct\.pack\("([^"]+)"', ms.NVME).group(1)
ok("the NVMe read is Python and its admin command 72 bytes", (compile(ms.NVME, "nvme", "exec") is not None, struct.calcsize(fmt)), (True, 72))
c = os.path.join(t, "c.toml")
with open(c, "w") as f:
    f.write('[devices]\nsmartctl = "/opt/smartctl"\nsmartctl_sha256 = "%s"\n\n[[device]]\nname = "b"\nhost = "Tower"\n' % ("a" * 64))
ok("yaaihh.toml's [devices] and [[device]]", ms.config(c), ({"smartctl": "/opt/smartctl", "smartctl_sha256": "a" * 64}, {"b": "Tower"}))
ok("no yaaihh.toml: nothing set", ms.config(os.path.join(t, "none.toml")), ({}, {}))
ok("the CPU's model", p["cpu_model"], "AMD Ryzen 9 7900X 12-Core Processor")
ok("virt, cores, load and memory", (p["virt"], p["cores"], p["load1"], p["mem_total_gb"], p["mem_used_gb"]), ("kvm", 24, 2.2, 20.0, 5.0))
ok("no sensor and no card: no field", ("temp_c" in p, "temp_crit_c" in p, "gpu" in p), (False, False, False))
ok("one disk per mount", p["disks"], [{"name": "/", "free_gb": 97.7, "total_gb": 195.9}, {"name": "/home", "free_gb": 391.1, "total_gb": 491.1}])
ok("the runner's unit", p["services"], [{"unit": "build-runner", "what": "runner", "on": True}])
ok("the hottest sensor with its own crit, and every sensor, hottest first", ms.temps("k10temp Tctl|45000|100000\nnvme Composite|52000|105000\nacpitz|0|\n"),
   {"temp_c": 52.0, "temp_crit_c": 105.0, "temps": [{"name": "nvme Composite", "c": 52.0, "crit_c": 105.0}, {"name": "k10temp Tctl", "c": 45.0, "crit_c": 100.0}]})
ok("a sensor with no crit or max: no crit", ms.temps("k10temp Tctl|45000|100000\nacpitz|61000|\n"),
   {"temp_c": 61.0, "temps": [{"name": "acpitz", "c": 61.0}, {"name": "k10temp Tctl", "c": 45.0, "crit_c": 100.0}]})
ok("a thermal zone hwmon gives too: once; a sensor with no name: `sensor`", ms.temps("acpitz temp1|27800|119000\nacpitz|27800|119000\n|30000|\n"),
   {"temp_c": 30.0, "temps": [{"name": "sensor", "c": 30.0}, {"name": "acpitz temp1", "c": 27.8, "crit_c": 119.0}]})
ok("at most eight sensors, the hottest", len(ms.temps("".join("cpu core %d|%d000|\n" % (i, 40 + i) for i in range(12)))["temps"]), 8)
ok("a line from before (no names) is no sensor", ms.temps("45000 100000\n"), {})
ok("a card's numbers", ms.gpu("NVIDIA RTX A2000, 48, 7, 512, 8192\n"), {"name": "NVIDIA RTX A2000", "temp_c": 48.0, "busy_pct": 7.0, "mem_used_gb": 0.5, "mem_total_gb": 8.0})
ok("a card's [N/A] left out", ms.gpu("NVIDIA T4, [N/A], [N/A], 0, 15360"), {"name": "NVIDIA T4", "mem_used_gb": 0.0, "mem_total_gb": 15.0})
units = """coordd-research.service  loaded active   running coordd research
cpa-core.service         loaded active   running Account proxy
● coord-relay.service    loaded failed   failed  coord-relay
cpa-usage.service        loaded inactive dead    usage
other.service            loaded active   running not ours
"""
ok("services with the window's word, failed and dead ones off, an unknown one left out", ms.services(units), [
    {"unit": "coordd-research", "what": "engine research", "on": True}, {"unit": "cpa-core", "what": "gateway", "on": True},
    {"unit": "coord-relay", "what": "relay", "on": False}, {"unit": "cpa-usage", "what": "usage reader", "on": False}])
ok("nothing read: no field", ms.device(ms.sections("==virt\n==load\n==temps\n==gpu\n==disks\n==units\n==end\n")), {})
ok("an answer cut before ==end is none", ms.sections(TOWER.replace("==end\n", "")), None)

# 2. whole runs against stand-ins
runner, fake, out = os.path.join(t, "runner"), os.path.join(t, "bin"), os.path.join(t, "machines", "machines.json")
os.makedirs(runner)
os.makedirs(fake)
with open(os.path.join(fake, "ssh"), "w") as f:
    f.write('#!/bin/sh\nfor a in "$@"; do printf "%%s\\n" "$a"; done > "%s/argv"\ncat "%s/answer"\nexit $(cat "%s/rc")\n' % (t, t, t))
os.chmod(os.path.join(fake, "ssh"), 0o755)
env = {**os.environ, "MACHINES_OUT": out, "MACHINES_RUNNER": runner, "PATH": fake + os.pathsep + os.environ.get("PATH", "")}


def run(answer, rc):
    with open(os.path.join(t, "answer"), "w") as f:
        f.write(answer)
    with open(os.path.join(t, "rc"), "w") as f:
        f.write(str(rc))
    r = subprocess.run([sys.executable, "-I", sampler], env=env, capture_output=True, text=True, timeout=60)
    ok("the sampler ends well", (r.returncode, r.stderr), (0, ""))
    with open(out) as f:
        text = f.read()
    return json.loads(text), text


with open(os.path.join(runner, "machines"), "w") as f:
    f.write("# the runner's own file\nyaaihh-build-1 t@yaaihh-build-1\n")
d, text = run(TOWER, 0)
server, tower = d["devices"]
virt = subprocess.run(["systemd-detect-virt"], capture_output=True, text=True).stdout.strip()
ok("the engine's own device first", (server["here"], server["roles"], server["on"], server["name"]), (True, ["server"], True, "WSL" if virt == "wsl" else "server"))
ok("its numbers read here", (isinstance(server.get("cores"), int), isinstance(server.get("mem_total_gb"), float), bool(server.get("disks"))), (True, True, True))
ok("its services only ours", all(s["unit"].startswith("coordd-") or s["unit"] in ms.WHAT for s in server.get("services", [])), True)
with open(os.path.join(t, "argv")) as f:
    argv = f.read().split("\n")
pairs = {(argv[i], argv[i + 1]) for i in range(len(argv) - 1)}
ok("the runner's own ssh line", {("-F", "none"), ("-i", runner + "/machine-key"), ("-o", "IdentitiesOnly=yes"), ("-o", "IdentityAgent=none"),
    ("-o", "ForwardAgent=no"), ("-o", "UserKnownHostsFile=" + runner + "/known_hosts"), ("-o", "StrictHostKeyChecking=yes"),
    ("-o", "BatchMode=yes"), ("-o", "ConnectTimeout=6")} <= pairs, True)
read_there = "\n".join(argv[argv.index("t@yaaihh-build-1") + 1:])
ok("its read names the runner's unit only", ("build-runner.service" in read_there, "coordd-" in read_there, "cpa-" in read_there), (True, False, False))
ok("the build machine answering", (tower["name"], tower["roles"], tower["on"], tower["runner"], tower["cores"], "why" in tower), ("yaaihh-build-1", ["jobs"], True, True, 24, False))
ok("build_machine for the engine's jobs reader", d["build_machine"], {"name": "yaaihh-build-1", "on": True, "asked": tower["asked"]})
asked = tower["asked"]
ok("no address or path in the file", [s for s in ("t@yaaihh-build-1", runner, t) if s in text], [])
ok("the engine's machine's own name its host", (server.get("host"), "host" in tower), (socket.gethostname() if virt in ("wsl", "none") else None, False))
ok("no temp file left", os.listdir(os.path.dirname(out)), ["machines.json"])

d, _ = run("", 255)
tower = d["devices"][1]
ok("the build machine not answering", (tower["on"], tower["why"], tower["last_on"], "cores" in tower, "runner" in tower), (False, "no answer", asked, False, False))
ok("build_machine says so", (d["build_machine"]["on"], d["build_machine"]["last_on"]), (False, asked))
d, _ = run(TOWER.replace("==end\n", ""), 0)
ok("an answer cut short is no answer, last_on kept", (d["devices"][1]["on"], d["devices"][1]["last_on"]), (False, asked))
d, _ = run(TOWER.replace("active running build-runner", "inactive dead build-runner"), 0)
ok("on, with its runner stopped", (d["devices"][1]["on"], d["devices"][1]["runner"]), (True, False))

with open(os.path.join(t, "yaaihh.toml"), "w") as f:
    f.write('[[device]]\nname = "yaaihh-build-1"\nhost = "Tower"\n\n[[device]]\nname = "odd"\n')
d, _ = run(TOWER, 0)
ok("a build VM's host as yaaihh.toml names it", d["devices"][1].get("host"), "Tower")
os.remove(os.path.join(t, "yaaihh.toml"))

os.remove(os.path.join(runner, "machines"))
d, _ = run(TOWER, 0)
ok("no machines file: the engine's device alone", ([x["name"] for x in d["devices"]] == [server["name"]], "build_machine" in d), (True, False))

subprocess.run(["rm", "-rf", t])
print("all passed" if not fails else f"{fails} failed")
sys.exit(1 if fails else 0)
