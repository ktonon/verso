use crate::ast::{
    Align, ColumnAlign, Environment, Figure, List, MathBlock, ProseFragment, Table,
};
use crate::tex_preamble::env_kind_name;
use crate::tex_prose::{
    escape_prose, write_prose_fragments, write_prose_fragments_math_mode, TexContext,
};
use std::fmt::Write;
use ogma_symbolic::ToTex;

pub(super) fn write_list(out: &mut String, list: &List, ctx: &TexContext) {
    let env = if list.ordered { "enumerate" } else { "itemize" };
    writeln!(out, "\\begin{{{}}}", env).unwrap();
    for item in &list.items {
        write!(out, "  \\item ").unwrap();
        write_prose_fragments(out, &item.fragments, ctx);
        writeln!(out).unwrap();
        if let Some(ref children) = item.children {
            write_list(out, children, ctx);
        }
    }
    writeln!(out, "\\end{{{}}}", env).unwrap();
}

pub(super) fn write_math_block(out: &mut String, mb: &MathBlock) {
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

pub(super) fn write_block_quote(
    out: &mut String,
    fragments: &[ProseFragment],
    ctx: &TexContext,
) {
    writeln!(out, "\\begin{{quote}}").unwrap();
    write_prose_fragments(out, fragments, ctx);
    writeln!(out).unwrap();
    writeln!(out, "\\end{{quote}}").unwrap();
}

/// Resolve a figure path to absolute against the current working directory,
/// so pdflatex (which runs in a temp build dir) can find it. Returns the
/// path unchanged if it's already absolute.
fn resolve_path(path: &str) -> String {
    let p = std::path::Path::new(path);
    if p.is_absolute() {
        return path.to_string();
    }
    if let Ok(cwd) = std::env::current_dir() {
        return cwd.join(p).to_string_lossy().into_owned();
    }
    path.to_string()
}

pub(super) fn write_figure(out: &mut String, fig: &Figure, ctx: &TexContext) {
    writeln!(out, "\\begin{{figure}}[H]").unwrap();
    writeln!(out, "\\centering").unwrap();
    // Increase fbox padding so the border doesn't crowd the figure body
    // or the caption text. Local to this figure via the surrounding group.
    writeln!(
        out,
        "{{\\setlength{{\\fboxsep}}{{10pt}}\\fbox{{\\begin{{minipage}}{{0.92\\linewidth}}\\centering"
    )
    .unwrap();
    // Body: \input{...} for .tex sources, \includegraphics{...} otherwise.
    // Paths are resolved to absolute against the current working directory
    // because pdflatex runs in a temp build directory and won't find
    // project-relative paths otherwise.
    let resolved = resolve_path(&fig.path);
    if fig.path.ends_with(".tex") {
        writeln!(out, "\\input{{{}}}", resolved).unwrap();
    } else {
        writeln!(
            out,
            "\\includegraphics[width={}\\textwidth]{{{}}}",
            fig.width, resolved
        )
        .unwrap();
    }
    if let Some(cap) = &fig.caption {
        write!(out, "\\caption{{\\small ").unwrap();
        write_prose_fragments(out, cap, ctx);
        writeln!(out, "}}").unwrap();
    }
    if let Some(label) = &fig.label {
        writeln!(out, "\\label{{fig:{}}}", label).unwrap();
    }
    writeln!(out, "\\end{{minipage}}}}}}").unwrap();
    writeln!(out, "\\end{{figure}}").unwrap();
}

pub(super) fn write_table(out: &mut String, table: &Table, ctx: &TexContext) {
    let n = table.columns.len();
    let frac = (1.0 / n as f64) - 0.035;
    let col_spec: String = table
        .columns
        .iter()
        .map(|a| {
            let align = match a {
                ColumnAlign::Left => "\\raggedright",
                ColumnAlign::Center => "\\centering",
                ColumnAlign::Right => "\\raggedleft",
            };
            format!(
                ">{{{}\\arraybackslash}}p{{{:.3}\\textwidth}}",
                align, frac
            )
        })
        .collect::<Vec<_>>()
        .join("");

    writeln!(out, "\\begin{{longtable}}{{{}}}", col_spec).unwrap();
    if let Some(title) = &table.title {
        writeln!(out, "\\caption{{{}}} \\\\", escape_prose(title)).unwrap();
    }
    if let Some(label) = &table.label {
        writeln!(out, "\\label{{tab:{}}}", label).unwrap();
    }
    writeln!(out, "\\hline").unwrap();
    for (i, cell) in table.header.iter().enumerate() {
        if i > 0 {
            write!(out, " & ").unwrap();
        }
        write!(out, "\\textbf{{").unwrap();
        write_prose_fragments(out, cell, ctx);
        write!(out, "}}").unwrap();
    }
    writeln!(out, " \\\\").unwrap();
    writeln!(out, "\\hline").unwrap();
    writeln!(out, "\\endfirsthead").unwrap();
    writeln!(out, "\\hline").unwrap();
    for (i, cell) in table.header.iter().enumerate() {
        if i > 0 {
            write!(out, " & ").unwrap();
        }
        write!(out, "\\textbf{{").unwrap();
        write_prose_fragments(out, cell, ctx);
        write!(out, "}}").unwrap();
    }
    writeln!(out, " \\\\").unwrap();
    writeln!(out, "\\hline").unwrap();
    writeln!(out, "\\endhead").unwrap();
    for row in &table.rows {
        for (i, cell) in row.iter().enumerate() {
            if i > 0 {
                write!(out, " & ").unwrap();
            }
            write_prose_fragments(out, cell, ctx);
        }
        writeln!(out, " \\\\").unwrap();
    }
    writeln!(out, "\\hline").unwrap();
    writeln!(out, "\\end{{longtable}}").unwrap();
}

pub(super) fn write_align(out: &mut String, align: &Align, ctx: &TexContext) {
    writeln!(out, "\\begin{{align*}}").unwrap();
    let last = align.rows.len().saturating_sub(1);
    for (i, row) in align.rows.iter().enumerate() {
        for (j, cell) in row.iter().enumerate() {
            if j > 0 {
                write!(out, " & ").unwrap();
            }
            write_prose_fragments_math_mode(out, cell, ctx);
        }
        if i < last {
            writeln!(out, " \\\\").unwrap();
        } else {
            writeln!(out).unwrap();
        }
    }
    writeln!(out, "\\end{{align*}}").unwrap();
}

pub(super) fn write_environment(
    out: &mut String,
    env: &Environment,
    ctx: &TexContext,
) {
    let name = env_kind_name(env.kind);
    if let Some(ref title) = env.title {
        writeln!(out, "\\begin{{{}}}[{}]", name, title).unwrap();
    } else {
        writeln!(out, "\\begin{{{}}}", name).unwrap();
    }
    write_prose_fragments(out, &env.body, ctx);
    writeln!(out).unwrap();
    writeln!(out, "\\end{{{}}}", name).unwrap();
}
