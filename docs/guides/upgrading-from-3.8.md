# Upgrading from Standout 3.8.0 to 6.0.0

Standout went through three major version bumps since 3.8.0 (4.0, 5.0, 6.0). This guide covers everything you need to change to get your code compiling and working on the latest version.

## Quick Summary

| Version | What Changed |
| --------- | ------------- |
| **4.0.0** | `App`/`LocalApp` unified into single-threaded `App` |
| **5.0.0** | New `standout-input` crate (additive, no breakage) |
| **6.0.0** | Internal dispatch fix for theme ordering (transparent for most users) |

The only version that requires code changes for most users is **4.0.0**.

## Step 1: Update Cargo.toml

```diff
[dependencies]
- standout = "3.8"
+ standout = "6"
```

## Step 2: Remove LocalApp / ThreadSafe / Local types (v4.0.0)

The dual `App`/`LocalApp` architecture has been removed. CLI apps are single-threaded, so the thread-safety distinction was unnecessary.

### Removed types

These types no longer exist:

- `LocalApp`, `LocalAppBuilder`
- `LocalHandler`
- `Local`, `ThreadSafe` marker types
- `HandlerMode` trait

### Update imports

```diff
- use standout::cli::{App, ThreadSafe, LocalApp, LocalHandler};
+ use standout::cli::{App, Handler};
```

### Update App::builder() calls

`App::builder()` no longer takes a generic type parameter:

```diff
- App::<ThreadSafe>::builder()
+ App::builder()
      .command("list", handler, template)?
      .build()?
```

If you were using `LocalApp`:

```diff
- LocalApp::builder()
+ App::builder()
      .command("list", handler, template)?
      .build()?
```

### Update Handler implementations

`Handler::handle()` now takes `&mut self` instead of `&self`:

```diff
  impl Handler for MyHandler {
-     fn handle(&self, m: &ArgMatches, ctx: &CommandContext) -> HandlerResult<T> {
+     fn handle(&mut self, m: &ArgMatches, ctx: &CommandContext) -> HandlerResult<T> {
          // ...
      }
  }
```

This means you can mutate handler state directly, without `Arc<Mutex<_>>` wrappers.

### Update closure handlers

Handler closures are now `FnMut` instead of `Fn`:

```diff
- let handler: Box<dyn Fn(&ArgMatches, &CommandContext) -> HandlerResult<T>> = ...;
+ let handler: Box<dyn FnMut(&ArgMatches, &CommandContext) -> HandlerResult<T>> = ...;
```

In practice, most closures work without changes. The difference matters only if you were explicitly annotating types.

### Update CommandContext.app_state references

```diff
- use std::sync::Arc;
- let state: Arc<Extensions> = ctx.app_state.clone();
+ use std::rc::Rc;
+ let state: Rc<Extensions> = ctx.app_state.clone();
```

`Arc` has been replaced with `Rc` throughout since thread-safety is no longer needed.

## Step 3: Check for custom DispatchFn (v6.0.0)

This only affects you if you wrote custom dispatch functions using the internal `DispatchFn` type. The signature changed to accept `&Theme` at runtime:

```diff
- type DispatchFn = Box<dyn Fn(ArgMatches, &CommandContext) -> RunResult>;
+ type DispatchFn = Box<dyn Fn(ArgMatches, &CommandContext, &Theme) -> RunResult>;
```

If you only use the public API (`.command()`, `.commands()`, `#[derive(Dispatch)]`), this change is transparent. The benefit is that `.theme()` and `.commands()` can now be called in any order.

## Common Compiler Errors and Fixes

### `cannot find type LocalApp in module cli`

Replace `LocalApp` with `App`. See Step 2.

### `cannot find type ThreadSafe`

Remove the type parameter from `App::<ThreadSafe>::builder()`. See Step 2.

### `method handle has an incompatible type for trait`

Change `&self` to `&mut self` in your `Handler` impl. See Step 2.

### `expected Rc, found Arc`

Replace `Arc` with `Rc` for `app_state` references. See Step 2.

## New Features Available After Upgrading

These are additive and don't require changes, but you may want to take advantage of them:

### standout-input (v5.0.0)

Declarative input collection from multiple sources with fallback chains:

```rust
use standout_input::{InputChain, ArgSource, StdinSource, EditorSource};

let message = InputChain::<String>::new()
    .try_source(ArgSource::new("message"))
    .try_source(StdinSource::new())
    .try_source(EditorSource::new())
    .resolve(&matches)?;
```

Sources include: CLI args, stdin, environment variables, clipboard, editor, and interactive prompts. See [Introduction to Input](../../crates/standout-input/docs/guides/intro-to-input.md) for the full guide.

### #[handler] macro (v3.6.1)

If you're still writing handlers with manual `ArgMatches` extraction, the `#[handler]` macro eliminates boilerplate:

```rust
// Before
fn list(m: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<Vec<Item>> {
    let all = m.get_flag("all");
    let items = storage::list(all)?;
    Ok(Output::Render(items))
}

// After
#[handler]
fn list(#[flag] all: bool) -> Result<Vec<Item>, Error> {
    storage::list(all)
}
```

### Auto-wrap `Result<T>` (v3.6.1)

Handlers can return `Result<T, E>` directly instead of `Ok(Output::Render(...))`:

```rust
fn list(m: &ArgMatches, ctx: &CommandContext) -> Result<Vec<Item>, Error> {
    storage::list()  // no more Ok(Output::Render(...)) wrapping
}
```

### Output piping (v3.6.1)

Pipe handler output to external commands or the clipboard:

```rust
App::builder()
    .commands(|g| {
        g.command_with("list", handlers::list, |cfg| {
            cfg.template("list.jinja")
               .pipe_through("jq '.data'")
        })
    })
```
