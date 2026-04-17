use crate::har::Entry;
use regex::Regex;
use std::str::FromStr;

/// A range of HTTP status codes, inclusive on both ends.
///
/// Parses from strings in two forms:
/// - Shorthand: `"4xx"` expands to 400-499 (valid digits 1-5)
/// - Numeric: `"200-299"` (start must be <= end)
#[derive(Debug, Clone)]
pub struct StatusRange {
    pub start: u16,
    pub end: u16,
}

impl StatusRange {
    pub fn contains(&self, status: u16) -> bool {
        status >= self.start && status <= self.end
    }
}

impl FromStr for StatusRange {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes = s.as_bytes();

        // Shorthand form: e.g. "4xx"
        if bytes.len() == 3 && &bytes[1..3] == b"xx" {
            let first = bytes[0];
            if !first.is_ascii_digit() {
                return Err(format!("invalid status range '{s}': expected e.g. '4xx'"));
            }
            let digit = u16::from(first - b'0');
            if !(1..=5).contains(&digit) {
                return Err(format!("invalid status range '{s}': digit must be 1-5"));
            }
            let start = digit * 100;
            return Ok(StatusRange {
                start,
                end: start + 99,
            });
        }

        // Numeric range: e.g. "200-299"
        if let Some((a, b)) = s.split_once('-') {
            let start: u16 = a
                .parse()
                .map_err(|_| format!("invalid status range '{s}'"))?;
            let end: u16 = b
                .parse()
                .map_err(|_| format!("invalid status range '{s}'"))?;
            if start > end {
                return Err(format!(
                    "invalid status range '{s}': start ({start}) > end ({end})"
                ));
            }
            return Ok(StatusRange { start, end });
        }

        Err(format!(
            "invalid status range '{s}': use '4xx' or '200-299'"
        ))
    }
}

/// A header filter: a name to match (case-insensitive) plus an optional value substring.
#[derive(Debug, Clone)]
pub struct HeaderFilter {
    pub name: String,
    pub value: Option<String>,
}

impl FromStr for HeaderFilter {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err("header filter is empty".to_string());
        }
        match s.split_once(':') {
            Some((name, value)) => {
                if name.is_empty() {
                    return Err(format!("header filter '{s}' has empty name"));
                }
                Ok(HeaderFilter {
                    name: name.to_string(),
                    value: Some(value.to_string()),
                })
            }
            None => Ok(HeaderFilter {
                name: s.to_string(),
                value: None,
            }),
        }
    }
}

#[derive(Default)]
pub struct FilterOptions {
    pub method: Option<String>,
    pub status: Option<u16>,
    pub status_range: Option<StatusRange>,
    pub url: Option<String>,
    pub url_regex: Option<Regex>,
    pub header: Option<HeaderFilter>,
    pub mime: Option<String>,
    pub min_time: Option<f64>,
    /// Substring match against request postData.text OR response content.text.
    /// Matches if either contains the pattern. Agents fall through to
    /// `grep`/`rg` on raw HAR otherwise, which is noisy and unreliable.
    pub body_grep: Option<String>,
}

/// Filter entries against the provided options, preserving each entry's
/// original index in the HAR. Downstream formatters emit this index as `id`,
/// which lets an LLM agent list entries and then fetch one by id stably even
/// after the filter set changes.
pub fn filter_entries(entries: Vec<Entry>, opts: &FilterOptions) -> Vec<(usize, Entry)> {
    entries
        .into_iter()
        .enumerate()
        .filter(|(_, entry)| matches_all(entry, opts))
        .collect()
}

fn matches_all(entry: &Entry, opts: &FilterOptions) -> bool {
    if let Some(ref method) = opts.method
        && !entry.request.method.eq_ignore_ascii_case(method)
    {
        return false;
    }
    if let Some(status) = opts.status
        && entry.response.status != status
    {
        return false;
    }
    if let Some(ref range) = opts.status_range
        && !range.contains(entry.response.status)
    {
        return false;
    }
    if let Some(ref url_sub) = opts.url
        && !entry.request.url.contains(url_sub.as_str())
    {
        return false;
    }
    if let Some(ref re) = opts.url_regex
        && !re.is_match(&entry.request.url)
    {
        return false;
    }
    if let Some(ref hf) = opts.header
        && !has_header(entry, hf)
    {
        return false;
    }
    if let Some(ref mime) = opts.mime {
        let entry_mime = entry.response.content.mime_type.as_deref().unwrap_or("");
        if !entry_mime
            .to_ascii_lowercase()
            .contains(&mime.to_ascii_lowercase())
        {
            return false;
        }
    }
    if let Some(min) = opts.min_time
        && entry.time < min
    {
        return false;
    }
    if let Some(ref pat) = opts.body_grep
        && !body_contains(entry, pat)
    {
        return false;
    }
    true
}

