mod aggregates;
mod filter;
mod har;
mod input;
mod output;
mod overview;

use anyhow::Result;
use clap::Parser;
use filter::{FilterOptions, HeaderFilter, StatusRange};
use output::{BodyMode, Field, OutputFormat, OutputMode};
use regex::Regex;
use std::path::PathBuf;
use std::process;

/// A Unix-style CLI tool for querying HAR (HTTP Archive) files.
/// Filters entries by method, status, URL, headers, and more.
/// Outputs structured, machine-readable results.
#[derive(Parser)]
#[command(name = "hargrep", version, about)]
struct Cli {
    /// Filter by HTTP method (e.g. GET, POST). Case-insensitive.
    #[arg(long)]
    method: Option<String>,

    /// Filter by exact response status code
    #[arg(long)]
    status: Option<u16>,

    /// Filter by status range: "4xx" shorthand or "200-299" numeric
    #[arg(long)]
    status_range: Option<StatusRange>,

    /// Filter by URL substring
    #[arg(long)]
    url: Option<String>,

    /// Filter by URL regex pattern
    #[arg(long)]
    url_regex: Option<Regex>,

    /// Filter by header: "NAME:VALUE" (value is a substring match) or "NAME" for presence-only
    #[arg(long)]
    header: Option<HeaderFilter>,

    /// Filter by response MIME type (substring, case-insensitive)
    #[arg(long)]
    mime: Option<String>,

    /// Filter entries slower than N milliseconds
    #[arg(long)]
    min_time: Option<f64>,

    /// Filter by substring match against request or response body text.
    /// Matches when either contains the pattern. Case-sensitive.
    #[arg(long)]
    body_grep: Option<String>,

    /// Output format
    #[arg(long, value_enum, default_value_t = OutputFormat::Json, conflicts_with = "count")]
    output: OutputFormat,

    /// Comma-separated fields to include (e.g. "url,status,time").
    /// Valid: url, method, status, status-text, time, mime-type, started-date-time
    #[arg(long, value_enum, value_delimiter = ',', conflicts_with = "count")]
    fields: Vec<Field>,

    /// Print only the count of matching entries
    #[arg(long)]
    count: bool,

    /// Print a single-shot JSON dashboard of the filtered HAR: entry count,
    /// status/method/MIME histograms, top domains, total body size, total time.
    /// Replaces a cascade of exploratory queries with one call.
    #[arg(
        long,
        conflicts_with_all = ["count", "fields", "entry", "no_body", "include_all_bodies", "output", "domains", "size_by_type", "redirects"]
    )]
    overview: bool,

    /// List unique request domains with per-domain request counts, sorted desc.
    /// Respects filters.
    #[arg(
        long,
        conflicts_with_all = ["count", "fields", "entry", "no_body", "include_all_bodies", "output", "overview", "size_by_type", "redirects"]
    )]
    domains: bool,

    /// Breakdown of response body size by MIME type: [{mime_type, total_bytes, count}]
    /// sorted by total_bytes desc. Respects filters.
    #[arg(
        long,
        conflicts_with_all = ["count", "fields", "entry", "no_body", "include_all_bodies", "output", "overview", "domains", "redirects"]
    )]
    size_by_type: bool,

    /// List 3xx entries with their Location header: [{id, url, status, location}].
    /// Respects filters.
    #[arg(
        long,
        conflicts_with_all = ["count", "fields", "entry", "no_body", "include_all_bodies", "output", "overview", "domains", "size_by_type"]
    )]
    redirects: bool,

    /// Fetch a single entry by id (the original 0-indexed position in the HAR).
    /// Returns a JSON object, not an array. Useful after listing entries with
    /// `--fields id,url,status` and then zeroing in on one. `--entry` is a
    /// direct lookup, not a filter operation — it conflicts with filter flags
    /// so an agent can't accidentally combine them and get surprising results.
    #[arg(
        long,
        conflicts_with_all = [
            "count", "fields", "output",
            "method", "status", "status_range", "url", "url_regex",
            "header", "mime", "min_time",
        ]
    )]
    entry: Option<usize>,

    /// Exclude request/response bodies from output
    #[arg(long, conflicts_with_all = ["count", "include_all_bodies"])]
    no_body: bool,

    /// Include bodies for static-asset MIME types (CSS/JS/images/fonts/WASM)
    /// that would otherwise be stripped by default. Use when you actually need
    /// to inspect an asset payload.
    #[arg(long, conflicts_with = "count")]
    include_all_bodies: bool,

    /// Validate HAR only, don't query
    #[arg(long)]
    validate: bool,

    /// Show parsing info on stderr
    #[arg(short, long)]
    verbose: bool,

    /// HAR file to read. Reads from stdin if omitted.
    file: Option<PathBuf>,
}

