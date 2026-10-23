param(
    [Parameter(Mandatory = $true)]
    [string]$StagedAppPath,

    [string]$InstallDir,

    # User: the per-user layout the release's Velopack installer produces (%LOCALAPPDATA%\Ghostex\current).
    # Machine: C:\Program Files\Ghostex (needs administrator approval). Custom: -InstallDir as given.
    [ValidateSet("User", "Machine", "Custom")]
    [string]$Scope = "User",

    [switch]$Elevated
)

$ErrorActionPreference = "Stop"

function Test-IsAdministrator {
    $Identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $Principal = [Security.Principal.WindowsPrincipal]::new($Identity)
    return $Principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

$StagedAppPath = [IO.Path]::GetFullPath($StagedAppPath)
$StagedExecutable = Join-Path $StagedAppPath "Ghostex.exe"
if (-not (Test-Path -LiteralPath $StagedExecutable -PathType Leaf)) {
    throw "The staged Ghostex executable is missing: $StagedExecutable"
}

$WindowsProgramFilesRoot = $env:ProgramW6432
if (-not $WindowsProgramFilesRoot) {
    $WindowsProgramFilesRoot = [Environment]::GetFolderPath([Environment+SpecialFolder]::ProgramFiles)
}
if (-not $WindowsProgramFilesRoot) {
    throw "Windows did not report its Program Files directory."
}
$LocalAppData = [Environment]::GetFolderPath([Environment+SpecialFolder]::LocalApplicationData)
if (-not $LocalAppData) {
    throw "Windows did not report its local application data directory."
}
$MachineInstallDir = Join-Path $WindowsProgramFilesRoot "Ghostex"
# Velopack's root for packId Ghostex; the app itself lives in its `current` folder.
$UserRootDir = Join-Path $LocalAppData "Ghostex"
$UserInstallDir = Join-Path $UserRootDir "current"
if (-not $InstallDir) {
    $InstallDir = switch ($Scope) {
        "User" { $UserInstallDir }
        "Machine" { $MachineInstallDir }
        default { throw "-Scope Custom needs -InstallDir." }
    }
}
$InstallDir = [IO.Path]::GetFullPath($InstallDir).TrimEnd('\')
if ($InstallDir -eq [IO.Path]::GetPathRoot($InstallDir).TrimEnd('\') -or
    $InstallDir -eq $StagedAppPath.TrimEnd('\') -or
    $StagedAppPath.StartsWith($InstallDir + '\', [StringComparison]::OrdinalIgnoreCase) -or
    $InstallDir.StartsWith($StagedAppPath.TrimEnd('\') + '\', [StringComparison]::OrdinalIgnoreCase)) {
    throw "The install directory must be separate from the staged app and cannot be a drive root: $InstallDir"
}
<#
CDXC:Build 2026-10-07 WHY:
The per-user app folder sits beside Ghostex's own Data, State, Cache and Logs under %LOCALAPPDATA%\Ghostex, and the install mirrors the staged app over the install folder (robocopy /MIR deletes what the build does not contain). Refuse any install folder that is or contains a Ghostex data folder, so a wrong -InstallDir or GHOSTEX_INSTALL_DIR can never wipe user data.
#>
$ProtectedDataDirs = @((Join-Path $UserRootDir "Data"), (Join-Path $UserRootDir "State"))
if ($env:GHOSTEX_HOME -and [IO.Path]::IsPathRooted($env:GHOSTEX_HOME)) {
    $ProtectedDataDirs += [IO.Path]::GetFullPath($env:GHOSTEX_HOME).TrimEnd('\')
}
foreach ($ProtectedDataDir in $ProtectedDataDirs) {
    if ([string]::Equals($InstallDir, $ProtectedDataDir, [StringComparison]::OrdinalIgnoreCase) -or
        $ProtectedDataDir.StartsWith($InstallDir + '\', [StringComparison]::OrdinalIgnoreCase)) {
        throw "The install directory cannot be or contain Ghostex's data folder ${ProtectedDataDir}: $InstallDir"
    }
}

if ($Scope -eq "Machine" -and -not (Test-IsAdministrator)) {
    if ($Elevated) {
        throw "Ghostex installation requires administrator access."
    }

    $Arguments = @(
        "-NoProfile"
        "-ExecutionPolicy"
        "Bypass"
        "-File"
        "`"$PSCommandPath`""
        "-StagedAppPath"
        "`"$StagedAppPath`""
        "-InstallDir"
        "`"$InstallDir`""
        "-Scope"
        $Scope
        "-Elevated"
    )
    $Installer = Start-Process `
        -FilePath "powershell.exe" `
        -Verb RunAs `
        -WindowStyle Hidden `
        -ArgumentList $Arguments `
        -Wait `
        -PassThru
    if ($Installer.ExitCode -ne 0) {
        throw "The elevated Ghostex installer failed with exit code $($Installer.ExitCode)."
    }
    exit 0
}

$InstalledExecutable = Join-Path $InstallDir "Ghostex.exe"
if ($Scope -eq "User" -and (Test-Path -LiteralPath (Join-Path $UserRootDir "Update.exe") -PathType Leaf)) {
    Write-Host "A release install lives in $UserRootDir; this build replaces its app files, so its Update.exe stops updating it until the release installer runs again."
}

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
<#
CDXC:Build 2026-09-24 WHY:
Remote Code clients can keep the bundled Node runtime and native editor modules mapped after the desktop closes. Close only processes using this installation's editor executable before mirroring its payload; terminal daemons have a separate lifecycle.
#>
$InstalledEditorExecutable = Join-Path $InstallDir "code-server/lib/node.exe"
$EditorProcesses = @(Get-Process -ErrorAction SilentlyContinue | Where-Object {
    $_.Path -and [string]::Equals($_.Path, $InstalledEditorExecutable, [StringComparison]::OrdinalIgnoreCase)
})
foreach ($EditorProcess in $EditorProcesses) {
    Stop-Process -InputObject $EditorProcess -Force -ErrorAction SilentlyContinue
}
foreach ($EditorProcess in $EditorProcesses) {
    if (-not $EditorProcess.WaitForExit(10000)) {
        throw "The bundled Code editor process $($EditorProcess.Id) did not exit before installation."
    }
}
<#
CDXC:Build 2026-09-23 WHY:
The server removes its HTTP endpoint before its workers finish shutting down, and its mapped image can outlive process-path discovery. Retire a changed image outside the mirror, as with the persistent session provider, so installation does not depend on worker exit or race a reconnecting client. Keep an identical server executable out of the mirror.
#>
$RetiredNativeDir = Join-Path $InstallDir ".retired-native"
# Images retired by earlier installs go once nothing runs them any more; Windows refuses to delete one still in use.
if (Test-Path -LiteralPath $RetiredNativeDir -PathType Container) {
    Get-ChildItem -LiteralPath $RetiredNativeDir -Force | Remove-Item -Recurse -Force -ErrorAction SilentlyContinue
}
$InstalledServer = Join-Path $InstallDir "resources/native/gxserver.exe"
$StagedServer = Join-Path $StagedAppPath "resources/native/gxserver.exe"
$KeepInstalledServer = (Test-Path -LiteralPath $InstalledServer -PathType Leaf) -and
    (Test-Path -LiteralPath $StagedServer -PathType Leaf) -and
    ((Get-FileHash -LiteralPath $InstalledServer -Algorithm SHA256).Hash -eq
        (Get-FileHash -LiteralPath $StagedServer -Algorithm SHA256).Hash)
if (-not $KeepInstalledServer -and (Test-Path -LiteralPath $InstalledServer -PathType Leaf)) {
    $RetiredServerDir = Join-Path $RetiredNativeDir ([Guid]::NewGuid().ToString("N"))
    New-Item -ItemType Directory -Path $RetiredServerDir | Out-Null
    Move-Item -LiteralPath $InstalledServer -Destination (Join-Path $RetiredServerDir "gxserver.exe")
}
$StagedHash = (Get-FileHash -LiteralPath $StagedExecutable -Algorithm SHA256).Hash
<#
CDXC:Build 2026-09-22 WHY:
Persistent Windows sessions keep wmx.exe mapped after the app closes. Windows permits moving that image but cannot overwrite it; retain it outside the mirrored payload so installing a compatible provider preserves live sessions.
#>
$StagedWmx = Join-Path $StagedAppPath "resources/native/wmx.exe"
$InstalledWmx = Join-Path $InstallDir "resources/native/wmx.exe"
if ((Test-Path -LiteralPath $StagedWmx -PathType Leaf) -and (Test-Path -LiteralPath $InstalledWmx -PathType Leaf)) {
    $StagedWmxHash = (Get-FileHash -LiteralPath $StagedWmx -Algorithm SHA256).Hash
    $InstalledWmxHash = (Get-FileHash -LiteralPath $InstalledWmx -Algorithm SHA256).Hash
    if ($StagedWmxHash -ne $InstalledWmxHash) {
        $RetiredVersionDir = Join-Path $RetiredNativeDir ([Guid]::NewGuid().ToString("N"))
        New-Item -ItemType Directory -Path $RetiredVersionDir | Out-Null
        Move-Item -LiteralPath $InstalledWmx -Destination (Join-Path $RetiredVersionDir "wmx.exe")
    }
}
# Exclude both trees when a staged app also contains retained runtime images.
$MirrorExclusions = @('/XD', '.retired-native')
$KeptFiles = @()
if ($KeepInstalledServer) {
    $KeptFiles += $StagedServer
}
<#
CDXC:Build 2026-10-07 WHY:
Any helper can keep a binary of this install running or loaded after the app closes: a `ghostex web` server another session started from resources\native\ghostex.exe failed a whole install with robocopy exit code 11. Windows lets an in-use image be moved but not overwritten or deleted, so every executable, DLL or Node addon that the mirror would replace or remove and that cannot be opened for writing moves to .retired-native first, and the helper keeps running from there; one identical to the staged copy stays out of the mirror instead. gxserver.exe and wmx.exe keep their own rules above, and Ghostex.exe is never retired, so a still-running app fails the install by name.
#>
function Test-FileInUse([string]$Path) {
    try {
        [IO.File]::Open($Path, [IO.FileMode]::Open, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None).Dispose()
        return $false
    } catch {
        return $true
    }
}
$InstallPrefix = $InstallDir + '\'
$RetiredRunDir = Join-Path $RetiredNativeDir ([Guid]::NewGuid().ToString("N"))
$OwnRules = @(".retired-native", "Ghostex.exe", "resources\native\gxserver.exe", "resources\native\wmx.exe")
foreach ($InstalledPath in @([IO.Directory]::EnumerateFiles($InstallDir, "*", [IO.SearchOption]::AllDirectories))) {
    if ([IO.Path]::GetExtension($InstalledPath) -notin @(".exe", ".dll", ".node")) { continue }
    $Relative = $InstalledPath.Substring($InstallPrefix.Length)
    if ($Relative -in $OwnRules -or $Relative.StartsWith(".retired-native\", [StringComparison]::OrdinalIgnoreCase)) { continue }
    $StagedPath = Join-Path $StagedAppPath $Relative
    $HasStaged = Test-Path -LiteralPath $StagedPath -PathType Leaf
    $Installed = [IO.FileInfo]::new($InstalledPath)
    if ($HasStaged) {
        $Staged = [IO.FileInfo]::new($StagedPath)
        # Robocopy skips a file with the same size and time, so it is never written.
        if ($Staged.Length -eq $Installed.Length -and $Staged.LastWriteTimeUtc -eq $Installed.LastWriteTimeUtc) { continue }
    }
    if (-not (Test-FileInUse $InstalledPath)) { continue }
    if ($HasStaged -and (Get-FileHash -LiteralPath $InstalledPath -Algorithm SHA256).Hash -eq (Get-FileHash -LiteralPath $StagedPath -Algorithm SHA256).Hash) {
        $KeptFiles += $StagedPath
        Write-Host "Kept $Relative in place: it is in use and already matches the rebuilt file"
        continue
    }
    $RetiredPath = Join-Path $RetiredRunDir $Relative
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $RetiredPath) | Out-Null
    try {
        Move-Item -LiteralPath $InstalledPath -Destination $RetiredPath
    } catch {
        throw "$InstalledPath is in use and could not be moved aside before installing: $($_.Exception.Message)"
    }
    $Holders = @(Get-Process -ErrorAction SilentlyContinue | Where-Object { $_.Path -and [string]::Equals($_.Path, $InstalledPath, [StringComparison]::OrdinalIgnoreCase) } | ForEach-Object { "$($_.ProcessName) (pid $($_.Id))" })
    $HeldBy = if ($Holders.Count -gt 0) { " (in use by $($Holders -join ', '))" } else { " (in use)" }
    Write-Host "Moved $Relative to $RetiredRunDir$HeldBy; it keeps running from there"
}
if ($KeptFiles.Count -gt 0) {
    $MirrorExclusions += @('/XF') + $KeptFiles
}
$RobocopyOutput = @(& robocopy.exe $StagedAppPath $InstallDir /MIR /COPY:DAT /DCOPY:DAT /R:2 /W:1 /NFL /NDL /NJH /NJS /NP @MirrorExclusions | ForEach-Object { "$_" })
$RobocopyExitCode = $LASTEXITCODE
if ($RobocopyExitCode -gt 7) {
    # Robocopy reports each failure as "<time> ERROR <code> (0x...) <action> <path>" followed by the Windows message.
    $Failures = @()
    for ($Index = 0; $Index -lt $RobocopyOutput.Count; $Index++) {
        $Match = [regex]::Match($RobocopyOutput[$Index], 'ERROR \d+ \(0x[0-9A-Fa-f]+\) (?<what>.+)$')
        if (-not $Match.Success) { continue }
        # "Copying File" names the staged source; the file that failed is its installed copy.
        $What = $Match.Groups["what"].Value.Trim().Replace($StagedAppPath.TrimEnd('\') + '\', $InstallPrefix)
        $Reason = if ($Index + 1 -lt $RobocopyOutput.Count) { $RobocopyOutput[$Index + 1].Trim() } else { "" }
        $Failure = "  $What"
        if ($Reason) { $Failure += ": $Reason" }
        $FailedPath = [regex]::Match($What, '[A-Za-z]:\\.+$').Value
        if ($FailedPath) {
            $Holders = @(Get-Process -ErrorAction SilentlyContinue | Where-Object { $_.Path -and [string]::Equals($_.Path, $FailedPath, [StringComparison]::OrdinalIgnoreCase) } | ForEach-Object { "$($_.ProcessName) (pid $($_.Id))" })
            if ($Holders.Count -gt 0) { $Failure += " Held by $($Holders -join ', ')." }
        }
        if ($Failures -notcontains $Failure) { $Failures += $Failure }
    }
    $Detail = if ($Failures.Count -gt 0) { "`n" + ($Failures -join "`n") } else { "`n" + (($RobocopyOutput | Where-Object { $_.Trim() }) -join "`n") }
    throw "Installing Ghostex into $InstallDir failed with robocopy exit code $RobocopyExitCode.$Detail"
}
if (-not (Test-Path -LiteralPath $InstalledExecutable -PathType Leaf)) {
    throw "The installed Ghostex executable is missing: $InstalledExecutable"
}
<#
CDXC:Release 2026-10-23 WHY:
The staged app has no ghostex.exe: it is gxserver.exe under the CLI's name, a hard link made on the PC (apps/desktop/src/windows_cli_link.rs). The mirror retired or removed the old one like any other executable missing from the build. Link it here so the CLI works before the app first starts, and so a machine-wide install, which the app cannot write to, has one.
#>
$InstalledCli = Join-Path $InstallDir "resources/native/ghostex.exe"
if (Test-Path -LiteralPath $InstalledServer -PathType Leaf) {
    if (Test-Path -LiteralPath $InstalledCli) {
        Remove-Item -LiteralPath $InstalledCli -Force
    }
    New-Item -ItemType HardLink -Path $InstalledCli -Target $InstalledServer | Out-Null
}

<#
CDXC:Build 2026-09-20 DECISION:
User: a rebuild must always replace the executable in Program Files, so an install may not report success while
the installed app is still the previous binary. Existence alone cannot tell a fresh copy from one that a
locked file or a skipped mirror entry left behind, so compare the installed executable with the staged one
and name whatever still holds it open.
#>
$InstalledHash = (Get-FileHash -LiteralPath $InstalledExecutable -Algorithm SHA256).Hash
if ($InstalledHash -ne $StagedHash) {
    $Holders = @(
        Get-Process -ErrorAction SilentlyContinue |
            Where-Object { $_.Path -eq $InstalledExecutable } |
            ForEach-Object { "$($_.ProcessName) (pid $($_.Id))" }
    )
    $Detail = if ($Holders.Count -gt 0) { " Still holding it open: $($Holders -join ', ')." } else { "" }
    throw "$InstalledExecutable still has the previous build after installing (staged $StagedHash, installed $InstalledHash).$Detail"
}
if (Test-Path -LiteralPath $StagedServer -PathType Leaf) {
    if ((Get-FileHash -LiteralPath $InstalledServer -Algorithm SHA256).Hash -ne (Get-FileHash -LiteralPath $StagedServer -Algorithm SHA256).Hash) {
        throw "The installed gxserver does not match the rebuilt binary."
    }
}
if (Test-Path -LiteralPath $StagedWmx -PathType Leaf) {
    if ((Get-FileHash -LiteralPath $InstalledWmx -Algorithm SHA256).Hash -ne (Get-FileHash -LiteralPath $StagedWmx -Algorithm SHA256).Hash) {
        throw "The installed Windows session provider does not match the rebuilt binary."
    }
}

$ProgramsFolder = if ($Scope -eq "Machine") { [Environment+SpecialFolder]::CommonPrograms } else { [Environment+SpecialFolder]::Programs }
$ProgramsDir = [Environment]::GetFolderPath($ProgramsFolder)
if (-not $ProgramsDir) {
    throw "Windows did not report its Start Menu directory."
}
$Shell = New-Object -ComObject WScript.Shell
Write-Host "Installed Ghostex to $InstallDir (Ghostex.exe verified as the rebuilt binary, SHA256 $($StagedHash.Substring(0, 12)))"

if ($Scope -ne "User") {
    $ShortcutDir = Join-Path $ProgramsDir "Ghostex"
    $ShortcutPath = Join-Path $ShortcutDir "Ghostex.lnk"
    New-Item -ItemType Directory -Force -Path $ShortcutDir | Out-Null
    $Shortcut = $Shell.CreateShortcut($ShortcutPath)
    $Shortcut.TargetPath = $InstalledExecutable
    $Shortcut.WorkingDirectory = $InstallDir
    $Shortcut.Description = "Ghostex"
    $Shortcut.IconLocation = "$InstalledExecutable,0"
    $Shortcut.Save()
    Write-Host "Created Start Menu shortcut at $ShortcutPath"
    exit 0
}

<#
CDXC:Build 2026-10-07 WHY:
A release install gets its Start Menu shortcut from Velopack (`--shortcuts StartMenuRoot`, so Programs\Ghostex.lnk with no folder) carrying the AppUserModelID velopack.Ghostex, the id the app's process takes (windows_updater.rs). The per-user dev install writes the same shortcut with the same id, so toasts, the taskbar button and pins behave as they do for users. WScript.Shell cannot write that property, so the shortcut goes through IShellLinkW and IPropertyStore, compiled only when a shortcut actually needs rewriting.
SEE-ALSO: tooling/release-gpui/windows.ps1 (packId, --shortcuts), apps/desktop/src/windows_updater.rs (the process id), apps/desktop/src/app/helpers/os_cli/windows_notifications.rs.
#>
$AppUserModelId = "velopack.Ghostex"
$MachineExecutable = Join-Path $MachineInstallDir "Ghostex.exe"

function Get-ShortcutState([string]$Path) {
    $Link = $Shell.CreateShortcut($Path)
    $Item = (New-Object -ComObject Shell.Application).NameSpace((Split-Path -Parent $Path)).ParseName((Split-Path -Leaf $Path))
    $Id = if ($Item) { [string]$Item.ExtendedProperty("System.AppUserModel.ID") } else { "" }
    [pscustomobject]@{ Target = $Link.TargetPath; Directory = $Link.WorkingDirectory; Id = $Id }
}

function Test-SamePath([string]$Left, [string]$Right) {
    $Left -and $Right -and [string]::Equals($Left.TrimEnd('\'), $Right.TrimEnd('\'), [StringComparison]::OrdinalIgnoreCase)
}

function Save-VelopackShortcut([string]$Path) {
    if (Test-Path -LiteralPath $Path -PathType Leaf) {
        $State = Get-ShortcutState $Path
        if ((Test-SamePath $State.Target $InstalledExecutable) -and (Test-SamePath $State.Directory $InstallDir) -and $State.Id -eq $AppUserModelId) {
            return $false
        }
    }
    if (-not ("GhostexShortcut" -as [type])) {
        Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Runtime.InteropServices.ComTypes;

public static class GhostexShortcut
{
    [ComImport, Guid("00021401-0000-0000-C000-000000000046")]
    private class ShellLink {}

    [ComImport, InterfaceType(ComInterfaceType.InterfaceIsIUnknown), Guid("000214F9-0000-0000-C000-000000000046")]
    private interface IShellLinkW
    {
        void GetPath(IntPtr file, int size, IntPtr data, uint flags);
        void GetIDList(out IntPtr list);
        void SetIDList(IntPtr list);
        void GetDescription(IntPtr name, int size);
        void SetDescription([MarshalAs(UnmanagedType.LPWStr)] string name);
        void GetWorkingDirectory(IntPtr directory, int size);
        void SetWorkingDirectory([MarshalAs(UnmanagedType.LPWStr)] string directory);
        void GetArguments(IntPtr arguments, int size);
        void SetArguments([MarshalAs(UnmanagedType.LPWStr)] string arguments);
        void GetHotkey(out short hotkey);
        void SetHotkey(short hotkey);
        void GetShowCmd(out int command);
        void SetShowCmd(int command);
        void GetIconLocation(IntPtr path, int size, out int index);
        void SetIconLocation([MarshalAs(UnmanagedType.LPWStr)] string path, int index);
        void SetRelativePath([MarshalAs(UnmanagedType.LPWStr)] string path, uint reserved);
        void Resolve(IntPtr window, uint flags);
        void SetPath([MarshalAs(UnmanagedType.LPWStr)] string file);
    }

    [StructLayout(LayoutKind.Sequential, Pack = 4)]
    private struct PropertyKey
    {
        public Guid FormatId;
        public uint PropertyId;
    }

    [StructLayout(LayoutKind.Explicit, Size = 24)]
    private struct PropVariant
    {
        [FieldOffset(0)] public ushort Type;
        [FieldOffset(8)] public IntPtr Pointer;
    }

    [ComImport, InterfaceType(ComInterfaceType.InterfaceIsIUnknown), Guid("886D8EEB-8CF2-4446-8D02-CDBA1DBDCF99")]
    private interface IPropertyStore
    {
        void GetCount(out uint count);
        void GetAt(uint index, out PropertyKey key);
        void GetValue(ref PropertyKey key, out PropVariant value);
        void SetValue(ref PropertyKey key, ref PropVariant value);
        void Commit();
    }

    public static void Save(string path, string target, string directory, string appUserModelId)
    {
        var link = (IShellLinkW)new ShellLink();
        link.SetPath(target);
        link.SetWorkingDirectory(directory);
        link.SetDescription("Ghostex");
        link.SetIconLocation(target, 0);
        var store = (IPropertyStore)link;
        // PKEY_AppUserModel_ID as VT_LPWSTR.
        var key = new PropertyKey { FormatId = new Guid("9F4C2855-9F79-4B39-A8D0-E1D42DE1D5F3"), PropertyId = 5 };
        var value = new PropVariant { Type = 31, Pointer = Marshal.StringToCoTaskMemUni(appUserModelId) };
        try
        {
            store.SetValue(ref key, ref value);
            store.Commit();
        }
        finally
        {
            Marshal.FreeCoTaskMem(value.Pointer);
        }
        ((IPersistFile)link).Save(path, true);
    }
}
'@
    }
    [GhostexShortcut]::Save($Path, $InstalledExecutable, $InstallDir, $AppUserModelId)
    return $true
}

$ShortcutPath = Join-Path $ProgramsDir "Ghostex.lnk"
if (Save-VelopackShortcut $ShortcutPath) {
    Write-Host "Created Start Menu shortcut at $ShortcutPath"
}

<#
CDXC:Build 2026-10-07 WHY:
Moving from the Program Files install to the per-user one must not leave the old copy in use: a Desktop shortcut, a taskbar pin or a login entry that still opens C:\Program Files\Ghostex would start that build and its own gxserver next to the new one. The user's own entry points that open exactly the Program Files Ghostex.exe are pointed at the per-user copy (the user decided nothing is deleted automatically, so the Program Files folder and its machine-wide Start Menu folder stay and are listed instead).
#>
$EntryPointFolders = @(
    [Environment]::GetFolderPath([Environment+SpecialFolder]::DesktopDirectory),
    (Join-Path $env:APPDATA "Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar")
)
foreach ($Folder in $EntryPointFolders) {
    if (-not $Folder -or -not (Test-Path -LiteralPath $Folder -PathType Container)) { continue }
    foreach ($Link in @(Get-ChildItem -LiteralPath $Folder -Filter "*.lnk" -File -ErrorAction SilentlyContinue)) {
        if (Test-SamePath ($Shell.CreateShortcut($Link.FullName).TargetPath) $MachineExecutable) {
            [void](Save-VelopackShortcut $Link.FullName)
            Write-Host "Pointed $($Link.FullName) at the per-user install (it opened $MachineExecutable)"
        }
    }
}
$RunKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run"
$RunValues = Get-ItemProperty -LiteralPath $RunKey -ErrorAction SilentlyContinue
if ($RunValues) {
    foreach ($Value in $RunValues.PSObject.Properties) {
        if ($Value.Name -like "PS*" -or $Value.Value -isnot [string]) { continue }
        $Index = $Value.Value.IndexOf($MachineExecutable, [StringComparison]::OrdinalIgnoreCase)
        if ($Index -lt 0) { continue }
        $Updated = $Value.Value.Remove($Index, $MachineExecutable.Length).Insert($Index, $InstalledExecutable)
        Set-ItemProperty -LiteralPath $RunKey -Name $Value.Name -Value $Updated
        Write-Host "Pointed the login item `"$($Value.Name)`" at the per-user install (it opened $MachineExecutable)"
    }
}

$Leftovers = @()
if (Test-Path -LiteralPath $MachineInstallDir) { $Leftovers += $MachineInstallDir }
$CommonPrograms = [Environment]::GetFolderPath([Environment+SpecialFolder]::CommonPrograms)
if ($CommonPrograms -and (Test-Path -LiteralPath (Join-Path $CommonPrograms "Ghostex"))) {
    $Leftovers += Join-Path $CommonPrograms "Ghostex"
}
if ($Leftovers.Count -gt 0) {
    Write-Host "No longer used and left in place (remove them once no session started before this install is running): $($Leftovers -join '; ')"
}
exit 0
