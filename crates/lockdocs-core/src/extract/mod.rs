//! Turn a package's files into entries: API symbols (signature + doc comment)
//! from TypeScript declarations, JavaScript, Python, Rust and Go sources, and
//! prose sections from READMEs, changelogs and docs folders.

use serde::{Deserialize, Serialize};

mod docs;
mod go;
mod npm;
mod package;
mod plan;
mod python;
mod rust;
mod syntax;
mod typescript;

#[cfg(test)]
mod tests;

pub(crate) use npm::{npm_entries, npm_sources, npm_stamp};
pub use package::extract;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Kind {
    Function,
    Method,
    Class,
    Interface,
    Type,
    Const,
    Enum,
    Struct,
    Trait,
    Macro,
    Module,
    Field,
    Alias,
    Prose,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Function => "function",
            Kind::Method => "method",
            Kind::Class => "class",
            Kind::Interface => "interface",
            Kind::Type => "type",
            Kind::Const => "const",
            Kind::Enum => "enum",
            Kind::Struct => "struct",
            Kind::Trait => "trait",
            Kind::Macro => "macro",
            Kind::Module => "module",
            Kind::Field => "field",
            Kind::Alias => "re-export",
            Kind::Prose => "doc",
        }
    }
    pub fn is_container(self) -> bool {
        matches!(self, Kind::Class | Kind::Interface | Kind::Struct | Kind::Trait | Kind::Enum | Kind::Module)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub kind: Kind,
    /// Short name (`object`, `spawn`), or the heading path for prose.
    pub name: String,
    /// Qualified path: `z.object`, `tokio::task::spawn`, `pydantic.main.BaseModel.model_dump`.
    pub path: String,
    /// File relative to the package root.
    pub file: String,
    pub line: u32,
    /// Declaration text without the body.
    pub sig: String,
    /// Doc comment, or the prose section text.
    pub doc: String,
    /// For re-exports: the local name this entry points at.
    pub alias_of: Option<String>,
    /// Lives in a legacy compatibility copy (`zod/v3` inside zod 4,
    /// `pydantic/v1` inside pydantic 2): ranked below the current API.
    #[serde(default)]
    pub legacy: bool,
    /// Effective module imports in this prose section and its setup examples.
    #[serde(default)]
    pub prose_imports: Vec<String>,
    /// Citation lines of the logical ancestor sections, before chunking.
    #[serde(default)]
    pub prose_parents: Vec<u32>,
}

fn entry(kind: Kind, name: &str, path: String, file: &str, line: u32, sig: String, doc: String) -> Entry {
    Entry {
        kind,
        name: name.to_string(),
        path,
        file: file.to_string(),
        line,
        sig,
        doc,
        alias_of: None,
        legacy: false,
        prose_imports: Vec::new(),
        prose_parents: Vec::new(),
    }
}
