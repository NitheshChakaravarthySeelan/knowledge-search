use async_trait::async_trait;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use common::errors::Result;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SparseVector {
    pub indices: Vec<u32>,
    pub values: Vec<f32>,
}

#[async_trait]
pub trait SparseEmbeddingProvider: Send + Sync {
    /// Generates a sparse vector representation for the given text.
    async fn embed_sparse(&self, text: &str) -> Result<SparseVector>;

    /// Called during ingestion to update corpus-level statistics (e.g., IDF for BM25).
    /// Default implementation is a no-op.
    async fn observe_document(&self, _text: &str) -> Result<()> {
        Ok(())
    }

    /// Persists any accumulated statistics to disk (if applicable).
    /// Default implementation is a no-op.
    async fn persist_stats(&self) -> Result<()> {
        Ok(())
    }
}

// ─── Tokenization Helpers ────────────────────────────────────────────────

/// Tokenizes text into lowercase alphanumeric tokens.
/// Uses the same algorithm as the original encoder for hash-space compatibility.
fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .map(|s| s.to_string())
        .collect()
}

/// Maps a term to a vocabulary index using a stable hash.
/// NOTE: Uses `DefaultHasher` which is NOT guaranteed stable across Rust versions.
/// This matches the existing `LocalHashingSparseEncoder` for backward compatibility
/// with data already stored in Qdrant.
fn hash_term(term: &str, vocabulary_size: u32) -> u32 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    term.hash(&mut hasher);
    (hasher.finish() % vocabulary_size as u64) as u32
}

// ─── BM25 Term Statistics ────────────────────────────────────────────────

/// Corpus-level term statistics used for BM25 IDF computation.
/// Thread-safe behind `Mutex` for concurrent read/write from ingestion and search.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BM25TermStats {
    /// Number of documents (chunks) containing each term.
    doc_frequency: HashMap<String, u64>,
    /// Total number of documents (chunks) observed.
    total_docs: u64,
    /// Sum of all document lengths (in tokens), for computing average document length.
    total_doc_length: u64,
}

impl BM25TermStats {
    pub fn new() -> Self {
        Self {
            doc_frequency: HashMap::new(),
            total_docs: 0,
            total_doc_length: 0,
        }
    }

    /// Returns the IDF for a term using Robertson-Sparck Jones formula:
    ///   IDF(t) = ln(1 + (N - n(t) + 0.5) / (n(t) + 0.5))
    /// Returns 0.0 for unknown terms to avoid negative scores.
    pub fn idf(&self, term: &str) -> f32 {
        let n = self.doc_frequency.get(term).copied().unwrap_or(0);
        let total = self.total_docs;
        if n == 0 || total == 0 {
            return 0.0;
        }
        // Robertson-Sparck Jones IDF (smooth, handles n up to N)
        let score = ((total as f64 - n as f64 + 0.5) / (n as f64 + 0.5) + 1.0).ln();
        score.max(0.0) as f32
    }

    /// Average document length (in tokens).
    pub fn avgdl(&self) -> f64 {
        if self.total_docs == 0 {
            return 0.0;
        }
        self.total_doc_length as f64 / self.total_docs as f64
    }

    /// Total number of observed documents.
    pub fn total_docs(&self) -> u64 {
        self.total_docs
    }

    /// Observe a single document's tokens: update document frequency for unique terms.
    pub fn observe_tokens(&mut self, tokens: &[String]) {
        self.total_docs += 1;
        self.total_doc_length += tokens.len() as u64;
        let mut seen: HashSet<&str> = HashSet::new();
        for token in tokens {
            if seen.insert(token) {
                *self.doc_frequency.entry(token.clone()).or_insert(0) += 1;
            }
        }
    }

    /// Load term statistics from a JSON file.
    pub fn load_from_file(path: &Path) -> std::io::Result<Self> {
        let data = std::fs::read_to_string(path)?;
        let stats: BM25TermStats = serde_json::from_str(&data)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(stats)
    }

    /// Save term statistics to a JSON file.
    pub fn save_to_file(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let data = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, data)?;
        Ok(())
    }
}

// ─── BM25 Sparse Encoder ─────────────────────────────────────────────────

