# Implementation quality checklist

The canonical public checklist is
[`docs/guides/leveraging-standout.md`](../../../docs/guides/leveraging-standout.md).
Use it for implementation reviews; do not duplicate its framework guidance
here.

Classify findings before recommending work:

- **Invariant:** required ownership or behavior boundary.
- **Applicable capability:** useful only when the application needs it.
- **Framework gap:** desired behavior that normal Standout integration does not
  provide.

Review in this order:

1. Confirm the reusable core is CLI-free and the CLI owns Clap, Standout,
   adapters, view DTOs, resources, app assembly, and final output.
2. Confirm handlers return data without printing, rendering, or output-mode
   branching.
3. Confirm structured modes preserve the CLI data contract and bypass
   templates.
4. Check rendering, diagnostics, and tabular behavior only where applicable.
5. Look for core, typed-handler, harness, and narrowly scoped process tests.
6. Classify hooks, inputs, pipes, and partial adoption as optional capabilities;
   name unsupported integration as a gap rather than forcing it into a handler.

Verify version-sensitive claims against the checked-out public types and tests.
