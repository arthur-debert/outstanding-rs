# Bless one item per axis behind a capability map

An **axis** is the set of items that answer one question about wiring an application: how a name binds to a handler (registration), how a function becomes the dispatch signature (adaptation), where the clap `Command` comes from (declaration), how a template reaches a command (template provision), how a theme reaches the app (theme provision), and how a process or a test enters the app (entry points). A working idiom takes one item from each axis, so the axes compose rather than substitute — which is why the count of items is not a count of alternatives, and why a deletion is only safe against the *same* axis.

On each axis exactly one item is **blessed**: the item the README, the wizard's generated project, `tdoo`, the guides and every new topic show, and the only one an adopter has to learn. Every other item on that axis either names a capability no surviving item on the same axis covers — and survives as a secondary path with that reason attached — or is deleted with its tests. The capability map below is what makes the second half checkable: a deletion that strands a capability reopens this ADR rather than being worked around in application code.

The blessed set, whole:

```rust
#[derive(Parser)]              // declaration: clap-derive
struct Cli { … }

#[derive(Dispatch)]            // registration
enum Commands { … }

#[handler]                     // adaptation
fn list(#[flag] all: bool, #[ctx] ctx: &CommandContext) -> Result<Listing> { … }

App::builder()
    .templates(embed_templates!("src/templates"))   // template provision
    .styles(embed_styles!("src/styles"))            // theme provision
    .default_theme("myapp")
    .commands(Commands::dispatch_config())?
    .build()?
    .run(Cli::command(), std::env::args());         // entry point
```

The pilot scorecard's ranked friction confirms the expected default rather than displacing it. Theme 1 (4/4 runs) and theme 3 (4/4) are both collisions *inside* the derive path — snake_case against kebab-case (#350), `#[handler]` against `#[dispatch(questionnaire = …)]` (#355), generated items against `non_snake_case` (#358), the questionnaire derives against a single `standout` dependency (#360), the parameter-name mapping against the documented one (#349). Not one is evidence that the derive path is the wrong default; each is evidence that it is the *unfinished* default, which is why the repairs ride with the blessing (Spec, Proposed Shape 2) instead of being triaged as ordinary bugs. Theme 2 is the adopter-seams epic's and does not bear on this choice. The item the pilot never reached for by preference is the one this ADR deletes most of: builder registration appears in the runs as a workaround — one of `systemdlike`'s six — not as the path an adopter chose from the documentation.

## Registration — bind a name to a handler

| Item | Verdict |
| --- | --- |
| `.commands(Commands::dispatch_config())` from `#[derive(Dispatch)]` | **Blessed.** One enum declares the command set; the derive writes the registration. |
| `.commands(\|g\| …)` with a hand-written closure | Keep — dynamic registration: a command set computed at run time (plugins, a config file) has no enum to derive from. Same method, unblessed form. |
| `AppBuilder::command_with(path, handler, configure)` | Keep — the per-command escape hatch: one command bolted onto a derive-registered app, or a handler the derive cannot spell. Its signature becomes today's `command_handler_with` (see below). |
| `AppBuilder::command_passthrough(path, handler)` | Keep — passthrough: a handler that owns its own bytes and returns `Result<(), anyhow::Error>`, with no serializable output and no render. Nothing else on this axis accepts that signature. |
| `AppBuilder::group(name, configure)` | **Delete** — `commands(\|g\| g.group(name, configure))` reaches the identical `register_group` call. |
| `AppBuilder::command(path, handler, template)` | **Delete** — `command_with` plus one `.template(…)` call, and its third argument is inline template source, which the template axis deletes. |
| `AppBuilder::command_handler(path, handler, template)` | **Delete** — same shorthand against `command_handler_with`. |
| `AppBuilder::command_handler_with(path, handler, configure)` | **Delete as a name.** Its signature is the survivor: the one builder registration takes `impl Handler`, which is the general case — a stateful struct implements `Handler`; a closure or a `#[handler]` fn reaches it through the public `FnHandler` adapter. |

