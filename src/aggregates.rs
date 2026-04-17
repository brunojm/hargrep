//! Standalone aggregate views — focused answers that would otherwise require
//! an agent to synthesize across multiple `hargrep` calls.
//!
//! Each view emits a single JSON document: an array of aggregate rows or, for
//! `--redirects`, a flat list of 3xx entries with their Location headers.
//! Respects the filter pipeline so you can scope a view with any of the
//! existing filter flags.

use crate::har::Entry;
use crate::overview::extract_host;
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// `--domains`: [{domain, count}] sorted by count desc, then domain asc.
pub fn domains(entries: &[(usize, Entry)]) -> Value {
    let mut counts: BTreeMap<String, u64> = BTreeMap::new();
    for (_, entry) in entries {
        if let Some(host) = extract_host(&entry.request.url) {
            *counts.entry(host).or_insert(0) += 1;
        }
    }
    let mut rows: Vec<(String, u64)> = counts.into_iter().collect();
    rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    Value::Array(
        rows.into_iter()
            .map(|(domain, count)| json!({ "domain": domain, "count": count }))
            .collect(),
    )
}

/// `--size-by-type`: [{mime_type, total_bytes, count}] sorted by total_bytes desc.
/// Uses the full MIME string (including charset) so `application/json` and
/// `application/json; charset=utf-8` are separate rows — matches how the HAR
/// actually labelled them. Agents that want to collapse variants can do so.
pub fn size_by_type(entries: &[(usize, Entry)]) -> Value {
    let mut by_mime: BTreeMap<String, (i64, u64)> = BTreeMap::new();
    for (_, entry) in entries {
        let mime = entry
            .response
            .content
            .mime_type
            .as_deref()
            .unwrap_or("unknown");
        let size = entry.response.content.size.max(0);
        let cell = by_mime.entry(mime.to_string()).or_insert((0, 0));
        cell.0 += size;
        cell.1 += 1;
    }
    let mut rows: Vec<(String, i64, u64)> = by_mime
        .into_iter()
        .map(|(mime, (bytes, count))| (mime, bytes, count))
        .collect();
    rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    Value::Array(
        rows.into_iter()
            .map(|(mime_type, total_bytes, count)| {
                json!({ "mime_type": mime_type, "total_bytes": total_bytes, "count": count })
            })
            .collect(),
    )
}

/// `--largest-bodies N`: top-N entries by response body size, descending.
/// Each row: {id, url, mime_type, content_size}. Answers "which URL returned
/// the largest body?" without forcing the agent to extract `content.size`
/// from every entry and sort client-side.
///
/// Entries whose HAR logger recorded `content.size = -1` (unknown) sort to
/// the bottom of the desc order — they're the smallest signed value. Sort is
/// stable, so among equal-size rows the original HAR order is preserved.
/// `limit = 0` is accepted and yields an empty array.
pub fn largest_bodies(entries: &[(usize, Entry)], limit: usize) -> Value {
    let mut rows: Vec<&(usize, Entry)> = entries.iter().collect();
    rows.sort_by_key(|row| std::cmp::Reverse(row.1.response.content.size));
    rows.truncate(limit);
    Value::Array(
        rows.into_iter()
            .map(|(id, entry)| {
                json!({
                    "id": id,
                    "url": entry.request.url,
                    "mime_type": entry.response.content.mime_type.clone().unwrap_or_default(),
                    "content_size": entry.response.content.size,
                })
            })
            .collect(),
    )
}

/// `--redirects`: flat list of 3xx entries with their Location header.
/// Each row: {id, url, status, location}. Chain reconstruction is left to the
/// caller — the raw pairs are enough information and the format stays simple.
pub fn redirects(entries: &[(usize, Entry)]) -> Value {
    let mut rows = Vec::new();
    for (id, entry) in entries {
        let status = entry.response.status;
        if (300..400).contains(&status) {
            let location = find_location_header(entry).unwrap_or_default();
            rows.push(json!({
                "id": id,
                "url": entry.request.url,
                "status": status,
                "location": location,
            }));
        }
    }
    Value::Array(rows)
}

