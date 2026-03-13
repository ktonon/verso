use crate::ast::{
    Block, Claim, DimDecl, Document, List, ListItem, Proof, ProofStep, ProseFragment, Span,
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

/// Parse `lhs = rhs` from a claim body string.
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
}

impl InlineMatch<'_> {
    fn start(&self) -> usize {
        match self {
            InlineMatch::Tag(t) => t.start,
            InlineMatch::Bold { start, .. } => *start,
            InlineMatch::Italic { start, .. } => *start,
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

/// Find the earliest inline construct in the text (tag or emphasis).
fn find_next_inline(text: &str) -> Option<InlineMatch<'_>> {
    let tag = find_tagged_backtick(text).map(InlineMatch::Tag);
    let emph = find_emphasis(text);

    match (tag, emph) {
        (Some(t), Some(e)) => {
            if t.start() <= e.start() {
                Some(t)
            } else {
                Some(e)
            }
        }
        (Some(t), None) => Some(t),
        (None, Some(e)) => Some(e),
        (None, None) => None,
    }
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
    let tags = ["math", "tex", "claim"];

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
  sin(x)**2 + cos(x)**2
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
        let src = "- Energy: math`mc**2`\n- Momentum: math`m * v`";
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
}
