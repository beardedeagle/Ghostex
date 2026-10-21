//! The General page's search rows, one function per section, in the order the page lists them.
mod chat;
mod sidebar;
mod system;
mod terminal;
mod theme;
mod tools;

use crate::rows::Section;
use crate::Platform;

pub fn sections(platform: Platform) -> Vec<Section> {
    vec![
        tools::file_opening(),
        tools::browser(),
        tools::editor(),
        system::auto_sleep(),
        system::power(),
        sidebar::session_cards(),
        sidebar::sidebar(),
        theme::theming(),
        chat::chat(),
        sidebar::sidebar_tags(),
        chat::sounds(),
        terminal::terminal(platform),
        terminal::terminal_behavior(platform),
        terminal::terminal_scrolling(),
        tools::terminal_dev_servers(),
        system::sleeping_sessions(),
        system::beta(),
    ]
}
