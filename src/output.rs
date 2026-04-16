use crate::har::Entry;
use clap::ValueEnum;
use serde_json::Value;

#[derive(Debug, Clone, Copy, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum OutputFormat {
    /// Pretty-printed JSON array
    Json,
    /// One JSON object per line
    Jsonl,
    /// Compact human-readable table
    Summary,
}

/// A specific projection of an Entry, chosen via `--fields`.
///
/// CLI names are kebab-case (e.g. `status-text`); the emitted JSON key keeps
/// HAR spec camelCase (e.g. `statusText`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum Field {
    Url,
    Method,
    Status,
    StatusText,
    Time,
    MimeType,
    StartedDateTime,
}

impl Field {
    fn json_key(self) -> &'static str {
        match self {
            Field::Url => "url",
            Field::Method => "method",
            Field::Status => "status",
            Field::StatusText => "statusText",
            Field::Time => "time",
            Field::MimeType => "mimeType",
            Field::StartedDateTime => "startedDateTime",
        }
    }

    fn value_for(self, entry: &Entry) -> anyhow::Result<Value> {
        Ok(match self {
            Field::Url => Value::String(entry.request.url.clone()),
            Field::Method => Value::String(entry.request.method.clone()),
            Field::Status => Value::Number(entry.response.status.into()),
            Field::StatusText => Value::String(entry.response.status_text.clone()),
            Field::Time => serde_json::to_value(entry.time)?,
            Field::MimeType => {
                Value::String(entry.response.content.mime_type.clone().unwrap_or_default())
            }
            Field::StartedDateTime => Value::String(entry.started_date_time.clone()),
        })
    }
}

/// What to emit for the filtered entries. Modeled as a sum type so illegal
/// combinations (e.g. `--count` with `--fields`) are unrepresentable at the
/// boundary between main and output logic.
pub enum OutputMode {
    Count,
    Formatted {
        format: OutputFormat,
        fields: Vec<Field>,
        no_body: bool,
    },
}

pub fn format_output(entries: &[Entry], mode: &OutputMode) -> anyhow::Result<String> {
    match mode {
        OutputMode::Count => Ok(format!("{}\n", entries.len())),
        OutputMode::Formatted {
            format,
            fields,
            no_body,
        } => match format {
            OutputFormat::Json => format_json(entries, fields, *no_body),
            OutputFormat::Jsonl => format_jsonl(entries, fields, *no_body),
            OutputFormat::Summary => Ok(format_summary(entries)),
        },
    }
}

fn entry_to_value(entry: &Entry, fields: &[Field], no_body: bool) -> anyhow::Result<Value> {
    if !fields.is_empty() {
        let mut map = serde_json::Map::with_capacity(fields.len());
        for field in fields {
            map.insert(field.json_key().to_string(), field.value_for(entry)?);
        }
        return Ok(Value::Object(map));
    }

    let mut value = serde_json::to_value(entry)?;
    if no_body {
        strip_bodies(&mut value);
    }
    Ok(value)
}

fn strip_bodies(value: &mut Value) {
    if let Some(content) = value
        .pointer_mut("/response/content")
        .and_then(Value::as_object_mut)
    {
        content.remove("text");
    }
    if let Some(post_data) = value
        .pointer_mut("/request/postData")
        .and_then(Value::as_object_mut)
    {
        post_data.remove("text");
    }
}

fn format_json(entries: &[Entry], fields: &[Field], no_body: bool) -> anyhow::Result<String> {
    let values: Vec<Value> = entries
        .iter()
        .map(|e| entry_to_value(e, fields, no_body))
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(serde_json::to_string_pretty(&values)? + "\n")
}

fn format_jsonl(entries: &[Entry], fields: &[Field], no_body: bool) -> anyhow::Result<String> {
    let mut output = String::new();
    for entry in entries {
        let value = entry_to_value(entry, fields, no_body)?;
        output.push_str(&serde_json::to_string(&value)?);
        output.push('\n');
    }
    Ok(output)
}

