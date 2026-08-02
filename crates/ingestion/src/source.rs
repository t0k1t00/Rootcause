//! [`TraceSource`]: the extensibility point for where raw trace bytes
//! come from.
//!
//! Per ADR-0007, this crate ships two network-free sources
//! ([`InMemoryTraceSource`], [`FileTraceSource`]) sufficient to exercise
//! the full pipeline against realistic fixtures. A real archive-node RPC
//! client is a future implementation of this same trait, added without
//! modifying [`crate::decode`], [`crate::normalize`], or [`crate::build`].

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::IngestionError;

/// A source of raw trace bytes, in the [`crate::raw::RawTraceDocument`]
/// JSON shape (ADR-0007).
///
/// ## Why `load` returns owned `Vec<u8>`, not a `Read` or a borrowed
/// slice
/// A source is not necessarily backed by data that already lives in
/// memory (a file, or eventually an HTTP response body, must be read
/// into a buffer regardless), and [`crate::decode::decode`] needs the
/// complete bytes to hand to `serde_json` in one call. Returning owned
/// bytes keeps the trait object-safe (`dyn TraceSource`) and keeps
/// lifetime management entirely inside each implementation, rather than
/// leaking a source's own internal borrow shape into every caller.
pub trait TraceSource {
    /// Load the complete raw trace document bytes.
    ///
    /// # Errors
    /// Returns [`IngestionError::SourceUnavailable`] if the bytes could
    /// not be loaded (e.g. a file does not exist). Does **not** validate
    /// that the bytes are well-formed JSON or match the expected schema
    /// — that is [`crate::decode::decode`]'s responsibility, kept
    /// separate so a source implementation never needs to know anything
    /// about the raw schema's shape.
    fn load(&self) -> Result<Vec<u8>, IngestionError>;
}

/// A [`TraceSource`] backed by bytes already held in memory — the
/// source used by every test fixture in this crate, and the natural
/// choice for a caller that has already obtained trace JSON by some
/// other means (e.g. a CLI reading from stdin).
#[derive(Debug, Clone)]
pub struct InMemoryTraceSource {
    bytes: Vec<u8>,
}

impl InMemoryTraceSource {
    /// Wrap raw bytes as a `TraceSource`.
    #[must_use]
    pub fn new(bytes: impl Into<Vec<u8>>) -> Self {
        Self {
            bytes: bytes.into(),
        }
    }
}

impl TraceSource for InMemoryTraceSource {
    fn load(&self) -> Result<Vec<u8>, IngestionError> {
        Ok(self.bytes.clone())
    }
}

/// A [`TraceSource`] backed by a JSON file on disk — the natural choice
/// for an offline trace dump, per the task's explicit "additional
/// sources (offline trace dumps, JSON fixtures, etc.)" requirement.
#[derive(Debug, Clone)]
pub struct FileTraceSource {
    path: PathBuf,
}

impl FileTraceSource {
    /// Create a source that will read from `path` when [`Self::load`]
    /// is called. Does not touch the filesystem until then — a
    /// `FileTraceSource` for a nonexistent path is constructible and
    /// only fails at `load()` time, consistent with every other
    /// fallible operation in this crate reporting errors through
    /// `Result` rather than at construction.
    #[must_use]
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }
}

impl TraceSource for FileTraceSource {
    fn load(&self) -> Result<Vec<u8>, IngestionError> {
        fs::read(&self.path).map_err(|e| IngestionError::SourceUnavailable {
            reason: format!("could not read {}: {e}", self.path.display()),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    #[test]
    fn in_memory_source_returns_its_bytes() {
        let source = InMemoryTraceSource::new(b"hello".to_vec());
        assert_eq!(source.load().unwrap(), b"hello");
    }

    #[test]
    fn file_source_reads_existing_file() {
        let path = std::env::temp_dir().join(format!(
            "rootcause-ingestion-test-{}.json",
            std::process::id()
        ));
        {
            let mut file = fs::File::create(&path).unwrap();
            file.write_all(b"trace-bytes").unwrap();
        }
        let source = FileTraceSource::new(&path);
        assert_eq!(source.load().unwrap(), b"trace-bytes");
        fs::remove_file(&path).ok();
    }

    #[test]
    fn file_source_reports_missing_file() {
        let source = FileTraceSource::new("/nonexistent/path/does/not/exist.json");
        let result = source.load();
        assert!(matches!(
            result,
            Err(IngestionError::SourceUnavailable { .. })
        ));
    }
}
