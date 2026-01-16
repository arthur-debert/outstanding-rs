# Outstanding How To

This is a small, focused guide for adopting Outstanding in a working shell application. Each step is self-sufficient, takes a positive step towards a sane CLI design, and can be incrementally merged. This can be done for one command (probably a good idea), then replicated to as many as you'd like.

Note that only 2 out of 8 steps are Outstanding related. The others are generally good practices and clear designs for maintainable shell programs. This is not an accident, as Outstanding's goal is to allow your app to keep a great structure effortlessly, while providing testability, rich and fast output design, and more.

For explanation's sake, we will show a hypothetical list command for todoier, a todo list manager.

1\. Start: The Argument Parsing

  Arg parsing is insanely intricate and deceptively simple. In case you are not already: define your   application's interface with clap. Nothing else is worth doing until you have a sane starting point.

  (If you are using a non-clap-compatible crate, for now, you'd have to write an adapter for clap.)

2\. Hard Split Logic and Formatting

  Now, your command should be split into two functions: the logic handler and its rendering. Don't worry   about the specifics, do the straightest path from your current code.

  This is the one key step, the key design rule. And that's not because Outstanding requires it,   rather the other way around: Outstanding is designed on top of it, and keeping it separate and easy to   iterate on both logic and presentation under this design is Outstanding's key value.

  If your CLI is in good shape this will be a small task, otherwise you may find yourself patching together   print statements everywhere, tidying up the data model and centralizing the processing. The silver   lining here being: if it takes considerable work, there will be considerable gain in doing so.

``` rust
use clap::ArgMatches;

// Data types for your domain
#[derive(Clone)]
pub enum Status { Pending, Done }

#[derive(Clone)]
pub struct Todo {
pub title: String,
pub status: Status,
}

pub struct TodoResult {
pub message: Option<String>,
pub todos: Vec<Todo>,
}

// This is your core logic handler, receiving parsed clap args
// and returning a pure Rust data type.
pub fn list(matches: &ArgMatches) -> TodoResult {
let show_done = matches.get_flag("all");
let todos = storage::list().unwrap();

let filtered: Vec<Todo> = if show_done {
todos
} else {
todos.into_iter()
.filter(|t| matches!(t.status, Status::Pending))
.collect()
};

TodoResult {
message: None,
todos: filtered,
}
}

// This will take the Rust data type and print the result to stdout
pub fn render_list(result: TodoResult) {
if let Some(msg) = result.message {
println!("{}", msg);
}
for (i, todo) in result.todos.iter().enumerate() {
let status = match todo.status {
Status::Done => "[x]",
Status::Pending => "[ ]",
};
println!("{}. {} {}", i + 1, status, todo.title);
}
}

// And the orchestrator:
pub fn list_command(matches: &ArgMatches) {
render_list(list(matches))
}
```

Intermezzo A: Milestone Logic and Presentation Split\!   It's easy to miss it, so let me be loud about this: congratulations, you've done the single most important step   for a sane CLI design. At this point:

  The rendering can also be tested by feeding data inputs and matching outputs. That's true, but also brittle (small   changes in output formatting or messages break many tests, etc). We'll see how that's better later.

3\. Fine Tune the Logic Handler's Return Type

  While any data type works, Outstanding's renderer takes a generic type that must implement `Serialize`.   This enables automatic JSON/YAML output modes and template rendering through MiniJinja's context system.   This is likely a small change, and beneficial as a baseline for logic results that will simplify writing renderers later.

``` rust
use serde::Serialize;

#[derive(Serialize)]
pub struct TodoResult {
pub message: Option<String>,
pub todos: Vec<Todo>,
}
```

4\. Replace Imperative Print Statements With a Template

  Reading a template of an output next to the substituting variables is much easier to reason about than scattered   prints, string concats and the like. If your template is quite simple, a std::fmt can be a good intermediate step   and a checkpoint. Else skip to the next one.

5\. Use a MiniJinja Template String

  Rewrite your std::fmt or imperative prints into a MiniJinja template string, and add minijinja to your crate.   If you're not familiar with it, it's a Rust implementation of Jinja, pretty much a de-facto standard for   more complex templates. The MiniJinja documentation is excellent: https://docs.rs/minijinja and the Jinja   template syntax reference: https://jinja.palletsprojects.com/en/3.1.x/templates/

  And then you call render in MiniJinja, passing the template string and the data to use.

``` rust

pub fn render_list(result: TodoResult) {
let output_tmpl = r#"
{% if message %}{{ message }} {% endif %}
{% for todo in todos %}
{{ loop.index }}. {{ todo.title }} [{{ todo.status }}]
{% endfor %}
"#;

let env = minijinja::Environment::new();
let tmpl = env.template_from_str(output_tmpl).unwrap();
let output = tmpl.render(&result).unwrap();
println!("{}", output);
}
```

6\. Use a Dedicated Template File

  Now, move the template content into a file (say templates/list.jinja), and embed its content in the rendering module.   Dedicated files have several advantages: triggering editor/IDE support for the file type, more descriptive diffs, less   risk of breaking the code/build and, in the event that you have less technical people helping out with the UI, a much   cleaner and simpler way for them to contribute.

``` jinja
{% if message %}{{ message }}
{% endif %}
{% for todo in todos %}
{{ loop.index }}. {{ todo.title }} [{{ todo.status }}]
{% endfor %}
```

Intermezzo B: Declarative Output Definition

  Congratulations, you've now reached another significant milestone. Instead of a maze of print statements and string   concatenations, you have a much simpler, less error-prone and easier to maintain rendering blueprint.   This plays well when reviewing code (are we changing logic or just display?) and more.

  Note: for some complex data models and complex outputs, this by itself does not produce a great result,   as it's all a one-pass render. The symptom being a template filled with conditionals and branches. Do not fret, however, as   very simple solutions to this exist: partials (smaller templates that can be embedded in larger ones), variables, and   template filters and macros.

  Also, notice we've yet to do anything Outstanding specific. This is not a coincidence, as the framework is born out of   making using and leveraging this design for easy testability, development speed and rich feature set easy under this design.

**Recap**:

Now your application looks like this:   - Clap-defined interface   - Orchestration list function that takes in parsed clap parameters, passing them to your application logic only handler,   which in turn returns your `Serialize` data type, which, in turn, gets passed to the rendering function. The latter   now is dead simple, with a template declaration and rendering the template with the command's result.

## 7\. Outstanding: Offload the Handler Orchestration

``` toml
```

2\. Annotate your list (the app logic) function with a dispatch macro. This tells Outstanding that the "list" command    should be dispatched to this logic. That's all Outstanding needs to know, and now it can manage the execution.

  use outstanding::cli::{Dispatch, CommandContext, HandlerResult, Output};   use clap::{ArgMatches, Subcommand};

  // Define your commands enum with the Dispatch derive   \#\[derive(Subcommand, Dispatch)\]   \#\[dispatch(handlers = handlers)\]   pub enum Commands {

  }

  // Your handlers module   mod handlers {

