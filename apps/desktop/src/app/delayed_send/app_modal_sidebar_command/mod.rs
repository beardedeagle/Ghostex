//! `handle_gpui_app_modal_sidebar_command`: the switch over every `sidebarCommand` message the app's own modal windows send. `dispatch.rs` routes each command type to one per-family handler (a sibling file with its own `impl GhostexGpuiApp` block).

mod agents_hub;
mod dialogs;
mod dispatch;
mod hotkey_action;
mod previous_sessions;
mod project_files;
mod projects;
mod saved_prompts;
mod settings;
mod settings_tools;
