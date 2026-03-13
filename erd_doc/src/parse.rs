use crate::ast::{Block, Claim, Document, Proof, ProofStep, ProseFragment, Span};
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

        // Prose line — collect consecutive non-special lines into a paragraph
        let mut prose_text = String::new();
        while i < lines.len() {
            let l = lines[i].trim();
            if l.is_empty() || l.starts_with('#') || l.starts_with(':') {
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

/// Parse prose text into fragments, extracting tagged inline expressions.
/// Supports: math`expr`, tex`raw latex`, claim`name`
fn parse_prose_fragments(text: &str) -> Result<Vec<ProseFragment>, ParseDocError> {
    let mut fragments = Vec::new();
    let mut rest = text;

    while !rest.is_empty() {
        // Look for the next tagged backtick expression
        if let Some(tag_match) = find_tagged_backtick(rest) {
            // Push any text before the tag
            if tag_match.start > 0 {
                fragments.push(ProseFragment::Text(rest[..tag_match.start].to_string()));
            }

            match tag_match.tag {
                "math" => {
                    let expr = parse_expr(tag_match.content).map_err(|e| ParseDocError {
                        line: 0,
                        message: format!("inline math`{}`: {:?}", tag_match.content, e),
                    })?;
                    fragments.push(ProseFragment::Math(expr));
                }
                "tex" => {
                    fragments.push(ProseFragment::Tex(tag_match.content.to_string()));
                }
                "claim" => {
                    fragments.push(ProseFragment::ClaimRef(tag_match.content.to_string()));
                }
                _ => {
                    // Unknown tag — treat as plain text
                    fragments.push(ProseFragment::Text(
                        rest[tag_match.start..tag_match.end].to_string(),
                    ));
                }
            }

            rest = &rest[tag_match.end..];
        } else {
            // No more tags — push remaining text
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
}
