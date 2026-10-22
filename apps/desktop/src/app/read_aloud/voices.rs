//! The Voice row's choices on Settings > Chat (`readAloudVoice`, `RuntimeOptions::SystemVoices`):
//! System voice first (stored as `""`), then the voices installed on this computer.

/// `(value, label)` for every choice, System voice first. Settings asks once as it opens.
pub(crate) fn voice_options() -> Vec<(String, String)> {
    let mut options = vec![(String::new(), "System voice".to_string())];
    options.extend(installed_voices());
    options
}

#[cfg(not(target_os = "macos"))]
fn installed_voices() -> Vec<(String, String)> {
    Vec::new()
}

/// This Mac's voices: the reader's own language first, then the rest of that language, then
/// every other language; within each, the better voices first and the novelty voices last.
///
/// CDXC:VoiceMode 2026-10-22 WHY: macOS lists 150 to 200 voices, some twice under one name and language (Samantha's compact and super-compact copies), and its Siri voices are named "Voice 2" and "Voice 4". Each name and language is shown once (the better copy), and every label says its language and, when it is better than the default, its quality, so two "Eddy"s and the numbered voices can be told apart.
#[cfg(target_os = "macos")]
fn installed_voices() -> Vec<(String, String)> {
    use serde_json::Value;
    let Some(answer) = super::macos::voices_json() else {
        return Vec::new();
    };
    let current = answer["currentLanguage"].as_str().unwrap_or_default();
    let current_base = current.split(['-', '_']).next().unwrap_or_default();
    struct Voice {
        id: String,
        name: String,
        language: String,
        language_name: String,
        quality: i64,
        novelty: bool,
    }
    let text = |value: &Value, key: &str| value[key].as_str().unwrap_or_default().to_string();
    let mut voices: Vec<Voice> = answer["voices"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|voice| Voice {
            id: text(voice, "id"),
            name: text(voice, "name"),
            language: text(voice, "language"),
            language_name: text(voice, "languageName"),
            quality: voice["quality"].as_i64().unwrap_or(1),
            novelty: voice["novelty"] == true,
        })
        .filter(|voice| !voice.id.is_empty() && !voice.name.is_empty())
        .collect();
    // The better copy of one name and language first, so the dedupe below keeps it.
    voices.sort_by_key(|voice| {
        (
            voice.name.clone(),
            voice.language.clone(),
            -voice.quality,
            voice.id.contains("super-compact"),
        )
    });
    voices.dedup_by(|later, earlier| {
        later.name == earlier.name && later.language == earlier.language
    });
    voices.sort_by_key(|voice| {
        let base = voice.language.split(['-', '_']).next().unwrap_or_default();
        let closeness = if voice.language == current {
            0
        } else if base == current_base {
            1
        } else {
            2
        };
        (
            closeness,
            voice.novelty,
            voice.language_name.clone(),
            -voice.quality,
            voice.name.clone(),
        )
    });
    voices
        .into_iter()
        .map(|voice| {
            let quality = match voice.quality {
                2 => " · Enhanced",
                3 => " · Premium",
                _ => "",
            };
            let label = format!("{} · {}{quality}", voice.name, voice.language_name);
            (voice.id, label)
        })
        .collect()
}
