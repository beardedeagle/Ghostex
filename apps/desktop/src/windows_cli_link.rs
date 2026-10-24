//! `ghostex.exe` is gxserver.exe under the CLI's name (CDXC:Release 2026-10-23 in server/src/main.rs).

use std::{
    fs,
    io::{self, Read},
    os::windows::io::AsRawHandle,
    path::Path,
};
use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
};

const SERVER: &str = "gxserver.exe";
const CLI: &str = "ghostex.exe";

/// Links the installed app's `resources\native\ghostex.exe`, after Velopack installs or updates the app and on every start.
pub(crate) fn link_installed_cli() {
    let Some(app_dir) = std::env::current_exe()
        .ok()
        .and_then(|executable| executable.parent().map(Path::to_path_buf))
    else {
        return;
    };
    let native = app_dir.join("resources").join("native");
    // Unbundled development executables have no packaged runtime.
    if !native.join(SERVER).is_file() {
        return;
    }
    if let Err(error) = link_cli(&native, &app_dir.join(".retired-native")) {
        eprintln!(
            "Ghostex could not create {}: {error}",
            native.join(CLI).display()
        );
    }
}

/// Makes `bin_dir\ghostex.exe` a hard link to `bin_dir\gxserver.exe` unless it already is one.
///
/// CDXC:Release 2026-10-23 WHY:
/// The installer package cannot hold links, so the CLI is made on the user's PC. A hard link needs no admin rights or Developer Mode, unlike a symbolic link, and Windows reports it under its own name, so the program still starts as the CLI. It keeps the program it was made from when gxserver.exe is replaced, so every place that replaces gxserver.exe links again; checking on every start also repairs a portable copy and a link an antivirus removed. A running CLI keeps its image open, and Windows lets that be renamed but not deleted or overwritten, so an old one moves into `retired_root` first. A drive without hard links (FAT, exFAT) gets a copy.
/// SEE-ALSO: windows_updater.rs, windows_terminal_backend/native_package.rs, tooling/install-windows-gpui.ps1.
pub(crate) fn link_cli(bin_dir: &Path, retired_root: &Path) -> io::Result<()> {
    const ERROR_INVALID_FUNCTION: i32 = 1;
    const ERROR_NOT_SUPPORTED: i32 = 50;
    let server = bin_dir.join(SERVER);
    let cli = bin_dir.join(CLI);
    if cli.is_file() && (same_file(&server, &cli)? || same_contents(&server, &cli)?) {
        return Ok(());
    }
    if cli.symlink_metadata().is_ok() {
        let retired = retired_root.join(uuid::Uuid::new_v4().to_string());
        fs::create_dir_all(&retired)?;
        fs::rename(&cli, retired.join(CLI))?;
    }
    match fs::hard_link(&server, &cli) {
        Err(error)
            if matches!(
                error.raw_os_error(),
                Some(ERROR_INVALID_FUNCTION | ERROR_NOT_SUPPORTED)
            ) =>
        {
            fs::copy(&server, &cli).map(|_| ())
        }
        result => result,
    }
}

fn same_file(first: &Path, second: &Path) -> io::Result<bool> {
    let identity = |path: &Path| -> io::Result<(u32, u32, u32)> {
        let file = fs::File::open(path)?;
        // SAFETY: the handle stays open for the call and the struct is plain data.
        let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok((
            info.dwVolumeSerialNumber,
            info.nFileIndexHigh,
            info.nFileIndexLow,
        ))
    };
    Ok(identity(first)? == identity(second)?)
}

/// A copy made on a drive without hard links.
fn same_contents(first: &Path, second: &Path) -> io::Result<bool> {
    if fs::metadata(first)?.len() != fs::metadata(second)?.len() {
        return Ok(false);
    }
    let (mut first, mut second) = (fs::File::open(first)?, fs::File::open(second)?);
    let (mut left, mut right) = (vec![0_u8; 1 << 16], vec![0_u8; 1 << 16]);
    loop {
        let count = first.read(&mut left)?;
        if count == 0 {
            return Ok(true);
        }
        second.read_exact(&mut right[..count])?;
        if left[..count] != right[..count] {
            return Ok(false);
        }
    }
}
