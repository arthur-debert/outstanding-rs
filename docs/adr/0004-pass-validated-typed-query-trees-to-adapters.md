# Pass validated typed query trees to adapters

Query adapters receive a validated, transport-neutral typed query tree derived from the Resource descriptor. Raw user syntax and Seeker's in-memory accessor callbacks do not cross the adapter seam: Seeker parses and validates into the tree, in-memory adapters execute it, and remote adapters translate it into application-owned transport conventions.
