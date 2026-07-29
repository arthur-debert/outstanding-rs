# Declarative Resource

## Context

Standout can already derive and register commands, route nested command paths, render typed handler output, and exercise applications through `TestHarness`. Its `standout-seeker` crate provides a CLI-free query model, schema metadata, string parsing, and in-memory filtering. The production-shaped todo example also demonstrates the intended ownership rule: reusable domain behavior is CLI-free, while Clap, Standout wiring, environment resolution, and view models remain application concerns.

Today those capabilities do not form a resource abstraction. Applications separately declare domain models, command shapes, handlers, filtering, lifecycle behavior, and persistence. Seeker's executable query methods are shaped around in-memory slices, and the todo example combines lifecycle behavior with a single JSON store rather than proving a replaceable storage seam.

This Spec defines a declarative Resource feature that unifies those repeated declarations without moving CLI concerns into domain code. There is no existing `CONTEXT.md` or ADR set for this area; current constraints come from the design guidelines, the Seeker proposal and implementation, the command/dispatch implementation, and the production-shaped todo example.

## Problem

Resource-oriented CLIs repeatedly implement the same work:

- declare create, patch, query, get, and delete commands;
- translate command inputs into typed domain operations;
- validate inputs and enforce lifecycle rules;
- keep filtering semantics aligned between local and remote storage;
- distinguish an omitted patch field from an explicit request to clear it;
- map named domain actions that are not ordinary CRUD updates;
- register nested command families such as `gh pr ...` and `gh issue ...`; and
- duplicate model facts across domain types, query metadata, command declarations, and handlers.

That duplication is both shallow and drift-prone. It also encourages lifecycle rules to leak into CLI handlers or storage adapters, and encourages each application to invent a second query concept instead of reusing Seeker.

## Goals

- Add a CLI-free `standout-resource` module for declaring and operating on typed Resources.
- Support typed create, patch, query, get, and delete operations, plus typed named actions.
- Make CRUD operations and named-action families opt-in compile-time capabilities, generating commands only for capabilities the Resource descriptor declares.
- Keep application-owned lifecycle behavior above storage adapters so every adapter observes the same domain rules, without introducing a Resource-specific validation protocol.
- Let one common model declaration drive both generated CRUD command descriptions and the filtering interface shipped to application and adapter authors.
- Replace Seeker's independent schema ownership with the Resource model as the single declaration of queryable fields, types, and supported operators.
- Reshape Seeker into query parsing and execution machinery that consumes the Resource model, preserving user-facing filter syntax where it remains a good fit.
- Support both local and remote adapters without imposing transport concepts on the Resource interface.
- Give each Resource an associated identity type and each operation capability an independently overridable output type.
- Express patch omission and explicit clearing as distinct typed states.
- Permit remote adapters to report optimistic-concurrency conflicts, including ETag-backed conflicts where an application uses them.
- Make optimistic concurrency an opt-in compile-time capability with a Resource-defined version-token type and shared typed conflict outcome.
- Generate command trees that fit Standout's existing nested command model, including sibling resource families such as `gh pr ...` and `gh issue ...`.
- Prove the seam with two adapters: a tiny in-memory adapter and an application-owned fake REST adapter.
- Make invalid declarations and unsupported operation combinations fail explicitly during compilation or application setup rather than falling back silently.

## Non-Goals

- A universal HTTP client, HTTP request model, or framework-owned REST abstraction.
- A database, ORM, query planner, transaction framework, or general persistence framework.
- Automatic support for every remote API's conventions.
- Moving Clap types, terminal I/O, rendering, environment lookup, or application-specific authentication into `standout-resource`.
- Treating named actions as disguised generic patches.
- Preserving backwards compatibility for the current filtering implementation when a cleaner common query interface requires a breaking change.
- Replacing application-owned domain models or response/view models.

## Proposed Shape

Introduce a sibling `standout-resource` module with a small typed interface through which callers exercise Resource behavior. Its architectural seam is a dedicated, application-owned Resource descriptor with associated types for create, patch, stored/read, query, and action shapes. For the ordinary case, a derive generates the descriptor and routine create, patch, and query types from the application model. Applications can override those associated types individually when a domain or remote Resource needs a different shape; opting out of one generated type does not require hand-writing the whole descriptor. The descriptor supplies focused generated views of the same metadata to CRUD commands and filtering machinery, reducing parallel declarations without turning one domain type or derive into a catch-all interface.

