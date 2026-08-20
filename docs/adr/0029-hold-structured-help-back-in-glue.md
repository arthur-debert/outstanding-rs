# Hold structured help back in glue

When the command is help or topics and the requested format is structured (`json`, `yaml`, `csv`, `xml`), glue builds a `RenderRequest` whose format is `Auto` instead. Color then follows stdout facts on `TargetProperties` the way a normal human help page does. The leaf has no help special case and never emits a help document in a structured mode.

A flag on `RenderRequest` ("not a public envelope") was rejected: it teaches the leaf what help is and puts a help secret on every render. Emitting structured help as an undocumented format was rejected: the machine-contract Spec forbids publishing that envelope until it is versioned, and shipping it here is what that Spec would then have to break.

`myapp help --output=json` on a TTY therefore still looks like help (mapping to `Auto`, not `Text`). Mapping to `Text` would strip color on a terminal for no reason.
