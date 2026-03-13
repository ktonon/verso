use crate::ast::{Block, Claim, Document, EnvKind, Environment, List, MathBlock, Proof, ProseFragment};
use std::collections::HashSet;
use erd_symbolic::ToTex;
use std::fmt::Write;

/// Compile a Document to a LaTeX string.
pub fn compile_to_tex(doc: &Document) -> String {
    let mut out = String::new();

    writeln!(out, "\\documentclass{{article}}").unwrap();
    writeln!(out, "\\usepackage{{amsmath}}").unwrap();
    writeln!(out, "\\usepackage{{amsthm}}").unwrap();

    // Collect used environment kinds for \newtheorem declarations
    let mut env_kinds: Vec<EnvKind> = Vec::new();
    let mut seen: HashSet<EnvKind> = HashSet::new();
    for block in &doc.blocks {
        if let Block::Environment(env) = block {
            if seen.insert(env.kind) {
                env_kinds.push(env.kind);
            }
        }
    }
    if !env_kinds.is_empty() {
        writeln!(out).unwrap();
        for kind in &env_kinds {
            let name = env_kind_name(*kind);
            let display = env_kind_display(*kind);
            writeln!(out, "\\newtheorem{{{}}}{{{}}}",name, display).unwrap();
        }
    }

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
            Block::Bibliography { .. } => {} // handled after loop
            Block::Environment(env) => {
                write_environment(&mut out, env);
            }
            Block::BlockQuote(fragments) => {
                write_block_quote(&mut out, fragments);
            }
        }
    }

    // Bibliography at end of document
    for block in &doc.blocks {
        if let Block::Bibliography { path, .. } = block {
            writeln!(out).unwrap();
            let bib_name = path.strip_suffix(".bib").unwrap_or(path);
            writeln!(out, "\\bibliographystyle{{plain}}").unwrap();
            writeln!(out, "\\bibliography{{{}}}", bib_name).unwrap();
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
            ProseFragment::Cite(keys) => {
                write!(out, "\\cite{{{}}}", keys.join(",")).unwrap();
            }
            ProseFragment::Footnote(inner) => {
                out.push_str("\\footnote{");
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

fn write_block_quote(out: &mut String, fragments: &[ProseFragment]) {
    writeln!(out, "\\begin{{quote}}").unwrap();
    write_prose_fragments(out, fragments);
    writeln!(out).unwrap();
    writeln!(out, "\\end{{quote}}").unwrap();
}

fn write_environment(out: &mut String, env: &Environment) {
    let name = env_kind_name(env.kind);
    if let Some(ref title) = env.title {
        writeln!(out, "\\begin{{{}}}[{}]", name, title).unwrap();
    } else {
        writeln!(out, "\\begin{{{}}}", name).unwrap();
    }
    write_prose_fragments(out, &env.body);
    writeln!(out).unwrap();
    writeln!(out, "\\end{{{}}}", name).unwrap();
}

fn env_kind_name(kind: EnvKind) -> &'static str {
    match kind {
        EnvKind::Theorem => "theorem",
        EnvKind::Lemma => "lemma",
        EnvKind::Definition => "definition",
        EnvKind::Corollary => "corollary",
        EnvKind::Remark => "remark",
        EnvKind::Example => "example",
    }
}

fn env_kind_display(kind: EnvKind) -> &'static str {
    match kind {
        EnvKind::Theorem => "Theorem",
        EnvKind::Lemma => "Lemma",
        EnvKind::Definition => "Definition",
        EnvKind::Corollary => "Corollary",
        EnvKind::Remark => "Remark",
        EnvKind::Example => "Example",
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

    #[test]
    fn compile_cite() {
        let doc = parse_document("See cite`einstein1905` here.").unwrap();
        let tex = compile_to_tex(&doc);
        assert!(tex.contains("\\cite{einstein1905}"));
    }

    #[test]
    fn compile_bibliography() {
        let doc = parse_document(":bibliography refs.bib").unwrap();
        let tex = compile_to_tex(&doc);
        assert!(tex.contains("\\bibliographystyle{plain}"));
        assert!(tex.contains("\\bibliography{refs}"));
        // Should appear before \end{document}
        let bib_pos = tex.find("\\bibliography{refs}").unwrap();
        let end_pos = tex.find("\\end{document}").unwrap();
        assert!(bib_pos < end_pos);
    }

    #[test]
    fn compile_theorem_with_title() {
        let src = ":theorem Pythagorean\n  For right triangles.";
        let doc = parse_document(src).unwrap();
        let tex = compile_to_tex(&doc);
        assert!(tex.contains("\\newtheorem{theorem}{Theorem}"));
        assert!(tex.contains("\\begin{theorem}[Pythagorean]"));
        assert!(tex.contains("For right triangles."));
        assert!(tex.contains("\\end{theorem}"));
    }

    #[test]
    fn compile_definition_no_title() {
        let src = ":definition\n  A group is a set.";
        let doc = parse_document(src).unwrap();
        let tex = compile_to_tex(&doc);
        assert!(tex.contains("\\newtheorem{definition}{Definition}"));
        assert!(tex.contains("\\begin{definition}"));
        assert!(!tex.contains("\\begin{definition}["));
        assert!(tex.contains("A group is a set."));
        assert!(tex.contains("\\end{definition}"));
    }

    #[test]
    fn compile_newtheorem_only_for_used_kinds() {
        let src = ":lemma\n  Body A.\n\n:lemma Another\n  Body B.";
        let doc = parse_document(src).unwrap();
        let tex = compile_to_tex(&doc);
        // Only one \newtheorem for lemma even though two lemmas exist
        assert_eq!(tex.matches("\\newtheorem{lemma}{Lemma}").count(), 1);
        // No theorem declaration since no theorems used
        assert!(!tex.contains("\\newtheorem{theorem}"));
    }

    #[test]
    fn compile_amsthm_included() {
        let src = ":theorem\n  Body.";
        let doc = parse_document(src).unwrap();
        let tex = compile_to_tex(&doc);
        assert!(tex.contains("\\usepackage{amsthm}"));
    }

    #[test]
    fn compile_env_with_inline_math() {
        let src = ":theorem\n  If math`x` is positive then result holds.";
        let doc = parse_document(src).unwrap();
        let tex = compile_to_tex(&doc);
        assert!(tex.contains("$x$"));
    }

    // Phase 6: Block quotes, footnotes, comments

    #[test]
    fn compile_block_quote() {
        let doc = parse_document("> A famous result.").unwrap();
        let tex = compile_to_tex(&doc);
        assert!(tex.contains("\\begin{quote}"));
        assert!(tex.contains("A famous result."));
        assert!(tex.contains("\\end{quote}"));
    }

    #[test]
    fn compile_footnote() {
        let doc = parse_document("Result^[First noted by Euler.] here.").unwrap();
        let tex = compile_to_tex(&doc);
        assert!(tex.contains("\\footnote{First noted by Euler.}"));
    }

    #[test]
    fn compile_comment_produces_no_output() {
        let doc = parse_document("% This is a comment\nVisible.").unwrap();
        let tex = compile_to_tex(&doc);
        assert!(!tex.contains("comment"));
        assert!(tex.contains("Visible."));
    }
}