<!-- lex:rust -->

<!-- /lex:rust -->

  }

``` rust
use outstanding::cli::App;
use outstanding::{embed_templates, embed_styles};

let app = App::builder()
.templates(embed_templates!("src/templates"))   // Embeds all .jinja/.j2 files
.styles(embed_styles!("src/styles"))            // Embeds all .yaml/.css files
.default_theme("default")                       // Use the "default" theme
.commands(Commands::dispatch_config())          // Register handlers from derive macro
.build()?;
```

4\. The final bit: handling the dispatching off to Outstanding:   // In your main function   fn main() -\> anyhow::Result\<()\> {

<!-- lex:rust -->

<!-- /lex:rust -->

  }

If your app has other clap commands that are not managed by Outstanding, use `run_to_string` instead:   match app.run\_to\_string(Cli::command(), std::env::args()) {

<!-- lex:rust -->

<!-- /lex:rust -->

  }

And now you can remove the boilerplate: the orchestrator (list\_command) and the rendering (render\_list). You're pretty much at global optima: a single line of derive macro links your app logic to a command name, a few lines configure Outstanding, and auto dispatch handles all the boilerplate.

For the next commands you'd wish to migrate, this is even simpler. Say you have a "create" logic handler: add a "create.jinja" to that template dir, add the derive macro for the create function and that is it. By default the macro will match the command's name to the handlers and to the template files, but you can change these and map explicitly to your heart's content.