Collapsing the closure and struct forms into one method costs one `FnHandler::new(…)` at the escape hatch and is the honest split: *which kind of callable this is* belongs to the adaptation axis, and `FnHandler` is that axis's non-macro adapter. The blessed path writes neither.

`GroupBuilder` keeps the entries the derive emits — `command`, `command_with`, `group`, `default_command` — plus `passthrough`; its remaining methods are pruned on this same rule by the pruning workstream.

## Adaptation and declaration — the fn, and the clap `Command`

| Item | Verdict |
| --- | --- |
| `#[handler]` with typed parameter attributes (`#[flag]`, `#[arg]`, `#[ctx]`, `#[matches]`) | **Blessed** for adaptation. |
| `FnHandler` | Keep — the non-macro adapter, and the only way a plain closure reaches the registration axis. |
| clap-derive on the CLI type | **Blessed** for declaration. It is clap's API, not Standout's, so it is not this repo's to prune. |
| A hand-built `clap::Command` | Keep — same reason: clap's own builder, reached by any app that assembles its `Command` at run time. |
| `#[command]` | **Delete.** It is `#[handler]` plus a `Command` built from Standout's own copy of clap's vocabulary (`about`, `long_about`, `visible_alias`, `hide`), so it duplicates the blessed declaration item; it still needs registration, so it saves nothing on the registration axis; it appears in one test file, no guide and no reference client; and its name collides with clap's `#[command]` attribute, which every Standout application already imports. Adaptation is covered by `#[handler]` and declaration by clap-derive, so nothing is stranded — the four generated items (`name__handler`, `name__expected_args`, `name__command`, `name__template`) go with it. |

## Template provision — how a command finds its template

Two questions live on this axis and both are answered here: how templates enter the registry, and how a command names the one it wants.

| Item | Verdict |
| --- | --- |
| `.templates(embed_templates!("src/templates"))` | **Blessed** for provision. Compile-time embedding, with the absolute source paths that make debug hot reload work (ADR-0019). |
| convention — no call; the command path is the template name | **Blessed** for selection, redefined: the convention *is* the default `template_name`, resolved by the same registry rule. |
| `CommandConfig::template_name(name)` | Keep — a command whose template is shared with another command, or whose registry name is not its path. |
| `.templates_dir(path)` | Keep — templates outside the crate's source tree (an installed data directory, a user override directory). The macro can only embed paths known at compile time. |
| `.template_engine(engine)` | Keep — the custom-engine capability. Nothing else on this axis provides it, and ADR-0031 already fixes where the default engine may be constructed. |
| `.include_framework_templates(bool)` | Keep — the only way to decline the framework's own help and topic templates, which an application that registers its own `standout/help` must do. Default stays `true`. |
| `.template_ext(ext)` | **Delete.** It exists to make the convention's suffix agree with the files on disk — a disagreement that only exists because convention resolved `path + ".j2"` literally while `template_name` resolved through the registry's extension priority. Redefining the convention as the default `template_name` removes the disagreement instead of configuring around it. It never reached the case it looks like it solves: `embed_templates!` and `templates_dir` accept a fixed extension list (`.jinja`, `.jinja2`, `.j2`, `.txt`), so a custom engine's `.hbs` files were never loadable through this knob. |
| `CommandConfig::template(source)` | **Delete** — inline template source for a command. A command's template is a registry entry; that single rule is what `build()` validates, and it is the rule ADR-0019 was reaching for when it replaced a `String` that was one of three unrelated things. A one-line template becomes a one-line file. `TemplateRef::Inline` stays as a variant, reached by the render entry and by standalone help and topic templates. |
| `AppBuilder::command`'s third argument | **Delete** — inline source again, and its method is already deleted on the registration axis. |

## Theme provision — how a theme reaches the app

