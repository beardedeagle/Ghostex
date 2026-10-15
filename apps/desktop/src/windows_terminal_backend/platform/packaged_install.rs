use std::{
    env,
    ffi::OsString,
    fs, io,
    path::{Path, PathBuf},
    process::Stdio,
};

use super::*;

pub(super) struct PackagedGxserver {
    archive_path: PathBuf,
    pub(super) sha256: Option<String>,
}

struct PackagedSourceRuntime {
    archive_path: PathBuf,
    component_version: String,
}

pub(super) fn install_packaged_gxserver(
    distribution: &str,
    install_root: &str,
    package: &PackagedGxserver,
) -> Result<(), String> {
    let mut archive = fs::File::open(&package.archive_path)
        .map_err(|_| "The packaged WSL gxserver runtime could not be read.".to_string())?;
    let script = "set -eu; install_root=\"$1\"; archive_sha256=\"$2\"; release_dir=\"$install_root/releases/windows-app-$(date +%s)-$$\"; mkdir -p \"$release_dir\"; tar -xzf - -C \"$release_dir\"; test -x \"$release_dir/bin/gxserver\"; \"$release_dir/bin/gxserver\" setup --install-root \"$install_root\" --release-dir \"$release_dir\" >/dev/null; if [ -n \"$archive_sha256\" ]; then printf '%s\\n' \"$archive_sha256\" >\"$install_root/windows-app-runtime.sha256\"; else rm -f \"$install_root/windows-app-runtime.sha256\"; fi";
    let mut child = hidden_command("wsl.exe")
        .args([
            "--distribution",
            distribution,
            "--exec",
            "sh",
            "-lc",
            script,
            "ghostex-wsl-installer",
            install_root,
            package.sha256.as_deref().unwrap_or(""),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "Could not start WSL to install gxserver.".to_string())?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "Could not stream the gxserver runtime into WSL.".to_string())?;
    if io::copy(&mut archive, &mut stdin).is_err() {
        let _ = child.kill();
        let _ = child.wait();
        return Err("Could not stream the gxserver runtime into WSL.".to_string());
    }
    drop(stdin);
    let status = child
        .wait()
        .map_err(|_| "The WSL gxserver installer did not finish.".to_string())?;
    status.success().then_some(()).ok_or_else(|| {
        "The WSL gxserver runtime could not be installed in the selected distribution.".to_string()
    })
}

fn install_packaged_source_runtime(
    distribution: &str,
    install_root: &str,
    package: &PackagedSourceRuntime,
) -> Result<(), String> {
    let platform = packaged_source_runtime_platform();
    crate::component_store::verify_code_server_archive(
        &package.archive_path,
        &package.component_version,
        platform,
    )?;
    let mut archive = fs::File::open(&package.archive_path)
        .map_err(|_| "The packaged WSL Source runtime could not be read.".to_string())?;
    let payload_checks = crate::component_store::code_server_payload_shell_validation_script()?;
    let script = format!(
        r#"set -eu
install_root="$1"
component_version="$2"
releases_root="$install_root/releases"
release_dir="$releases_root/.install-$component_version-$$"
final_release="$releases_root/$component_version"
previous_release="$releases_root/.previous-$component_version-$$"
package_path="$install_root/package"
marker_path="$install_root/component-version"
marker_next="$install_root/.component-version.next-$$"
marker_previous="$install_root/.component-version.previous-$$"
previous_release_moved=0
new_release_installed=0
package_touched=0
marker_touched=0
had_package=0
had_marker=0
rollback_armed=1
rollback_install() {{
  status="$?"
  trap - EXIT HUP INT TERM
  if [ "$rollback_armed" = 1 ]; then
    if [ "$marker_touched" = 1 ]; then
      if [ "$had_marker" = 1 ]; then
        cp -p -- "$marker_previous" "$marker_path"
      else
        rm -f -- "$marker_path"
      fi
    fi
    if [ "$new_release_installed" = 1 ]; then
      rm -rf -- "$final_release"
    fi
    if [ "$previous_release_moved" = 1 ]; then
      mv -- "$previous_release" "$final_release"
    fi
    if [ "$package_touched" = 1 ]; then
      if [ "$had_package" = 1 ]; then
        ln -sfn -- "$previous_package_target" "$package_path"
      else
        rm -f -- "$package_path"
      fi
    fi
  fi
  rm -rf -- "$release_dir"
  rm -f -- "$marker_next" "$marker_previous"
  exit "$status"
}}
trap rollback_install EXIT HUP INT TERM
mkdir -p "$releases_root"
if [ -L "$package_path" ]; then
  had_package=1
  previous_package_target="$(readlink "$package_path")"
elif [ -e "$package_path" ]; then
  echo "Existing WSL Source package path is not a symlink." >&2
  exit 1
fi
if [ -e "$marker_path" ]; then
  test -f "$marker_path"
  cp -p -- "$marker_path" "$marker_previous"
  had_marker=1
fi
rm -rf -- "$release_dir" "$previous_release"
mkdir -p "$release_dir"
tar -xzf - -C "$release_dir"
code_server_root="$release_dir"
{payload_checks}
"$release_dir/lib/node" "$release_dir/out/node/entry.js" --version >/dev/null
if [ -e "$final_release" ] || [ -L "$final_release" ]; then
  mv -- "$final_release" "$previous_release"
  previous_release_moved=1
fi
mv -- "$release_dir" "$final_release"
new_release_installed=1
package_touched=1
ln -sfn -- "$final_release" "$package_path"
printf '%s\n' "$component_version" >"$marker_next"
marker_touched=1
mv -f -- "$marker_next" "$marker_path"
rollback_armed=0
trap - EXIT HUP INT TERM
rm -rf -- "$previous_release"
rm -f -- "$marker_previous""#
    );
    let mut child = hidden_command("wsl.exe")
        .args([
            "--distribution",
            distribution,
            "--exec",
            "sh",
            "-lc",
            &script,
            "ghostex-wsl-source-installer",
            install_root,
            &package.component_version,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "Could not start WSL to install Source.".to_string())?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "Could not stream the Source runtime into WSL.".to_string())?;
    if io::copy(&mut archive, &mut stdin).is_err() {
        drop(stdin);
        let _ = child.wait();
        cleanup_partial_source_runtime_release(
            distribution,
            install_root,
            &package.component_version,
        );
        return Err("Could not stream the Source runtime into WSL.".to_string());
    }
    drop(stdin);
    let status = match child.wait() {
        Ok(status) => status,
        Err(_) => {
            cleanup_partial_source_runtime_release(
                distribution,
                install_root,
                &package.component_version,
            );
            return Err("The WSL Source runtime installer did not finish.".to_string());
        }
    };
    if !status.success() {
        cleanup_partial_source_runtime_release(
            distribution,
            install_root,
            &package.component_version,
        );
        return Err(
            "The WSL Source runtime could not be installed in the selected distribution."
                .to_string(),
        );
    }
    Ok(())
}

