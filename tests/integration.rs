use std::process::Command;

fn hargrep(args: &[&str]) -> (String, String, i32) {
    let output = Command::new(env!("CARGO_BIN_EXE_hargrep"))
        .args(args)
        .output()
        .expect("Failed to run hargrep");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let code = output.status.code().unwrap_or(-1);
    (stdout, stderr, code)
}

// --- Happy paths ---

#[test]
fn test_no_args_reads_all_entries() {
    let (stdout, _, code) = hargrep(&["tests/fixtures/valid.har"]);
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap();
    assert_eq!(parsed.len(), 4);
    assert_eq!(code, 0);
}

#[test]
fn test_method_filter() {
    let (stdout, _, code) = hargrep(&["--method", "POST", "tests/fixtures/valid.har"]);
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap();
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0]["request"]["method"], "POST");
    assert_eq!(code, 0);
}

#[test]
fn test_status_filter() {
    let (stdout, _, code) = hargrep(&["--status", "404", "tests/fixtures/valid.har"]);
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap();
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0]["response"]["status"], 404);
    assert_eq!(code, 0);
}

#[test]
fn test_status_range_filter() {
    let (stdout, _, _) = hargrep(&["--status-range", "2xx", "tests/fixtures/valid.har"]);
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap();
    assert_eq!(parsed.len(), 3);
}

#[test]
fn test_header_presence_only() {
    let (stdout, _, code) = hargrep(&["--header", "Authorization", "tests/fixtures/valid.har"]);
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap();
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0]["request"]["method"], "POST");
    assert_eq!(code, 0);
}

// --- Exit codes ---

#[test]
fn test_no_matches_exit_code_1() {
    let (stdout, _, code) = hargrep(&["--status", "500", "tests/fixtures/valid.har"]);
    assert_eq!(stdout.trim(), "[]");
    assert_eq!(code, 1);
}

#[test]
fn test_empty_har_exits_1() {
    let (stdout, _, code) = hargrep(&["tests/fixtures/minimal.har"]);
    assert_eq!(stdout.trim(), "[]");
    assert_eq!(code, 1);
}

// --- Validation ---

#[test]
fn test_validate_valid_har() {
    let (_, _, code) = hargrep(&["--validate", "tests/fixtures/valid.har"]);
    assert_eq!(code, 0);
}

#[test]
fn test_validate_malformed_har() {
    let (_, stderr, code) = hargrep(&["--validate", "tests/fixtures/malformed.har"]);
    assert_eq!(code, 2);
    assert!(
        stderr.to_lowercase().contains("parse"),
        "expected parse error in stderr, got: {stderr}"
    );
}

#[test]
fn test_parse_error_includes_source() {
    let (_, stderr, code) = hargrep(&["tests/fixtures/malformed.har"]);
    assert_eq!(code, 2);
    assert!(
        stderr.contains("malformed.har"),
        "expected file path in error message, got: {stderr}"
    );
}

// --- Output formats ---

#[test]
fn test_count_output() {
    let (stdout, _, _) = hargrep(&["--count", "tests/fixtures/valid.har"]);
    assert_eq!(stdout.trim(), "4");
}

#[test]
fn test_count_with_filter() {
    let (stdout, _, _) = hargrep(&["--count", "--method", "GET", "tests/fixtures/valid.har"]);
    assert_eq!(stdout.trim(), "3");
}

#[test]
fn test_summary_output() {
    let (stdout, _, _) = hargrep(&["--output", "summary", "tests/fixtures/valid.har"]);
    let lines: Vec<&str> = stdout.trim().lines().collect();
    assert_eq!(lines.len(), 4);
    assert!(lines[0].contains("GET"));
    assert!(lines[0].contains("200"));
}

#[test]
fn test_jsonl_output() {
    let (stdout, _, _) = hargrep(&["--output", "jsonl", "tests/fixtures/valid.har"]);
    let lines: Vec<&str> = stdout.trim().lines().collect();
    assert_eq!(lines.len(), 4);
    for line in &lines {
        let _: serde_json::Value = serde_json::from_str(line).unwrap();
    }
}

#[test]
fn test_fields_selection() {
    let (stdout, _, _) = hargrep(&["--fields", "url,status,time", "tests/fixtures/valid.har"]);
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap();
    let first = &parsed[0];
    assert!(first.get("url").is_some());
    assert!(first.get("status").is_some());
    assert!(first.get("time").is_some());
    assert!(first.get("request").is_none());
}

#[test]
fn test_fields_kebab_case_names() {
    let (stdout, _, _) = hargrep(&[
        "--fields",
        "status-text,mime-type",
        "tests/fixtures/valid.har",
    ]);
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap();
    let first = &parsed[0];
    // JSON keys keep HAR camelCase even though CLI uses kebab-case.
    assert_eq!(first["statusText"], "OK");
    assert_eq!(first["mimeType"], "application/json");
}

