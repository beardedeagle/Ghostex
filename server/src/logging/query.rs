use super::*;

#[derive(Debug)]
pub enum LogQueryError {
    Input(String),
    Io(io::Error),
}

impl From<io::Error> for LogQueryError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LogQueryOrder {
    Asc,
    Desc,
}

struct LogQueryParams {
    client: Option<String>,
    event: Option<String>,
    event_prefix: Option<String>,
    level: Option<Vec<String>>,
    limit: Option<usize>,
    order: Option<LogQueryOrder>,
    project_id: Option<String>,
    server_id: Option<String>,
    session_id: Option<String>,
    since: Option<String>,
    since_ms: Option<i64>,
    /// A plain, case-sensitive substring the stored line must contain (`ghostex logs --grep`).
    text: Option<String>,
    until_ms: Option<i64>,
}

struct LogLineScan {
    entries: Vec<Value>,
    malformed_line_count: usize,
    scanned_line_count: usize,
}

struct LogQueryRead {
    entries: Vec<Value>,
    malformed_line_count: usize,
    scanned_line_count: usize,
    complete: bool,
    scanned_bytes: u64,
}

/*
CDXC:Diagnostics 2026-06-19-14:45:
`/api/queryLogs` is a read-only local support API over the resolved Ghostex gxserver log. Match the TypeScript request filters, default/maximum limits, reverse/order aliasing, malformed-line tolerance, and bounded head/tail scanning so clients do not scrape support logs directly.
*/
pub fn query_gxserver_logs(
    paths: &GxserverPaths,
    raw_params: &Map<String, Value>,
) -> std::result::Result<Value, LogQueryError> {
    let params = parse_query_logs_params(raw_params)?;
    let limit = params.limit.unwrap_or(DEFAULT_LOG_QUERY_LIMIT);
    let order = params.order.unwrap_or(LogQueryOrder::Asc);
    let Some(file_size_bytes) = read_log_file_size(&paths.log_file)? else {
        return Ok(json!({
            "entries": [],
            "logFileSizeBytes": 0,
            "malformedLineCount": 0,
            "malformedLineCountIsExact": true,
            "scannedBytes": 0,
            "scannedLineCount": 0,
            "totalMatched": 0,
            "totalMatchedIsExact": true,
            "truncated": false,
        }));
    };

    let read = if file_size_bytes <= LOG_QUERY_FULL_SCAN_MAX_BYTES {
        read_complete_gxserver_log_entries(&paths.log_file, &params, file_size_bytes)?
    } else {
        read_bounded_gxserver_log_entries(&paths.log_file, &params, file_size_bytes, limit, order)?
    };
    let total_matched = read.entries.len();
    let entries = order_entries(read.entries, order)
        .into_iter()
        .take(limit)
        .collect::<Vec<_>>();
    let mut result = Map::new();
    result.insert("entries".to_string(), Value::Array(entries));
    result.insert("logFileSizeBytes".to_string(), json!(file_size_bytes));
    result.insert(
        "malformedLineCount".to_string(),
        json!(read.malformed_line_count),
    );
    result.insert(
        "malformedLineCountIsExact".to_string(),
        json!(read.complete),
    );
    result.insert("scannedBytes".to_string(), json!(read.scanned_bytes));
    result.insert(
        "scannedLineCount".to_string(),
        json!(read.scanned_line_count),
    );
    result.insert("totalMatched".to_string(), json!(total_matched));
    result.insert("totalMatchedIsExact".to_string(), json!(read.complete));
    result.insert("truncated".to_string(), json!(!read.complete));
    if !read.complete {
        result.insert("truncatedReason".to_string(), json!("fileWindowExceeded"));
    }
    Ok(Value::Object(result))
}

