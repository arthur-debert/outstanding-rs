# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.9.0] - 2026-01-10

## [0.8.0] - 2026-01-10

## [0.7.2] - 2026-01-10

### Added

- **Post-dispatch hooks** - New hook phase that runs after handler execution but before rendering
  - `post_dispatch` hooks receive raw handler data as `serde_json::Value`
  - Can inspect, modify, or replace data before it's rendered
  - Useful for data enrichment, validation, filtering, and normalization
  - Full access to `ArgMatches` and `CommandContext` in hook functions
- `HookError::post_dispatch()` factory method for creating post-dispatch errors
- `HookPhase::PostDispatch` variant for error phase tracking
- `serde_json` dependency added to `outstanding-clap` (previously dev-only)

### Example

```rust
use outstanding_clap::{Outstanding, Hooks, HookError};
use serde_json::json;

Outstanding::builder()
    .command("list", handler, template)
    .hooks("list", Hooks::new()
        .pre_dispatch(|_m, ctx| {
            println!("Running: {}", ctx.command_path.join(" "));
            Ok(())
        })
        .post_dispatch(|_m, _ctx, mut data| {
            // Add metadata before rendering
            if let Some(obj) = data.as_object_mut() {
                obj.insert("timestamp".into(), json!(chrono::Utc::now().to_rfc3339()));
            }
            Ok(data)
        })
        .post_output(|_m, _ctx, output| {
            // Transform or inspect output
            Ok(output)
        }))
    .run_and_print(cmd, args);
```

## [0.7.1] - 2026-01-10

## [0.7.0] - 2026-01-10

### Added

- **Hook system for pre/post command execution** - Register custom callbacks that run before and after command handlers execute
  - `pre_dispatch` hooks: Run before command handler, can abort execution
  - `post_output` hooks: Run after output is generated, can transform output or abort
  - Multiple hooks can be chained at each phase
  - Full access to `ArgMatches` and `CommandContext` in hook functions
- New `Output` enum for hook output handling:
  - `Output::Text(String)` - Text output from templates
  - `Output::Binary(Vec<u8>, String)` - Binary output with filename
  - `Output::Silent` - No output
- `HookError` type with phase information (`PreDispatch` / `PostOutput`)
- `Hooks::new()` builder with fluent `.pre_dispatch()` and `.post_output()` methods

### Example

```rust
use outstanding_clap::{Outstanding, Hooks, Output, HookError};

Outstanding::builder()
    .command("list", handler, template)
    .hooks("list", Hooks::new()
        .pre_dispatch(|matches, ctx| {
            println!("Running: {}", ctx.command_path.join(" "));
            Ok(())
        })
        .post_output(|matches, ctx, output| {
            // Transform or inspect output
            Ok(output)
        }))
    .run_and_print(cmd, args);
```

## [0.6.2] - 2025-01-10

### Changed

- Code reorganization: split `lib.rs` into focused modules for both `outstanding` and `outstanding-clap` crates

## [0.6.1] - 2025-01-09

### Changed

- Switched to cargo-release for publishing

## [0.6.0] - 2025-01-09

### Added

- Tabular output support with `TableFormatter` and MiniJinja filters
- Width resolution algorithm for responsive column layouts
- ANSI-aware text manipulation utilities
- `OutputMode::Json` for structured output
- `render_or_serialize()` method for conditional rendering/serialization
- Command handler system with `dispatch_from` convenience method
- Archive variant support in clap integration

[Unreleased]: https://github.com/arthur-debert/outstanding-rs/compare/v0.9.0...HEAD
[0.9.0]: https://github.com/arthur-debert/outstanding-rs/compare/v0.8.0...v0.9.0
[0.8.0]: https://github.com/arthur-debert/outstanding-rs/compare/v0.7.2...v0.8.0
[0.7.2]: https://github.com/arthur-debert/outstanding-rs/compare/v0.7.1...v0.7.2
[0.7.1]: https://github.com/arthur-debert/outstanding-rs/compare/v0.7.0...v0.7.1
[0.7.0]: https://github.com/arthur-debert/outstanding-rs/compare/v0.6.2...v0.7.0
[0.6.2]: https://github.com/arthur-debert/outstanding-rs/compare/v0.6.1...v0.6.2
[0.6.1]: https://github.com/arthur-debert/outstanding-rs/compare/v0.6.0...v0.6.1
[0.6.0]: https://github.com/arthur-debert/outstanding-rs/releases/tag/v0.6.0
