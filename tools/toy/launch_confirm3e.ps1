# Registers and starts the confirmation chain (three-knobs-revision4.md 12) as a Windows Scheduled Task with no Claude parent.  Run from any PowerShell:  .\launch_confirm3e.ps1
# One process, one pool of 9 workers; no stored row of any earlier stage is read.  It stops at the first STOP marker under out3e\confirm.
$toy = Split-Path -Parent $MyInvocation.MyCommand.Path
$py = if ($env:RP_TOY_PYTHON) { $env:RP_TOY_PYTHON } else { (Get-Command python).Source }   # the Python the tests ran under (needs psutil)
$a = New-ScheduledTaskAction -Execute $py -Argument "confirm3e.py --workers 9" -WorkingDirectory $toy
$set = New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::Zero) -MultipleInstances IgnoreNew -StartWhenAvailable -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
Register-ScheduledTask -TaskName 'Stage3eConfirm' -Action $a -Settings $set -Force | Select-Object TaskName, State
Start-ScheduledTask -TaskName 'Stage3eConfirm'
# progress: the task's console output is not kept; read $toy\out3e\confirm\DONE-* (markers), $toy\out3e\confirm\STOP (a gate fired) and $toy\results-confirm3e.md (the readings, appended cell by cell)
