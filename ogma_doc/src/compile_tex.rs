use crate::ast::{Block, Document};
use crate::tex_blocks::{
    write_block_quote, write_environment, write_figure, write_list, write_math_block, write_table,
};
use crate::tex_preamble::{
    build_section_title_map, collect_metadata, collect_used_env_kinds, write_bibliography,
    write_preamble, write_theorem_preamble,
};
use crate::tex_prose::{
    escape_prose, format_date, write_def, write_prose, write_prose_fragments, write_var,
    TexContext,
};
use crate::tex_refs::block_has_refs;
use crate::tex_structure::{write_claim, write_proof, write_section};
pub use crate::tex_queries::{
    collect_labels, collect_symbols, find_claim_line, find_decl_line, find_label_line,
    find_symbol, find_unresolved_refs, find_unresolved_refs_against, slugify, SymbolInfo,
};
use std::fmt::Write;

/// Compile a Document to a LaTeX string.
pub fn compile_to_tex(doc: &Document) -> String {
    let mut out = String::new();

    let ctx = TexContext {
        section_titles: build_section_title_map(doc),
        symbols: collect_symbols(doc),
    };

    let metadata = collect_metadata(doc);

    // Check if document uses ref tags (to conditionally include hyperref)
    let has_refs = doc.blocks.iter().any(|b| block_has_refs(b));

    write_preamble(&mut out, has_refs);

    // Title block in preamble
    if let Some(lines) = metadata.title_lines {
        writeln!(out).unwrap();
        let title_tex = lines
            .iter()
            .map(|l| escape_prose(l))
            .collect::<Vec<_>>()
            .join(" \\\\\n");
        writeln!(out, "\\title{{{}}}", title_tex).unwrap();
    }
    if !metadata.authors.is_empty() {
        let joined = metadata
            .authors
            .iter()
            .map(|a| escape_prose(a))
            .collect::<Vec<_>>()
            .join(" \\and ");
        writeln!(out, "\\author{{{}}}", joined).unwrap();
    }
    match metadata.date {
        Some(Some(d)) => writeln!(out, "\\date{{{}}}", format_date(d)).unwrap(),
        Some(None) => writeln!(out, "\\date{{\\today}}").unwrap(),
        None => {} // no !date directive — LaTeX defaults to \today
    }

    let env_kinds = collect_used_env_kinds(doc);
    write_theorem_preamble(&mut out, &env_kinds);

    writeln!(out).unwrap();
    writeln!(out, "\\begin{{document}}").unwrap();

    if metadata.has_metadata {
        writeln!(out).unwrap();
        writeln!(out, "\\maketitle").unwrap();
    }

    if let Some(frags) = metadata.abstract_fragments {
        writeln!(out).unwrap();
        writeln!(out, "\\begin{{abstract}}").unwrap();
        write_prose_fragments(&mut out, frags, &ctx);
        writeln!(out).unwrap();
        writeln!(out, "\\end{{abstract}}").unwrap();
    }

    for block in &doc.blocks {
        writeln!(out).unwrap();
        match block {
            Block::Section {
                level,
                title,
                label,
                ..
            } => {
                write_section(&mut out, *level, title, label.as_deref());
            }
            Block::Prose(fragments) => {
                write_prose(&mut out, fragments, &ctx);
            }
            Block::Claim(claim) => {
                write_claim(&mut out, claim);
            }
            Block::Proof(proof) => {
                write_proof(&mut out, proof);
            }
            Block::Var(decl) => {
                write_var(&mut out, decl, &ctx);
            }
            Block::Def(decl) => {
                write_def(&mut out, decl, &ctx);
            }
            Block::Func(_) => {}
            Block::Title(_) | Block::Author(_) | Block::Date(_) | Block::Abstract(_) => {}
            Block::PageBreak => {
                writeln!(out, "\\newpage").unwrap();
            }
            Block::Toc => {
                writeln!(out, "\\tableofcontents").unwrap();
            }
            Block::List(list) => {
                write_list(&mut out, list, &ctx);
            }
            Block::MathBlock(mb) => {
                write_math_block(&mut out, mb);
            }
            Block::Bibliography { .. } => {} // handled after loop
            Block::Environment(env) => {
                write_environment(&mut out, env, &ctx);
            }
            Block::BlockQuote(fragments) => {
                write_block_quote(&mut out, fragments, &ctx);
            }
            Block::Center(fragments) => {
                writeln!(out, "\\begin{{center}}").unwrap();
                write_prose_fragments(&mut out, fragments, &ctx);
                writeln!(out).unwrap();
                writeln!(out, "\\end{{center}}").unwrap();
            }
            Block::Figure(fig) => {
                write_figure(&mut out, fig, &ctx);
            }
            Block::Table(table) => {
                write_table(&mut out, table, &ctx);
            }
            Block::ExpectFail { .. } => {
                // Test-only construct, not emitted in output
            }
        }
    }

    write_bibliography(&mut out, doc);

    writeln!(out).unwrap();
    writeln!(out, "\\end{{document}}").unwrap();
    out
}
#[cfg(test)]
mod tests;
