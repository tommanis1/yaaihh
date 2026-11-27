# yaaihh-autostart.ps1: the Windows window opens at the owner's logon, from a shortcut in his Startup folder
# (docket-core7, N-433 post 57; N-547). The exe starts work-hold itself (since 6e544c6), so opening it brings up Ubuntu,
# the coordds and their agents, as the WSL window's start file did. A shortcut and not a scheduled task: it opens his
# window in his session, he sees and removes it in Task Manager's Startup apps or the folder itself, and it needs no
# rights. yaaihh-hold.ps1's task is a different thing: the hold without any window.
#
#   powershell -ExecutionPolicy Bypass -File yaaihh-autostart.ps1           the shortcut "yaaihh" in shell:startup
#   powershell -ExecutionPolicy Bypass -File yaaihh-autostart.ps1 -Remove   the way back: the shortcut removed
#
# The shortcut points at C:\yaaihh\yaaihh.exe, which release.sh live replaces in place, so it stays right across
# releases. Untried: written for this machine, not run.
param(
  [switch]$Remove,
  [string]$Exe = "C:\yaaihh\yaaihh.exe"
)
$lnk = Join-Path ([Environment]::GetFolderPath("Startup")) "yaaihh.lnk"
if ($Remove) {
  Remove-Item -LiteralPath $lnk -ErrorAction SilentlyContinue
  Write-Host "removed: $lnk"
  exit 0
}
if (-not (Test-Path -LiteralPath $Exe)) { Write-Host "no ${Exe}: nothing made"; exit 1 }
$s = (New-Object -ComObject WScript.Shell).CreateShortcut($lnk)
$s.TargetPath = $Exe
$s.WorkingDirectory = Split-Path -Parent $Exe
$s.Description = "yaaihh, at logon"
$s.Save()
Write-Host "made: $lnk -> $Exe (the way back: -Remove)"
