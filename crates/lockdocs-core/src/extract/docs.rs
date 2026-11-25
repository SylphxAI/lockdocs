//! Extract prose and examples from package docs and upstream pages.

mod page;
mod prose;

pub(super) use page::{page_markdown, page_title};
pub(super) use prose::{example, prose, prose_section, prose_titled, Includes};
