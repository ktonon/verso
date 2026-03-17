use crate::ast::{
    Block, Claim, ColumnAlign, Document, EnvKind, Environment, Figure, List, MathBlock, Proof,
    ProseFragment, Table,
};
use crate::parse::parse_prose_fragments;
use std::collections::{HashMap, HashSet};
use std::fmt::Write;
use verso_symbolic::ToTex;

/// Convert a section title to a URL-friendly slug for use as a label.
pub fn slugify(title: &str) -> String {
    let mut slug = String::new();
    for c in title.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if c == ' ' || c == '-' || c == '_' {
            slug.push('-');
        }
        // other characters (apostrophes, symbols, unicode) are dropped
    }
    // Collapse consecutive hyphens and trim
    let mut result = String::new();
    let mut prev_hyphen = false;
    for c in slug.chars() {
        if c == '-' {
            if !prev_hyphen && !result.is_empty() {
                result.push('-');
            }
            prev_hyphen = true;
        } else {
            result.push(c);
            prev_hyphen = false;
        }
    }
    // Trim trailing hyphen
    if result.ends_with('-') {
        result.pop();
    }
    result
}

/// Context passed through LaTeX compilation for resolving references and symbols.
struct TexContext {
    section_titles: HashMap<String, String>,
    symbols: Vec<SymbolInfo>,
}

/// Compile a Document to a LaTeX string.
pub fn compile_to_tex(doc: &Document) -> String {
    let mut out = String::new();

    // Build section label→title map for resolving ref`label` display text
    let mut section_titles: HashMap<String, String> = HashMap::new();
    for block in &doc.blocks {
        if let Block::Section { title, label, .. } = block {
            if let Some(lbl) = label {
                section_titles.insert(lbl.clone(), title.clone());
            }
            section_titles.insert(slugify(title), title.clone());
        }
    }

    let ctx = TexContext {
        section_titles,
        symbols: collect_symbols(doc),
    };

    // Collect metadata
    let mut title_lines: Option<&Vec<String>> = None;
    let mut authors: Vec<&str> = Vec::new();
    let mut date: Option<Option<&str>> = None; // None = no !date, Some(None) = !date with no value, Some(Some(d)) = !date d
    let mut abstract_fragments: Option<&Vec<ProseFragment>> = None;
    for block in &doc.blocks {
        match block {
            Block::Title(lines) => title_lines = Some(lines),
            Block::Author(a) => authors.push(a),
            Block::Date(d) => date = Some(d.as_deref()),
            Block::Abstract(frags) => abstract_fragments = Some(frags),
            _ => {}
        }
    }
    let has_metadata = title_lines.is_some() || !authors.is_empty() || date.is_some();

    // Check if document uses ref tags (to conditionally include hyperref)
    let has_refs = doc.blocks.iter().any(|b| block_has_refs(b));

    // Preamble: document class and packages
    writeln!(out, "\\documentclass[11pt]{{article}}").unwrap();
    writeln!(out, "\\usepackage[margin=1in]{{geometry}}").unwrap();
    writeln!(out, "\\usepackage[T1]{{fontenc}}").unwrap();
    writeln!(out, "\\usepackage[utf8]{{inputenc}}").unwrap();
    writeln!(out, "\\usepackage{{lmodern}}").unwrap();
    writeln!(out, "\\usepackage{{microtype}}").unwrap();
    writeln!(out, "\\usepackage{{amsmath}}").unwrap();
    writeln!(out, "\\usepackage{{amsthm}}").unwrap();
    writeln!(out, "\\usepackage{{xcolor}}").unwrap();
    writeln!(out, "\\usepackage{{framed}}").unwrap();
    if has_refs {
        writeln!(out, "\\usepackage[colorlinks=true,linkcolor=black,urlcolor=blue,citecolor=black]{{hyperref}}").unwrap();
    }
    writeln!(out, "\\usepackage{{bookmark}}").unwrap();
    writeln!(out, "\\usepackage{{graphicx}}").unwrap();
    writeln!(out, "\\usepackage{{wrapfig}}").unwrap();

    // Layout defaults
    writeln!(out).unwrap();
    writeln!(out, "\\setlength{{\\parindent}}{{0pt}}").unwrap();
    writeln!(out, "\\setlength{{\\parskip}}{{6pt plus 2pt minus 1pt}}").unwrap();
    writeln!(out, "\\setlength{{\\emergencystretch}}{{3em}}").unwrap();
    writeln!(out, "\\setcounter{{tocdepth}}{{3}}").unwrap();

    // Title block in preamble
    if let Some(lines) = title_lines {
        writeln!(out).unwrap();
        let title_tex = lines
            .iter()
            .map(|l| escape_prose(l))
            .collect::<Vec<_>>()
            .join(" \\\\\n");
        writeln!(out, "\\title{{{}}}", title_tex).unwrap();
    }
    if !authors.is_empty() {
        let joined = authors
            .iter()
            .map(|a| escape_prose(a))
            .collect::<Vec<_>>()
            .join(" \\and ");
        writeln!(out, "\\author{{{}}}", joined).unwrap();
    }
    match date {
        Some(Some(d)) => writeln!(out, "\\date{{{}}}", format_date(d)).unwrap(),
        Some(None) => writeln!(out, "\\date{{\\today}}").unwrap(),
        None => {} // no !date directive — LaTeX defaults to \today
    }

    // Collect used environment kinds for \newtheorem declarations
    let mut env_kinds: Vec<EnvKind> = Vec::new();
    let mut seen: HashSet<EnvKind> = HashSet::new();
    let mut has_definitions = false;
    for block in &doc.blocks {
        if let Block::Environment(env) = block {
            if seen.insert(env.kind) {
                env_kinds.push(env.kind);
            }
        }
        if let Block::Claim(claim) = block {
            if claim.is_definition {
                has_definitions = true;
            }
        }
    }
    if !env_kinds.is_empty() || has_definitions {
        writeln!(out).unwrap();
        for kind in &env_kinds {
            let name = env_kind_name(*kind);
            let display = env_kind_display(*kind);
            writeln!(out, "\\newtheorem{{{}}}{{{}}}", name, display).unwrap();
        }
        if has_definitions {
            writeln!(out, "\\newtheorem{{definition}}{{Definition}}").unwrap();
        }
    }

    writeln!(out).unwrap();
    writeln!(out, "\\begin{{document}}").unwrap();

    if has_metadata {
        writeln!(out).unwrap();
        writeln!(out, "\\maketitle").unwrap();
    }

    if let Some(frags) = abstract_fragments {
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
            Block::Var(_) | Block::Const(_) | Block::Func(_) => {}
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

fn write_section(out: &mut String, level: u8, title: &str, label: Option<&str>) {
    let cmd = match level {
        1 => "section",
        2 => "subsection",
        3 => "subsubsection",
        _ => "paragraph",
    };
    writeln!(out, "\\{}{{{}}}", cmd, escape_prose(title)).unwrap();
    // Prefer explicit label, fall back to slug
    let lbl = label
        .map(|l| l.to_string())
        .unwrap_or_else(|| slugify(title));
    if !lbl.is_empty() {
        writeln!(out, "\\label{{{}}}", lbl).unwrap();
    }
}

fn write_prose(
    out: &mut String,
    fragments: &[ProseFragment],
    ctx: &TexContext,
) {
    write_prose_fragments(out, fragments, ctx);
    writeln!(out).unwrap();
}

/// Format a date string for LaTeX output.
/// Recognizes ISO format `YYYY-MM-DD` and formats as "Month DD, YYYY".
/// Other values are passed through as-is.
fn format_date(s: &str) -> String {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() == 3 {
        if let (Ok(year), Ok(month), Ok(day)) = (
            parts[0].parse::<u32>(),
            parts[1].parse::<u32>(),
            parts[2].parse::<u32>(),
        ) {
            let month_name = match month {
                1 => "January",
                2 => "February",
                3 => "March",
                4 => "April",
                5 => "May",
                6 => "June",
                7 => "July",
                8 => "August",
                9 => "September",
                10 => "October",
                11 => "November",
                12 => "December",
                _ => return s.to_string(),
            };
            return format!("{} {}, {}", month_name, day, year);
        }
    }
    s.to_string()
}

/// Escape a dimension string for use in LaTeX math mode.
/// Base dimension letters (L, M, T, etc.) are set in upright roman type
/// per physics convention. Exponents are wrapped in braces and spaces
/// become thin spaces.
fn escape_tex_dim(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 2);
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '^' {
            out.push_str("^{");
            while let Some(&next) = chars.peek() {
                if next == '-' || next.is_ascii_digit() {
                    out.push(chars.next().unwrap());
                } else {
                    break;
                }
            }
            out.push('}');
        } else if ch == ' ' {
            out.push_str("\\,");
        } else if ch.is_ascii_alphabetic() {
            write!(out, "\\mathrm{{{}}}", ch).unwrap();
        } else {
            out.push(ch);
        }
    }
    out
}

