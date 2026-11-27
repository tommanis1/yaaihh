# Devices: one server per project, his window as its client, and machines that only take jobs (24 Nov 2026)

docket-edge7. This is the page sent to paper-research as N-1446 p2768, which was shown to the owner in a shorter form as N-1104 p1093. Section 12 is N-1446 p2773, which paper-research took as the standard, and its four lines are in AGENTS.md. His answers came on 24 Nov (N-1104 p1096 to p1111). Section 10 lists them, and each is also written where it changes the page. Steps 1 and 2 are built and handed in (heads H-1594 and H-1595). Nothing has been created or changed on his machines: the server machine and the build machine's size wait for the walk-through (section 9).

**In one line:** half of the split already exists. His window has always reached its engine through one pipe, and today that pipe goes into WSL. The same pipe can go over an encrypted link to the tower instead. Then, for each project, the laptop either runs both or only the window, with the server on the tower.

## 1. His words
- N-1104 p1070: "add the devices themselves … separate server from clients. so my lat can run both a server and a client or only run the server on the desktop … maybe encrypted p2p for sync … look for off the shelf too for … the temps cpu etc or … do it ourself for the fundamentals".
- N-1104 p1079: "use eg github account managed in yaaihh to hold a record so we can do eg handoff resume join and leave network eg we want the phone as a client only but only when the app is actually open".
- On the tower (N-989):
  - "3-5 of these here maybe 1-3 elsewhere" (p23);
  - "some machine in my house i want local only accessible" (p24);
  - "yaaihh could generalize over whatever … or … use something existing" (p28);
  - "personal and encrypted, never read by any machine hosted as a service" (p29);
  - other yaaihh users get "this setup out of the box" (p35);
  - "where can i see the … dashboard for resources … my 2 machines … and the runner" (p48).

## 2. What a device is
A device is one machine with a name. Home shows each one with its numbers, and each does one or more of three things:
- **Server of a project.** It holds that project's record and is the only machine that writes it. The engine sits there, with the agents, the repositories they work in, and the gateway holding the accounts he signed in on that machine. Each project has exactly one server.
- **Machine for jobs.** Any machine that joins on these terms (his p1102 on Colab: "just add the infra"):
  - **How it joins:** he admits it to his network by his own key exchange, and the runner names it, with a key of its own.
  - **What it is sent:** a job's code and the built files it needs, found by their content (the store). It never gets a login or the record.
  - **What it keeps:** nothing that exists only there. It can go away at any moment, which costs only the job it was running.
  - The build machine on the tower is the first, and its rule stays: no login and no record there, only code and build output. Colab is the second (below).
- **Client.** This is his window. There can be any number of them. Each holds only its own things: its place on the screen, his unsent text, and the window's own file.

Today:
| device | what runs there | what it is |
|---|---|---|
| Laptop, Windows side | his window | client |
| Laptop, WSL side | the engines of research and cav-paper, 14 live agents, the gateway, the accounts and usage services, the runner's sending side, the builds' folders | server of both projects |
| Tower, the build machine | jobs, and the store's compiled code (60 GB) | machine for jobs |
| Tower, its own system (Proxmox) | his machines 100, 101, 105, 106 and his Main pool | his. Shown on Home, never used: we only start and stop the build machine |

Later:
- a server machine on the tower (section 4);
- more machines, his 3–5 at home and 1–3 elsewhere, one of them maybe local only;
- a phone as a client, connected only while its app is open (section 5);
- Colab as borrowed compute, the second machine for jobs on the terms above. It is one only while a notebook is open under his Google sign-in, which we never read or copy. It is not limited to one kind of job (p2807). Nothing of it is built before steps 1 and 2 stand.

## 3. What the code does already, and the one thing that changes
- The window never reads the server's files itself. Every page asks the engine: files, folders, edits, the language helper and the raw pane. That has been true since his ask 13, because to the window on Windows the server was always another machine (WSL).
- The window reaches each project's engine by running one command on the server's machine and talking to it over that command's input and output. Today `wsl.exe` comes first in that command. The accounts service is reached the same way, one line in and one line out.
- When the pipe drops, the window starts it again by itself: after 3 s at first, doubling up to once a minute. The new pipe brings only the events he missed, not everything again. A message that could not go stands as "not sent" with Send again, and goes by itself once the pipe is back. His words are saved when he presses Send.
- So set-up B needs one new kind of "server's machine": an encrypted link to another machine in place of `wsl.exe`. That link is SSH over his Tailscale network, which the runner already uses to reach the tower. Everything above the link stays as it is.
- Only one thing assumes the server and the window are on the same machine: at a new build, the window's file is copied straight into C:\yaaihh from WSL. With the server elsewhere, the window fetches the new file over its link, checks its hash, and swaps itself as it does today.

