use crate::ast::{Block, Document, EnvKind, ProseFragment};
use crate::tex_queries::slugify;
use std::collections::{HashMap, HashSet};
use std::fmt::Write;

pub(super) struct TexMetadata<'a> {
    pub title_lines: Option<&'a Vec<String>>,
    pub authors: Vec<&'a str>,
    pub date: Option<Option<&'a str>>,
    pub abstract_fragments: Option<&'a Vec<ProseFragment>>,
    pub has_metadata: bool,
}

pub(super) fn build_section_title_map(doc: &Document) -> HashMap<String, String> {
    let mut section_titles = HashMap::new();
    for block in &doc.blocks {
        match block {
            Block::Section { title, label, .. } | Block::Part { title, label, .. } => {
                if let Some(lbl) = label {
                    section_titles.insert(lbl.clone(), title.clone());
                }
                section_titles.insert(slugify(title), title.clone());
            }
            _ => {}
        }
    }
    section_titles
}

pub(super) fn collect_metadata(doc: &Document) -> TexMetadata<'_> {
    let mut title_lines = None;
    let mut authors = Vec::new();
    let mut date = None;
    let mut abstract_fragments = None;

    for block in &doc.blocks {
        match block {
            Block::Title(lines) => title_lines = Some(lines),
            Block::Author(author) => authors.push(author.as_str()),
            Block::Date(value) => date = Some(value.as_deref()),
            Block::Abstract(frags) => abstract_fragments = Some(frags),
            _ => {}
        }
    }

    let has_metadata = title_lines.is_some() || !authors.is_empty() || date.is_some();

    TexMetadata {
        title_lines,
        authors,
        date,
        abstract_fragments,
        has_metadata,
    }
}

pub(super) fn write_preamble(out: &mut String, has_refs: bool, opts: &crate::compile_tex::CompileOptions) {
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
    // Dark mode color definitions must precede hyperref since its options
    // reference the named colors.
    if opts.dark {
        writeln!(out, "\\definecolor{{ogmabg}}{{HTML}}{{262B36}}").unwrap();
        writeln!(out, "\\definecolor{{ogmafg}}{{HTML}}{{B8C0CC}}").unwrap();
        writeln!(out, "\\definecolor{{ogmalink}}{{HTML}}{{88C0D0}}").unwrap();
        writeln!(out, "\\definecolor{{ogmacite}}{{HTML}}{{A3BE8C}}").unwrap();
    }
    if has_refs {
        let (link, url, cite) = if opts.dark {
            ("ogmalink", "ogmalink", "ogmacite")
        } else {
            ("black", "blue", "black")
        };
        writeln!(
            out,
            "\\usepackage[colorlinks=true,linkcolor={},urlcolor={},citecolor={}]{{hyperref}}",
            link, url, cite
        )
        .unwrap();
    }
    writeln!(out, "\\usepackage{{bookmark}}").unwrap();
    writeln!(out, "\\usepackage{{array}}").unwrap();
    writeln!(out, "\\usepackage{{float}}").unwrap();
    writeln!(out, "\\usepackage{{longtable}}").unwrap();
    writeln!(out, "\\usepackage{{graphicx}}").unwrap();
    writeln!(out, "\\usepackage{{wrapfig}}").unwrap();
    writeln!(out, "\\usepackage{{tikz}}").unwrap();
    writeln!(out, "\\usetikzlibrary{{arrows.meta,positioning,calc}}").unwrap();

    if opts.dark {
        writeln!(out).unwrap();
        writeln!(out, "% Dark mode: --dark flag").unwrap();
        writeln!(out, "\\pagecolor{{ogmabg}}").unwrap();
        writeln!(out, "\\color{{ogmafg}}").unwrap();
    }

    writeln!(out).unwrap();
    writeln!(out, "\\setlength{{\\parindent}}{{0pt}}").unwrap();
    writeln!(out, "\\setlength{{\\parskip}}{{6pt plus 2pt minus 1pt}}").unwrap();
    writeln!(out, "\\setlength{{\\emergencystretch}}{{3em}}").unwrap();
    writeln!(out, "\\setcounter{{tocdepth}}{{3}}").unwrap();

    // Widen the TOC number columns so two-digit subsection numbers like
    // "13.10" don't crowd the title. The article-class defaults reserve only
    // 2.3em / 3.2em / 4.1em — enough for one-digit numbers but not for
    // documents with more than ten sections or subsections-per-section.
    writeln!(out, "\\makeatletter").unwrap();
    writeln!(
        out,
        "\\renewcommand*\\l@subsection{{\\@dottedtocline{{2}}{{1.5em}}{{3.0em}}}}"
    )
    .unwrap();
    writeln!(
        out,
        "\\renewcommand*\\l@subsubsection{{\\@dottedtocline{{3}}{{4.5em}}{{3.8em}}}}"
    )
    .unwrap();
    writeln!(
        out,
        "\\renewcommand*\\l@paragraph{{\\@dottedtocline{{4}}{{8.3em}}{{4.5em}}}}"
    )
    .unwrap();
    writeln!(out, "\\makeatother").unwrap();
}

pub(super) fn collect_used_env_kinds(doc: &Document) -> Vec<EnvKind> {
    let mut env_kinds = Vec::new();
    let mut seen = HashSet::new();

    for block in &doc.blocks {
        if let Block::Environment(env) = block {
            if seen.insert(env.kind) {
                env_kinds.push(env.kind);
            }
        }
    }

    env_kinds
}

pub(super) fn write_theorem_preamble(out: &mut String, env_kinds: &[EnvKind]) {
    if env_kinds.is_empty() {
        return;
    }

    writeln!(out).unwrap();
    for kind in env_kinds {
        writeln!(
            out,
            "\\newtheorem{{{}}}{{{}}}",
            env_kind_name(*kind),
            env_kind_display(*kind)
        )
        .unwrap();
    }
}

pub(super) fn write_bibliography(out: &mut String, doc: &Document) {
    for block in &doc.blocks {
        if let Block::Bibliography { path, .. } = block {
            writeln!(out).unwrap();
            let bib_name = path.strip_suffix(".bib").unwrap_or(path);
            writeln!(out, "\\bibliographystyle{{unsrt}}").unwrap();
            writeln!(out, "\\bibliography{{{}}}", bib_name).unwrap();
        }
    }
}

pub(super) fn env_kind_name(kind: EnvKind) -> &'static str {
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
