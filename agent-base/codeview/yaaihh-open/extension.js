// Performs the opens coordd queues in $YAAIHH_CODEVIEW_DIR/open.jsonl for any repository of this window's workspace.
const fs = require('fs');
const path = require('path');

const EMPTY_TREE = '4b825dc642cb6eb9a060e54bf8d69288fbee4904';
const FRESH_SECS = 120;

// The opens of FOLDERS not yet taken and still fresh, taken by creating done/ID first (one window wins).
function pending(dir, folders, now) {
  let text = '';
  try { text = fs.readFileSync(path.join(dir, 'open.jsonl'), 'utf8'); } catch { return []; }
  const out = [];
  for (const line of text.split('\n')) {
    let o;
    try { o = JSON.parse(line); } catch { continue; }
    if (!folders.includes(o.folder) || now - o.at > FRESH_SECS) continue;
    try { fs.writeFileSync(path.join(dir, 'done', o.id), '', { flag: 'wx' }); } catch { continue; }
    out.push(o);
  }
  return out;
}

// The commit's changes as VS Code's own multi-file diff, against its first parent (the empty tree for a root).
async function openCommit(vscode, folder, sha) {
  const ext = vscode.extensions.getExtension('vscode.git');
  if (!ext) throw new Error('the git extension is not there');
  const git = (ext.isActive ? ext.exports : await ext.activate()).getAPI(1);
  const root = vscode.Uri.file(folder);
  const repo = git.getRepository(root) || await git.openRepository(root);
  if (!repo) throw new Error(`${folder} is not a git repository`);
  const c = await repo.getCommit(sha);
  const parent = c.parents[0] || EMPTY_TREE;
  const changes = await repo.diffBetween(parent, sha);
  const rows = changes.map(ch => {
    const before = ch.status === 1 ? undefined : git.toGitUri(ch.originalUri, parent);
    const after = ch.status === 6 ? undefined : git.toGitUri(ch.uri, sha);
    return [ch.uri, before, after];
  });
  const title = `${sha.slice(0, 9)} ${c.message.split('\n')[0]}`;
  await vscode.commands.executeCommand('vscode.changes', title, rows);
}

// The walkthrough a fresh profile opens: the startup editor reads its setting before the server's are known.
function closeWelcome(vscode) {
  const tabs = vscode.window.tabGroups.all.flatMap(g => g.tabs).filter(t => t.input === undefined && /^(Welcome|Walkthrough)/.test(t.label));
  if (tabs.length) vscode.window.tabGroups.close(tabs);
}

async function perform(vscode, o) {
  if (o.is === 'file') {
    const line = Math.max(0, (o.line || 1) - 1);
    const range = new vscode.Range(line, 0, line, 0);
    const ed = await vscode.window.showTextDocument(vscode.Uri.file(o.path), { selection: range, preview: false });
    ed.revealRange(range, vscode.TextEditorRevealType.InCenter);
  } else if (o.is === 'commit') {
    await openCommit(vscode, o.folder, o.sha);
  } else if (o.is === 'repo') {
    await vscode.commands.executeCommand('workbench.view.explorer');
    await vscode.commands.executeCommand('revealInExplorer', vscode.Uri.file(o.folder));
  }
}

function activate(context) {
  const vscode = require('vscode');
  const dir = process.env.YAAIHH_CODEVIEW_DIR;
  const folders = () => (vscode.workspace.workspaceFolders || []).map(f => f.uri.fsPath);
  if (!dir) return;
  closeWelcome(vscode);
  const run = () => {
    for (const o of pending(dir, folders(), Math.floor(Date.now() / 1000))) {
      perform(vscode, o).catch(e => {
        fs.appendFileSync(path.join(dir, 'extension.log'), `${new Date().toISOString()} ${o.id}: ${e.stack || e}\n`);
        vscode.window.showErrorMessage(`yaaihh: ${e.message}`);
      });
    }
  };
  run();
  const w = fs.watch(dir, (_, f) => { if (f === 'open.jsonl') run(); });
  context.subscriptions.push({ dispose: () => w.close() }, vscode.workspace.onDidChangeWorkspaceFolders(run));
}

module.exports = { activate, deactivate() {}, pending, perform, openCommit, closeWelcome };
