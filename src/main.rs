mod filter;
mod har;
mod input;
mod output;

use anyhow::Result;
use clap::Parser;
use filter::{FilterOptions, HeaderFilter, StatusRange};
use output::{Field, OutputFormat, OutputMode};
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

    /// Filter by response MIME type (exact, case-insensitive)
    #[arg(long)]
    mime: Option<String>,

    /// Filter entries slower than N milliseconds
    #[arg(long)]
    min_time: Option<f64>,

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

    /// Exclude request/response bodies from output
    #[arg(long, conflicts_with = "count")]
    no_body: bool,

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

    let filter_opts = FilterOptions {
        method: cli.method,
        status: cli.status,
        status_range: cli.status_range,
        url: cli.url,
        url_regex: cli.url_regex,
        header: cli.header,
        mime: cli.mime,
        min_time: cli.min_time,
    };

    let filtered = filter::filter_entries(har.log.entries, &filter_opts);
    let exit_code = if filtered.is_empty() { 1 } else { 0 };

    let mode = if cli.count {
        OutputMode::Count
    } else {
        OutputMode::Formatted {
            format: cli.output,
            fields: cli.fields,
            no_body: cli.no_body,
        }
    };

    let output = output::format_output(&filtered, &mode)?;
    print!("{output}");

    Ok(exit_code)
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
