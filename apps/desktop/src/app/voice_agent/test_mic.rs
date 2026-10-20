//! A developer-only stand-in for the microphone, so a voice conversation and Grok dictation can be
//! tested without anyone speaking. Not in Ghostex Help.
//!
//! When `GHOSTEX_VOICE_TEST_MIC` names a folder at the time the app starts, the microphone is
//! still opened (so echo cancellation and the permission prompt run for real), but what it hears is
//! replaced: each `.wav` file dropped into the folder (16-bit mono 24 kHz, as
//! `say -o q.wav --data-format=LEI16@24000 "..."` writes) is played in, in name order, and deleted;
//! silence fills the gaps. Set it for an app LaunchServices starts with
//! `launchctl setenv GHOSTEX_VOICE_TEST_MIC <folder>` before launching, and `launchctl unsetenv`
//! after.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

const ENV: &str = "GHOSTEX_VOICE_TEST_MIC";
/// How often the folder is looked at while nothing is playing.
const SCAN_INTERVAL: Duration = Duration::from_millis(200);

/// The folder, read once per app run.
fn folder() -> Option<&'static Path> {
    static FOLDER: OnceLock<Option<PathBuf>> = OnceLock::new();
    FOLDER
        .get_or_init(|| {
            std::env::var_os(ENV)
                .map(PathBuf::from)
                .filter(|path| path.is_dir())
        })
        .as_deref()
}

pub(super) struct TestMic {
    folder: &'static Path,
    samples: Vec<i16>,
    position: usize,
    last_scan: Option<Instant>,
}

impl TestMic {
    /// The stand-in, when the developer switch is on.
    pub(super) fn from_env() -> Option<Self> {
        folder().map(|folder| Self {
            folder,
            samples: Vec::new(),
            position: 0,
            last_scan: None,
        })
    }

    /// Overwrites `out` with the next stretch of the queued recordings, or silence.
    pub(super) fn fill(&mut self, out: &mut [i16]) {
        let mut written = 0;
        while written < out.len() {
            if self.position >= self.samples.len() && !self.load_next() {
                break;
            }
            let take = (out.len() - written).min(self.samples.len() - self.position);
            out[written..written + take]
                .copy_from_slice(&self.samples[self.position..self.position + take]);
            self.position += take;
            written += take;
        }
        out[written..].fill(0);
    }

    fn load_next(&mut self) -> bool {
        if self
            .last_scan
            .is_some_and(|scanned| scanned.elapsed() < SCAN_INTERVAL)
        {
            return false;
        }
        self.last_scan = Some(Instant::now());
        let Ok(entries) = std::fs::read_dir(self.folder) else {
            return false;
        };
        let mut wavs: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "wav"))
            .collect();
        wavs.sort();
        for path in wavs {
            let samples = std::fs::read(&path)
                .ok()
                .and_then(|bytes| pcm16_mono_24k(&bytes));
            let _ = std::fs::remove_file(&path);
            if let Some(samples) = samples.filter(|samples| !samples.is_empty()) {
                self.samples = samples;
                self.position = 0;
                return true;
            }
        }
        false
    }
}

/// The samples of a 16-bit mono 24 kHz PCM WAV file, or `None` for anything else.
fn pcm16_mono_24k(bytes: &[u8]) -> Option<Vec<i16>> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return None;
    }
    let mut offset = 12;
    let mut format_ok = false;
    while offset + 8 <= bytes.len() {
        let id = &bytes[offset..offset + 4];
        let size = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().ok()?) as usize;
        let body = bytes.get(offset + 8..(offset + 8 + size).min(bytes.len()))?;
        if id == b"fmt " && body.len() >= 16 {
            let format = u16::from_le_bytes([body[0], body[1]]);
            let channels = u16::from_le_bytes([body[2], body[3]]);
            let rate = u32::from_le_bytes([body[4], body[5], body[6], body[7]]);
            let bits = u16::from_le_bytes([body[14], body[15]]);
            format_ok = format == 1 && channels == 1 && rate == 24_000 && bits == 16;
        } else if id == b"data" {
            if !format_ok {
                return None;
            }
            return Some(
                body.chunks_exact(2)
                    .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
                    .collect(),
            );
        }
        offset += 8 + size + (size & 1);
    }
    None
}
