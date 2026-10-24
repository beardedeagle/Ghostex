//! The frame header of zmx's private IPC, for the few requests gxserver writes
//! to a session socket itself instead of running the `zmx` CLI.

/// `@sizeOf(ipc.Header)`.
pub(crate) const ZMX_IPC_HEADER_BYTES: usize = 8;
/// `ipc.PROTOCOL_VERSION`.
const ZMX_IPC_PROTOCOL_VERSION: u8 = 1;

/// The header for a message with `tag` and a payload of `payload_len` bytes.
///
/// CDXC:ZmxWireGeneration 2026-10-24 WHY:
/// zmx wire generation 3 adopted upstream's frozen header, an 8-byte `extern struct { tag: u8, protocol_version: u8, _reserved: u16, len: u32 }` in host byte order. A daemon discards every frame whose protocol_version differs, so a frame in the old layout (length at bytes 1..5) is ignored without an error: the chat claim would silently stop holding the grid.
/// SEE-ALSO: .dependencies/zmx/src/ipc.zig (Header, PROTOCOL_VERSION, WIRE_GENERATION), server/src/zmx/screen_capture.rs, server/src/session_chat_grid_claim.rs.
pub(crate) fn zmx_ipc_header(tag: u8, payload_len: u32) -> [u8; ZMX_IPC_HEADER_BYTES] {
    let mut header = [0_u8; ZMX_IPC_HEADER_BYTES];
    header[0] = tag;
    header[1] = ZMX_IPC_PROTOCOL_VERSION;
    header[4..8].copy_from_slice(&payload_len.to_ne_bytes());
    header
}

/// The tag and payload length of a header read off a session socket.
pub(crate) fn parse_zmx_ipc_header(header: &[u8; ZMX_IPC_HEADER_BYTES]) -> (u8, usize) {
    let payload_len = u32::from_ne_bytes([header[4], header[5], header[6], header[7]]);
    (header[0], payload_len as usize)
}
