//! Cheap look at a frame's `type` without parsing the frame.

/// Returns the top-level `type` string of a JSON object frame, or `None` when the text is not an
/// object, has no top-level string `type`, or the value needs unescaping.
///
/// A full-stream event socket from a gxserver of a release before 2026-10-23 carries one
/// `apiRequestHandled` frame per HTTP request served to any client, its highest-volume frame by far,
/// and every socket carries chat frames a host may not want. This scan allocates nothing and stops at
/// the first top-level `type` key, so a client can drop those frames before paying for a full parse.
/// gxserver serializes keys in sorted order, so `type` sits near the end and the scan walks most
/// of the text once; that is still far cheaper than building values.
///
/// The scan trusts the text to be valid JSON (it comes from gxserver); on malformed input it
/// returns `None` or a best-effort answer and the full parse reports the real error.
pub fn peek_event_type(frame: &str) -> Option<&str> {
    let bytes = frame.as_bytes();
    let mut index = skip_whitespace(bytes, 0);
    if bytes.get(index) != Some(&b'{') {
        return None;
    }
    index += 1;
    loop {
        index = skip_whitespace(bytes, index);
        match bytes.get(index)? {
            b'}' => return None,
            b',' => {
                index += 1;
                continue;
            }
            b'"' => {}
            _ => return None,
        }
        let key_start = index + 1;
        let key_end = string_end(bytes, key_start)?;
        index = skip_whitespace(bytes, key_end + 1);
        if bytes.get(index) != Some(&b':') {
            return None;
        }
        index = skip_whitespace(bytes, index + 1);
        if &bytes[key_start..key_end] == b"type" {
            if bytes.get(index) != Some(&b'"') {
                return None;
            }
            let value_start = index + 1;
            let value_end = string_end(bytes, value_start)?;
            let value = &bytes[value_start..value_end];
            if value.contains(&b'\\') {
                return None;
            }
            // Both ends sit on ASCII quote bytes, so the slice is on char boundaries.
            return frame.get(value_start..value_end);
        }
        index = skip_value(bytes, index)?;
    }
}

fn skip_whitespace(bytes: &[u8], mut index: usize) -> usize {
    while matches!(bytes.get(index), Some(b' ' | b'\t' | b'\n' | b'\r')) {
        index += 1;
    }
    index
}

/// Index of the closing quote of a string whose content starts at `start`.
fn string_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut index = start;
    loop {
        match bytes.get(index)? {
            b'"' => return Some(index),
            b'\\' => index += 2,
            _ => index += 1,
        }
    }
}

/// Index just past the JSON value that starts at `start`.
fn skip_value(bytes: &[u8], start: usize) -> Option<usize> {
    match bytes.get(start)? {
        b'"' => Some(string_end(bytes, start + 1)? + 1),
        b'{' | b'[' => {
            let mut depth = 0usize;
            let mut index = start;
            loop {
                match bytes.get(index)? {
                    b'"' => index = string_end(bytes, index + 1)?,
                    b'{' | b'[' => depth += 1,
                    b'}' | b']' => {
                        depth -= 1;
                        if depth == 0 {
                            return Some(index + 1);
                        }
                    }
                    _ => {}
                }
                index += 1;
            }
        }
        _ => {
            let mut index = start;
            while !matches!(bytes.get(index), None | Some(b',' | b'}' | b']')) {
                index += 1;
            }
            Some(index)
        }
    }
}