fn find_location_header(entry: &Entry) -> Option<String> {
    entry
        .response
        .headers
        .iter()
        .find(|h| h.name.eq_ignore_ascii_case("location"))
        .map(|h| h.value.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::har::{Content, Entry, Header, Request, Response, Timings};

    fn make_entry(method: &str, url: &str, status: u16, mime: &str, body_size: i64) -> Entry {
        Entry {
            started_date_time: "2026-01-15T10:00:00.000Z".to_string(),
            time: 10.0,
            request: Request {
                method: method.to_string(),
                url: url.to_string(),
                http_version: "HTTP/1.1".to_string(),
                headers: vec![],
                query_string: vec![],
                headers_size: -1,
                body_size: -1,
                post_data: None,
            },
            response: Response {
                status,
                status_text: String::new(),
                http_version: "HTTP/1.1".to_string(),
                headers: vec![],
                content: Content {
                    size: body_size,
                    mime_type: Some(mime.to_string()),
                    text: None,
                },
                redirect_url: String::new(),
                headers_size: -1,
                body_size: 0,
            },
            timings: Timings {
                send: 0.0,
                wait: 10.0,
                receive: 0.0,
            },
            cache: None,
        }
    }

    fn with_location(mut entry: Entry, location: &str) -> Entry {
        entry.response.headers.push(Header {
            name: "Location".to_string(),
            value: location.to_string(),
        });
        entry
    }

    fn indexed(entries: Vec<Entry>) -> Vec<(usize, Entry)> {
        entries.into_iter().enumerate().collect()
    }

    #[test]
    fn domains_counts_and_sorts() {
        let rows = domains(&indexed(vec![
            make_entry("GET", "https://a.example/x", 200, "application/json", 10),
            make_entry("GET", "https://a.example/y", 200, "application/json", 10),
            make_entry("GET", "https://b.example/z", 200, "application/json", 10),
        ]));
        let arr = rows.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["domain"], "a.example");
        assert_eq!(arr[0]["count"], 2);
        assert_eq!(arr[1]["domain"], "b.example");
        assert_eq!(arr[1]["count"], 1);
    }

    #[test]
    fn size_by_type_sums_and_sorts() {
        let rows = size_by_type(&indexed(vec![
            make_entry("GET", "u", 200, "image/png", 1000),
            make_entry("GET", "u", 200, "application/json", 50),
            make_entry("GET", "u", 200, "application/json", 150),
        ]));
        let arr = rows.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["mime_type"], "image/png");
        assert_eq!(arr[0]["total_bytes"], 1000);
        assert_eq!(arr[0]["count"], 1);
        assert_eq!(arr[1]["mime_type"], "application/json");
        assert_eq!(arr[1]["total_bytes"], 200);
        assert_eq!(arr[1]["count"], 2);
    }

    #[test]
    fn size_by_type_treats_unknown_mime_as_unknown_bucket() {
        let mut e = make_entry("GET", "u", 200, "application/json", 10);
        e.response.content.mime_type = None;
        let rows = size_by_type(&indexed(vec![e]));
        let arr = rows.as_array().unwrap();
        assert_eq!(arr[0]["mime_type"], "unknown");
    }

    #[test]
    fn size_by_type_treats_negative_sizes_as_zero() {
        let rows = size_by_type(&indexed(vec![make_entry(
            "GET",
            "u",
            200,
            "application/json",
            -1,
        )]));
        assert_eq!(rows.as_array().unwrap()[0]["total_bytes"], 0);
    }

    #[test]
    fn largest_bodies_sorts_desc_by_content_size() {
        let rows = largest_bodies(
            &indexed(vec![
                make_entry("GET", "https://x/a", 200, "application/json", 50),
                make_entry("GET", "https://x/b", 200, "image/png", 5000),
                make_entry("GET", "https://x/c", 200, "text/html", 800),
            ]),
            10,
        );
        let arr = rows.as_array().unwrap();
        assert_eq!(arr.len(), 3);
        // The 5000-byte PNG wins; id references the original HAR index (1 here).
        assert_eq!(arr[0]["id"], 1);
        assert_eq!(arr[0]["url"], "https://x/b");
        assert_eq!(arr[0]["content_size"], 5000);
        assert_eq!(arr[0]["mime_type"], "image/png");
        assert_eq!(arr[1]["content_size"], 800);
        assert_eq!(arr[2]["content_size"], 50);
    }

    #[test]
    fn largest_bodies_truncates_to_limit() {
        let entries = (0..20)
            .map(|i| make_entry("GET", "u", 200, "application/json", (i * 10) as i64))
            .collect::<Vec<_>>();
        let rows = largest_bodies(&indexed(entries), 3);
        let arr = rows.as_array().unwrap();
        assert_eq!(arr.len(), 3);
        // Top three by size, descending: 190, 180, 170.
        assert_eq!(arr[0]["content_size"], 190);
        assert_eq!(arr[1]["content_size"], 180);
        assert_eq!(arr[2]["content_size"], 170);
    }

    #[test]
    fn largest_bodies_limit_zero_yields_empty_array() {
        let rows = largest_bodies(
            &indexed(vec![make_entry("GET", "u", 200, "application/json", 100)]),
            0,
        );
        assert!(rows.as_array().unwrap().is_empty());
    }

    #[test]
    fn largest_bodies_sinks_unknown_size_entries() {
        // HAR records content.size = -1 when the logger didn't measure it.
        // Desc sort treats -1 as smaller than real sizes, so these sort last.
        let rows = largest_bodies(
            &indexed(vec![
                make_entry("GET", "https://x/a", 200, "application/json", -1),
                make_entry("GET", "https://x/b", 200, "image/png", 2000),
                make_entry("GET", "https://x/c", 200, "application/json", -1),
                make_entry("GET", "https://x/d", 200, "text/html", 100),
            ]),
            10,
        );
        let arr = rows.as_array().unwrap();
        assert_eq!(arr[0]["content_size"], 2000);
        assert_eq!(arr[1]["content_size"], 100);
        // Both -1 rows come last; stable sort preserves their original order
        // so a comes before c.
        assert_eq!(arr[2]["content_size"], -1);
        assert_eq!(arr[2]["id"], 0);
        assert_eq!(arr[3]["content_size"], -1);
        assert_eq!(arr[3]["id"], 2);
    }

    #[test]
    fn redirects_only_includes_3xx() {
        let entries = vec![
            make_entry("GET", "https://x/home", 200, "text/html", 0),
            with_location(
                make_entry("GET", "https://x/", 301, "text/html", 0),
                "https://x/home",
            ),
            with_location(
                make_entry("GET", "https://x/old", 302, "text/html", 0),
                "https://x/new",
            ),
            make_entry("GET", "https://x/y", 404, "text/html", 0),
        ];
        let rows = redirects(&indexed(entries));
        let arr = rows.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["status"], 301);
        assert_eq!(arr[0]["id"], 1);
        assert_eq!(arr[0]["location"], "https://x/home");
        assert_eq!(arr[1]["status"], 302);
    }

    #[test]
    fn redirects_handles_missing_location_header() {
        let rows = redirects(&indexed(vec![make_entry(
            "GET",
            "https://x/",
            301,
            "text/html",
            0,
        )]));
        let arr = rows.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["location"], "");
    }
}
