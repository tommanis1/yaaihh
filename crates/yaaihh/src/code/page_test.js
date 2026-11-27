// node code/page_test.js [SCRIPT]: code/page.js on a stand-in page (no head yet, as at a page's start), checked: its
// style is in at once, the body unseen until the workbench's editor is in, then shown and nothing watched after; and
// shown after 10 s when the workbench never comes. Exit 0 when all hold.
const fs = require('fs');
const path = require('path');
const src = fs.readFileSync(process.argv[2] || path.join(__dirname, 'page.js'), 'utf8');
let fails = 0;
const check = (what, ok) => { console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}`); if (!ok) fails++; };

function page() {
  const classes = new Set();
  const root = { children: [], classList: { add: (c) => classes.add(c), has: (c) => classes.has(c) }, appendChild(c) { this.children.push(c); } };
  const observers = [];
  const timers = [];
  let editor = false;
  const env = {
    document: { documentElement: root, head: null, createElement: () => ({ textContent: '' }), querySelector: (q) => (q === '.monaco-workbench .part.editor' && editor ? {} : null) },
    MutationObserver: class { constructor(f) { this.f = f; this.on = false; observers.push(this); } observe() { this.on = true; } disconnect() { this.on = false; } },
    setTimeout: (f, ms) => timers.push({ f, ms }),
    addEventListener: () => {},
  };
  new Function('document', 'MutationObserver', 'setTimeout', 'addEventListener', src)(env.document, env.MutationObserver, env.setTimeout, env.addEventListener);
  const mutate = () => observers.filter((o) => o.on).forEach((o) => o.f());
  return { root, classes, observers, timers, mutate, editorIn: () => { editor = true; } };
}

const a = page();
check('its style is on the page at once, with no head yet', a.root.children.length === 1 && /visibility:hidden/.test(a.root.children[0].textContent) && /activitybar/.test(a.root.children[0].textContent));
a.mutate();
check('unseen while the workbench is not in', !a.classes.has('yaaihh-ready'));
a.editorIn();
a.mutate();
check('shown once its editor is in', a.classes.has('yaaihh-ready'));
check('and nothing watched after', a.observers.every((o) => !o.on));
const b = page();
check('a fallback after 10 s', b.timers.length === 1 && b.timers[0].ms === 10000);
b.timers[0].f();
check('shown by it when the workbench never comes', b.classes.has('yaaihh-ready') && b.observers.every((o) => !o.on));
process.exit(fails ? 1 : 0);
