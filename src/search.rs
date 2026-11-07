//! Bounded, deterministic repository search. No external process dependency.
use anyhow::{Context, Result};
use ignore::WalkBuilder;
use regex::Regex;
use std::fs;
use std::path::Path;

const MAX_FILE_BYTES: u64 = 1024 * 1024;
const MAX_MATCHES: usize = 100;
const MAX_OUTPUT_BYTES: usize = 32 * 1024;

pub(super) fn grep(root: &Path, pattern: &str) -> Result<String> {
    let regex = Regex::new(pattern).context("invalid search regex")?;
    let canonical_root = root.canonicalize()?;
    let mut output = String::new();
    let mut count = 0;
    let mut truncated = false;
    // Sorting guarantees stable output for agent replay and regression tests.
    let mut walker = WalkBuilder::new(&canonical_root);
    walker.sort_by_file_path(|a, b| a.cmp(b));
    walker.hidden(false);
    for entry in walker.build().filter_map(std::result::Result::ok) {
        if !entry.file_type().is_some_and(|kind| kind.is_file()) {
            continue;
        }
        let path = entry.path();
        if !path
            .canonicalize()
            .is_ok_and(|resolved| resolved.starts_with(&canonical_root))
        {
            continue;
        }
        if !fs::metadata(path).is_ok_and(|metadata| metadata.len() <= MAX_FILE_BYTES) {
            continue;
        }
        let Ok(content) = fs::read_to_string(path) else {
            continue;
        };
        let relative = path.strip_prefix(&canonical_root)?.to_string_lossy();
        for (line_number, line) in content.lines().enumerate() {
            if !regex.is_match(line) {
                continue;
            }
            if count == MAX_MATCHES {
                truncated = true;
                break;
            }
            let snippet: String = line.chars().take(240).collect();
            let record = format!("{}:{}:{}\n", relative, line_number + 1, snippet);
            if output.len() + record.len() > MAX_OUTPUT_BYTES {
                truncated = true;
                break;
            }
            output.push_str(&record);
            count += 1;
        }
        if truncated {
            break;
        }
    }
    if truncated {
        output.push_str("[TRUNCATED: search result limit reached]\n");
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stable_relative_paths_and_line_numbers() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("b.txt"), "match\n").unwrap();
        fs::write(dir.path().join("a.txt"), "skip\nmatch\n").unwrap();
        let result = grep(dir.path(), "match").unwrap();
        assert_eq!(result, "a.txt:2:match\nb.txt:1:match\n");
    }
    #[test]
    fn invalid_regex_is_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(grep(dir.path(), "[").is_err());
    }
    #[test]
    fn limits_match_count() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("many.txt"), "yes\n".repeat(200)).unwrap();
        let result = grep(dir.path(), "yes").unwrap();
        assert_eq!(result.matches("many.txt:").count(), 100);
        assert!(result.contains("[TRUNCATED:"));
    }
    #[test]
    fn skips_binary_and_oversized_files() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("binary"), b"needle\xff").unwrap();
        fs::write(
            dir.path().join("large"),
            vec![b'x'; MAX_FILE_BYTES as usize + 1],
        )
        .unwrap();
        assert!(grep(dir.path(), "needle|xxx").unwrap().is_empty());
    }
}

#[cfg(test)]
mod search_regressions {
    use super::*;

    #[test]
    fn empty_matches_and_invalid_root() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("file.txt"), "hello\n").unwrap();
        assert!(grep(dir.path(), "missing").unwrap().is_empty());
        assert!(grep(&dir.path().join("missing"), "hello").is_err());
    }

    #[test]
    fn line_snippets_are_bounded() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("long.txt"),
            format!("key{}\n", "x".repeat(500)),
        )
        .unwrap();
        let result = grep(dir.path(), "key").unwrap();
        let snippet = result.splitn(3, ':').nth(2).unwrap();
        assert_eq!(snippet.trim_end().chars().count(), 240);
    }

    #[test]
    fn respects_gitignore_and_returns_sorted_results() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(".gitignore"), "ignored.txt\n").unwrap();
        fs::write(dir.path().join("ignored.txt"), "needle\n").unwrap();
        fs::write(dir.path().join("visible.txt"), "needle\n").unwrap();
        let result = grep(dir.path(), "needle").unwrap();
        assert!(result.contains("visible.txt:1:needle"));
        assert!(!result.contains("ignored.txt:"));
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_outside_workspace_are_not_searched() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("secret.txt"), "needle\n").unwrap();
        symlink(
            outside.path().join("secret.txt"),
            root.path().join("link.txt"),
        )
        .unwrap();
        assert!(!grep(root.path(), "needle").unwrap().contains("link.txt"));
    }
}
