# Outstanding API Unification Proposal

## Overview

This document outlines the reorganization of Outstanding's public API to create a coherent,
user-friendly interface that matches the design vision in `docs/intro.lex`.

## Key Decision: Merge `outstanding-clap` into `outstanding`

The `outstanding-clap` crate will be folded into `outstanding` core, with clap-specific
functionality behind a `clap` feature flag.

**Rationale:**
- Users almost always need both crates together
- Documentation is awkwardly split between crates
- Re-exports create circular-feeling dependencies
- The clap-specific code is relatively small
- Simplifies dependency management for users

**New structure:**
```
outstanding/
├── src/
│   ├── lib.rs           # Core exports
│   ├── render/          # Template rendering
│   ├── theme/           # Themes and styles
│   ├── cli/             # CLI integration (feature = "clap")
│   │   ├── mod.rs
│   │   ├── builder.rs
│   │   ├── dispatch.rs
│   │   ├── handler.rs
│   │   ├── hooks.rs
│   │   └── help.rs
│   └── ...
```

---

## API Changes

### 1. Run Semantics Fix

**Problem:** `run()` doesn't run anything - it just parses arguments.

**Current (confusing):**
```rust
Outstanding::run(cmd)           // Returns ArgMatches, doesn't execute
builder.run_and_print(cmd, args) // Actually runs and prints
builder.dispatch_from(cmd, args) // Runs, returns result
```

**Proposed (intuitive):**
```rust
// Primary API - matches user expectation
app.run(cmd, args)              // Execute handlers, print output, exit on error
app.run_to_string(cmd, args)    // Execute handlers, return output string

// For manual control
app.parse(cmd, args)            // Just parse, return matches
app.dispatch(matches)           // Execute handler for matches, return Output
```

### 2. Unified Builder

**Problem:** Two incompatible builders (`RenderSetup` vs `OutstandingBuilder`).

**Proposed:** Single `Outstanding::builder()` that supports all use cases:

```rust
let app = Outstanding::builder()
    // Templates - embedded or directory-based
    .templates(embed_templates!("src/templates"))
    .templates_dir("~/.myapp/templates")  // Optional override

    // Styles - embedded or directory-based
    .styles(embed_styles!("src/styles"))
    .styles_dir("~/.myapp/themes")        // Optional override
    .default_theme("dark")

    // Commands (requires "clap" feature)
    .commands(Commands::dispatch_config())  // From derive macro
    .command("list", handler)               // Individual registration
    .group("db", |g| g
        .command("migrate", db_migrate)
        .command("backup", db_backup))

    // Context injection
    .context("version", "1.0.0")
    .context_fn("terminal_width", |ctx| ctx.terminal_width)

    // Build
    .build()?;
```

### 3. Simplified Render Functions

**Problem:** Too many render variants with unclear distinctions.

**Current (7 functions):**
```rust
render()
render_with_output()
render_with_mode()
render_with_context()
render_or_serialize()
render_or_serialize_with_context()
render_or_serialize_with_spec()
```

**Proposed (3 functions + builder for advanced):**
```rust
// Simple rendering
render(template, data, theme) -> String

// With explicit output mode
render_with_mode(template, data, theme, output_mode) -> String

// Auto-dispatch: template for text modes, serialize for structured
render_auto(template, data, theme, output_mode) -> String

// For advanced use cases (context, specs, etc.)
Renderer::new(theme)
    .template(template)
    .context(registry)
    .output_mode(mode)
    .color_mode(color)
    .render(data)?
```

### 4. Type Renames for Clarity

| Current | Proposed | Rationale |
|---------|----------|-----------|
| `Outstanding` (clap) | `App` | Shorter, clearer |
| `OutstandingApp` (core) | `RenderEngine` | Distinguishes from App |
| `OutstandingBuilder` | `AppBuilder` | Follows App rename |
| `RenderSetup` | Merged into `AppBuilder` | Single builder |
| `RunResult::Unhandled` | `RunResult::NoMatch` | Clearer semantics |
| `CommandResult<T>` | `HandlerResult<T>` | More specific |