fn parse_query_logs_params(
    raw_params: &Map<String, Value>,
) -> std::result::Result<LogQueryParams, LogQueryError> {
    let level = parse_level_filter(raw_params.get("level"))?;
    let event = parse_optional_string(raw_params.get("event"), "event")?;
    let event_prefix = parse_optional_string(raw_params.get("eventPrefix"), "eventPrefix")?;
    let server_id = parse_optional_string(raw_params.get("serverId"), "serverId")?;
    let project_id = parse_optional_string(raw_params.get("projectId"), "projectId")?;
    let session_id = parse_optional_string(raw_params.get("sessionId"), "sessionId")?;
    let client = parse_optional_string(raw_params.get("client"), "client")?;
    let since = parse_optional_timestamp(raw_params.get("since"), "since")?;
    let until = parse_optional_timestamp(raw_params.get("until"), "until")?;
    let text = parse_optional_string(raw_params.get("text"), "text")?;
    let limit = parse_optional_limit(raw_params.get("limit"))?;
    let order = parse_order(raw_params.get("order"))?;
    let reverse = parse_optional_boolean(raw_params.get("reverse"), "reverse")?;
    let resolved_order = resolve_order(order, reverse)?;
    let since_ms = since.as_deref().and_then(parse_timestamp_ms);
    let until_ms = until.as_deref().and_then(parse_timestamp_ms);
    Ok(LogQueryParams {
        client,
        event,
        event_prefix,
        level,
        limit,
        order: resolved_order,
        project_id,
        server_id,
        session_id,
        since,
        since_ms,
        text,
        until_ms,
    })
}

fn read_log_file_size(log_file: &Path) -> std::result::Result<Option<u64>, LogQueryError> {
    match fs::metadata(log_file) {
        Ok(metadata) => Ok(Some(metadata.len())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(LogQueryError::Io(error)),
    }
}

fn read_complete_gxserver_log_entries(
    log_file: &Path,
    params: &LogQueryParams,
    file_size_bytes: u64,
) -> std::result::Result<LogQueryRead, LogQueryError> {
    let text = fs::read_to_string(log_file)?;
    let scanned = scan_log_lines(split_log_lines(&text), params);
    Ok(LogQueryRead {
        entries: scanned.entries,
        malformed_line_count: scanned.malformed_line_count,
        scanned_line_count: scanned.scanned_line_count,
        complete: true,
        scanned_bytes: file_size_bytes,
    })
}

fn read_bounded_gxserver_log_entries(
    log_file: &Path,
    params: &LogQueryParams,
    file_size_bytes: u64,
    limit: usize,
    order: LogQueryOrder,
) -> std::result::Result<LogQueryRead, LogQueryError> {
    let window_bytes = file_size_bytes.min(log_query_window_bytes(limit));
    let text = if order == LogQueryOrder::Desc || params.since.is_some() {
        read_log_text_window(
            log_file,
            file_size_bytes - window_bytes,
            window_bytes,
            file_size_bytes,
            LogTextWindowMode::Tail,
        )?
    } else {
        read_log_text_window(
            log_file,
            0,
            window_bytes,
            file_size_bytes,
            LogTextWindowMode::Head,
        )?
    };
    let scanned = scan_log_lines(split_log_lines(&text), params);
    Ok(LogQueryRead {
        entries: scanned.entries,
        malformed_line_count: scanned.malformed_line_count,
        scanned_line_count: scanned.scanned_line_count,
        complete: window_bytes >= file_size_bytes,
        scanned_bytes: window_bytes,
    })
}

fn log_query_window_bytes(limit: usize) -> u64 {
    LOG_QUERY_MAX_WINDOW_BYTES
        .min(LOG_QUERY_WINDOW_BASE_BYTES.max(limit as u64 * LOG_QUERY_ESTIMATED_BYTES_PER_ENTRY))
}

#[derive(Clone, Copy)]
enum LogTextWindowMode {
    Head,
    Tail,
}

fn read_log_text_window(
    log_file: &Path,
    start_offset: u64,
    byte_length: u64,
    file_size_bytes: u64,
    mode: LogTextWindowMode,
) -> std::result::Result<String, LogQueryError> {
    let mut file = File::open(log_file)?;
    file.seek(SeekFrom::Start(start_offset))?;
    let mut buffer = vec![0_u8; byte_length as usize];
    let bytes_read = file.read(&mut buffer)?;
    let mut text = String::from_utf8_lossy(&buffer[..bytes_read]).to_string();
    if matches!(mode, LogTextWindowMode::Tail) && start_offset > 0 {
        text = match text.find('\n') {
            Some(index) => text[index + 1..].to_string(),
            None => String::new(),
        };
    }
    if matches!(mode, LogTextWindowMode::Head)
        && start_offset + bytes_read as u64 >= file_size_bytes
    {
        return Ok(text);
    }
    if matches!(mode, LogTextWindowMode::Head) {
        text = match text.rfind('\n') {
            Some(index) => text[..=index].to_string(),
            None => String::new(),
        };
    }
    Ok(text)
}

fn split_log_lines(text: &str) -> impl Iterator<Item = &str> {
    text.split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
}

fn scan_log_lines<'a>(
    lines: impl IntoIterator<Item = &'a str>,
    params: &LogQueryParams,
) -> LogLineScan {
    let mut entries = Vec::new();
    let mut malformed_line_count = 0;
    let mut scanned_line_count = 0;
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        scanned_line_count += 1;
        match parse_log_line(line) {
            Some(entry)
                if params
                    .text
                    .as_deref()
                    .is_none_or(|text| line.contains(text))
                    && matches_query(&entry, params) =>
            {
                entries.push(entry)
            }
            Some(_) => {}
            None => malformed_line_count += 1,
        }
    }
    LogLineScan {
        entries,
        malformed_line_count,
        scanned_line_count,
    }
}

