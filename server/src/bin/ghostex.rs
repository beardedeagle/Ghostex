/*
CDXC:Cli 2026-07-13:
The `ghostex`/`gx` user CLI, ported from scripts/ghostex-cli.mjs into the
gxserver workspace so macOS, Windows, Linux desktops and remote hosts ship one
implementation with no Node runtime. All logic lives in
gxserver::ghostex_cli; this entry point only maps the result to the process
exit code (errors are printed inside run() with the Node CLI's JSON shape).
macOS and Linux releases ship `ghostex` as a link to gxserver instead of this
binary (`invoked_as_cli` in main.rs); Windows and `cargo run --bin ghostex`
still use it.
*/
fn main() {
    std::process::exit(gxserver::ghostex_cli::run());
}