### 5. Handler Result Refactor

**Problem:** `CommandResult` mixes success/error with output type.

**Current:**
```rust
pub enum CommandResult<T: Serialize> {
    Ok(T),
    Err(anyhow::Error),
    Silent,
    Archive(Vec<u8>, String),
}
```

**Proposed:**
```rust
pub enum Output<T: Serialize> {
    Render(T),                    // Render with template
    Silent,                       // No output
    Binary { data: Vec<u8>, filename: String },
}

// Handler signature becomes:
fn handler(matches, ctx) -> Result<Output<T>, Error>
```

This allows `?` operator and standard error handling.

### 6. Gradual Adoption API

**For adding Outstanding to one command in existing clap app:**

```rust
// In your existing main.rs with manual dispatch:
let matches = cli.get_matches();

match matches.subcommand() {
    Some(("new-feature", sub_m)) => {
        // Use Outstanding for just this command
        let output = outstanding::render_command(
            sub_m,
            new_feature_handler,
            "templates/new_feature.j2",
            &theme,
        )?;
        println!("{}", output);
    }
    // Legacy commands continue unchanged
    Some(("old-cmd", sub_m)) => old_handler(sub_m),
    _ => {}
}
```

**For fallback from auto-dispatch:**

```rust
let app = Outstanding::builder()
    .command("new", new_handler)
    .build()?;

match app.dispatch(matches) {
    DispatchResult::Handled(output) => println!("{}", output),
    DispatchResult::NoMatch(matches) => {
        // Fall back to legacy handling
        legacy_dispatch(matches);
    }
}
```

### 7. Remove Deprecated Items

Per project guidelines (no backwards compatibility), remove:
- `TopicHelper` type alias
- `TopicHelperBuilder` type alias
- `TopicHelpResult` type alias
- `Config` type alias (for `HelpConfig`)

---

## Module Structure After Merge

```
outstanding/
├── Cargo.toml
└── src/
    ├── lib.rs                    # Public API exports
    │
    ├── render/
    │   ├── mod.rs
    │   ├── functions.rs          # render(), render_auto(), etc.
    │   ├── renderer.rs           # Renderer struct
    │   └── registry.rs           # TemplateRegistry
    │
    ├── theme/
    │   ├── mod.rs
    │   ├── theme.rs              # Theme struct
    │   ├── color_mode.rs         # ColorMode, detection
    │   └── style.rs              # Style types
    │
    ├── output/
    │   ├── mod.rs
    │   ├── mode.rs               # OutputMode enum
    │   └── destination.rs        # OutputDestination
    │
    ├── cli/                      # feature = "clap"
    │   ├── mod.rs
    │   ├── app.rs                # App (was Outstanding)
    │   ├── builder.rs            # AppBuilder (unified)
    │   ├── handler.rs            # Handler trait, HandlerResult
    │   ├── dispatch.rs           # Dispatch logic
    │   ├── hooks.rs              # Hook system
    │   ├── help.rs               # Help rendering
    │   └── context.rs            # CommandContext, RenderContext
    │
    ├── topics/
    │   ├── mod.rs
    │   ├── topic.rs
    │   └── registry.rs
    │
    ├── table/                    # Table formatting
    ├── stylesheet/               # YAML stylesheet loading
    ├── embedded.rs               # Embedded resource types
    └── util.rs
```

---

## Feature Flags

```toml
[features]
default = ["clap", "macros"]
clap = ["dep:clap"]              # CLI integration
macros = ["dep:outstanding-macros"]  # embed_*! macros
```

---

## Public API Surface (Post-Unification)

```rust
// Core rendering (always available)
pub use render::{render, render_auto, render_with_mode, Renderer};
pub use theme::{Theme, ColorMode, detect_color_mode};
pub use output::{OutputMode, OutputDestination};
pub use topics::{Topic, TopicRegistry};

// CLI integration (feature = "clap")
#[cfg(feature = "clap")]
pub use cli::{
    App, AppBuilder,
    Handler, HandlerResult, Output,
    CommandContext, DispatchResult,
    Hooks, HookPhase,
};

// Macros (feature = "macros")
#[cfg(feature = "macros")]
pub use outstanding_macros::{embed_templates, embed_styles, Dispatch};
```

