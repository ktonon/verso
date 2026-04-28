// Re-export from ogma_symbolic — all dimensional analysis logic lives there.
pub use ogma_symbolic::context::{
    check_claim_dim, check_dim, collect_units, DimEnv, DimError, DimOutcome,
};

use crate::ast::{Block, Document};
use ogma_symbolic::{BaseDim, Dimension};
use std::collections::HashSet;

/// An undeclared user dimension found in a document.
#[derive(Debug, PartialEq, Eq)]
pub struct UndeclaredDimension {
    pub name: String,
    pub line: usize,
    pub context: String,
}

/// Walk every declared dimension in `doc` and report any user-declared
/// (`BaseDim::User`) name that is not present in `registry`.
pub fn find_undeclared_dimensions(
    doc: &Document,
    registry: &HashSet<String>,
) -> Vec<UndeclaredDimension> {
    let mut out = Vec::new();
    collect_undeclared_in_blocks(&doc.blocks, registry, &mut out);
    out
}

fn collect_undeclared_in_blocks(
    blocks: &[Block],
    registry: &HashSet<String>,
    out: &mut Vec<UndeclaredDimension>,
) {
    for block in blocks {
        match block {
            Block::Var(decl) | Block::Concept(decl) => {
                check_dimension(&decl.dimension, registry, decl.span.line, &decl.var_name, out);
            }
            Block::Def(decl) => {
                if let Some(dim) = &decl.dimension {
                    check_dimension(dim, registry, decl.span.line, &decl.name, out);
                }
            }
            Block::ExpectFail { blocks, .. } => {
                collect_undeclared_in_blocks(blocks, registry, out);
            }
            _ => {}
        }
    }
}

fn check_dimension(
    dim: &Dimension,
    registry: &HashSet<String>,
    line: usize,
    decl_name: &str,
    out: &mut Vec<UndeclaredDimension>,
) {
    for base in dim.exponents().keys() {
        if let BaseDim::User(name) = base {
            if !registry.contains(name) {
                out.push(UndeclaredDimension {
                    name: name.clone(),
                    line,
                    context: decl_name.to_string(),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_document;

    #[test]
    fn finds_undeclared_user_dim_in_concept() {
        let doc = parse_document("concept n [Population]").unwrap();
        let registry = HashSet::new();
        let undeclared = find_undeclared_dimensions(&doc, &registry);
        assert_eq!(undeclared.len(), 1);
        assert_eq!(undeclared[0].name, "Population");
        assert_eq!(undeclared[0].context, "n");
    }

    #[test]
    fn declared_user_dim_passes() {
        let doc = parse_document("concept n [Population]").unwrap();
        let mut registry = HashSet::new();
        registry.insert("Population".to_string());
        let undeclared = find_undeclared_dimensions(&doc, &registry);
        assert!(undeclared.is_empty());
    }

    #[test]
    fn si_dim_does_not_require_registry() {
        let doc = parse_document("var x [L]").unwrap();
        let registry = HashSet::new();
        let undeclared = find_undeclared_dimensions(&doc, &registry);
        assert!(undeclared.is_empty());
    }

    #[test]
    fn finds_undeclared_user_dim_in_def() {
        let doc = parse_document("def constant [Currency] := 100").unwrap();
        let registry = HashSet::new();
        let undeclared = find_undeclared_dimensions(&doc, &registry);
        assert_eq!(undeclared.len(), 1);
        assert_eq!(undeclared[0].name, "Currency");
    }
}
