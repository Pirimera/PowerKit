# WinGet COM metadata

`Microsoft.Management.Deployment.winmd` comes from the MIT-licensed NuGet package
[Microsoft.WindowsPackageManager.ComInterop](https://www.nuget.org/packages/Microsoft.WindowsPackageManager.ComInterop)
version **1.29.380** (upstream [winget-cli](https://github.com/microsoft/winget-cli)).

PowerKit vendors only the out-of-proc winmd used for marshaling. It does not ship
the optional in-proc `Microsoft.Management.Deployment.dll`. The build copies this
file next to `powerkit.exe`; Packaged COM still runs inside App Installer.

Refresh the pin when deliberately moving to a newer ComInterop release, then
re-verify search and installed listing on ARM64 and x64.
