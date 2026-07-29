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
