use crate::ast::{ColumnAlign, Environment, Figure, List, MathBlock, ProseFragment, Table};
use crate::tex_preamble::env_kind_name;
use crate::tex_prose::{escape_prose, write_prose_fragments, TexContext};
use std::fmt::Write;
use verso_symbolic::ToTex;

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

pub(super) fn write_figure(out: &mut String, fig: &Figure, ctx: &TexContext) {
    writeln!(out, "\\begin{{figure}}[H]").unwrap();
    writeln!(out, "\\centering").unwrap();
    writeln!(
        out,
        "\\includegraphics[width={}\\textwidth]{{{}}}",
        fig.width, fig.path
    )
    .unwrap();
    if let Some(cap) = &fig.caption {
        write!(out, "\\caption{{").unwrap();
        write_prose_fragments(out, cap, ctx);
        writeln!(out, "}}").unwrap();
    }
    if let Some(label) = &fig.label {
        writeln!(out, "\\label{{fig:{}}}", label).unwrap();
    }
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
