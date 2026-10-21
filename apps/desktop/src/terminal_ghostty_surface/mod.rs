//! The embedded GhosttyKit boundary the composited terminal still uses on macOS: the FFI function
//! table and Ghostty config loading (fonts, colors, metrics, keybind-free settings).

pub(crate) mod function_table;
pub(crate) mod runtime_config;

pub(crate) use function_table::*;
pub(crate) use runtime_config::*;