fn cleanup_partial_source_runtime_release(
    distribution: &str,
    install_root: &str,
    component_version: &str,
) {
    let releases_root = format!("{install_root}/releases");
    let _ = run_wsl_status(
        distribution,
        &format!(
            "releases_root={}; component_version={}; for partial in \"$releases_root/.install-$component_version-\"*; do test -e \"$partial\" || continue; rm -rf -- \"$partial\"; done",
            posix_single_quote(&releases_root),
            posix_single_quote(component_version),
        ),
    );
}

pub(super) fn ensure_source_runtime_installed(distribution: &str) -> Result<(), String> {
    let package = resolve_packaged_source_runtime()?
        .ok_or_else(|| "Install the VS Code IDE component before opening Source.".to_string())?;
    let paths = resolve_wsl_ghostex_paths(distribution)?;
    let install_root = paths.data_path("source-runtime");
    let runtime_path = paths.data_path("source-runtime/package");
    let identity_path = paths.data_path("source-runtime/component-version");
    let payload_checks = crate::component_store::code_server_payload_shell_validation_script()?;
    let installed = run_wsl_status(
        distribution,
        &format!(
            "code_server_root={}; {payload_checks}",
            posix_single_quote(&runtime_path),
        ),
    );
    let package_matches = run_wsl_capture(
        distribution,
        &format!(
            "test -r {} && cat {}",
            posix_single_quote(&identity_path),
            posix_single_quote(&identity_path),
        ),
    )
    .is_some_and(|actual| actual.trim() == package.component_version);
    if !installed || !package_matches {
        install_packaged_source_runtime(distribution, &install_root, &package)?;
    }
    Ok(())
}

pub(super) fn resolve_packaged_gxserver() -> Option<PackagedGxserver> {
    if let Some(path) = env::var_os("GHOSTEX_WSL_GXSERVER_ARCHIVE") {
        let path = PathBuf::from(path);
        if path.is_absolute() && path.is_file() {
            let sha256 = packaged_archive_identity(&path);
            return Some(PackagedGxserver {
                archive_path: path,
                sha256,
            });
        }
        return None;
    }
    #[cfg(target_arch = "x86_64")]
    const ARCHIVE_NAME: &str = "gxserver-linux-x64.tar.gz";
    #[cfg(target_arch = "aarch64")]
    const ARCHIVE_NAME: &str = "gxserver-linux-arm64.tar.gz";
    let executable_dir = env::current_exe().ok()?.parent()?.to_path_buf();
    let archive_path = executable_dir
        .join("resources")
        .join("wsl")
        .join(ARCHIVE_NAME);
    archive_path.is_file().then(|| PackagedGxserver {
        sha256: packaged_archive_identity(&archive_path),
        archive_path,
    })
}

