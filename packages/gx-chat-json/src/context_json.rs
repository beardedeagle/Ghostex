//! The JSON a phone host sends for one turn's [`ChatContext`].
//!
//! `ChatContext`'s own serde form carries its two random ids as `u128`, which a JavaScript number
//! cannot hold, so the boundary takes them as UUID strings. When the host leaves the random draws
//! out, they are drawn here from the OS source, the same source the desktop host uses
//! (`context()` in `apps/desktop/src/app/gx_chat/worker.rs`); Hermes has no `crypto.randomUUID`.
//! wasm32-unknown-unknown has no OS source, so the wasm build (`packages/gx-chat-wasm`) draws from a
//! splitmix64 stream its page seeds from `crypto.getRandomValues` through [`seed_entropy`].

use ghostex_gx_chat_core::{ChatContext, FormattedTime};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ContextJson {
    now_ms: f64,
    #[serde(default)]
    utc_offset_minutes: i32,
    #[serde(default)]
    random_units: Option<[f64; 2]>,
    #[serde(default)]
    random_ids: Option<[String; 2]>,
    #[serde(default)]
    formatted_times: Vec<FormattedTime>,
    #[serde(default)]
    clock_reads: Vec<f64>,
}

pub(crate) fn parse(json: &str) -> Result<ChatContext, String> {
    let parsed: ContextJson = serde_json::from_str(json).map_err(|error| error.to_string())?;
    let random_units = match parsed.random_units {
        Some(units) => units,
        None => {
            let bytes = entropy::<16>()?;
            [unit(&bytes[0..7]), unit(&bytes[8..15])]
        }
    };
    let random_ids = match parsed.random_ids {
        Some([first, second]) => [uuid_bits(&first)?, uuid_bits(&second)?],
        None => [
            u128::from_be_bytes(entropy::<16>()?),
            u128::from_be_bytes(entropy::<16>()?),
        ],
    };
    Ok(ChatContext::at(parsed.now_ms)
        .with_utc_offset_minutes(parsed.utc_offset_minutes)
        .with_random_units(random_units)
        .with_random_ids(random_ids)
        .with_formatted_times(parsed.formatted_times)
        .with_clock_reads(parsed.clock_reads))
}

#[cfg(not(target_arch = "wasm32"))]
fn entropy<const N: usize>() -> Result<[u8; N], String> {
    let mut bytes = [0u8; N];
    getrandom::fill(&mut bytes).map_err(|error| format!("OS random source: {error}"))?;
    Ok(bytes)
}

#[cfg(target_arch = "wasm32")]
thread_local!(static SPLITMIX: std::cell::Cell<u64> = const { std::cell::Cell::new(0x9E37_79B9_7F4A_7C15) });

/// Seeds the wasm build's random stream; the page calls it once with 64 bits from
/// `crypto.getRandomValues` before it creates a core.
#[cfg(target_arch = "wasm32")]
pub fn seed_entropy(seed: u64) {
    SPLITMIX.with(|state| state.set(seed));
}

#[cfg(target_arch = "wasm32")]
fn entropy<const N: usize>() -> Result<[u8; N], String> {
    let mut bytes = [0u8; N];
    for chunk in bytes.chunks_mut(8) {
        let mut z = SPLITMIX.with(|state| {
            let next = state.get().wrapping_add(0x9E37_79B9_7F4A_7C15);
            state.set(next);
            next
        });
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        chunk.copy_from_slice(&z.to_le_bytes()[..chunk.len()]);
    }
    Ok(bytes)
}

/// 56 random bits as a uniform draw in `[0, 1)`, the desktop host's construction.
fn unit(bytes: &[u8]) -> f64 {
    let value = bytes
        .iter()
        .fold(0u64, |value, byte| (value << 8) | u64::from(*byte));
    value as f64 / (1u64 << 56) as f64
}

/// A UUID (hyphens optional) as the 128-bit number `ChatContext::random_id` prints back.
fn uuid_bits(text: &str) -> Result<u128, String> {
    let hex: String = text.chars().filter(|c| *c != '-').collect();
    if hex.len() != 32 {
        return Err(format!("randomIds entry is not a UUID: {text:?}"));
    }
    u128::from_str_radix(&hex, 16).map_err(|_| format!("randomIds entry is not a UUID: {text:?}"))
}
