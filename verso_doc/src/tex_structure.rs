use crate::ast::{Claim, Proof};
use crate::tex_prose::escape_prose;
use crate::tex_queries::slugify;
use std::fmt::Write;
use verso_symbolic::ToTex;

pub(super) fn write_section(out: &mut String, level: u8, title: &str, label: Option<&str>) {
    let cmd = match level {
        1 => "section",
        2 => "subsection",
        3 => "subsubsection",
        _ => "paragraph",
    };
    let escaped = escape_prose(title);
    if escaped != title {
        writeln!(
            out,
            "\\{}{{\\texorpdfstring{{{}}}{{{}}}}}",
            cmd, escaped, title
        )
        .unwrap();
    } else {
        writeln!(out, "\\{}{{{}}}", cmd, escaped).unwrap();
    }

    let lbl = label
        .map(|value| value.to_string())
        .unwrap_or_else(|| slugify(title));
    if !lbl.is_empty() {
        writeln!(out, "\\label{{{}}}", lbl).unwrap();
    }
}

pub(super) fn write_claim(out: &mut String, claim: &Claim) {
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
