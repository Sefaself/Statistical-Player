//! Library management: folder scanner, metadata tagger, SQLite database, cover cache, and audit tool.

pub mod audit;
pub mod loader;

pub use audit::{AuditReport, FileAuditResult, FileAuditStatus, audit_single_file, run_audit};
pub use loader::{LoaderEvent, load_folder, natural_cmp, sort_tracks};
