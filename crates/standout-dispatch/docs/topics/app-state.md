# App State and Extensions

`CommandContext` provides two mechanisms for state injection: **app state** (shared, immutable) and **extensions** (per-request, mutable). Understanding the distinction is key to building clean, testable CLI applications.

---

## The Two State Types

| Aspect | `app_state` | `extensions` |
| -------- | ------------- | -------------- |
| **Mutability** | Immutable (`&`) | Mutable (`&mut`) |
| **Lifetime** | App lifetime | Per-request |
| **Set by** | `AppBuilder::app_state()` | Pre-dispatch hooks |
| **Storage** | `Rc<Extensions>` | `Extensions` |
| **Use for** | Database, Config, API clients | User sessions, request IDs |

---

## App State: Shared Resources

App state is configured once at build time and shared immutably across all command dispatches. Use it for long-lived resources that are expensive to create or need to be shared.

### Setup

```rust
use standout::cli::App;

struct Database { pool: Pool }
struct Config { api_url: String, debug: bool }
struct ApiClient { base_url: String }

let app = App::builder()
    .app_state(Database::connect()?)
    .app_state(Config::load()?)
    .app_state(ApiClient { base_url: "https://api.example.com".into() })
    .commands(|g| g.command_with("list", list_handler, |c| c.template_name("list")))?
    .build()?;
```

### Accessing App State in Handlers

```rust
fn list_handler(matches: &ArgMatches, ctx: &CommandContext) -> HandlerResult<Vec<Item>> {
    // Get required state (returns error if not found)
    let db = ctx.app_state.get_required::<Database>()?;
    let config = ctx.app_state.get_required::<Config>()?;

    // Optional state (returns None if not found)
    let api = ctx.app_state.get::<ApiClient>();

    let items = db.list_items(&config.api_url)?;
    Ok(Output::Render(items))
}
```

### Type Safety

Each type can only be stored once. Storing a second value of the same type replaces the first:

```rust
App::builder()
    .app_state(Config { debug: false })
    .app_state(Config { debug: true })  // Replaces previous Config
```

If you need multiple instances of the same type, wrap them in distinct newtype wrappers:

```rust
struct PrimaryDb(Pool);
struct AnalyticsDb(Pool);

App::builder()
    .app_state(PrimaryDb(primary_pool))
    .app_state(AnalyticsDb(analytics_pool))
```

---

## Extensions: Per-Request State

Extensions are mutable and scoped to a single command dispatch. Pre-dispatch hooks inject state that handlers consume. Each dispatch starts with empty extensions.

### Injection via Hooks

```rust
use standout_dispatch::{Hooks, HookError};

struct UserScope { user_id: String, permissions: Vec<String> }
struct RequestId(String);

let hooks = Hooks::new()
    .pre_dispatch(|matches, ctx| {
        // Parse user from args or environment
        let user_id = matches.get_one::<String>("user")
            .cloned()
            .unwrap_or_else(|| std::env::var("USER").unwrap_or_default());

        // Look up permissions (could use app_state here!)
        let db = ctx.app_state.get_required::<Database>()?;
        let permissions = db.get_permissions(&user_id)?;

        // Inject per-request state
        ctx.extensions.insert(UserScope { user_id, permissions });
        ctx.extensions.insert(RequestId(uuid::Uuid::new_v4().to_string()));

        Ok(())
    });
```

### Accessing Extensions in Handlers

```rust
fn list_handler(matches: &ArgMatches, ctx: &CommandContext) -> HandlerResult<Vec<Item>> {
    // App state: shared database
    let db = ctx.app_state.get_required::<Database>()?;

    // Extensions: per-request user scope
    let scope = ctx.extensions.get_required::<UserScope>()?;

    // Use both
    let items = db.list_items_for_user(&scope.user_id)?;
    Ok(Output::Render(items))
}
```

---

## When to Use Which

### Use App State For

- **Database connections** - Expensive to create, should be pooled
- **Configuration** - Loaded once at startup
- **API clients** - Shared HTTP clients with connection pooling
- **Caches** - Shared lookup tables or memoization
- **Feature flags** - Global toggles loaded at startup

### Use Extensions For

- **User context** - Current user, session, permissions
- **Request metadata** - Request ID, timing, correlation ID
- **Scoped overrides** - Per-request configuration overrides
- **Transient state** - Data computed by one hook, used by handler

---

## The Hook + Handler Pattern

A common pattern is using pre-dispatch hooks to set up request-scoped state that handlers consume:

```rust
// In builder setup
App::builder()
    .app_state(Database::connect()?)
    .app_state(PermissionService::new())
    .commands(|g| g.command_with("admin.delete", admin_delete_handler, |c| c.template_name("admin.delete")))?
    .hooks("admin.delete", Hooks::new()
        .pre_dispatch(|matches, ctx| {
            // Validate admin permissions using app state
            let perms = ctx.app_state.get_required::<PermissionService>()?;
            let user = std::env::var("USER").unwrap_or_default();

            if !perms.is_admin(&user)? {
                return Err(HookError::pre_dispatch("Admin access required"));
            }

            // Inject validated user context
            ctx.extensions.insert(AdminUser { name: user });
            Ok(())
        }))
    .build()?
```

```rust
// Handler can assume validation passed
fn admin_delete_handler(matches: &ArgMatches, ctx: &CommandContext) -> HandlerResult<DeleteResult> {
    let db = ctx.app_state.get_required::<Database>()?;
    let admin = ctx.extensions.get_required::<AdminUser>()?;

    let id = matches.get_one::<String>("id").unwrap();
    db.delete_with_audit(id, &admin.name)?;

    Ok(Output::Render(DeleteResult { id: id.clone() }))
}
```

---

## Error Handling

### get_required vs get

Use `get_required` when the state must be present (fail fast):

```rust
// Fails with clear error if Database not configured
let db = ctx.app_state.get_required::<Database>()?;
```

Use `get` when state is optional:

```rust
// Returns None if optional feature not configured
if let Some(cache) = ctx.app_state.get::<Cache>() {
    if let Some(cached) = cache.get(key) {
        return Ok(Output::Render(cached));
    }
}
```

### Error Messages

`get_required` produces descriptive errors:

```text
Extension missing: type myapp::Database not found in context
```

---

## Testing with App State

App state makes handlers easily testable by allowing dependency injection:

```rust
use std::rc::Rc;

#[test]
fn test_list_handler() {
    // Create test fixtures
    let mock_db = MockDatabase::with_items(vec![
        Item { id: "1", name: "Test" }
    ]);

    // Build context with test state
    let mut app_state = Extensions::new();
    app_state.insert(mock_db);

    let ctx = CommandContext {
        command_path: vec!["list".into()],
        app_state: Rc::new(app_state),
        extensions: Extensions::new(),
    };

    // Test handler
    let cmd = Command::new("test");
    let matches = cmd.get_matches_from(["test"]);

    let result = list_handler(&matches, &ctx);
    assert!(result.is_ok());
}
```

---

## Single-Threaded Design

App state is wrapped in `Rc<Extensions>` for cheap cloning within the single-threaded dispatch system. Since CLI apps are fundamentally single-threaded (parse → run one handler → output → exit), there are no thread-safety requirements on app state values.

```rust
// Both work - no Send + Sync requirements
app_state(Database { pool: Pool::new() })
app_state(Wrapper { rc: Rc::new(data) })  // Works fine
```

---

## Summary

- **App state** = shared, immutable, configured at build time
- **Extensions** = per-request, mutable, set by hooks
- Use `get_required` for mandatory dependencies
- Hooks can read app state to populate extensions
- Both types use the same `Extensions` API for access