#[test]
fn test_no_body_strips_response_and_request_bodies() {
    let (stdout, _, _) = hargrep(&["--no-body", "tests/fixtures/valid.har"]);
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap();
    for entry in &parsed {
        assert!(entry["response"]["content"].get("text").is_none());
    }
    // The POST entry (index 1) has postData — text must be stripped, object preserved.
    let post_entry = &parsed[1];
    assert!(post_entry["request"]["postData"].is_object());
    assert!(post_entry["request"]["postData"].get("text").is_none());
}

#[test]
fn test_combined_filters() {
    let (stdout, _, _) = hargrep(&[
        "--method",
        "GET",
        "--mime",
        "application/json",
        "tests/fixtures/valid.har",
    ]);
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap();
    assert_eq!(parsed.len(), 1);
    assert!(
        parsed[0]["request"]["url"]
            .as_str()
            .unwrap()
            .contains("/users")
    );
}

// --- I/O error paths ---

#[test]
fn test_nonexistent_file() {
    let (_, stderr, code) = hargrep(&["nonexistent.har"]);
    assert_eq!(code, 2);
    assert!(stderr.to_lowercase().contains("failed to read"));
}

#[test]
fn test_empty_file_errors() {
    let tmp = std::env::temp_dir().join("hargrep_integration_empty.har");
    std::fs::write(&tmp, "").unwrap();
    let path = tmp.to_string_lossy().to_string();
    let (_, stderr, code) = hargrep(&[&path]);
    let _ = std::fs::remove_file(&tmp);
    assert_eq!(code, 2);
    assert!(stderr.contains("empty"));
}

#[test]
fn test_verbose_flag() {
    let (_, stderr, _) = hargrep(&["-v", "tests/fixtures/valid.har"]);
    assert!(stderr.contains("parsed 4 entries"));
}

#[test]
fn test_stdin_input() {
    let output = Command::new(env!("CARGO_BIN_EXE_hargrep"))
        .arg("--count")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            let stdin = child.stdin.as_mut().unwrap();
            let har_content = std::fs::read("tests/fixtures/valid.har").unwrap();
            stdin.write_all(&har_content).unwrap();
            drop(child.stdin.take());
            child.wait_with_output()
        })
        .expect("Failed to run hargrep with stdin");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert_eq!(stdout.trim(), "4");
}

// --- --overview dashboard ---

#[test]
fn test_overview_emits_json_object_with_expected_shape() {
    let (stdout, _, code) = hargrep(&["--overview", "tests/fixtures/valid.har"]);
    assert_eq!(code, 0);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(parsed.is_object());
    assert_eq!(parsed["entries"], 4);
    assert!(parsed["status"].is_object());
    assert!(parsed["methods"].is_object());
    assert!(parsed["mime_types"].is_object());
    assert!(parsed["top_domains"].is_array());
    assert!(parsed["total_body_size_bytes"].is_number());
    assert!(parsed["total_time_ms"].is_number());
}

#[test]
fn test_overview_status_histogram_uses_xx_buckets() {
    let (stdout, _, _) = hargrep(&["--overview", "tests/fixtures/valid.har"]);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    // valid.har has 3 2xx and 1 4xx
    assert_eq!(parsed["status"]["2xx"], 3);
    assert_eq!(parsed["status"]["4xx"], 1);
}

#[test]
fn test_overview_methods_histogram() {
    let (stdout, _, _) = hargrep(&["--overview", "tests/fixtures/valid.har"]);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(parsed["methods"]["GET"], 3);
    assert_eq!(parsed["methods"]["POST"], 1);
}

#[test]
fn test_overview_respects_filter() {
    // With --method GET, the overview should only reflect GETs.
    let (stdout, _, _) = hargrep(&["--overview", "--method", "GET", "tests/fixtures/valid.har"]);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(parsed["entries"], 3);
    assert_eq!(parsed["methods"]["GET"], 3);
    assert!(parsed["methods"].get("POST").is_none());
}

#[test]
fn test_overview_conflicts_with_count() {
    let (_, _, code) = hargrep(&["--overview", "--count", "tests/fixtures/valid.har"]);
    assert_eq!(code, 2);
}

#[test]
fn test_overview_conflicts_with_fields() {
    let (_, _, code) = hargrep(&["--overview", "--fields", "url", "tests/fixtures/valid.har"]);
    assert_eq!(code, 2);
}

#[test]
fn test_overview_conflicts_with_entry() {
    let (_, _, code) = hargrep(&["--overview", "--entry", "0", "tests/fixtures/valid.har"]);
    assert_eq!(code, 2);
}

// --- TTY-aware compact default JSON ---

