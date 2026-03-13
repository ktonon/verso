use crate::ast::{Block, Claim, Document, List, MathBlock, Proof, ProseFragment};
use erd_symbolic::ToTex;
use std::fmt::Write;

/// Compile a Document to a LaTeX string.
pub fn compile_to_tex(doc: &Document) -> String {
    let mut out = String::new();

    writeln!(out, "\\documentclass{{article}}").unwrap();
    writeln!(out, "\\usepackage{{amsmath}}").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "\\begin{{document}}").unwrap();

    for block in &doc.blocks {
        writeln!(out).unwrap();
        match block {
            Block::Section { level, title, .. } => {
                write_section(&mut out, *level, title);
            }
            Block::Prose(fragments) => {
                write_prose(&mut out, fragments);
            }
            Block::Claim(claim) => {
                write_claim(&mut out, claim);
            }
            Block::Proof(proof) => {
                write_proof(&mut out, proof);
            }
            Block::Dim(_) => {} // metadata, no LaTeX output
            Block::List(list) => {
                write_list(&mut out, list);
            }
            Block::MathBlock(mb) => {
                write_math_block(&mut out, mb);
            }
        }
    }

    writeln!(out).unwrap();
    writeln!(out, "\\end{{document}}").unwrap();
    out
}

fn write_section(out: &mut String, level: u8, title: &str) {
    let cmd = match level {
        1 => "section",
        2 => "subsection",
        3 => "subsubsection",
        _ => "paragraph",
    };
    writeln!(out, "\\{}{{{}}}",cmd, title).unwrap();
}

fn write_prose(out: &mut String, fragments: &[ProseFragment]) {
    write_prose_fragments(out, fragments);
    writeln!(out).unwrap();
}

fn write_prose_fragments(out: &mut String, fragments: &[ProseFragment]) {
    for fragment in fragments {
        match fragment {
            ProseFragment::Text(text) => out.push_str(text),
            ProseFragment::Math(expr) => {
                write!(out, "${}$", expr.to_tex()).unwrap();
            }
            ProseFragment::Tex(raw) => {
                write!(out, "${}$", raw).unwrap();
            }
            ProseFragment::ClaimRef(name) => {
                write!(out, "\\eqref{{eq:{}}}", name).unwrap();
            }
            ProseFragment::Bold(inner) => {
                out.push_str("\\textbf{");
                write_prose_fragments(out, inner);
                out.push('}');
            }
            ProseFragment::Italic(inner) => {
                out.push_str("\\textit{");
                write_prose_fragments(out, inner);
                out.push('}');
            }
        }
    }
}

fn write_claim(out: &mut String, claim: &Claim) {
    writeln!(out, "\\begin{{equation}} \\label{{eq:{}}}", claim.name).unwrap();
    writeln!(out, "  {} = {}", claim.lhs.to_tex(), claim.rhs.to_tex()).unwrap();
    writeln!(out, "\\end{{equation}}").unwrap();
}

fn write_proof(out: &mut String, proof: &Proof) {
    if proof.steps.is_empty() {
        return;
    }

    writeln!(out, "\\begin{{align*}}").unwrap();
    for (i, step) in proof.steps.iter().enumerate() {
        if i == 0 {
            write!(out, "  {}", step.expr.to_tex()).unwrap();
        } else {
            write!(out, "  &= {}", step.expr.to_tex()).unwrap();
        }

        // Add justification as a tag
        if let Some(ref just) = step.justification {
            write!(out, " && \\text{{({})}}", just).unwrap();
        }

        if i < proof.steps.len() - 1 {
            writeln!(out, " \\\\").unwrap();
        } else {
            writeln!(out).unwrap();
        }
    }
    writeln!(out, "\\end{{align*}}").unwrap();
}

fn write_list(out: &mut String, list: &List) {
    let env = if list.ordered { "enumerate" } else { "itemize" };
    writeln!(out, "\\begin{{{}}}", env).unwrap();
    for item in &list.items {
        write!(out, "  \\item ").unwrap();
        write_prose_fragments(out, &item.fragments);
        writeln!(out).unwrap();
        if let Some(ref children) = item.children {
            write_list(out, children);
        }
    }
    writeln!(out, "\\end{{{}}}", env).unwrap();
}

