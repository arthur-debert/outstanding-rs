- `TestHarness::rendering(Representation, ColorPolicy)` names a test's rendering in one call
  where a suite was writing `.output_mode(...)` and `.color(...)` at every site, and takes
  the pair as two arguments so a test can parameterize over an array of them. `output_mode`,
  `color` and `text_output()` are unchanged (closes #553).
- **Breaking:** the help-page oracles are gone: `standout_test::clap_parity` (`clap_facts`,
  `assert_states_clap_facts`, `assert_page_states_clap_facts`, `Fact`, `Subject`,
  `Omission`, `DELIBERATE_OMISSIONS`) and `standout_test::invariants` (its `assert_*`
  helpers and their `_in_page(s)` forms). `TestResult::tag_resolutions()` and
  `unresolved_tag_names()` remain the way to check style-tag accounting (closes #539).
- **Breaking:** a `test-support` feature on `standout` and `standout-render`, off by default
  and enabled by `standout-test`, gates the harness-only seams
  `standout_render::diagnostics::take_captured`,
  `standout::cli::warnings_delivered_on_stdout` and the
  `STANDOUT_QUESTIONNAIRE_TERMINAL` scripted terminal, which an adopter's debug build no
  longer reads. `standout-test` no longer exports `assert_page_snapshot!`, `SnapshotCase`,
  `matrix`, `MatrixCell` or the `pty` module (closes #540).
