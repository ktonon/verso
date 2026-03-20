use crate::ast::{
    Block, Claim, ClaimRelation, ColumnAlign, DefDecl, Document, EnvKind, Environment,
    ExpectFailType, Figure, FuncDecl, List, ListItem, MathBlock, Proof, ProofStep, ProseFragment,
    Span, Table, VarDecl,
};
pub use crate::source::{
    collect_dependencies, parse_document_from_file, resolve_includes, ParseDocError,
};
use verso_symbolic::{parse_expr, Dimension};

/// Parse an `.verso` document from source text.
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
            let raw_title = trimmed[level as usize..].trim();
            let (title, label) = extract_section_label(raw_title);
            blocks.push(Block::Section {
                level,
                title,
                label,
                span: Span { line: i + 1 },
            });
            i += 1;
            continue;
        }

        // Table of contents
        if trimmed == "!toc" {
            blocks.push(Block::Toc);
            i += 1;
            continue;
        }

        // Page break
        if trimmed == "!pagebreak" {
            blocks.push(Block::PageBreak);
            i += 1;
            continue;
        }

        // Document metadata
        if trimmed.starts_with("!title") {
            let text = trimmed["!title".len()..].trim().to_string();
            if text.is_empty() {
                // Multiline title: collect indented lines
                i += 1;
                let mut title_lines = Vec::new();
                while i < lines.len() {
                    let line = lines[i];
                    if line.trim().is_empty() {
                        break;
                    } else if is_continuation(line) {
                        title_lines.push(line.trim().to_string());
                        i += 1;
                    } else {
                        break;
                    }
                }
                if title_lines.is_empty() {
                    return Err(ParseDocError {
                        line: i,
                        message: "!title requires text".into(),
                    });
                }
                blocks.push(Block::Title(title_lines));
            } else {
                blocks.push(Block::Title(vec![text]));
                i += 1;
            }
            continue;
        }
        if trimmed.starts_with("!author") {
            let text = trimmed["!author".len()..].trim().to_string();
            if text.is_empty() {
                return Err(ParseDocError {
                    line: i + 1,
                    message: "!author requires a name".into(),
                });
            }
            blocks.push(Block::Author(text));
            i += 1;
            continue;
        }
        if trimmed.starts_with("!date") {
            let text = trimmed["!date".len()..].trim().to_string();
            blocks.push(Block::Date(if text.is_empty() { None } else { Some(text) }));
            i += 1;
            continue;
        }
        if trimmed.starts_with("!abstract") {
            let abs_line = i + 1;
            i += 1;
            let fragments = collect_indented_body(&lines, &mut i, abs_line)?;
            blocks.push(Block::Abstract(fragments));
            continue;
        }

        if trimmed.starts_with("!center") {
            let center_line = i + 1;
            i += 1;
            let fragments = collect_indented_body(&lines, &mut i, center_line)?;
            blocks.push(Block::Center(fragments));
            continue;
        }

        // Figure block
        if trimmed.starts_with("!figure") {
            let path = trimmed["!figure".len()..].trim().to_string();
            if path.is_empty() {
                return Err(ParseDocError {
                    line: i + 1,
                    message: "!figure requires a file path".into(),
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
                    let cap_line = i + 1;
                    caption = Some(parse_prose_fragments(val.trim()).map_err(|mut e| {
                        if e.line == 0 {
                            e.line = cap_line;
                        }
                        e
                    })?);
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
        if trimmed.starts_with("!table") {
            let title_text = trimmed["!table".len()..].trim();
            let title = if title_text.is_empty() {
                None
            } else {
                Some(title_text.to_string())
            };
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
                    message: "!table requires header row and separator row".into(),
                });
            }

            // Parse header row
            let header = parse_table_row(&body_lines[0])?;
            let num_cols = header.len();

            // Parse separator row for alignment
            let sep = &body_lines[1];
            let sep_cells: Vec<&str> = sep
                .split('|')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();
            if sep_cells.len() != num_cols
                || !sep_cells
                    .iter()
                    .all(|s| s.chars().all(|c| c == '-' || c == ':'))
            {
                return Err(ParseDocError {
                    line: table_line + 1,
                    message: "!table second row must be a separator (e.g. |---|---|)".into(),
                });
            }
            let columns: Vec<ColumnAlign> = sep_cells
                .iter()
                .map(|s| {
                    let left = s.starts_with(':');
                    let right = s.ends_with(':');
                    match (left, right) {
                        (true, true) => ColumnAlign::Center,
                        (false, true) => ColumnAlign::Right,
                        _ => ColumnAlign::Left,
                    }
                })
                .collect();

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
        if trimmed == "claim" || trimmed.starts_with("claim ") {
            let name = trimmed.strip_prefix("claim").unwrap().trim().to_string();
            if name.is_empty() {
                return Err(ParseDocError {
                    line: i + 1,
                    message: "claim requires a name".into(),
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
                    message: "claim body is empty".into(),
                });
            }

            let claim = parse_claim_body(&name, &body, claim_line)?;
            blocks.push(Block::Claim(claim));
            continue;
        }

        // Proof block
        if trimmed == "proof" || trimmed.starts_with("proof ") {
            let claim_name = trimmed.strip_prefix("proof").unwrap().trim().to_string();
            if claim_name.is_empty() {
                return Err(ParseDocError {
                    line: i + 1,
                    message: "proof requires a claim name".into(),
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
                    step_text
                        .strip_prefix('=')
                        .map(|s| s.trim_start())
                        .unwrap_or(step_text)
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
                    message: "proof requires at least two steps".into(),
                });
            }

            blocks.push(Block::Proof(Proof {
                claim_name,
                steps,
                span: Span { line: proof_line },
            }));
            continue;
        }

        // Expect-fail block: expect_fail name [failure_type]
        if trimmed == "expect_fail" || trimmed.starts_with("expect_fail ") {
            let rest = trimmed.strip_prefix("expect_fail").unwrap().trim();
            if rest.is_empty() {
                return Err(ParseDocError {
                    line: i + 1,
                    message: "expect_fail requires: expect_fail name [type]. Valid types: symbolic, comparison_false, comparison_unknown, dimension_mismatch, dimension_error".into(),
                });
            }
            // Parse: name [type]
            let (name, failure_type) = if let Some(bracket_pos) = rest.find('[') {
                let name = rest[..bracket_pos].trim().to_string();
                let type_str = rest[bracket_pos..].trim();
                // Strip [ and ]
                let inner = type_str
                    .strip_prefix('[')
                    .and_then(|s| s.strip_suffix(']'))
                    .map(|s| s.trim())
                    .ok_or_else(|| ParseDocError {
                        line: i + 1,
                        message: "expect_fail type must be in brackets: [symbolic], [comparison_false], [comparison_unknown], [dimension_mismatch], or [dimension_error]".into(),
                    })?;
                let ft = ExpectFailType::from_str(inner).ok_or_else(|| ParseDocError {
                    line: i + 1,
                    message: format!(
                        "unknown expect_fail type '{}'. Valid types: symbolic, comparison_false, comparison_unknown, dimension_mismatch, dimension_error",
                        inner
                    ),
                })?;
                (name, ft)
            } else {
                return Err(ParseDocError {
                    line: i + 1,
                    message: format!(
                        "expect_fail '{}' missing failure type. Use: expect_fail {} [symbolic|comparison_false|comparison_unknown|dimension_mismatch|dimension_error]",
                        rest, rest
                    ),
                });
            };
            if name.is_empty() {
                return Err(ParseDocError {
                    line: i + 1,
                    message: "expect_fail requires a name before the type bracket".into(),
                });
            }
            let ef_line = i + 1;
            i += 1;

            // Collect indented body lines and dedent them
            let mut body_lines = Vec::new();
            while i < lines.len() && is_continuation(&lines[i]) {
                // Strip the first level of indentation (up to 2 spaces)
                let line = &lines[i];
                let dedented = if line.starts_with("  ") {
                    &line[2..]
                } else {
                    line.trim_start()
                };
                body_lines.push(dedented.to_string());
                i += 1;
            }

            let body_text = body_lines.join("\n");
            let inner_doc = parse_document(&body_text).map_err(|e| ParseDocError {
                line: ef_line + e.line,
                message: e.message,
            })?;

            blocks.push(Block::ExpectFail {
                name,
                failure_type,
                blocks: inner_doc.blocks,
                span: Span { line: ef_line },
            });
            continue;
        }

        // Variable declaration
        if trimmed == "var" || trimmed.starts_with("var ") {
            let rest = trimmed.strip_prefix("var").unwrap().trim();
            // Parse: varname [dim spec]
            let bracket_pos = rest.find('[').ok_or_else(|| ParseDocError {
                line: i + 1,
                message: "var requires a variable name and dimension, e.g. var x [L T^-1]".into(),
            })?;
            let var_name = rest[..bracket_pos].trim().to_string();
            if var_name.is_empty() {
                return Err(ParseDocError {
                    line: i + 1,
                    message: "var requires a variable name".into(),
                });
            }
            let dim_str = rest[bracket_pos..].trim();
            let dimension = Dimension::parse(dim_str).map_err(|e| ParseDocError {
                line: i + 1,
                message: format!("var '{}': {}", var_name, e),
            })?;
            let span = Span { line: i + 1 };
            i += 1;
            let description = collect_description(&lines, &mut i);
            blocks.push(Block::Var(VarDecl {
                var_name,
                dimension,
                description,
                span,
            }));
            continue;
        }

        // Definition: def name [dim] := expr
        if trimmed == "def" || trimmed.starts_with("def ") {
            let rest = trimmed.strip_prefix("def").unwrap().trim();
            let assign_pos = rest.find(":=").ok_or_else(|| ParseDocError {
                line: i + 1,
                message: "def requires name := expr, e.g. def c := 3*10^8".into(),
            })?;
            let before_assign = rest[..assign_pos].trim();
            // Check for optional dimension annotation: `name [dim]`
            let (name, dimension) = if let Some(bracket_pos) = before_assign.find('[') {
                let name = before_assign[..bracket_pos].trim().to_string();
                let dim_str = before_assign[bracket_pos..].trim();
                let dimension =
                    Dimension::parse(dim_str).map_err(|e| ParseDocError {
                        line: i + 1,
                        message: format!("def '{}': {}", name, e),
                    })?;
                (name, Some(dimension))
            } else {
                (before_assign.to_string(), None)
            };
            if name.is_empty() {
                return Err(ParseDocError {
                    line: i + 1,
                    message: "def requires a name".into(),
                });
            }
            let value_str = rest[assign_pos + 2..].trim();
            let value = parse_expr(value_str).map_err(|e| ParseDocError {
                line: i + 1,
                message: format!("def '{}': {:?}", name, e),
            })?;
            let span = Span { line: i + 1 };
            i += 1;
            let description = collect_description(&lines, &mut i);
            blocks.push(Block::Def(DefDecl {
                name,
                dimension,
                value,
                description,
                span,
            }));
            continue;
        }

        // Function declaration
        if trimmed == "func" || trimmed.starts_with("func ") {
            let rest = trimmed.strip_prefix("func").unwrap().trim();
            let lparen = rest.find('(').ok_or_else(|| ParseDocError {
                line: i + 1,
                message: "func requires name(params) := expr, e.g. func KE(m, v) := (1/2)*m*v^2"
                    .into(),
            })?;
            let name = rest[..lparen].trim().to_string();
            if name.is_empty() {
                return Err(ParseDocError {
                    line: i + 1,
                    message: "func requires a name".into(),
                });
            }
            let rparen = rest.find(')').ok_or_else(|| ParseDocError {
                line: i + 1,
                message: "func missing closing parenthesis".into(),
            })?;
            let params_str = &rest[lparen + 1..rparen];
            let params: Vec<String> = params_str
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            if params.is_empty() {
                return Err(ParseDocError {
                    line: i + 1,
                    message: "func requires at least one parameter".into(),
                });
            }
            let after_rparen = rest[rparen + 1..].trim();
            let body_str = after_rparen
                .strip_prefix(":=")
                .ok_or_else(|| ParseDocError {
                    line: i + 1,
                    message: "func requires := after parameters".into(),
                })?
                .trim();
            let body = parse_expr(body_str).map_err(|e| ParseDocError {
                line: i + 1,
                message: format!("func '{}': {:?}", name, e),
            })?;
            let span = Span { line: i + 1 };
            i += 1;
            let description = collect_description(&lines, &mut i);
            blocks.push(Block::Func(FuncDecl {
                name,
                params,
                body,
                description,
                span,
            }));
            continue;
        }

        // Bibliography declaration
        if trimmed.starts_with("!bibliography") {
            let path = trimmed["!bibliography".len()..].trim().to_string();
            if path.is_empty() {
                return Err(ParseDocError {
                    line: i + 1,
                    message: "!bibliography requires a file path".into(),
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
            let quote_line = i + 1;
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
                parse_prose_fragments(&quote_text).map_err(|mut e| {
                    if e.line == 0 {
                        e.line = quote_line;
                    }
                    e
                })?
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

        // Skip !include and use lines (resolved before parsing in parse_document_from_file)
        if trimmed.starts_with("!include")
            || trimmed == "use"
            || trimmed.starts_with("use ")
        {
            i += 1;
            continue;
        }

        // Skip any other unrecognized directive to avoid infinite loop
        if trimmed.starts_with('!') {
            i += 1;
            continue;
        }

        // Prose line — collect consecutive non-special lines into a paragraph
        let mut prose_text = String::new();
        // Map: (char_offset_in_prose_text, 1-based_line_number)
        let mut line_map: Vec<(usize, usize)> = Vec::new();
        while i < lines.len() {
            let l = lines[i].trim();
            if l.is_empty()
                || l.starts_with('#')
                || l.starts_with('!')
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
            line_map.push((prose_text.len(), i + 1));
            prose_text.push_str(l);
            i += 1;
        }
        if !prose_text.is_empty() {
            let fragments = parse_prose_fragments(&prose_text).map_err(|mut e| {
                if e.line == 0 {
                    e.line = resolve_line(&prose_text, &e.message, &line_map);
                }
                e
            })?;
            blocks.push(Block::Prose(fragments));
        }
    }

    Ok(Document { blocks })
}

/// Given a concatenated prose string, an error message, and a line map,
/// find the 1-based line number where the error occurred.
///
/// The error message includes the inline content (e.g., `inline math`EXPR`: ...`).
/// We extract `math`EXPR`` and find its byte offset in the prose text,
/// then map that offset to the original source line.
fn resolve_line(prose_text: &str, error_msg: &str, line_map: &[(usize, usize)]) -> usize {
    // Extract the inline tag from the error message: "inline math`...`:" or "inline tex`...`:"
    let offset = if let Some(start) = error_msg.find("math`") {
        let tag_in_msg = &error_msg[start..];
        // Find in the prose text
        prose_text.find(tag_in_msg.split("`:").next().unwrap_or(tag_in_msg))
    } else {
        None
    };
    if let Some(off) = offset {
        // Find the last line_map entry whose offset <= off
        for &(char_off, line_num) in line_map.iter().rev() {
            if char_off <= off {
                return line_num;
            }
        }
    }
    // Fallback to first line of the block
    line_map.first().map_or(0, |&(_, ln)| ln)
}

/// Extract an optional `label`...`` tag from a section title.
/// Returns (clean_title, optional_label).
/// Also supports legacy `\label{...}` syntax for backward compatibility.
fn extract_section_label(title: &str) -> (String, Option<String>) {
    // Native syntax: label`...`
    if let Some(start) = title.find("label`") {
        let rest = &title[start + 6..];
        if let Some(end) = rest.find('`') {
            let label = rest[..end].to_string();
            let before = title[..start].trim_end();
            let after = rest[end + 1..].trim_start();
            let clean = if after.is_empty() {
                before.to_string()
            } else {
                format!("{} {}", before, after)
            };
            return (clean, Some(label));
        }
    }
    // Legacy: \label{...}
    if let Some(start) = title.find("\\label{") {
        let rest = &title[start + 7..];
        if let Some(end) = rest.find('}') {
            let label = rest[..end].to_string();
            let before = title[..start].trim_end();
            let after = rest[end + 1..].trim_start();
            let clean = if after.is_empty() {
                before.to_string()
            } else {
                format!("{} {}", before, after)
            };
            return (clean, Some(label));
        }
    }
    (title.to_string(), None)
}

/// A continuation line is indented (starts with whitespace).
/// Collect optional indented description lines following a declaration.
/// Returns `None` if there are no continuation lines.
fn collect_description(lines: &[&str], i: &mut usize) -> Option<String> {
    let mut parts = Vec::new();
    while *i < lines.len() && is_continuation(lines[*i]) {
        parts.push(lines[*i].trim());
        *i += 1;
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" "))
    }
}

fn is_continuation(line: &str) -> bool {
    if line.trim().is_empty() {
        return false;
    }
    line.starts_with(' ') || line.starts_with('\t')
}

/// Collect an indented block body, treating blank lines between indented lines
/// as paragraph breaks (Python-style blocking). Returns fragments with
/// `ParBreak` inserted at blank-line boundaries.
fn collect_indented_body(
    lines: &[&str],
    i: &mut usize,
    base_line: usize,
) -> Result<Vec<ProseFragment>, ParseDocError> {
    let mut paragraphs: Vec<String> = Vec::new();
    let mut current = String::new();

    while *i < lines.len() {
        let line = lines[*i];
        if line.trim().is_empty() {
            // Blank line: check if the next non-blank line is indented
            let mut peek = *i + 1;
            while peek < lines.len() && lines[peek].trim().is_empty() {
                peek += 1;
            }
            if peek < lines.len() && is_continuation(&lines[peek]) {
                // Blank line within block — paragraph break
                if !current.is_empty() {
                    paragraphs.push(current);
                    current = String::new();
                }
                *i = peek; // skip blank lines, continue with next indented line
                continue;
            } else {
                // Blank line followed by non-indented or EOF — block ends
                break;
            }
        } else if is_continuation(line) {
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(line.trim());
            *i += 1;
        } else {
            break;
        }
    }

    if !current.is_empty() {
        paragraphs.push(current);
    }

    if paragraphs.is_empty() {
        return Ok(Vec::new());
    }

    let mut fragments = Vec::new();
    for (idx, para) in paragraphs.iter().enumerate() {
        if idx > 0 {
            fragments.push(ProseFragment::ParBreak);
        }
        let mut para_fragments = parse_prose_fragments(para).map_err(|mut e| {
            if e.line == 0 {
                e.line = base_line;
            }
            e
        })?;
        fragments.append(&mut para_fragments);
    }
    Ok(fragments)
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
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('!') {
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
            let item_line = *i + 1;
            let fragments = parse_prose_fragments(content).map_err(|mut e| {
                if e.line == 0 {
                    e.line = item_line;
                }
                e
            })?;
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
        ("!theorem", EnvKind::Theorem),
        ("!lemma", EnvKind::Lemma),
        ("!corollary", EnvKind::Corollary),
        ("!remark", EnvKind::Remark),
        ("!example", EnvKind::Example),
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
        EnvKind::Theorem => "!theorem",
        EnvKind::Lemma => "!lemma",
        EnvKind::Corollary => "!corollary",
        EnvKind::Remark => "!remark",
        EnvKind::Example => "!example",
    }
    .len();
    let title_str = directive_line[prefix_len..].trim();
    let title = if title_str.is_empty() {
        None
    } else {
        Some(title_str.to_string())
    };

    // Collect indented body lines (with paragraph break support)
    *i += 1;
    let body = collect_indented_body(lines, i, span.line)?;

    Ok(Environment {
        kind,
        title,
        body,
        span,
    })
}

/// Parse `lhs = rhs` from a claim body string.
fn parse_table_row(line: &str) -> Result<Vec<Vec<ProseFragment>>, ParseDocError> {
    let cells: Vec<&str> = line
        .split('|')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    cells.iter().map(|c| parse_prose_fragments(c)).collect()
}

fn parse_claim_body(name: &str, body: &str, line: usize) -> Result<Claim, ParseDocError> {
    let (relation, op, op_pos) = find_claim_relation(body).ok_or_else(|| ParseDocError {
        line,
        message: format!(
            "claim '{}': expected one of 'lhs = rhs', 'lhs > rhs', 'lhs >= rhs', 'lhs < rhs', or 'lhs <= rhs'",
            name
        ),
    })?;

    let lhs_str = body[..op_pos].trim();
    let rhs_str = body[op_pos + op.len()..].trim();

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
        relation,
        rhs,
        span: Span { line },
    })
}

