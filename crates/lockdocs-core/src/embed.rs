//! Dense retrieval with a small static embedding model (model2vec
//! potion-retrieval-32M, distilled from bge-base-en-v1.5, MIT). A text's
//! embedding is the normalized mean of its WordPiece tokens' vectors, so
//! embedding every symbol of a large package takes milliseconds on a CPU.
//!
//! The model (129 MB) is downloaded once, verified by SHA-256, quantized to
//! int8 (32 MB) in the cache, and used offline afterwards. Without it,
//! lockdocs is BM25-only.

use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, OnceLock, RwLock};

pub const MODEL_ID: &str = "potion-retrieval-32M";
const REPO: &str = "minishlab/potion-retrieval-32M";
const REVISION: &str = "6fc8051fab2a1e0ee76689cf08c853792ac285e7";
const WEIGHTS_SHA256: &str = "07609e5bd33aad37900b3fd62f4ec96f6daec88ca4d46b9d8b928bfababf6ea0";
const WEIGHTS_BYTES: u64 = 129_210_456;
const MAX_WORD_CHARS: usize = 100;

pub struct Model {
    vocab: HashMap<String, u32>,
    dims: usize,
    /// Row-major int8 weights (stored as bytes) and one scale per row.
    q: Vec<u8>,
    scale: Vec<f32>,
}

/// An embedding: int8 values and a scale (value = q * scale); unit length.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, Default)]
pub struct Vec8 {
    pub q: Vec<i8>,
    pub s: f32,
}

fn model_dir() -> PathBuf {
    crate::cache::dir().join("models").join(MODEL_ID)
}

/// Embeddings are on unless `LOCKDOCS_EMBED=0`.
pub fn enabled() -> bool {
    !matches!(std::env::var("LOCKDOCS_EMBED").as_deref(), Ok("0") | Ok("false") | Ok("off"))
}

/// Is the model downloaded and converted?
pub fn installed() -> bool {
    let d = model_dir();
    d.join("model.q8").is_file() && d.join("vocab.txt").is_file()
}

static MODEL: OnceLock<RwLock<Option<Arc<Model>>>> = OnceLock::new();

/// The model when installed and enabled (loaded once per process).
pub fn get() -> Option<Arc<Model>> {
    if !enabled() {
        return None;
    }
    let cell = MODEL.get_or_init(|| RwLock::new(None));
    if let Some(m) = cell.read().unwrap().as_ref() {
        return Some(m.clone());
    }
    if !installed() {
        return None;
    }
    let m = Arc::new(Model::load().ok()?);
    *cell.write().unwrap() = Some(m.clone());
    Some(m)
}

