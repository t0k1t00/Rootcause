//! Exporting a [`crate::report::BenchmarkReport`] to JSON, CSV, or
//! Markdown.
//!
//! Every format is a projection of the same [`crate::report::BenchmarkReport`]
//! (see that type's own docs) built from an intermediate, purely-owned
//! `json::ReportView` so the JSON export does not need `serde` impls
//! on this crate's richer internal types (several of which wrap types
//! from `matcher`/`grounding`/`dsl` that do not themselves implement
//! `Serialize`).

pub mod csv;
pub mod json;
pub mod markdown;

pub use csv::to_csv;
pub use json::to_json;
pub use markdown::to_markdown;