#[test]
fn test_json_output_is_compact_when_piped() {
    // When hargrep's stdout is a pipe (like `.output()` captures), the default
    // json format should be compact (single line, no indentation).
    let (stdout, _, _) = hargrep(&["tests/fixtures/valid.har"]);
    let first_newline = stdout.find('\n').unwrap_or(stdout.len());
    let first_line = &stdout[..first_newline];
    let non_newline_count = stdout.chars().filter(|&c| c != '\n').count();
    // Pretty-printed JSON would have many newlines; compact has at most 1 trailing.
    assert!(
        stdout.matches('\n').count() <= 1,
        "compact JSON should have at most one trailing newline; got {} newlines.\nfirst line: {}",
        stdout.matches('\n').count(),
        first_line
    );
    // Should still be valid JSON
    let _: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap();
    // Smoke: output is non-empty
    assert!(non_newline_count > 100);
}

// --- Auto-skip static asset bodies ---

#[test]
fn test_asset_bodies_stripped_by_default() {
    let (stdout, _, _) = hargrep(&["tests/fixtures/valid.har"]);
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap();
    // Index 3 in valid.har is the image/png asset — its body text must be stripped by default.
    let image_entry = parsed.iter().find(|e| {
        e["response"]["content"]["mimeType"]
            .as_str()
            .is_some_and(|m| m.starts_with("image/"))
    });
    let entry = image_entry.expect("valid.har should have an image entry");
    assert!(
        entry["response"]["content"].get("text").is_none(),
        "asset bodies (image/*) should be stripped by default; got: {}",
        entry["response"]["content"]
    );
}

#[test]
fn test_non_asset_bodies_kept_by_default() {
    let (stdout, _, _) = hargrep(&["tests/fixtures/valid.har"]);
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap();
    // application/json bodies must remain. Find the /users GET response.
    let json_entry = parsed.iter().find(|e| {
        e["response"]["content"]["mimeType"]
            .as_str()
            .is_some_and(|m| m.starts_with("application/json"))
    });
    let entry = json_entry.expect("valid.har has a json entry");
    assert!(
        entry["response"]["content"].get("text").is_some(),
        "json bodies must be kept by default"
    );
}

#[test]
fn test_include_all_bodies_keeps_asset_bodies() {
    let (stdout, _, _) = hargrep(&["--include-all-bodies", "tests/fixtures/valid.har"]);
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap();
    let image_entry = parsed.iter().find(|e| {
        e["response"]["content"]["mimeType"]
            .as_str()
            .is_some_and(|m| m.starts_with("image/"))
    });
    let entry = image_entry.expect("valid.har should have an image entry");
    assert!(
        entry["response"]["content"].get("text").is_some(),
        "--include-all-bodies keeps asset bodies"
    );
}

#[test]
fn test_no_body_still_strips_everything() {
    // --no-body wins over --include-all-bodies semantics
    let (stdout, _, _) = hargrep(&["--no-body", "tests/fixtures/valid.har"]);
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap();
    for entry in &parsed {
        assert!(entry["response"]["content"].get("text").is_none());
    }
}

// --- Entry IDs + --entry N ---

#[test]
fn test_entries_include_id_field_in_json() {
    let (stdout, _, _) = hargrep(&["tests/fixtures/valid.har"]);
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap();
    // IDs are the original indices: 0, 1, 2, 3.
    for (i, entry) in parsed.iter().enumerate() {
        assert_eq!(entry["id"], i, "entry at position {i} should have id={i}");
    }
}

#[test]
fn test_entries_include_id_field_in_jsonl() {
    let (stdout, _, _) = hargrep(&["--output", "jsonl", "tests/fixtures/valid.har"]);
    for (i, line) in stdout.trim().lines().enumerate() {
        let entry: serde_json::Value = serde_json::from_str(line).unwrap();
        assert_eq!(entry["id"], i);
    }
}

#[test]
fn test_ids_are_stable_after_filter() {
    // Filter to 4xx only; original index for the 404 entry is 2 (index-2 in valid.har).
    let (stdout, _, _) = hargrep(&["--status", "404", "tests/fixtures/valid.har"]);
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap();
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0]["id"], 2);
}

#[test]
fn test_fields_can_include_id() {
    let (stdout, _, _) = hargrep(&["--fields", "id,url", "tests/fixtures/valid.har"]);
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&stdout).unwrap();
    for (i, entry) in parsed.iter().enumerate() {
        assert_eq!(entry["id"], i);
        assert!(entry.get("url").is_some());
        assert!(entry.get("request").is_none());
    }
}

#[test]
fn test_entry_flag_fetches_single_entry_by_id() {
    let (stdout, _, code) = hargrep(&["--entry", "1", "tests/fixtures/valid.har"]);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert!(
        parsed.is_object(),
        "--entry returns a single object, not array"
    );
    assert_eq!(parsed["id"], 1);
    assert_eq!(parsed["request"]["method"], "POST"); // index 1 is the POST
    assert_eq!(code, 0);
}

