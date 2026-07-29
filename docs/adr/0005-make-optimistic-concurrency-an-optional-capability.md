# Make optimistic concurrency an optional capability

Optimistic concurrency is an opt-in compile-time Resource capability with a Resource-defined version-token type on relevant patch, delete, and action interfaces and a shared typed conflict outcome. It is distinct from synchronization: the tiny in-memory adapter can serialize operations with process-local Rust primitives without declaring the capability, while remote or filesystem adapters may map it to ETags, revisions, or hashes and remain responsible for their own transport or cross-process coordination.
