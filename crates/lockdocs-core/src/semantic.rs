//! Product policy for the shared embedding engine: the retrieval model,
//! identifier-aware inputs, lockdocs' opt-out and existing cache location.

use anyhow::Result;
use mcp_kit::embed::{self, Model, Tokenization, POTION_RETRIEVAL_32M};
use std::path::PathBuf;
use std::sync::{Arc, OnceLock, RwLock};

pub use mcp_kit::embed::{cosine, quantize, Vec8};
pub const MODEL_ID: &str = POTION_RETRIEVAL_32M.id;

fn model_dir() -> PathBuf {
    crate::cache::dir().join("models").join(MODEL_ID)
}

/// Embeddings are on unless `LOCKDOCS_EMBED=0` (or false/off).
pub fn enabled() -> bool {
    !matches!(std::env::var("LOCKDOCS_EMBED").as_deref(), Ok("0") | Ok("false") | Ok("off"))
}

pub fn installed() -> bool {
    embed::installed_at(&model_dir())
}

static MODEL: OnceLock<RwLock<Option<Arc<Model>>>> = OnceLock::new();

/// Load existing model.q8/vocab.txt directly. The shared input policy keeps
/// cached entry vectors and new query vectors in the same embedding space.
pub fn get() -> Option<Arc<Model>> {
    if !enabled() {
        return None;
    }
    let cell = MODEL.get_or_init(|| RwLock::new(None));
    if let Some(model) = cell.read().unwrap().as_ref() {
        return Some(model.clone());
    }
    if !installed() {
        return None;
    }
    let model = Arc::new(Model::load_dir(&model_dir()).ok()?.with_tokenization(Tokenization::Identifiers));
    *cell.write().unwrap() = Some(model.clone());
    Some(model)
}

pub fn ensure() -> Result<()> {
    let base = std::env::var("LOCKDOCS_MODEL_URL")
        .unwrap_or_else(|_| format!("https://huggingface.co/{}/resolve/{}", POTION_RETRIEVAL_32M.repo, POTION_RETRIEVAL_32M.revision));
    embed::ensure_at(
        &POTION_RETRIEVAL_32M,
        &model_dir(),
        "lockdocs",
        "Set LOCKDOCS_EMBED=0 to stay keyword-only.",
        Some(&base),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persisted_entry_vectors_keep_their_postcard_layout() {
        #[derive(serde::Serialize)]
        struct PreviousVec8 {
            q: Vec<i8>,
            s: f32,
        }
        let previous = PreviousVec8 {
            q: vec![-127, 0, 127],
            s: 0.01,
        };
        let bytes = postcard::to_stdvec(&previous).unwrap();
        let current: Vec8 = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(current.q, previous.q);
        assert_eq!(current.s, previous.s);
        assert_eq!(postcard::to_stdvec(&current).unwrap(), bytes);
    }
}
