use crate::ast::{
    Block, Claim, ColumnAlign, DimDecl, Document, EnvKind, Environment, Figure, List, ListItem,
    MathBlock, Proof, ProofStep, ProseFragment, Span, Table,
};
use crate::dim::Dimension;
use erd_symbolic::parse_expr;
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

/// Resolve `:include` directives by reading and inlining included files.
/// Detects circular includes. Paths are relative to `base_dir`.
pub fn resolve_includes(
    src: &str,
    base_dir: &std::path::Path,
    seen: &mut Vec<std::path::PathBuf>,
) -> Result<String, ParseDocError> {
    let mut out = String::new();
    for (i, line) in src.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with(":include") {
            let path_str = trimmed[":include".len()..].trim();
            if path_str.is_empty() {
                return Err(ParseDocError {
                    line: i + 1,
                    message: ":include requires a file path".into(),
                });
            }
            let path = base_dir.join(path_str);
            let canonical = path.canonicalize().map_err(|e| ParseDocError {
                line: i + 1,
                message: format!(":include '{}': {}", path_str, e),
            })?;
            if seen.contains(&canonical) {
                return Err(ParseDocError {
                    line: i + 1,
                    message: format!(":include '{}': circular include detected", path_str),
                });
            }
            seen.push(canonical.clone());
            let content = std::fs::read_to_string(&canonical).map_err(|e| ParseDocError {
                line: i + 1,
                message: format!(":include '{}': {}", path_str, e),
            })?;
            let child_dir = canonical.parent().unwrap_or(base_dir);
            let resolved = resolve_includes(&content, child_dir, seen)?;
            out.push_str(&resolved);
            out.push('\n');
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    Ok(out)
}

/// Parse an `.erd` file, resolving `:include` directives.
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
    parse_document(&resolved)
}

/// Parse an `.erd` document from source text.
pub fn parse_document(src: &str) -> Result<Document, ParseDocError> {
    let mut blocks = Vec::new();
    let lines: Vec<&str> = src.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        if trimmed.is_empty() {
            i += 1;
            continue;
        }

        // Comment lines: skip entirely
        if trimmed.starts_with('%') {
            i += 1;
            continue;
        }

        // Fenced math block
        if trimmed == "```math" {
            let mb = parse_math_block(&lines, &mut i)?;
            blocks.push(Block::MathBlock(mb));
            continue;
        }

        // Section heading
        if trimmed.starts_with('#') {
            let level = trimmed.chars().take_while(|&c| c == '#').count() as u8;
            let title = trimmed[level as usize..].trim().to_string();
            blocks.push(Block::Section {
                level,
                title,
                span: Span { line: i + 1 },
            });
            i += 1;
            continue;
        }

        // Table of contents
        if trimmed == ":toc" {
            blocks.push(Block::Toc);
            i += 1;
            continue;
        }

        // Page break
        if trimmed == ":pagebreak" {
            blocks.push(Block::PageBreak);
            i += 1;
            continue;
        }

        // Document metadata
        if trimmed.starts_with(":title") {
            let text = trimmed[":title".len()..].trim().to_string();
            if text.is_empty() {
                return Err(ParseDocError {
                    line: i + 1,
                    message: ":title requires text".into(),
                });
            }
            blocks.push(Block::Title(text));
            i += 1;
            continue;
        }
        if trimmed.starts_with(":author") {
            let text = trimmed[":author".len()..].trim().to_string();
            if text.is_empty() {
                return Err(ParseDocError {
                    line: i + 1,
                    message: ":author requires a name".into(),
                });
            }
            blocks.push(Block::Author(text));
            i += 1;
            continue;
        }
        if trimmed.starts_with(":date") {
            let text = trimmed[":date".len()..].trim().to_string();
            if text.is_empty() {
                return Err(ParseDocError {
                    line: i + 1,
                    message: ":date requires a value".into(),
                });
            }
            blocks.push(Block::Date(text));
            i += 1;
            continue;
        }
        if trimmed.starts_with(":abstract") {
            i += 1;
            let mut body = String::new();
            while i < lines.len() && is_continuation(&lines[i]) {
                if !body.is_empty() {
                    body.push(' ');
                }
                body.push_str(lines[i].trim());
                i += 1;
            }
            let fragments = if body.is_empty() {
                Vec::new()
            } else {
                parse_prose_fragments(&body)?
            };
            blocks.push(Block::Abstract(fragments));
            continue;
        }

        // Document class
        if trimmed.starts_with(":class") {
            let rest = trimmed[":class".len()..].trim();
            if rest.is_empty() {
                return Err(ParseDocError {
                    line: i + 1,
                    message: ":class requires a document class name".into(),
                });
            }
            let (name, options) = parse_name_with_options(rest);
            blocks.push(Block::DocumentClass { name, options });
            i += 1;
            continue;
        }
        // Use package
        if trimmed.starts_with(":usepackage") {
            let rest = trimmed[":usepackage".len()..].trim();
            if rest.is_empty() {
                return Err(ParseDocError {
                    line: i + 1,
                    message: ":usepackage requires a package name".into(),
                });
            }
            let (name, options) = parse_name_with_options(rest);
            blocks.push(Block::UsePackage { name, options });
            i += 1;
            continue;
        }

        // Figure block
        if trimmed.starts_with(":figure") {
            let path = trimmed[":figure".len()..].trim().to_string();
            if path.is_empty() {
                return Err(ParseDocError {
                    line: i + 1,
                    message: ":figure requires a file path".into(),
                });
            }
            let fig_line = i + 1;
            i += 1;
            let mut caption: Option<Vec<ProseFragment>> = None;
            let mut label: Option<String> = None;
            let mut width: f64 = 1.0;
            while i < lines.len() && is_continuation(&lines[i]) {
                let kv = lines[i].trim();
                if let Some(val) = kv.strip_prefix("caption:") {
                    caption = Some(parse_prose_fragments(val.trim())?);
                } else if let Some(val) = kv.strip_prefix("label:") {
                    label = Some(val.trim().to_string());
                } else if let Some(val) = kv.strip_prefix("width:") {
                    width = val.trim().parse::<f64>().map_err(|_| ParseDocError {
                        line: i + 1,
                        message: "invalid width value".into(),
                    })?;
                }
                i += 1;
            }
            blocks.push(Block::Figure(Figure {
                path,
                caption,
                label,
                width,
                span: Span { line: fig_line },
            }));
            continue;
        }

        // Table block
        if trimmed.starts_with(":table") {
            let title_text = trimmed[":table".len()..].trim();
            let title = if title_text.is_empty() { None } else { Some(title_text.to_string()) };
            let table_line = i + 1;
            i += 1;

            // Collect indented body lines
            let mut body_lines: Vec<String> = Vec::new();
            let mut label: Option<String> = None;
            while i < lines.len() && is_continuation(&lines[i]) {
                let l = lines[i].trim();
                if let Some(val) = l.strip_prefix("label:") {
                    label = Some(val.trim().to_string());
                } else {
                    body_lines.push(l.to_string());
                }
                i += 1;
            }

            // Need at least header + separator
            if body_lines.len() < 2 {
                return Err(ParseDocError {
                    line: table_line,
                    message: ":table requires header row and separator row".into(),
                });
            }

            // Parse header row
            let header = parse_table_row(&body_lines[0])?;
            let num_cols = header.len();

            // Parse separator row for alignment
            let sep = &body_lines[1];
            let sep_cells: Vec<&str> = sep.split('|')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();
            if sep_cells.len() != num_cols || !sep_cells.iter().all(|s| s.chars().all(|c| c == '-' || c == ':')) {
                return Err(ParseDocError {
                    line: table_line + 1,
                    message: ":table second row must be a separator (e.g. |---|---|)".into(),
                });
            }
            let columns: Vec<ColumnAlign> = sep_cells.iter().map(|s| {
                let left = s.starts_with(':');
                let right = s.ends_with(':');
                match (left, right) {
                    (true, true) => ColumnAlign::Center,
                    (false, true) => ColumnAlign::Right,
                    _ => ColumnAlign::Left,
                }
            }).collect();

            // Parse data rows
            let mut rows: Vec<Vec<Vec<ProseFragment>>> = Vec::new();
            for line in &body_lines[2..] {
                rows.push(parse_table_row(line)?);
            }

            blocks.push(Block::Table(Table {
                title,
                columns,
                header,
                rows,
                label,
                span: Span { line: table_line },
            }));
            continue;
        }

        // Claim block
        if trimmed.starts_with(":claim") {
            let name = trimmed[":claim".len()..].trim().to_string();
            if name.is_empty() {
                return Err(ParseDocError {
                    line: i + 1,
                    message: ":claim requires a name".into(),
                });
            }
            let claim_line = i + 1;

            // Collect indented body lines
            i += 1;
            let mut body = String::new();
            while i < lines.len() && is_continuation(&lines[i]) {
                if !body.is_empty() {
                    body.push(' ');
                }
                body.push_str(lines[i].trim());
                i += 1;
            }

            if body.is_empty() {
                return Err(ParseDocError {
                    line: claim_line,
                    message: ":claim body is empty".into(),
                });
            }

            let claim = parse_claim_body(&name, &body, claim_line)?;
            blocks.push(Block::Claim(claim));
            continue;
        }

        // Proof block
        if trimmed.starts_with(":proof") {
            let claim_name = trimmed[":proof".len()..].trim().to_string();
            if claim_name.is_empty() {
                return Err(ParseDocError {
                    line: i + 1,
                    message: ":proof requires a claim name".into(),
                });
            }
            let proof_line = i + 1;

            i += 1;
            let mut steps = Vec::new();
            while i < lines.len() && is_continuation(&lines[i]) {
                let step_line = i + 1;
                let step_text = lines[i].trim();

                // Skip empty continuation lines
                if step_text.is_empty() {
                    i += 1;
                    continue;
                }

                // Strip leading `=` from non-first steps
                let step_text = if !steps.is_empty() {
                    step_text.strip_prefix('=').map(|s| s.trim_start()).unwrap_or(step_text)
                } else {
                    step_text
                };

                let step = parse_proof_step(step_text, step_line)?;
                steps.push(step);
                i += 1;
            }

            if steps.len() < 2 {
                return Err(ParseDocError {
                    line: proof_line,
                    message: ":proof requires at least two steps".into(),
                });
            }

            blocks.push(Block::Proof(Proof {
                claim_name,
                steps,
                span: Span { line: proof_line },
            }));
            continue;
        }

        // Dimension declaration
        if trimmed.starts_with(":dim") {
            let rest = trimmed[":dim".len()..].trim();
            // Parse: varname [dim spec]
            let bracket_pos = rest.find('[').ok_or_else(|| ParseDocError {
                line: i + 1,
                message: ":dim requires a variable name and dimension, e.g. :dim x [L T^-1]"
                    .into(),
            })?;
            let var_name = rest[..bracket_pos].trim().to_string();
            if var_name.is_empty() {
                return Err(ParseDocError {
                    line: i + 1,
                    message: ":dim requires a variable name".into(),
                });
            }
            let dim_str = rest[bracket_pos..].trim();
            let dimension =
                Dimension::parse(dim_str).map_err(|e| ParseDocError {
                    line: i + 1,
                    message: format!(":dim '{}': {}", var_name, e),
                })?;
            blocks.push(Block::Dim(DimDecl {
                var_name,
                dimension,
                span: Span { line: i + 1 },
            }));
            i += 1;
            continue;
        }

        // Bibliography declaration
        if trimmed.starts_with(":bibliography") {
            let path = trimmed[":bibliography".len()..].trim().to_string();
            if path.is_empty() {
                return Err(ParseDocError {
                    line: i + 1,
                    message: ":bibliography requires a file path".into(),
                });
            }
            blocks.push(Block::Bibliography {
                path,
                span: Span { line: i + 1 },
            });
            i += 1;
            continue;
        }

        // Theorem-like environments
        if let Some(kind) = parse_env_kind(trimmed) {
            let env = parse_environment(kind, trimmed, &lines, &mut i)?;
            blocks.push(Block::Environment(env));
            continue;
        }

        // Block quote
        if trimmed.starts_with("> ") || trimmed == ">" {
            let mut quote_text = String::new();
            while i < lines.len() {
                let l = lines[i].trim();
                if l.starts_with("> ") {
                    if !quote_text.is_empty() {
                        quote_text.push(' ');
                    }
                    quote_text.push_str(&l[2..]);
                    i += 1;
                } else if l == ">" {
                    // Empty quote line treated as space
                    if !quote_text.is_empty() {
                        quote_text.push(' ');
                    }
                    i += 1;
                } else {
                    break;
                }
            }
            let fragments = if quote_text.is_empty() {
                Vec::new()
            } else {
                parse_prose_fragments(&quote_text)?
            };
            blocks.push(Block::BlockQuote(fragments));
            continue;
        }

        // List block
        if is_list_marker(trimmed) {
            let list = parse_list(&lines, &mut i)?;
            blocks.push(Block::List(list));
            continue;
        }

        // Prose line — collect consecutive non-special lines into a paragraph
        let mut prose_text = String::new();
        while i < lines.len() {
            let l = lines[i].trim();
            if l.is_empty()
                || l.starts_with('#')
                || l.starts_with(':')
                || l.starts_with('%')
                || l.starts_with("> ")
                || l == ">"
                || l == "```math"
                || is_list_marker(l)
            {
                break;
            }
            if !prose_text.is_empty() {
                prose_text.push(' ');
            }
            prose_text.push_str(l);
            i += 1;
        }
        if !prose_text.is_empty() {
            let fragments = parse_prose_fragments(&prose_text)?;
            blocks.push(Block::Prose(fragments));
        }
    }

    Ok(Document { blocks })
}

