- **Breaking:** `style_as` escapes the style-tag brackets in its value, so
  `{{ value | style_as('error') }}` on `missing [severity_map] table` renders them literally
  instead of opening a tag the caller never wrote. A value carrying style tags on purpose is
  literal too; write the outer style as tag syntax around the value to keep inner tags live.
  ANSI escape sequences in the value still pass through whole (for #551).
- **Breaking:** `style_as` rejects a style name the tag grammar (`[a-z_][a-z0-9_-]*`) does
  not accept, with a render error naming the filter and the name, rather than emitting
  markup the parser prints literally. An empty name still passes the value through unstyled
  (closes #568).
- A `verbatim` filter escapes the same brackets on demand. A command printing a generated
  file, a JSON Schema, a regex or a TOML snippet writes `{{ body | verbatim }}` and gets
  back what it handed the filter, with no warning and no failure under
  `STANDOUT_STRICT_STYLE_TAGS=1` (closes #551).