fn run(cli: Cli) -> Result<i32> {
    let raw = input::read_input(cli.file.as_deref())?;

    let har: har::Har = serde_json::from_str(&raw).map_err(|e| {
        let source = match &cli.file {
            Some(p) => format!("file '{}'", p.display()),
            None => "stdin".to_string(),
        };
        anyhow::anyhow!("failed to parse HAR from {source}: {e}")
    })?;

    if cli.verbose {
        eprintln!("hargrep: parsed {} entries", har.log.entries.len());
    }

    if cli.validate {
        if cli.verbose {
            eprintln!("hargrep: HAR is valid (version {})", har.log.version);
        }
        return Ok(0);
    }

    let body_mode = if cli.no_body {
        BodyMode::StripAll
    } else if cli.include_all_bodies {
        BodyMode::IncludeAll
    } else {
        BodyMode::SkipAssets
    };

    if let Some(id) = cli.entry {
        let total = har.log.entries.len();
        let entry = har.log.entries.into_iter().nth(id).ok_or_else(|| {
            anyhow::anyhow!("entry id {id} out of range (HAR has {total} entries)")
        })?;
        let output = output::format_single_entry(id, &entry, body_mode)?;
        print!("{output}");
        return Ok(0);
    }

    let filter_opts = FilterOptions {
        method: cli.method,
        status: cli.status,
        status_range: cli.status_range,
        url: cli.url,
        url_regex: cli.url_regex,
        header: cli.header,
        mime: cli.mime,
        min_time: cli.min_time,
        body_grep: cli.body_grep,
    };

    let filtered = filter::filter_entries(har.log.entries, &filter_opts);
    let exit_code = if filtered.is_empty() { 1 } else { 0 };

    // All aggregate views honor grep-like exit semantics: exit 1 when the
    // emitted document is empty. The document is still printed either way so
    // downstream tooling sees well-formed output.
    if cli.overview {
        let doc = overview::build_overview(&filtered);
        emit_json_doc(&doc)?;
        return Ok(aggregate_exit_code(&doc));
    }

    if cli.domains {
        let doc = aggregates::domains(&filtered);
        emit_json_doc(&doc)?;
        return Ok(aggregate_exit_code(&doc));
    }

    if cli.size_by_type {
        let doc = aggregates::size_by_type(&filtered);
        emit_json_doc(&doc)?;
        return Ok(aggregate_exit_code(&doc));
    }

    if cli.redirects {
        let doc = aggregates::redirects(&filtered);
        emit_json_doc(&doc)?;
        return Ok(aggregate_exit_code(&doc));
    }

    let mode = if cli.count {
        OutputMode::Count
    } else {
        OutputMode::Formatted {
            format: cli.output,
            fields: cli.fields,
            body: body_mode,
        }
    };

    let output = output::format_output(&filtered, &mode)?;
    print!("{output}");

    Ok(exit_code)
}

/// Exit 1 when the aggregate document has nothing to report, 0 otherwise.
/// Array documents (`--domains`, `--size-by-type`, `--redirects`) are empty
/// when the array has no rows. The overview object is empty when its
/// `entries` count is zero.
fn aggregate_exit_code(doc: &serde_json::Value) -> i32 {
    let is_empty = match doc {
        serde_json::Value::Array(rows) => rows.is_empty(),
        serde_json::Value::Object(_) => doc.get("entries").and_then(|v| v.as_u64()) == Some(0),
        _ => false,
    };
    if is_empty { 1 } else { 0 }
}

fn emit_json_doc(value: &serde_json::Value) -> Result<()> {
    let serialized = if std::io::IsTerminal::is_terminal(&std::io::stdout()) {
        serde_json::to_string_pretty(value)?
    } else {
        serde_json::to_string(value)?
    };
    println!("{serialized}");
    Ok(())
}

fn main() {
    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => process::exit(code),
        Err(e) => {
            eprintln!("hargrep: {e}");
            process::exit(2);
        }
    }
}
