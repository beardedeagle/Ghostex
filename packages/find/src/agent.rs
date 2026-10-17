use serde::{Deserialize, Serialize};

/// The agents whose local prompt history zehn indexes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Agent {
    Claude,
    Codex,
    Pi,
    Opencode,
    Cursor,
    Grok,
    Empryo,
    Antigravity,
    Hermes,
    Copilot,
    Droid,
    Omp,
    Zcode,
}

/// Filter order. The agents added on 2026-10-17 come after Empryo so the fork targets stay the
/// leading six every surface numbers 1-6.
pub const ALL_AGENTS: [Agent; 13] = [
    Agent::Claude,
    Agent::Codex,
    Agent::Pi,
    Agent::Opencode,
    Agent::Cursor,
    Agent::Grok,
    Agent::Empryo,
    Agent::Antigravity,
    Agent::Hermes,
    Agent::Copilot,
    Agent::Droid,
    Agent::Omp,
    Agent::Zcode,
];

impl Agent {
    pub fn label(self) -> &'static str {
        match self {
            Agent::Claude => "claude",
            Agent::Codex => "codex",
            Agent::Pi => "pi",
            Agent::Opencode => "opencode",
            Agent::Cursor => "cursor",
            Agent::Grok => "grok",
            Agent::Empryo => "empryo",
            Agent::Antigravity => "antigravity",
            Agent::Hermes => "hermes",
            Agent::Copilot => "copilot",
            Agent::Droid => "droid",
            Agent::Omp => "omp",
            Agent::Zcode => "zcode",
        }
    }

    /// The id Ghostex stores as a session's `agentId` for this agent, which a live owner's row is
    /// matched against. Only Hermes differs from its label.
    pub fn ghostex_agent_id(self) -> &'static str {
        match self {
            Agent::Hermes => "hermes-agent",
            other => other.label(),
        }
    }

    pub fn parse(name: &str) -> Option<Agent> {
        ALL_AGENTS.into_iter().find(|agent| agent.label() == name)
    }

    pub fn bit(self) -> u16 {
        match self {
            Agent::Claude => 1 << 0,
            Agent::Codex => 1 << 1,
            Agent::Pi => 1 << 2,
            Agent::Opencode => 1 << 3,
            Agent::Cursor => 1 << 4,
            Agent::Grok => 1 << 5,
            Agent::Empryo => 1 << 6,
            Agent::Antigravity => 1 << 7,
            Agent::Hermes => 1 << 8,
            Agent::Copilot => 1 << 9,
            Agent::Droid => 1 << 10,
            Agent::Omp => 1 << 11,
            Agent::Zcode => 1 << 12,
        }
    }

    /// Official brand colors as 24-bit truecolor escapes, used by the TUI.
    pub fn ansi_color(self) -> &'static str {
        match self {
            Agent::Claude => "\x1b[38;2;218;119;86m", // #DA7756 Anthropic terra cotta
            Agent::Codex => "\x1b[38;2;16;163;127m",  // #10A37F OpenAI green
            Agent::Opencode => "\x1b[38;2;207;206;205m", // #CFCECD opencode logo gray
            Agent::Pi => "\x1b[38;2;136;192;208m",    // #88C0D0 pi Nord frost
            Agent::Cursor => "\x1b[38;2;74;144;226m", // Cursor blue
            Agent::Grok => "\x1b[38;2;180;160;255m",  // xAI/Grok purple accent
            Agent::Empryo => "\x1b[38;2;31;163;29m",  // #1FA31D Empryo logo green
            Agent::Antigravity => "\x1b[38;2;66;133;244m", // #4285F4 Google blue
            Agent::Hermes => "\x1b[38;2;232;184;74m", // #E8B84A Hermes gold
            Agent::Copilot => "\x1b[38;2;137;87;229m", // #8957E5 GitHub Copilot purple
            Agent::Droid => "\x1b[38;2;239;111;46m",  // #EF6F2E Factory orange
            Agent::Omp => "\x1b[38;2;199;146;234m",   // #C792EA oh-my-pi lavender
            Agent::Zcode => "\x1b[38;2;61;214;198m",  // #3DD6C6 Z.ai teal
        }
    }

    /// The same brand colors as CSS hex, used by the Find GUI.
    pub fn hex_color(self) -> &'static str {
        match self {
            Agent::Claude => "#DA7756",
            Agent::Codex => "#10A37F",
            Agent::Opencode => "#CFCECD",
            Agent::Pi => "#88C0D0",
            Agent::Cursor => "#4A90E2",
            Agent::Grok => "#B4A0FF",
            Agent::Empryo => "#1FA31D",
            Agent::Antigravity => "#4285F4",
            Agent::Hermes => "#E8B84A",
            Agent::Copilot => "#8957E5",
            Agent::Droid => "#EF6F2E",
            Agent::Omp => "#C792EA",
            Agent::Zcode => "#3DD6C6",
        }
    }

    /// Argv that resumes an existing session in this agent. `accept_all` adds
    /// the supported permission-bypass mode; Pi, Empryo, Droid, OMP and ZCode have no bypass
    /// flag in Ghostex (gxserver's accept-all table has none for them either).
    pub fn resume_argv(self, session: &str, accept_all: bool) -> Vec<String> {
        let parts: Vec<&str> = if accept_all {
            match self {
                Agent::Claude => vec![
                    "claude",
                    "--dangerously-skip-permissions",
                    "--resume",
                    session,
                ],
                Agent::Codex => vec!["codex", "--yolo", "resume", session],
                Agent::Pi => vec!["pi", "--session", session],
                Agent::Opencode => {
                    vec![
                        "opencode",
                        "--dangerously-skip-permissions",
                        "--session",
                        session,
                    ]
                }
                Agent::Cursor => vec!["cursor-agent", "--yolo", "--resume", session],
                Agent::Grok => vec![
                    "grok",
                    "--permission-mode",
                    "bypassPermissions",
                    "--resume",
                    session,
                ],
                Agent::Empryo => vec!["empryo", "--session", session],
                Agent::Antigravity => vec![
                    "agy",
                    "--dangerously-skip-permissions",
                    "--conversation",
                    session,
                ],
                Agent::Hermes => vec!["hermes", "--yolo", "--resume", session],
                Agent::Copilot => vec!["copilot", "--yolo", "--resume", session],
                Agent::Droid => vec!["droid", "--resume", session],
                Agent::Omp => vec!["omp", "--session", session],
                Agent::Zcode => vec!["zcode", "--resume", session],
            }
        } else {
            match self {
                Agent::Claude => vec!["claude", "--resume", session],
                Agent::Codex => vec!["codex", "resume", session],
                Agent::Pi => vec!["pi", "--session", session],
                Agent::Opencode => vec!["opencode", "--session", session],
                Agent::Cursor => vec!["cursor-agent", "--resume", session],
                Agent::Grok => vec!["grok", "--resume", session],
                Agent::Empryo => vec!["empryo", "--session", session],
                Agent::Antigravity => vec!["agy", "--conversation", session],
                Agent::Hermes => vec!["hermes", "--resume", session],
                Agent::Copilot => vec!["copilot", "--resume", session],
                Agent::Droid => vec!["droid", "--resume", session],
                Agent::Omp => vec!["omp", "--session", session],
                Agent::Zcode => vec!["zcode", "--resume", session],
            }
        };
        parts.into_iter().map(str::to_string).collect()
    }

    /// Argv that starts a brand-new session seeded with `prompt`. Unlike resume
    /// this needs no session id, so a prompt can be branched into any agent;
    /// `None` for Empryo and ZCode, whose terminal apps take no starting prompt
    /// (`zcode -p` runs one prompt headless and exits).
    pub fn fresh_session_argv(self, prompt: &str, accept_all: bool) -> Option<Vec<String>> {
        let executable = match self {
            Agent::Claude => "claude",
            Agent::Codex => "codex",
            Agent::Pi => "pi",
            Agent::Opencode => "opencode",
            Agent::Cursor => "cursor-agent",
            Agent::Grok => "grok",
            Agent::Antigravity => "agy",
            Agent::Hermes => "hermes",
            Agent::Copilot => "copilot",
            Agent::Droid => "droid",
            Agent::Omp => "omp",
            Agent::Empryo | Agent::Zcode => return None,
        };
        let mut argv = vec![executable.to_string()];
        // Hermes seeds an interactive chat from `chat -q` on a terminal.
        if self == Agent::Hermes {
            argv.push("chat".to_string());
        }
        if accept_all {
            match self {
                Agent::Claude => argv.push("--dangerously-skip-permissions".to_string()),
                Agent::Codex => argv.push("--yolo".to_string()),
                Agent::Opencode => argv.push("--dangerously-skip-permissions".to_string()),
                Agent::Cursor => argv.push("--yolo".to_string()),
                Agent::Grok => {
                    argv.push("--permission-mode".to_string());
                    argv.push("bypassPermissions".to_string());
                }
                Agent::Antigravity => argv.push("--dangerously-skip-permissions".to_string()),
                Agent::Hermes | Agent::Copilot => argv.push("--yolo".to_string()),
                Agent::Pi | Agent::Empryo | Agent::Droid | Agent::Omp | Agent::Zcode => {}
            }
        }
        // These take a bare argument as a one-shot prompt; the flag keeps the session open.
        match self {
            Agent::Antigravity | Agent::Copilot => argv.push("-i".to_string()),
            Agent::Hermes => argv.push("-q".to_string()),
            _ => {}
        }
        argv.push(prompt.to_string());
        Some(argv)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_filter_parsing_accepts_known_names_only() {
        assert_eq!(Agent::parse("claude"), Some(Agent::Claude));
        assert_eq!(Agent::parse("opencode"), Some(Agent::Opencode));
        assert_eq!(Agent::parse("grok"), Some(Agent::Grok));
        assert_eq!(Agent::parse("antigravity"), Some(Agent::Antigravity));
        assert_eq!(Agent::parse("hermes-agent"), None);
    }

    #[test]
    fn resume_argv_optionally_applies_accept_all_flags() {
        assert_eq!(
            Agent::Codex.resume_argv("s", false),
            vec!["codex", "resume", "s"]
        );
        assert_eq!(
            Agent::Codex.resume_argv("s", true),
            vec!["codex", "--yolo", "resume", "s"]
        );
        assert_eq!(
            Agent::Pi.resume_argv("s", true),
            vec!["pi", "--session", "s"]
        );
        assert_eq!(
            Agent::Grok.resume_argv("s", true),
            vec![
                "grok",
                "--permission-mode",
                "bypassPermissions",
                "--resume",
                "s"
            ]
        );
    }

    #[test]
    fn fresh_session_argv_carries_the_prompt_as_one_arg() {
        assert_eq!(
            Agent::Cursor.fresh_session_argv("make it faster", false),
            Some(vec![
                "cursor-agent".to_string(),
                "make it faster".to_string()
            ])
        );
    }
}
