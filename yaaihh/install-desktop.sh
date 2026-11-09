#!/usr/bin/env bash
# install-desktop.sh: yaaihh in the desktop. The icon,
# a launcher entry, and a login autostart that starts it in the tray (--background), all running the release build
# that is current (release.sh: releases/current/yaaihh; a new release going live moves the link, so the launcher and
# the next login start it). A second start opens the running yaaihh's window (src/tray.rs).
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
bin=$(dirname "$here")/releases/current/yaaihh
[ -x "$bin" ] || { echo "no current release: ../release.sh build, then ../release.sh live" >&2; exit 1; }
icons=$HOME/.local/share/icons/hicolor/scalable/apps; apps=$HOME/.local/share/applications; auto=$HOME/.config/autostart
mkdir -p "$icons" "$apps" "$auto"
cp "$here/crates/yaaihh/assets/yaaihh.svg" "$icons/yaaihh.svg"
entry() {  # entry ARGS: a desktop entry running yaaihh with ARGS
  printf '[Desktop Entry]\nType=Application\nName=yaaihh\nComment=Projects and their agents\nExec=%s%s\nIcon=yaaihh\nTerminal=false\nCategories=Development;\nStartupWMClass=yaaihh\n' "$bin" "$1"
}
entry "" >"$apps/yaaihh.desktop"
{ entry " --background"; echo "X-GNOME-Autostart-enabled=true"; } >"$auto/yaaihh.desktop"
update-desktop-database "$apps" 2>/dev/null || true
gtk-update-icon-cache -q "$HOME/.local/share/icons/hicolor" 2>/dev/null || true
echo "yaaihh: launcher $apps/yaaihh.desktop, autostart $auto/yaaihh.desktop (tray), icon $icons/yaaihh.svg"