/// Escape prose text for LaTeX: `~` → `\textasciitilde{}`, paired `"` → ``` `` ''' ```.
/// Unpaired trailing `"` is left as-is.
fn escape_prose(text: &str) -> String {
    let quote_count = text.chars().filter(|&c| c == '"').count();
    let pairs = quote_count / 2;
    if pairs == 0 {
        return text.replace('~', "\\textasciitilde{}");
    }
    let mut result = String::with_capacity(text.len() + pairs * 4);
    let mut open = true;
    let mut quotes_remaining = quote_count;
    for ch in text.chars() {
        match ch {
            '"' if open && quotes_remaining > 1 => {
                result.push_str("``");
                open = false;
                quotes_remaining -= 1;
            }
            '"' if !open => {
                result.push_str("''");
                open = true;
                quotes_remaining -= 1;
            }
            '"' => result.push('"'),
            '~' => result.push_str("\\textasciitilde{}"),
            _ => result.push(ch),
        }
    }
    result
}

fn write_prose_fragments(
    out: &mut String,
    fragments: &[ProseFragment],
    ctx: &TexContext,
) {
    for fragment in fragments {
        match fragment {
            ProseFragment::Text(text) => out.push_str(&escape_prose(text)),
            ProseFragment::Math(expr) => {
                write!(out, "${}$", expr.to_tex()).unwrap();
            }
            ProseFragment::MathEquality(lhs, rhs) => {
                write!(out, "${} = {}$", lhs.to_tex(), rhs.to_tex()).unwrap();
            }
            ProseFragment::Tex(raw) => {
                write!(out, "${}$", raw).unwrap();
            }
            ProseFragment::ClaimRef(name) => {
                write!(out, "\\eqref{{eq:{}}}", name).unwrap();
            }
            ProseFragment::Bold(inner) => {
                out.push_str("\\textbf{");
                write_prose_fragments(out, inner, ctx);
                out.push('}');
            }
            ProseFragment::Italic(inner) => {
                out.push_str("\\textit{");
                write_prose_fragments(out, inner, ctx);
                out.push('}');
            }
            ProseFragment::Cite(keys) => {
                write!(out, "\\cite{{{}}}", keys.join(",")).unwrap();
            }
            ProseFragment::Footnote(inner) => {
                out.push_str("\\footnote{");
                write_prose_fragments(out, inner, ctx);
                out.push('}');
            }
            ProseFragment::Ref { label, display } => {
                let text = display
                    .as_deref()
                    .or_else(|| ctx.section_titles.get(label.as_str()).map(|s| s.as_str()))
                    .unwrap_or(label.as_str());
                write!(out, "\\hyperref[{}]{{{}}}", label, text).unwrap();
            }
            ProseFragment::Url { url, display } => {
                if let Some(text) = display {
                    write!(out, "\\href{{{}}}{{{}}}", url, text).unwrap();
                } else {
                    write!(out, "\\url{{{}}}", url).unwrap();
                }
            }
            ProseFragment::Sym { name, display } => {
                let base = verso_symbolic::context::subscript_base(name);
                let sym = ctx.symbols.iter().find(|s| {
                    s.name == *name || verso_symbolic::context::subscript_base(&s.name) == base
                });
                // Render symbol name as math
                let tex_name = verso_symbolic::parse_expr(name)
                    .map(|e| e.to_tex())
                    .unwrap_or_else(|_| name.clone());
                write!(out, "${}$", tex_name).unwrap();
                if let Some(sym) = sym {
                    // Append dimension/value info (rendered as math to handle ^ and _)
                    // Suppress [1] (dimensionless) as it adds no useful information
                    let type_info = sym.detail.lines().next().unwrap_or("");
                    if !type_info.is_empty() && type_info != "[1]" {
                        write!(out, " ${}$", escape_tex_dim(type_info)).unwrap();
                    }
                    // Use override display if provided, otherwise the declared description
                    let desc = display
                        .as_deref()
                        .or_else(|| {
                            // Description is everything after the first line of detail
                            let rest = sym.detail.find("\n\n").map(|i| sym.detail[i + 2..].trim());
                            rest.filter(|s| !s.is_empty())
                        });
                    if let Some(desc) = desc {
                        // Parse description as prose to handle inline tags like math`...`
                        out.push_str(": ");
                        match parse_prose_fragments(desc) {
                            Ok(frags) => write_prose_fragments(out, &frags, ctx),
                            Err(_) => out.push_str(&escape_prose(desc)),
                        }
                    }
                }
            }
            ProseFragment::ParBreak => {
                out.push_str("\n\\par\n");
            }
        }
    }
}