fn body_contains(entry: &Entry, pat: &str) -> bool {
    if let Some(resp_text) = entry.response.content.text.as_deref()
        && resp_text.contains(pat)
    {
        return true;
    }
    if let Some(post_data) = &entry.request.post_data
        && let Some(req_text) = post_data.text.as_deref()
        && req_text.contains(pat)
    {
        return true;
    }
    false
}

fn has_header(entry: &Entry, hf: &HeaderFilter) -> bool {
    entry
        .request
        .headers
        .iter()
        .chain(entry.response.headers.iter())
        .any(|h| {
            if !h.name.eq_ignore_ascii_case(&hf.name) {
                return false;
            }
            match &hf.value {
                Some(val) => h.value.contains(val.as_str()),
                None => true,
            }
        })
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

    // --- StatusRange::from_str ---

    #[test]
    fn status_range_shorthand() {
        let r = StatusRange::from_str("4xx").unwrap();
        assert_eq!(r.start, 400);
        assert_eq!(r.end, 499);
    }

    #[test]
    fn status_range_numeric() {
        let r = StatusRange::from_str("200-299").unwrap();
        assert_eq!(r.start, 200);
        assert_eq!(r.end, 299);
    }

    #[test]
    fn status_range_rejects_reversed() {
        let err = StatusRange::from_str("500-200").unwrap_err();
        assert!(err.contains("start"));
    }

    #[test]
    fn status_range_rejects_bad_shorthand() {
        assert!(StatusRange::from_str("0xx").is_err());
        assert!(StatusRange::from_str("9xx").is_err());
        assert!(StatusRange::from_str("xxx").is_err());
    }

    #[test]
    fn status_range_rejects_nonsense() {
        assert!(StatusRange::from_str("").is_err());
        assert!(StatusRange::from_str("abc").is_err());
        assert!(StatusRange::from_str("200-").is_err());
        assert!(StatusRange::from_str("-299").is_err());
        assert!(StatusRange::from_str("100").is_err());
    }

    // --- HeaderFilter::from_str ---

    #[test]
    fn header_filter_name_and_value() {
        let h = HeaderFilter::from_str("Authorization:Bearer foo").unwrap();
        assert_eq!(h.name, "Authorization");
        assert_eq!(h.value.as_deref(), Some("Bearer foo"));
    }

    #[test]
    fn header_filter_name_only() {
        let h = HeaderFilter::from_str("X-Custom").unwrap();
        assert_eq!(h.name, "X-Custom");
        assert_eq!(h.value, None);
    }

    #[test]
    fn header_filter_rejects_empty_input() {
        assert!(HeaderFilter::from_str("").is_err());
    }

    #[test]
    fn header_filter_rejects_empty_name() {
        assert!(HeaderFilter::from_str(":value").is_err());
    }

    // --- filter_entries ---

    #[test]
    fn test_no_filters_returns_all() {
        let entries = load_entries();
        let result = filter_entries(entries, &FilterOptions::default());
        assert_eq!(result.len(), 4);
    }

    #[test]
    fn test_filter_by_method() {
        let entries = load_entries();
        let opts = FilterOptions {
            method: Some("POST".to_string()),
            ..Default::default()
        };
        let result = filter_entries(entries, &opts);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].1.request.method, "POST");
    }

    #[test]
    fn test_filter_by_method_is_case_insensitive() {
        let entries = load_entries();
        let opts = FilterOptions {
            method: Some("post".to_string()),
            ..Default::default()
        };
        let result = filter_entries(entries, &opts);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_filter_by_status() {
        let entries = load_entries();
        let opts = FilterOptions {
            status: Some(404),
            ..Default::default()
        };
        let result = filter_entries(entries, &opts);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].1.response.status, 404);
    }

    #[test]
    fn test_filter_by_status_range_shorthand() {
        let entries = load_entries();
        let opts = FilterOptions {
            status_range: Some(StatusRange::from_str("4xx").unwrap()),
            ..Default::default()
        };
        let result = filter_entries(entries, &opts);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].1.response.status, 404);
    }

    #[test]
    fn test_filter_by_status_range_numeric() {
        let entries = load_entries();
        let opts = FilterOptions {
            status_range: Some(StatusRange::from_str("200-299").unwrap()),
            ..Default::default()
        };
        let result = filter_entries(entries, &opts);
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_filter_by_url_substring() {
        let entries = load_entries();
        let opts = FilterOptions {
            url: Some("/users".to_string()),
            ..Default::default()
        };
        let result = filter_entries(entries, &opts);
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_filter_by_url_regex() {
        let entries = load_entries();
        let opts = FilterOptions {
            url_regex: Some(Regex::new(r"/users/\d+").unwrap()),
            ..Default::default()
        };
        let result = filter_entries(entries, &opts);
        assert_eq!(result.len(), 1);
        assert!(result[0].1.request.url.contains("/users/999"));
    }

    #[test]
    fn test_filter_by_header_name_and_value() {
        let entries = load_entries();
        let opts = FilterOptions {
            header: Some(HeaderFilter::from_str("Authorization:Bearer token123").unwrap()),
            ..Default::default()
        };
        let result = filter_entries(entries, &opts);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].1.request.method, "POST");
    }

    #[test]
    fn test_filter_by_header_presence_only() {
        let entries = load_entries();
        let opts = FilterOptions {
            header: Some(HeaderFilter::from_str("Authorization").unwrap()),
            ..Default::default()
        };
        let result = filter_entries(entries, &opts);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].1.request.method, "POST");
    }

    #[test]
    fn test_filter_by_header_is_case_insensitive() {
        let entries = load_entries();
        let opts = FilterOptions {
            header: Some(HeaderFilter::from_str("authorization").unwrap()),
            ..Default::default()
        };
        let result = filter_entries(entries, &opts);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_filter_by_mime() {
        let entries = load_entries();
        let opts = FilterOptions {
            mime: Some("image/png".to_string()),
            ..Default::default()
        };
        let result = filter_entries(entries, &opts);
        assert_eq!(result.len(), 1);
        assert!(result[0].1.request.url.contains("image.png"));
    }

    #[test]
    fn test_filter_by_mime_is_case_insensitive() {
        let entries = load_entries();
        let opts = FilterOptions {
            mime: Some("IMAGE/PNG".to_string()),
            ..Default::default()
        };
        let result = filter_entries(entries, &opts);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_filter_by_mime_is_substring() {
        // Simulates real-world HARs where the same logical type appears with
        // and without charset suffixes. `--mime application/json` should
        // match both `application/json` and `application/json; charset=utf-8`.
        use crate::har::{Content, Request, Response, Timings};

        fn entry_with_mime(mime: &str) -> Entry {
            Entry {
                started_date_time: "2026-01-15T10:00:00.000Z".to_string(),
                time: 1.0,
                request: Request {
                    method: "GET".to_string(),
                    url: "https://example.com/".to_string(),
                    http_version: "HTTP/1.1".to_string(),
                    headers: vec![],
                    query_string: vec![],
                    headers_size: -1,
                    body_size: -1,
                    post_data: None,
                },
                response: Response {
                    status: 200,
                    status_text: "OK".to_string(),
                    http_version: "HTTP/1.1".to_string(),
                    headers: vec![],
                    content: Content {
                        size: 0,
                        mime_type: Some(mime.to_string()),
                        text: None,
                    },
                    redirect_url: String::new(),
                    headers_size: -1,
                    body_size: 0,
                },
                timings: Timings {
                    send: 0.0,
                    wait: 0.0,
                    receive: 0.0,
                },
                cache: None,
            }
        }

        let entries = vec![
            entry_with_mime("application/json"),
            entry_with_mime("application/json; charset=utf-8"),
            entry_with_mime("application/json; charset=UTF-8"),
            entry_with_mime("text/html"),
        ];
        let opts = FilterOptions {
            mime: Some("application/json".to_string()),
            ..Default::default()
        };
        let result = filter_entries(entries, &opts);
        assert_eq!(
            result.len(),
            3,
            "substring match should hit all three json variants"
        );
    }

    #[test]
    fn test_filter_by_min_time() {
        let entries = load_entries();
        let opts = FilterOptions {
            min_time: Some(100.0),
            ..Default::default()
        };
        let result = filter_entries(entries, &opts);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_combined_filters_and_logic() {
        let entries = load_entries();
        let opts = FilterOptions {
            method: Some("GET".to_string()),
            status: Some(200),
            ..Default::default()
        };
        let result = filter_entries(entries, &opts);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_no_matches_returns_empty() {
        let entries = load_entries();
        let opts = FilterOptions {
            status: Some(500),
            ..Default::default()
        };
        let result = filter_entries(entries, &opts);
        assert!(result.is_empty());
    }
}