---

## Migration Examples

### Before (current API)
```rust
use outstanding::{render_or_serialize, Theme, OutputMode};
use outstanding_clap::{Outstanding, CommandResult, CommandContext};

let matches = Outstanding::builder()
    .command("list", |m, ctx| {
        CommandResult::Ok(list_items())
    }, "{% for item in items %}{{ item }}\n{% endfor %}")
    .build()
    .run_with(Cli::command());
```

### After (new API)
```rust
use outstanding::{App, Theme, OutputMode, HandlerResult, Output};

App::builder()
    .command("list", |m, ctx| {
        Ok(Output::Render(list_items()))
    })
    .template("list", "{% for item in items %}{{ item }}\n{% endfor %}")
    .build()?
    .run(Cli::command(), std::env::args());
```

---

## Implementation Phases

The work is broken into small, self-contained commits. Each phase should leave
the codebase in a working state with passing tests.

---

### Phase 1: Preparation (Non-Breaking)

**1.1 Remove deprecated type aliases**
- Delete `TopicHelper`, `TopicHelperBuilder`, `TopicHelpResult`, `Config` aliases
- Update any internal usage
- ~30 min

**1.2 Consolidate duplicate render functions**
- `render_with_output()` and `render_with_mode()` have overlapping purposes
- Audit and document the actual difference (output mode vs color mode)
- Add deprecation notes in code comments for functions to be merged later
- ~1 hr

**1.3 Add feature flag structure to outstanding**
- Add `[features]` section to `outstanding/Cargo.toml`
- Create empty `src/cli/mod.rs` behind `clap` feature
- Ensure existing API unchanged
- ~30 min

---

### Phase 2: Crate Merge (Structural)

