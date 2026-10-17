/*
CDXC:Telemetry 2026-08-26:
Bake the SHIPPING marketing version into the gxserver binary, mirroring
`apps/desktop/build.rs`.

`CARGO_PKG_VERSION` for this crate is the placeholder `0.1.0` and has never
tracked releases, so without this every analytics event, and anything else that
wants to know what build it is, would report the same value for every version
Ghostex has ever shipped. The release scripts already resolve the real marketing
version for the desktop crate; they now pass the same value here.

CDXC:Build 2026-10-07 WHY:
Builds without `GHOSTEX_GPUI_MARKETING_VERSION` (`cargo xtask start`, `cargo check`)
read the same root `package.json` version, so a dev build reports the real app
version too instead of 0.1.0. They stay identifiable as dev builds through
`GHOSTEX_BUILD_VERSION_STAMPED=0` (`telemetry::base::is_dev_build`), not through
a version that differs from the release. Mirrors `apps/desktop/build.rs`.
*/

use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=GHOSTEX_GPUI_MARKETING_VERSION");
    let (marketing_version, stamped) = marketing_version();
    println!("cargo:rustc-env=GHOSTEX_BUILD_MARKETING_VERSION={marketing_version}");
    println!(
        "cargo:rustc-env=GHOSTEX_BUILD_VERSION_STAMPED={}",
        u8::from(stamped)
    );
}

fn marketing_version() -> (String, bool) {
    if let Some(version) = env::var("GHOSTEX_GPUI_MARKETING_VERSION")
        .ok()
        .map(|version| version.trim().to_string())
        .filter(|version| !version.is_empty())
    {
        return (version, true);
    }
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let package_json = manifest_dir.join("../package.json");
    println!("cargo:rerun-if-changed={}", package_json.display());
    let text = fs::read_to_string(&package_json)
        .unwrap_or_else(|error| panic!("read {}: {error}", package_json.display()));
    let version = text
        .lines()
        .find_map(|line| {
            let rest = line.trim().strip_prefix("\"version\"")?;
            let rest = rest.trim_start().strip_prefix(':')?;
            let rest = rest.trim_start().strip_prefix('"')?;
            Some(rest.split('"').next()?.to_string())
        })
        .filter(|version| !version.is_empty())
        .unwrap_or_else(|| panic!("no \"version\" in {}", package_json.display()));
    (version, false)
}
