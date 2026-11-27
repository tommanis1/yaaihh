# ops/voice

Speech for yaaihh heard on the tower's RTX 3060 instead of the laptop's processor. yaaihh streams a take's sound over
Tailscale to `worker.py`, which hears it with faster-whisper `large-v3-turbo` (INT8 weights, FP16 compute) cut by
Silero's voice detector, told the words to expect from `vocab.md`. NVIDIA Parakeet TDT 0.6B v3 is the other choice,
one line of the config away. When the tower is not reached, yaaihh hears on the laptop as before.

| file | what |
|---|---|
| `worker.py` | the listener and the wire (its head says the frames); one take per connection |
| `asr.py` | the recognisers (`whisper`, `parakeet`) and the voice detector, loaded on the GPU once |
| `prompt.py` | the vocab files' terms made into Whisper's `initial_prompt` |
| `vocab.md` | the global words: yaaihh, tovrin, Docket, eFLINT, agda2hs, bwrap, Neon, … |
| `config.toml` | the worker's settings: address, who may connect, recogniser, detector, vocab files |
| `install.sh` | a venv with `requirements.txt` (and NeMo with `--parakeet`), the config, the user unit |
| `yaaihh-voice.service` | the systemd user unit |
| `test_worker.py` | the worker's tests, with a stand-in recogniser and detector: `python3 -m unittest -v test_worker` |

## Who can reach it

It listens only on its machine's Tailscale address (`host = "tailscale"`; any address off the tailnet is refused at
start), and answers only a peer whose Tailscale address or stable node id (`tailscale whois`) is in `allow`; a node's
name is never enough, since whoever manages the tailnet can change it. Anyone else's connection is closed
unanswered. Addresses are compared in one written form, and Tailscale is asked who a peer is only when `allow` names
a node id: one question at a time, each answer kept a minute. Tailscale's WireGuard already encrypts and names both
ends, so the worker needs no key of its own, and yaaihh carries no TLS library for it.

A machine on the VM's own network can still reach the Tailscale address directly, from an address it gives itself in
Tailscale's range. Against that, `token` in the config (and the same `token` under `[tower]` in yaaihh's
speech.toml) makes every take give it. Empty, as shipped, means no token is asked.

It answers at most `max_connections` takes at once (one more is told it is busy), so the GPU's queue is at most that
long. A take that sends nothing for `read_timeout_s` is closed, and so is a reply that cannot go out in that time.
Each look at a take's sound answers a line, `{"heard": SAMPLE}` when nothing else changed, so yaaihh can tell from
silence that the tower is gone.

## The words it expects

Each take reads, fresh, the project's own vocab file (`[vocab.projects]`, by the project's id as `projects.toml` has
it) and then the global one. From each list item it takes the bold words, else the text before a colon, so the
project's `vocabulary.md` serves as it stands. The project's terms come first; together they are cut at 500
characters, and the end of what the take has settled so far follows them, so Whisper keeps the sentence going.
Parakeet takes no prompt, so with it the vocab does nothing.

## How a take goes

The detector looks at the open sound after each second of it: a pause of `min_silence_ms` settles the words before
it, and the open words are heard again as the tail. Sound with no speech is dropped unheard (Whisper makes words of
silence). Each settled piece says up to which sample it heard. yaaihh keeps every sample past that one, so a dropped
connection loses no word: it reconnects and sends them again from there. When the tower is away longer than
`fallback_ms`, or is not reached at the press, yaaihh's own engine gets the kept sound and the rest of the take.

## What it loads

Each model is pinned to one commit of its Hugging Face repository (`repo`, `revision` in `config.toml`), as
`requirements.txt` pins the packages it names. It pins those three alone, with no hashes: what they pull in is
whatever pip resolves on the day it installs, and install.sh uses the venv's own pip as it comes. A revision that is not a full commit hash is refused, and the files fetched are
checked to be that commit's before the model loads, so every start loads the same files.

## Where it runs, and who installs it

It goes on a Linux VM of its own on the tower, with the RTX 3060 passed through from Proxmox and reached over
Tailscale (the owner, Q-1907). He makes that VM and admits it; until then neither Proxmox nor the tower is touched.
His Proxmox host and his VMs are not touched by agents, and the build VM is reached only with the runner's key, for
builds. So `install.sh` is run only on his word, by him or by whoever he names, on that VM, and never by an agent
through the runner's key. It runs there now (N-1898); a change here reaches the VM only when he
installs it again. Nothing in it names a machine: the
address is the VM's own Tailscale one, the paths come from where the checkout is, `YAAIHH_VOICE_VENV` and
`YAAIHH_VOICE_CONF`.

## Setting it up

On the GPU VM:

    ops/voice/install.sh                # or --parakeet
    $EDITOR ~/.config/yaaihh-voice/config.toml   # allow = the laptop's Tailscale address; the projects' vocab files
    systemctl --user enable --now yaaihh-voice

On the laptop, `%APPDATA%\yaaihh\speech.toml`:

    [tower]
    at = "tower-server:8765"
    fallback_ms = 1500