fn parse_log_line(line: &str) -> Option<Value> {
    let parsed = serde_json::from_str::<Value>(line).ok()?;
    is_gxserver_log_entry(&parsed).then_some(parsed)
}

fn matches_query(entry: &Value, params: &LogQueryParams) -> bool {
    if let Some(levels) = &params.level {
        if !entry
            .get("level")
            .and_then(Value::as_str)
            .map(|level| levels.iter().any(|expected| expected == level))
            .unwrap_or(false)
        {
            return false;
        }
    }
    if let Some(event) = &params.event {
        if entry.get("event").and_then(Value::as_str) != Some(event.as_str()) {
            return false;
        }
    }
    if let Some(event_prefix) = &params.event_prefix {
        if !entry
            .get("event")
            .and_then(Value::as_str)
            .map(|event| event.starts_with(event_prefix))
            .unwrap_or(false)
        {
            return false;
        }
    }
    if let Some(server_id) = &params.server_id {
        if entry.get("serverId").and_then(Value::as_str) != Some(server_id.as_str()) {
            return false;
        }
    }
    if let Some(project_id) = &params.project_id {
        if entry.get("projectId").and_then(Value::as_str) != Some(project_id.as_str()) {
            return false;
        }
    }
    if let Some(session_id) = &params.session_id {
        if entry.get("sessionId").and_then(Value::as_str) != Some(session_id.as_str()) {
            return false;
        }
    }
    if let Some(client) = &params.client {
        if entry.get("client").and_then(Value::as_str) != Some(client.as_str()) {
            return false;
        }
    }
    if params.since_ms.is_some() || params.until_ms.is_some() {
        let Some(timestamp_ms) = entry
            .get("ts")
            .and_then(Value::as_str)
            .and_then(parse_timestamp_ms)
        else {
            return false;
        };
        if params
            .since_ms
            .map(|since_ms| timestamp_ms < since_ms)
            .unwrap_or(false)
        {
            return false;
        }
        if params
            .until_ms
            .map(|until_ms| timestamp_ms > until_ms)
            .unwrap_or(false)
        {
            return false;
        }
    }
    true
}

fn order_entries(mut entries: Vec<Value>, order: LogQueryOrder) -> Vec<Value> {
    entries.sort_by(|left, right| {
        let left_ts = left.get("ts").and_then(Value::as_str).unwrap_or_default();
        let right_ts = right.get("ts").and_then(Value::as_str).unwrap_or_default();
        let ordering = match (parse_timestamp_ms(left_ts), parse_timestamp_ms(right_ts)) {
            (Some(left_ms), Some(right_ms)) => left_ms.cmp(&right_ms),
            _ => left_ts.cmp(right_ts),
        };
        match order {
            LogQueryOrder::Asc => ordering,
            LogQueryOrder::Desc => ordering.reverse(),
        }
    });
    entries
}

fn parse_level_filter(
    value: Option<&Value>,
) -> std::result::Result<Option<Vec<String>>, LogQueryError> {
    let Some(value) = value else {
        return Ok(None);
    };
    if let Some(level) = value.as_str() {
        return Ok(Some(vec![parse_level(level)?.to_string()]));
    }
    if let Some(levels) = value.as_array().filter(|levels| !levels.is_empty()) {
        return levels
            .iter()
            .map(|level| parse_level(level.as_str().unwrap_or_default()).map(str::to_string))
            .collect::<std::result::Result<Vec<_>, _>>()
            .map(Some);
    }
    Err(LogQueryError::Input(
        "level must be a log level or a non-empty log level array.".to_string(),
    ))
}

fn parse_level(value: &str) -> std::result::Result<&'static str, LogQueryError> {
    match value {
        "debug" => Ok("debug"),
        "info" => Ok("info"),
        "warn" => Ok("warn"),
        "error" => Ok("error"),
        _ => Err(LogQueryError::Input(
            "level must be one of debug, info, warn, or error.".to_string(),
        )),
    }
}