fn format_summary(entries: &[Entry]) -> String {
    let mut output = String::new();
    for entry in entries {
        output.push_str(&format!(
            "{:<6} {:<4} {:<6} {}\n",
            entry.request.method,
            entry.response.status,
            format!("{}ms", entry.time as i64),
            entry.request.url
        ));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::har::Har;

    fn load_entries() -> Vec<Entry> {
        let json = include_str!("../tests/fixtures/valid.har");
        let har: Har = serde_json::from_str(json).unwrap();
        har.log.entries
    }

    fn formatted(format: OutputFormat, fields: Vec<Field>, no_body: bool) -> OutputMode {
        OutputMode::Formatted {
            format,
            fields,
            no_body,
        }
    }

    #[test]
    fn test_json_output() {
        let entries = load_entries();
        let output =
            format_output(&entries, &formatted(OutputFormat::Json, vec![], false)).unwrap();
        let parsed: Vec<Value> = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed.len(), 4);
    }

    #[test]
    fn test_jsonl_output() {
        let entries = load_entries();
        let output =
            format_output(&entries, &formatted(OutputFormat::Jsonl, vec![], false)).unwrap();
        let lines: Vec<&str> = output.trim().lines().collect();
        assert_eq!(lines.len(), 4);
        for line in &lines {
            let _: Value = serde_json::from_str(line).unwrap();
        }
    }

    #[test]
    fn test_summary_output() {
        let entries = load_entries();
        let output =
            format_output(&entries, &formatted(OutputFormat::Summary, vec![], false)).unwrap();
        let lines: Vec<&str> = output.trim().lines().collect();
        assert_eq!(lines.len(), 4);
        assert!(lines[0].contains("GET"));
        assert!(lines[0].contains("200"));
        assert!(lines[0].contains("https://api.example.com/users"));
    }

    #[test]
    fn test_count_output() {
        let entries = load_entries();
        let output = format_output(&entries, &OutputMode::Count).unwrap();
        assert_eq!(output.trim(), "4");
    }

    #[test]
    fn test_fields_selection_basic() {
        let entries = load_entries();
        let fields = vec![Field::Url, Field::Status, Field::Time];
        let output =
            format_output(&entries, &formatted(OutputFormat::Json, fields, false)).unwrap();
        let parsed: Vec<Value> = serde_json::from_str(&output).unwrap();
        let first = &parsed[0];
        assert!(first.get("url").is_some());
        assert!(first.get("status").is_some());
        assert!(first.get("time").is_some());
        assert!(first.get("request").is_none());
    }

    #[test]
    fn test_fields_preserves_camelcase_json_keys() {
        let entries = load_entries();
        let fields = vec![Field::StatusText, Field::MimeType, Field::StartedDateTime];
        let output =
            format_output(&entries, &formatted(OutputFormat::Json, fields, false)).unwrap();
        let parsed: Vec<Value> = serde_json::from_str(&output).unwrap();
        let first = &parsed[0];
        assert_eq!(first["statusText"], "OK");
        assert_eq!(first["mimeType"], "application/json");
        assert!(first.get("startedDateTime").is_some());
    }

    #[test]
    fn test_no_body_strips_response_text() {
        let entries = load_entries();
        let output = format_output(&entries, &formatted(OutputFormat::Json, vec![], true)).unwrap();
        let parsed: Vec<Value> = serde_json::from_str(&output).unwrap();
        for (i, entry) in parsed.iter().enumerate() {
            assert!(
                entry["response"]["content"].get("text").is_none(),
                "entry {i}: response.content.text should be stripped"
            );
        }
    }

    #[test]
    fn test_no_body_strips_post_data_text() {
        // Second entry in fixture (index 1) is the POST with postData.
        let entries = load_entries();
        let output = format_output(&entries, &formatted(OutputFormat::Json, vec![], true)).unwrap();
        let parsed: Vec<Value> = serde_json::from_str(&output).unwrap();
        let post_entry = &parsed[1];
        assert!(
            post_entry["request"]["postData"].is_object(),
            "postData object should still exist after stripping"
        );
        assert!(
            post_entry["request"]["postData"].get("text").is_none(),
            "postData.text should be stripped"
        );
    }

    #[test]
    fn test_empty_entries_json() {
        let entries: Vec<Entry> = vec![];
        let output =
            format_output(&entries, &formatted(OutputFormat::Json, vec![], false)).unwrap();
        assert_eq!(output.trim(), "[]");
    }

    #[test]
    fn test_empty_entries_count() {
        let entries: Vec<Entry> = vec![];
        let output = format_output(&entries, &OutputMode::Count).unwrap();
        assert_eq!(output.trim(), "0");
    }
}
