# Registers and starts the stage 3e chain as a Windows Scheduled Task (no Claude parent).  Run from any PowerShell:  .\launch_chain3e.ps1
# Reuse stores: the re-baseline's outputs (3d) and proof stage 1's run store (ps1); light workers 4 + heavy workers 2 = 6.
$toy = Split-Path -Parent $MyInvocation.MyCommand.Path
$s = 'C:\Users\pierr\AppData\Local\Temp\claude\c92d18e023ce8566\28b06ad0-9458-4799-81e9-72731fe0b356\scratchpad'
$py = 'C:\Users\pierr\AppData\Roaming\uv\python\cpython-3.11-windows-x86_64-none\python.exe'
$argline = "run_chain3e.py --reuse-dirs `"3d=$s\amend\out_new,ps1=$s\proof1\tools\toy\out_proof_s1`" --light-workers 4 --heavy-workers 2"
$a = New-ScheduledTaskAction -Execute $py -Argument $argline -WorkingDirectory $toy
$set = New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::Zero) -MultipleInstances IgnoreNew -StartWhenAvailable -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
Register-ScheduledTask -TaskName 'Stage3eChain' -Action $a -Settings $set -Force | Select-Object TaskName, State
Start-ScheduledTask -TaskName 'Stage3eChain'
# logs: $toy\out3e\chain.log (driver), $toy\out3e\logs\*.log (every call), $toy\out3e\TIME_TOTAL.txt once TIME is computed, $toy\out3e\verdict-<PART>.txt per part
