# Apply styles from the request, not process globals

The leaf applies ANSI from the `RenderRequest`, not from `console::colors_enabled()`. `OutputMode::Term`, and `Auto` when `TargetProperties` says the destination is color-capable, call `force_styling(true)` — the path warnings already use so piped stdout does not strip stderr. `Text`, and `Auto` with no color, do not. `TestHarness::with_color()` only fills `TargetProperties`; it does not call `set_colors_enabled`. The width lock in `standout-render` is deleted: width is on `TargetProperties`, so an atomic around it has nothing to protect.

Leaving the `console` global in the apply path was rejected: `TargetProperties.color_capability` would be a suggestion the global could ignore (the same shape as the ambiguous-width bug), in-process color tests would still mutate process state and still need `#[serial]`, and render would not be a function of the request. Setting that global at the `run()` edge was rejected as the same ambient API with a different caller.

Warnings collected during a run are returned on the run result / harness API (the Spec). The warning thread-local goes away with the detectors: a thread-local is another process-shaped slot for a value the result can carry.
