//! Documentation relevance: keyword and embedding ranking, with entry boosts.

mod hybrid;
mod signals;

pub(super) use hybrid::hybrid_rank;
pub use signals::redirects;
pub(super) use signals::{api_words, change_intent, deprecated_page, heading_terms, identifiers, old_upgrade_guide};

pub(super) type Scored = (f32, usize, usize);

pub(super) struct LexicalAnchor {
    pub entry: (usize, usize),
    pub literal: bool,
}
