# hargrep

[![CI](https://github.com/brunojm/hargrep/actions/workflows/ci.yml/badge.svg)](https://github.com/brunojm/hargrep/actions/workflows/ci.yml)

A Unix-style CLI tool for querying HAR (HTTP Archive) files. Like `grep` or `jq`, but purpose-built for HAR. Validates input, filters entries by flags, and outputs structured, machine-readable results optimized for LLM code agents.

## Why

HAR files are noisy. A single page load can produce hundreds of requests and dozens of megabytes of JSON — most of it base64-encoded images, third-party beacons, and padding you don't care about. `hargrep` lets you slice through that noise with simple flags, and produces compact output you can pipe into other tools or feed to an LLM without blowing the context window.

## Installation

From [crates.io](https://crates.io/crates/hargrep) (requires [Rust](https://www.rust-lang.org/tools/install)):

```bash
cargo install hargrep
```

This builds and installs the `hargrep` binary into `~/.cargo/bin` (make sure that's on your `PATH`).

### From source

Clone the repo and install from the checkout:

```bash
git clone https://github.com/brunojm/hargrep.git
cd hargrep
cargo install --path .
```

Or build a release binary without installing:

```bash
cargo build --release
# binary at ./target/release/hargrep
```

## Usage

```
hargrep [FLAGS] [FILE]
```

Reads from stdin if no file is given.

### Filtering

| Flag | Example |
|------|---------|
| `--method <METHOD>` | `--method POST` |
| `--status <CODE>` | `--status 200` |
| `--status-range <RANGE>` | `--status-range 4xx` or `--status-range 200-299` |
| `--url <PATTERN>` | `--url '/api/auth'` |
| `--url-regex <REGEX>` | `--url-regex '/users/\d+'` |
| `--header <NAME:VALUE>` | `--header 'Authorization:Bearer'` |
| `--mime <SUBSTRING>` | `--mime application/json` (matches `application/json; charset=utf-8` too) |
| `--min-time <MS>` | `--min-time 500` |
| `--body-grep <SUBSTRING>` | Match against request or response body text (case-sensitive). |
| `--body-regex <REGEX>` | Regex match against request or response body text. Use `(?i)pattern` for case-insensitive. |

Filters combine with AND logic.

### Output

| Flag | Description |
|------|-------------|
| `--output <FORMAT>` | `json` (default, pretty in a TTY, compact when piped), `jsonl`, or `summary`. |
| `--fields <FIELDS>` | Comma-separated. Valid names: `id`, `url`, `method`, `status`, `status-text`, `time`, `mime-type`, `started-date-time`. CLI names are kebab-case; emitted JSON keys preserve HAR camelCase (`statusText`, `mimeType`). Unknown names error at parse time. |
| `--count` | Print only the count of matching entries. Conflicts with `--fields`, `--no-body`, `--output`. |
| `--overview` | Print a single JSON dashboard of the filtered HAR: entry count, status/method/MIME histograms, top 10 domains, total body size, total time. Replaces a cascade of exploratory queries with one call. |
| `--domains` | Emit `[{domain, count}]` sorted by count desc. Respects filters. |
| `--size-by-type` | Emit `[{mime_type, total_bytes, count}]` sorted by total_bytes desc. Respects filters. |
| `--redirects` | Emit `[{id, url, status, location}]` for every 3xx entry. Respects filters. |
| `--entry <N>` | Fetch a single entry by id (its original 0-indexed position in the HAR). Returns a JSON object, not an array. |
| `--no-body` | Exclude all request/response body text. |
| `--include-all-bodies` | Include bodies for static-asset MIME types (CSS/JS/images/fonts/WASM). By default those are stripped to save tokens. |

Every output entry includes an `id` field — the entry's original 0-indexed position in the HAR. IDs are stable across filter changes, so you can list matches with `--fields id,url,status` and then drill into a specific one with `--entry N`.

Static-asset response bodies (images, fonts, CSS, JS, WASM, video, audio) are stripped by default, since they dominate HAR size but rarely help debug API behaviour. Use `--include-all-bodies` to keep them, or `--no-body` to strip everything.

### Utility

| Flag | Description |
|------|-------------|
| `--validate` | Validate HAR only, no query |
| `-v`, `--verbose` | Print parsing info to stderr |
| `--help-llm` | Print a compact, LLM-tuned cheatsheet of every flag (~1.5 KB vs ~3.5 KB for `--help`) and exit. |

### Exit codes

- `0` — matches found
- `1` — no matches (like `grep`)
- `2` — error (invalid HAR, bad arguments, IO failure)

## Examples

```bash
# Find all failed requests
hargrep --status-range 4xx recording.har

# Slow API calls, compact per-line JSON
hargrep --url '/api/' --min-time 500 --output jsonl app.har

# Count POST requests
hargrep --method POST --count session.har

# LLM-friendly: just URLs, statuses, timings, no bodies
hargrep --fields url,status,time --no-body --output jsonl recording.har

# One-shot overview of a HAR: entry count, histograms, top domains, totals
hargrep --overview recording.har

# Narrow with filters, list IDs, then fetch one entry in full
hargrep --status-range 5xx --fields id,url,status --output jsonl recording.har
hargrep --entry 42 recording.har

# Aggregate views — one call each
hargrep --domains recording.har                           # which hosts?
hargrep --size-by-type recording.har                      # where's the bandwidth going?
hargrep --redirects recording.har                         # all 3xx + Location headers

# Body search that actually knows about HAR schema
hargrep --body-grep 'session expired' --fields id,url,status recording.har

# Validate before processing
hargrep --validate untrusted.har

# Pipe from stdin
cat recording.har | hargrep --method POST --status-range 5xx

# Find requests with a specific header
hargrep --header 'Authorization:Bearer' --fields url,status recording.har
```

## For LLM agents

`hargrep` is designed to fit into agent pipelines:

- **Predictable schema** — every output mode produces deterministic, well-formed JSON or compact text.
- **Stable entry IDs** — every entry includes an `id` field (its original HAR index). List matches cheaply, then fetch specific entries with `--entry N`.
- **`--overview`** — one call returns a dashboard of the (optionally filtered) HAR. Replaces several exploratory queries.
- **`--fields`** — request only the columns you need so the output stays small.
- **Asset bodies stripped by default** — CSS/JS/images/fonts/WASM response bodies are dropped automatically since they dominate HAR size. `--include-all-bodies` disables this; `--no-body` strips everything.
- **`--count`** — check scope cheaply before committing context to a full query.
- **`--output jsonl`** — stream one entry per line, easy to chunk. Default JSON is compact when piped and pretty in a TTY.
- **Fails fast** — CLI arguments are validated before any file is read. Unknown `--fields` names, invalid `--status-range`, bad `--url-regex`, and conflicting flags (e.g. `--count --fields`) all error with exit code 2 and a descriptive message on stderr. Typos surface immediately instead of producing empty results.

Typical agent flow: overview → filter → fetch specific entries.

```bash
hargrep --overview recording.har                          # shape + scope in one call
hargrep --status-range 5xx --fields id,url,status \
  --output jsonl recording.har                            # list candidates
hargrep --entry 42 recording.har                          # pull the full entry for one id
```

## HAR format

Follows [HAR 1.2](http://www.softwareishard.com/blog/har-12-spec/). You can get a HAR file by:
- Chrome/Edge DevTools → Network → right-click → "Save all as HAR with content"
- Firefox DevTools → Network → gear icon → "Save All As HAR"
- Safari Web Inspector → Network → Export

## License

MIT — see [LICENSE](./LICENSE).