/// Download, verify and convert the model if it is missing. Prints one line
/// to stderr before downloading.
pub fn ensure() -> Result<()> {
    if installed() {
        return Ok(());
    }
    let dir = model_dir();
    std::fs::create_dir_all(&dir)?;
    eprintln!(
        "lockdocs: downloading the embedding model {MODEL_ID} ({} MB, once) from huggingface.co to {}. Set LOCKDOCS_EMBED=0 to stay keyword-only.",
        WEIGHTS_BYTES / 1_000_000,
        crate::locate::tilde(&dir)
    );
    let base = std::env::var("LOCKDOCS_MODEL_URL").unwrap_or_else(|_| format!("https://huggingface.co/{REPO}/resolve/{REVISION}"));
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(900)))
        .user_agent(concat!("lockdocs/", env!("CARGO_PKG_VERSION")))
        .build()
        .into();
    // Tokenizer vocab
    let mut tok = String::new();
    agent
        .get(&format!("{base}/tokenizer.json"))
        .call()
        .context("downloading tokenizer.json")?
        .body_mut()
        .as_reader()
        .take(16 << 20)
        .read_to_string(&mut tok)?;
    let t: serde_json::Value = serde_json::from_str(&tok)?;
    let vocab = t
        .pointer("/model/vocab")
        .and_then(|v| v.as_object())
        .context("tokenizer.json has no WordPiece vocab")?;
    let mut rows: Vec<(&String, u64)> = vocab.iter().filter_map(|(k, v)| v.as_u64().map(|i| (k, i))).collect();
    rows.sort_by_key(|r| r.1);
    // Weights, hashed while streaming.
    let tmp = dir.join("model.safetensors.part");
    let mut res = agent
        .get(&format!("{base}/model.safetensors"))
        .call()
        .context("downloading model.safetensors")?;
    let mut reader = res.body_mut().as_reader().take(WEIGHTS_BYTES + 1);
    let mut file = std::fs::File::create(&tmp)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    let mut total = 0u64;
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        file.write_all(&buf[..n])?;
        total += n as u64;
    }
    drop(file);
    let got = format!("{:x}", hasher.finalize());
    if got != WEIGHTS_SHA256 || total != WEIGHTS_BYTES {
        let _ = std::fs::remove_file(&tmp);
        bail!("model download failed verification (sha256 {got}, {total} bytes)");
    }
    let bytes = std::fs::read(&tmp)?;
    let (dims, data) = parse_safetensors(&bytes)?;
    if data.len() / dims != rows.len() {
        bail!("model has {} rows but the vocab has {}", data.len() / dims, rows.len());
    }
    // Quantize per row to int8.
    let n = rows.len();
    let mut out = Vec::with_capacity(8 + n * 4 + n * dims);
    out.extend_from_slice(&(n as u32).to_le_bytes());
    out.extend_from_slice(&(dims as u32).to_le_bytes());
    let mut q = Vec::with_capacity(n * dims);
    for r in 0..n {
        let row = &data[r * dims..(r + 1) * dims];
        let max = row.iter().fold(0f32, |m, x| m.max(x.abs())).max(1e-12);
        let s = max / 127.0;
        out.extend_from_slice(&s.to_le_bytes());
        q.extend(row.iter().map(|x| (x / s).round().clamp(-127.0, 127.0) as i8 as u8));
    }
    out.extend_from_slice(&q);
    std::fs::write(dir.join("model.q8.part"), &out)?;
    let words: Vec<&str> = rows.iter().map(|r| r.0.as_str()).collect();
    std::fs::write(dir.join("vocab.txt"), words.join("\n"))?;
    std::fs::rename(dir.join("model.q8.part"), dir.join("model.q8"))?;
    let _ = std::fs::remove_file(&tmp);
    Ok(())
}

fn parse_safetensors(bytes: &[u8]) -> Result<(usize, Vec<f32>)> {
    let hlen = u64::from_le_bytes(bytes.get(..8).context("short file")?.try_into()?) as usize;
    let header: serde_json::Value = serde_json::from_slice(bytes.get(8..8 + hlen).context("short header")?)?;
    let t = header.get("embeddings").context("no embeddings tensor")?;
    if t.get("dtype").and_then(|d| d.as_str()) != Some("F32") {
        bail!("unexpected dtype");
    }
    let dims = t.pointer("/shape/1").and_then(|d| d.as_u64()).context("no shape")? as usize;
    let start = 8 + hlen + t.pointer("/data_offsets/0").and_then(|d| d.as_u64()).context("offsets")? as usize;
    let end = 8 + hlen + t.pointer("/data_offsets/1").and_then(|d| d.as_u64()).context("offsets")? as usize;
    let raw = bytes.get(start..end).context("short data")?;
    Ok((dims, raw.as_chunks::<4>().0.iter().map(|c| f32::from_le_bytes(*c)).collect()))
}

impl Model {
    fn load() -> Result<Model> {
        let d = model_dir();
        let mut bytes = std::fs::read(d.join("model.q8"))?;
        let n = u32::from_le_bytes(bytes[0..4].try_into()?) as usize;
        let dims = u32::from_le_bytes(bytes[4..8].try_into()?) as usize;
        if bytes.len() != 8 + n * 4 + n * dims {
            bail!("model.q8 is truncated");
        }
        let scale: Vec<f32> = bytes[8..8 + n * 4].as_chunks::<4>().0.iter().map(|c| f32::from_le_bytes(*c)).collect();
        let q = bytes.split_off(8 + n * 4);
        let vocab: HashMap<String, u32> = std::fs::read_to_string(d.join("vocab.txt"))?
            .split('\n')
            .enumerate()
            .map(|(i, w)| (w.to_string(), i as u32))
            .collect();
        Ok(Model { vocab, dims, q, scale })
    }

