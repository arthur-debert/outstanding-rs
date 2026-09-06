//! The book's two checks, as ordinary Rust.
//!
//! [`examples`] names the pages whose fenced `rust` blocks rustdoc compiles, so
//! `cargo test --doc -p standout-docs` fails when a page teaches an API that no
//! longer exists; a block that is a fragment by design carries `ignore`.
//! [`book`] walks the book instead of compiling it: reachability from
//! `docs/SUMMARY.md`, and relative links that resolve.

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
