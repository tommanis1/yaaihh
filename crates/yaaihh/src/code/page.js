// Run in the code view's page before its own scripts (code.rs `surface`): the chrome its settings hide kept hidden from
// the first paint, and the page itself unseen on a plain background until the workbench's editor is in, so its raw
// HTML never flashes; shown after 10 s whatever happens, so the view is never left blank.
(() => {
  const css =
    '.monaco-workbench .part.activitybar,.monaco-workbench .part.statusbar,.monaco-workbench .part.titlebar,.monaco-workbench .part.banner{display:none!important}' +
    'html{background:#1f1f1f}@media (prefers-color-scheme: light){html{background:#ffffff}}' +
    'html:not(.yaaihh-ready) body{visibility:hidden!important}';
  const root = document.documentElement;
  const style = document.createElement('style');
  style.textContent = css;
  // the head does not exist yet at the page's start: the style goes on the root at once
  (document.head || root).appendChild(style);
  let seen = null;
  const shown = () => {
    root.classList.add('yaaihh-ready');
    if (seen) seen.disconnect();
  };
  const look = () => {
    if (document.querySelector('.monaco-workbench .part.editor')) shown();
  };
  seen = new MutationObserver(look);
  seen.observe(root, { childList: true, subtree: true });
  setTimeout(shown, 10000);
})();