#[test]
fn test_entry_flag_out_of_range_errors() {
    let (_, stderr, code) = hargrep(&["--entry", "999", "tests/fixtures/valid.har"]);
    assert_eq!(code, 2);
    assert!(
        stderr.to_lowercase().contains("entry") || stderr.to_lowercase().contains("range"),
        "expected out-of-range error, got: {stderr}"
    );
}

#[test]
fn test_entry_flag_conflicts_with_count() {
    let (_, _, code) = hargrep(&["--entry", "0", "--count", "tests/fixtures/valid.har"]);
    assert_eq!(code, 2);
}

#[test]
fn test_entry_flag_conflicts_with_filter_flags() {
    // --entry is a direct lookup; combining with filters would silently ignore
    // the predicates and mislead automation.
    let cases: &[&[&str]] = &[
        &["--status", "500"],
        &["--method", "GET"],
        &["--url", "/users"],
        &["--status-range", "5xx"],
        &["--mime", "json"],
        &["--min-time", "100"],
        &["--header", "Authorization"],
    ];
    for filter_args in cases {
        let mut args = vec!["--entry", "0"];
        args.extend_from_slice(filter_args);
        args.push("tests/fixtures/valid.har");
        let (_, stderr, code) = hargrep(&args);
        assert_eq!(
            code, 2,
            "--entry with {filter_args:?} should exit 2; stderr: {stderr}"
        );
    }
}

#[test]
fn test_overview_exits_1_when_filter_produces_no_matches() {
    // Grep-like exit contract: empty result → exit 1.
    let (stdout, _, code) = hargrep(&["--overview", "--status", "999", "tests/fixtures/valid.har"]);
    assert_eq!(code, 1);
    // Body still emitted — empty overview is informative.
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(parsed["entries"], 0);
}

#[test]
fn test_overview_exits_0_when_there_are_matches() {
    let (_, _, code) = hargrep(&["--overview", "tests/fixtures/valid.har"]);
    assert_eq!(code, 0);
}

// --- CLI argument validation (parse-time errors) ---

#[test]
fn test_invalid_status_range_reversed() {
    let (_, stderr, code) = hargrep(&["--status-range", "500-200", "tests/fixtures/valid.har"]);
    assert_eq!(code, 2);
    assert!(
        stderr.to_lowercase().contains("status"),
        "expected status-range error, got: {stderr}"
    );
}

#[test]
fn test_invalid_status_range_nonsense() {
    let (_, _, code) = hargrep(&["--status-range", "abc", "tests/fixtures/valid.har"]);
    assert_eq!(code, 2);
}

#[test]
fn test_unknown_output_format() {
    let (_, stderr, code) = hargrep(&["--output", "xml", "tests/fixtures/valid.har"]);
    assert_eq!(code, 2);
    assert!(
        stderr.to_lowercase().contains("output"),
        "expected output error, got: {stderr}"
    );
}

#[test]
fn test_unknown_field_name() {
    let (_, stderr, code) = hargrep(&["--fields", "bogus", "tests/fixtures/valid.har"]);
    assert_eq!(code, 2);
    assert!(
        stderr.to_lowercase().contains("fields") || stderr.to_lowercase().contains("bogus"),
        "expected field error, got: {stderr}"
    );
}

#[test]
fn test_invalid_regex() {
    let (_, stderr, code) = hargrep(&["--url-regex", "[invalid", "tests/fixtures/valid.har"]);
    assert_eq!(code, 2);
    assert!(
        stderr.to_lowercase().contains("url-regex"),
        "expected url-regex error, got: {stderr}"
    );
}

#[test]
fn test_invalid_header_filter_empty_name() {
    let (_, _, code) = hargrep(&["--header", ":value", "tests/fixtures/valid.har"]);
    assert_eq!(code, 2);
}

#[test]
fn test_count_conflicts_with_fields() {
    let (_, stderr, code) = hargrep(&["--count", "--fields", "url", "tests/fixtures/valid.har"]);
    assert_eq!(code, 2);
    assert!(
        stderr.to_lowercase().contains("count") || stderr.to_lowercase().contains("cannot"),
        "expected conflict error, got: {stderr}"
    );
}

#[test]
fn test_count_conflicts_with_no_body() {
    let (_, _, code) = hargrep(&["--count", "--no-body", "tests/fixtures/valid.har"]);
    assert_eq!(code, 2);
}

#[test]
fn test_count_conflicts_with_output() {
    let (_, _, code) = hargrep(&["--count", "--output", "jsonl", "tests/fixtures/valid.har"]);
    assert_eq!(code, 2);
}
