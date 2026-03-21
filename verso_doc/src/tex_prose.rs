use crate::ast::{DefDecl, ProseFragment, VarDecl};
use crate::parse::parse_prose_fragments;
use crate::tex_queries::{declaration_equation_label, find_symbol, SymbolInfo};
use std::collections::HashMap;
use std::fmt::Write;
use verso_symbolic::ToTex;

/// Context passed through LaTeX prose rendering for resolving references and symbols.
pub(super) struct TexContext {
    pub(super) section_titles: HashMap<String, String>,
    pub(super) symbols: Vec<SymbolInfo>,
}

/// Format a date string for LaTeX output.
/// Recognizes ISO format `YYYY-MM-DD` and formats as "Month DD, YYYY".
/// Other values are passed through as-is.
pub(super) fn format_date(s: &str) -> String {
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
pub(super) fn escape_tex_dim(text: &str) -> String {
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

/// Escape prose text for LaTeX: special characters are escaped so that
/// plain text survives pdflatex without entering math mode.
/// Paired `"` are converted to ``` `` ''' ```.
pub(super) fn escape_prose(text: &str) -> String {
    let quote_count = text.chars().filter(|&c| c == '"').count();
    let mut result = String::with_capacity(text.len() + 16);
    let pairs = quote_count / 2;
    let mut open = true;
    let mut quotes_remaining = quote_count;
    for ch in text.chars() {
        match ch {
            '"' if pairs > 0 && open && quotes_remaining > 1 => {
                result.push_str("``");
                open = false;
                quotes_remaining -= 1;
            }
            '"' if pairs > 0 && !open => {
                result.push_str("''");
                open = true;
                quotes_remaining -= 1;
            }
            '~' => result.push_str("\\textasciitilde{}"),
            '_' => result.push_str("\\_"),
            '^' => result.push_str("\\^{}"),
            '&' => result.push_str("\\&"),
            '#' => result.push_str("\\#"),
            '%' => result.push_str("\\%"),
            '$' => result.push_str("\\$"),
            _ => result.push(ch),
        }
    }
    result
}

pub(super) fn write_prose(
    out: &mut String,
    fragments: &[ProseFragment],
    ctx: &TexContext,
) {
    write_prose_fragments(out, fragments, ctx);
    writeln!(out).unwrap();
}

pub(super) fn write_prose_fragments(
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
                write!(
                    out,
                    "${}$",
                    verso_symbolic::unicode::replace_unicode_with_latex(raw)
                )
                .unwrap();
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
                let sym = find_symbol(&ctx.symbols, name);
                let tex_name = verso_symbolic::parse_expr(name)
                    .map(|e| e.to_tex())
                    .unwrap_or_else(|_| name.clone());
                match (sym, display.as_deref()) {
                    (Some(sym), Some(override_text)) => {
                        write_sym_display(out, override_text, ctx);
                        if let Some(label) = &sym.reference_label {
                            write!(out, "~\\eqref{{{}}}", label).unwrap();
                        }
                    }
                    (Some(sym), None) => {
                        write!(out, "${}$", tex_name).unwrap();
                        if let Some(label) = &sym.reference_label {
                            write!(out, "~\\eqref{{{}}}", label).unwrap();
                        }
                    }
                    (None, _) => {
                        write!(out, "${}$", tex_name).unwrap();
                    }
                }
            }
            ProseFragment::ParBreak => {
                out.push_str("\n\\par\n");
            }
        }
    }
}

pub(super) fn write_var(out: &mut String, decl: &VarDecl, ctx: &TexContext) {
    if let Some(desc) = &decl.description {
        write_description(out, desc, ctx);
    }
    let tex_name = verso_symbolic::parse_expr(&decl.var_name)
        .map(|e| e.to_tex())
        .unwrap_or_else(|_| decl.var_name.clone());
    let dim = format!("{}", decl.dimension);
    let label = declaration_equation_label("var", &decl.var_name)
        .expect("var declarations should always have equation labels");
    writeln!(out, "\\begin{{equation}} \\label{{{}}}", label).unwrap();
    if dim != "1" {
        writeln!(out, "  {} \\quad {}", tex_name, escape_tex_dim(&dim)).unwrap();
    } else {
        writeln!(out, "  {}", tex_name).unwrap();
    }
    writeln!(out, "\\end{{equation}}").unwrap();
}

pub(super) fn write_def(out: &mut String, decl: &DefDecl, ctx: &TexContext) {
    if let Some(desc) = &decl.description {
        write_description(out, desc, ctx);
    }
    let tex_name = verso_symbolic::parse_expr(&decl.name)
        .map(|e| e.to_tex())
        .unwrap_or_else(|_| decl.name.clone());
    let label = declaration_equation_label("def", &decl.name)
        .expect("def declarations should always have equation labels");
    writeln!(out, "\\begin{{equation}} \\label{{{}}}", label).unwrap();
    writeln!(out, "  {} \\mathrel{{:=}} {}", tex_name, decl.value.to_tex()).unwrap();
    writeln!(out, "\\end{{equation}}").unwrap();
}

pub(super) fn write_description(out: &mut String, desc: &str, ctx: &TexContext) {
    match parse_prose_fragments(desc) {
        Ok(frags) => {
            write_prose_fragments(out, &frags, ctx);
            writeln!(out).unwrap();
        }
        Err(_) => writeln!(out, "{}", escape_prose(desc)).unwrap(),
    }
}

fn write_sym_display(out: &mut String, display: &str, ctx: &TexContext) {
    match parse_prose_fragments(display) {
        Ok(frags) => write_prose_fragments(out, &frags, ctx),
        Err(_) => out.push_str(&escape_prose(display)),
    }
}
