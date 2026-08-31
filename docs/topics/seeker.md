# Seeker: Filtering, Ordering, and Query Strings

Commands that list something almost always grow filter flags: `--status`,
`--name-contains`, `--priority-gte`, `--order-by`. Seeker is the crate behind
`standout::seeker` that gives those filters a single implementation: a
`Query` built either programmatically or by parsing a flat set of key/value
pairs (typically `--filter key=value` flags, or a raw query string), run
against a slice of items through a per-type accessor function.

Reach for it when a command needs to filter, sort, or paginate an in-memory
collection and you want the filter vocabulary (`eq`, `contains`, `gte`,
`before`, `in`, ordering, `limit`/`offset`) to be consistent across commands
instead of each one inventing its own flags.

## Where it lives

```rust
use standout::seeker::{
    Dir, Op, OrderBy, Query, SeekType, SeekerEnum, SeekerSchema, Seekable, Value, parse_query,
};
use standout::Seekable; // #[derive(Seekable)]
```

`standout::seeker` is a re-export of the `standout-seeker` crate
(`pub use standout_seeker as seeker;`); the derive macro is
`standout_macros::Seekable`, re-exported as `standout::Seekable`.

## Deriving `Seekable`

`#[derive(Seekable)]` on a struct with named fields generates:

- An implementation of the `Seekable` trait: `seeker_field_value(&self, field:
  &str) -> Value<'_>`, the trait's one required method. The
  `Self::accessor(item, field)` function `Query::filter` and friends expect is
  a provided method on the trait, so it comes with any implementation, derived
  or written by hand.
