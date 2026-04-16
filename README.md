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

Filters combine with AND logic.

### Output

| Flag | Description |
|------|-------------|
| `--output <FORMAT>` | `json` (default), `jsonl`, or `summary` |
| `--fields <FIELDS>` | Comma-separated. Valid names: `url`, `method`, `status`, `status-text`, `time`, `mime-type`, `started-date-time`. CLI names are kebab-case; emitted JSON keys preserve HAR camelCase (`statusText`, `mimeType`). Unknown names error at parse time. |
| `--count` | Print only the count of matching entries. Conflicts with `--fields`, `--no-body`, `--output`. |
| `--no-body` | Exclude request/response bodies |

### Utility

| Flag | Description |
|------|-------------|
| `--validate` | Validate HAR only, no query |
| `-v`, `--verbose` | Print parsing info to stderr |

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

# Validate before processing
hargrep --validate untrusted.har

# Pipe from stdin
cat recording.har | hargrep --method POST --status-range 5xx

# Find requests with a specific header
hargrep --header 'Authorization:Bearer' --fields url,status recording.har
```

## For LLM agents

`hargrep` is designed to fit into agent pipelines:

- **Predictable schema** — every output mode produces deterministic, well-formed JSON or compact text
- **`--fields`** — request only the columns you need so the output stays small
- **`--no-body`** — strip base64 images and large response bodies
- **`--count`** — check scope cheaply before committing context to a full query
- **`--jsonl`** — stream one entry per line, easy to chunk
- **Fails fast** — CLI arguments are validated before any file is read. Unknown `--fields` names, invalid `--status-range`, bad `--url-regex`, and conflicting flags (e.g. `--count --fields`) all error with exit code 2 and a descriptive message on stderr. Typos surface immediately instead of producing empty results.

Typical agent flow: validate → count → filter narrowly → read specific entries.

```bash
hargrep --validate recording.har                          # check it parses
hargrep --count --status-range 5xx recording.har          # probe the scope
hargrep --status-range 5xx --fields url,status,time \
  --output jsonl recording.har                            # pull just what's needed
```

## HAR format

Follows [HAR 1.2](http://www.softwareishard.com/blog/har-12-spec/). You can get a HAR file by:
- Chrome/Edge DevTools → Network → right-click → "Save all as HAR with content"
- Firefox DevTools → Network → gear icon → "Save All As HAR"
- Safari Web Inspector → Network → Export

## License

MIT — see [LICENSE](./LICENSE).
