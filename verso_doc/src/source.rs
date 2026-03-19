use crate::ast::Document;
use std::fmt;

#[derive(Debug)]
pub struct ParseDocError {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for ParseDocError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for ParseDocError {}

/// Resolve `!include` and `use` directives by reading and inlining included files.
/// `!include` inlines the entire file; `use` inlines only declarations (var, def, func).
/// Detects circular includes. Paths are relative to `base_dir`.
pub fn resolve_includes(
    src: &str,
    base_dir: &std::path::Path,
    seen: &mut Vec<std::path::PathBuf>,
) -> Result<String, ParseDocError> {
    let mut out = String::new();
    for (i, line) in src.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("!include") {
            let path_str = trimmed["!include".len()..].trim();
            if path_str.is_empty() {
                return Err(ParseDocError {
                    line: i + 1,
                    message: "!include requires a file path".into(),
                });
            }
            let resolved = resolve_file(path_str, base_dir, seen, i, false)?;
            out.push_str(&resolved);
            out.push('\n');
        } else if trimmed == "use" || trimmed.starts_with("use ") {
            let path_str = trimmed.strip_prefix("use").unwrap().trim();
            if path_str.is_empty() {
                return Err(ParseDocError {
                    line: i + 1,
                    message: "use requires a file path, e.g. use notation.verso".into(),
                });
            }
            let resolved = resolve_file(path_str, base_dir, seen, i, true)?;
            out.push_str(&resolved);
            out.push('\n');
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    Ok(out)
}

/// Collect all file dependencies for a `.verso` file (the file itself + all includes, recursively).
pub fn collect_dependencies(
    path: &std::path::Path,
) -> Result<Vec<std::path::PathBuf>, ParseDocError> {
    let src = std::fs::read_to_string(path).map_err(|e| ParseDocError {
        line: 0,
        message: format!("cannot read '{}': {}", path.display(), e),
    })?;
    let base_dir = path.parent().unwrap_or(std::path::Path::new("."));
    let mut seen = vec![path.canonicalize().map_err(|e| ParseDocError {
        line: 0,
        message: format!("cannot resolve '{}': {}", path.display(), e),
    })?];
    let _ = resolve_includes(&src, base_dir, &mut seen)?;
    Ok(seen)
}

/// Parse an `.verso` file, resolving `!include` directives.
pub fn parse_document_from_file(path: &std::path::Path) -> Result<Document, ParseDocError> {
    let src = std::fs::read_to_string(path).map_err(|e| ParseDocError {
        line: 0,
        message: format!("cannot read '{}': {}", path.display(), e),
    })?;
    let base_dir = path.parent().unwrap_or(std::path::Path::new("."));
    let mut seen = vec![path.canonicalize().map_err(|e| ParseDocError {
        line: 0,
        message: format!("cannot resolve '{}': {}", path.display(), e),
    })?];
    let resolved = resolve_includes(&src, base_dir, &mut seen)?;
    crate::parse::parse_document(&resolved)
}

/// Read and resolve a file for `!include` or `use`.
/// If `symbols_only` is true, only declaration lines (var, def, func) and their
/// indented body lines are extracted.
fn resolve_file(
    path_str: &str,
    base_dir: &std::path::Path,
    seen: &mut Vec<std::path::PathBuf>,
    line_idx: usize,
    symbols_only: bool,
) -> Result<String, ParseDocError> {
    let directive = if symbols_only { "use" } else { "!include" };
    let path = base_dir.join(path_str);
    let canonical = path.canonicalize().map_err(|e| ParseDocError {
        line: line_idx + 1,
        message: format!("{} '{}': {}", directive, path_str, e),
    })?;
    if seen.contains(&canonical) {
        return Err(ParseDocError {
            line: line_idx + 1,
            message: format!("{} '{}': circular include detected", directive, path_str),
        });
    }
    seen.push(canonical.clone());
    let content = std::fs::read_to_string(&canonical).map_err(|e| ParseDocError {
        line: line_idx + 1,
        message: format!("{} '{}': {}", directive, path_str, e),
    })?;
    let child_dir = canonical.parent().unwrap_or(base_dir);
    let resolved = resolve_includes(&content, child_dir, seen)?;

    if symbols_only {
        Ok(extract_declarations(&resolved))
    } else {
        Ok(resolved)
    }
}

/// Extract only declaration lines (var, def, func) and their indented body lines
/// from a resolved document source.
fn extract_declarations(src: &str) -> String {
    let lines: Vec<&str> = src.lines().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim();
        let is_decl = trimmed.starts_with("var ")
            || trimmed == "var"
            || trimmed.starts_with("def ")
            || trimmed == "def"
            || trimmed.starts_with("func ")
            || trimmed == "func";
        if is_decl {
            out.push_str(lines[i]);
            out.push('\n');
            i += 1;
            while i < lines.len() && !lines[i].is_empty() && lines[i].starts_with(' ') {
                out.push_str(lines[i]);
                out.push('\n');
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    out
}