fn parse_optional_string(
    value: Option<&Value>,
    field: &str,
) -> std::result::Result<Option<String>, LogQueryError> {
    match value {
        None => Ok(None),
        Some(Value::String(text)) if !text.trim().is_empty() => Ok(Some(text.clone())),
        Some(_) => Err(LogQueryError::Input(format!(
            "{field} must be a non-empty string."
        ))),
    }
}

fn parse_optional_timestamp(
    value: Option<&Value>,
    field: &str,
) -> std::result::Result<Option<String>, LogQueryError> {
    let timestamp = parse_optional_string(value, field)?;
    if timestamp
        .as_deref()
        .map(|timestamp| parse_timestamp_ms(timestamp).is_none())
        .unwrap_or(false)
    {
        return Err(LogQueryError::Input(format!(
            "{field} must be a parseable timestamp."
        )));
    }
    Ok(timestamp)
}

fn parse_optional_limit(
    value: Option<&Value>,
) -> std::result::Result<Option<usize>, LogQueryError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let limit = json_integer(value).filter(|limit| *limit >= 1);
    let Some(limit) = limit.and_then(|limit| usize::try_from(limit).ok()) else {
        return Err(LogQueryError::Input(format!(
            "limit must be an integer from 1 to {MAX_LOG_QUERY_LIMIT}."
        )));
    };
    if limit > MAX_LOG_QUERY_LIMIT {
        return Err(LogQueryError::Input(format!(
            "limit must be an integer from 1 to {MAX_LOG_QUERY_LIMIT}."
        )));
    }
    Ok(Some(limit))
}

fn parse_order(value: Option<&Value>) -> std::result::Result<Option<LogQueryOrder>, LogQueryError> {
    match value {
        None => Ok(None),
        Some(Value::String(value)) if value == "asc" => Ok(Some(LogQueryOrder::Asc)),
        Some(Value::String(value)) if value == "desc" => Ok(Some(LogQueryOrder::Desc)),
        Some(_) => Err(LogQueryError::Input(
            "order must be asc or desc.".to_string(),
        )),
    }
}

fn parse_optional_boolean(
    value: Option<&Value>,
    field: &str,
) -> std::result::Result<Option<bool>, LogQueryError> {
    match value {
        None => Ok(None),
        Some(Value::Bool(value)) => Ok(Some(*value)),
        Some(_) => Err(LogQueryError::Input(format!("{field} must be a boolean."))),
    }
}

fn resolve_order(
    order: Option<LogQueryOrder>,
    reverse: Option<bool>,
) -> std::result::Result<Option<LogQueryOrder>, LogQueryError> {
    if let (Some(order), Some(reverse)) = (order, reverse) {
        let reverse_order = if reverse {
            LogQueryOrder::Desc
        } else {
            LogQueryOrder::Asc
        };
        if order != reverse_order {
            return Err(LogQueryError::Input(
                "order and reverse specify conflicting log order.".to_string(),
            ));
        }
    }
    Ok(order.or_else(|| {
        reverse.map(|reverse| {
            if reverse {
                LogQueryOrder::Desc
            } else {
                LogQueryOrder::Asc
            }
        })
    }))
}

fn json_integer(value: &Value) -> Option<i64> {
    let number = value.as_number()?;
    if let Some(value) = number.as_i64() {
        return Some(value);
    }
    if let Some(value) = number.as_u64() {
        return i64::try_from(value).ok();
    }
    let value = number.as_f64()?;
    if value.is_finite()
        && value.fract() == 0.0
        && value >= i64::MIN as f64
        && value <= i64::MAX as f64
    {
        Some(value as i64)
    } else {
        None
    }
}

fn is_gxserver_log_entry(value: &Value) -> bool {
    value
        .as_object()
        .map(|object| {
            object.get("ts").and_then(Value::as_str).is_some()
                && object.get("event").and_then(Value::as_str).is_some()
                && matches!(
                    object.get("level").and_then(Value::as_str),
                    Some("debug" | "info" | "warn" | "error")
                )
        })
        .unwrap_or(false)
}

fn parse_timestamp_ms(value: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(value)
        .map(|timestamp| timestamp.timestamp_millis())
        .ok()
        .or_else(|| {
            NaiveDate::parse_from_str(value, "%Y-%m-%d")
                .ok()
                .and_then(|date| date.and_hms_opt(0, 0, 0))
                .map(|timestamp| timestamp.and_utc().timestamp_millis())
        })
}
