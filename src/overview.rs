//! Single-shot HAR overview for LLM agents.
//!
//! `hargrep --overview <file>` replaces a sequence of small exploratory queries
//! (count, domains, methods, status ranges, size) with one compact JSON
//! document. Cheap to produce, cheap to read.

use crate::har::Entry;
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

/// Maximum number of domains to list in the `top_domains` section. An agent
/// drilling into long-tail traffic should filter by `--url` rather than walk a
/// giant histogram.
const TOP_DOMAINS_LIMIT: usize = 10;

/// Build the overview document for the given (already-filtered) entries.
pub fn build_overview(entries: &[(usize, Entry)]) -> Value {
    let mut status_buckets = [0u64; 6]; // 1xx..5xx indexed by leading digit; slot 0 unused
    let mut methods: BTreeMap<String, u64> = BTreeMap::new();
    let mut mime_types: BTreeMap<String, u64> = BTreeMap::new();
    let mut domains: BTreeMap<String, u64> = BTreeMap::new();
    let mut total_body_size: i64 = 0;
    let mut total_time_ms: f64 = 0.0;

    for (_, entry) in entries {
        let status_digit = (entry.response.status / 100) as usize;
        if status_digit < status_buckets.len() {
            status_buckets[status_digit] += 1;
        }

        *methods.entry(entry.request.method.clone()).or_insert(0) += 1;

        if let Some(mime) = &entry.response.content.mime_type {
            let normalized = mime
                .split(';')
                .next()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase();
            if !normalized.is_empty() {
                *mime_types.entry(normalized).or_insert(0) += 1;
            }
        }

        if let Some(host) = extract_host(&entry.request.url) {
            *domains.entry(host).or_insert(0) += 1;
        }

        // `content.size` can be -1 in HAR when unknown; treat those as 0.
        if entry.response.content.size > 0 {
            total_body_size += entry.response.content.size;
        }
        total_time_ms += entry.time;
    }

    let mut status = Map::new();
    for (i, bucket_name) in ["", "1xx", "2xx", "3xx", "4xx", "5xx"].iter().enumerate() {
        if i == 0 {
            continue;
        }
        if status_buckets[i] > 0 {
            status.insert((*bucket_name).to_string(), json!(status_buckets[i]));
        }
    }

    let mut top_domains: Vec<(String, u64)> = domains.into_iter().collect();
    top_domains.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    top_domains.truncate(TOP_DOMAINS_LIMIT);
    let top_domains_value: Vec<Value> = top_domains
        .into_iter()
        .map(|(host, count)| json!({ "domain": host, "count": count }))
        .collect();

    json!({
        "entries": entries.len(),
        "status": Value::Object(status),
        "methods": Value::Object(methods_to_map(methods)),
        "mime_types": Value::Object(methods_to_map(mime_types)),
        "top_domains": top_domains_value,
        "total_body_size_bytes": total_body_size,
        "total_time_ms": total_time_ms,
    })
}

fn methods_to_map(bt: BTreeMap<String, u64>) -> Map<String, Value> {
    let mut m = Map::new();
    for (k, v) in bt {
        m.insert(k, json!(v));
    }
    m
}

/// Extract the host portion of a URL without pulling in a full URL parser.
/// Tolerates missing schemes and malformed input — returns None rather than
/// erroring, so one weird URL doesn't break the whole overview.
fn extract_host(url: &str) -> Option<String> {
    let after_scheme = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    let host = after_scheme
        .split(['/', '?', '#'])
        .next()?
        .split('@')
        .next_back()?;
    if host.is_empty() {
        return None;
    }
    Some(host.split(':').next().unwrap_or(host).to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::har::Har;

    fn load_entries() -> Vec<(usize, Entry)> {
        let json = include_str!("../tests/fixtures/valid.har");
        let har: Har = serde_json::from_str(json).unwrap();
        har.log.entries.into_iter().enumerate().collect()
    }

    #[test]
    fn overview_counts_entries() {
        let v = build_overview(&load_entries());
        assert_eq!(v["entries"], 4);
    }

    #[test]
    fn overview_status_buckets_present_only_for_nonzero() {
        let v = build_overview(&load_entries());
        assert_eq!(v["status"]["2xx"], 3);
        assert_eq!(v["status"]["4xx"], 1);
        assert!(v["status"].get("1xx").is_none());
        assert!(v["status"].get("3xx").is_none());
        assert!(v["status"].get("5xx").is_none());
    }

    #[test]
    fn overview_methods_and_mimes() {
        let v = build_overview(&load_entries());
        assert_eq!(v["methods"]["GET"], 3);
        assert_eq!(v["methods"]["POST"], 1);
        assert!(v["mime_types"]["application/json"].as_u64().unwrap() >= 1);
    }

    #[test]
    fn overview_top_domains_sorted_by_count() {
        let v = build_overview(&load_entries());
        let domains = v["top_domains"].as_array().unwrap();
        assert_eq!(domains.len(), 2);
        // valid.har: 3 requests to api.example.com, 1 to cdn.example.com.
        assert_eq!(domains[0]["domain"], "api.example.com");
        assert_eq!(domains[0]["count"], 3);
        assert_eq!(domains[1]["domain"], "cdn.example.com");
        assert_eq!(domains[1]["count"], 1);
    }

    #[test]
    fn overview_total_time_and_body_size_sum() {
        let v = build_overview(&load_entries());
        let time = v["total_time_ms"].as_f64().unwrap();
        assert!(time > 0.0);
        let size = v["total_body_size_bytes"].as_i64().unwrap();
        assert!(size > 0);
    }

    #[test]
    fn extract_host_handles_common_shapes() {
        assert_eq!(
            extract_host("https://api.example.com/path"),
            Some("api.example.com".to_string())
        );
        assert_eq!(
            extract_host("https://api.example.com:8443/path"),
            Some("api.example.com".to_string())
        );
        assert_eq!(
            extract_host("http://user:pass@host.example.com/x"),
            Some("host.example.com".to_string())
        );
        assert_eq!(
            extract_host("api.example.com/path"),
            Some("api.example.com".to_string())
        );
        assert_eq!(extract_host(""), None);
    }
}
