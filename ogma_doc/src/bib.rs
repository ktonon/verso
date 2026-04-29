//! Minimal BibTeX entry parser used to power editor completions for `cite`
//! tags. Only extracts the entry key and (optionally) the title — that is
//! everything the LSP needs to surface useful suggestions.

use crate::ast::{Block, Document};
use std::path::{Path, PathBuf};

/// A single BibTeX entry as far as we care about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BibEntry {
    pub key: String,
    pub title: Option<String>,
}

/// Extract the entry keys (and optional titles) from a `.bib` file.
///
/// Recognises lines like `@article{key,` or `@book{key, title = {...}}`.
/// Comments (`%`) and `@preamble`/`@string`/`@comment` blocks are skipped.
pub fn parse_bib(text: &str) -> Vec<BibEntry> {
    let mut entries = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'@' {
            i += 1;
            continue;
        }
        // Read entry type after '@'.
        let type_start = i + 1;
        let mut j = type_start;
        while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
            j += 1;
        }
        let entry_type = text[type_start..j].to_ascii_lowercase();
        // Skip non-entry blocks.
        if matches!(entry_type.as_str(), "preamble" | "string" | "comment" | "") {
            i = j;
            continue;
        }
        // Skip whitespace, then expect '{' or '('.
        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        if j >= bytes.len() || (bytes[j] != b'{' && bytes[j] != b'(') {
            i = j.max(i + 1);
            continue;
        }
        let open = bytes[j];
        let close = if open == b'{' { b'}' } else { b')' };
        j += 1;
        // Read the key up to the first comma (or closing brace if no fields).
        let key_start = j;
        while j < bytes.len() && bytes[j] != b',' && bytes[j] != close {
            j += 1;
        }
        let key = text[key_start..j].trim().to_string();
        if key.is_empty() {
            i = j.max(i + 1);
            continue;
        }
        // Walk to the matching close brace, tracking nesting, and grab the
        // title field if we see it. The body may contain nested braces.
        let body_start = j;
        let mut depth = 1;
        let mut k = j;
        while k < bytes.len() && depth > 0 {
            match bytes[k] {
                b if b == open => depth += 1,
                b if b == close => depth -= 1,
                _ => {}
            }
            k += 1;
        }
        let body_end = if depth == 0 { k - 1 } else { bytes.len() };
        let body = &text[body_start..body_end];
        let title = extract_title(body);
        entries.push(BibEntry { key, title });
        i = k;
    }
    entries
}

/// Pull the `title = {...}` or `title = "..."` field out of an entry body.
/// Returns the inner text with surrounding braces/quotes stripped and
/// whitespace collapsed; returns None if no title is present.
fn extract_title(body: &str) -> Option<String> {
    let lower = body.to_ascii_lowercase();
    let mut start = 0;
    let title = loop {
        let idx = lower[start..].find("title")?;
        let pos = start + idx;
        // Ensure 'title' is a standalone field name (preceded by a delimiter).
        let before_ok = pos == 0
            || matches!(
                body.as_bytes()[pos - 1],
                b',' | b'{' | b' ' | b'\t' | b'\n'
            );
        let after = pos + "title".len();
        if before_ok {
            // Skip whitespace, expect '='.
            let mut k = after;
            while k < body.len() && body.as_bytes()[k].is_ascii_whitespace() {
                k += 1;
            }
            if k < body.len() && body.as_bytes()[k] == b'=' {
                break Some(k + 1);
            }
        }
        start = pos + 1;
    }?;
    let bytes = body.as_bytes();
    let mut k = title;
    while k < bytes.len() && bytes[k].is_ascii_whitespace() {
        k += 1;
    }
    if k >= bytes.len() {
        return None;
    }
    let (open, close) = match bytes[k] {
        b'{' => (b'{', b'}'),
        b'"' => (b'"', b'"'),
        _ => return None,
    };
    k += 1;
    let value_start = k;
    let mut depth = 1;
    while k < bytes.len() && depth > 0 {
        if open == b'{' {
            if bytes[k] == open {
                depth += 1;
            } else if bytes[k] == close {
                depth -= 1;
            }
        } else if bytes[k] == close {
            depth -= 1;
        }
        if depth > 0 {
            k += 1;
        }
    }
    let raw = &body[value_start..k];
    let cleaned: String = raw
        .replace(['{', '}'], "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

/// Resolve every `!bibliography` path declared in `doc` against `base_dir`,
/// load the files, and return the union of their entries.
pub fn collect_bib_entries(doc: &Document, base_dir: &Path) -> Vec<BibEntry> {
    let mut out = Vec::new();
    for block in &doc.blocks {
        if let Block::Bibliography { path, .. } = block {
            let resolved: PathBuf = if Path::new(path).is_absolute() {
                PathBuf::from(path)
            } else {
                base_dir.join(path)
            };
            if let Ok(text) = std::fs::read_to_string(&resolved) {
                out.extend(parse_bib(&text));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_entry_with_title() {
        let src = r#"
            @article{einstein1905,
              author = "A. Einstein",
              title  = {On the electrodynamics of moving bodies},
              year   = 1905
            }
        "#;
        let entries = parse_bib(src);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].key, "einstein1905");
        assert_eq!(
            entries[0].title.as_deref(),
            Some("On the electrodynamics of moving bodies")
        );
    }

    #[test]
    fn parses_multiple_entries() {
        let src = r#"
            @book{knuth1984, title = {The TeXbook}}
            @article{lamport1986, title = "LaTeX"}
        "#;
        let entries = parse_bib(src);
        let keys: Vec<&str> = entries.iter().map(|e| e.key.as_str()).collect();
        assert_eq!(keys, vec!["knuth1984", "lamport1986"]);
    }

    #[test]
    fn skips_preamble_and_string() {
        let src = r#"
            @preamble{ "foo" }
            @string{aw = "Addison-Wesley"}
            @article{key1, title = {A}}
        "#;
        let entries = parse_bib(src);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].key, "key1");
    }

    #[test]
    fn handles_entry_without_title() {
        let src = "@misc{just_a_key}";
        let entries = parse_bib(src);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].key, "just_a_key");
        assert!(entries[0].title.is_none());
    }

    #[test]
    fn handles_nested_braces_in_title() {
        let src = r#"@article{k, title = {The {LaTeX} Book}}"#;
        let entries = parse_bib(src);
        assert_eq!(entries[0].title.as_deref(), Some("The LaTeX Book"));
    }
}
