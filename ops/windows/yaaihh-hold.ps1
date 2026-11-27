# yaaihh-hold.ps1: OPTIONAL. Ubuntu held from the owner's logon on, with no window ever opened, so the agents come back
# after a Windows restart before any window is started (docket-core7, N-433 post 32). Without it, the Windows window
# starts the same hold at its own start (agent-base/bin/work-hold), and nothing runs before a window is opened, as today.
#
#   powershell -ExecutionPolicy Bypass -File yaaihh-hold.ps1           register the task "yaaihh-hold" for this user
#   powershell -ExecutionPolicy Bypass -File yaaihh-hold.ps1 -Remove   the way back: the task removed
#
# The task runs at logon: `conhost.exe --headless wsl.exe -d DISTRO -e AGENT_BASE/bin/work-hold`, with no window, no run
# time limit (a task's default of 72 h would end the hold after three days), on battery too, one at a time. A holder
# already running (the window's) makes it exit at once. Untried: written for this machine, not run.
param(
  [switch]$Remove,
  [string]$Distro = "Ubuntu-main",
  [string]$AgentBase = "/home/t/Documents/papers/soft-swarm/agent-base"
)
$name = "yaaihh-hold"
if ($Remove) {
  Unregister-ScheduledTask -TaskName $name -Confirm:$false -ErrorAction SilentlyContinue
  Write-Host "removed: $name (a running hold ends with: wsl -d $Distro -e pkill -f '^yaaihh-holder')"
  exit 0
}
$action = New-ScheduledTaskAction -Execute "$env:SystemRoot\System32\conhost.exe" `
  -Argument "--headless $env:SystemRoot\System32\wsl.exe -d $Distro -e $AgentBase/bin/work-hold"
$trigger = New-ScheduledTaskTrigger -AtLogOn -User "$env:USERDOMAIN\$env:USERNAME"
$settings = New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::Zero) -AllowStartIfOnBatteries `
  -DontStopIfGoingOnBatteries -MultipleInstances IgnoreNew -StartWhenAvailable
$principal = New-ScheduledTaskPrincipal -UserId "$env:USERDOMAIN\$env:USERNAME" -LogonType Interactive -RunLevel Limited
Register-ScheduledTask -TaskName $name -Action $action -Trigger $trigger -Settings $settings -Principal $principal -Force | Out-Null
Write-Host "registered: $name, at logon: wsl.exe -d $Distro -e $AgentBase/bin/work-hold (the way back: -Remove)"
