use crate::ast::{Block, Document, List, ProseFragment};
use std::collections::HashSet;

/// Information about a declared symbol.
pub struct SymbolInfo {
    pub name: String,
    pub kind: String,
    pub detail: String,
    pub line: usize,
}

/// Convert a section title to a URL-friendly slug for use as a label.
pub fn slugify(title: &str) -> String {
    let mut slug = String::new();
    for c in title.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if c == ' ' || c == '-' || c == '_' {
            slug.push('-');
        }
    }
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
    if result.ends_with('-') {
        result.pop();
    }
    result
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

        if trimmed.starts_with('#') {
            let level = trimmed.chars().take_while(|&c| c == '#').count();
            let raw_title = trimmed[level..].trim();
            if let Some(start) = raw_title.find("label`") {
                let rest = &raw_title[start + 6..];
                if let Some(end) = rest.find('`') {
                    if &rest[..end] == label {
                        return Some(i + 1);
                    }
                }
            }
            if let Some(start) = raw_title.find("\\label{") {
                let rest = &raw_title[start + 7..];
                if let Some(end) = rest.find('}') {
                    if &rest[..end] == label {
                        return Some(i + 1);
                    }
                }
            }
            let clean_title = strip_label_tag(raw_title);
            if slugify(&clean_title) == label {
                return Some(i + 1);
            }
        }

        if trimmed.starts_with("label:") {
            let val = trimmed["label:".len()..].trim();
            if val == label {
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

/// Find the line number of a `var`, `def`, or `func` declaration by name.
/// Uses subscript base matching (e.g. `ℓ` matches `var ℓ_{n} [L]`).
pub fn find_decl_line(name: &str, text: &str) -> Option<usize> {
    let base = verso_symbolic::context::subscript_base(name);
    for (i, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        for prefix in &["var ", "def ", "func "] {
            if let Some(rest) = trimmed.strip_prefix(prefix) {
                let decl_name = rest
                    .split(|c: char| c == '[' || c == ':' || c == '=' || c == '(')
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
        if let Some(rest) = trimmed.strip_prefix("claim") {
            if rest.trim() == name {
                return Some(i + 1);
            }
        }
    }
    None
}

/// Look up a symbol by name, preferring exact match over subscript base fallback.
pub fn find_symbol<'a>(symbols: &'a [SymbolInfo], query: &str) -> Option<&'a SymbolInfo> {
    if let Some(sym) = symbols.iter().find(|s| s.name == query) {
        return Some(sym);
    }
    let query_base = verso_symbolic::context::subscript_base(query);
    symbols
        .iter()
        .find(|s| verso_symbolic::context::subscript_base(&s.name) == query_base)
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
            Block::Def(decl) => {
                let mut detail = format!("{}", decl.value);
                if let Some(desc) = &decl.description {
                    detail.push_str("\n\n");
                    detail.push_str(desc);
                }
                symbols.push(SymbolInfo {
                    name: decl.name.clone(),
                    kind: "def".to_string(),
                    detail,
                    line: decl.span.line,
                });
            }
            Block::Func(decl) => {
                let params = decl.params.join(", ");
                let mut detail = format!("({}) := {}", params, decl.body);
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
                symbols.push(SymbolInfo {
                    name: claim.name.clone(),
                    kind: "claim".to_string(),
                    detail: format!("{} = {}", claim.lhs, claim.rhs),
                    line: claim.span.line,
                });
            }
            _ => {}
        }
    }
    symbols
}

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
