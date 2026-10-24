//! The end of the Windows installer: the LZMA-compressed Velopack Setup.exe, this marker, then the payload's length.
//! Written by src/bin/ghostex_windows_installer_pack.rs at release time, read by src/bin/ghostex_windows_installer.rs.

/// Marks an xz (LZMA) payload. Installers before 2026-10-23 carried the Setup.exe uncompressed behind `GXSETUP1TRAILER!`.
pub(crate) const MAGIC: &[u8; 16] = b"GXSETUP2TRAILER!";
/// The compressed payload's length, a little-endian u64 after the marker.
pub(crate) const LENGTH_LEN: usize = 8;
