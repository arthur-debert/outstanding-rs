//! The one sha256 hex fingerprint used by every pin in the run report.
//!
//! Spec, acceptance-suite, docs-snapshot, and produced-binary pins all
//! record the same shape: lowercase hex over a sha256 digest. Encoding it
//! here once keeps the pins byte-comparable across modules.

use sha2::{Digest, Sha256};

/// sha256 of `bytes`, lowercase hex.
pub fn sha256_hex(bytes: impl AsRef<[u8]>) -> String {
    hex(Sha256::digest(bytes.as_ref()))
}

/// Lowercase hex of an already-finished digest (for incremental hashing).
pub fn hex(digest: impl AsRef<[u8]>) -> String {
    digest.as_ref().iter().map(|b| format!("{b:02x}")).collect()
}
