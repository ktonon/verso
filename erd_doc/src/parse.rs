use crate::ast::{Block, Claim, Document, Span};
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

        // Prose line — collect consecutive non-special lines into a paragraph
        let mut prose = String::new();
        while i < lines.len() {
            let l = lines[i].trim();
            if l.is_empty() || l.starts_with('#') || l.starts_with(':') {
                break;
            }
            if !prose.is_empty() {
                prose.push(' ');
            }
            prose.push_str(l);
            i += 1;
        }
        if !prose.is_empty() {
            blocks.push(Block::Prose(prose));
        }
    }

    Ok(Document { blocks })
}

/// A continuation line is indented (starts with whitespace) or is blank.
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
            Block::Prose(text) => {
                assert_eq!(text, "This is prose. Continued on next line.");
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