fn write_claim(out: &mut String, claim: &Claim) {
    if claim.is_definition {
        let title = claim.name.replace('-', " ");
        writeln!(out, "\\begin{{definition}}[{}]", title).unwrap();
    }
    writeln!(out, "\\begin{{equation}} \\label{{eq:{}}}", claim.name).unwrap();
    writeln!(out, "  {} = {}", claim.lhs.to_tex(), claim.rhs.to_tex()).unwrap();
    writeln!(out, "\\end{{equation}}").unwrap();
    if claim.is_definition {
        writeln!(out, "\\end{{definition}}").unwrap();
    }
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

fn write_list(out: &mut String, list: &List, ctx: &TexContext) {
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

fn write_block_quote(
    out: &mut String,
    fragments: &[ProseFragment],
    ctx: &TexContext,
) {
    writeln!(out, "\\begin{{quote}}").unwrap();
    write_prose_fragments(out, fragments, ctx);
    writeln!(out).unwrap();
    writeln!(out, "\\end{{quote}}").unwrap();
}

fn write_figure(out: &mut String, fig: &Figure, ctx: &TexContext) {
    writeln!(out, "\\begin{{figure}}[htbp]").unwrap();
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

fn write_table(out: &mut String, table: &Table, ctx: &TexContext) {
    writeln!(out, "\\begin{{table}}[htbp]").unwrap();
    writeln!(out, "\\centering").unwrap();
    let col_spec: String = table
        .columns
        .iter()
        .map(|a| match a {
            ColumnAlign::Left => 'l',
            ColumnAlign::Center => 'c',
            ColumnAlign::Right => 'r',
        })
        .collect();
    writeln!(out, "\\begin{{tabular}}{{{}}}", col_spec).unwrap();
    writeln!(out, "\\hline").unwrap();
    // Header row
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
    // Data rows
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
    writeln!(out, "\\end{{tabular}}").unwrap();
    if let Some(title) = &table.title {
        writeln!(out, "\\caption{{{}}}", escape_prose(title)).unwrap();
    }
    if let Some(label) = &table.label {
        writeln!(out, "\\label{{tab:{}}}", label).unwrap();
    }
    writeln!(out, "\\end{{table}}").unwrap();
}

fn write_environment(
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

/// Check if any prose fragment in a slice contains a Ref or Url (both need hyperref).
fn fragments_have_refs(fragments: &[ProseFragment]) -> bool {
    fragments.iter().any(|f| match f {
        ProseFragment::Ref { .. } | ProseFragment::Url { .. } => true,
        ProseFragment::Bold(inner)
        | ProseFragment::Italic(inner)
        | ProseFragment::Footnote(inner) => fragments_have_refs(inner),
        _ => false,
    })
}

/// Check if a block contains any Ref prose fragments.
fn block_has_refs(block: &Block) -> bool {
    match block {
        Block::Prose(fragments)
        | Block::BlockQuote(fragments)
        | Block::Abstract(fragments)
        | Block::Center(fragments) => fragments_have_refs(fragments),
        Block::List(list) => list_has_refs(list),
        Block::Environment(env) => fragments_have_refs(&env.body),
        Block::Figure(fig) => fig
            .caption
            .as_ref()
            .map_or(false, |c| fragments_have_refs(c)),
        Block::Table(table) => {
            table.header.iter().any(|c| fragments_have_refs(c))
                || table
                    .rows
                    .iter()
                    .any(|r| r.iter().any(|c| fragments_have_refs(c)))
        }
        _ => false,
    }
}

fn list_has_refs(list: &List) -> bool {
    list.items.iter().any(|item| {
        fragments_have_refs(&item.fragments)
            || item.children.as_ref().map_or(false, |c| list_has_refs(c))
    })
}

fn env_kind_name(kind: EnvKind) -> &'static str {
    match kind {
        EnvKind::Theorem => "theorem",
        EnvKind::Lemma => "lemma",
        EnvKind::Corollary => "corollary",
        EnvKind::Remark => "remark",
        EnvKind::Example => "example",
    }
}

fn env_kind_display(kind: EnvKind) -> &'static str {
    match kind {
        EnvKind::Theorem => "Theorem",
        EnvKind::Lemma => "Lemma",
        EnvKind::Corollary => "Corollary",
        EnvKind::Remark => "Remark",
        EnvKind::Example => "Example",
    }
}

/// Collect all defined labels in the document.
pub fn collect_labels(doc: &Document) -> HashSet<String> {
    let mut labels = HashSet::new();
    for block in &doc.blocks {
        match block {
            Block::Section { title, label, .. } => {
                if let Some(explicit) = label {
                    labels.insert(explicit.clone());
                }
                let slug = slugify(title);
                if !slug.is_empty() {
                    labels.insert(slug);
                }
            }
            Block::Figure(fig) => {
                if let Some(label) = &fig.label {
                    labels.insert(label.clone());
                }
            }
            Block::Table(table) => {
                if let Some(label) = &table.label {
                    labels.insert(label.clone());
                }
            }
            _ => {}
        }
    }
    labels
}

/// Find all ref labels used in the document.
fn collect_refs_from_fragments(fragments: &[ProseFragment], refs: &mut Vec<String>) {
    for f in fragments {
        match f {
            ProseFragment::Ref { label, .. } => refs.push(label.clone()),
            ProseFragment::Bold(inner)
            | ProseFragment::Italic(inner)
            | ProseFragment::Footnote(inner) => collect_refs_from_fragments(inner, refs),
            _ => {}
        }
    }
}

fn collect_refs_from_block(block: &Block, refs: &mut Vec<String>) {
    match block {
        Block::Prose(fragments)
        | Block::BlockQuote(fragments)
        | Block::Abstract(fragments)
        | Block::Center(fragments) => {
            collect_refs_from_fragments(fragments, refs);
        }
        Block::List(list) => collect_refs_from_list(list, refs),
        Block::Environment(env) => collect_refs_from_fragments(&env.body, refs),
        Block::Figure(fig) => {
            if let Some(cap) = &fig.caption {
                collect_refs_from_fragments(cap, refs);
            }
        }
        Block::Table(table) => {
            for cell in &table.header {
                collect_refs_from_fragments(cell, refs);
            }
            for row in &table.rows {
                for cell in row {
                    collect_refs_from_fragments(cell, refs);
                }
            }
        }
        _ => {}
    }
}

fn collect_refs_from_list(list: &List, refs: &mut Vec<String>) {
    for item in &list.items {
        collect_refs_from_fragments(&item.fragments, refs);
        if let Some(children) = &item.children {
            collect_refs_from_list(children, refs);
        }
    }
}

/// Return labels referenced by `ref` tags that don't match any defined label.
pub fn find_unresolved_refs(doc: &Document) -> Vec<String> {
    find_unresolved_refs_against(doc, doc)
}

/// Check refs in `ref_doc` against labels defined in `label_doc`.
///
/// This allows resolving refs against a broader document (e.g., one with
/// includes expanded) while only checking refs from the current file.
pub fn find_unresolved_refs_against(label_doc: &Document, ref_doc: &Document) -> Vec<String> {
    let labels = collect_labels(label_doc);
    let mut refs = Vec::new();
    for block in &ref_doc.blocks {
        collect_refs_from_block(block, &mut refs);
    }
    let mut unresolved: Vec<String> = refs.into_iter().filter(|r| !labels.contains(r)).collect();
    unresolved.sort();
    unresolved.dedup();
    unresolved
}

/// Find the line number (1-indexed) where a `ref` label is defined in raw text.
///
/// Searches for section headings (explicit `label`...`` or slugified title),
/// figure/table `label:` fields, and environment labels.
pub fn find_label_line(label: &str, text: &str) -> Option<usize> {
    let lines: Vec<&str> = text.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        // Section heading: # Title label`foo` or # Title (slugified)
        if trimmed.starts_with('#') {
            let level = trimmed.chars().take_while(|&c| c == '#').count();
            let raw_title = trimmed[level..].trim();
            // Check explicit label`...` syntax
            if let Some(start) = raw_title.find("label`") {
                let rest = &raw_title[start + 6..];
                if let Some(end) = rest.find('`') {
                    if &rest[..end] == label {
                        return Some(i + 1);
                    }
                }
            }
            // Check legacy \label{...} syntax
            if let Some(start) = raw_title.find("\\label{") {
                let rest = &raw_title[start + 7..];
                if let Some(end) = rest.find('}') {
                    if &rest[..end] == label {
                        return Some(i + 1);
                    }
                }
            }
            // Check slugified title (strip label tag first)
            let clean_title = strip_label_tag(raw_title);
            if slugify(&clean_title) == label {
                return Some(i + 1);
            }
        }

        // Figure/table label: field on a continuation line
        if trimmed.starts_with("label:") {
            let val = trimmed["label:".len()..].trim();
            if val == label {
                // Return the parent directive line (walk back to find !figure or !table)
                for j in (0..i).rev() {
                    let parent = lines[j].trim();
                    if parent.starts_with("!figure") || parent.starts_with("!table") {
                        return Some(j + 1);
                    }
                    if !parent.is_empty() && !lines[j].starts_with(char::is_whitespace) {
                        break;
                    }
                }
                return Some(i + 1);
            }
        }
    }
    None
}

/// Strip `label`...`` or `\label{...}` from a section title for slugification.
fn strip_label_tag(title: &str) -> String {
    if let Some(start) = title.find("label`") {
        let rest = &title[start + 6..];
        if let Some(end) = rest.find('`') {
            let before = title[..start].trim_end();
            let after = rest[end + 1..].trim_start();
            return if after.is_empty() {
                before.to_string()
            } else {
                format!("{} {}", before, after)
            };
        }
    }
    if let Some(start) = title.find("\\label{") {
        let rest = &title[start + 7..];
        if let Some(end) = rest.find('}') {
            let before = title[..start].trim_end();
            let after = rest[end + 1..].trim_start();
            return if after.is_empty() {
                before.to_string()
            } else {
                format!("{} {}", before, after)
            };
        }
    }
    title.to_string()
}

/// Find the line number (1-indexed) where a claim or definition is defined in raw text.
/// Find the line number of a `!var`, `!const`, or `!func` declaration by name.
/// Uses subscript base matching (e.g. `ℓ` matches `!var ℓ_{n} [L]`).
pub fn find_decl_line(name: &str, text: &str) -> Option<usize> {
    let base = verso_symbolic::context::subscript_base(name);
    for (i, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        for prefix in &["!var ", "!const ", "!func "] {
            if let Some(rest) = trimmed.strip_prefix(prefix) {
                // Extract the declared name (up to space, `[`, `=`, or `(`)
                let decl_name = rest
                    .split(|c: char| c == '[' || c == '=' || c == '(')
                    .next()
                    .unwrap_or("")
                    .trim();
                let decl_base = verso_symbolic::context::subscript_base(decl_name);
                if decl_name == name || decl_base == base {
                    return Some(i + 1);
                }
            }
        }
    }
    None
}

pub fn find_claim_line(name: &str, text: &str) -> Option<usize> {
    for (i, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("!claim") {
            if rest.trim() == name {
                return Some(i + 1);
            }
        }
        if let Some(rest) = trimmed.strip_prefix("!definition") {
            if rest.trim() == name {
                return Some(i + 1);
            }
        }
    }
    None
}

/// Collect symbol information from a parsed document for hover display.
pub fn collect_symbols(doc: &Document) -> Vec<SymbolInfo> {
    let mut symbols = Vec::new();
    for block in &doc.blocks {
        match block {
            Block::Var(decl) => {
                let mut detail = format!("{}", decl.dimension);
                if let Some(desc) = &decl.description {
                    detail.push_str("\n\n");
                    detail.push_str(desc);
                }
                symbols.push(SymbolInfo {
                    name: decl.var_name.clone(),
                    kind: "var".to_string(),
                    detail,
                    line: decl.span.line,
                });
            }
            Block::Const(decl) => {
                let mut detail = format!("{}", decl.value);
                if let Some(desc) = &decl.description {
                    detail.push_str("\n\n");
                    detail.push_str(desc);
                }
                symbols.push(SymbolInfo {
                    name: decl.name.clone(),
                    kind: "const".to_string(),
                    detail,
                    line: decl.span.line,
                });
            }
            Block::Func(decl) => {
                let params = decl.params.join(", ");
                let mut detail = format!("({}) = {}", params, decl.body);
                if let Some(desc) = &decl.description {
                    detail.push_str("\n\n");
                    detail.push_str(desc);
                }
                symbols.push(SymbolInfo {
                    name: decl.name.clone(),
                    kind: "func".to_string(),
                    detail,
                    line: decl.span.line,
                });
            }
            Block::Claim(claim) => {
                let kind = if claim.is_definition {
                    "definition"
                } else {
                    "claim"
                };
                symbols.push(SymbolInfo {
                    name: claim.name.clone(),
                    kind: kind.to_string(),
                    detail: format!("{} = {}", claim.lhs, claim.rhs),
                    line: claim.span.line,
                });
            }
            _ => {}
        }
    }
    symbols
}

/// Information about a declared symbol.
pub struct SymbolInfo {
    pub name: String,
    pub kind: String,
    pub detail: String,
    pub line: usize,
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
    fn compile_inline_math_equality() {
        let doc = parse_document("We define math`a = b + c` here.").unwrap();
        let tex = compile_to_tex(&doc);
        assert!(tex.contains("We define $a = b + c$ here."));
    }

    #[test]
    fn compile_definition_unicode() {
        let doc =
            parse_document("!definition Characteristic-Length\n  ℓ_{n-1} = ℓ_n / σ")
                .unwrap();
        let tex = compile_to_tex(&doc);
        assert!(
            tex.contains("\\begin{definition}[Characteristic Length]"),
            "got: {}",
            tex
        );
        assert!(
            tex.contains("\\ell_{n-1} = \\frac{\\ell_{n}}{\\sigma}"),
            "got: {}",
            tex
        );
    }

    #[test]
    fn compile_prose_with_claim_ref() {
        let doc = parse_document("See claim`pythag` for details.").unwrap();
        let tex = compile_to_tex(&doc);
        assert!(tex.contains("See \\eqref{eq:pythag} for details."));
    }

    #[test]
    fn compile_claim() {
        let doc = parse_document("!claim foo\n  x + 1 = y").unwrap();
        let tex = compile_to_tex(&doc);
        assert!(tex.contains("\\begin{equation} \\label{eq:foo}"));
        assert!(tex.contains("\\end{equation}"));
    }

    #[test]
    fn compile_proof() {
        let src = "\
!proof expand
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

!claim add_zero
  x + 0 = x
";
        let doc = parse_document(src).unwrap();
        let tex = compile_to_tex(&doc);
        assert!(tex.contains("\\documentclass[11pt]{article}"));
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
        let doc = parse_document("!bibliography refs.bib").unwrap();
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
        let src = "!theorem Pythagorean\n  For right triangles.";
        let doc = parse_document(src).unwrap();
        let tex = compile_to_tex(&doc);
        assert!(tex.contains("\\newtheorem{theorem}{Theorem}"));
        assert!(tex.contains("\\begin{theorem}[Pythagorean]"));
        assert!(tex.contains("For right triangles."));
        assert!(tex.contains("\\end{theorem}"));
    }

    #[test]
    fn compile_definition_as_equation() {
        let src = "!definition char-length\n  a = b + c";
        let doc = parse_document(src).unwrap();
        let tex = compile_to_tex(&doc);
        assert!(tex.contains("\\newtheorem{definition}{Definition}"));
        assert!(tex.contains("\\begin{definition}[char length]"));
        assert!(tex.contains("\\begin{equation} \\label{eq:char-length}"));
        assert!(tex.contains("a = b + c"));
        assert!(tex.contains("\\end{equation}"));
        assert!(tex.contains("\\end{definition}"));
    }

    #[test]
    fn compile_newtheorem_only_for_used_kinds() {
        let src = "!lemma\n  Body A.\n\n!lemma Another\n  Body B.";
        let doc = parse_document(src).unwrap();
        let tex = compile_to_tex(&doc);
        // Only one \newtheorem for lemma even though two lemmas exist
        assert_eq!(tex.matches("\\newtheorem{lemma}{Lemma}").count(), 1);
        // No theorem declaration since no theorems used
        assert!(!tex.contains("\\newtheorem{theorem}"));
    }

    #[test]
    fn compile_amsthm_included() {
        let src = "!theorem\n  Body.";
        let doc = parse_document(src).unwrap();
        let tex = compile_to_tex(&doc);
        assert!(tex.contains("\\usepackage{amsthm}"));
    }

    #[test]
    fn compile_env_with_inline_math() {
        let src = "!theorem\n  If math`x` is positive then result holds.";
        let doc = parse_document(src).unwrap();
        let tex = compile_to_tex(&doc);
        assert!(tex.contains("$x$"));
    }

    // Center

    #[test]
    fn compile_center() {
        let doc = parse_document("!center\n\tSome centered text.").unwrap();
        let tex = compile_to_tex(&doc);
        assert!(tex.contains("\\begin{center}"));
        assert!(tex.contains("Some centered text."));
        assert!(tex.contains("\\end{center}"));
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

    // Cross-references

    #[test]
    fn compile_section_has_label() {
        let doc = parse_document("# My Section").unwrap();
        let tex = compile_to_tex(&doc);
        assert!(tex.contains("\\section{My Section}"));
        assert!(tex.contains("\\label{my-section}"));
    }

    #[test]
    fn compile_ref_with_auto_title() {
        let src = "# Newton's Laws\n\nSee ref`newtons-laws` for details.";
        let doc = parse_document(src).unwrap();
        let tex = compile_to_tex(&doc);
        assert!(tex.contains("\\hyperref[newtons-laws]{Newton's Laws}"));
        assert!(tex.contains(
            "\\usepackage[colorlinks=true,linkcolor=black,urlcolor=blue,citecolor=black]{hyperref}"
        ));
    }

    #[test]
    fn compile_ref_with_custom_display() {
        let src =
            "# Earth and the Solar System\n\nref`earth-and-the-solar-system|Hydrogen creation`";
        let doc = parse_document(src).unwrap();
        let tex = compile_to_tex(&doc);
        assert!(tex.contains("\\hyperref[earth-and-the-solar-system]{Hydrogen creation}"));
    }

    #[test]
    fn compile_ref_unresolved_uses_label() {
        let doc = parse_document("See ref`unknown-section` here.").unwrap();
        let tex = compile_to_tex(&doc);
        assert!(tex.contains("\\hyperref[unknown-section]{unknown-section}"));
    }

    #[test]
    fn compile_no_hyperref_without_refs() {
        let doc = parse_document("# My Section\n\nJust prose.").unwrap();
        let tex = compile_to_tex(&doc);
        assert!(!tex.contains(
            "\\usepackage[colorlinks=true,linkcolor=black,urlcolor=blue,citecolor=black]{hyperref}"
        ));
    }

    #[test]
    fn compile_ref_in_bold_in_list() {
        let src = "## Earth and the Solar System\n\n1. **ref`earth-and-the-solar-system|Hydrogen creation`** *— abundant*";
        let doc = parse_document(src).unwrap();
        let tex = compile_to_tex(&doc);
        assert!(tex.contains("\\textbf{\\hyperref[earth-and-the-solar-system]{Hydrogen creation}}"));
        assert!(tex.contains("\\textit{— abundant}"));
    }

    // Default preamble

    #[test]
    fn compile_default_preamble() {
        let src = "Just prose.";
        let doc = parse_document(src).unwrap();
        let tex = compile_to_tex(&doc);
        assert!(tex.contains("\\documentclass[11pt]{article}"));
        assert!(tex.contains("\\usepackage[margin=1in]{geometry}"));
        assert!(tex.contains("\\usepackage{amsmath}"));
        assert!(tex.contains("\\usepackage{microtype}"));
        assert!(tex.contains("\\usepackage{lmodern}"));
        assert!(tex.contains("\\setlength{\\parindent}{0pt}"));
        assert!(tex.contains("\\setlength{\\parskip}{6pt plus 2pt minus 1pt}"));
    }

    // Tables

    #[test]
    fn compile_table_full() {
        let src = "!table Results\n  | A | B |\n  |:--|--:|\n  | 1 | 2 |";
        let doc = parse_document(src).unwrap();
        let tex = compile_to_tex(&doc);
        assert!(tex.contains("\\begin{table}[htbp]"));
        assert!(tex.contains("\\begin{tabular}{lr}"));
        assert!(tex.contains("\\textbf{A} & \\textbf{B}"));
        assert!(tex.contains("1 & 2"));
        assert!(tex.contains("\\caption{Results}"));
        assert!(tex.contains("\\end{table}"));
    }

    #[test]
    fn compile_table_with_label() {
        let src = "!table T\n  | X |\n  |---|\n  | 1 |\n  label: tab-x";
        let doc = parse_document(src).unwrap();
        let tex = compile_to_tex(&doc);
        assert!(tex.contains("\\label{tab:tab-x}"));
    }

    #[test]
    fn compile_table_no_title() {
        let src = "!table\n  | X |\n  |---|\n  | 1 |";
        let doc = parse_document(src).unwrap();
        let tex = compile_to_tex(&doc);
        assert!(!tex.contains("\\caption"));
    }

    // Figures

    #[test]
    fn compile_figure_full() {
        let src = "!figure plots/energy.pdf\n  caption: Energy levels.\n  label: fig-energy\n  width: 0.8";
        let doc = parse_document(src).unwrap();
        let tex = compile_to_tex(&doc);
        assert!(tex.contains("\\usepackage{graphicx}"));
        assert!(tex.contains("\\begin{figure}[htbp]"));
        assert!(tex.contains("\\centering"));
        assert!(tex.contains("\\includegraphics[width=0.8\\textwidth]{plots/energy.pdf}"));
        assert!(tex.contains("\\caption{Energy levels.}"));
        assert!(tex.contains("\\label{fig:fig-energy}"));
        assert!(tex.contains("\\end{figure}"));
    }

    #[test]
    fn compile_figure_path_only() {
        let src = "!figure img.png";
        let doc = parse_document(src).unwrap();
        let tex = compile_to_tex(&doc);
        assert!(tex.contains("\\includegraphics[width=1\\textwidth]{img.png}"));
        assert!(!tex.contains("\\caption"));
        assert!(!tex.contains("\\label{fig:"));
    }

    // Document metadata

    #[test]
    fn compile_full_metadata() {
        let src = "!title My Paper\n!author Alice\n!author Bob\n!date 2026\n!abstract\n  Some abstract text.\n\nBody here.";
        let doc = parse_document(src).unwrap();
        let tex = compile_to_tex(&doc);
        assert!(tex.contains("\\title{My Paper}"));
        assert!(tex.contains("\\author{Alice \\and Bob}"));
        assert!(tex.contains("\\date{2026}"));
        assert!(tex.contains("\\maketitle"));
        assert!(tex.contains("\\begin{abstract}"));
        assert!(tex.contains("Some abstract text."));
        assert!(tex.contains("\\end{abstract}"));
    }

    #[test]
    fn compile_multiline_title() {
        let src = "!title\n\tLine One\n\tLine Two";
        let doc = parse_document(src).unwrap();
        let tex = compile_to_tex(&doc);
        assert!(tex.contains("\\title{Line One \\\\\nLine Two}"));
    }

    #[test]
    fn compile_date_iso_format() {
        let src = "!title T\n!date 2026-03-14";
        let doc = parse_document(src).unwrap();
        let tex = compile_to_tex(&doc);
        assert!(tex.contains("\\date{March 14, 2026}"));
    }

    #[test]
    fn compile_date_no_value_uses_today() {
        let src = "!title T\n!date";
        let doc = parse_document(src).unwrap();
        let tex = compile_to_tex(&doc);
        assert!(tex.contains("\\date{\\today}"));
    }

    #[test]
    fn compile_no_date_directive_omits_date() {
        let src = "!title T";
        let doc = parse_document(src).unwrap();
        let tex = compile_to_tex(&doc);
        assert!(!tex.contains("\\date"));
    }

    #[test]
    fn compile_no_metadata_no_maketitle() {
        let src = "Just some prose.";
        let doc = parse_document(src).unwrap();
        let tex = compile_to_tex(&doc);
        assert!(!tex.contains("\\maketitle"));
    }

    #[test]
    fn compile_abstract_with_math() {
        let src = "!title T\n!abstract\n  We study math`x^2`.";
        let doc = parse_document(src).unwrap();
        let tex = compile_to_tex(&doc);
        assert!(tex.contains("\\begin{abstract}"));
        assert!(tex.contains("$x^{2}$"));
        assert!(tex.contains("\\end{abstract}"));
    }

    #[test]
    fn compile_abstract_paragraph_break() {
        let src = "!title T\n!abstract\n  First paragraph.\n\n  Second paragraph.";
        let doc = parse_document(src).unwrap();
        let tex = compile_to_tex(&doc);
        assert!(tex.contains("First paragraph.\n\\par\nSecond paragraph."));
    }

    #[test]
    fn compile_tilde_in_prose() {
        let src = "~200 million years and T~5000K";
        let doc = parse_document(src).unwrap();
        let tex = compile_to_tex(&doc);
        assert!(tex.contains("\\textasciitilde{}200 million years and T\\textasciitilde{}5000K"));
    }

    #[test]
    fn compile_quotes_in_heading() {
        let src = r#"## The "Standard" Model"#;
        let doc = parse_document(src).unwrap();
        let tex = compile_to_tex(&doc);
        assert!(tex.contains("\\subsection{The ``Standard'' Model}"));
    }

    #[test]
    fn compile_smart_quotes() {
        let src = r#"He said "hello" and she said "goodbye""#;
        let doc = parse_document(src).unwrap();
        let tex = compile_to_tex(&doc);
        assert!(tex.contains("He said ``hello'' and she said ``goodbye''"));
    }

    #[test]
    fn compile_smart_quotes_unmatched_stays() {
        let src = r#"A lone " on this line"#;
        let doc = parse_document(src).unwrap();
        let tex = compile_to_tex(&doc);
        assert!(tex.contains(r#"A lone " on this line"#));
    }

    #[test]
    fn compile_smart_quotes_and_tilde() {
        let src = r#"~"both""#;
        let doc = parse_document(src).unwrap();
        let tex = compile_to_tex(&doc);
        assert!(tex.contains("\\textasciitilde{}``both''"));
    }

    // Table of contents

    #[test]
    fn compile_toc() {
        let src = "!toc";
        let doc = parse_document(src).unwrap();
        let tex = compile_to_tex(&doc);
        assert!(tex.contains("\\tableofcontents"));
    }

    // URLs

    #[test]
    fn compile_url_plain() {
        let src = "See url`https://example.com`.";
        let doc = parse_document(src).unwrap();
        let tex = compile_to_tex(&doc);
        assert!(tex.contains("\\url{https://example.com}"));
        assert!(tex.contains(
            "\\usepackage[colorlinks=true,linkcolor=black,urlcolor=blue,citecolor=black]{hyperref}"
        ));
    }

    #[test]
    fn compile_url_with_display() {
        let src = "Click url`https://example.com|here`.";
        let doc = parse_document(src).unwrap();
        let tex = compile_to_tex(&doc);
        assert!(tex.contains("\\href{https://example.com}{here}"));
    }

    // Page breaks

    #[test]
    fn compile_pagebreak() {
        let src = "Text.\n\n!pagebreak\n\nMore.";
        let doc = parse_document(src).unwrap();
        let tex = compile_to_tex(&doc);
        assert!(tex.contains("\\newpage"));
    }

    // Unresolved ref diagnostics

    #[test]
    fn unresolved_ref_detected() {
        let src = "## Introduction\n\nSee ref`nonexistent` and ref`introduction`.";
        let doc = parse_document(src).unwrap();
        let unresolved = find_unresolved_refs(&doc);
        assert_eq!(unresolved, vec!["nonexistent"]);
    }

    #[test]
    fn all_refs_resolved() {
        let src = "## Newton's Laws\n\nSee ref`newtons-laws`.";
        let doc = parse_document(src).unwrap();
        let unresolved = find_unresolved_refs(&doc);
        assert!(unresolved.is_empty());
    }

    #[test]
    fn unresolved_ref_figure_label_resolved() {
        let src = "!figure img.png\n  label: my-fig\n\nSee ref`my-fig`.";
        let doc = parse_document(src).unwrap();
        let unresolved = find_unresolved_refs(&doc);
        assert!(unresolved.is_empty());
    }

    #[test]
    fn unresolved_ref_table_label_resolved() {
        let src = "!table T\n  | A |\n  |---|\n  | 1 |\n  label: my-tab\n\nSee ref`my-tab`.";
        let doc = parse_document(src).unwrap();
        let unresolved = find_unresolved_refs(&doc);
        assert!(unresolved.is_empty());
    }

    #[test]
    fn slugify_basic() {
        assert_eq!(slugify("Newton's Laws"), "newtons-laws");
        assert_eq!(slugify("E = mc²"), "e-mc");
        assert_eq!(slugify("The 2nd Law"), "the-2nd-law");
        assert_eq!(
            slugify("Earth and the Solar System"),
            "earth-and-the-solar-system"
        );
        assert_eq!(slugify("  Leading spaces  "), "leading-spaces");
    }

    #[test]
    fn native_label_resolves_ref() {
        let src = "## Long Title label`short`\n\nSee ref`short`.";
        let doc = parse_document(src).unwrap();
        let unresolved = find_unresolved_refs(&doc);
        assert!(unresolved.is_empty());
    }

    #[test]
    fn legacy_backslash_label_resolves_ref() {
        let src = "## Long Title \\label{short}\n\nSee ref`short`.";
        let doc = parse_document(src).unwrap();
        let unresolved = find_unresolved_refs(&doc);
        assert!(unresolved.is_empty());
    }

    #[test]
    fn label_stripped_from_section_title_in_tex() {
        let src = "## Absolute Time label`absolute-time`";
        let doc = parse_document(src).unwrap();
        let tex = compile_to_tex(&doc);
        assert!(tex.contains("\\subsection{Absolute Time}"));
        assert!(tex.contains("\\label{absolute-time}"));
        // Title should not contain the label`...` tag
        assert!(!tex.contains("label`"));
    }

    // find_label_line tests

    #[test]
    fn find_label_line_section_slug() {
        let text = "# Introduction\n\nSome text.";
        assert_eq!(find_label_line("introduction", text), Some(1));
    }

    #[test]
    fn find_label_line_section_explicit() {
        let text = "# Long Title label`short`\n\nSome text.";
        assert_eq!(find_label_line("short", text), Some(1));
    }

    #[test]
    fn find_label_line_section_legacy_label() {
        let text = "# Long Title \\label{short}\n\nSome text.";
        assert_eq!(find_label_line("short", text), Some(1));
    }

    #[test]
    fn find_label_line_figure_label() {
        let text = "Some text.\n\n!figure img.png\n  caption: A figure\n  label: my-fig";
        assert_eq!(find_label_line("my-fig", text), Some(3));
    }

    #[test]
    fn find_label_line_table_label() {
        let text = "!table My Table\n  | A |\n  |---|\n  | 1 |\n  label: my-tab";
        assert_eq!(find_label_line("my-tab", text), Some(1));
    }

    #[test]
    fn find_label_line_not_found() {
        let text = "# Introduction\n\nSome text.";
        assert_eq!(find_label_line("nonexistent", text), None);
    }

    #[test]
    fn find_label_line_explicit_over_slug() {
        // When a section has an explicit label, both the explicit and slug should work
        let text = "# Newton's Laws label`laws`\n\ntext";
        assert_eq!(find_label_line("laws", text), Some(1));
        assert_eq!(find_label_line("newtons-laws", text), Some(1));
    }

    // find_claim_line tests

    #[test]
    fn find_claim_line_basic() {
        let text = "!var x [L]\n\n!claim energy\n  x = x";
        assert_eq!(find_claim_line("energy", text), Some(3));
    }

    #[test]
    fn find_claim_line_definition() {
        let text = "!definition KE\n  (1/2) * m * v^2 = KE";
        assert_eq!(find_claim_line("KE", text), Some(1));
    }

    #[test]
    fn find_claim_line_not_found() {
        let text = "!claim energy\n  x = x";
        assert_eq!(find_claim_line("missing", text), None);
    }

    // collect_symbols tests

    #[test]
    fn collect_symbols_var() {
        let doc = parse_document("!var v [L T^-1]").unwrap();
        let syms = collect_symbols(&doc);
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "v");
        assert_eq!(syms[0].kind, "var");
    }

    #[test]
    fn collect_symbols_const() {
        let doc = parse_document("!const k = 2").unwrap();
        let syms = collect_symbols(&doc);
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "k");
        assert_eq!(syms[0].kind, "const");
        assert_eq!(syms[0].detail, "2");
    }

    #[test]
    fn collect_symbols_func() {
        let doc = parse_document("!func sq(x) = x^2").unwrap();
        let syms = collect_symbols(&doc);
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "sq");
        assert_eq!(syms[0].kind, "func");
    }

    #[test]
    fn collect_symbols_claim() {
        let doc = parse_document("!claim trivial\n  x = x").unwrap();
        let syms = collect_symbols(&doc);
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "trivial");
        assert_eq!(syms[0].kind, "claim");
    }

    #[test]
    fn collect_symbols_definition() {
        let doc = parse_document("!definition KE\n  (1/2) * m * v^2 = KE").unwrap();
        let syms = collect_symbols(&doc);
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "KE");
        assert_eq!(syms[0].kind, "definition");
    }

    #[test]
    fn collect_symbols_var_with_description() {
        let doc = parse_document("!var v [L T^-1]\n  Velocity.").unwrap();
        let syms = collect_symbols(&doc);
        assert_eq!(syms.len(), 1);
        assert!(syms[0].detail.contains("Velocity."));
    }

    #[test]
    fn compile_sym_var() {
        let src = "!var v [L T^-1]\n  Velocity.\n\nHere: sym`v`";
        let doc = parse_document(src).unwrap();
        let tex = compile_to_tex(&doc);
        assert!(tex.contains("$v$"), "should render symbol as math: {}", tex);
        assert!(tex.contains("Velocity."), "should include description: {}", tex);
    }

    #[test]
    fn compile_sym_with_override() {
        let src = "!var v [L T^-1]\n  Velocity.\n\nHere: sym`v|Speed of the particle.`";
        let doc = parse_document(src).unwrap();
        let tex = compile_to_tex(&doc);
        assert!(
            tex.contains("Speed of the particle."),
            "should use override: {}",
            tex
        );
        assert!(
            !tex.contains("Velocity."),
            "should not use declared desc: {}",
            tex
        );
    }

    #[test]
    fn compile_sym_unknown() {
        let src = "Here: sym`unknown`";
        let doc = parse_document(src).unwrap();
        let tex = compile_to_tex(&doc);
        assert!(
            tex.contains("$unknown$"),
            "should still render name as math: {}",
            tex
        );
    }
}