## 4. His two set-ups
- **A. The laptop runs both.** This is today, and nothing changes.
- **B. The server on the tower, and the laptop runs only his window.**
  - The agents keep working while the laptop sleeps, travels or restarts. His window, opened anywhere with Tailscale up, picks up where it was.
  - The laptop's load drops to the window's own. Today the server part is most of it: research's engine and agents use 12.2 GB of memory, plus 8.7 GB of cache and 34.6 GB at their peak, and the gateway uses 1.6 GB.
  - Replays and checks run next to the builds and read the record where it is written. That is a2h-speed's section 6 without sending a copy across.
  - **It needs memory in the tower.** The tower has had 24 GB since the bad stick came out, and the build machine takes 20 GB of that. A server machine beside it needs about 16 GB, and more at peaks. So either he buys a stick ("i can buy hardware", N-989 p25), or the build machine gets smaller.
  - The server goes in a machine of its own, not in the build machine, so that jobs built from any change never sit next to his sign-ins and the record.
- **Per project.** Each project in his window already has its own pipe. So research can have its server on the tower while a small project stays on the laptop.
- **Moving a project's server to another device is a step he presses, never automatic.** The old server stops, the record's last lines go across, and the server starts on the new device. Two servers never write one record, so nothing ever has to be merged and the record can never split in two.

## 5. What travels between devices, and how

**Candidate 1, his p1079: GitHub holds each project's record**, in a private repository on his account. The server sends the record's new lines there. A device taking over picks the record up from there, so handing off, resuming, joining and leaving need no link between the machines.
- **What it gives:**
  - Handoff and resume with the old machine off or away. GitHub is always on, and the machines never have to be on at the same time.
  - History for free. The record only grows, so each send adds only its new lines.
  - A copy kept off his machines.
