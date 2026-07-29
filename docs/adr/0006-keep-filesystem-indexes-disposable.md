# Keep filesystem indexes disposable

A filesystem adapter stores each UUID-addressed object in its own authoritative file, publishes updates by atomic replacement, and represents deletion with tombstones before cleanup. Any property index is a disposable derived projection: reconciliation adds object files missing from the index and removes entries with no live object, while large content fields may stay out of the index and be hydrated or scanned after indexed facets narrow query candidates. The index never becomes a second source of truth.
