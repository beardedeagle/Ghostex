fn main() {
    #[cfg(unix)]
    if invoked_as_cli() {
        std::process::exit(gxserver::ghostex_cli::run());
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(result) = gxserver::cli::run_hook_command(&args) {
        if let Err(error) = result {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }
    // Before the runtime starts any thread: PATH is process-wide state.
    gxserver::prepare_process_environment();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("start the gxserver runtime");
    runtime.block_on(async {
        if let Err(error) = gxserver::cli::run(args).await {
            eprintln!("{error}");
            std::process::exit(1);
        }
    });
    // Dropping the runtime waits for every blocking task without limit, and one stuck on a hung child or drive kept finished processes alive for hours.
    runtime.shutdown_timeout(std::time::Duration::from_secs(2));
}

/// CDXC:Release 2026-10-23 DECISION:
/// The user chose to ship gxserver and the `ghostex` CLI as one program on macOS and Linux to shrink the downloads: the CLI was a second 3.9 MB (compressed) copy of mostly the same code (SQLite, networking, the shared server modules); merged, gxserver grows by 1.8 MB, so the DMG shrinks by about 2 MB and the installed app by about 6 MB (measured 2026-10-23). A CLI call starts about 0.35 ms slower. `ghostex` and `gx` are links to gxserver, and the name the program was started with picks the CLI. Windows WSL runs the Linux package, links included; native Windows still ships `ghostex.exe` as its own program because its installer package cannot hold links, so this check and the CLI code it pulls in are left out of gxserver.exe.
/// SEE-ALSO: `current_cli_executable` in server/src/ghostex_cli/launchers.rs, `gpui_is_package_cli_link_target` in apps/desktop/src/app/helpers/os_cli/cli_install.rs, build-macos-app.sh and prepare-macos-runtime.sh (macOS bundle), server/package-remote-linux.mjs (Linux package), tooling/xtask/src/start_server.rs.
#[cfg(unix)]
fn invoked_as_cli() -> bool {
    std::env::args_os()
        .next()
        .as_deref()
        .map(std::path::Path::new)
        .and_then(std::path::Path::file_name)
        .is_some_and(|name| name == "ghostex" || name == "gx")
}