The descriptor declares each supported CRUD operation and named-action family as an opt-in compile-time capability. Command generation follows those capabilities exactly: unsupported operations are absent from the generated command tree and adapter requirements rather than present as runtime paths that return an unsupported-operation error. Read-only, create-only, and API-specific Resources therefore expose only their real interface.

Each named action is an independent typed descriptor with its own input type, output type, command metadata, and adapter capability. Actions are not variants of a Resource-wide enum selected by runtime strings. The Standout integration renders each declared action as an ordinary generated subcommand under the Resource's mount path, while adapters translate that action according to its distinct contract.

The adapter seam is composed from separate synchronous operation capability interfaces for create, query, get, patch, delete, and declared action families. An adapter implements only the capability interfaces required by its Resource descriptor; there is no monolithic full-CRUD adapter interface. Resource behavior sits above this composed seam. Applications compose ordinary domain validation and lifecycle functions around create, patch, delete, and named actions before those operations reach storage. The Resource module does not prescribe separate input and lifecycle validation phases: existing Standout input chains and hooks remain available for pre-dispatch validation, while state-dependent rules remain ordinary application/domain code. Query and get use the same typed Resource vocabulary. Adapters implement storage- or remote-specific translation while applications retain ownership of transport clients, credentials, endpoint conventions, and response mapping.

Each Resource declares an associated identity type. Adapters ordinarily assign identity during create and return the canonical read shape, while an overridden create type may carry a client-assigned identity. Every capability owns an overridable output type; generated defaults are the canonical read shape for create, get, and patch, a typed page of read shapes for query, `()` for delete, and the action descriptor's output for each named action.

The common operation error interface standardizes only cross-adapter semantic outcomes: not-found and, when declared, optimistic conflict. Every adapter retains an associated error type for persistence, transport, and remote rejection details. Application/domain validation remains above the seam. The Resource module does not impose `Serialize`, `Deserialize`, `Send`, or `Sync` on all domain types; concrete adapters and the Standout integration add only the bounds they require.

Optimistic concurrency is a separate opt-in compile-time capability. Resources that declare it add their own version-token type to relevant read, patch, delete, and action interfaces and receive a shared typed conflict outcome; adapters may map that token to ETags, revisions, hashes, or another application-owned mechanism. Relevant read results expose the version token, and generated mutation commands accept an explicit expected version such as `--if-version`. The framework does not silently fetch, retry, or replay a mutation. Synchronization is orthogonal: the tiny in-memory adapter relies on one process-local synchronized operation and does not declare optimistic concurrency. A future filesystem adapter would own cross-process locking and crash-safe replacement as well as any version-token comparison; the Resource module does not mistake Rust memory safety for filesystem coordination.

A future filesystem adapter uses a data directory containing one authoritative object file per UUID. Object updates publish complete replacement files atomically, and deletion publishes tombstones before later cleanup. Any property index is strictly a disposable derived projection: an object file missing from the index is added during reconciliation, while an index entry with no live object file is removed. The filesystem adapter's configuration selects which queryable fields it projects; indexed-field selection is not part of the common Resource descriptor because it is an adapter-specific performance policy. Large content fields may remain only in object files; queries use indexed facets to narrow candidates and hydrate or scan authoritative files when evaluating non-indexed or full-text predicates. The index never becomes a second source of truth or a reason to impose database-style size accounting on object content.

Generated patch types distinguish omission from clearing without nested options. Non-clearable fields use ordinary `Option<T>` to represent unchanged versus set and cannot express clear. Clearable fields use an explicit tri-state value representing unchanged, set, or clear. Applications may still replace the generated patch associated type when a Resource requires a different domain-specific patch model.