/// A proper BM25 sparse encoder with corpus-level IDF statistics.
///
/// ## BM25 Formula
///
/// ```text
/// Score(t, d) = IDF(t) * (TF(t,d) * (k1 + 1)) / (TF(t,d) + k1 * (1 - b + b * |d| / avgdl))
/// ```
///
/// Where:
/// - `IDF(t)` = Robertson-Sparck Jones inverse document frequency
/// - `k1`     = term frequency saturation (default 1.2)
/// - `b`      = length normalization (default 0.75)
/// - `|d|`    = document length in tokens
/// - `avgdl`  = average document length across corpus
///
/// Statistics are persisted to a JSON file and loaded on restart.
pub struct BM25SparseEncoder {
    stats: Mutex<BM25TermStats>,
    stats_path: Option<PathBuf>,
    k1: f32,
    b: f32,
    vocabulary_size: u32,
}

impl BM25SparseEncoder {
    /// Create a new BM25 encoder with the given vocabulary size and BM25 parameters.
    ///
    /// * `vocabulary_size` — Number of hash buckets for the vocabulary (default 100_000).
    /// * `stats_path` — Optional path to persist term statistics. If `None`, stats are
    ///   kept in memory only and will be lost on restart.
    /// * `k1` — Term frequency saturation parameter (typically 1.2).
    /// * `b` — Length normalization parameter (typically 0.75).
    pub fn new(
        vocabulary_size: u32,
        stats_path: Option<PathBuf>,
        k1: f32,
        b: f32,
    ) -> Self {
        // Try to load existing stats from disk
        let stats = match stats_path.as_ref().and_then(|p| BM25TermStats::load_from_file(p).ok()) {
            Some(s) => {
                tracing::info!(path = ?stats_path, docs = s.total_docs, "Loaded BM25 term statistics from disk");
                s
            }
            None => {
                tracing::info!("No existing BM25 statistics found, starting fresh");
                BM25TermStats::new()
            }
        };

        Self {
            stats: Mutex::new(stats),
            stats_path,
            k1,
            b,
            vocabulary_size,
        }
    }

    /// Create a BM25 encoder with default parameters.
    /// Vocabulary: 100,000. k1 = 1.2. b = 0.75. No persistence.
    pub fn default() -> Self {
        Self::new(100_000, None, 1.2, 0.75)
    }

    /// Create a BM25 encoder that persists statistics to the given path.
    pub fn with_persistence(stats_path: impl Into<PathBuf>) -> Self {
        Self::new(100_000, Some(stats_path.into()), 1.2, 0.75)
    }
}

fn compute_bm25_weights(
    tokens: &[String],
    stats: &BM25TermStats,
    k1: f32,
    b: f32,
    vocabulary_size: u32,
) -> Vec<(u32, f32)> {
    if tokens.is_empty() {
        return Vec::new();
    }

    let doc_len = tokens.len() as f32;
    let avgdl = stats.avgdl() as f32;

    // Compute TF per vocabulary index
    let mut tf_map: HashMap<u32, f32> = HashMap::new();
    let mut term_map: HashMap<u32, String> = HashMap::new();
    for token in tokens {
        let idx = hash_term(token, vocabulary_size);
        *tf_map.entry(idx).or_insert(0.0) += 1.0;
        term_map.entry(idx).or_insert_with(|| token.clone());
    }

    // Apply BM25 scoring to each term
    let mut pairs: Vec<(u32, f32)> = tf_map
        .into_iter()
        .map(|(idx, tf_val)| {
            let term = term_map.get(&idx).map(|s| s.as_str()).unwrap_or("");
            let idf = stats.idf(term);
            // BM25: Score = IDF * (TF * (k1+1)) / (TF + k1 * (1 - b + b * docLen/avgdl))
            let denom = tf_val + k1 * (1.0 - b + b * doc_len / avgdl.max(1.0));
            let bm25_score = idf * (tf_val * (k1 + 1.0)) / denom;
            (idx, bm25_score)
        })
        .collect();

    // Qdrant requires sparse indices to be sorted in ascending order
    pairs.sort_by_key(|&(idx, _)| idx);

    pairs
}

