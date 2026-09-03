//! The book's two checks, as ordinary Rust.
//!
//! [`examples`] names the pages whose fenced `rust` blocks rustdoc compiles, so
//! `cargo test --doc -p standout-docs` fails when a page teaches an API that no
//! longer exists. A page enters that list once its examples are written to
//! stand on their own; a block that is a fragment by design carries `ignore`.
//!
//! [`book`] walks the book instead of compiling it: every page under the
//! mounted roots must be reachable from `docs/SUMMARY.md`, and every relative
//! link between pages must resolve to a file that exists and, when the link
//! carries a fragment, to a heading that exists. `tests/book.rs` runs both
//! walks against this repository; the unit tests in [`book`] run them against
//! fixtures that are deliberately broken, which is what proves a break fails.

pub mod book;

/// The pages whose examples rustdoc compiles.
pub mod examples {
    #[doc = include_str!("../../../docs/topics/dispatch-attributes.md")]
    pub mod dispatch_attributes {}

    #[doc = include_str!("../../../docs/topics/stability.md")]
    pub mod stability {}

    #[doc = include_str!("../../standout-dispatch/docs/topics/handler-contract.md")]
    pub mod handler_contract {}

    #[doc = include_str!("../../../docs/topics/config-files.md")]
    pub mod config_files {}
}