The Resource model becomes the authoritative filtering declaration. Seeker is modified to parse user-facing filter syntax against that model and produce a validated, transport-neutral typed query tree. That tree is the only query representation crossing the adapter seam: raw string pairs and Seeker's current accessor callbacks remain implementation details outside it. The first common query contract remains deliberately bounded to flat declared fields, typed predicates, Seeker's normalized AND/OR/NOT semantics, ordering, limit, and offset. Query returns a typed page containing items and an optional total count; cursor protocols are not part of the common interface. In-memory adapters execute the tree, while remote adapters translate it to their application-owned transport conventions. An adapter must honor every operator exposed by its bound Resource and may not silently ignore or approximate predicates; a cursor-only or otherwise incompatible application overrides the query capability rather than weakening the common contract.

The Resource derive emits CLI-free command metadata, not Clap types or executable dispatch. A separate Standout integration converts that metadata into the framework's existing command-description and nested-dispatch machinery at an application-chosen mount path; no second dispatcher is introduced. It provides overridable conventional leaf names—`create`, `list`, `get`, `patch`, and `delete`, plus each declared action name—and relies on existing Standout setup verification to reject collisions and invalid trees before dispatch. Applications can therefore mount several Resources as trees such as `gh pr ...` and `gh issue ...`, while continuing to own view models, templates, output policy, and any non-Resource commands.

The initial proof includes a minimal synchronized in-memory adapter and a fake REST adapter owned by a sample application. The fake records typed-operation translation into representative POST, PATCH, DELETE, query, action, and concurrency behavior; it is evidence for the seam, not a framework transport layer. Capability interfaces remain synchronous to match Standout's current execution model; applications needing network access use an application-owned blocking client. Async execution requires a separate Standout-wide design rather than an executor hidden inside this feature.

## User / Agent Stories

1. As a library author, I want to declare a Resource's operations and queryable fields once, so that command and filtering behavior cannot drift independently.
2. As a domain-library author, I want Resource behavior to compile without Clap or terminal dependencies, so that the same model and lifecycle rules work outside a CLI.
3. As an application author, I want generated typed CRUD commands, so that I only write application-specific wiring and presentation where it adds value.
4. As an application author, I want to mount Resources under nested command groups, so that tools with shapes like `gh pr` and `gh issue` remain natural.
5. As an adapter author, I want a typed query value rather than in-memory callbacks, so that I can translate the same query to local evaluation or a remote API.
6. As an adapter author, I want omitted and explicitly cleared patch fields to be distinct, so that remote partial updates do not erase data accidentally.
7. As an adapter author, I want typed operation and conflict results, so that not-found, validation, and optional optimistic-concurrency failures are not collapsed into strings.
8. As a domain author, I want named actions with lifecycle validation, so that transitions such as close, archive, or merge are not misrepresented as arbitrary patches.
9. As a test author, I want one adapter contract suite, so that every adapter proves the same Resource semantics.
10. As a maintainer, I want generated declarations verified during build or tests, so that invalid command trees and unsupported operation combinations fail before users invoke them.
11. As a CLI user, I want familiar filter syntax to remain where it fits, so that the internal query redesign does not create needless command-line churn.
12. As an operator, I want structured-output and error behavior to pass through normal Standout execution, so that Resource commands compose with existing output modes and diagnostics.

## Risks And Rabbit Holes