**2.1 Copy outstanding-clap source into outstanding/src/cli/**
- Move all source files
- Update internal `use` statements to reference sibling modules
- Keep `outstanding-clap` crate temporarily (will delete later)
- ~2 hr

**2.2 Update outstanding/Cargo.toml dependencies**
- Add clap dependency behind feature flag
- Add other outstanding-clap dependencies (anyhow, etc.)
- ~30 min

**2.3 Wire up cli module exports**
- Add `pub mod cli` behind `#[cfg(feature = "clap")]`
- Export types from `lib.rs`
- ~1 hr

**2.4 Create outstanding-clap as thin re-export crate**
- Replace outstanding-clap/src/lib.rs with `pub use outstanding::cli::*;`
- Add deprecation notice to crate docs
- Ensures existing users still compile
- ~30 min

**2.5 Delete outstanding-clap crate**
- Remove from workspace
- Delete crate directory
- Update workspace Cargo.toml
- ~15 min

---

### Phase 3: Builder Unification

**3.1 Add `.templates()` method to AppBuilder**
- Accept `EmbeddedTemplates` (like RenderSetup does)
- Store alongside existing `template_dir`
- ~1 hr

**3.2 Unify RenderSetup into AppBuilder**
- Move RenderSetup's build logic into AppBuilder
- AppBuilder now handles both embedded resources AND command dispatch
- RenderSetup becomes type alias (temporarily) or internal
- ~2 hr

**3.3 Make builder.build() return Result**
- Change signature from `fn build(self) -> Outstanding` to `fn build(self) -> Result<App, SetupError>`
- Update all call sites
- ~1 hr

---

### Phase 4: Run Semantics Fix

**4.1 Rename current run methods (preparation)**
- `Outstanding::run()` → `Outstanding::parse()` (internal rename, keep old as deprecated alias)
- `Outstanding::run_with()` → `Outstanding::parse_with()`
- `Outstanding::run_from()` → `Outstanding::parse_from()`
- ~1 hr

**4.2 Implement new run() that executes and prints**
- New `fn run(self, cmd, args) -> !` that calls dispatch + prints + exits
- This is what users expect
- ~1 hr

**4.3 Implement run_to_string()**
- New `fn run_to_string(&self, cmd, args) -> Result<String, Error>`
- For testing and composition
- ~1 hr

**4.4 Remove deprecated parse aliases**
- Delete the old `run()` → `parse()` aliases
- Final cleanup
- ~30 min

---

### Phase 5: Type Renames

**5.1 Rename Outstanding → App**
- Rename struct and impl blocks
- Update all references
- ~1 hr

**5.2 Rename OutstandingBuilder → AppBuilder**
- Rename struct
- Update builder() method
- ~30 min

**5.3 Rename OutstandingApp → RenderEngine**
- Or merge into Renderer
- Evaluate if this type is still needed
- ~1 hr

**5.4 Rename RunResult::Unhandled → RunResult::NoMatch**
- Simple enum variant rename
- ~15 min

---

### Phase 6: Handler Result Refactor

**6.1 Create new Output<T> enum**
- Define new enum alongside CommandResult
- ~30 min

**6.2 Create HandlerResult type alias**
- `type HandlerResult<T> = Result<Output<T>, anyhow::Error>`
- ~15 min

**6.3 Update Handler trait to use new types**
- Change return type from `CommandResult<T>` to `HandlerResult<T>`
- Update all handler implementations
- ~2 hr

**6.4 Remove old CommandResult enum**
- Delete after all usages migrated
- ~30 min

---

### Phase 7: Render Function Cleanup

**7.1 Implement render_auto()**
- New function that chooses template vs serialize based on OutputMode
- ~1 hr

**7.2 Deprecate redundant render functions**
- Mark render_or_serialize* as deprecated in favor of render_auto
- ~30 min

**7.3 Remove deprecated render functions**
- Delete after deprecation period (or immediately per project guidelines)
- ~30 min

---

### Phase 8: Documentation and Polish

**8.1 Update lib.rs documentation**
- Rewrite module docs to reflect new API
- Add migration guide
- ~2 hr

**8.2 Update intro.lex examples**
- Ensure all code examples work with new API
- ~1 hr

**8.3 Update README and other docs**
- Sync all documentation
- ~1 hr

---

## Commit Strategy

Each numbered item (1.1, 1.2, etc.) should be a single commit with:
- Descriptive commit message referencing this proposal
- All tests passing
- No breaking changes to external API until Phase 4+

Example commit messages:
```
refactor: remove deprecated type aliases (Phase 1.1)

Remove TopicHelper, TopicHelperBuilder, TopicHelpResult, and Config
type aliases per API unification proposal.

Ref: docs/proposals/new-api.md
```

```
feat: merge outstanding-clap into outstanding core (Phase 2.1-2.3)

Move CLI integration code into outstanding crate behind "clap" feature.
This is a structural change - API remains unchanged.

Ref: docs/proposals/new-api.md
```

---

## Risk Mitigation

**Testing strategy:**
- Run full test suite after each commit
- Phase 2 (crate merge) is highest risk - take extra care
- Consider temporary CI job that tests both old and new import paths

**Rollback points:**
- After Phase 2.4, old outstanding-clap still works (re-exports)
- After Phase 3, builder unification complete but types unchanged
- After Phase 4, run semantics fixed but types unchanged

**Dependencies:**
- Phase 1 has no dependencies
- Phase 2 depends on Phase 1.3
- Phase 3 depends on Phase 2
- Phases 4-7 can proceed in parallel after Phase 3
- Phase 8 depends on all others

---

## Timeline Estimate

| Phase | Estimated Time | Dependencies |
|-------|---------------|--------------|
| 1. Preparation | 2 hr | None |
| 2. Crate Merge | 4 hr | 1.3 |
| 3. Builder Unification | 4 hr | 2 |
| 4. Run Semantics | 4 hr | 3 |
| 5. Type Renames | 3 hr | 3 |
| 6. Handler Refactor | 4 hr | 3 |
| 7. Render Cleanup | 2 hr | 3 |
| 8. Documentation | 4 hr | All |

**Total: ~27 hours of focused work**

Phases 4-7 can be parallelized if multiple contributors, reducing wall-clock time.
