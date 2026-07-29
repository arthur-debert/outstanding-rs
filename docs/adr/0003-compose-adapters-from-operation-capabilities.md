# Compose adapters from operation capabilities

The adapter seam is composed from separate capability interfaces for create, query, get, patch, delete, and declared action families rather than one monolithic adapter interface. Each adapter implements only the interfaces required by its Resource descriptor, keeping compile-time capability selection, generated commands, adapter obligations, and reusable contract tests aligned.
