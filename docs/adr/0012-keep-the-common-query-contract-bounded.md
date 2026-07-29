# Keep the common query contract bounded

The common typed query tree supports flat declared fields, typed predicates, normalized AND/OR/NOT groups, ordering, limit, and offset, and returns a page with items plus an optional total. Cursor protocols and SQL-like features remain outside the common interface, and a bound adapter must honor every declared operator rather than silently approximating unsupported semantics.
