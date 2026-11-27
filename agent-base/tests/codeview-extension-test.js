// The code view's extension against a stand-in vscode API: opens taken once, a file at its line, a commit as one diff, a repository in the Explorer.
const assert = require('assert');
const fs = require('fs');
const os = require('os');
const path = require('path');
const ext = require('../codeview/yaaihh-open/extension.js');

const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'codeview-ext-'));
fs.mkdirSync(path.join(dir, 'done'));
const now = 1000;
const lines = [
  { id: 'a', at: now - 5, folder: '/r', is: 'file', path: '/r/src/core.rs', line: 142 },
  { id: 'b', at: now - 500, folder: '/r', is: 'repo' },
  { id: 'c', at: now - 5, folder: '/other', is: 'repo' },
];
fs.writeFileSync(path.join(dir, 'open.jsonl'), lines.map(l => JSON.stringify(l)).join('\n') + '\nnot json\n');
assert.deepStrictEqual(ext.pending(dir, ['/q', '/r'], now).map(o => o.id), ['a'], 'fresh, a folder of the workspace, parsed');
assert.deepStrictEqual(ext.pending(dir, ['/r'], now), [], 'taken once');

const calls = [];
const uri = p => ({ fsPath: p, toString: () => p });
const vscode = {
  Uri: { file: uri },
  Range: function (a, b, c, d) { this.at = [a, b, c, d]; },
  TextEditorRevealType: { InCenter: 2 },
  window: { showTextDocument: async (u, o) => { calls.push(['show', u.fsPath, o.selection.at, o.preview]); return { revealRange: (r, t) => calls.push(['reveal', r.at, t]) }; } },
  commands: { executeCommand: async (...a) => calls.push(['cmd', ...a]) },
  extensions: { getExtension: () => ({ isActive: true, exports: { getAPI: () => ({
    getRepository: () => ({
      getCommit: async sha => ({ parents: sha === 'root' ? [] : ['p1'], message: 'second\nbody' }),
      diffBetween: async (a, b) => { calls.push(['diff', a, b]); return [
        { status: 5, uri: uri('/r/m.rs'), originalUri: uri('/r/m.rs') },
        { status: 1, uri: uri('/r/new.rs'), originalUri: uri('/r/new.rs') },
        { status: 6, uri: uri('/r/gone.rs'), originalUri: uri('/r/gone.rs') },
      ]; },
    }),
    toGitUri: (u, ref) => `git:${u.fsPath}@${ref}`,
  }) } }) },
};

(async () => {
  await ext.perform(vscode, { is: 'file', path: '/r/src/core.rs', line: 142 });
  assert.deepStrictEqual(calls.shift(), ['show', '/r/src/core.rs', [141, 0, 141, 0], false]);
  assert.deepStrictEqual(calls.shift(), ['reveal', [141, 0, 141, 0], 2]);
  await ext.perform(vscode, { is: 'commit', folder: '/r', sha: 'abcdef1234' });
  assert.deepStrictEqual(calls.shift(), ['diff', 'p1', 'abcdef1234']);
  const [, cmd, title, rows] = calls.shift();
  assert.strictEqual(cmd, 'vscode.changes');
  assert.strictEqual(title, 'abcdef123 second');
  assert.deepStrictEqual(rows.map(r => [r[0].fsPath, r[1], r[2]]), [
    ['/r/m.rs', 'git:/r/m.rs@p1', 'git:/r/m.rs@abcdef1234'],
    ['/r/new.rs', undefined, 'git:/r/new.rs@abcdef1234'],
    ['/r/gone.rs', 'git:/r/gone.rs@p1', undefined],
  ]);
  await ext.perform(vscode, { is: 'commit', folder: '/r', sha: 'root' });
  assert.deepStrictEqual(calls.shift(), ['diff', '4b825dc642cb6eb9a060e54bf8d69288fbee4904', 'root'], 'a root commit against the empty tree');
  calls.length = 0;
  await ext.perform(vscode, { is: 'repo', folder: '/r' });
  assert.deepStrictEqual(calls.map(c => [c[1], c[2]?.fsPath]), [['workbench.view.explorer', undefined], ['revealInExplorer', '/r']], 'the repository in the Explorer');
  const closed = [];
  const tab = (label, input) => ({ label, input });
  ext.closeWelcome({ window: { tabGroups: { all: [{ tabs: [tab('Welcome'), tab('Walkthrough: Setup VS Code Web'), tab('core.rs', {}), tab('Welcome', {})] }], close: t => closed.push(...t.map(x => x.label)) } } });
  assert.deepStrictEqual(closed, ['Welcome', 'Walkthrough: Setup VS Code Web'], 'only the welcome pages');
  fs.rmSync(dir, { recursive: true });
  console.log('codeview-extension-test: all passed');
})().catch(e => { console.error(e); process.exit(1); });