| Item | Verdict |
| --- | --- |
| `.styles(embed_styles!("src/styles"))` | **Blessed** for provision. It accepts `.css`, `.yaml` and `.yml`, so blessing it does not choose a stylesheet dialect. |
| `.default_theme(name)` | **Blessed** for selection. Both reference clients already name their theme. |
| `.styles_dir(path)` | Keep — stylesheets outside the crate (user themes read at run time). |
| `.theme(Theme)` | Keep — a theme computed in Rust rather than parsed from a stylesheet. |
| `.include_framework_styles(bool)` | Keep — declines the framework's style vocabulary from the base half of the merge (ADR-0020). Default stays `true`. |
| `Theme::new()`, `with_name`, `add`, `add_adaptive`, `add_icon`, `merge` | Keep — the programmatic cluster `.theme(Theme)` exists to feed, and `add_adaptive` is where an adaptive theme is expressed in Rust. |
| `Theme::from_yaml(&str)`, `Theme::from_css(&str)` | Keep — stylesheet text the application already holds (generated, fetched, or embedded by something other than the macro). Two items because they are two dialects, not two paths to one dialect: only the YAML parse reads icons. |
| `Theme::named(name)` | **Delete** — `Theme::new().with_name(name)`. |
| `Theme::from_file(path)`, `Theme::from_css_file(path)` | **Delete** — a stylesheet file reaches the app through `.styles(embed_styles!(…))` or `.styles_dir(…)`, both of which already read both dialects and register the result under a name. A bare file path with no registry entry is how an application ends up with a theme the `default_theme` name cannot select. |
| `Theme::from_variants(ThemeVariants)` | **Delete** — `ThemeVariants` is the stylesheet parser's own output type; the constructor exists for a producer that is not on the public path. |

Two silent behaviors go with the blessing, because both are the shape this epic exists to delete rather than separate defects. `.theme(Theme)` short-circuits theme resolution entirely: an application that calls both `.styles(…)` and `.theme(…)` today loses its whole stylesheet registry without a word. That combination becomes a `SetupError` naming both calls. And when no `.default_theme(name)` is set, resolution tries the registry names `default`, then `theme`, then `base` — a three-name fallback nobody documented. It goes: an application names its theme, and a registry with no `.default_theme(name)` resolves to no application theme, leaving the framework base.

## Entry points — how a process or a test enters the app