/// A continuation line is indented (starts with whitespace).
fn is_continuation(line: &str) -> bool {
    if line.trim().is_empty() {
        return false;
    }
    line.starts_with(' ') || line.starts_with('\t')
}

/// Check if a trimmed line starts with a list marker (`- ` or `N. `).
fn is_list_marker(trimmed: &str) -> bool {
    if trimmed.starts_with("- ") {
        return true;
    }
    is_ordered_marker(trimmed)
}

/// Check if a trimmed line starts with an ordered list marker (`N. `).
fn is_ordered_marker(trimmed: &str) -> bool {
    let mut chars = trimmed.chars();
    // Must start with a digit
    match chars.next() {
        Some(c) if c.is_ascii_digit() => {}
        _ => return false,
    }
    // Consume remaining digits
    loop {
        match chars.next() {
            Some(c) if c.is_ascii_digit() => continue,
            Some('.') => break,
            _ => return false,
        }
    }
    // Must be followed by a space
    matches!(chars.next(), Some(' '))
}

/// Strip a list marker from a line, returning the content after the marker.
fn strip_marker(trimmed: &str) -> &str {
    if trimmed.starts_with("- ") {
        return &trimmed[2..];
    }
    // Ordered: skip digits, dot, space
    if let Some(dot_pos) = trimmed.find(". ") {
        if trimmed[..dot_pos].chars().all(|c| c.is_ascii_digit()) {
            return &trimmed[dot_pos + 2..];
        }
    }
    trimmed
}