## Intermezzo C: Welcome to Outstanding

We've seen some of the benefits: once your organization is well structured (split logic and presentation, template-driven presentation) a single line of code hooks up dispatch, your logic and the template.

This creates a logical barrier where it's easy to keep the decoupling of logic and presentation, makes working on the output easier and safer, and the naming conventions tend to bring more consistency to most codebases too.

But there are more:   - You can alter the template and re-run your CLI, without compilation, and the new template will be used. Much faster dev workflow.   - Your CLI just got quite a few output options via --output:

## 8\. Make the Output Awesome

Let's transform that mono-typed, monochrome string into a richer and more useful UI. Borrowing from web apps setup, we keep the content in a template file, and we define styles in a stylesheet file:   - Create the styles/default.css (or default.yaml)   - In your app builder add the styles and the default theme (already done in step 7):

<!-- lex:rust -->

<!-- /lex:rust -->

**let app = App::builder()**:

  .templates(embed\_templates\!("src/templates"))   .styles(embed\_styles\!("src/styles"))       // Load stylesheets   .default\_theme("default")                  // Use styles/default.css or default.yaml   .commands(Commands::dispatch\_config())   .build()?;

And in the CSS file (styles/default.css):   /\* Styles for completed todos \*/   .done {

  }

  /\* Style for todo index numbers \*/   .index {

  }

  /\* Style for pending todos \*/   .pending {

  }

  /\* Adaptive style for messages \*/   .message {

  }

**@media (prefers-color-scheme: light) {**:

  .pending { color: black; }

  }

**@media (prefers-color-scheme: dark) {**:

  .pending { color: white; }

<!-- lex:css -->

<!-- /lex:css -->

  }

``` yaml
done:
strikethrough: true
fg: gray
index:
fg: yellow
pending:
bold: true
fg: white
light:
fg: black
dark:
fg: white
message:
fg: cyan
```

``` jinja
{% if message %}[message]{{ message }}[/message]
{% endif %}
{% for todo in todos %}
[index]{{ loop.index }}.[/index] [{{ todo.status }}]{{ todo.title }}[/{{ todo.status }}]
{% endfor %}
```

The style tags use BBCode-like syntax: \[style-name\]content\[/style-name\] Notice how we use \[{{ todo.status }}\] dynamically - if todo.status is "done", it applies the .done style; if it's "pending", it applies the .pending style.

**Now add this to your app bulder, to cin**:

<!-- lex:rust -->

 

<!-- /lex:rust -->

Now you're leveraging the core rendering design of Outstanding:   - File-based templates for content, and stylesheets for styles   - Custom template syntax with BBCode for markup styles \[style\][/style](/style)   - Live reload: iterate through content and styling without recompiling

You can now serve a finely crafted experience, as formatting is easy to control, scalable (easy to render, to reuse) and fast (live reload).

Intermezzo D: The Full Setup Is Done

  A few lines to configure where to look for templates and themes, marking each command's handler and triggering the autodispatch   is all it takes to leverage Outstanding. You're now enjoying a clear hard boundary between data and presentation, a fully unit   testable logic and dispatch, a rich set of tools from MiniJinja to stylesheets and themes that, together with hot reload, make   producing high quality outputs a breeze.

  You've also gained automated output formats, with structured data support and a plethora of tools to help in debugging (term-debug),   JSON output. For brevity's sake, we've ignored a bunch of finer and relevant points:

  Aside from exposing the library primitives, Outstanding leverages best-in-breed crates like MiniJinja and console::Style under the hood.   The lock-in is really negligible: you can use Outstanding's BB parser or swap it, manually dispatch handlers, and use the renderers directly in your clap dispatch.
