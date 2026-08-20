# Interactive Flows

This page is for apps that drive an interactive shell themselves — wizards, setup helpers, REPLs, anything that asks one question, reacts, asks the next. `standout` does not own the driver loop; you do. What it does provide is the two ingredients each step needs:

1. **Dynamic, themed text** for the step body — same `Renderer` + `Theme` you use for normal command output.
2. **Prompts** that work without a `&clap::ArgMatches` — every interactive source in `standout::input` exposes a `.prompt()` shortcut.

Composing those with a ~30-line step graph you own gives you the full pattern.

---

## The Step Graph You Own

Standout is deliberately not opinionated about flow control. A small, hand-rolled state machine is the right tool — you get loops, jumps, early exit, branching on side-effect output, all in idiomatic Rust:

```rust
use std::collections::HashMap;

enum Next {
    Go(&'static str),  // jump to a step (also used to re-ask)
    Done,
    Quit,
}

struct Step {
    render: fn(&Ctx, &Renderer) -> String,
    prompt: fn(&Ctx) -> Result<Answer, FlowError>,
    branch: fn(Answer, &mut Ctx) -> Next,
}

struct Ctx { /* whatever your wizard accumulates */ }
enum Answer { Text(String), Bool(bool), Choice(usize) }

fn run(steps: &HashMap<&str, Step>, mut ctx: Ctx, r: &Renderer) -> Result<(), FlowError> {
    let mut cur = "intro";
    loop {
        let step = &steps[cur];
        println!("{}", (step.render)(&ctx, r));
        let answer = (step.prompt)(&ctx)?;
        match (step.branch)(answer, &mut ctx) {
            Next::Go(next) => cur = next,
            Next::Done => return Ok(()),
            Next::Quit => return Err(FlowError::Cancelled),
        }
    }
}
```

That's the whole driver. From here on we focus on what each step looks like.

---

## A Step in Detail

### Render

Every step's body is a registered template, rendered against `Ctx`. Templates can use the full styling system: colors, adaptive themes, tags, `{% if %}` / `{% for %}`. The same machinery your CLI commands already use.

```rust
// One-time setup, before the loop
let theme = Theme::default()
    .add("title", Style::new().bold().cyan())
    .add("path",  Style::new().green());
let mut renderer = Renderer::new(theme)?;
renderer.add_template("pick_pack", PICK_PACK_TPL)?;

// Inside the step's render fn
fn render_pick_pack(ctx: &Ctx, r: &Renderer) -> String {
    r.render("pick_pack", ctx).expect("template")
}
```

The body of `pick_pack` template is just a normal standout template:

```jinja
[title]Choose a pack[/title]

Found [count]{{ packs | length }}[/count] packs in [path]{{ root }}[/path]:
{% for p in packs %}
  - {{ p.name }}{% if p.recommended %} [hint](recommended)[/hint]{% endif %}
{% endfor %}
```

Use `embed_templates!` for static templates so the wizard ships with no runtime file dependencies.

### Prompt

Every interactive source exposes `.prompt()`. No `&ArgMatches`, no chain — just call it:

```rust
use standout::input::{InquireSelect, InquireText, InquireConfirm};

// Free-form text
let pack = InquireText::new("Pack name:")
    .help("a-z0-9-")
    .prompt()?;                       // Result<String, InputError>

// Pick from options
let env = InquireSelect::new("Environment:", vec!["dev", "staging", "prod"])
    .prompt()?;                       // Result<&'static str, _>

// Yes/no
let proceed = InquireConfirm::new("Continue?")
    .default(true)
    .prompt()?;                       // Result<bool, _>
```

Behavior:

- Stdin not a TTY *or* empty submission → `InputError::NoInput`
- Otherwise → the typed value
- User cancellation is **backend-specific**:
  - `Inquire*` prompts: Esc / Ctrl+C → `InputError::PromptCancelled`
  - `TextPromptSource` / `ConfirmPromptSource`: EOF (Ctrl+D) → `InputError::PromptCancelled`; Ctrl+C terminates the process the same way it does for any line-buffered read
  - `EditorSource` (with `require_save`): closing the editor without saving → `InputError::EditorCancelled`

A re-ask on bad input is a single `match`:

```rust
fn prompt_pack_name(_ctx: &Ctx) -> Result<Answer, FlowError> {
    loop {
        let pack = InquireText::new("Pack name:").prompt()?;
        if valid_pack_name(&pack) {
            return Ok(Answer::Text(pack));
        }
        // Could render an error template here for context
        eprintln!("Pack names must be lowercase a-z, 0-9, '-'.");
    }
}
```

Same idea for `EditorSource` if a step opens an editor:

```rust
let body = EditorSource::new()
    .extension(".md")
    .initial_content("# Pack notes\n\n")
    .prompt()?;
```

### Branch

Pure user code. The branch decides the next step from the answer plus any side-effects you ran:

```rust
fn branch_pick_pack(answer: Answer, ctx: &mut Ctx) -> Next {
    let Answer::Text(pack) = answer else { return Next::Quit };
    ctx.pack = Some(pack.clone());
    match read_status(&ctx.root, &pack) {
        Ok(s) if s.dirty => Next::Go("confirm_dirty"),
        Ok(_) => Next::Go("apply"),
        Err(_) => Next::Go("setup_help"),
    }
}
```

---

## Restart Later

"Run the wizard again next week" is just `run(&steps, Ctx::fresh(), &renderer)`. If you want to resume mid-flow with previously collected state, make `Ctx` `Serialize`/`Deserialize`, persist on each branch, and pass `cur` and `Ctx` into `run`. Standout doesn't standardize a checkpoint format — but every piece of `Ctx` is your data, so serde is fine.

