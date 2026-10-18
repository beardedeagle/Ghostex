/// CDXC:ServerDaemon 2026-10-18 WHY:
/// WSL health failures can carry 4096 characters of stderr, which overwhelms the toast and the first-run setup message; those show the first 300 while startup diagnostics keep the full detail.
pub(super) fn bounded_health_detail(detail: &str) -> String {
    match detail.char_indices().nth(300) {
        Some((end, _)) => format!("{}…", &detail[..end]),
        None => detail.to_string(),
    }
}