| Item | Verdict |
| --- | --- |
| `App::run(cmd, args) -> bool` | **Blessed** run form: detect, dispatch, write both streams, page, exit with the status. |
| `App::run_with(cmd, args, target, sources) -> CompletedRun` | **Blessed** capture form: destination properties and input sources passed in (ADR-0027), nothing written, nothing detected. `TestHarness::run` is its test-facing wrapper and already calls exactly this. |
| `App::dispatch(matches, output_mode)` | Keep — the application owns clap parsing and already holds `ArgMatches`. `extract_output_mode` is part of this path, not a separate entry. |
| `App::run_command(path, matches, handler, template)` | Keep — the manual-dispatch seam ROB04 repaired: one handler with its hooks and its render, without a dispatch table. It is what partial adoption documents. |
| `App::get_matches_from(cmd, argv, &InputSources) -> HelpResult` | Keep, taking today's `get_matches_from_with_sources` signature under the shorter name — the augmented `Command` and help interception for an application that parses for itself, and what feeds `dispatch`. |
| `App::render_with(template: TemplateRef, data, mode, target) -> Result<String, SetupError>` | Keep — rendering outside dispatch, for an embedding caller. Its first parameter becomes `TemplateRef` (ADR-0019's type), which is what folds the inline variant back into one entry. |
| `App::run_to_string`, `App::dispatch_from` | **Delete** — both are `run_with` with `TargetProperties::detect()` and `InputSources::from_process()` filled in, `run_to_string` additionally opening a render-diagnostics capture window that `TestHarness` opens for itself. Both are public, so both invite a test to measure the machine it runs on, which is the failure ADR-0026 and ADR-0027 exist to prevent. |
| `App::render`, `App::render_inline`, `App::render_inline_with` | **Delete** — `render`/`render_inline` are the detected-target halves of the pair, and the named/inline split is what `TemplateRef` carries. |
| `App::parse`, `App::parse_with` | **Delete** — `parse` calls `parse_with` with no change at all; `parse_with` is `parse_from(cmd, env::args())`. |
| `App::parse_from` | **Delete** — `get_matches_from` plus `println!` and `process::exit` on help or a parse error. An application that wants that behavior at the process edge wants `run`. |
| `App::get_matches`, and `get_matches_from`'s process-filled form | **Delete** — `get_matches` is `get_matches_from(cmd, env::args())`, and `get_matches_from` fills `InputSources::from_process()` for a caller who is about to be handed the parse anyway. The name survives on the sources-taking signature. |
| `cli::parse`, `cli::parse_from` | **Delete** — each builds a default `App` with no commands, templates or theme and parses through it. |
| `AppBuilder::default()` | **Delete** — `App::builder()` is the constructor; two names for one thing is the census's whole complaint in miniature. |

The residue on `App` that this axis does not rule on — `augment_command_with_help`, `verify_command`, `template_names`, `theme_names`, `registry`, `registry_mut`, `get_hooks`, `get_default_theme`, `get_theme` — is the visibility sweep's, judged by the same rule: an item stays `pub` when a blessed or reasoned path needs it.

## The capability map

Every capability the census's items expose, and the surviving item on the *same* axis that covers it. No row is empty; that is the property this table exists to hold.

| Capability | Axis | Covered by |
| --- | --- | --- |
| Static command set, nested groups | registration | `.commands(Commands::dispatch_config())` (the derive emits `GroupBuilder::group` for nesting) |
| Dynamic registration (set known only at run time) | registration | `.commands(\|g\| …)` with a hand-written closure |
| Stateful struct handlers | registration | `command_with(path, handler: impl Handler, configure)` — a struct implementing `Handler` |
| Closure and `#[handler]` fn handlers | registration + adaptation | the same method, through `FnHandler::new(f)` |
| Passthrough (handler owns its bytes, no render) | registration | `command_passthrough` |
| A single command added to a derive-registered app | registration | `command_with` |
| Typed parameters mapped to clap ids | adaptation | `#[handler]` with `#[flag]` / `#[arg]` / `#[ctx]` / `#[matches]` |
| Declaring the clap `Command` | declaration | clap-derive; a hand-built `clap::Command` when it is assembled at run time |
| Templates compiled into the binary | template | `embed_templates!` |
| File-backed templates and debug hot reload | template | `embed_templates!` (absolute source paths) and `templates_dir` |
| Templates outside the crate source tree | template | `templates_dir` |
| A custom template engine | template | `template_engine` |
| Declining the framework's own templates | template | `include_framework_templates(false)` |
| Naming a template that is not the command path | template | `CommandConfig::template_name` |
| Declaring that a command has no template | template | `CommandConfig::silent` / `structured_only` / `binary` |
| Stylesheets compiled into the binary, either dialect | theme | `embed_styles!` (`.css`, `.yaml`, `.yml`) |
| Stylesheets outside the crate; user themes at run time | theme | `styles_dir` |
| Several themes, one selected | theme | `styles` / `styles_dir` plus `default_theme(name)` |
| A theme built in Rust | theme | `Theme::new()` with `add`, `add_icon`, `merge`, passed to `.theme(…)` |
| Adaptive (light/dark) themes | theme | the stylesheet dialect's light/dark sections; `Theme::add_adaptive` programmatically |
| Stylesheet text the app already holds | theme | `Theme::from_yaml` (with icons), `Theme::from_css` |
| Declining the framework's style vocabulary | theme | `include_framework_styles(false)` |
| Running as a process: write, page, exit | entry | `run` |
| Capturing output with an injected destination and inputs | entry | `run_with`, and `TestHarness` over it |
| Rendering diagnostics captured for assertions | entry | `TestHarness` (it opens the capture window itself) |
| The application owns clap parsing | entry | `get_matches_from` then `dispatch`, with `extract_output_mode` |
| One handler dispatched by hand (partial adoption) | entry | `run_command` |
| Rendering data outside dispatch | entry | `render_with`, taking a `TemplateRef` |

## The adopter-seams additions are blessed as they land

The adopter-seams epic (`docs/spec/robustness-adopter-seams.md`) runs in parallel and adds public entries to the same files this ADR prunes. Every entry it names is **blessed on arrival**, and the pruning workstream must not remove one it finds:

- the `AppBuilder` output-mode fallback — the mode used when `--output` is absent, which ROB04 left hard-coded to `Auto` (#356);
- the handler-returnable domain error carrying an exit status (any nonzero `u8`) and a verbatim stderr payload, with zero rejected at construction (#357);
- confirmation-gate configuration — acceptance rule, prompt wording, and the stream it writes to (#354);
- public constants for the injected `--answers` and `--yes` argument ids (#354);
- the application-replaceable answer-sheet parser seam in `standout-input` (#351).

Four more of that epic's items change behavior without adding an entry, and are named here so a prune does not read them as accidents: pre-dispatch hooks receiving the deepest matches with a stated ordering rule (#352), a single framing for hook diagnostics (#353), `tabular()` reaching the whole-table width resolver (#359), and `-h/--help` and `-V/--version` rows in themed help (#334). An entry the seams epic adds that is *not* on this list amends this ADR when it lands; it does not arrive unblessed.

## Alternatives rejected

**Blessing the builder path and demoting the derive.** The pilot's collisions are all in the derive path, so "bless what already works" has a surface appeal. It fails on what the collisions are: five bounded defects in `standout-macros`, each with a filed mechanism and a one-change repair, against an axis where the builder path costs one call per command, per group and per template and re-teaches clap's vocabulary in Standout's words. Blessing the builder would also strand the enum as the single place a reader can see an application's whole command set.

**Deleting a layer instead of an item.** Deleting all builder registration in favour of the derive, or all of `Theme`'s constructors in favour of stylesheets, is a shorter ADR and a stranded capability each time: dynamic registration and passthrough have no derive spelling, and a computed theme has no stylesheet. The per-axis map is longer precisely because it is checkable.

**Counting alternatives instead of axes.** Reading the census's nine wiring items as nine competing ways to register a command produces a deletion that removes adaptation and leaves registration with nothing to bind. The axis rule is what keeps the arithmetic honest, and it is why this ADR's verdicts are stated per axis and never per item count.

**Keeping every path and documenting the choice.** This is what the repository does today, and the pilot measured it: four blind adopters, four independent collisions between a documented path and the macro implementing it. A framework whose examples disagree cannot be adopted from its documentation, and one more page explaining which of three ways to prefer is a fifth path.

## Consequences

The census's items go from 7 registration to 3 builder methods, 2 adaptation-and-declaration to 1 (plus clap's own, which is not this repository's to count), 9 template to 6, 7 `Theme` constructors to 3 behind an unchanged 5 builder methods, 18 entry points to 6, and 2 constructors to 1. The before/after count is the epic's metric and is recorded when the prune lands, not here.

Three renames ride with the deletions — `command_handler_with` to `command_with`, `get_matches_from_with_sources` to `get_matches_from`, `render_with`'s first parameter to `TemplateRef` — each of which is a deletion plus the survivor taking the shorter name, and none of which is a new capability. One loud failure rides with them: `.styles(…)` combined with `.theme(…)` becomes a `SetupError` naming both calls instead of the registry silently losing.

This is the program's deliberate compatibility break. Source compatibility for the deleted paths is not preserved, each deleted path's tests are deleted in the commit that deletes it, and the consolidated migration notes carry the whole list.