- **Size.** GitHub refuses any file over 100 MB (and warns above 50 MB), so today's 230 MB record cannot go as one file. It goes in pieces, and a piece never changes once it is closed.
  - Today the record grows by about 19 MB a day: 230 MB in its 12 days since 12 Nov. At the size it should be, only the lines from outside (docket-edge5's 41.6 MB, p2749), it grows by about 3.5 MB a day.
  - A year at today's rate is about 7 GB. GitHub's own advice is a repository "ideally less than 1 GB, and less than 5 GB is strongly recommended". At the right size a year is about 1.3 GB. So this candidate goes together with the record holding only what came from outside.
- **How often it sends.** Every 5 minutes, and at once at a handoff. That is 288 sends a day, about 70 KB each today, or 12 KB at the right size.
- **Who may write when two servers are up.** GitHub refuses a send that does not build on the latest one, and the repository also names which device is the server. Taking over means sending a change of that name on top of the latest version. The old server's next send is then refused, so it stops being the server and keeps its unsent lines aside, shown to him and never merged quietly.
  - The cost is what a server writes while it cannot reach GitHub and does not yet know it was replaced: at most the lines since its last send. With the Tailscale link up as well, a handoff is clean: the old server stops, sends its last lines, and only then does the new one start.
- **What a phone reads:** nothing from GitHub.
  - A phone cannot run the engine, and today's record is 230 MB. It is a client like the window: while its app is open, it reaches the server over Tailscale with a key of its own and reads what the window reads. When the app is closed it is connected to nothing and runs nothing.
  - A phone window is a new piece of work of its own. It is not started.
- **What his rules require:**
  - Private repositories on his account only, and nothing sent before his word.
  - The GitHub key is never in a box and never in an agent's hands. Each server device gets a key for that one repository only, made on that device and added by him on GitHub's own page. Only the engine's sending step uses it, and it runs as a user of its own that no agent is. His GitHub sign-in itself is never used. Today research's agents run as his own user on the laptop, so a key there would be within their reach: that separate user comes first.
  - Even a private repository is readable by GitHub. The record holds his words, the agents' words, and pieces of files they quoted. To keep GitHub blind, as his N-989 p29 asks for personal data, each piece is encrypted on the server before it leaves, with a key that only his devices hold. Encrypted pieces do not shrink against each other, which costs little because each piece is sent once.
- "Managed in yaaihh like his other accounts": the Accounts page gets a GitHub card per repository, with its last send, its size and which device is the server. Signing in is his own step, on GitHub's page.

**Candidate 2, encrypted device to device, his "maybe encrypted p2p".** That is what the link already is. Tailscale makes one WireGuard tunnel from one device to another, direct where it can. Where it cannot, Tailscale's relays pass on packets they cannot read. SSH adds a second layer of encryption inside the tunnel.
- What it gives: no company holds any of the data, and the handoff is fast.
- What it does not give: both machines must be on at the same time.

**My proposal for his word: both.** The window's live link goes device to device. GitHub, with encrypted pieces, is the record's always-on meeting point and its copy off his machines.

**His answer (p1098): yes, as a general option, "extra place for syncing".** A project can name one or more extra places its record is sent to. To the engine, every such place is the same thing, a place that:
- **takes** closed, encrypted pieces of the record, added in order;
- **refuses** a send that does not build on the last piece it has, and that refusal is what keeps one writer;
- **gives back** every piece it holds, in order, to a device taking over.

A private GitHub repository is the first such place. Another git host, a folder on one of his machines, or object storage would be others. His rules hold for each place, as written above for GitHub. Step 4 builds the contract and the GitHub place, and no second place is built until he names one.

Everything that travels:
| what | from, to | how |
|---|---|---|
| code | servers and machines for jobs | git, as now. Code goes to the tower by its content (a2h-speed's step B) |
| build output | the tower to the server | the store, by content, each file fetched once (a2h-speed's step A) |
| the record | server to GitHub, and to whichever device takes over | new lines only (candidate 1). One writer |
| the window's file at a new build | server to each client | over the window's link, checked by its hash |
| the window's own state | stays on each client | nothing travels |
| his sign-ins | only between two servers he has admitted, and only inside his Disconnect or his Move (p1106, p1109) | the account's folder goes sealed and unopened, moved by the engine's own step over the device link, and it is live on one server at a time. By default an account stays on the machine it is on. No agent reads, prints or copies it. Otherwise he signs in on each server's gateway himself, as now |
| his personal files | not part of this | his rule from 19 Nov: encrypted, and no agent reads them. A backup tool comes later, on his word |

Each thing has one writer, so nothing needs a two-way sync tool.

## 6. The numbers: load, memory, temperature
**We read the basics ourselves.** One read-only command goes over the same link while Home is open. While Home is closed, nothing runs at all.
- It reads the Linux kernel's own files (load, cores, memory, free disk), the temperature sensors and the graphics card's own tool. It also reads whether our services are running, and the runner's queue.
- No program stays running, nothing listens, and nothing is installed.
- here-2 wrote this reader for the Machines page on 20 Nov. It was never merged, and this work extends it.

What we cannot read ourselves:
- **The laptop's temperature.** WSL sees no sensor: I checked now, and it finds only the battery and the charger. Windows' own counters give load and memory but no temperature. LibreHardwareMonitor (free, open source) reads it, but it needs an administrator's driver.
- **The tower's temperature.** Only the tower's own system (Proxmox) sees it, because a machine inside it sees no sensor. Reading it means one read-only command on his system, where today we only start and stop the build machine.
- **His answer (p1100): either way, and more hardware numbers later.** Both are later pieces:
  - The tower's read needs a way in to his system, which waits for the walk-through. The exact command is named in the piece's post before it first runs there, and nothing else is done there.
  - The laptop's needs an administrator step on his Windows, which he is asked for when that piece exists.
- Step 1's reader (H-1594) leaves out every number it does not read, so these show as "—" until then.

## 7. Off the shelf: what each would and would not do for us
| part | candidate | would | would not |
|---|---|---|---|
| link | **Tailscale** (in use since 20 Nov) | an encrypted network between his devices that works away from home, one key per device; he is already signed in | its coordination service is a company's: it sees device names, addresses and when they are on, never what travels |
| link | Headscale | the same, with that coordination run on the tower instead | needs a machine that is always on, and someone to look after it |
| link | plain WireGuard | no company in it at all | no way through home routers: away from home, the laptop reaches the tower only through a port opened to the internet |
| the pipe | **OpenSSH** (already the runner's) | encrypts, one key per client, and the server can let a key run only the window's own commands | nothing alone: it rides on the network above |
| record meeting point | **GitHub**, private repository (candidate 1) | always on, history, a copy off his machines | files over 100 MB; reads anything not encrypted first; needs its own key handled as section 5 says |
| sync | **git** | code, as now | — |
| sync | rsync | the record's new lines between his own machines, one way, picking up where a drop left off | two-way |
| sync | Syncthing | his own folders kept the same on several devices, device to device, encrypted | the record or a working repository (two writers make conflict copies and can break a repository); it runs on every device and listens |
| backup | restic | encrypted backups that only his key can read, on the tower's empty 4 TB disk or elsewhere | live sync |
| numbers | Netdata | everything, every second, with alerts | a program and a web page listening on every machine, a dashboard of its own beside his window, and a hosted part |
| numbers | Prometheus + Grafana | history and graphs | a listener on every machine, a server to keep the numbers, another window |
| numbers | Glances | load, memory and temperatures in one call | little beyond the kernel's own files; its server mode listens |
| numbers | Proxmox's own status | the tower and all its machines in one read | it is his system (question 6) |
| laptop temperature | LibreHardwareMonitor | the CPU and graphics temperature on Windows | runs as administrator with a driver (question 7) |

My choice, for his word:
- links: Tailscale and OpenSSH, both already in place;
- the record: GitHub with encrypted pieces, together with the record holding only what came from outside;
- code and build output: git and the store;
- the numbers: our own reader;
- backups: restic, when he wants them;
- Syncthing only if he wants his own folders on several devices.

## 8. What he would see on Home
The laptop's numbers are real, read at 17:20. The tower's and the phone's are examples.
```
Devices                    load          memory          temp    runs here                                   store held here
  Laptop · Windows         —             —               —       your window                                 —
    disk C:  25 GB free of 953 ●
  Laptop · WSL             3.6 of 16     18.8 of 39 GB   —       server of research (14 agents), cav-paper    e.g. 6.2 GB of built files
                                                                 gateway · accounts · usage
  Tower · build machine    e.g. 9 of 20  e.g. 11 of 20 GB —      2 jobs running, 1 waiting                   compiled code 60.2 GB of 60
  Tower · own system       e.g. 1 of 24  e.g. 22 of 24 GB 58 °C  the build machine · 4 machines of yours     —
    graphics RTX 3060                    e.g. 0.3 of 12 GB 41 °C idle
  Phone                    app closed                            —                                           —
  Colab                    not open                              —                                           nothing kept
Records                    research: last sent to GitHub 3 min ago · server: Laptop · WSL
```
- A red mark appears only when a device needs him: a disk nearly full (his C: today), a machine too hot, a machine not answering.
- Pressing a device opens it. It shows its link (direct or through a relay, and when it last answered), what runs there one by one, its disks, and for a server its projects.
- In set-up B, the WSL row reads "nothing of yours runs here", and a row "Tower · server" carries research.
- **Disconnect and Move (his p1109)** are behind the press on a device, never on the list itself:
```
  Laptop · WSL                                                   link: this PC
    runs here    engine research · engine cav-paper · gateway · usage reader · runner · relay
    disks        / 229 GB free of 688
    projects     research (14 agents) · cav-paper
    accounts     e.g. 5 signed in on its gateway, staying on this machine
                                                       [ Move a project… ]   [ Disconnect ]
```
  - **Disconnect** is the one press that ends a device's part. What the device serves then either stops, or moves to another server that can take it, which he chooses in the press's dialog.
  - **Move** sends one project's server to another device (section 4).
  - Accounts stay on their machine by default. They travel only inside his Disconnect or his Move, and only if he says so there. Nothing moves on its own.
  - An account that moves takes its profile with it: the profile is made once per account and then carried, never made again on the new machine (his p1115).

## 9. The walk-through he asked for, set-up B, before any step on his machines
- **Which machine.** The server is a new machine of its own on the tower: a second machine under his Proxmox, beside the build machine (VM 200), with the name he gives it (we propose `yaaihh-server`). His trust is for that one machine he named (p2828).
  - Its memory comes from the build machine for now (his p1096: "memory wil locme for nwo smaller build machine i guess"). a2h-speed measured it (N-1446 p2849): with the runner as it is (up to four jobs at once), the build machine cannot go below its 20 GB. With at most two jobs at once, 13 GB holds the two largest jobs measured, which frees 7 GB for the server machine. The cost is that a build's checks run two jobs at a time instead of four.
  - It holds the record, the engine, the agents, and the gateway with his sign-ins (p1106). The gateway goes there too for now (p1111), with a management key of its own made there first.
- **What connects.** His window dials out over Tailscale to the tower's server machine, and SSH answers there. Nothing on the laptop listens. The server machine answers only on its Tailscale address, so nothing on his home network or on the internet reaches it. The server dials out to GitHub to send the record. Nothing calls in.
- **The keys.**
  - One key per client device, made on that device, and it never leaves it. The server lets that key run only the window's own commands: the pipe to the engine, starting a project, and the accounts service. To shut a device out, he deletes one line on the server. A lost laptop costs that one line, not a password change.
  - **What makes a machine trusted is his own key exchange (p1106).** He admits it to his Tailscale network by signing in there himself, and he adds his window's key on it himself. Nothing joins his network or takes a key any other way.
  - Tailscale's device keys stay with his Tailscale account, as now.
  - **On the server machine, the sign-ins' folders belong to a Linux user that no agent runs as** (p2828). The agents run as users of their own and cannot open those folders.
  - The GitHub key is one per server and one repository each (section 5).
- **What travels.**
  - To the window: what it shows and sends, meaning messages, the agents' words, the terminals he opens, the files he opens, and the accounts' state (never a login). At a new build, the window's new file. All of it goes inside WireGuard and SSH.
  - To GitHub: the record's new lines, encrypted before they leave.
- **When the link drops.**
  - The agents go on, and nothing on the server notices.
  - The window keeps what it shows, marks the project as not reached, and tries again by itself, after 3 s at first and then up to once a minute. When the link is back, only what he missed comes.
  - His text stays as "not sent" and goes by itself once the server answers.
  - If the tower itself goes off, the agents stop with it, as they do today when the laptop sleeps. GitHub then has the record up to its last send, at most 5 minutes old, and he can move the project to the laptop with one press.

## 10. His questions
His answers are on N-1104. Where quoted, his words are as he wrote them; the rest is paper-research's reading of them (N-1446 p2804 to p2836). paper-research put seven of these to him, so his numbering differs from this list's.
1. **Which machine is the server, per project?** Is research on the tower (set-up B)? If so, the tower needs memory: a new stick, or a smaller build machine.
   - **p1096:** "memory wil locme for nwo smaller build machine i guess". Research's server goes on the tower, and the build machine gives up the memory for now (section 9).
2. **What may listen where?** I propose that only the server machine's SSH listens, and only on its Tailscale address, and that nothing on the laptop listens.
   - **p1097:** "yes".
3. **Keys.** One per client device, made there and never leaving it, and allowed only the window's commands. Tailscale's company coordination as now, or Headscale on the tower later?
   - Not put to him as a question of its own. His p1106 answers its core: a machine he admitted by his own key exchange is trusted (section 9). Headscale was not asked.
4. **GitHub for the records (his p1079).** One private repository per project, encrypted pieces, a key per server for that one repository, and nothing sent before his word?
   - **p1098:** "yes but is genral option ofr extra place for syncyng to work well no ?". GitHub is the first of any number of extra places (section 5).
5. **Agents and his sign-ins on the tower.** Today's rule for the tower is no agent, no login, only builds. Set-up B changes that for the server machine alone, where he would sign in on its gateway himself.
   - **p1106:** "we can share the login we turst hte machien int he ntework rihgt becise we need ot manual key exhange / auth them somewhow no ?". Agents and his sign-ins go on a server machine he admitted.
   - **p1109** ("exactly", to a sealed sign-in live on one server at a time; "sticky the acounts to machine by defult"): a sign-in may travel, sealed and live on one server at a time. Disconnect is the press that ends a device's part, and accounts stick to their machine by default (sections 5 and 8).
   - **p1111:** "mayeb just he gatewya too for nwo". The gateway goes to the server as well, with its own management key first (section 9).
   - **p1115:** "with eh 0.4 we want ot move to all cladue accoutn off the gateway … negngeat it maybe psosornadmly once then we jsut carry tit arounf". An account's profile is made once and travels with the account when it moves (section 8). In 0.4 every Claude account comes off the gateway and runs on Claude Code's own sign-in; the gateway stays for the others, Codex and the like.
6. **Reading the tower's own system** (read-only) for its temperature and load?
   - **p1100:** "either ofc and we will need i guess other metrics to about idk the hardware too wbut we will do this later" (section 6).
7. **The laptop's temperature.** LibreHardwareMonitor running as administrator, or no temperature for the laptop?
   - The same p1100: later, with the administrator step asked for when that piece exists.
8. **Colab.** Through his Antigravity account, as his p1070 says, and only for jobs that need a graphics card, with nothing kept there?
   - **p1102:** "yeah no just add htei nfra for doing it okay". A general way for any borrowed machine to join as a machine for jobs, with Colab its second instance (section 2).
9. **Local only** (N-989 p24). Which machine, if any, stays off Tailscale?
   - **p1104:** "for now none". Every machine of his joins Tailscale for now.

## 11. The way, once he says yes, smallest first

Each step extends something that exists, and none starts before his word.
1. Devices on Home, from our own reader. It extends here-2's reader and the engine's reader of the machines file. Nothing listens, and nothing runs while Home is closed.
   - **Handed in:** H-1594 (the engine's request and the reader), with docket-edge8's Devices section on Home.
2. The window's link to a server on another device: SSH beside `wsl.exe`, with each project naming its device.
   - **Handed in:** H-1595 (the window). The proxy's own pipe to the gateway gets the same form when the gateway moves (p1111), after its own key.
3. The window's file over the link at a new build.
4. The record to GitHub in encrypted pieces, with a takeover by name (a2h-speed's side, with the record holding only what came from outside).
5. The tower's server machine, and moving research there. His yes is in (p1096, p1106, p1111). It waits for the walk-through, the build machine's measured size, and his own key exchange with the new machine.
6. Colab as a machine for jobs. Later, the phone window.

## 12. Where each thing lives
1. **What is written by hand goes in git.** That means code, Docket programs, the policy, texts (pages, AGENTS.md, vocabulary.md, project files) and tests.
   - People and agents write these things, a commit changes them, and git keeps every version.
   - They live in the project's repositories on its server. Where a repository has a private copy on his GitHub account, that copy is git too.
2. **What is built goes in the store.** That means the engine's and the window's programs, test programs, compiled code, and saved states (built from an engine and the record up to a given line).
   - Nobody edits them. Each is found by what it was made from, and any of them can be built again, so losing one costs only time.
   - Only the runner writes into the store.
3. **What happened goes in the record.** The record is the one growing file of everything said and done in a project: his words and presses, the agents' words, the models' answers, and the jobs' results.
   - Only the engine on the project's server writes it. Lines are only added at the end, and nothing in it is ever changed.
   - It is not in git, because nobody writes it by hand. If he says yes to GitHub for records (devices page, section 5), GitHub holds a copy for handing the project over. The record is still written in one place only.
4. **Views and links hold nothing of their own.** That covers Home, the Map, every page, an agent's folder of links, a piece's store layer, a build's folder, and a replay's view of the record up to a line.
   - Each one points into the three above and can be deleted and made again at any time.

There are only two other things:
- **each device's own small things**: the window's place on the screen, his unsent text, and logs. They belong to that device and are never shared;
- **his sign-ins**, which the gateway holds on the server, and nowhere else: never in git, the store, the record or a view.

His personal files are outside all of this, under his rule of 19 Nov.

**Where today's practice differs from this** (figures from the store page, p2726, and docket-edge5's p2749):
- **Built things kept beside the code:**
  - 318 worktrees, 75 of them with their own build output (10.5 GB);
  - builds' folders inside four of them (22 GB);
  - every build's folder holds its own copy of the engine and the window (188 MB each).
  - By the standard they should be the store, with links pointing to it.
- **Copies of the record in place of views:** 243 copies in scratch folders (34.6 GB) and 41 more set aside (6.5 GB). By the standard each should be a view up to its line.
- **Worked-out lines written into the record:** of its 230 MB, 188.7 MB is what the Docket programs and the policy worked out, and it can be worked out again. By the standard that part is a view. Only the 41.6 MB that came from outside is the record.
- **Saved states kept by hand** in scratch folders (21 of them, 12.7 GB), not in the store under their engine and line.
- **The window's program copied:** 56 times into Windows' temporary folder by still runs (7.8 GB), and into C:\yaaihh at each build. By the standard both should come from the store, each one checked by its hash.
- **Copies of code outside git:** the tower keeps a copy of every worktree a job ever came from (457 of them, 31 GB). By the standard a job's code is put together from the store and removed when the job ends.
- **Texts that last, kept outside git:** these two pages and consult-lead's design of 18 Nov (one-interpreter.md, which was then lost from view, p2755) were all written in scratch folders. By the standard, a text meant to last goes into a repository with a commit; scratch folders hold only what can be thrown away.

The store page's steps 1, 2 and 4 remove the built copies, the record copies, the saved states kept by hand and the tower's copies of code. Taking the worked-out lines out of the record is the engine's own work (p2745, p2749). The last difference needs a rule for us: a text meant to last gets a commit on the day it is written.

## Step 1 from what exists (N-1446 p2777)

- **here-2's engine head f2df1afd, unmerged, from 20 Nov.**
  - Its `Request::Machines` and coordd machines.rs stay as they are. They read `~/agent-coord/machines/machines.json` by its fixed name, answer at once, and start one sampler run when the file is 30 s old, so nothing runs while nobody asks.
  - Its `agent-base/bin/machines-sample` reads load, cores and memory here, and the same on the build machine over the runner's own ssh link.
  - Its jobs list and its "reached" builds come out: main's jobs reader (N-1334) and the Releases page already have them.
- **here-2's window head 9dcb1970.** Its page goes, because Home's Devices section is docket-edge8's (N-1446 p2772). Its row pieces and its lenient reading of fields are reused.
- **Missing:**
  - devices as a list, each with its roles;
  - temperatures, the graphics card, disks, and which of our services run;
  - projects and agents, read from the engines;
  - the Windows row, read by the window itself;
  - the store held on each device (a2h-speed's step A);
  - the link (Tailscale's status);
  - the Records row;
  - red marks taken from the sensors' own limits.
- **Handed in as H-1594** (24 Nov), with the fields agreed with docket-edge8 (N-1446 p2814). The answer lists `devices`: the engine's own device first (`here`, `virt`), then the build machine under the runner's name. Jobs, projects and the Windows row come from the window and the engine, not from the reader. `build_machine` stays for the engine's jobs reader.
- **Field shape first given for the stand-in** (every field optional):
  `devices: [{name, roles, on, asked, last_on, load1, cores, mem_used_gb, mem_total_gb, temp_c, temp_crit_c, gpu, disks, projects, services, jobs, store_gb, store_cap_gb, link}], records: [{project, server, sent_at}]`.

## Code and measurements (24 Nov 2026)

- **Code:**
  - The window reaches the server through `yaaihh/src/places.rs` `server_in`, which puts `wsl.exe` in front. The accounts service's twin of it is the proxy's `yaaihh-proxy/src/pipe.rs` `command`. Step 2 added the SSH form to the window (`places.rs` `on`, projects.toml's `[[device]]`, H-1595). The proxy's form follows when the gateway moves.
  - `projects.rs` `pipe`.
  - `link.rs` `Resume`, which restarts the pipe with `--from RUN:SEQ`.
  - `message_box.rs` `sent`, for "not sent" and its resend.
  - `restart.rs`, which swaps on the release.txt that release.sh's `put_exe` writes today.
  - coordd's `jobs.rs` `machines_file`.
- **Measured at about 17:20:**
  - this laptop's WSL: load 3.6 of 16 cores, memory 18.8 of 39 GB, / with 225 GB free;
  - his C: drive: 25 GB free of 953;
  - research's engine unit: 12.2 GiB of anon memory, 8.7 GiB of cache, a peak of 34.6 GiB;
  - the gateway (cpa-core): 1.6 GiB;
  - temperature sensors: none in WSL.
- **Taken from notes, not checked on the machine:** the tower's 24 GB and the build machine's 20 GB (yaaihh's notes of 23 Nov).
- **GitHub's limits,** from docs.github.com, "About large files on GitHub": "GitHub blocks files larger than 100 MiB", a warning above 50 MiB, and repositories "ideally less than 1 GB, and less than 5 GB is strongly recommended".