/// Compute the indentation level of a raw line (number of leading spaces).
fn indent_level(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// Parse a list starting at lines[*i]. Advances *i past the list.
fn parse_list(lines: &[&str], i: &mut usize) -> Result<List, ParseDocError> {
    let span = Span { line: *i + 1 };
    let base_indent = indent_level(lines[*i]);
    let first_trimmed = lines[*i].trim();
    let ordered = is_ordered_marker(first_trimmed);

    let mut items: Vec<ListItem> = Vec::new();

    while *i < lines.len() {
        let line = lines[*i];
        let trimmed = line.trim();

        // Stop on blank line, heading, directive, or outdented line
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(':') {
            break;
        }

        let line_indent = indent_level(line);

        // If indented deeper than base, this is a nested list
        if line_indent > base_indent && is_list_marker(trimmed) {
            // Attach as children to the last item
            if let Some(last) = items.last_mut() {
                let child_list = parse_list(lines, i)?;
                last.children = Some(child_list);
                continue;
            }
        }

        // If at our indent level with a marker, it's a new item
        if line_indent == base_indent && is_list_marker(trimmed) {
            let content = strip_marker(trimmed);
            let fragments = parse_prose_fragments(content)?;
            items.push(ListItem {
                fragments,
                children: None,
            });
            *i += 1;
            continue;
        }

        // Otherwise (outdented or not a list marker at base level), stop
        break;
    }

    Ok(List {
        ordered,
        items,
        span,
    })
}

/// Parse a fenced math block: ```math ... ```. Advances *i past the closing fence.
fn parse_math_block(lines: &[&str], i: &mut usize) -> Result<MathBlock, ParseDocError> {
    let span = Span { line: *i + 1 };
    *i += 1; // skip opening ```math

    let mut exprs = Vec::new();
    while *i < lines.len() {
        let trimmed = lines[*i].trim();
        if trimmed == "```" {
            *i += 1;
            return Ok(MathBlock { exprs, span });
        }
        if !trimmed.is_empty() {
            let expr = parse_expr(trimmed).map_err(|e| ParseDocError {
                line: *i + 1,
                message: format!("math block: {:?}", e),
            })?;
            exprs.push(expr);
        }
        *i += 1;
    }

    Err(ParseDocError {
        line: span.line,
        message: "unclosed ```math block".into(),
    })
}

/// Check if a trimmed line starts with a theorem-like directive.
fn parse_env_kind(trimmed: &str) -> Option<EnvKind> {
    let directives = [
        (":theorem", EnvKind::Theorem),
        (":lemma", EnvKind::Lemma),
        (":definition", EnvKind::Definition),
        (":corollary", EnvKind::Corollary),
        (":remark", EnvKind::Remark),
        (":example", EnvKind::Example),
    ];
    for (prefix, kind) in &directives {
        if trimmed.starts_with(prefix) {
            // Must be exactly the prefix or followed by whitespace
            let rest = &trimmed[prefix.len()..];
            if rest.is_empty() || rest.starts_with(' ') {
                return Some(*kind);
            }
        }
    }
    None
}

/// Parse a theorem-like environment block. Advances *i past the block.
fn parse_environment(
    kind: EnvKind,
    directive_line: &str,
    lines: &[&str],
    i: &mut usize,
) -> Result<Environment, ParseDocError> {
    let span = Span { line: *i + 1 };

    // Extract title (rest of directive line after the kind keyword)
    let prefix_len = match kind {
        EnvKind::Theorem => ":theorem",
        EnvKind::Lemma => ":lemma",
        EnvKind::Definition => ":definition",
        EnvKind::Corollary => ":corollary",
        EnvKind::Remark => ":remark",
        EnvKind::Example => ":example",
    }
    .len();
    let title_str = directive_line[prefix_len..].trim();
    let title = if title_str.is_empty() {
        None
    } else {
        Some(title_str.to_string())
    };

    // Collect indented body lines
    *i += 1;
    let mut body_text = String::new();
    while *i < lines.len() && is_continuation(&lines[*i]) {
        if !body_text.is_empty() {
            body_text.push(' ');
        }
        body_text.push_str(lines[*i].trim());
        *i += 1;
    }

    let body = if body_text.is_empty() {
        Vec::new()
    } else {
        parse_prose_fragments(&body_text)?
    };

    Ok(Environment {
        kind,
        title,
        body,
        span,
    })
}

/// Parse `lhs = rhs` from a claim body string.
/// Parse `name [options]` — returns (name, Some(options)) or (name, None).
fn parse_name_with_options(s: &str) -> (String, Option<String>) {
    if let Some(bracket_start) = s.find('[') {
        let name = s[..bracket_start].trim().to_string();
        let opts = s[bracket_start + 1..].trim_end_matches(']').trim().to_string();
        (name, Some(opts))
    } else {
        (s.to_string(), None)
    }
}

fn parse_table_row(line: &str) -> Result<Vec<Vec<ProseFragment>>, ParseDocError> {
    let cells: Vec<&str> = line.split('|')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    cells.iter().map(|c| parse_prose_fragments(c)).collect()
}

fn parse_claim_body(name: &str, body: &str, line: usize) -> Result<Claim, ParseDocError> {
    let eq_pos = body.find('=').ok_or_else(|| ParseDocError {
        line,
        message: format!("claim '{}': expected 'lhs = rhs'", name),
    })?;

    let lhs_str = body[..eq_pos].trim();
    let rhs_str = body[eq_pos + 1..].trim();

    let lhs = parse_expr(lhs_str).map_err(|e| ParseDocError {
        line,
        message: format!("claim '{}' lhs: {:?}", name, e),
    })?;

    let rhs = parse_expr(rhs_str).map_err(|e| ParseDocError {
        line,
        message: format!("claim '{}' rhs: {:?}", name, e),
    })?;

    Ok(Claim {
        name: name.to_string(),
        lhs,
        rhs,
        span: Span { line },
    })
}

/// Parse a proof step line: `expr` or `expr ; justification`
fn parse_proof_step(text: &str, line: usize) -> Result<ProofStep, ParseDocError> {
    let (expr_str, justification) = if let Some(semi_pos) = text.find(';') {
        let expr_part = text[..semi_pos].trim();
        let just_part = text[semi_pos + 1..].trim().to_string();
        (expr_part, Some(just_part))
    } else {
        (text.trim(), None)
    };

    let expr = parse_expr(expr_str).map_err(|e| ParseDocError {
        line,
        message: format!("proof step: {:?}", e),
    })?;

    Ok(ProofStep {
        expr,
        justification,
        span: Span { line },
    })
}

/// An inline match found in prose text.
enum InlineMatch<'a> {
    Tag(TagMatch<'a>),
    Bold { start: usize, end: usize, content: &'a str },
    Italic { start: usize, end: usize, content: &'a str },
    Footnote { start: usize, end: usize, content: &'a str },
}

impl InlineMatch<'_> {
    fn start(&self) -> usize {
        match self {
            InlineMatch::Tag(t) => t.start,
            InlineMatch::Bold { start, .. } => *start,
            InlineMatch::Italic { start, .. } => *start,
            InlineMatch::Footnote { start, .. } => *start,
        }
    }
}

/// Find the next bold `**...**` or italic `*...*` marker.
/// Bold (`**`) is checked before italic (`*`) to avoid false matches.
fn find_emphasis(text: &str) -> Option<InlineMatch<'_>> {
    // Look for ** first (bold)
    if let Some(open) = text.find("**") {
        // Check for *** (bold+italic)
        if text[open..].starts_with("***") {
            // Find closing ***
            if let Some(close_offset) = text[open + 3..].find("***") {
                let content = &text[open + 3..open + 3 + close_offset];
                if !content.is_empty() {
                    return Some(InlineMatch::Bold {
                        start: open,
                        end: open + 3 + close_offset + 3,
                        content,
                    });
                }
            }
        }
        // Find closing **
        if let Some(close_offset) = text[open + 2..].find("**") {
            let content = &text[open + 2..open + 2 + close_offset];
            if !content.is_empty() {
                return Some(InlineMatch::Bold {
                    start: open,
                    end: open + 2 + close_offset + 2,
                    content,
                });
            }
        }
    }

    // Look for single * (italic)
    let mut search_from = 0;
    while search_from < text.len() {
        if let Some(open) = text[search_from..].find('*') {
            let open = search_from + open;
            // Skip if this is part of a ** sequence
            if open + 1 < text.len() && text.as_bytes()[open + 1] == b'*' {
                search_from = open + 2;
                continue;
            }
            // Also skip if preceded by * (we're at the second char of **)
            if open > 0 && text.as_bytes()[open - 1] == b'*' {
                search_from = open + 1;
                continue;
            }
            // Find closing * (that isn't part of **)
            let mut close_from = open + 1;
            while close_from < text.len() {
                if let Some(close_offset) = text[close_from..].find('*') {
                    let close = close_from + close_offset;
                    // Skip if part of **
                    if close + 1 < text.len() && text.as_bytes()[close + 1] == b'*' {
                        close_from = close + 2;
                        continue;
                    }
                    if close > 0 && text.as_bytes()[close - 1] == b'*' {
                        close_from = close + 1;
                        continue;
                    }
                    let content = &text[open + 1..close];
                    if !content.is_empty() {
                        return Some(InlineMatch::Italic {
                            start: open,
                            end: close + 1,
                            content,
                        });
                    }
                    break;
                } else {
                    break;
                }
            }
            search_from = open + 1;
        } else {
            break;
        }
    }

    None
}

/// Find the next `^[...]` footnote in the text.
fn find_footnote(text: &str) -> Option<InlineMatch<'_>> {
    let open = text.find("^[")?;
    // Find matching ], accounting for nesting
    let mut depth = 0;
    for (i, ch) in text[open + 2..].char_indices() {
        match ch {
            '[' => depth += 1,
            ']' => {
                if depth == 0 {
                    let content = &text[open + 2..open + 2 + i];
                    if !content.is_empty() {
                        return Some(InlineMatch::Footnote {
                            start: open,
                            end: open + 2 + i + 1,
                            content,
                        });
                    }
                    return None;
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    None
}

/// Find the earliest inline construct in the text (tag, emphasis, or footnote).
fn find_next_inline(text: &str) -> Option<InlineMatch<'_>> {
    let tag = find_tagged_backtick(text).map(InlineMatch::Tag);
    let emph = find_emphasis(text);
    let foot = find_footnote(text);

    let candidates: Vec<InlineMatch<'_>> = [tag, emph, foot]
        .into_iter()
        .flatten()
        .collect();

    candidates.into_iter().min_by_key(|c| c.start())
}

/// Parse prose text into fragments, extracting tagged inline expressions.
/// Supports: math`expr`, tex`raw latex`, claim`name`, **bold**, *italic*
fn parse_prose_fragments(text: &str) -> Result<Vec<ProseFragment>, ParseDocError> {
    let mut fragments = Vec::new();
    let mut rest = text;

    while !rest.is_empty() {
        if let Some(inline) = find_next_inline(rest) {
            let start = inline.start();
            if start > 0 {
                fragments.push(ProseFragment::Text(rest[..start].to_string()));
            }

            match inline {
                InlineMatch::Tag(tag_match) => {
                    match tag_match.tag {
                        "math" => {
                            let expr =
                                parse_expr(tag_match.content).map_err(|e| ParseDocError {
                                    line: 0,
                                    message: format!(
                                        "inline math`{}`: {:?}",
                                        tag_match.content, e
                                    ),
                                })?;
                            fragments.push(ProseFragment::Math(expr));
                        }
                        "tex" => {
                            fragments.push(ProseFragment::Tex(tag_match.content.to_string()));
                        }
                        "claim" => {
                            fragments
                                .push(ProseFragment::ClaimRef(tag_match.content.to_string()));
                        }
                        "cite" => {
                            let keys: Vec<String> = tag_match
                                .content
                                .split(',')
                                .map(|k| k.trim().to_string())
                                .collect();
                            fragments.push(ProseFragment::Cite(keys));
                        }
                        "ref" => {
                            let (label, display) =
                                if let Some(pipe) = tag_match.content.find('|') {
                                    (
                                        tag_match.content[..pipe].to_string(),
                                        Some(tag_match.content[pipe + 1..].to_string()),
                                    )
                                } else {
                                    (tag_match.content.to_string(), None)
                                };
                            fragments.push(ProseFragment::Ref { label, display });
                        }
                        "url" => {
                            let (url, display) =
                                if let Some(pipe) = tag_match.content.find('|') {
                                    (
                                        tag_match.content[..pipe].to_string(),
                                        Some(tag_match.content[pipe + 1..].to_string()),
                                    )
                                } else {
                                    (tag_match.content.to_string(), None)
                                };
                            fragments.push(ProseFragment::Url { url, display });
                        }
                        _ => {
                            fragments.push(ProseFragment::Text(
                                rest[tag_match.start..tag_match.end].to_string(),
                            ));
                        }
                    }
                    rest = &rest[tag_match.end..];
                }
                InlineMatch::Bold { end, content, .. } => {
                    // For ***, the inner content should be parsed as italic
                    let inner = if rest[start..].starts_with("***") {
                        let italic_inner = parse_prose_fragments(content)?;
                        vec![ProseFragment::Italic(italic_inner)]
                    } else {
                        parse_prose_fragments(content)?
                    };
                    fragments.push(ProseFragment::Bold(inner));
                    rest = &rest[end..];
                }
                InlineMatch::Italic { end, content, .. } => {
                    let inner = parse_prose_fragments(content)?;
                    fragments.push(ProseFragment::Italic(inner));
                    rest = &rest[end..];
                }
                InlineMatch::Footnote { end, content, .. } => {
                    let inner = parse_prose_fragments(content)?;
                    fragments.push(ProseFragment::Footnote(inner));
                    rest = &rest[end..];
                }
            }
        } else {
            fragments.push(ProseFragment::Text(rest.to_string()));
            break;
        }
    }

    Ok(fragments)
}

struct TagMatch<'a> {
    tag: &'a str,
    content: &'a str,
    start: usize,
    end: usize,
}

/// Find the next `tag\`content\`` pattern in the text.
fn find_tagged_backtick(text: &str) -> Option<TagMatch<'_>> {
    let tags = ["math", "tex", "claim", "cite", "ref", "url"];

    let mut best: Option<TagMatch<'_>> = None;

    for tag in &tags {
        let pattern = format!("{}`", tag);
        if let Some(pos) = text.find(&pattern) {
            let content_start = pos + pattern.len();
            if let Some(end_tick) = text[content_start..].find('`') {
                let content = &text[content_start..content_start + end_tick];
                let end = content_start + end_tick + 1;

                if best.as_ref().map_or(true, |b| pos < b.start) {
                    best = Some(TagMatch {
                        tag,
                        content,
                        start: pos,
                        end,
                    });
                }
            }
        }
    }

    best
}

/// Extract plain text from prose fragments (for backward compatibility in tests).
pub fn prose_to_string(fragments: &[ProseFragment]) -> String {
    let mut s = String::new();
    for f in fragments {
        match f {
            ProseFragment::Text(t) => s.push_str(t),
            ProseFragment::Math(_) => s.push_str("[math]"),
            ProseFragment::Tex(t) => {
                s.push_str("tex`");
                s.push_str(t);
                s.push('`');
            }
            ProseFragment::ClaimRef(name) => {
                s.push_str("claim`");
                s.push_str(name);
                s.push('`');
            }
            ProseFragment::Bold(inner) => {
                s.push_str("**");
                s.push_str(&prose_to_string(inner));
                s.push_str("**");
            }
            ProseFragment::Italic(inner) => {
                s.push_str("*");
                s.push_str(&prose_to_string(inner));
                s.push_str("*");
            }
            ProseFragment::Cite(keys) => {
                s.push_str("cite`");
                s.push_str(&keys.join(","));
                s.push('`');
            }
            ProseFragment::Footnote(inner) => {
                s.push_str("^[");
                s.push_str(&prose_to_string(inner));
                s.push(']');
            }
            ProseFragment::Ref { label, display } => {
                s.push_str("ref`");
                s.push_str(label);
                if let Some(d) = display {
                    s.push('|');
                    s.push_str(d);
                }
                s.push('`');
            }
            ProseFragment::Url { url, display } => {
                s.push_str("url`");
                s.push_str(url);
                if let Some(d) = display {
                    s.push('|');
                    s.push_str(d);
                }
                s.push('`');
            }
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_empty_document() {
        let doc = parse_document("").unwrap();
        assert!(doc.blocks.is_empty());
    }

    #[test]
    fn parse_section_heading() {
        let doc = parse_document("# My Section\n## Subsection").unwrap();
        assert_eq!(doc.blocks.len(), 2);
        match &doc.blocks[0] {
            Block::Section { level, title, .. } => {
                assert_eq!(*level, 1);
                assert_eq!(title, "My Section");
            }
            _ => panic!("expected Section"),
        }
        match &doc.blocks[1] {
            Block::Section { level, title, .. } => {
                assert_eq!(*level, 2);
                assert_eq!(title, "Subsection");
            }
            _ => panic!("expected Section"),
        }
    }

    #[test]
    fn parse_prose() {
        let doc = parse_document("This is prose.\nContinued on next line.").unwrap();
        assert_eq!(doc.blocks.len(), 1);
        match &doc.blocks[0] {
            Block::Prose(fragments) => {
                assert_eq!(
                    prose_to_string(fragments),
                    "This is prose. Continued on next line."
                );
            }
            _ => panic!("expected Prose"),
        }
    }

    #[test]
    fn parse_prose_with_inline_math() {
        let doc = parse_document("The identity math`sin(x)` is well known.").unwrap();
        match &doc.blocks[0] {
            Block::Prose(fragments) => {
                assert_eq!(fragments.len(), 3);
                assert!(matches!(&fragments[0], ProseFragment::Text(t) if t == "The identity "));
                assert!(matches!(&fragments[1], ProseFragment::Math(_)));
                assert!(matches!(&fragments[2], ProseFragment::Text(t) if t == " is well known."));
            }
            _ => panic!("expected Prose"),
        }
    }

    #[test]
    fn parse_prose_with_tex_and_claim_ref() {
        let doc =
            parse_document("See tex`\\vec{v}` and claim`pythagorean` for details.").unwrap();
        match &doc.blocks[0] {
            Block::Prose(fragments) => {
                assert_eq!(fragments.len(), 5);
                assert!(matches!(&fragments[0], ProseFragment::Text(t) if t == "See "));
                assert!(matches!(&fragments[1], ProseFragment::Tex(t) if t == "\\vec{v}"));
                assert!(matches!(&fragments[2], ProseFragment::Text(t) if t == " and "));
                assert!(matches!(&fragments[3], ProseFragment::ClaimRef(n) if n == "pythagorean"));
                assert!(matches!(&fragments[4], ProseFragment::Text(t) if t == " for details."));
            }
            _ => panic!("expected Prose"),
        }
    }

    #[test]
    fn parse_claim() {
        let src = ":claim identity\n  x + 0 = x";
        let doc = parse_document(src).unwrap();
        assert_eq!(doc.blocks.len(), 1);
        match &doc.blocks[0] {
            Block::Claim(c) => {
                assert_eq!(c.name, "identity");
            }
            _ => panic!("expected Claim"),
        }
    }

    #[test]
    fn parse_claim_missing_equals() {
        let src = ":claim bad\n  x + 0";
        let err = parse_document(src).unwrap_err();
        assert!(err.message.contains("expected 'lhs = rhs'"));
    }

    #[test]
    fn parse_claim_missing_name() {
        let src = ":claim\n  x = x";
        let err = parse_document(src).unwrap_err();
        assert!(err.message.contains("requires a name"));
    }

    #[test]
    fn parse_proof() {
        let src = "\
:proof pythag
  sin(x)^2 + cos(x)^2
  = 1                        ; pythagorean_identity";
        let doc = parse_document(src).unwrap();
        assert_eq!(doc.blocks.len(), 1);
        match &doc.blocks[0] {
            Block::Proof(p) => {
                assert_eq!(p.claim_name, "pythag");
                assert_eq!(p.steps.len(), 2);
                assert!(p.steps[0].justification.is_none());
                assert_eq!(
                    p.steps[1].justification.as_deref(),
                    Some("pythagorean_identity")
                );
            }
            _ => panic!("expected Proof"),
        }
    }

    #[test]
    fn parse_proof_requires_two_steps() {
        let src = ":proof foo\n  x";
        let err = parse_document(src).unwrap_err();
        assert!(err.message.contains("at least two steps"));
    }

    #[test]
    fn parse_proof_missing_name() {
        let src = ":proof\n  x\n  = y";
        let err = parse_document(src).unwrap_err();
        assert!(err.message.contains("requires a claim name"));
    }

    #[test]
    fn parse_dim_declaration() {
        let src = ":dim x [L]\n:dim v [L T^-1]";
        let doc = parse_document(src).unwrap();
        assert_eq!(doc.blocks.len(), 2);
        match &doc.blocks[0] {
            Block::Dim(d) => {
                assert_eq!(d.var_name, "x");
                assert_eq!(d.dimension.to_string(), "[L]");
            }
            _ => panic!("expected Dim"),
        }
        match &doc.blocks[1] {
            Block::Dim(d) => {
                assert_eq!(d.var_name, "v");
                assert_eq!(d.dimension.to_string(), "[L T^-1]");
            }
            _ => panic!("expected Dim"),
        }
    }

    #[test]
    fn parse_dim_missing_brackets() {
        let src = ":dim x L";
        let err = parse_document(src).unwrap_err();
        assert!(
            err.message.contains(":dim requires"),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn parse_mixed_document() {
        let src = "\
# Algebra

Some introductory text.

:claim add_zero
  x + 0 = x

More prose here.
";
        let doc = parse_document(src).unwrap();
        assert_eq!(doc.blocks.len(), 4);
        assert!(matches!(&doc.blocks[0], Block::Section { .. }));
        assert!(matches!(&doc.blocks[1], Block::Prose(_)));
        assert!(matches!(&doc.blocks[2], Block::Claim(_)));
        assert!(matches!(&doc.blocks[3], Block::Prose(_)));
    }

    #[test]
    fn parse_bold_text() {
        let doc = parse_document("This is **bold** text.").unwrap();
        match &doc.blocks[0] {
            Block::Prose(fragments) => {
                assert_eq!(fragments.len(), 3);
                assert!(matches!(&fragments[0], ProseFragment::Text(t) if t == "This is "));
                match &fragments[1] {
                    ProseFragment::Bold(inner) => {
                        assert_eq!(inner.len(), 1);
                        assert!(matches!(&inner[0], ProseFragment::Text(t) if t == "bold"));
                    }
                    other => panic!("expected Bold, got {:?}", other),
                }
                assert!(matches!(&fragments[2], ProseFragment::Text(t) if t == " text."));
            }
            _ => panic!("expected Prose"),
        }
    }

    #[test]
    fn parse_italic_text() {
        let doc = parse_document("This is *italic* text.").unwrap();
        match &doc.blocks[0] {
            Block::Prose(fragments) => {
                assert_eq!(fragments.len(), 3);
                assert!(matches!(&fragments[0], ProseFragment::Text(t) if t == "This is "));
                match &fragments[1] {
                    ProseFragment::Italic(inner) => {
                        assert_eq!(inner.len(), 1);
                        assert!(matches!(&inner[0], ProseFragment::Text(t) if t == "italic"));
                    }
                    other => panic!("expected Italic, got {:?}", other),
                }
                assert!(matches!(&fragments[2], ProseFragment::Text(t) if t == " text."));
            }
            _ => panic!("expected Prose"),
        }
    }

    #[test]
    fn parse_bold_with_inner_math() {
        let doc = parse_document("See **the value math`x`** here.").unwrap();
        match &doc.blocks[0] {
            Block::Prose(fragments) => {
                assert_eq!(fragments.len(), 3);
                match &fragments[1] {
                    ProseFragment::Bold(inner) => {
                        assert_eq!(inner.len(), 2);
                        assert!(matches!(&inner[0], ProseFragment::Text(t) if t == "the value "));
                        assert!(matches!(&inner[1], ProseFragment::Math(_)));
                    }
                    other => panic!("expected Bold, got {:?}", other),
                }
            }
            _ => panic!("expected Prose"),
        }
    }

    #[test]
    fn parse_bold_italic() {
        let doc = parse_document("This is ***both*** here.").unwrap();
        match &doc.blocks[0] {
            Block::Prose(fragments) => {
                assert_eq!(fragments.len(), 3);
                match &fragments[1] {
                    ProseFragment::Bold(inner) => {
                        assert_eq!(inner.len(), 1);
                        match &inner[0] {
                            ProseFragment::Italic(inner2) => {
                                assert_eq!(inner2.len(), 1);
                                assert!(
                                    matches!(&inner2[0], ProseFragment::Text(t) if t == "both")
                                );
                            }
                            other => panic!("expected Italic inside Bold, got {:?}", other),
                        }
                    }
                    other => panic!("expected Bold, got {:?}", other),
                }
            }
            _ => panic!("expected Prose"),
        }
    }

    #[test]
    fn parse_prose_to_string_roundtrip_bold_italic() {
        let doc = parse_document("Hello **world** and *emphasis*.").unwrap();
        match &doc.blocks[0] {
            Block::Prose(fragments) => {
                assert_eq!(
                    prose_to_string(fragments),
                    "Hello **world** and *emphasis*."
                );
            }
            _ => panic!("expected Prose"),
        }
    }

    #[test]
    fn parse_bullet_list() {
        let src = "- First\n- Second\n- Third";
        let doc = parse_document(src).unwrap();
        assert_eq!(doc.blocks.len(), 1);
        match &doc.blocks[0] {
            Block::List(list) => {
                assert!(!list.ordered);
                assert_eq!(list.items.len(), 3);
                assert!(matches!(&list.items[0].fragments[0], ProseFragment::Text(t) if t == "First"));
                assert!(matches!(&list.items[1].fragments[0], ProseFragment::Text(t) if t == "Second"));
                assert!(matches!(&list.items[2].fragments[0], ProseFragment::Text(t) if t == "Third"));
            }
            other => panic!("expected List, got {:?}", other),
        }
    }

    #[test]
    fn parse_numbered_list() {
        let src = "1. Alpha\n2. Beta\n3. Gamma";
        let doc = parse_document(src).unwrap();
        assert_eq!(doc.blocks.len(), 1);
        match &doc.blocks[0] {
            Block::List(list) => {
                assert!(list.ordered);
                assert_eq!(list.items.len(), 3);
                assert!(matches!(&list.items[0].fragments[0], ProseFragment::Text(t) if t == "Alpha"));
            }
            other => panic!("expected List, got {:?}", other),
        }
    }

    #[test]
    fn parse_nested_bullet_list() {
        let src = "- Outer\n  - Inner A\n  - Inner B\n- Back";
        let doc = parse_document(src).unwrap();
        match &doc.blocks[0] {
            Block::List(list) => {
                assert_eq!(list.items.len(), 2);
                assert!(matches!(&list.items[0].fragments[0], ProseFragment::Text(t) if t == "Outer"));
                let children = list.items[0].children.as_ref().expect("expected nested list");
                assert_eq!(children.items.len(), 2);
                assert!(matches!(&children.items[0].fragments[0], ProseFragment::Text(t) if t == "Inner A"));
                assert!(matches!(&list.items[1].fragments[0], ProseFragment::Text(t) if t == "Back"));
            }
            other => panic!("expected List, got {:?}", other),
        }
    }

    #[test]
    fn parse_list_with_inline_math() {
        let src = "- Energy: math`mc^2`\n- Momentum: math`m * v`";
        let doc = parse_document(src).unwrap();
        match &doc.blocks[0] {
            Block::List(list) => {
                assert_eq!(list.items.len(), 2);
                assert_eq!(list.items[0].fragments.len(), 2);
                assert!(matches!(&list.items[0].fragments[0], ProseFragment::Text(t) if t == "Energy: "), "got {:?}", list.items[0].fragments[0]);
                assert!(matches!(&list.items[0].fragments[1], ProseFragment::Math(_)));
            }
            other => panic!("expected List, got {:?}", other),
        }
    }

    #[test]
    fn parse_list_terminated_by_blank_line() {
        let src = "- Item one\n- Item two\n\nProse after list.";
        let doc = parse_document(src).unwrap();
        assert_eq!(doc.blocks.len(), 2);
        assert!(matches!(&doc.blocks[0], Block::List(_)));
        assert!(matches!(&doc.blocks[1], Block::Prose(_)));
    }

    #[test]
    fn parse_list_terminated_by_directive() {
        let src = "- Item one\n:claim foo\n  x = x";
        let doc = parse_document(src).unwrap();
        assert_eq!(doc.blocks.len(), 2);
        assert!(matches!(&doc.blocks[0], Block::List(_)));
        assert!(matches!(&doc.blocks[1], Block::Claim(_)));
    }

    #[test]
    fn parse_math_block_single() {
        let src = "```math\nx + 1\n```";
        let doc = parse_document(src).unwrap();
        assert_eq!(doc.blocks.len(), 1);
        match &doc.blocks[0] {
            Block::MathBlock(mb) => {
                assert_eq!(mb.exprs.len(), 1);
            }
            other => panic!("expected MathBlock, got {:?}", other),
        }
    }

    #[test]
    fn parse_math_block_multi() {
        let src = "```math\nx + 1\ny + 2\nz + 3\n```";
        let doc = parse_document(src).unwrap();
        match &doc.blocks[0] {
            Block::MathBlock(mb) => {
                assert_eq!(mb.exprs.len(), 3);
            }
            other => panic!("expected MathBlock, got {:?}", other),
        }
    }

    #[test]
    fn parse_math_block_skips_blank_lines() {
        let src = "```math\nx + 1\n\ny + 2\n```";
        let doc = parse_document(src).unwrap();
        match &doc.blocks[0] {
            Block::MathBlock(mb) => {
                assert_eq!(mb.exprs.len(), 2);
            }
            other => panic!("expected MathBlock, got {:?}", other),
        }
    }

    #[test]
    fn parse_math_block_unclosed() {
        let src = "```math\nx + 1";
        let err = parse_document(src).unwrap_err();
        assert!(err.message.contains("unclosed"));
    }

    #[test]
    fn parse_math_block_not_verified() {
        // Math blocks should appear in the document but not in verification
        let src = "```math\nx + 1\n```\n\n:claim foo\n  x + 0 = x";
        let doc = parse_document(src).unwrap();
        assert_eq!(doc.blocks.len(), 2);
        assert!(matches!(&doc.blocks[0], Block::MathBlock(_)));
        assert!(matches!(&doc.blocks[1], Block::Claim(_)));
    }

    #[test]
    fn parse_cite_single_key() {
        let doc = parse_document("See cite`einstein1905` for details.").unwrap();
        match &doc.blocks[0] {
            Block::Prose(fragments) => {
                assert_eq!(fragments.len(), 3);
                match &fragments[1] {
                    ProseFragment::Cite(keys) => {
                        assert_eq!(keys, &["einstein1905"]);
                    }
                    other => panic!("expected Cite, got {:?}", other),
                }
            }
            _ => panic!("expected Prose"),
        }
    }

    #[test]
    fn parse_cite_multiple_keys() {
        let doc = parse_document("See cite`einstein1905,dirac1928` here.").unwrap();
        match &doc.blocks[0] {
            Block::Prose(fragments) => {
                match &fragments[1] {
                    ProseFragment::Cite(keys) => {
                        assert_eq!(keys, &["einstein1905", "dirac1928"]);
                    }
                    other => panic!("expected Cite, got {:?}", other),
                }
            }
            _ => panic!("expected Prose"),
        }
    }

    #[test]
    fn parse_bibliography() {
        let doc = parse_document(":bibliography refs.bib").unwrap();
        assert_eq!(doc.blocks.len(), 1);
        match &doc.blocks[0] {
            Block::Bibliography { path, .. } => {
                assert_eq!(path, "refs.bib");
            }
            other => panic!("expected Bibliography, got {:?}", other),
        }
    }

    #[test]
    fn parse_bibliography_missing_path() {
        let err = parse_document(":bibliography").unwrap_err();
        assert!(err.message.contains("requires a file path"));
    }

    #[test]
    fn parse_theorem_with_title() {
        let src = ":theorem Pythagorean\n  For any right triangle, math`a^2 + b^2` is important.";
        let doc = parse_document(src).unwrap();
        assert_eq!(doc.blocks.len(), 1);
        match &doc.blocks[0] {
            Block::Environment(env) => {
                assert_eq!(env.kind, EnvKind::Theorem);
                assert_eq!(env.title.as_deref(), Some("Pythagorean"));
                assert!(!env.body.is_empty());
            }
            other => panic!("expected Environment, got {:?}", other),
        }
    }

    #[test]
    fn parse_definition_no_title() {
        let src = ":definition\n  A group is a set with a binary operation.";
        let doc = parse_document(src).unwrap();
        match &doc.blocks[0] {
            Block::Environment(env) => {
                assert_eq!(env.kind, EnvKind::Definition);
                assert!(env.title.is_none());
                assert_eq!(prose_to_string(&env.body), "A group is a set with a binary operation.");
            }
            other => panic!("expected Environment, got {:?}", other),
        }
    }

    #[test]
    fn parse_all_env_kinds() {
        for (directive, expected_kind) in &[
            (":theorem", EnvKind::Theorem),
            (":lemma", EnvKind::Lemma),
            (":definition", EnvKind::Definition),
            (":corollary", EnvKind::Corollary),
            (":remark", EnvKind::Remark),
            (":example", EnvKind::Example),
        ] {
            let src = format!("{}\n  Body text.", directive);
            let doc = parse_document(&src).unwrap();
            match &doc.blocks[0] {
                Block::Environment(env) => {
                    assert_eq!(env.kind, *expected_kind, "failed for {}", directive);
                }
                other => panic!("expected Environment for {}, got {:?}", directive, other),
            }
        }
    }

    #[test]
    fn parse_env_body_with_inline_math() {
        let src = ":lemma\n  If math`x` is positive then math`x^2` is positive.";
        let doc = parse_document(src).unwrap();
        match &doc.blocks[0] {
            Block::Environment(env) => {
                assert_eq!(env.kind, EnvKind::Lemma);
                // Should have Text, Math, Text, Math, Text
                assert_eq!(env.body.len(), 5);
                assert!(matches!(&env.body[1], ProseFragment::Math(_)));
                assert!(matches!(&env.body[3], ProseFragment::Math(_)));
            }
            other => panic!("expected Environment, got {:?}", other),
        }
    }

    #[test]
    fn parse_env_multiline_body() {
        let src = ":remark Important\n  First line.\n  Second line.";
        let doc = parse_document(src).unwrap();
        match &doc.blocks[0] {
            Block::Environment(env) => {
                assert_eq!(env.kind, EnvKind::Remark);
                assert_eq!(env.title.as_deref(), Some("Important"));
                assert_eq!(prose_to_string(&env.body), "First line. Second line.");
            }
            other => panic!("expected Environment, got {:?}", other),
        }
    }

    #[test]
    fn parse_env_not_confused_with_claim() {
        let src = ":theorem Test\n  Body.\n\n:claim foo\n  x = x";
        let doc = parse_document(src).unwrap();
        assert_eq!(doc.blocks.len(), 2);
        assert!(matches!(&doc.blocks[0], Block::Environment(_)));
        assert!(matches!(&doc.blocks[1], Block::Claim(_)));
    }

    // Phase 6: Block quotes, footnotes, comments

    #[test]
    fn parse_comment_skipped() {
        let src = "% This is a comment\nVisible text.";
        let doc = parse_document(src).unwrap();
        assert_eq!(doc.blocks.len(), 1);
        match &doc.blocks[0] {
            Block::Prose(fragments) => {
                assert_eq!(prose_to_string(fragments), "Visible text.");
            }
            other => panic!("expected Prose, got {:?}", other),
        }
    }

    #[test]
    fn parse_comment_between_blocks() {
        let src = "# Title\n% comment\nProse here.";
        let doc = parse_document(src).unwrap();
        assert_eq!(doc.blocks.len(), 2);
        assert!(matches!(&doc.blocks[0], Block::Section { .. }));
        assert!(matches!(&doc.blocks[1], Block::Prose(_)));
    }

    #[test]
    fn parse_comment_only_document() {
        let src = "% just a comment\n% another comment";
        let doc = parse_document(src).unwrap();
        assert!(doc.blocks.is_empty());
    }

    #[test]
    fn parse_block_quote() {
        let src = "> This is a quoted passage.";
        let doc = parse_document(src).unwrap();
        assert_eq!(doc.blocks.len(), 1);
        match &doc.blocks[0] {
            Block::BlockQuote(fragments) => {
                assert_eq!(prose_to_string(fragments), "This is a quoted passage.");
            }
            other => panic!("expected BlockQuote, got {:?}", other),
        }
    }

    #[test]
    fn parse_block_quote_multiline() {
        let src = "> First line of the quote.\n> Second line continues.";
        let doc = parse_document(src).unwrap();
        assert_eq!(doc.blocks.len(), 1);
        match &doc.blocks[0] {
            Block::BlockQuote(fragments) => {
                assert_eq!(
                    prose_to_string(fragments),
                    "First line of the quote. Second line continues."
                );
            }
            other => panic!("expected BlockQuote, got {:?}", other),
        }
    }

    #[test]
    fn parse_block_quote_with_inline_formatting() {
        let src = "> This has **bold** and math`x` in it.";
        let doc = parse_document(src).unwrap();
        match &doc.blocks[0] {
            Block::BlockQuote(fragments) => {
                assert!(fragments.len() >= 3);
                assert!(fragments.iter().any(|f| matches!(f, ProseFragment::Bold(_))));
                assert!(fragments.iter().any(|f| matches!(f, ProseFragment::Math(_))));
            }
            other => panic!("expected BlockQuote, got {:?}", other),
        }
    }

    #[test]
    fn parse_block_quote_terminated_by_non_quote() {
        let src = "> Quoted text.\nProse after.";
        let doc = parse_document(src).unwrap();
        assert_eq!(doc.blocks.len(), 2);
        assert!(matches!(&doc.blocks[0], Block::BlockQuote(_)));
        assert!(matches!(&doc.blocks[1], Block::Prose(_)));
    }

    #[test]
    fn parse_footnote_inline() {
        let src = "This is surprising^[First noted by Euler.].";
        let doc = parse_document(src).unwrap();
        match &doc.blocks[0] {
            Block::Prose(fragments) => {
                assert_eq!(fragments.len(), 3);
                assert!(matches!(&fragments[0], ProseFragment::Text(t) if t == "This is surprising"));
                match &fragments[1] {
                    ProseFragment::Footnote(inner) => {
                        assert_eq!(prose_to_string(inner), "First noted by Euler.");
                    }
                    other => panic!("expected Footnote, got {:?}", other),
                }
                assert!(matches!(&fragments[2], ProseFragment::Text(t) if t == "."));
            }
            _ => panic!("expected Prose"),
        }
    }

    #[test]
    fn parse_footnote_with_math() {
        let src = "Result^[See math`x^2` for details.] here.";
        let doc = parse_document(src).unwrap();
        match &doc.blocks[0] {
            Block::Prose(fragments) => {
                match &fragments[1] {
                    ProseFragment::Footnote(inner) => {
                        assert!(inner.iter().any(|f| matches!(f, ProseFragment::Math(_))));
                    }
                    other => panic!("expected Footnote, got {:?}", other),
                }
            }
            _ => panic!("expected Prose"),
        }
    }

    #[test]
    fn parse_footnote_roundtrip() {
        let doc = parse_document("Text^[a note] more.").unwrap();
        match &doc.blocks[0] {
            Block::Prose(fragments) => {
                assert_eq!(prose_to_string(fragments), "Text^[a note] more.");
            }
            _ => panic!("expected Prose"),
        }
    }

    // Cross-references

    #[test]
    fn parse_ref_label_only() {
        let doc = parse_document("See ref`newtons-laws` for details.").unwrap();
        match &doc.blocks[0] {
            Block::Prose(fragments) => {
                assert_eq!(fragments.len(), 3);
                match &fragments[1] {
                    ProseFragment::Ref { label, display } => {
                        assert_eq!(label, "newtons-laws");
                        assert!(display.is_none());
                    }
                    other => panic!("expected Ref, got {:?}", other),
                }
            }
            _ => panic!("expected Prose"),
        }
    }

    #[test]
    fn parse_ref_with_display_text() {
        let doc =
            parse_document("ref`earth-and-the-solar-system|Hydrogen creation`").unwrap();
        match &doc.blocks[0] {
            Block::Prose(fragments) => {
                assert_eq!(fragments.len(), 1);
                match &fragments[0] {
                    ProseFragment::Ref { label, display } => {
                        assert_eq!(label, "earth-and-the-solar-system");
                        assert_eq!(display.as_deref(), Some("Hydrogen creation"));
                    }
                    other => panic!("expected Ref, got {:?}", other),
                }
            }
            _ => panic!("expected Prose"),
        }
    }

    #[test]
    fn parse_ref_roundtrip() {
        let doc = parse_document("See ref`foo|bar baz` here.").unwrap();
        match &doc.blocks[0] {
            Block::Prose(fragments) => {
                assert_eq!(prose_to_string(fragments), "See ref`foo|bar baz` here.");
            }
            _ => panic!("expected Prose"),
        }
    }

    // Document metadata

    #[test]
    fn parse_title() {
        let doc = parse_document(":title My Great Paper").unwrap();
        assert!(matches!(&doc.blocks[0], Block::Title(t) if t == "My Great Paper"));
    }

    #[test]
    fn parse_author() {
        let doc = parse_document(":author Alice Smith").unwrap();
        assert!(matches!(&doc.blocks[0], Block::Author(a) if a == "Alice Smith"));
    }

    #[test]
    fn parse_multiple_authors() {
        let doc = parse_document(":author Alice Smith\n:author Bob Jones").unwrap();
        assert_eq!(doc.blocks.len(), 2);
        assert!(matches!(&doc.blocks[0], Block::Author(a) if a == "Alice Smith"));
        assert!(matches!(&doc.blocks[1], Block::Author(a) if a == "Bob Jones"));
    }

    #[test]
    fn parse_date() {
        let doc = parse_document(":date 2026-03-13").unwrap();
        assert!(matches!(&doc.blocks[0], Block::Date(d) if d == "2026-03-13"));
    }

    #[test]
    fn parse_abstract() {
        let src = ":abstract\n  We present a novel approach\n  to computing corrections.";
        let doc = parse_document(src).unwrap();
        match &doc.blocks[0] {
            Block::Abstract(fragments) => {
                assert_eq!(
                    prose_to_string(fragments),
                    "We present a novel approach to computing corrections."
                );
            }
            other => panic!("expected Abstract, got {:?}", other),
        }
    }

    #[test]
    fn parse_abstract_with_inline_math() {
        let src = ":abstract\n  We study math`x^2 + 1` in detail.";
        let doc = parse_document(src).unwrap();
        match &doc.blocks[0] {
            Block::Abstract(fragments) => {
                assert!(fragments.iter().any(|f| matches!(f, ProseFragment::Math(_))));
            }
            other => panic!("expected Abstract, got {:?}", other),
        }
    }

    #[test]
    fn parse_title_empty_error() {
        let err = parse_document(":title").unwrap_err();
        assert!(err.message.contains(":title requires"));
    }

    // Figures

    #[test]
    fn parse_figure_basic() {
        let src = ":figure plots/energy.pdf\n  caption: Energy levels.\n  label: fig-energy\n  width: 0.8";
        let doc = parse_document(src).unwrap();
        match &doc.blocks[0] {
            Block::Figure(fig) => {
                assert_eq!(fig.path, "plots/energy.pdf");
                assert_eq!(prose_to_string(fig.caption.as_ref().unwrap()), "Energy levels.");
                assert_eq!(fig.label.as_deref(), Some("fig-energy"));
                assert!((fig.width - 0.8).abs() < 1e-6);
            }
            other => panic!("expected Figure, got {:?}", other),
        }
    }

    #[test]
    fn parse_figure_path_only() {
        let doc = parse_document(":figure img.png").unwrap();
        match &doc.blocks[0] {
            Block::Figure(fig) => {
                assert_eq!(fig.path, "img.png");
                assert!(fig.caption.is_none());
                assert!(fig.label.is_none());
                assert!((fig.width - 1.0).abs() < 1e-6);
            }
            other => panic!("expected Figure, got {:?}", other),
        }
    }

    #[test]
    fn parse_figure_empty_path_error() {
        let err = parse_document(":figure").unwrap_err();
        assert!(err.message.contains(":figure requires"));
    }

    #[test]
    fn parse_figure_caption_with_math() {
        let src = ":figure plot.pdf\n  caption: Energy math`mc^2` shown.";
        let doc = parse_document(src).unwrap();
        match &doc.blocks[0] {
            Block::Figure(fig) => {
                let cap = fig.caption.as_ref().unwrap();
                assert!(cap.iter().any(|f| matches!(f, ProseFragment::Math(_))));
            }
            other => panic!("expected Figure, got {:?}", other),
        }
    }

    // Document class and packages

    #[test]
    fn parse_document_class() {
        let doc = parse_document(":class revtex4-2 [aps,prl,twocolumn]").unwrap();
        match &doc.blocks[0] {
            Block::DocumentClass { name, options } => {
                assert_eq!(name, "revtex4-2");
                assert_eq!(options.as_deref(), Some("aps,prl,twocolumn"));
            }
            other => panic!("expected DocumentClass, got {:?}", other),
        }
    }

    #[test]
    fn parse_document_class_no_options() {
        let doc = parse_document(":class article").unwrap();
        match &doc.blocks[0] {
            Block::DocumentClass { name, options } => {
                assert_eq!(name, "article");
                assert!(options.is_none());
            }
            other => panic!("expected DocumentClass, got {:?}", other),
        }
    }

    #[test]
    fn parse_usepackage() {
        let doc = parse_document(":usepackage pgfplots [compat=1.18]").unwrap();
        match &doc.blocks[0] {
            Block::UsePackage { name, options } => {
                assert_eq!(name, "pgfplots");
                assert_eq!(options.as_deref(), Some("compat=1.18"));
            }
            other => panic!("expected UsePackage, got {:?}", other),
        }
    }

    #[test]
    fn parse_usepackage_no_options() {
        let doc = parse_document(":usepackage siunitx").unwrap();
        match &doc.blocks[0] {
            Block::UsePackage { name, options } => {
                assert_eq!(name, "siunitx");
                assert!(options.is_none());
            }
            other => panic!("expected UsePackage, got {:?}", other),
        }
    }

    #[test]
    fn parse_class_empty_error() {
        let err = parse_document(":class").unwrap_err();
        assert!(err.message.contains(":class requires"));
    }

    #[test]
    fn parse_usepackage_empty_error() {
        let err = parse_document(":usepackage").unwrap_err();
        assert!(err.message.contains(":usepackage requires"));
    }

    // Table of contents

    #[test]
    fn parse_toc() {
        let doc = parse_document(":toc").unwrap();
        assert!(matches!(&doc.blocks[0], Block::Toc));
    }

    // URLs

    #[test]
    fn parse_url_plain() {
        let doc = parse_document("See url`https://example.com` for info.").unwrap();
        match &doc.blocks[0] {
            Block::Prose(fragments) => {
                match &fragments[1] {
                    ProseFragment::Url { url, display } => {
                        assert_eq!(url, "https://example.com");
                        assert!(display.is_none());
                    }
                    other => panic!("expected Url, got {:?}", other),
                }
            }
            _ => panic!("expected Prose"),
        }
    }

    #[test]
    fn parse_url_with_display() {
        let doc = parse_document("Click url`https://example.com|here`.").unwrap();
        match &doc.blocks[0] {
            Block::Prose(fragments) => {
                match &fragments[1] {
                    ProseFragment::Url { url, display } => {
                        assert_eq!(url, "https://example.com");
                        assert_eq!(display.as_deref(), Some("here"));
                    }
                    other => panic!("expected Url, got {:?}", other),
                }
            }
            _ => panic!("expected Prose"),
        }
    }

    // Page breaks

    #[test]
    fn parse_pagebreak() {
        let doc = parse_document("Text.\n\n:pagebreak\n\nMore text.").unwrap();
        assert_eq!(doc.blocks.len(), 3);
        assert!(matches!(&doc.blocks[1], Block::PageBreak));
    }

    // Includes

    #[test]
    fn parse_include_basic() {
        let dir = std::env::temp_dir().join("erd_test_include_basic");
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join("main.erd"), "# Main\n\n:include sub.erd\n\nEnd.").unwrap();
        std::fs::write(dir.join("sub.erd"), "Sub content.").unwrap();
        let doc = parse_document_from_file(&dir.join("main.erd")).unwrap();
        // Should have: Section, Prose("Sub content."), Prose("End.")
        assert!(doc.blocks.len() >= 3);
        assert!(matches!(&doc.blocks[0], Block::Section { .. }));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_include_circular_error() {
        let dir = std::env::temp_dir().join("erd_test_include_circular");
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join("a.erd"), ":include b.erd").unwrap();
        std::fs::write(dir.join("b.erd"), ":include a.erd").unwrap();
        let err = parse_document_from_file(&dir.join("a.erd")).unwrap_err();
        assert!(err.message.contains("circular"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_include_missing_file_error() {
        let dir = std::env::temp_dir().join("erd_test_include_missing");
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join("main.erd"), ":include nonexistent.erd").unwrap();
        let err = parse_document_from_file(&dir.join("main.erd")).unwrap_err();
        assert!(err.message.contains("nonexistent.erd"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_include_nested() {
        let dir = std::env::temp_dir().join("erd_test_include_nested");
        let _ = std::fs::create_dir_all(dir.join("sub"));
        std::fs::write(dir.join("main.erd"), ":include sub/a.erd").unwrap();
        std::fs::write(dir.join("sub/a.erd"), ":include b.erd").unwrap();
        std::fs::write(dir.join("sub/b.erd"), "Nested content.").unwrap();
        let doc = parse_document_from_file(&dir.join("main.erd")).unwrap();
        match &doc.blocks[0] {
            Block::Prose(fragments) => {
                assert_eq!(prose_to_string(fragments), "Nested content.");
            }
            other => panic!("expected Prose, got {:?}", other),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    // Tables

    #[test]
    fn parse_table_basic() {
        let src = ":table Results\n  | A | B |\n  |---|---|\n  | 1 | 2 |";
        let doc = parse_document(src).unwrap();
        match &doc.blocks[0] {
            Block::Table(table) => {
                assert_eq!(table.title.as_deref(), Some("Results"));
                assert_eq!(table.header.len(), 2);
                assert_eq!(prose_to_string(&table.header[0]), "A");
                assert_eq!(table.rows.len(), 1);
                assert_eq!(prose_to_string(&table.rows[0][1]), "2");
            }
            other => panic!("expected Table, got {:?}", other),
        }
    }

    #[test]
    fn parse_table_alignment() {
        let src = ":table\n  | L | C | R |\n  |:--|:--:|--:|\n  | a | b | c |";
        let doc = parse_document(src).unwrap();
        match &doc.blocks[0] {
            Block::Table(table) => {
                assert!(matches!(table.columns[0], ColumnAlign::Left));
                assert!(matches!(table.columns[1], ColumnAlign::Center));
                assert!(matches!(table.columns[2], ColumnAlign::Right));
                assert!(table.title.is_none());
            }
            other => panic!("expected Table, got {:?}", other),
        }
    }

    #[test]
    fn parse_table_with_label() {
        let src = ":table T\n  | X |\n  |---|\n  | 1 |\n  label: tab-x";
        let doc = parse_document(src).unwrap();
        match &doc.blocks[0] {
            Block::Table(table) => {
                assert_eq!(table.label.as_deref(), Some("tab-x"));
            }
            other => panic!("expected Table, got {:?}", other),
        }
    }

    #[test]
    fn parse_table_cell_with_math() {
        let src = ":table\n  | Expr |\n  |------|\n  | math`x^2` |";
        let doc = parse_document(src).unwrap();
        match &doc.blocks[0] {
            Block::Table(table) => {
                assert!(table.rows[0][0].iter().any(|f| matches!(f, ProseFragment::Math(_))));
            }
            other => panic!("expected Table, got {:?}", other),
        }
    }

    #[test]
    fn parse_table_missing_separator_error() {
        let err = parse_document(":table T\n  | A |\n  | 1 |").unwrap_err();
        assert!(err.message.contains("separator"));
    }

    #[test]
    fn parse_ref_inside_bold_in_list() {
        let src = "1. **ref`my-section|Custom text`** *— description*";
        let doc = parse_document(src).unwrap();
        match &doc.blocks[0] {
            Block::List(list) => {
                assert_eq!(list.items.len(), 1);
                // Bold should contain the ref
                let bold = &list.items[0].fragments[0];
                match bold {
                    ProseFragment::Bold(inner) => {
                        assert!(
                            inner.iter().any(|f| matches!(f, ProseFragment::Ref { .. })),
                            "expected Ref inside Bold, got {:?}",
                            inner
                        );
                    }
                    other => panic!("expected Bold, got {:?}", other),
                }
            }
            other => panic!("expected List, got {:?}", other),
        }
    }
}
