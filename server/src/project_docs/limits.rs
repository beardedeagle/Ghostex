/// CDXC:Docs 2026-09-10 WHY:
/// Older screenshots exhausted the 1,200-entry project budget before newer date folders were scanned.
/// Match the desktop budget and fail explicitly at a limit so a partial tree cannot look complete.
/// SEE-ALSO: apps/desktop/src/app/helpers/os_cli/process_and_constants.rs.
pub(super) const FILE_LIST_MAX_ENTRIES: usize = 20_000;
/// CDXC:Docs 2026-10-20 WHY:
/// A project whose Docs folders nest deeper than this lost its whole listing on the phone ("Project Docs nests deeper than 8 folders"), README.md included.
/// The full walk now stops descending at this depth and keeps everything above it; the progressive folder listing has no depth cap, so a folder opened by hand still lists its children.
/// The entry cap below still fails loudly. SEE-ALSO: `MANAGE_FILE_LIST_MAX_DEPTH` in apps/desktop/src/app/helpers/os_cli/process_and_constants.rs.
pub(super) const FILE_LIST_MAX_DEPTH: usize = 8;
/*
CDXC:Docs 2026-08-09:
A mounted Docs directory is a notes tree with a bounded walk.
A directory pointed at a home folder must
fail loudly naming the entry cap instead of walking forever or returning a tree
that silently stops. The depth cap only stops descending (2026-10-20, see
`FILE_LIST_MAX_DEPTH`).
*/
pub(super) const DOCS_TREE_MAX_ENTRIES: usize = 20_000;
pub(super) const DOCS_TREE_MAX_DEPTH: usize = 12;
/// CDXC:Docs 2026-09-28 SEE-ALSO: `MANAGE_FILE_PREVIEW_MAX_BYTES` in apps/desktop/src/app/helpers/os_cli/process_and_constants.rs holds the decision: only files that open in the Markdown editor keep these limits.
pub(super) const FILE_PREVIEW_MAX_BYTES: u64 = 2_000_000;
pub const FILE_SAVE_MAX_BYTES: usize = 2_000_000;
pub(super) const GIT_BASELINE_MAX_BYTES: usize = 1024 * 1024;
pub(super) const RESOURCE_MAX_BYTES: u64 = 12 * 1024 * 1024;
pub(super) const SESSION_CONTEXT_MAX_BYTES: usize = 300_000;
pub(super) const DOCS_RELATIVE_PATH: &str = "docs";
pub(super) const BUILT_IN_DOCS_RELATIVE_PATHS: &[&str] =
    &[DOCS_RELATIVE_PATH, "artifacts", "ai", "tmp"];
/*
CDXC:Docs 2026-08-09:
The reserved first path segment that addresses the mounted Docs directory.
Every other Docs path is project-relative, so one relative path can only ever
mean one root and no read, save, rename, delete, move, or reveal can resolve
out of the root it was addressed to. The cost of that guarantee is that a
project folder with this exact name is not reachable from Docs, which is a
better trade than a vault named `docs` quietly shadowing the repo's own.
*/
pub(super) const EXTRA_ROOT_MOUNT_SEGMENT: &str = ".ghostex-docs-root";
pub(super) const ANNOTATIONS_SIDECAR_RELATIVE_PATH: &str = ".ghostex/manage-annotations.json";
pub(super) const ROOT_ARTIFACT_FILE_EXTENSIONS: &[&str] = &[
    "excalidraw",
    "htm",
    "html",
    "markdown",
    "md",
    "mdown",
    "mkdn",
];
pub(super) const IGNORED_DIRECTORY_NAMES: &[&str] = &[
    ".cache",
    ".git",
    ".ghostex",
    ".gradle",
    ".next",
    ".nuxt",
    ".pytest_cache",
    ".ruff_cache",
    ".svelte-kit",
    ".turbo",
    ".tox",
    ".venv",
    ".vite",
    "DerivedData",
    "build",
    "coverage",
    "dist",
    "node_modules",
    "out",
    "storybook-static",
    "target",
    "tmp",
    "venv",
    "zig-out",
];