fn find_claim_relation(body: &str) -> Option<(ClaimRelation, &'static str, usize)> {
    for (relation, op) in [
        (ClaimRelation::Ge, ">="),
        (ClaimRelation::Le, "<="),
        (ClaimRelation::Eq, "="),
        (ClaimRelation::Gt, ">"),
        (ClaimRelation::Lt, "<"),
    ] {
        if let Some(pos) = body.find(op) {
            return Some((relation, op, pos));
        }
    }
    None
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
    Bold {
        start: usize,
        end: usize,
        content: &'a str,
    },
    Italic {
        start: usize,
        end: usize,
        content: &'a str,
    },
    Footnote {
        start: usize,
        end: usize,
        content: &'a str,
    },
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
/// Returns whichever appears first in the text.
fn find_emphasis(text: &str) -> Option<InlineMatch<'_>> {
    let bold = find_bold(text);
    let italic = find_italic(text);
    match (bold, italic) {
        (Some(b), Some(i)) => {
            if i.start() < b.start() {
                Some(i)
            } else {
                Some(b)
            }
        }
        (Some(b), None) => Some(b),
        (None, i) => i,
    }
}

/// Find the next bold `**...**` or `***...***` marker.
fn find_bold(text: &str) -> Option<InlineMatch<'_>> {
    let open = text.find("**")?;
    // Check for *** (bold+italic)
    if text[open..].starts_with("***") {
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
    let close_offset = text[open + 2..].find("**")?;
    let content = &text[open + 2..open + 2 + close_offset];
    if content.is_empty() {
        return None;
    }
    Some(InlineMatch::Bold {
        start: open,
        end: open + 2 + close_offset + 2,
        content,
    })
}