fn packaged_source_runtime_platform() -> &'static str {
    #[cfg(target_arch = "x86_64")]
    return "linux-x64";
    #[cfg(target_arch = "aarch64")]
    return "linux-arm64";
}

fn packaged_source_runtime_version(archive_path: &Path) -> Result<String, String> {
    let archive_name = archive_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "The packaged WSL Source runtime name is invalid.".to_string())?;
    let prefix = "code-server-";
    let suffix = format!("-{}.tar.gz", packaged_source_runtime_platform());
    archive_name
        .strip_prefix(prefix)
        .and_then(|value| value.strip_suffix(&suffix))
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| {
            format!(
                "The packaged WSL Source runtime must use the exact canonical archive name for {}.",
                packaged_source_runtime_platform()
            )
        })
}

fn bundled_source_runtime_archive(resources_dir: &Path) -> Result<Option<PathBuf>, String> {
    let wsl_dir = resources_dir.join("wsl");
    let entries = match fs::read_dir(&wsl_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!(
                "Could not inspect bundled WSL Source runtimes under {}: {error}",
                wsl_dir.display()
            ));
        }
    };
    let suffix = format!("-{}.tar.gz", packaged_source_runtime_platform());
    let mut archives = Vec::new();
    for entry in entries {
        let entry = entry
            .map_err(|error| format!("Could not inspect a bundled WSL Source runtime: {error}"))?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if entry
            .file_type()
            .map(|kind| kind.is_file())
            .unwrap_or(false)
            && name.starts_with("code-server-")
            && name.ends_with(&suffix)
        {
            archives.push(entry.path());
        }
    }
    if archives.len() > 1 {
        return Err(format!(
            "The bundled WSL Source runtime contains multiple canonical {} archives.",
            packaged_source_runtime_platform()
        ));
    }
    Ok(archives.pop())
}

/// CDXC:CodeEditor 2026-10-16 WHY:
/// This runs on every WSL Source launch. Verifying the 240MB archive here (a SHA-256 and two gunzip passes, twice through the component store) cost seconds on each open although WSL already held that exact version; `install_packaged_source_runtime` verifies the archive before it unpacks it, the only time its bytes are used.
fn resolve_packaged_source_runtime() -> Result<Option<PackagedSourceRuntime>, String> {
    if let Some(path) = env::var_os("GHOSTEX_WSL_CODE_SERVER_ARCHIVE") {
        let path = PathBuf::from(path);
        if !path.is_absolute() || !path.is_file() {
            return Err(
                "GHOSTEX_WSL_CODE_SERVER_ARCHIVE must name an existing absolute archive."
                    .to_string(),
            );
        }
        let component_version = packaged_source_runtime_version(&path)?;
        return Ok(Some(PackagedSourceRuntime {
            archive_path: path,
            component_version,
        }));
    }
    let executable_dir = env::current_exe()
        .map_err(|error| format!("Could not locate the Ghostex executable: {error}"))?
        .parent()
        .ok_or_else(|| "Could not locate the Ghostex executable directory.".to_string())?
        .to_path_buf();
    let resources_dir = executable_dir.join("resources");
    if let Some(archive_path) = bundled_source_runtime_archive(&resources_dir)? {
        let component_version = packaged_source_runtime_version(&archive_path)?;
        return Ok(Some(PackagedSourceRuntime {
            archive_path,
            component_version,
        }));
    }
    let manifest_path = resources_dir.join("on-demand-resources.json");
    if !manifest_path.is_file() {
        return Ok(None);
    }
    let manifest = crate::component_store::OnDemandManifest::load(&manifest_path)?;
    let store = crate::component_store::ComponentStore::from_manifest(manifest)?;
    let Some((component_version, component_path)) = store.current_marked_install("code-server")?
    else {
        return Ok(None);
    };
    let archive_name = format!(
        "code-server-{}-{}.tar.gz",
        component_version,
        packaged_source_runtime_platform()
    );
    Ok(Some(PackagedSourceRuntime {
        archive_path: component_path.join(archive_name),
        component_version,
    }))
}

fn packaged_archive_identity(archive_path: &Path) -> Option<String> {
    let mut sidecar_name: OsString = archive_path.as_os_str().to_owned();
    sidecar_name.push(".sha256");
    let value = fs::read_to_string(PathBuf::from(sidecar_name)).ok()?;
    let sha256 = value.trim();
    (sha256.len() == 64
        && sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)))
    .then(|| sha256.to_string())
}
