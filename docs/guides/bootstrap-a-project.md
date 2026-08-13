# Bootstrap a Standout project

The `standout` package includes a `new-project` wizard that creates a small,
runnable workspace. Use it when you want the production-shaped Standout
ownership split without assembling the first command, template, theme, and
tests by hand.

The result is an architectural starter, not a complete application. A CLI-free
library owns reusable behavior, while a binary crate owns Clap, Standout
assembly, input-source policy, view types, templates, styles, and process
execution.

## Install and start the wizard

Install the package's `standout` executable from crates.io:

```sh
cargo install standout
```

Run the wizard from the directory that should contain the new project:

```sh
standout new-project
```

The project name is also the destination directory. The wizard refuses to
overwrite a non-empty destination.

The questionnaire asks for the project and executable names, one initial
command, its inputs, and a message or record result. It then prints the
destination, generated files, command syntax, source precedence, core
operation, output shape, and generated test seams. No files are published until
you type `yes` at the confirmation prompt.

An invalid answer does not abort the questionnaire. The wizard prints the
validation error and asks again, keeping every earlier valid answer. It retries
the smallest coherent unit: the current question for an invalid scalar answer,
the whole comma-separated list for an invalid record field, and the whole
current input block when a combination of answers is unsupported — for
example a `path` input with a `file` source, or an input whose generated flag
collides with an earlier input's.

Answer an exact uppercase `X` at any questionnaire prompt to cancel the wizard.
Cancellation is not an error: the wizard publishes no files and reports that
generation was cancelled.

## Work from an answer sheet

Long or repeated questionnaires are easier to complete in an editor than one
prompt at a time. The wizard can render its complete questionnaire as a prose
*answer sheet* and later generate the project from the completed file:

```sh
# Print the blank questionnaire; nothing is generated.
standout new-project questions

# Write the same deterministic sheet to a file.
standout new-project questions --file answers.txt

# Generate from the completed file.
standout new-project --answers answers.txt

# Generate from an answer sheet on stdin, with attended confirmation.
standout new-project --answers - < answers.txt

# Generate from stdin without prompting for confirmation.
standout new-project --answers - --yes < answers.txt

# Automate a named-file run the same way.
standout new-project --answers answers.txt --yes
```

Each question renders as one line — a cosmetic number, its wording, a type
hint, and a stable ID tag such as `<id:project.name>` at the end of the line.
Write the answer on the line (or lines) below the question; a `text` answer
such as the command description may span several lines, and everything up to
the next question line belongs to it. Declared defaults are pre-filled as the
answer text — leave them untouched to accept them, and leave the executable
name blank to reuse the project name. The repeatable input section renders
one block; add another input by copying the complete block — its heading line
and its questions — below the last block and answering the copy. Only the
line-ending `<id:...>` tags carry meaning: rewording, renumbering, or
re-indenting a sheet does not change what it means, and a tag only counts
when it ends its line, so mentioning one mid-prose is harmless (the wizard
prints a warning when an answer contains `<id:`, in case a tag was mangled).

`--answers` replaces question collection entirely — it never merges file
answers with prompts — but everything after collection is the interactive
experience: the same validation, the same review, and the same `yes`
confirmation before anything is published. `--answers -` reads exactly one
complete sheet from piped standard input instead of a file; both sources
produce identical results for identical documents. A sheet that fails to
parse or validate reports every independent problem in one pass, each
identified by its stable ID (for repeated inputs, an indexed path such as
`command.inputs[1].sources`), and publishes nothing; the same no-partial-write
guarantee as the interactive wizard applies to every failure and rejection.

Submitting a sheet is not consent to generate. Piping a file — or reaching
its end — never confirms anything: without `--yes`, the wizard shows the
review and asks for confirmation on your terminal, independent of the answer
stream, and only an exact `yes` reply publishes the project. If confirmation
is required but no attended terminal is available (a CI job, a redirected
shell), the run fails before publishing anything and says so. Automation
opts out of the prompt explicitly with `--yes`, which skips only the
confirmation gate — parsing, validation, the review output, and atomic
publication all still run.

The sheet's `#!` preamble pins the answer format, the questionnaire ID, and a
fingerprint of the questionnaire's semantics. A sheet rendered by an older
`standout` whose questionnaire has since changed is rejected with a
compatibility error rather than reinterpreted; render a fresh sheet with
`standout new-project questions` and copy your answers into it. Answer sheets
are plain text and hold whatever you answered — including any sensitive
values — so keep them out of version control, shared locations, and shell
history (piping with `< answers.txt` beats inlining a heredoc), and delete
them when done.

## Supported inputs

The first release deliberately supports a small, explicit matrix:

| Value type | Cardinality | Sources |
| --- | --- | --- |
| `string` | `required` or `optional` | Any ordered combination of `argument`, `file`, and `stdin` |
| `string` | `repeated` | `argument` only |
| `bool` | `boolean` | `argument` only |
| `path` | `required`, `optional`, or `repeated` | `argument` only |

For a string with multiple sources, the order entered is the precedence order.
For example, `argument,file,stdin` tries `--document`, then
`--document-file PATH`, then piped standard input. A file source means the
file's contents become the string value. Path inputs instead pass a
`PathBuf`; they do not read the file.

Boolean inputs are generated as `--name` flags. Repeated string and path
inputs repeat the same named option:

```sh
myapp process --tag first --tag second
```

## What the wizard generates

For a project named `myapp`, the workspace has this shape:

```text
myapp/
├── Cargo.toml
└── crates/
    ├── myapplib/
    │   ├── Cargo.toml
    │   └── src/lib.rs
    └── myapp/
        ├── Cargo.toml
        ├── README.md
        └── src/
            ├── main.rs
            ├── cli.rs
            ├── handlers.rs
            ├── templates/<command>.jinja
            └── styles/myapp.css
```

The library includes a typed operation, result, validation error, and unit
tests without Clap or Standout dependencies. The binary includes the command
declaration, a thin typed handler, a serializable view, human-output assets,
and tests for handler mapping and the full argv-to-output pipeline through
`TestHarness`. The generated manifest uses the installed wizard's Standout
version as a normal compatible Cargo requirement.

The generated binary supports the chosen command in human and structured
output modes:

```sh
cd myapp
cargo run -p myapp -- process --document "hello"
cargo run -p myapp -- process --document "hello" --output json
```

Its generated README records the exact syntax and input-source policy selected
in the wizard.

## Verify and continue

The generated project is ready for the standard Rust checks:

```sh
cargo fmt --check
cargo check --workspace
cargo test --workspace
```

Keep application behavior in the CLI-free library as the project grows. Keep
shell inputs, environment lookup, Standout wiring, view models, and
presentation assets in the binary crate. The
[production-shaped application](production-shaped-example.md) explains that
ownership boundary in depth.
