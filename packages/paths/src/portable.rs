//! A Windows portable copy: the release's portable zip, unpacked anywhere.
//!
//! CDXC:PlatformSupport 2026-10-17 DECISION:
//! The user chose to "fix portable mode". The portable zip used the installed app's data, gxserver port and single-instance lock, so on a PC that also had Ghostex installed it handed its launch to the installed app, or stopped the installed gxserver and every live session with it. A copy whose Velopack root (the folder holding `current\`) carries the `.portable` marker now keeps all its data in `GhostexData\` beside the marker, and uses its own gxserver, code-server and CEF debugging ports, its own editor pipe and its own lock, so it runs beside an installed Ghostex without touching it. An installed Ghostex keeps its folders and ports.
//!
//! CDXC:PlatformSupport 2026-10-17 WHY:
//! The copy is recognised from the running executable's own path (the app in `current\`, gxserver and the CLI in `current\resources\native\` or in the managed package under `GhostexData\`), never from an environment variable: a gxserver started over SSH is handed to the desktop session, which drops the environment (see `start_gxserver_in_user_desktop` in server/src/cli.rs). Ports are picked once, free at that moment, starting from a block derived from the data folder so two portable copies rarely even probe the same ports, and recorded in the data folder with the folder they were picked for: a copied portable folder picks its own instead of reusing the original's.
//! SEE-ALSO: packages/editor-client/src/windows.rs (editor pipe), apps/desktop/src/windows_single_instance.rs (lock), server/src/zmx/scripts_windows.rs (sessions run this copy's CLI), server/src/config.rs and server/src/ghostex_cli/rpc.rs and apps/desktop/src/app/helpers/os_cli/process_and_constants.rs (gxserver port), apps/desktop/src/app/helpers/source_server/code_server.rs (code-server port), apps/desktop/src/cef/shell/browser.rs and server/src/ghostex_cli/browser_mcp/command.rs (CEF debugging port).

use std::{
    env, fs, io,
    net::TcpListener,
    path::{Path, PathBuf},
    sync::OnceLock,
};

/// The file Velopack's portable zip puts in its root folder.
pub const MARKER_FILE: &str = ".portable";
/// The folder beside the marker that holds every Ghostex folder of the copy.
pub const DATA_DIR_NAME: &str = "GhostexData";
const PORTS_FILE: &str = "portable-ports.txt";
/// Ports come in blocks of three, all below Windows' dynamic range (49152 and up), which
/// outgoing connections borrow from at random.
const FIRST_PORT: u16 = 41_000;
const BLOCK: u16 = 3;
const BLOCKS: u16 = 2_700;
const PROBED_BLOCKS: u16 = 64;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PortableCopy {
    /// The Velopack root: `Ghostex.exe`, `Update.exe`, `.portable` and `current\`.
    pub root: PathBuf,
    /// `GhostexData\` beside the marker.
    pub data_dir: PathBuf,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PortablePorts {
    pub gxserver: u16,
    pub code_server: u16,
    pub cef_remote_debugging: u16,
}

/// The portable copy this process belongs to; always `None` outside Windows.
pub fn current() -> Option<&'static PortableCopy> {
    static COPY: OnceLock<Option<PortableCopy>> = OnceLock::new();
    COPY.get_or_init(|| {
        if !cfg!(windows) {
            return None;
        }
        find_from_executable(&env::current_exe().ok()?)
    })
    .as_ref()
}

/// This copy's ports, picked and recorded on first use; `None` outside a portable copy.
pub fn ports() -> Option<PortablePorts> {
    static PORTS: OnceLock<Option<PortablePorts>> = OnceLock::new();
    *PORTS.get_or_init(|| current().map(PortableCopy::ports))
}

/// The nearest folder above `executable` that holds both the marker and `current\`.
pub fn find_from_executable(executable: &Path) -> Option<PortableCopy> {
    executable
        .ancestors()
        .skip(1)
        .find(|directory| {
            directory.join(MARKER_FILE).is_file() && directory.join("current").is_dir()
        })
        .map(|root| PortableCopy {
            root: root.to_path_buf(),
            data_dir: root.join(DATA_DIR_NAME),
        })
}

impl PortableCopy {
    /// A short name unique to this copy's data folder, for Windows object names (pipes) that
    /// would otherwise be shared with an installed Ghostex.
    pub fn instance_key(&self) -> String {
        format!("{:016x}", fnv1a(&self.folder_identity()))
    }

    /// The desktop app inside `current\`.
    pub fn app_executable(&self) -> PathBuf {
        self.root.join("current").join("Ghostex.exe")
    }

    fn folder_identity(&self) -> String {
        self.data_dir
            .to_string_lossy()
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_lowercase()
    }

    fn ports(&self) -> PortablePorts {
        let file = self.data_dir.join(PORTS_FILE);
        let identity = self.folder_identity();
        if let Some(ports) = read_ports(&file, &identity) {
            return ports;
        }
        let chosen = choose_ports(fnv1a(&identity), port_is_free);
        if let Err(error) = write_ports(&file, &identity, chosen) {
            eprintln!(
                "Ghostex could not record its portable ports in {}: {error}",
                file.display()
            );
            return chosen;
        }
        // Another process of this copy may have recorded its pick at the same moment; the file wins.
        read_ports(&file, &identity).unwrap_or(chosen)
    }
}

fn read_ports(file: &Path, identity: &str) -> Option<PortablePorts> {
    let text = fs::read_to_string(file).ok()?;
    let value = |key: &str| {
        text.lines().find_map(|line| {
            let (name, value) = line.split_once('=')?;
            (name.trim() == key).then(|| value.trim().to_string())
        })
    };
    if value("dataDir")?.to_lowercase() != identity {
        return None;
    }
    let port = |key: &str| value(key)?.parse::<u16>().ok().filter(|port| *port != 0);
    Some(PortablePorts {
        gxserver: port("gxserver")?,
        code_server: port("codeServer")?,
        cef_remote_debugging: port("cefRemoteDebugging")?,
    })
}

fn write_ports(file: &Path, identity: &str, ports: PortablePorts) -> io::Result<()> {
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent)?;
    }
    let staged = file.with_extension(format!("{}.tmp", std::process::id()));
    fs::write(
        &staged,
        format!(
            "dataDir={identity}\ngxserver={}\ncodeServer={}\ncefRemoteDebugging={}\n",
            ports.gxserver, ports.code_server, ports.cef_remote_debugging
        ),
    )?;
    fs::rename(&staged, file).inspect_err(|_| {
        let _ = fs::remove_file(&staged);
    })
}

/// The first block, from the one `hash` names onwards, whose three ports are all free.
fn choose_ports(hash: u64, is_free: impl Fn(u16) -> bool) -> PortablePorts {
    let start = (hash % u64::from(BLOCKS)) as u16;
    let block_ports = |block: u16| {
        let base = FIRST_PORT + block * BLOCK;
        PortablePorts {
            gxserver: base,
            code_server: base + 1,
            cef_remote_debugging: base + 2,
        }
    };
    (0..PROBED_BLOCKS)
        .map(|offset| block_ports((start + offset) % BLOCKS))
        .find(|ports| {
            is_free(ports.gxserver)
                && is_free(ports.code_server)
                && is_free(ports.cef_remote_debugging)
        })
        .unwrap_or_else(|| block_ports(start))
}

fn port_is_free(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_ok()
}

/// FNV-1a, the same hash the single-instance lock uses for its mutex name.
fn fnv1a(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}
