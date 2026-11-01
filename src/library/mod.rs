//! Library management: folder scanner, metadata tagger, SQLite database, cover cache, and audit tool.

pub mod audit;

pub use audit::{AuditReport, FileAuditResult, FileAuditStatus, audit_single_file, run_audit};
