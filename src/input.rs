use anyhow::{Context, Result};
use std::io::Read;
use std::path::Path;

/// Read HAR JSON from a file path, or from stdin if `file` is `None`.
/// Rejects empty input with a clear message so callers don't have to debug
/// opaque "EOF while parsing" errors from the JSON layer.
pub fn read_input(file: Option<&Path>) -> Result<String> {
    match file {
        Some(path) => {
            let content = std::fs::read_to_string(path)
                .with_context(|| format!("failed to read file '{}'", path.display()))?;
            if content.is_empty() {
                anyhow::bail!("file '{}' is empty", path.display());
            }
            Ok(content)
        }
        None => {
            let mut buf = String::new();
            std::io::stdin()
                .read_to_string(&mut buf)
                .context("failed to read from stdin")?;
            if buf.is_empty() {
                anyhow::bail!("stdin was empty (did you pipe a HAR file?)");
            }
            Ok(buf)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_from_file() {
        let result = read_input(Some(Path::new("tests/fixtures/valid.har")));
        let content = result.expect("reading valid fixture should succeed");
        assert!(content.contains("\"version\": \"1.2\""));
    }

    #[test]
    fn test_read_nonexistent_file() {
        let result = read_input(Some(Path::new("nonexistent.har")));
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_file_errors() {
        let tmp = std::env::temp_dir().join("hargrep_empty_test.har");
        std::fs::write(&tmp, "").unwrap();
        let result = read_input(Some(&tmp));
        let _ = std::fs::remove_file(&tmp);
        let err = result.expect_err("empty file should error");
        assert!(err.to_string().contains("empty"));
    }
}
