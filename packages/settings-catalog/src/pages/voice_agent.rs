use crate::json::opt;
use crate::rows::{row, section, Page, Section};

/// Settings › Voice agent: the provider, its voice and its API key for the manager voice agent,
/// and the xAI key Grok dictation uses.
///
/// CDXC:VoiceMode 2026-10-20 DECISION:
/// User: the voice agent is built into the app, not an extension, so it is a plain Settings page and not a built-in extension card with an on/off switch.
/// CDXC:VoiceMode 2026-10-20 SEE-ALSO:
/// gxserver reads `voiceAgentProvider` and the two voice keys (server/src/voice_agent/keys.rs, which seals the API keys in its vault), and answers `/api/readVoiceAgent` and `/api/setVoiceAgentKey`; the key rows are UI rows with no stored value. Client contract: docs/2026-10-20/voice-mode/voice-agent-client-contract.md.
pub(crate) fn page() -> Page {
    Page {
        id: "voiceAgent",
        title: "Voice agent",
        sections: vec![provider(), keys()],
    }
}

pub(crate) fn provider() -> Section {
    section(
        "provider",
        "Voice agent",
        vec![
            row("voiceAgentProvider", "Voice provider", "Which realtime voice model you talk to: xAI Grok Voice or OpenAI GPT Realtime. Both are billed by the provider while a conversation is open.").options(&[opt("xAI Grok Voice", "xai"), opt("OpenAI GPT Realtime", "openai")]),
            row("voiceAgentXaiVoice", "Grok voice", "The voice Grok Voice speaks with.").options(&[
                opt("Eve", "eve"), opt("Ara", "ara"), opt("Rex", "rex"), opt("Sal", "sal"), opt("Leo", "leo"),
                opt("Altair", "altair"), opt("Atlas", "atlas"), opt("Aurora", "aurora"), opt("Carina", "carina"),
                opt("Castor", "castor"), opt("Celeste", "celeste"), opt("Cosmo", "cosmo"), opt("Helios", "helios"),
                opt("Helix", "helix"), opt("Iris", "iris"), opt("Kepler", "kepler"), opt("Liora", "liora"),
                opt("Lumen", "lumen"), opt("Luna", "luna"), opt("Lux", "lux"), opt("Naksh", "naksh"),
                opt("Orion", "orion"), opt("Perseus", "perseus"), opt("Rigel", "rigel"), opt("Sirius", "sirius"),
                opt("Ursa", "ursa"), opt("Zagan", "zagan"), opt("Zenith", "zenith"),
            ]),
            row("voiceAgentOpenaiVoice", "OpenAI voice", "The voice GPT Realtime speaks with. Marin and Cedar sound the most natural.").options(&[
                opt("Marin", "marin"), opt("Cedar", "cedar"), opt("Alloy", "alloy"), opt("Ash", "ash"),
                opt("Ballad", "ballad"), opt("Coral", "coral"), opt("Echo", "echo"), opt("Sage", "sage"),
                opt("Shimmer", "shimmer"), opt("Verse", "verse"),
            ]),
        ],
    )
}

pub(crate) fn keys() -> Section {
    section(
        "keys",
        "API keys",
        vec![
            row("voiceAgentXaiKey", "xAI API key", "Your xAI API key for Grok Voice and for the chat box's dictation microphone. Kept encrypted on this computer; your phone uses it through this computer and never stores it."),
            row("voiceAgentOpenaiKey", "OpenAI API key", "Your OpenAI API key for GPT Realtime. Kept encrypted on this computer and never shown again after you save it."),
        ],
    )
}