---

## Section Framing (cliclack-style)

`cliclack` ships nice `intro`/`outro`/`note`/`log` helpers for visual pacing. Standout doesn't ship equivalents, but the pattern is two lines of template:

```jinja
{# templates/note.jinja #}
[note_marker]●[/note_marker] [note_title]{{ title }}[/note_title]
{{ body }}
```

```rust
fn note(r: &Renderer, title: &str, body: &str) {
    let v = serde_json::json!({ "title": title, "body": body });
    println!("{}", r.render("note", &v).unwrap());
}
```

Style `note_marker` and `note_title` in your theme — adaptive light/dark falls out for free.

---

## Putting It Together

```rust
use std::collections::HashMap;
use standout::{Renderer, Theme};
use standout::input::{InquireConfirm, InquireSelect, InquireText};

fn main() -> anyhow::Result<()> {
    let mut renderer = Renderer::new(theme())?;
    register_templates(&mut renderer)?;

    let steps: HashMap<&str, Step> = HashMap::from([
        ("intro",        Step { render: render_intro,     prompt: noop_prompt,     branch: |_, _| Next::Go("pick_pack") }),
        ("pick_pack",    Step { render: render_pick_pack, prompt: prompt_pack,     branch: branch_pick_pack }),
        ("confirm_dirty",Step { render: render_dirty,     prompt: prompt_confirm,  branch: branch_dirty }),
        ("apply",        Step { render: render_apply,     prompt: noop_prompt,     branch: |_, _| Next::Done }),
        ("setup_help",   Step { render: render_help,      prompt: noop_prompt,     branch: |_, _| Next::Done }),
    ]);

    let ctx = Ctx::fresh();
    run(&steps, ctx, &renderer)?;
    Ok(())
}
```

You wrote ~50 lines of glue and got: themed dynamic text per step, polished TUI prompts, branching, looping, re-ask, restart. That's the deal: standout owns the *I/O quality*, you own the *flow shape*.

---

## Testing Wizards

A wizard built on `.prompt()` is fully testable in process — no real TTY, no `expectrl` subprocess. Every interactive source consults a [`PromptResponder`](https://docs.rs/standout-input/latest/standout_input/trait.PromptResponder.html) before it touches stdin; in tests you install a `ScriptedResponder` and the production wizard code is unchanged.

```rust
use serial_test::serial;
use standout_input::{PromptResponse, ScriptedResponder};
use standout_test::TestHarness;
use std::sync::Arc;

#[test]
#[serial]
fn setup_wizard_creates_pack_and_picks_environment() {
    let result = TestHarness::new()
        .prompts(Arc::new(ScriptedResponder::new([
            PromptResponse::text("foo"),     // pack name
            PromptResponse::Bool(true),      // confirm dirty
            PromptResponse::Choice(2),       // env: dev=0, staging=1, prod=2 -> "prod"
        ])))
        .run(&app(), command(), ["mycli", "setup"]);

    result.assert_success();
    result.assert_stdout_contains("Created pack `foo` in prod");
}
```

Two design choices to keep tests honest:

- **Open prompts** (`InquireText`, `InquirePassword`, `InquireEditor`, `TextPromptSource`, `EditorSource`) take `PromptResponse::Text("...")` — the answer *is* the value.
- **Finite-choice prompts** take a *position*, not a label. `Choice(2)` picks `options[2]` from whatever the wizard passed to `InquireSelect::new`. Renaming `"Production"` to `"Live"` in the option list doesn't break a test that picked index 2 — the wizard logic is unchanged, only copy moved. Same for `Confirm`: assert on the bool, not on `"y"`/`"yes"`.

`ScriptedResponder` validates each response against the prompt kind the source actually asked for. A wizard reorder bug — e.g., a `Confirm` step swapped to land where a `Text` was expected — fails the test loudly with the position, the prompt kind, and the queued response, rather than producing a silently wrong assertion three steps later.

Two kind-agnostic responses cover the cancel and skip branches:

```rust
PromptResponse::Cancel  // -> Err(InputError::PromptCancelled) inside the wizard
PromptResponse::Skip    // -> Err(InputError::NoInput)        — same path as "no TTY"
```

Use them to test the wizard's abort and re-ask logic without involving real signal handling.

For lower-level tests that don't need the harness, put the responder on [`InputSources`](https://docs.rs/standout-input/latest/standout_input/struct.InputSources.html):

```rust
use std::sync::Arc;
use standout_input::{InputSources, ScriptedResponder, PromptResponse};

#[test]
fn pack_name_validation_re_asks_on_invalid() {
    let sources = InputSources::from_process().with_responder(Arc::new(ScriptedResponder::new([
        PromptResponse::text("BadName!"),  // first try, rejected by validator
        PromptResponse::text("good-name"), // re-ask, accepted
    ])));

    assert_eq!(
        prompt_pack_name_from(&sources).unwrap(),
        Answer::Text("good-name".into())
    );
}
```

The harness places that responder on the run's `InputSources` when used as `.prompts(...)`.

---

## When to Reach for the Framework Instead

If your interactive flow is launched as a subcommand of an otherwise-normal CLI app (e.g. `mycli setup`), you can still use `App::builder()` for everything *outside* the wizard — argument parsing, help rendering, the other commands. Just have the `setup` handler call your wizard `run()` function. The handler itself produces `Output::Silent` (or a small summary) and lets the wizard own its own stdout while it runs. See [Framework Integration](framework-integration.md) for the broader CLI integration story.