- An implementation of `SeekerSchema`, providing `field_type(field)` and
  `field_names()` — this is what `parse_query` uses to validate a query string
  against the struct's actual fields. It does **not** provide
  `resolve_enum_variant(field, variant)`, which keeps the trait's default
  `None`; see [Enum fields](#enum-fields) for what that costs.
- One `pub const` per seekable field, upper-cased (`NAME`, `CREATED_AT`), so
  callers write `Task::PRIORITY` instead of the string literal `"priority"`.

Each field opts in with a `#[seek(...)]` attribute; unannotated fields are
skipped entirely (not queryable, no constant, absent from `field_names()`).

| `#[seek(...)]` key | Effect |
| --- | --- |
| `String` / `string` | Field type is `SeekType::String` |
| `Number` / `number` | Field type is `SeekType::Number` |
| `Timestamp` / `timestamp` | Field type is `SeekType::Timestamp` (the field type must implement `SeekerTimestamp`; `i64` and `u64` do out of the box, read as milliseconds) |
| `Enum` / `enumeration` | Field type is `SeekType::Enum` (the field type must implement `SeekerEnum`) |
| `Bool` / `boolean` / `bool` | Field type is `SeekType::Bool` |
| `ty = "string"` (etc.) | Same five types, spelled as a string literal instead of a bare identifier |
| `skip` | Field is not queryable and gets no constant |
| `rename = "name"` | The query-facing field name differs from the Rust field name |

`String`/`Number`/etc. and `rename`/`ty` combine on the same attribute:
`#[seek(String, rename = "title")]`. A field with no `#[seek(...)]` attribute
at all is treated the same as `#[seek(skip)]`.

## Building a `Query` by hand

```rust
let query = Query::new()
    .and_gte(Task::PRIORITY, 4i32)
    .not_eq(Task::DONE, true)
    .order_desc(Task::PRIORITY)
    .limit(20)
    .build();

let results = query.filter(&tasks, Task::accessor);
```

`Query` holds three clause groups — `and`, `or`, `not` — combined with fixed
semantics: an item matches when *all* `and` clauses match, *and* (any `or`
clause matches, or there are none), *and* no `not` clause matches. Each group
builds with `.and(field, op, value)` / `.or(...)` / `.not(...)`, or a
per-operator shortcut (`.and_eq`, `.and_gt`, `.and_contains`,
`.and_before`, `.and_in`, and their `or_`/`not_` counterparts). `.order_by`,
`.order_asc`, `.order_desc`, `.limit`, and `.offset` configure the rest.
`.filter` returns matching references in order; `.filter_cloned`,
`.filter_mut`, `.count`, `.any`, `.all`, `.find`, and `.position` cover the
other common shapes.

## The query string grammar

`parse_query::<S>(pairs)` turns an ordered sequence of `(key, value)` string
pairs into a `Query`, validating every *clause* field and operator against `S:
SeekerSchema`. This is the shape a `--filter` flag or a raw query string
typically produces.

**Field clauses** — a key is either a bare field name (using the field's
default operator: `Eq` for everything except `Bool`, which defaults to `Is`)
or `field-operator`:

```text
name=docs                  # name eq "docs"
name-contains=docs         # name contains "docs"
priority-gte=4             # priority >= 4
created-at-before=2024-01-01
status-in=1,2              # enum field, comma-separated discriminants
                           # (variant names too, on a hand-written schema)
done                       # bare boolean flag => done eq true
```

Recognized operators (case-insensitive) and their aliases:
`eq`, `ne`/`neq`, `gt`, `gte`, `lt`, `lte`, `startswith`/`prefix`,
`endswith`/`suffix`, `contains`, `regex`/`re`/`match`, `before`, `after`,
`in`, `is`. A compound field name like `created-at` still parses correctly
when combined with an operator (`created-at-before`), because only the last
hyphen-separated segment is checked against the operator list.

**Group markers** — the bare keys `AND`, `OR`, and `NOT` (case-insensitive)
switch which clause group subsequent pairs join, starting from `AND`:

```text
name-contains=a&OR&name-contains=b&NOT&done=true
```

**Ordering, limit, offset** — reserved keys, not field clauses:

```text
order=priority-desc   # also: orderby, order-by, sort
limit=10
offset=5              # also: skip
```

`order`'s value is `field` (ascending) or `field-asc`/`field-desc`. The
ordering field is **not** checked against the schema: `parse_query` reads the
reserved `order`/`sort` key before it consults `S::field_type`, so
`order=does-not-exist` parses without error and then sorts every item on
`Value::None`, leaving the input order. Validate an ordering field yourself
against `S::field_names()` if a typo there should be an error.

**Value parsing per field type:**

- String: taken verbatim, unless the operator is `regex` (compiled as a
  regular expression).
- Number: tried as `i64`, then `u64`, then `f64`.
- Timestamp: a Unix timestamp in milliseconds, `YYYY-MM-DD`, an ISO
  datetime (`YYYY-MM-DDTHH:MM:SS[.fff]Z`), or a bare four-digit year.
- Enum: a numeric discriminant, or a variant name resolved through
  `SeekerSchema::resolve_enum_variant`; `in` accepts a comma-separated list
  of either. See [Enum fields](#enum-fields) — a derived schema takes
  discriminants only.
- Bool: `true`/`1`/`yes`/`on` or `false`/`0`/`no`/`off` (case-insensitive);
  an empty value defaults to `true`.

An unknown *clause* field, an operator invalid for the field's type
(`name-gt`, since strings don't support `Gt`), or an unparsable value each
produce a `ParseError` naming the field and, for unknown fields, the schema's
actual `field_names()`. The reserved `order`/`sort` key is the exception noted
above.

## Enum fields

`SeekerSchema::resolve_enum_variant(field, variant)` is what turns
`status=pending` into a discriminant, and it defaults to returning `None`.
`#[derive(Seekable)]` does not override it, so a **derived schema matches enum
fields on numeric discriminants only** — `status=pending` fails with a parse
error, `status=0` works. Because the derive owns the whole `SeekerSchema`
implementation, a second `impl` cannot add the method beside it.

To match on variant names, drop the derive for that struct and write both
traits by hand — `Seekable` for the values, `SeekerSchema` for the schema plus
the variant mapping. You give up the generated field constants along with it — `accessor` still
comes from the trait:

```rust
#[derive(Clone, Copy)]
enum Status {
    Pending = 0,
    Active = 1,
}

impl SeekerEnum for Status {
    fn seeker_discriminant(&self) -> u32 {
        *self as u32
    }
}

struct Task {
    name: String,
    status: Status,
}

impl Seekable for Task {
    fn seeker_field_value(&self, field: &str) -> Value<'_> {
        match field {
            "name" => Value::String(&self.name),
            "status" => Value::Enum(self.status.seeker_discriminant()),
            _ => Value::None,
        }
    }
}

impl SeekerSchema for Task {
    fn field_type(field: &str) -> Option<SeekType> {
        match field {
            "name" => Some(SeekType::String),
            "status" => Some(SeekType::Enum),
            _ => None,
        }
    }

    fn field_names() -> &'static [&'static str] {
        &["name", "status"]
    }

    fn resolve_enum_variant(field: &str, variant: &str) -> Option<u32> {
        match (field, variant) {
            ("status", "pending") => Some(Status::Pending as u32),
            ("status", "active") => Some(Status::Active as u32),
            _ => None,
        }
    }
}
```

`parse_query::<Task>` now accepts `status=pending` and `status-in=pending,active`
as well as the discriminants.

## Example

```rust
#[derive(Seekable)]
struct Task {
    #[seek(String)]
    name: String,
    #[seek(Number)]
    priority: i32,
    #[seek(Bool)]
    done: bool,
}

let pairs = vec![
    ("priority-gte".to_string(), "4".to_string()),
    ("NOT".to_string(), "".to_string()),
    ("done".to_string(), "true".to_string()),
];
let query = parse_query::<Task>(pairs)?;
let results = query.filter(&tasks, Task::accessor);
```