#[async_trait]
impl SparseEmbeddingProvider for BM25SparseEncoder {
    async fn embed_sparse(&self, text: &str) -> Result<SparseVector> {
        let tokens = tokenize(text);
        if tokens.is_empty() {
            return Ok(SparseVector {
                indices: vec![],
                values: vec![],
            });
        }

        let stats = self.stats.lock().map_err(|e| {
            common::errors::AppError::Internal(anyhow::anyhow!("BM25 stats lock poisoned: {}", e))
        })?;

        let pairs = compute_bm25_weights(&tokens, &stats, self.k1, self.b, self.vocabulary_size);

        let (indices, values) = pairs.into_iter().unzip();
        Ok(SparseVector { indices, values })
    }

    /// Observe a document to update term frequency statistics.
    /// Should be called BEFORE `embed_sparse` during ingestion so the document's
    /// own terms contribute to IDF for subsequent documents (self-IDF is standard BM25 behavior).
    async fn observe_document(&self, text: &str) -> Result<()> {
        let tokens = tokenize(text);
        if tokens.is_empty() {
            return Ok(());
        }

        {
            let mut stats = self.stats.lock().map_err(|e| {
                common::errors::AppError::Internal(anyhow::anyhow!("BM25 stats lock poisoned: {}", e))
            })?;
            stats.observe_tokens(&tokens);
        }

        // Auto-save after every observe if persistence is configured
        if let Some(ref path) = self.stats_path {
            let stats = self.stats.lock().map_err(|e| {
                common::errors::AppError::Internal(anyhow::anyhow!("BM25 stats lock poisoned: {}", e))
            })?;
            stats.save_to_file(path)?;
        }

        Ok(())
    }

    /// Flush statistics to disk immediately.
    async fn persist_stats(&self) -> Result<()> {
        if let Some(ref path) = self.stats_path {
            let stats = self.stats.lock().map_err(|e| {
                common::errors::AppError::Internal(anyhow::anyhow!("BM25 stats lock poisoned: {}", e))
            })?;
            stats.save_to_file(path)?;
            tracing::info!(path = ?path, "BM25 term statistics persisted");
        }
        Ok(())
    }
}

// ─── Local Hashing Sparse Encoder (Legacy) ───────────────────────────────

/// A deterministic local sparse encoder that tokenizes text and hashes words 
/// to a large vocabulary space, applying basic TF (Term Frequency) weight calculation.
///
/// **Deprecated**: Prefer `BM25SparseEncoder` which uses proper BM25 scoring with
/// corpus-level IDF statistics. This encoder remains for backward compatibility
/// and testing purposes.
pub struct LocalHashingSparseEncoder {
    vocabulary_size: u32,
}

impl LocalHashingSparseEncoder {
    pub fn new(vocabulary_size: u32) -> Self {
        Self { vocabulary_size }
    }

    pub fn default() -> Self {
        Self::new(100_000)
    }
}

#[async_trait]
impl SparseEmbeddingProvider for LocalHashingSparseEncoder {
    async fn embed_sparse(&self, text: &str) -> Result<SparseVector> {
        let tokens = tokenize(text);

        if tokens.is_empty() {
            return Ok(SparseVector {
                indices: vec![],
                values: vec![],
            });
        }

        // Count token frequencies mapped to vocabulary index hashes
        let mut index_weights: HashMap<u32, f32> = HashMap::new();
        let total_tokens = tokens.len() as f32;

        for token in tokens {
            let index = hash_term(&token, self.vocabulary_size);
            let entry = index_weights.entry(index).or_insert(0.0);
            *entry += 1.0;
        }

        // Convert and sort by index (Qdrant REQUIRES sparse indices to be sorted in ascending order)
        let mut pairs: Vec<(u32, f32)> = index_weights
            .into_iter()
            .map(|(idx, freq)| {
                // Basic TF-IDF/BM25 approximation: term frequency normalized by document length
                let tf = freq / total_tokens;
                // Add log scaling to dampen high-frequency term domination
                let weight = (1.0 + tf).ln();
                (idx, weight)
            })
            .collect();

        pairs.sort_by_key(|&(idx, _)| idx);

        let (indices, values) = pairs.into_iter().unzip();

        Ok(SparseVector { indices, values })
    }
}
