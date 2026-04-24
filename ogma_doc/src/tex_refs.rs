use crate::ast::{Block, List, ProseFragment};

/// Determine whether a document block requires hyperref support in the output.
pub(super) fn block_has_refs(block: &Block) -> bool {
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
            .is_some_and(|caption| fragments_have_refs(caption)),
        Block::Table(table) => {
            table.header.iter().any(|cell| fragments_have_refs(cell))
                || table
                    .rows
                    .iter()
                    .any(|row| row.iter().any(|cell| fragments_have_refs(cell)))
        }
        _ => false,
    }
}

fn fragments_have_refs(fragments: &[ProseFragment]) -> bool {
    fragments.iter().any(|fragment| match fragment {
        ProseFragment::Ref { .. } | ProseFragment::Url { .. } => true,
        ProseFragment::Bold(inner)
        | ProseFragment::Italic(inner)
        | ProseFragment::Footnote(inner) => fragments_have_refs(inner),
        _ => false,
    })
}

fn list_has_refs(list: &List) -> bool {
    list.items.iter().any(|item| {
        fragments_have_refs(&item.fragments)
            || item
                .children
                .as_ref()
                .is_some_and(list_has_refs)
    })
}
