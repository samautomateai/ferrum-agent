//! Optimistic, exact-match edits. This is not a filesystem security boundary.
use anyhow::{bail, Result};
use sha2::{Digest, Sha256};

pub(crate) const MAX_EDIT_BYTES: usize = 1024 * 1024;

pub(crate) fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub(crate) fn replace_once(
    original: &[u8],
    old: &str,
    new: &str,
    expected_sha256: Option<&str>,
) -> Result<Vec<u8>> {
    if original.len() > MAX_EDIT_BYTES {
        bail!("edit source exceeds 1 MiB limit");
    }
    if old.is_empty() {
        bail!("old text cannot be empty");
    }
    if let Some(expected) = expected_sha256 {
        if expected.len() != 64 || !expected.bytes().all(|b| b.is_ascii_hexdigit()) {
            bail!("expected_sha256 must be 64 hexadecimal characters");
        }
        if !digest(original).eq_ignore_ascii_case(expected) {
            bail!("edit conflict: content changed");
        }
    }
    let text = std::str::from_utf8(original)?;
    if text.matches(old).count() != 1 {
        bail!("edit requires exactly one match");
    }
    let updated = text.replacen(old, new, 1).into_bytes();
    if updated.len() > MAX_EDIT_BYTES {
        bail!("edited file exceeds 1 MiB limit");
    }
    if updated == original {
        bail!("edit makes no change");
    }
    Ok(updated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_one_match_preserving_crlf_and_surrounding_bytes() {
        let input = b"first\r\nold\r\nlast\r\n";
        let result = replace_once(input, "old", "new", Some(&digest(input))).unwrap();
        assert_eq!(result, b"first\r\nnew\r\nlast\r\n");
    }

    #[test]
    fn rejects_stale_digest() {
        assert!(replace_once(b"old", "old", "new", Some(&digest(b"previous"))).is_err());
    }

    #[test]
    fn rejects_ambiguous_or_noop_edits() {
        assert!(replace_once(b"old old", "old", "new", None).is_err());
        assert!(replace_once(b"old", "old", "old", None).is_err());
    }

    #[test]
    fn rejects_invalid_digests_and_oversized_input() {
        assert!(replace_once(b"old", "old", "new", Some("abc")).is_err());
        assert!(replace_once(&vec![b'x'; MAX_EDIT_BYTES + 1], "x", "y", None).is_err());
    }

    #[test]
    fn handles_unicode_without_normalizing() {
        assert_eq!(
            replace_once("café".as_bytes(), "fé", "fe", None).unwrap(),
            b"cafe"
        );
    }
}

#[cfg(test)]
mod boundary_regressions {
    use super::*;

    #[test]
    fn rejects_invalid_utf8_and_empty_search() {
        assert!(replace_once(&[0xff, 0xfe], "x", "y", None).is_err());
        assert!(replace_once(b"abc", "", "x", None).is_err());
    }

    #[test]
    fn accepts_case_insensitive_sha256_and_rejects_nonhex() {
        let checksum = digest(b"abc").to_uppercase();
        assert_eq!(
            replace_once(b"abc", "b", "B", Some(&checksum)).unwrap(),
            b"aBc"
        );
        assert!(replace_once(b"abc", "a", "b", Some(&"z".repeat(64))).is_err());
    }

    #[test]
    fn refuses_expansion_beyond_limit_and_missing_match() {
        let original = vec![b'a'; MAX_EDIT_BYTES];
        assert!(replace_once(&original, "a", "bb", None).is_err());
        assert!(replace_once(b"abc", "missing", "x", None).is_err());
    }

    #[test]
    fn accepts_exact_limit_without_modifying_other_bytes() {
        let original = format!("x{}", "a".repeat(MAX_EDIT_BYTES - 1));
        let result = replace_once(original.as_bytes(), "x", "y", None).unwrap();
        assert_eq!(result.len(), MAX_EDIT_BYTES);
        assert_eq!(result[0], b'y');
        assert_eq!(&result[1..], &original.as_bytes()[1..]);
    }
}