    pub fn dims(&self) -> usize {
        self.dims
    }

    /// WordPiece ids for a text (BERT uncased normalization; identifiers are
    /// split on camelCase and snake_case first).
    pub fn tokenize(&self, text: &str) -> Vec<u32> {
        let mut ids = Vec::new();
        for word in pre_tokenize(text) {
            if word.chars().count() > MAX_WORD_CHARS {
                continue;
            }
            let chars: Vec<char> = word.chars().collect();
            let mut start = 0;
            let mut pieces = Vec::new();
            let mut ok = true;
            while start < chars.len() {
                let mut end = chars.len();
                let mut found = None;
                while start < end {
                    let mut s: String = chars[start..end].iter().collect();
                    if start > 0 {
                        s.insert_str(0, "##");
                    }
                    if let Some(id) = self.vocab.get(&s) {
                        found = Some(*id);
                        break;
                    }
                    end -= 1;
                }
                match found {
                    Some(id) => {
                        pieces.push(id);
                        start = end;
                    }
                    None => {
                        ok = false;
                        break;
                    }
                }
            }
            if ok {
                ids.extend(pieces);
            }
        }
        ids
    }

    /// Unit-length embedding of a text, or None when no token is known.
    pub fn embed(&self, text: &str) -> Option<Vec<f32>> {
        let ids = self.tokenize(text);
        if ids.is_empty() {
            return None;
        }
        let mut acc = vec![0f32; self.dims];
        for id in &ids {
            let r = *id as usize;
            let s = self.scale[r];
            for (a, q) in acc.iter_mut().zip(&self.q[r * self.dims..(r + 1) * self.dims]) {
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

    pub fn embed8(&self, text: &str) -> Vec8 {
        match self.embed(text) {
            Some(v) => quantize(&v),
            None => Vec8::default(),
        }
    }
}

pub fn quantize(v: &[f32]) -> Vec8 {
    let max = v.iter().fold(0f32, |m, x| m.max(x.abs())).max(1e-12);
    let s = max / 127.0;
    Vec8 {
        q: v.iter().map(|x| (x / s).round().clamp(-127.0, 127.0) as i8).collect(),
        s,
    }
}

/// Cosine similarity of a unit query with a stored unit vector.
pub fn cosine(q: &[f32], v: &Vec8) -> f32 {
    if v.q.len() != q.len() {
        return 0.0;
    }
    let mut dot = 0f32;
    for (a, b) in q.iter().zip(&v.q) {
        dot += a * *b as f32;
    }
    dot * v.s
}

/// BERT basic tokenization with identifier splitting: lowercase words and
/// single punctuation characters.
fn pre_tokenize(text: &str) -> Vec<String> {
    let mut spaced = String::with_capacity(text.len() + 16);
    let mut prev: Option<char> = None;
    for c in text.chars() {
        if let Some(p) = prev {
            // camelCase / PascalCase boundary
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wordpiece_and_embedding() {
        let words = ["[PAD]", "[UNK]", "model", "valid", "##ate", "dict", "strict", "object", "(", ")"];
        let vocab: HashMap<String, u32> = words.iter().enumerate().map(|(i, w)| (w.to_string(), i as u32)).collect();
        let dims = 2;
        let mut q = Vec::new();
        for i in 0..words.len() {
            q.push(((i % 3) * 10) as u8);
            q.push((((i + 1) % 2) * 10) as u8);
        }
        let m = Model {
            vocab,
            dims,
            q,
            scale: vec![0.1; words.len()],
        };
        assert_eq!(m.tokenize("model_validate(dict)"), vec![2, 3, 4, 8, 5, 9]);
        assert_eq!(m.tokenize("strictObject"), vec![6, 7]);
        let e = m.embed("model dict").unwrap();
        assert!((e.iter().map(|x| x * x).sum::<f32>() - 1.0).abs() < 1e-4);
        let v = quantize(&e);
        assert!((cosine(&e, &v) - 1.0).abs() < 0.02);
        assert!(m.embed("zzzz").is_none());
    }
}
