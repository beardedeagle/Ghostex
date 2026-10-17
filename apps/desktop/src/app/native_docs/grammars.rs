//! Grammars the Files code editor colours with on top of the ones gpui-component bundles.
//!
//! CDXC:Docs 2026-10-17 DECISION:
//! User (Q221): "mapping and top grammars": files that showed without colours get the closest grammar the editor already has (`.env` and ignore files as shell, `.ini`/`.cfg`/`.conf` as TOML, `.jsonl`/`.ndjson`/`.ipynb` as JSON; `editor_language` in state.rs), and Dockerfile, PowerShell and Terraform/HCL get real grammars, registered here. Dart, R, Haskell and Nix stay plain.

use gpui_component::highlighter::{LanguageConfig, LanguageRegistry};

/// Adds the Dockerfile, PowerShell and HCL grammars to the editor's highlighter, once at start-up,
/// so a ```dockerfile fence in chat colours too.
pub(crate) fn register() {
    let registry = LanguageRegistry::singleton();
    registry.register(
        "dockerfile",
        &LanguageConfig::new(
            "dockerfile",
            tree_sitter::Language::new(tree_sitter_containerfile::LANGUAGE),
            // A RUN line's shell and a heredoc's JSON, YAML or TOML.
            vec!["bash".into(), "json".into(), "yaml".into(), "toml".into()],
            tree_sitter_containerfile::HIGHLIGHTS_QUERY,
            tree_sitter_containerfile::INJECTIONS_QUERY,
            "",
        ),
    );
    registry.register(
        "powershell",
        &LanguageConfig::new(
            "powershell",
            tree_sitter::Language::new(tree_sitter_powershell::LANGUAGE),
            Vec::new(),
            tree_sitter_powershell::HIGHLIGHTS_QUERY,
            "",
            "",
        ),
    );
    // The HCL crate ships no highlight query; this one is nvim-treesitter's (Apache-2.0).
    registry.register(
        "hcl",
        &LanguageConfig::new(
            "hcl",
            tree_sitter::Language::new(tree_sitter_hcl::LANGUAGE),
            Vec::new(),
            include_str!("grammars/hcl-highlights.scm"),
            "",
            "",
        ),
    );
}
