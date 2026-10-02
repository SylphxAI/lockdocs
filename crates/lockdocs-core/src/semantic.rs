//! Product policy for the shared embedding engine: the retrieval model,
//! identifier-aware inputs, lockdocs' opt-out and existing cache location.

use anyhow::Result;
use mcp_kit::embed::{self, Model, Tokenization, POTION_RETRIEVAL_32M};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
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

/// Query-side embedder: reads only the vocabulary, the row scales and the rows
/// of the query's own tokens (about 512 bytes each) with positioned reads, so
/// a search never pulls the 32 MB weight table into memory. It reproduces
/// `Model::embed` under `Tokenization::Identifiers` exactly (a test checks
/// this against the full model); entry vectors are still built by the full
/// model, only once per package version.
pub struct QueryModel {
    file: std::fs::File,
    vocab: PathBuf,
    rows: usize,
    dims: usize,
}

const MAX_WORD_CHARS: usize = 100;

/// FxHash for short vocabulary keys (SipHash costs milliseconds over 63k lines).
#[derive(Default)]
struct Fx(u64);

impl std::hash::Hasher for Fx {
    fn write(&mut self, bytes: &[u8]) {
        for chunk in bytes.chunks(8) {
            let mut b = [0u8; 8];
            b[..chunk.len()].copy_from_slice(chunk);
            self.0 = (self.0.rotate_left(5) ^ u64::from_le_bytes(b)).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
        }
    }
    fn write_u8(&mut self, i: u8) {
        self.0 = (self.0.rotate_left(5) ^ i as u64).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
    }
    fn finish(&self) -> u64 {
        self.0
    }
}

type Pieces = HashMap<String, u32, std::hash::BuildHasherDefault<Fx>>;

impl QueryModel {
    pub fn open(dir: &Path) -> Option<QueryModel> {
        let file = std::fs::File::open(dir.join("model.q8")).ok()?;
        let mut head = [0u8; 8];
        read_at(&file, &mut head, 0).ok()?;
        let rows = u32::from_le_bytes(head[0..4].try_into().ok()?) as usize;
        let dims = u32::from_le_bytes(head[4..8].try_into().ok()?) as usize;
        (file.metadata().ok()?.len() as usize == 8 + rows * 4 + rows * dims).then(|| QueryModel {
            file,
            vocab: dir.join("vocab.txt"),
            rows,
            dims,
        })
    }

    /// The vocabulary entries that can spell any of `words`: every substring
    /// of a word is a candidate, so one pass over vocab.txt finds them all
    /// without building the 63k-entry lookup tables.
    fn pieces(&self, words: &[String]) -> Option<(Pieces, Pieces)> {
        let mut first = Pieces::default();
        let mut cont = Pieces::default();
        for w in words {
            if w.chars().count() > MAX_WORD_CHARS {
                continue;
            }
            let bounds: Vec<usize> = w.char_indices().map(|(i, _)| i).chain([w.len()]).collect();
            for (si, &s) in bounds.iter().enumerate() {
                for &e in &bounds[si + 1..] {
                    let map = if s == 0 { &mut first } else { &mut cont };
                    map.entry(w[s..e].to_string()).or_insert(u32::MAX);
                }
            }
        }
        let text = std::fs::read_to_string(&self.vocab).ok()?;
        let mut n = 0;
        for (i, w) in text.split('\n').enumerate() {
            n += 1;
            match w.strip_prefix("##") {
                Some(rest) if !rest.is_empty() => {
                    if let Some(slot) = cont.get_mut(rest) {
                        *slot = i as u32;
                    }
                }
                _ => {
                    if let Some(slot) = first.get_mut(w) {
                        *slot = i as u32;
                    }
                }
            }
        }
        (n == self.rows).then_some((first, cont))
    }

    fn wordpiece(first: &Pieces, cont: &Pieces, word: &str, ids: &mut Vec<u32>) {
        if word.chars().count() > MAX_WORD_CHARS {
            return;
        }
        let start_len = ids.len();
        let mut start = 0;
        while start < word.len() {
            let map = if start == 0 { first } else { cont };
            let mut end = word.len();
            let mut found = None;
            while end > start {
                if word.is_char_boundary(end) {
                    if let Some(id) = map.get(&word[start..end]).filter(|id| **id != u32::MAX) {
                        found = Some(*id);
                        break;
                    }
                }
                end -= 1;
            }
            match found {
                Some(id) => {
                    ids.push(id);
                    start = end;
                }
                None => {
                    ids.truncate(start_len);
                    return;
                }
            }
        }
    }

