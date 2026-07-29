# Make concurrency tokens explicit and non-retrying

Resources opting into optimistic concurrency expose their version token in relevant read results and require an explicit expected token on protected patch, delete, and action operations; generated commands may surface it as `--if-version`. The framework reports the shared typed conflict outcome but does not silently fetch, retry, or replay mutations because retry and idempotency policy remain application-owned.
