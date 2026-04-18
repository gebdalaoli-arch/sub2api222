Set shell = CreateObject("WScript.Shell")
Set fso = CreateObject("Scripting.FileSystemObject")
repoRoot = fso.GetParentFolderName(WScript.ScriptFullName)
scriptPath = repoRoot & "\start-desktop-client.ps1"
shell.Run "powershell.exe -NoProfile -ExecutionPolicy Bypass -File """ & scriptPath & """", 0, False
