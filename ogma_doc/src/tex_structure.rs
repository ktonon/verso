use crate::ast::{Claim, Proof};
use crate::parse::{parse_prose_fragments, prose_to_string};
use crate::tex_prose::{escape_prose, write_prose_fragments, TexContext};
use crate::tex_queries::slugify;
use std::fmt::Write;
use ogma_symbolic::ToTex;

pub(super) fn write_section(
    out: &mut String,
    level: u8,
    title: &str,
    label: Option<&str>,
    ctx: &TexContext,
) {
    let cmd = match level {
        1 => "section",
        2 => "subsection",
        3 => "subsubsection",
        _ => "paragraph",
    };

    // Render the title as prose so inline math, bold, italic, etc. are
    // typeset properly. The PDF outline can't render math, so the bookmark
    // text falls back to plain stringification.
    let (rendered, plain) = match parse_prose_fragments(title) {
        Ok(frags) => {
            let mut buf = String::new();
            write_prose_fragments(&mut buf, &frags, ctx);
            (buf, prose_to_string(&frags))
        }
        Err(_) => (escape_prose(title), title.to_string()),
    };

    let rendered = rendered.trim_end().to_string();
    if rendered != plain {
        // Bookmark text (second arg) is consumed verbatim by the PDF outline
        // and must not be LaTeX-escaped.
        writeln!(
            out,
            "\\{}{{\\texorpdfstring{{{}}}{{{}}}}}",
            cmd, rendered, plain
        )
        .unwrap();
    } else {
        writeln!(out, "\\{}{{{}}}", cmd, rendered).unwrap();
    }

    let lbl = label
        .map(|value| value.to_string())
        .unwrap_or_else(|| slugify(&plain));
    if !lbl.is_empty() {
        writeln!(out, "\\label{{{}}}", lbl).unwrap();
    }
}

/// Emit a `\part{...}` heading. Part titles are rendered as prose so
/// inline math, bold, italic, etc. are typeset properly. An optional label
/// sets the referenceable target; absent that, the slug of the plain title
/// is used so `ref\`title-slug\`` still resolves (matching section behaviour).
pub(super) fn write_part(
    out: &mut String,
    title: &str,
    label: Option<&str>,
    ctx: &TexContext,
) {
    let (rendered, plain) = match parse_prose_fragments(title) {
        Ok(frags) => {
            let mut buf = String::new();
            write_prose_fragments(&mut buf, &frags, ctx);
            (buf, prose_to_string(&frags))
        }
        Err(_) => (escape_prose(title), title.to_string()),
    };

    let rendered = rendered.trim_end().to_string();
    if rendered != plain {
        writeln!(
            out,
            "\\part{{\\texorpdfstring{{{}}}{{{}}}}}",
            rendered, plain
        )
        .unwrap();
    } else {
        writeln!(out, "\\part{{{}}}", rendered).unwrap();
    }

    let lbl = label
        .map(|value| value.to_string())
        .unwrap_or_else(|| slugify(&plain));
    if !lbl.is_empty() {
        writeln!(out, "\\label{{{}}}", lbl).unwrap();
    }
}

pub(super) fn write_claim(out: &mut String, claim: &Claim) {
    let humanized = humanize_label(&claim.name);
    writeln!(
        out,
        "\\textit{{Claim}}({})",
        escape_prose(&humanized)
    )
    .unwrap();
    writeln!(out, "\\begin{{equation}} \\label{{eq:{}}}", claim.name).unwrap();
    writeln!(
        out,
        "  {} {} {}",
        claim.lhs.to_tex(),
        claim.relation.as_tex_str(),
        claim.rhs.to_tex()
    )
    .unwrap();
    writeln!(out, "\\end{{equation}}").unwrap();
}

/// Convert an identifier-style label (e.g. `universal_wavelength_stretch`)
/// into a human-readable phrase: underscores become spaces and the first
/// letter is capitalised. Other characters are preserved as-is, so
/// camelCase identifiers retain their internal capitalisation.
fn humanize_label(name: &str) -> String {
    let mut result = String::with_capacity(name.len());
    let mut chars = name.chars();
    if let Some(first) = chars.next() {
        for c in first.to_uppercase() {
            result.push(c);
        }
    }
    for c in chars {
        if c == '_' {
            result.push(' ');
        } else {
            result.push(c);
        }
    }
    result
}

pub(super) fn write_proof(out: &mut String, proof: &Proof) {
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
