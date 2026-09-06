- **Breaking:** the text standout composes around application- and argv-supplied strings — a
  `RunError`'s stderr prose and `Diagnostic`, `ctx.warn` text, and the captured clap usage
  message — escapes control characters to their Rust codepoint spelling (an `ESC` reads
  `\u{1b}`): everything `char::is_control()` matches except `\n` and `\t`, so an untrusted
  path cannot paint the terminal through them. Put styled failure output in `AppFailure`
  instead; a stderr or `TestResult::warnings()` snapshot that pinned a control character
  changes (closes #552).
- `AppFailure` and `ExternalFailure` still write their bytes to stderr verbatim, and
  handler-rendered template output is untouched: an application printing untrusted text
  through its own templates still escapes it itself.