- Making the derive itself the architectural seam would trap advanced Resources inside generated shapes, while requiring every application to hand-write the descriptor and all operation types would make routine Resources needlessly expensive. The derive therefore supplies defaults behind the descriptor seam, and each associated type remains independently replaceable.
- Treating the stored/read type as the Resource declaration would conflate server-generated state, create inputs, patch tri-state fields, query exposure, and command metadata. The descriptor composes those distinct shapes even when the ordinary derive generates them.
- Keeping Seeker's current slice/accessor execution shape as the Resource query interface would make remote translation awkward or force duplicate query concepts.
- Passing raw user syntax through the adapter seam would make every adapter repeat parsing and validation, while passing accessor callbacks would preserve an in-memory execution assumption. Only the validated typed query tree crosses the seam.
- Preserving an independently declared Seeker schema beside the Resource model would create two authorities for the same fields and operators. This is explicitly rejected, even if migration requires a breaking change.
- A generic HTTP abstraction would pull authentication, endpoint layout, retries, pagination, rate limits, and API-specific actions into Standout without a stable common model.
- Adding async capability interfaces solely for the fake REST proof would introduce executor and cancellation policy that current Standout dispatch does not own.
- Over-generating domain behavior or introducing a generic validation lifecycle could hide important rules. Generation should remove declaration drift, not replace explicit application/domain decisions.
- Conflating create, stored, read, and patch shapes could make server-generated fields writable or lose the omitted-versus-clear distinction.
- Using one tri-state type for every patch field would permit invalid clears on required fields, while `Option<Option<T>>` obscures intent. Generated patch types use the smallest representation that expresses each field's valid transitions.
- Treating every named action as CRUD would weaken validation and make remote translation less faithful.
- A Resource-wide action enum or string dispatcher would couple unrelated contracts and make adapters handle actions they do not support. Independent typed action descriptors keep capability selection, command generation, testing, and remote translation local to each action.
- Generating command trees independently of existing dispatch verification would create a second command system.
- Emitting Clap types from the Resource derive would leak CLI ownership into the Resource module, while executable generated dispatch would compete with Standout's existing command verification and routing. Only CLI-free metadata crosses into the separate integration.
- A single full-CRUD adapter interface with runtime unsupported errors would make descriptors and generated help overstate a Resource's capabilities. Capability selection must remain visible to the type system and command generator.
- A monolithic adapter interface would force every adapter to implement irrelevant methods or runtime unsupported branches. Operation capability interfaces keep the adapter seam aligned with the descriptor while allowing the shared contract suite to compose the same behavioral capabilities.
- A contract suite that asserts adapter internals rather than observable Resource semantics would make legitimate adapters impossible to implement.
- The existing Seeker fixed boolean semantics and flat fields may not fit every remote API. The first version must state supported query behavior without growing into SQL.
- Cursor pagination, automatic retries, cross-object transactions, and batch semantics would each add a new protocol to the common interface and remain outside this feature.

## Cross-Cutting Concerns

- **Security and privacy:** Resource declarations must not require secrets. Authentication and credential storage remain application/adapter concerns. Generated diagnostics must not expose request bodies or concurrency tokens by default.
- **Errors and observability:** Common not-found and optional conflict outcomes remain distinguishable from the associated adapter error through normal Standout error reporting. Application/domain validation remains above the adapter seam, and transport-specific details remain adapter-owned.
- **CI and release:** Adding a workspace module and generated code affects crate publication, feature wiring, documentation, and compatibility guidance. The Shipit install pin is currently behind main and will be reconciled separately under ADR-0033; that maintenance must not be folded into this feature.
- **Migration and compatibility:** Backwards-breaking filtering changes are allowed. Existing Seeker declarations migrate to the Resource model; the implementation must not retain a second schema system as a compatibility layer. Preserve user-facing syntax where it remains coherent and document intentional changes.
- **Performance:** The in-memory adapter targets ordinary CLI-sized collections. Remote adapters must be able to translate ordering and pagination instead of fetching everything solely to reuse local execution.
- **Configuration safety:** Unsupported operations, invalid field/operator combinations, duplicate command paths, and inconsistent declarations must fail explicitly at compile time or app setup.
- **Retries and idempotency:** The Resource module does not automatically retry mutations or named actions. Applications own retry and idempotency policy because only they know whether an operation is safe to replay.
- **Concurrency and durability:** Process-local synchronization, cross-process locking, crash-safe persistence, and optimistic conflict detection are distinct guarantees. Adapters declare and test only the guarantees they actually provide; filesystem locking and durability are adapter concerns, not implicit Resource behavior.
- **Filesystem recovery:** Object files are authoritative and the index is disposable. Reconciliation adds orphaned object files to the index, removes index entries whose live object is absent, and respects tombstones so deletion cannot be mistaken for an indexing lag.

## Testing / Verification

The feature is complete only with all three approved layers of proof:

1. **Shared adapter contract suite.** Run one reusable behavioral suite unchanged against the tiny in-memory adapter and the application-owned fake REST adapter. Cover typed create, query, get, patch, delete, and at least one named action; validation before adapter effects; not-found behavior; ordering/pagination supported by the declared query contract; patch omission versus explicit clear; and optional conflict behavior when a concurrency token is supplied.
2. **Adapter translation tests.** Test the fake REST adapter directly with a recording application fake. Assert representative operation-to-method/path/body/query/header mappings; POST/PATCH/DELETE or API-specific equivalents; omission versus explicit null/clear; not-found decoding; named-action translation; and optional 409/412 plus ETag conflict mapping. These are deterministic translation tests, not network tests.
3. **CLI and `TestHarness` proof.** Build a real application command tree with nested Resource families such as `gh pr` and `gh issue`, then run representative argv through `TestHarness`. Prove generated create/query/get/patch/delete/action paths, preserved filter syntax where selected, structured output, and a validation failure that causes no adapter effect.

Existing prior art to reuse includes Seeker's coverage and property tests, command verification tests for dotted nested paths, `TestHarness` integration tests, and the todo example's real add-then-list state transition through the application seam.

## Workstream Hints

- Stabilize the common Resource declaration and typed operation/query vocabulary before command generation.
- Modify Seeker to consume Resource model metadata and provide parsing and in-memory execution behind the common query interface.
- Add adapter contracts, application lifecycle composition points, and the tiny in-memory adapter.
- Integrate generated command descriptions with existing Standout dispatch and nesting.
- Build the application-owned fake REST proof, translation tests, and end-to-end `TestHarness` example.
- Finish with compatibility, migration, and publication documentation.

These are planning hints only; issue decomposition happens after the Spec and ADRs are merged.

## Out Of Scope

- Creating an epic or workstream issues in this planning step.
- Implementing the Resource module or changing executable code.
- Selecting a universal serialization format or network stack.
- Adding an async execution model, runtime, or cancellation protocol.
- Providing production adapters for GitHub, databases, or arbitrary web services.
- Supporting joins, nested relational filters, aggregation, or an unrestricted query language.
- Supporting cursor pagination, cross-object transactions, batch operations, automatic retries, or idempotency infrastructure.
- Automatically deriving application presentation, templates, authentication, or retry policy.
- Reconciling the repository's Shipit pin as part of this feature.

## Further Notes

The architectural grill follows this Spec and records only hard-to-reverse decisions with meaningful alternatives as ADRs.

- [ADR-0001: Use a Resource descriptor with derivable defaults](../adr/0001-resource-descriptor-with-derivable-defaults.md)
- [ADR-0002: Make Resource operations compile-time capabilities](../adr/0002-resource-operations-as-compile-time-capabilities.md)
- [ADR-0003: Compose adapters from operation capabilities](../adr/0003-compose-adapters-from-operation-capabilities.md)
- [ADR-0004: Pass validated typed query trees to adapters](../adr/0004-pass-validated-typed-query-trees-to-adapters.md)
- [ADR-0005: Make optimistic concurrency an optional capability](../adr/0005-make-optimistic-concurrency-an-optional-capability.md)
- [ADR-0006: Keep filesystem indexes disposable](../adr/0006-keep-filesystem-indexes-disposable.md)
- [ADR-0007: Keep index selection adapter-owned](../adr/0007-keep-index-selection-adapter-owned.md)
- [ADR-0008: Generate CLI-free command metadata](../adr/0008-generate-cli-free-command-metadata.md)
- [ADR-0009: Use tri-state patches only for clearable fields](../adr/0009-use-tri-state-patches-only-for-clearable-fields.md)
- [ADR-0010: Model named actions as typed capabilities](../adr/0010-model-named-actions-as-typed-capabilities.md)
- [ADR-0011: Keep adapter interfaces synchronous and minimally bounded](../adr/0011-keep-adapter-interfaces-synchronous-and-minimally-bounded.md)
- [ADR-0012: Keep the common query contract bounded](../adr/0012-keep-the-common-query-contract-bounded.md)
- [ADR-0013: Make concurrency tokens explicit and non-retrying](../adr/0013-make-concurrency-tokens-explicit-and-non-retrying.md)

The architectural grill is complete. These ADRs and this Spec define the approved feature shape; implementation decomposition belongs to the later issue-planning leg.