    /// Unit-length embedding of a query, as `Model::embed` gives it.
    pub fn embed(&self, text: &str) -> Option<Vec<f32>> {
        let words = identifier_words(text);
        let (first, cont) = self.pieces(&words)?;
        let mut ids = Vec::new();
        for word in &words {
            Self::wordpiece(&first, &cont, word, &mut ids);
        }
        if ids.is_empty() {
            return None;
        }
        let mut acc = vec![0f32; self.dims];
        let mut row = vec![0u8; self.dims];
        let mut scale = [0u8; 4];
        let base = 8 + self.rows as u64 * 4;
        for id in ids {
            let r = id as usize;
            read_at(&self.file, &mut scale, 8 + r as u64 * 4).ok()?;
            read_at(&self.file, &mut row, base + (r * self.dims) as u64).ok()?;
            let s = f32::from_le_bytes(scale);
            for (a, q) in acc.iter_mut().zip(&row) {
                *a += *q as i8 as f32 * s;
            }
        }
        let norm = acc.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm < 1e-9 {
            return None;
        }
        acc.iter_mut().for_each(|x| *x /= norm);
        Some(acc)
    }
}

#[cfg(unix)]
fn read_at(f: &std::fs::File, buf: &mut [u8], off: u64) -> std::io::Result<()> {
    std::os::unix::fs::FileExt::read_exact_at(f, buf, off)
}

#[cfg(windows)]
fn read_at(f: &std::fs::File, buf: &mut [u8], mut off: u64) -> std::io::Result<()> {
    let mut done = 0;
    while done < buf.len() {
        let n = std::os::windows::fs::FileExt::seek_read(f, &mut buf[done..], off)?;
        if n == 0 {
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }
        done += n;
        off += n as u64;
    }
    Ok(())
}

/// Words of an identifier-aware text: camelCase and underscores split,
/// lowercased, every punctuation mark its own word (as `Tokenization::Identifiers`).
fn identifier_words(text: &str) -> Vec<String> {
    let mut spaced = String::with_capacity(text.len() + 16);
    let mut prev: Option<char> = None;
    for c in text.chars() {
        if let Some(p) = prev {
            if c.is_uppercase() && p.is_lowercase() {
                spaced.push(' ');
            }
        }
        spaced.push(if c == '_' { ' ' } else { c });
        prev = Some(c);
    }
    let mut out = Vec::new();
    let mut cur = String::new();
    for c in spaced.chars() {
        if c.is_whitespace() || c.is_control() {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
        } else if c.is_ascii_punctuation() || (!c.is_alphanumeric() && !c.is_whitespace()) {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            out.push(c.to_string());
        } else {
            cur.extend(c.to_lowercase());
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// A cheap embedder for query text, without loading the weight table.
pub fn query_model() -> Option<&'static QueryModel> {
    static QM: OnceLock<Option<QueryModel>> = OnceLock::new();
    QM.get_or_init(|| if enabled() && installed() { QueryModel::open(&model_dir()) } else { None })
        .as_ref()
}

/// Embeddings are on and the model is installed (nothing is loaded).
pub fn available() -> bool {
    enabled() && installed()
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
    fn query_model_matches_the_full_model() {
        // A tiny model: 12 word pieces, 6 dims, deterministic weights.
        let words = ["use", "state", "form", "action", "##s", "get", "(", ")", "a", "##b", "context", "provider"];
        let dims = 6usize;
        let dir = tempfile::tempdir().unwrap();
        let mut bytes = Vec::new();
        bytes.extend((words.len() as u32).to_le_bytes());
        bytes.extend((dims as u32).to_le_bytes());
        for i in 0..words.len() {
            bytes.extend((0.01 + i as f32 * 0.003).to_le_bytes());
        }
        for i in 0..words.len() {
            for d in 0..dims {
                bytes.push((((i * 7 + d * 13) % 255) as i32 - 127) as i8 as u8);
            }
        }
        std::fs::write(dir.path().join("model.q8"), bytes).unwrap();
        std::fs::write(dir.path().join("vocab.txt"), words.join("\n")).unwrap();
        let full = Model::load_dir(dir.path()).unwrap().with_tokenization(Tokenization::Identifiers);
        let lite = QueryModel::open(dir.path()).unwrap();
        for text in ["useState", "form_actions get(a) abs", "ContextProvider", "unknownword", "", "FormAction (use)"] {
            match (full.embed(text), lite.embed(text)) {
                (None, None) => {}
                (Some(a), Some(b)) => assert!(a.iter().zip(&b).all(|(x, y)| (x - y).abs() < 1e-6), "{text}"),
                _ => panic!("{text}: one side empty"),
            }
        }
    }

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
