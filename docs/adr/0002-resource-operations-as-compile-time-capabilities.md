# Make Resource operations compile-time capabilities

Each CRUD operation and named-action family is an opt-in compile-time capability of the Resource descriptor. Generated commands and adapter requirements exist only for declared capabilities, so read-only or API-specific Resources expose their real interface instead of a nominal full-CRUD surface that fails with unsupported-operation errors at runtime.
