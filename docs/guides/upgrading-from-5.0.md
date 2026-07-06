# Upgrading from Standout 5.0.0 to 6.0.0

This is a minor upgrade. Only one internal change landed, and it is transparent for most users.

## Quick Summary

| Version   | What Changed                                                           |
| --------- | ---------------------------------------------------------------------- |
| **6.0.0** | Internal dispatch fix: theme resolved at runtime instead of build time |

## Step 1: Update Cargo.toml

```diff
[dependencies]
- standout = "5"
+ standout = "6"
```

## Step 2: Check for custom DispatchFn (unlikely)

This only affects you if you wrote custom dispatch functions using the internal `DispatchFn` type directly. The signature changed to accept `&Theme` at runtime:

```diff
- type DispatchFn = Box<dyn Fn(ArgMatches, &CommandContext) -> RunResult>;
+ type DispatchFn = Box<dyn Fn(ArgMatches, &CommandContext, &Theme) -> RunResult>;
```

If you only use the public API (`.command()`, `.commands()`, `#[derive(Dispatch)]`), **no code changes are needed**. The benefit is that `.theme()` and `.commands()` can now be called in any order without the theme being silently ignored.

## That's It

If you weren't using internal dispatch types, upgrading is just a version bump. All public APIs are unchanged.
