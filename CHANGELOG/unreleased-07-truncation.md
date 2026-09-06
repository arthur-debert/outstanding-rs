- **Breaking:** truncation closes a style it cuts through — `{{ value | col(10) }}`,
  `{{ value | truncate_at(10) }}`, a table column narrower than its cell,
  `truncate_to_width`, and `standout-render`'s `truncate_end` and `truncate_middle`. The
  colour no longer runs on into everything printed afterwards, and a snapshot pinning the
  truncated bytes of a pre-styled value gains a `\u{1b}[0m` before the ellipsis. Wrapping
  still cuts to continue, so a wrapped value keeps its colour across the line break
  (closes #566, #568).
- The `config` command escapes its output with the same ANSI-aware function the filters use,
  rather than a private copy that escaped the `[` inside an escape sequence. What `config`
  prints today is unchanged. `escape_style_tags` is exported from `standout-render`
  (closes #565).