fn write_math_block(out: &mut String, mb: &MathBlock) {
    if mb.exprs.len() == 1 {
        writeln!(out, "\\[").unwrap();
        writeln!(out, "  {}", mb.exprs[0].to_tex()).unwrap();
        writeln!(out, "\\]").unwrap();
    } else {
        writeln!(out, "\\begin{{gather*}}").unwrap();
        for (i, expr) in mb.exprs.iter().enumerate() {
            if i < mb.exprs.len() - 1 {
                writeln!(out, "  {} \\\\", expr.to_tex()).unwrap();
            } else {
                writeln!(out, "  {}", expr.to_tex()).unwrap();
            }
        }
        writeln!(out, "\\end{{gather*}}").unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_document;

    #[test]
    fn compile_section() {
        let doc = parse_document("# My Section").unwrap();
        let tex = compile_to_tex(&doc);
        assert!(tex.contains("\\section{My Section}"));
    }

    #[test]
    fn compile_prose_with_inline_math() {
        let doc = parse_document("The value math`x + 1` is positive.").unwrap();
        let tex = compile_to_tex(&doc);
        assert!(tex.contains("The value $x + 1$ is positive."));
    }

    #[test]
    fn compile_prose_with_claim_ref() {
        let doc = parse_document("See claim`pythag` for details.").unwrap();
        let tex = compile_to_tex(&doc);
        assert!(tex.contains("See \\eqref{eq:pythag} for details."));
    }

    #[test]
    fn compile_claim() {
        let doc = parse_document(":claim foo\n  x + 1 = y").unwrap();
        let tex = compile_to_tex(&doc);
        assert!(tex.contains("\\begin{equation} \\label{eq:foo}"));
        assert!(tex.contains("\\end{equation}"));
    }

    #[test]
    fn compile_proof() {
        let src = "\
:proof expand
  x + 0
  = x             ; add_identity
";
        let doc = parse_document(src).unwrap();
        let tex = compile_to_tex(&doc);
        assert!(tex.contains("\\begin{align*}"));
        assert!(tex.contains("&= x"));
        assert!(tex.contains("\\text{(add_identity)}"));
        assert!(tex.contains("\\end{align*}"));
    }

    #[test]
    fn compile_full_document() {
        let src = "\
# Algebra

:claim add_zero
  x + 0 = x
";
        let doc = parse_document(src).unwrap();
        let tex = compile_to_tex(&doc);
        assert!(tex.contains("\\documentclass{article}"));
        assert!(tex.contains("\\usepackage{amsmath}"));
        assert!(tex.contains("\\begin{document}"));
        assert!(tex.contains("\\end{document}"));
        assert!(tex.contains("\\section{Algebra}"));
    }

    #[test]
    fn compile_bold_and_italic() {
        let doc = parse_document("This is **bold** and *italic* text.").unwrap();
        let tex = compile_to_tex(&doc);
        assert!(tex.contains("This is \\textbf{bold} and \\textit{italic} text."));
    }

    #[test]
    fn compile_bold_italic_combined() {
        let doc = parse_document("This is ***emphasized*** text.").unwrap();
        let tex = compile_to_tex(&doc);
        assert!(tex.contains("\\textbf{\\textit{emphasized}}"));
    }

    #[test]
    fn compile_math_block_single() {
        let doc = parse_document("```math\nx + 1\n```").unwrap();
        let tex = compile_to_tex(&doc);
        assert!(tex.contains("\\["));
        assert!(tex.contains("x + 1"));
        assert!(tex.contains("\\]"));
    }

    #[test]
    fn compile_math_block_multi() {
        let doc = parse_document("```math\nx + 1\ny + 2\n```").unwrap();
        let tex = compile_to_tex(&doc);
        assert!(tex.contains("\\begin{gather*}"));
        assert!(tex.contains("x + 1 \\\\"));
        assert!(tex.contains("y + 2"));
        assert!(tex.contains("\\end{gather*}"));
    }
}