/// Find the next italic `*...*` marker (single `*`, not part of `**`).
fn find_italic(text: &str) -> Option<InlineMatch<'_>> {
    let mut search_from = 0;
    while search_from < text.len() {
        let open = search_from + text[search_from..].find('*')?;
        // Skip if this is part of a ** sequence
        if open + 1 < text.len() && text.as_bytes()[open + 1] == b'*' {
            search_from = open + 2;
            continue;
        }
        if open > 0 && text.as_bytes()[open - 1] == b'*' {
            search_from = open + 1;
            continue;
        }
        // Find closing * (that isn't part of **)
        let mut close_from = open + 1;
        while close_from < text.len() {
            if let Some(close_offset) = text[close_from..].find('*') {
                let close = close_from + close_offset;
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

    let candidates: Vec<InlineMatch<'_>> = [tag, emph, foot].into_iter().flatten().collect();

    candidates.into_iter().min_by_key(|c| c.start())
}

/// Parse prose text into fragments, extracting tagged inline expressions.
/// Supports: math`expr`, tex`raw latex`, claim`name`, **bold**, *italic*
pub fn parse_prose_fragments(text: &str) -> Result<Vec<ProseFragment>, ParseDocError> {
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
                            if let Some(eq_pos) = tag_match.content.find('=') {
                                let lhs_str = tag_match.content[..eq_pos].trim();
                                let rhs_str = tag_match.content[eq_pos + 1..].trim();
                                let lhs =
                                    parse_expr(lhs_str).map_err(|e| ParseDocError {
                                        line: 0,
                                        message: format!("inline math`{}`: lhs: {:?}", tag_match.content, e),
                                    })?;
                                let rhs =
                                    parse_expr(rhs_str).map_err(|e| ParseDocError {
                                        line: 0,
                                        message: format!("inline math`{}`: rhs: {:?}", tag_match.content, e),
                                    })?;
                                fragments.push(ProseFragment::MathEquality(lhs, rhs));
                            } else {
                                let expr =
                                    parse_expr(tag_match.content).map_err(|e| ParseDocError {
                                        line: 0,
                                        message: format!("inline math`{}`: {:?}", tag_match.content, e),
                                    })?;
                                fragments.push(ProseFragment::Math(expr));
                            }
                        }
                        "tex" => {
                            fragments.push(ProseFragment::Tex(tag_match.content.to_string()));
                        }
                        "claim" => {
                            fragments.push(ProseFragment::ClaimRef(tag_match.content.to_string()));
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
                            let (label, display) = if let Some(pipe) = tag_match.content.find('|') {
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
                            let (url, display) = if let Some(pipe) = tag_match.content.find('|') {
                                (
                                    tag_match.content[..pipe].to_string(),
                                    Some(tag_match.content[pipe + 1..].to_string()),
                                )
                            } else {
                                (tag_match.content.to_string(), None)
                            };
                            fragments.push(ProseFragment::Url { url, display });
                        }
                        "sym" => {
                            let (name, display) = if let Some(pipe) = tag_match.content.find('|') {
                                (
                                    tag_match.content[..pipe].to_string(),
                                    Some(tag_match.content[pipe + 1..].to_string()),
                                )
                            } else {
                                (tag_match.content.to_string(), None)
                            };
                            fragments.push(ProseFragment::Sym { name, display });
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
    let tags = ["math", "tex", "claim", "cite", "ref", "url", "sym"];

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
            ProseFragment::Math(_) | ProseFragment::MathEquality(_, _) => s.push_str("[math]"),
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
            ProseFragment::Sym { name, display } => {
                s.push_str("sym`");
                s.push_str(name);
                if let Some(d) = display {
                    s.push('|');
                    s.push_str(d);
                }
                s.push('`');
            }
            ProseFragment::ParBreak => {
                s.push_str("\n\n");
            }
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "verso-{prefix}-{}-{}",
            std::process::id(),
            id
        ))
    }

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
        let doc = parse_document("See tex`\\vec{v}` and claim`pythagorean` for details.").unwrap();
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
        let src = "claim identity\n  x + 0 = x";
        let doc = parse_document(src).unwrap();
        assert_eq!(doc.blocks.len(), 1);
        match &doc.blocks[0] {
            Block::Claim(c) => {
                assert_eq!(c.name, "identity");
                assert_eq!(c.relation, ClaimRelation::Eq);
            }
            _ => panic!("expected Claim"),
        }
    }

    #[test]
    fn parse_claim_missing_equals() {
        let src = "claim bad\n  x + 0";
        let err = parse_document(src).unwrap_err();
        assert!(err.message.contains("expected one of"));
    }

    #[test]
    fn parse_greater_than_claim() {
        let src = "claim threshold\n  x > 1";
        let doc = parse_document(src).unwrap();
        match &doc.blocks[0] {
            Block::Claim(c) => assert_eq!(c.relation, ClaimRelation::Gt),
            _ => panic!("expected Claim"),
        }
    }

    #[test]
    fn parse_greater_equal_claim() {
        let src = "claim threshold\n  x >= 1";
        let doc = parse_document(src).unwrap();
        match &doc.blocks[0] {
            Block::Claim(c) => assert_eq!(c.relation, ClaimRelation::Ge),
            _ => panic!("expected Claim"),
        }
    }

    #[test]
    fn parse_less_than_claim() {
        let src = "claim threshold\n  x < 1";
        let doc = parse_document(src).unwrap();
        match &doc.blocks[0] {
            Block::Claim(c) => assert_eq!(c.relation, ClaimRelation::Lt),
            _ => panic!("expected Claim"),
        }
    }

    #[test]
    fn parse_less_equal_claim() {
        let src = "claim threshold\n  x <= 1";
        let doc = parse_document(src).unwrap();
        match &doc.blocks[0] {
            Block::Claim(c) => assert_eq!(c.relation, ClaimRelation::Le),
            _ => panic!("expected Claim"),
        }
    }

    #[test]
    fn parse_claim_missing_name() {
        let src = "claim\n  x = x";
        let err = parse_document(src).unwrap_err();
        assert!(err.message.contains("requires a name"));
    }

    #[test]
    fn parse_proof() {
        let src = "\
proof pythag
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
        let src = "proof foo\n  x";
        let err = parse_document(src).unwrap_err();
        assert!(err.message.contains("at least two steps"));
    }

    #[test]
    fn parse_proof_missing_name() {
        let src = "proof\n  x\n  = y";
        let err = parse_document(src).unwrap_err();
        assert!(err.message.contains("requires a claim name"));
    }

    #[test]
    fn parse_var_declaration() {
        let src = "var x [L]\nvar v [L T^-1]";
        let doc = parse_document(src).unwrap();
        assert_eq!(doc.blocks.len(), 2);
        match &doc.blocks[0] {
            Block::Var(d) => {
                assert_eq!(d.var_name, "x");
                assert_eq!(d.dimension.to_string(), "[L]");
            }
            _ => panic!("expected Var"),
        }
        match &doc.blocks[1] {
            Block::Var(d) => {
                assert_eq!(d.var_name, "v");
                assert_eq!(d.dimension.to_string(), "[L T^-1]");
            }
            _ => panic!("expected Var"),
        }
    }

    #[test]
    fn parse_var_with_description() {
        let src = "var σ[1]\n  Rung scaling factor.\n  Dimensionless ratio.";
        let doc = parse_document(src).unwrap();
        assert_eq!(doc.blocks.len(), 1);
        match &doc.blocks[0] {
            Block::Var(d) => {
                assert_eq!(d.var_name, "σ");
                assert_eq!(
                    d.description.as_deref(),
                    Some("Rung scaling factor. Dimensionless ratio.")
                );
            }
            _ => panic!("expected Var"),
        }
    }

    #[test]
    fn parse_var_no_description() {
        let src = "var x [L]\n\nSome prose.";
        let doc = parse_document(src).unwrap();
        match &doc.blocks[0] {
            Block::Var(d) => {
                assert!(d.description.is_none());
            }
            _ => panic!("expected Var"),
        }
    }

    #[test]
    fn parse_def_with_description() {
        let src = "def N := 3\n  Number of rungs.";
        let doc = parse_document(src).unwrap();
        match &doc.blocks[0] {
            Block::Def(d) => {
                assert_eq!(d.description.as_deref(), Some("Number of rungs."));
            }
            _ => panic!("expected Def"),
        }
    }

    #[test]
    fn parse_func_with_description() {
        let src = "func sq(x) := x^2\n  Square function.";
        let doc = parse_document(src).unwrap();
        match &doc.blocks[0] {
            Block::Func(f) => {
                assert_eq!(f.description.as_deref(), Some("Square function."));
            }
            _ => panic!("expected Func"),
        }
    }

    #[test]
    fn parse_var_missing_brackets() {
        let src = "var xL";
        let err = parse_document(src).unwrap_err();
        assert!(
            err.message.contains("var requires"),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn parse_def_declaration() {
        let src = "def c := 3*10^8";
        let doc = parse_document(src).unwrap();
        assert_eq!(doc.blocks.len(), 1);
        match &doc.blocks[0] {
            Block::Def(d) => {
                assert_eq!(d.name, "c");
                let formatted = format!("{}", d.value);
                assert!(
                    formatted.contains("10"),
                    "expected numeric expr, got: {}",
                    formatted
                );
            }
            _ => panic!("expected Def"),
        }
    }

    #[test]
    fn parse_def_with_dimension() {
        let src = "def c_{s} [L T^-1] := sqrt(μ / ρ_{0})";
        let doc = parse_document(src).unwrap();
        match &doc.blocks[0] {
            Block::Def(d) => {
                assert_eq!(d.name, "c_{s}");
                assert!(d.dimension.is_some(), "expected dimension annotation");
                let dim = d.dimension.as_ref().unwrap();
                assert_eq!(*dim, Dimension::parse("[L T^-1]").unwrap());
            }
            _ => panic!("expected Def"),
        }
    }

    #[test]
    fn parse_def_without_dimension() {
        let src = "def c := 3*10^8";
        let doc = parse_document(src).unwrap();
        match &doc.blocks[0] {
            Block::Def(d) => {
                assert!(d.dimension.is_none());
            }
            _ => panic!("expected Def"),
        }
    }

    #[test]
    fn parse_def_missing_assign() {
        let src = "def c 3";
        let err = parse_document(src).unwrap_err();
        assert!(
            err.message.contains("def requires"),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn parse_def_missing_name() {
        let src = "def := 3";
        let err = parse_document(src).unwrap_err();
        assert!(
            err.message.contains("def requires a name"),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn parse_func_declaration() {
        let src = "func KE(m, v) := (1/2) * m * v^2";
        let doc = parse_document(src).unwrap();
        assert_eq!(doc.blocks.len(), 1);
        match &doc.blocks[0] {
            Block::Func(f) => {
                assert_eq!(f.name, "KE");
                assert_eq!(f.params, vec!["m", "v"]);
            }
            _ => panic!("expected Func"),
        }
    }

    #[test]
    fn parse_func_missing_parens() {
        let src = "func f := x";
        let err = parse_document(src).unwrap_err();
        assert!(
            err.message.contains("func requires"),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn parse_func_no_params() {
        let src = "func f() := x";
        let err = parse_document(src).unwrap_err();
        assert!(
            err.message.contains("at least one parameter"),
            "unexpected error: {}",
            err.message
        );
    }

    #[test]
    fn parse_mixed_document() {
        let src = "\
# Algebra

Some introductory text.

claim add_zero
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
    fn parse_italic_before_bold() {
        // Italic *seems* appears before bold **energy density** — both must render
        let doc =
            parse_document("gravity *seems* negligible but **energy density** matters").unwrap();
        match &doc.blocks[0] {
            Block::Prose(fragments) => {
                let has_italic = fragments
                    .iter()
                    .any(|f| matches!(f, ProseFragment::Italic(_)));
                let has_bold = fragments
                    .iter()
                    .any(|f| matches!(f, ProseFragment::Bold(_)));
                assert!(
                    has_italic,
                    "expected italic *seems*, fragments: {:?}",
                    fragments
                );
                assert!(
                    has_bold,
                    "expected bold **energy density**, fragments: {:?}",
                    fragments
                );
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
                assert!(
                    matches!(&list.items[0].fragments[0], ProseFragment::Text(t) if t == "First")
                );
                assert!(
                    matches!(&list.items[1].fragments[0], ProseFragment::Text(t) if t == "Second")
                );
                assert!(
                    matches!(&list.items[2].fragments[0], ProseFragment::Text(t) if t == "Third")
                );
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
                assert!(
                    matches!(&list.items[0].fragments[0], ProseFragment::Text(t) if t == "Alpha")
                );
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
                assert!(
                    matches!(&list.items[0].fragments[0], ProseFragment::Text(t) if t == "Outer")
                );
                let children = list.items[0]
                    .children
                    .as_ref()
                    .expect("expected nested list");
                assert_eq!(children.items.len(), 2);
                assert!(
                    matches!(&children.items[0].fragments[0], ProseFragment::Text(t) if t == "Inner A")
                );
                assert!(
                    matches!(&list.items[1].fragments[0], ProseFragment::Text(t) if t == "Back")
                );
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
                assert!(
                    matches!(&list.items[0].fragments[0], ProseFragment::Text(t) if t == "Energy: "),
                    "got {:?}",
                    list.items[0].fragments[0]
                );
                assert!(matches!(
                    &list.items[0].fragments[1],
                    ProseFragment::Math(_)
                ));
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
        let src = "- Item one\nclaim foo\n  x = x";
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
        let src = "```math\nx + 1\n```\n\nclaim foo\n  x + 0 = x";
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
            Block::Prose(fragments) => match &fragments[1] {
                ProseFragment::Cite(keys) => {
                    assert_eq!(keys, &["einstein1905", "dirac1928"]);
                }
                other => panic!("expected Cite, got {:?}", other),
            },
            _ => panic!("expected Prose"),
        }
    }

    #[test]
    fn parse_bibliography() {
        let doc = parse_document("!bibliography refs.bib").unwrap();
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
        let err = parse_document("!bibliography").unwrap_err();
        assert!(err.message.contains("requires a file path"));
    }

    #[test]
    fn parse_theorem_with_title() {
        let src = "!theorem Pythagorean\n  For any right triangle, math`a^2 + b^2` is important.";
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
    fn parse_theorem_with_paragraph_break() {
        let src = "!theorem Main Result\n  First paragraph.\n\n  Second paragraph.";
        let doc = parse_document(src).unwrap();
        match &doc.blocks[0] {
            Block::Environment(env) => {
                assert!(
                    env.body
                        .iter()
                        .any(|f| matches!(f, ProseFragment::ParBreak)),
                    "expected ParBreak in theorem body: {:?}",
                    env.body
                );
            }
            other => panic!("expected Environment, got {:?}", other),
        }
    }

    #[test]
    fn parse_all_env_kinds() {
        for (directive, expected_kind) in &[
            ("!theorem", EnvKind::Theorem),
            ("!lemma", EnvKind::Lemma),
            ("!corollary", EnvKind::Corollary),
            ("!remark", EnvKind::Remark),
            ("!example", EnvKind::Example),
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
        let src = "!lemma\n  If math`x` is positive then math`x^2` is positive.";
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
    fn parse_inline_math_equality() {
        let doc = parse_document("We define math`a = b + c` here.").unwrap();
        match &doc.blocks[0] {
            Block::Prose(fragments) => {
                assert!(matches!(&fragments[1], ProseFragment::MathEquality(_, _)));
            }
            other => panic!("expected Prose, got {:?}", other),
        }
    }

    #[test]
    fn parse_env_multiline_body() {
        let src = "!remark Important\n  First line.\n  Second line.";
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
        let src = "!theorem Test\n  Body.\n\nclaim foo\n  x = x";
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
                assert!(fragments
                    .iter()
                    .any(|f| matches!(f, ProseFragment::Bold(_))));
                assert!(fragments
                    .iter()
                    .any(|f| matches!(f, ProseFragment::Math(_))));
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
                assert!(
                    matches!(&fragments[0], ProseFragment::Text(t) if t == "This is surprising")
                );
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
            Block::Prose(fragments) => match &fragments[1] {
                ProseFragment::Footnote(inner) => {
                    assert!(inner.iter().any(|f| matches!(f, ProseFragment::Math(_))));
                }
                other => panic!("expected Footnote, got {:?}", other),
            },
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
        let doc = parse_document("ref`earth-and-the-solar-system|Hydrogen creation`").unwrap();
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
        let doc = parse_document("!title My Great Paper").unwrap();
        assert!(matches!(&doc.blocks[0], Block::Title(t) if t == &["My Great Paper"]));
    }

    #[test]
    fn parse_title_multiline() {
        let src = "!title\n\tThe Two-Medium Model:\n\tA Narrative Framework";
        let doc = parse_document(src).unwrap();
        match &doc.blocks[0] {
            Block::Title(lines) => {
                assert_eq!(lines, &["The Two-Medium Model:", "A Narrative Framework"]);
            }
            other => panic!("expected Title, got {:?}", other),
        }
    }

    #[test]
    fn parse_author() {
        let doc = parse_document("!author Alice Smith").unwrap();
        assert!(matches!(&doc.blocks[0], Block::Author(a) if a == "Alice Smith"));
    }

    #[test]
    fn parse_multiple_authors() {
        let doc = parse_document("!author Alice Smith\n!author Bob Jones").unwrap();
        assert_eq!(doc.blocks.len(), 2);
        assert!(matches!(&doc.blocks[0], Block::Author(a) if a == "Alice Smith"));
        assert!(matches!(&doc.blocks[1], Block::Author(a) if a == "Bob Jones"));
    }

    #[test]
    fn parse_date() {
        let doc = parse_document("!date 2026-03-13").unwrap();
        assert!(matches!(&doc.blocks[0], Block::Date(Some(d)) if d == "2026-03-13"));
    }

    #[test]
    fn parse_date_no_value_defaults_to_none() {
        let doc = parse_document("!date").unwrap();
        assert!(matches!(&doc.blocks[0], Block::Date(None)));
    }

    #[test]
    fn parse_abstract() {
        let src = "!abstract\n  We present a novel approach\n  to computing corrections.";
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
    fn parse_abstract_with_paragraph_break() {
        let src = "!abstract\n  First paragraph.\n\n  Second paragraph.";
        let doc = parse_document(src).unwrap();
        match &doc.blocks[0] {
            Block::Abstract(fragments) => {
                assert!(
                    fragments
                        .iter()
                        .any(|f| matches!(f, ProseFragment::ParBreak)),
                    "expected ParBreak in abstract fragments: {:?}",
                    fragments
                );
            }
            other => panic!("expected Abstract, got {:?}", other),
        }
    }

    #[test]
    fn parse_abstract_ends_on_outdent() {
        let src = "!abstract\n  First paragraph.\n\n  Second paragraph.\nNot abstract.";
        let doc = parse_document(src).unwrap();
        assert_eq!(doc.blocks.len(), 2);
        assert!(matches!(&doc.blocks[0], Block::Abstract(_)));
        assert!(matches!(&doc.blocks[1], Block::Prose(_)));
    }

    #[test]
    fn parse_abstract_with_inline_math() {
        let src = "!abstract\n  We study math`x^2 + 1` in detail.";
        let doc = parse_document(src).unwrap();
        match &doc.blocks[0] {
            Block::Abstract(fragments) => {
                assert!(fragments
                    .iter()
                    .any(|f| matches!(f, ProseFragment::Math(_))));
            }
            other => panic!("expected Abstract, got {:?}", other),
        }
    }

    // Center

    #[test]
    fn parse_center() {
        let src = "!center\n\tSome centered text.";
        let doc = parse_document(src).unwrap();
        match &doc.blocks[0] {
            Block::Center(fragments) => {
                assert!(fragments
                    .iter()
                    .any(|f| matches!(f, ProseFragment::Text(_))));
            }
            other => panic!("expected Center, got {:?}", other),
        }
    }

    #[test]
    fn parse_center_with_paragraph_break() {
        let src = "!center\n\tFirst line.\n\n\tSecond line.";
        let doc = parse_document(src).unwrap();
        assert_eq!(doc.blocks.len(), 1);
        match &doc.blocks[0] {
            Block::Center(fragments) => {
                assert!(
                    fragments
                        .iter()
                        .any(|f| matches!(f, ProseFragment::ParBreak)),
                    "expected ParBreak in center fragments: {:?}",
                    fragments
                );
            }
            other => panic!("expected Center, got {:?}", other),
        }
    }

    #[test]
    fn parse_center_ends_on_outdent() {
        let src = "!center\n\tCentered.\nNot centered.";
        let doc = parse_document(src).unwrap();
        assert_eq!(doc.blocks.len(), 2);
        assert!(matches!(&doc.blocks[0], Block::Center(_)));
        assert!(matches!(&doc.blocks[1], Block::Prose(_)));
    }

    #[test]
    fn parse_title_empty_error() {
        let err = parse_document("!title\n\nSome body.").unwrap_err();
        assert!(err.message.contains("!title requires"));
    }

    // Figures

    #[test]
    fn parse_figure_basic() {
        let src = "!figure plots/energy.pdf\n  caption: Energy levels.\n  label: fig-energy\n  width: 0.8";
        let doc = parse_document(src).unwrap();
        match &doc.blocks[0] {
            Block::Figure(fig) => {
                assert_eq!(fig.path, "plots/energy.pdf");
                assert_eq!(
                    prose_to_string(fig.caption.as_ref().unwrap()),
                    "Energy levels."
                );
                assert_eq!(fig.label.as_deref(), Some("fig-energy"));
                assert!((fig.width - 0.8).abs() < 1e-6);
            }
            other => panic!("expected Figure, got {:?}", other),
        }
    }

    #[test]
    fn parse_figure_path_only() {
        let doc = parse_document("!figure img.png").unwrap();
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
        let err = parse_document("!figure").unwrap_err();
        assert!(err.message.contains("!figure requires"));
    }

    #[test]
    fn parse_figure_caption_with_math() {
        let src = "!figure plot.pdf\n  caption: Energy math`mc^2` shown.";
        let doc = parse_document(src).unwrap();
        match &doc.blocks[0] {
            Block::Figure(fig) => {
                let cap = fig.caption.as_ref().unwrap();
                assert!(cap.iter().any(|f| matches!(f, ProseFragment::Math(_))));
            }
            other => panic!("expected Figure, got {:?}", other),
        }
    }

    // Table of contents

    #[test]
    fn parse_toc() {
        let doc = parse_document("!toc").unwrap();
        assert!(matches!(&doc.blocks[0], Block::Toc));
    }

    // URLs

    #[test]
    fn parse_url_plain() {
        let doc = parse_document("See url`https://example.com` for info.").unwrap();
        match &doc.blocks[0] {
            Block::Prose(fragments) => match &fragments[1] {
                ProseFragment::Url { url, display } => {
                    assert_eq!(url, "https://example.com");
                    assert!(display.is_none());
                }
                other => panic!("expected Url, got {:?}", other),
            },
            _ => panic!("expected Prose"),
        }
    }

    #[test]
    fn parse_url_with_display() {
        let doc = parse_document("Click url`https://example.com|here`.").unwrap();
        match &doc.blocks[0] {
            Block::Prose(fragments) => match &fragments[1] {
                ProseFragment::Url { url, display } => {
                    assert_eq!(url, "https://example.com");
                    assert_eq!(display.as_deref(), Some("here"));
                }
                other => panic!("expected Url, got {:?}", other),
            },
            _ => panic!("expected Prose"),
        }
    }

    // Use (symbol-only imports)

    #[test]
    fn use_imports_declarations_only() {
        let dir = unique_temp_dir("use-basic");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(
            dir.join("main.verso"),
            "use notation.verso\n\nclaim test\n  c = 3*10^8",
        )
        .unwrap();
        std::fs::write(
            dir.join("notation.verso"),
            "# Notation\n\nvar x [L]\n\ndef c := 3*10^8\n\nSome prose paragraph.\n\nclaim foo\n  x = x",
        )
        .unwrap();
        let doc = parse_document_from_file(&dir.join("main.verso")).unwrap();
        // Should only get var + def from notation.verso, then claim from main.verso
        // No section, prose, or claim from notation.verso
        let mut has_var = false;
        let mut has_def = false;
        let mut has_claim = false;
        let mut has_section = false;
        let mut has_prose = false;
        for block in &doc.blocks {
            match block {
                Block::Var(_) => has_var = true,
                Block::Def(_) => has_def = true,
                Block::Claim(c) => {
                    assert_eq!(c.name, "test", "only main.verso claim should be present");
                    has_claim = true;
                }
                Block::Section { .. } => has_section = true,
                Block::Prose(_) => has_prose = true,
                _ => {}
            }
        }
        assert!(has_var, "var from notation.verso should be imported");
        assert!(has_def, "def from notation.verso should be imported");
        assert!(has_claim, "claim from main.verso should be present");
        assert!(!has_section, "section from notation.verso should NOT be imported");
        assert!(!has_prose, "prose from notation.verso should NOT be imported");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn use_imports_descriptions() {
        let dir = unique_temp_dir("use-descriptions");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join("main.verso"), "use defs.verso").unwrap();
        std::fs::write(
            dir.join("defs.verso"),
            "var v [L T^-1]\n  Velocity.\n\ndef c := 3*10^8\n  Speed of light.",
        )
        .unwrap();
        let doc = parse_document_from_file(&dir.join("main.verso")).unwrap();
        // Descriptions should come through
        match &doc.blocks[0] {
            Block::Var(decl) => {
                assert_eq!(decl.var_name, "v");
                assert_eq!(decl.description.as_deref(), Some("Velocity."));
            }
            other => panic!("expected Var, got {:?}", other),
        }
        match &doc.blocks[1] {
            Block::Def(decl) => {
                assert_eq!(decl.name, "c");
                assert_eq!(decl.description.as_deref(), Some("Speed of light."));
            }
            other => panic!("expected Def, got {:?}", other),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn use_tracks_dependencies() {
        let dir = unique_temp_dir("use-deps");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join("main.verso"), "use notation.verso").unwrap();
        std::fs::write(dir.join("notation.verso"), "var x [L]").unwrap();
        let deps = collect_dependencies(&dir.join("main.verso")).unwrap();
        assert_eq!(deps.len(), 2, "should track both main and notation files");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn use_missing_file_error() {
        let dir = unique_temp_dir("use-missing");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join("main.verso"), "use nonexistent.verso").unwrap();
        let err = parse_document_from_file(&dir.join("main.verso")).unwrap_err();
        assert!(err.message.contains("nonexistent.verso"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn use_empty_path_error() {
        let dir = unique_temp_dir("use-empty");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join("main.verso"), "use").unwrap();
        let err = parse_document_from_file(&dir.join("main.verso")).unwrap_err();
        assert!(err.message.contains("use requires a file path"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn use_imports_func() {
        let dir = unique_temp_dir("use-func");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join("main.verso"), "use funcs.verso").unwrap();
        std::fs::write(dir.join("funcs.verso"), "func KE(m, v) := (1/2)*m*v^2").unwrap();
        let doc = parse_document_from_file(&dir.join("main.verso")).unwrap();
        assert_eq!(doc.blocks.len(), 1);
        match &doc.blocks[0] {
            Block::Func(f) => assert_eq!(f.name, "KE"),
            other => panic!("expected Func, got {:?}", other),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    // Expect fail

    #[test]
    fn parse_expect_fail_basic() {
        let src = "expect_fail bad_dim [dimension_mismatch]\n  var v [L T^-1]\n  var a [L T^-2]\n  claim bad\n    v = a";
        let doc = parse_document(src).unwrap();
        assert_eq!(doc.blocks.len(), 1);
        match &doc.blocks[0] {
            Block::ExpectFail {
                name,
                failure_type,
                blocks,
                ..
            } => {
                assert_eq!(name, "bad_dim");
                assert_eq!(*failure_type, ExpectFailType::DimensionMismatch);
                assert_eq!(blocks.len(), 3); // var, var, claim
            }
            other => panic!("expected ExpectFail, got {:?}", other),
        }
    }

    #[test]
    fn parse_expect_fail_missing_name_and_type() {
        let src = "expect_fail\n  claim bad\n    x = y";
        let err = parse_document(src).unwrap_err();
        assert!(err.message.contains("expect_fail requires"));
    }

    #[test]
    fn parse_expect_fail_missing_type() {
        let src = "expect_fail my_test\n  claim bad\n    x = y";
        let err = parse_document(src).unwrap_err();
        assert!(err.message.contains("missing failure type"));
    }

    #[test]
    fn parse_expect_fail_unknown_type() {
        let src = "expect_fail my_test [bogus]\n  claim bad\n    x = y";
        let err = parse_document(src).unwrap_err();
        assert!(err.message.contains("unknown expect_fail type"));
    }

    // Page breaks

    #[test]
    fn parse_pagebreak() {
        let doc = parse_document("Text.\n\n!pagebreak\n\nMore text.").unwrap();
        assert_eq!(doc.blocks.len(), 3);
        assert!(matches!(&doc.blocks[1], Block::PageBreak));
    }

    // Includes

    #[test]
    fn parse_include_basic() {
        let dir = unique_temp_dir("include-basic");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(
            dir.join("main.verso"),
            "# Main\n\n!include sub.verso\n\nEnd.",
        )
        .unwrap();
        std::fs::write(dir.join("sub.verso"), "Sub content.").unwrap();
        let doc = parse_document_from_file(&dir.join("main.verso")).unwrap();
        // Should have: Section, Prose("Sub content."), Prose("End.")
        assert!(doc.blocks.len() >= 3);
        assert!(matches!(&doc.blocks[0], Block::Section { .. }));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_include_circular_error() {
        let dir = unique_temp_dir("include-circular");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join("a.verso"), "!include b.verso").unwrap();
        std::fs::write(dir.join("b.verso"), "!include a.verso").unwrap();
        let err = parse_document_from_file(&dir.join("a.verso")).unwrap_err();
        assert!(err.message.contains("circular"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_include_missing_file_error() {
        let dir = unique_temp_dir("include-missing");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join("main.verso"), "!include nonexistent.verso").unwrap();
        let err = parse_document_from_file(&dir.join("main.verso")).unwrap_err();
        assert!(err.message.contains("nonexistent.verso"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_include_nested() {
        let dir = unique_temp_dir("include-nested");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(dir.join("sub"));
        std::fs::write(dir.join("main.verso"), "!include sub/a.verso").unwrap();
        std::fs::write(dir.join("sub/a.verso"), "!include b.verso").unwrap();
        std::fs::write(dir.join("sub/b.verso"), "Nested content.").unwrap();
        let doc = parse_document_from_file(&dir.join("main.verso")).unwrap();
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
        let src = "!table Results\n  | A | B |\n  |---|---|\n  | 1 | 2 |";
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
        let src = "!table\n  | L | C | R |\n  |:--|:--:|--:|\n  | a | b | c |";
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
        let src = "!table T\n  | X |\n  |---|\n  | 1 |\n  label: tab-x";
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
        let src = "!table\n  | Expr |\n  |------|\n  | math`x^2` |";
        let doc = parse_document(src).unwrap();
        match &doc.blocks[0] {
            Block::Table(table) => {
                assert!(table.rows[0][0]
                    .iter()
                    .any(|f| matches!(f, ProseFragment::Math(_))));
            }
            other => panic!("expected Table, got {:?}", other),
        }
    }

    #[test]
    fn parse_table_missing_separator_error() {
        let err = parse_document("!table T\n  | A |\n  | 1 |").unwrap_err();
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

    #[test]
    fn parse_inline_math_with_dimension() {
        // math`v [L T^-1]` should parse as a Var with dimension annotation
        let doc = parse_document("The velocity math`v [L T^-1]` is measured.").unwrap();
        match &doc.blocks[0] {
            Block::Prose(fragments) => {
                assert_eq!(fragments.len(), 3);
                match &fragments[1] {
                    ProseFragment::Math(expr) => match &expr.kind {
                        verso_symbolic::ExprKind::Var { name, dim, .. } => {
                            assert_eq!(name, "v");
                            assert!(dim.is_some());
                            assert_eq!(
                                dim.as_ref().unwrap(),
                                &verso_symbolic::Dimension::parse("[L T^-1]").unwrap()
                            );
                        }
                        other => panic!("expected Var with dim, got {:?}", other),
                    },
                    other => panic!("expected Math, got {:?}", other),
                }
            }
            _ => panic!("expected Prose"),
        }
    }

    #[test]
    fn parse_inline_math_with_unit() {
        // math`3 [m]` should parse as a Quantity
        let doc = parse_document("A distance of math`3 [m]` was observed.").unwrap();
        match &doc.blocks[0] {
            Block::Prose(fragments) => {
                assert_eq!(fragments.len(), 3);
                match &fragments[1] {
                    ProseFragment::Math(expr) => {
                        assert!(
                            matches!(&expr.kind, verso_symbolic::ExprKind::Quantity(_, _)),
                            "expected Quantity, got {:?}",
                            expr
                        );
                    }
                    other => panic!("expected Math, got {:?}", other),
                }
            }
            _ => panic!("expected Prose"),
        }
    }

    #[test]
    fn parse_inline_math_compound_unit() {
        // math`3*10^8 [m/s]` should parse as a Quantity
        let doc = parse_document("The speed of light is math`3*10^8 [m/s]`.").unwrap();
        match &doc.blocks[0] {
            Block::Prose(fragments) => match &fragments[1] {
                ProseFragment::Math(expr) => {
                    assert!(
                        matches!(&expr.kind, verso_symbolic::ExprKind::Quantity(_, _)),
                        "expected Quantity, got {:?}",
                        expr
                    );
                }
                other => panic!("expected Math, got {:?}", other),
            },
            _ => panic!("expected Prose"),
        }
    }

    #[test]
    fn parse_inline_math_var_with_unit_is_error() {
        // math`c [m/s]` should fail: variable with unit
        let result = parse_document("Value is math`c [m/s]`.");
        assert!(result.is_err());
    }

    #[test]
    fn parse_inline_math_unknown_unit_is_error() {
        let result = parse_document("about math`3000 [kg/zm^3]`.");
        assert!(result.is_err());
    }

    #[test]
    fn parse_inline_math_error_reports_correct_line() {
        // Error in inline math on line 3 of a multi-line prose block
        let src =
            "First line of prose.\nSecond line continues.\nThird has math`3000 [kg/zm^3]` here.";
        let err = parse_document(src).unwrap_err();
        assert_eq!(
            err.line, 3,
            "error should point to line 3, got {}",
            err.line
        );
    }

    #[test]
    fn parse_claim_with_inline_dim() {
        // Claim using inline dimension annotations on variables
        let src = "var F [M L T^-2]\nclaim newton\n  F = m [M] * a [L T^-2]";
        let doc = parse_document(src).unwrap();
        // Should parse without error — the claim body has inline dimensions
        assert_eq!(doc.blocks.len(), 2); // Var + Claim
    }

    #[test]
    fn parse_claim_with_quantity() {
        // Claim using a quantity (numeric with unit)
        let src = "claim speed_of_light\n  c [L T^-1] = 3*10^8 [m/s]";
        let doc = parse_document(src).unwrap();
        assert_eq!(doc.blocks.len(), 1); // just the Claim (no var needed)
    }

    // --- Error paths ---

    #[test]
    fn parse_author_missing_name() {
        let err = parse_document("!author").unwrap_err();
        assert!(err.message.contains("requires a name"));
    }

    #[test]
    fn parse_claim_empty_body() {
        let err = parse_document("claim foo\n\nSome prose").unwrap_err();
        assert!(err.message.contains("body is empty"));
    }

    #[test]
    fn parse_proof_requires_name() {
        let err = parse_document("proof\n  x\n  x").unwrap_err();
        assert!(err.message.contains("requires a claim name"));
    }

    #[test]
    fn parse_var_missing_name() {
        let err = parse_document("var [L]").unwrap_err();
        assert!(err.message.contains("requires a variable name"));
    }

    #[test]
    fn parse_def_empty_name() {
        let err = parse_document("def := 5").unwrap_err();
        assert!(err.message.contains("requires a name"));
    }

    #[test]
    fn parse_func_missing_closing_paren() {
        let err = parse_document("func f(x = x + 1").unwrap_err();
        assert!(err.message.contains("closing parenthesis"));
    }

    #[test]
    fn parse_func_missing_eq() {
        let err = parse_document("func f(x) x + 1").unwrap_err();
        assert!(err.message.contains("="));
    }

    #[test]
    fn parse_func_empty_name() {
        let err = parse_document("func (x) := x").unwrap_err();
        assert!(err.message.contains("requires a name"));
    }

    #[test]
    fn parse_table_bad_separator() {
        let src = "!table Test\n  | A | B |\n  | x | y |";
        let err = parse_document(src).unwrap_err();
        assert!(err.message.contains("separator"));
    }

    #[test]
    fn parse_table_too_few_rows() {
        let src = "!table Test\n  | A | B |";
        let err = parse_document(src).unwrap_err();
        assert!(err.message.contains("header row and separator"));
    }

    #[test]
    fn parse_unknown_tag_becomes_text() {
        // Unknown backtick tag should be kept as text
        let doc = parse_document("See foo`bar` here.").unwrap();
        match &doc.blocks[0] {
            Block::Prose(fragments) => {
                let text: String = fragments
                    .iter()
                    .map(|f| match f {
                        ProseFragment::Text(t) => t.as_str(),
                        _ => "",
                    })
                    .collect();
                assert!(text.contains("foo`bar`"));
            }
            other => panic!("expected Prose, got {:?}", other),
        }
    }

    #[test]
    fn parse_doc_error_display() {
        let err = ParseDocError {
            line: 42,
            message: "something went wrong".into(),
        };
        let s = format!("{}", err);
        assert!(s.contains("42"));
        assert!(s.contains("something went wrong"));
    }

    #[test]
    fn parse_url_with_fragment() {
        let doc = parse_document("See url`https://example.com/page#section` for details.").unwrap();
        match &doc.blocks[0] {
            Block::Prose(fragments) => match &fragments[1] {
                ProseFragment::Url { url, display } => {
                    assert_eq!(url, "https://example.com/page#section");
                    assert!(display.is_none());
                }
                other => panic!("expected Url, got {:?}", other),
            },
            other => panic!("expected Prose, got {:?}", other),
        }
    }

    #[test]
    fn parse_table_center_and_right_alignment() {
        let src = "!table\n  | A | B | C |\n  | :---: | ---: | --- |\n  | 1 | 2 | 3 |";
        let doc = parse_document(src).unwrap();
        match &doc.blocks[0] {
            Block::Table(table) => {
                assert_eq!(table.columns[0], ColumnAlign::Center);
                assert_eq!(table.columns[1], ColumnAlign::Right);
                assert_eq!(table.columns[2], ColumnAlign::Left);
            }
            other => panic!("expected Table, got {:?}", other),
        }
    }

    #[test]
    fn parse_include_empty_path_error() {
        // resolve_includes handles !include, not parse_document.
        // Test via resolve_includes directly.
        let err =
            resolve_includes("!include", std::path::Path::new("."), &mut Vec::new()).unwrap_err();
        assert!(err.message.contains("requires a file path"));
    }

    #[test]
    fn parse_date_with_value() {
        let doc = parse_document("!date 2026-03-15").unwrap();
        match &doc.blocks[0] {
            Block::Date(Some(d)) => assert_eq!(d, "2026-03-15"),
            other => panic!("expected Date(Some), got {:?}", other),
        }
    }

    #[test]
    fn parse_func_empty_params_error() {
        let err = parse_document("func f() := x").unwrap_err();
        assert!(err.message.contains("requires at least one parameter"));
    }
}
