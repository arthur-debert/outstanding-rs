## Create Oustanding Shell Applications for free

Outstanding is a rust library for building finely crafted non-interactive command line applications.

## The Use Case

If you're building your cli in Rust, chances are the core logic is not the regular small time string replacements of shell scripts, but rather a significan anount of logic or processing.

Your time should be spent on building the core logic, not on pampering the shell interface. You're likely using clap or another arg parsing library, which is great . From that onwards, you're responsible for the glue code between logic and output formatting. Rich and complex applications benefit from equally rich outputs, that present information dense results in a clear and consise manner.

While simple, with time, print statements creep in , and before you know it, your application can't be unit tested, and you're either ignoriing tests, or integration testings while reverse engineering the output formatting. Likewise formatted term output is hard to write and iterate on, hence you're likely delivering subpar user experiences.

## Oustanding Apps

Oustanding is a framework that takes on from a clap defined cli definition, and handles all the boiler plate for you: - Effectively enforcing logic and presentation seperation, keeping your code testable and maintanable. - Dispatching cli input from declaratively defined logic handlers and their command names. - Running your application logic , a pure rust function, with rich rust data types and regular rust restult data types . - Rendering the results of the command through outstanding expressive rendering layer: - Rich Term Support:

In short, oustanding gives you a set of primitives so you can focus on your app, while still easily offering a finely crafter user experience, helping your codebase to remanin well structured, leading to easier to write and maiain, both the application's core logic and it's user interface.

It's job is to ask: tell me where you logic is,  want it  outputs(templates), how it should look (styles), and I'll take care of the rest. By giving you optimal tools for rich term output , templates and styles, in a few lines of code and you're done.

## How Oustanding Works

Conceptually, oustanding core has the low level primitives that support the functionality.  The clap integration let's you leverage your existing clap definition, and map commands to rust functions.

While a significant part of the value comes from the integration and convinence in leveraging the default oustanding model, low level parts are exposed,  so you can use only the template rendered, or the template file based registry, or the auto dispatching layer. The higher livel features, like auto dispatch, is written as syntatic sugar on top of the lowel level ones., so you can pick and choose what parts you want to use.

This means that that you can still use only the rendering layer, template registry and so forth.

## The General Picture

### 1\. The Execution Flow

``` text
Input Parsing (clap) -> Dispatcher -> Logic Handler -> Rendering
```

The flow is responsible from taking the cli input, parsing it with clap, dispatching to the correct logic handler, running the logic handler, and rendering the result. calling any hooks along the way.  This is a significant amount of boiler plate saved.

With workflow, once you have the clap definition, all you need is point to the logic handlers and write the output templates.

### 2\. The Rendering Layer

The rendering layer is design to offer best-in-class developer workflow, with content, styling and code separation and hot releading during development.

#### 1\. Templates

The templating use minijijnja, which sports the familiar jinja sytax, a rich feature set which includes partials  and customizable filters and macros.

``` bbcode
[title]{{post.title}}[/title]
```

The combination offers a robust and easy to adopt syntax, with the ergonomics of style markup .

#### 2\. Styles

##### Styles can be defined in yaml files.  At their core, , they are represented as console::Style structs, but have two additional features: aliasing and adaptative attributes

###### 2.1 Aliasing

Aliasing allows for styles that simply refer to other sytles.  This allwos application writers to define semantic styles in code and templates, letting the presentation layer indirectly handle it, while keeping consistent styling between application components, views and so on.

For example you may have a title and commit-message styles, that both refer to the same actual values, but this allows you to alter each or both at your leisure without changing the code or templates.

###### 2.2 Adaptative Attributes

Adaptative attributes allow styles to hold different values depending on light and dark modes.

**What it looks like**:

```yaml
                    title:
                        fg: 
                            light: "black"
                            dark: "white"
                        bg: "white"
                    commit-message: title 

```

## Example Todo List Command

In this example, our task management app.

```rust
// The Data Model:
// Your application level data structures, annotated for serialization:
#[derive(Serialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Pending,
    Done,
}

#[derive(Serialize, Clone)]
pub struct Todo {
    pub title: String,
    pub status: Status,
}

#[derive(Serialize)]
pub struct TodoResult {
    pub message: Option<String>,
    pub todos: Vec<Todo>,
}

// The application logic for list:
#[dispatch]
pub fn list(_matches: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<TodoResult> {
    let todos = storage::list()?;

    Ok(Output::Render(TodoResult {
        message: None,
        todos,
    }))
}

// Your file system:
// src/
//     main.rs
//     ...
//     templates/
//         list.jinja
//         add.jinja  # each command has its own template, by default name-matched
//     styles/
//         default.yml

// Setup and configure outstanding:
let app = App::builder()
    .templates(embed_templates!("src/templates"))  // Embeds all .jinja/.j2/.txt files
    .styles(embed_styles!("src/styles"))           // Embeds all .yaml/.yml files
    .default_theme("default")                      // Set the default theme
    .commands(Commands::dispatch_config())         // Generated auto dispatch
    .build()?;

// Once configured, auto dispatch handles the cli input
app.run(Cli::command(), std::env::args());
```
