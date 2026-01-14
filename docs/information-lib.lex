Information Lib

  This document contains a library of howtos , or a FAQ like collection of knowledge from which we will build the full documentation.
  This is to be lex formatted, no markdown. That entails: 
    - numbered items as titles, surrounded by blank lines 
    - Content one tab stop indented
    - file refs are [./crates/] like this.
    - Code samples are done through 
      1. A subject (what you are showing), ending with a colon. 
      2. The code sample , 1 stop indented
      3. The closing block with the language tag (i.e. :: rust ::)

      For example: 
        println!("hi mom"); 
      :: rust::

    1. How To Adopt Outstanding Dispatch alongside vanilla Clap Loops

      This allows for partial adoption, a likely user path, testing in a 
      command or two before taking the plunge.

      More than an adoption pathway, this is useful for large and old apps
      where there is value in migrating to outstanding for a few of the more
      complex commands, but not much to migrate the boiler plate commands.

      We want to show two different solution: 

        - Call outstanding auto-dispatch inside your clap main handler, for 
        the unmatched commands.
        - Use the outstanding auto dispatch, and check if no command is found. If that is the case, 
        call your legacy loop.

      p.s.: How is the autodispatch api? I see : 
        let cmd = clap::Command::new("myapp");
        app.run(cmd, std::env::args());
      :: rust ::

      Which  looks alright, but not sure if a cleaner rust api would be to app.match(cli::command(), std::env::args()) -> return a Option ,
      the some case boing a command which you could cmd.run() or otherwise failed?


================================================================================
THEME: Architecture & Flow
================================================================================


2. What is the execution flow?

	Outstanding follows a linear pipeline from CLI input to rendered output:

		Clap Parsing → Dispatch → Handler → Hooks → Rendering → Output

	Clap Parsing: Your existing clap Command definition is augmented with
	Outstanding's flags (--output, help integration) and parsed normally.

	Dispatch: Outstanding extracts the command path from the parsed ArgMatches,
	navigating through subcommands to find the deepest match. It then looks up
	the registered handler for that path.

	Handler: Your logic function executes. It receives the ArgMatches and a
	CommandContext, and returns a HandlerResult<T> - either data to render,
	a silent marker, or binary content.

	Hooks: If registered, hooks run at three points:
	  - pre_dispatch: Before the handler (can abort)
	  - post_dispatch: After the handler, before rendering (can transform data)
	  - post_output: After rendering (can transform the final string)

	Rendering: The handler's output data is serialized and passed through the
	template engine, producing styled terminal output (or structured data like
	JSON, depending on output mode).

	Output: The result is written to stdout or a file.

	This pipeline is what Outstanding manages for you - the glue code between
	"I have a clap definition" and "I want rich, testable output."


3. What is two-pass rendering?

	Templates are processed in two distinct passes:

	Pass 1 - MiniJinja: Standard template processing. Variables are substituted,
	control flow executes, filters apply.

	A template with style tags before rendering:
		{% for item in items %}
		[title]{{ item.name | upper }}[/title]
		{% endfor %}
	:: jinja ::

	After this pass, you have a string with all variables resolved, but style
	tags remain as literal text: [title]WIDGET[/title]

	Pass 2 - BBParser: Style tag processing. The bracket-notation tags are
	parsed and converted to ANSI escape codes (or stripped, depending on
	output mode).

	BBParser transformation (assuming title is bold green):
		Input:  [title]WIDGET[/title]
		Output: \x1b[1;32mWIDGET\x1b[0m
	:: text ::

	This separation means:
	  - Template logic (loops, conditionals) is handled by a mature,
	    well-documented engine
	  - Style application is a simple, predictable transformation
	  - You can debug each pass independently (use TermDebug mode to see
	    tags as literals)

	The tag syntax ([name]...[/name]) was chosen over Jinja filters because
	it reads naturally in templates and doesn't interfere with Jinja's own
	syntax.


4. How does dispatch work?

	Dispatch is the process of routing parsed CLI arguments to the correct
	handler.

	When you register commands with AppBuilder:
		App::builder()
		    .command("list", list_handler)
		    .group("db", |g| g
		        .command("migrate", migrate_handler)
		        .command("status", status_handler))
		    .build()?
	:: rust ::

	Outstanding builds an internal registry mapping command paths to handlers:
	  - ["list"] → list_handler
	  - ["db", "migrate"] → migrate_handler
	  - ["db", "status"] → status_handler

	When app.run() executes:
	  1. Clap parses the arguments
	  2. Outstanding traverses the ArgMatches subcommand chain to find the
	     deepest match
	  3. It extracts the command path (e.g., ["db", "migrate"])
	  4. It looks up the handler for that path
	  5. It executes the handler with the appropriate ArgMatches slice

	If no handler matches, run() returns RunResult::NoMatch(matches), letting
	you fall back to manual dispatch for commands Outstanding doesn't manage.


5. What is a command path?

	A command path is a vector of strings representing the subcommand chain
	the user invoked.

	For a CLI invocation like:
		myapp db migrate --steps 5
	:: shell ::

	The command path is ["db", "migrate"].

	This path is:
	  - Used internally to look up the registered handler
	  - Available in CommandContext.command_path for your handler to inspect
	  - Used to match hooks to specific commands (e.g., .hooks("db.migrate", ...))

	The dot notation ("db.migrate") is used when registering hooks, while the
	vector form is used at runtime. They represent the same thing.

	Command paths enable:
	  - Precise hook targeting (run this hook only for db migrate, not db status)
	  - Handler introspection (know which command is running without parsing
	    args again)
	  - Hierarchical command organization with group() builders


================================================================================
THEME: Handlers
================================================================================


6. What is the Handler trait?

	The Handler trait defines the interface for command logic. It lives in
	[./crates/outstanding/src/cli/handler.rs].

	The trait definition:
		pub trait Handler: Send + Sync {
		    type Output: Serialize;
		    fn handle(&self, matches: &ArgMatches, ctx: &CommandContext) -> HandlerResult<Self::Output>;
		}
	:: rust ::

	Key constraints:
	  - Send + Sync required: handlers may be called from multiple threads
	  - Output must be Serialize: needed for JSON/YAML modes and template context
	  - Both parameters are immutable references: handlers cannot modify args or context

	Implementing the trait directly is useful when your handler needs internal
	state (database connections, configuration, etc.). For stateless logic,
	closure handlers are more convenient.


7. How do closure handlers work?

	Closures are wrapped in FnHandler<F, T> which implements the Handler trait.
	This is done automatically when you use .command().

	The closure signature:
		fn(&ArgMatches, &CommandContext) -> HandlerResult<T>
		where T: Serialize + Send + Sync
	:: rust ::

	The closure must be Fn (not FnMut or FnOnce) because Outstanding may
	need to call it multiple times in certain scenarios.

	Registering a closure handler:
		App::builder()
		    .command("list", |matches, ctx| {
		        let verbose = matches.get_flag("verbose");
		        Ok(Output::Render(ListResult { items, verbose }))
		    })
	:: rust ::


8. What is HandlerResult?

	HandlerResult<T> is the return type for handlers:
		pub type HandlerResult<T> = Result<Output<T>, anyhow::Error>;
	:: rust ::

	This is a standard Result type, so the ? operator works naturally for
	error propagation:
		fn list_handler(matches: &ArgMatches, ctx: &CommandContext) -> HandlerResult<Items> {
		    let items = storage::load()?;      // Propagates errors
		    let filtered = filter_items(&items)?;
		    Ok(Output::Render(Items { filtered }))
		}
	:: rust ::

	Errors become the command output - Outstanding formats and displays them.


9. What is the Output enum?

	Output<T> represents what a handler produces. Defined in
	[./crates/outstanding/src/cli/handler.rs]:
		pub enum Output<T: Serialize> {
		    Render(T),
		    Silent,
		    Binary { data: Vec<u8>, filename: String },
		}
	:: rust ::

	Render(T): The common case. Data is serialized to JSON, passed to the
	template engine, and rendered with styles. In structured output modes
	(--output json), the template is skipped and data serializes directly.

	Silent: No output produced. Useful for commands with side effects only
	(delete, update). Post-output hooks still run and can transform Silent
	into something else if needed.

	Binary: Raw bytes written to a file. The filename is used directly as a
	path (relative or absolute). The bytes are written via std::fs::write(),
	overwriting any existing file. A confirmation message prints to stderr:
	"Wrote N bytes to filename".


10. What is CommandContext?

	CommandContext provides execution environment information to handlers.
	It has exactly two fields:
		pub struct CommandContext {
		    pub output_mode: OutputMode,
		    pub command_path: Vec<String>,
		}
	:: rust ::

	output_mode: The resolved output format (Term, Text, Json, etc.). Handlers
	can inspect this to adjust behavior - for example, skipping interactive
	prompts in JSON mode.

	command_path: The subcommand chain as a vector, e.g., ["db", "migrate"].
	Useful for logging or conditional logic based on which command is running.

	Note: CommandContext is intentionally minimal. Application-specific context
	(config, connections) should be captured in struct handlers or closures.


11. How do I access CLI arguments in a handler?

	The ArgMatches parameter provides access to parsed arguments through
	clap's standard API:
		fn handler(matches: &ArgMatches, ctx: &CommandContext) -> HandlerResult<Data> {
		    // Flags
		    let verbose = matches.get_flag("verbose");

		    // Options with values
		    let name: &String = matches.get_one("name").unwrap();

		    // Optional values
		    let limit: Option<&u32> = matches.get_one("limit");

		    // Multiple values
		    let tags: Vec<&String> = matches.get_many("tags")
		        .map(|v| v.collect())
		        .unwrap_or_default();

		    Ok(Output::Render(Data { ... }))
		}
	:: rust ::

	For subcommands, you receive the ArgMatches for your specific command,
	not the root. Outstanding navigates to the deepest match before calling.


12. How do I return no output (Silent)?

	Return Output::Silent when the command performs an action but has nothing
	to display:
		fn delete_handler(matches: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<()> {
		    let id: &String = matches.get_one("id").unwrap();
		    storage::delete(id)?;
		    Ok(Output::Silent)
		}
	:: rust ::

	Silent behavior in the pipeline:
	  - Post-output hooks still receive RenderedOutput::Silent and can transform it
	  - If --output-file is set, nothing is written (no-op)
	  - Nothing prints to stdout

	Note: The type parameter for Output::Silent is often () but can be any
	Serialize type - it's never used.


13. How do I return binary data?

	Use Output::Binary for non-text output like archives, images, or PDFs:
		fn export_handler(matches: &ArgMatches, _ctx: &CommandContext) -> HandlerResult<()> {
		    let data = generate_report()?;
		    let pdf_bytes = render_to_pdf(&data)?;

		    Ok(Output::Binary {
		        data: pdf_bytes,
		        filename: "report.pdf".into(),
		    })
		}
	:: rust ::

	The filename is used as a literal file path. Outstanding writes the bytes
	using std::fs::write() and prints a confirmation to stderr. The filename
	can be:
	  - Relative: "output/report.pdf" (relative to current directory)
	  - Absolute: "/tmp/report.pdf"
	  - Dynamic: format!("report-{}.pdf", timestamp)

	Binary output bypasses the template engine entirely.


14. What does the #[dispatch] attribute do?

	The #[dispatch] attribute macro generates command registration from an
	enum. It lives in the outstanding-macros crate.

	Basic usage:
		#[derive(Dispatch)]
		enum Commands {
		    List,
		    Add,
		    Remove,
		}
	:: rust ::

	This generates a dispatch_config() method that registers handlers. Variant
	names are converted to snake_case command names:
	  - List → "list"
	  - ListAll → "list_all"
	  - HTTPServer → "h_t_t_p_server" (each capital becomes _lowercase)

	The macro expects handler functions named after the variant in snake_case
	(e.g., fn list(...) for List variant).

	Variant attributes for customization:
		#[derive(Dispatch)]
		enum Commands {
		    #[dispatch(handler = custom_list_fn)]
		    List,

		    #[dispatch(template = "custom/add.j2")]
		    Add,

		    #[dispatch(pre_dispatch = validate_auth)]
		    Remove,

		    #[dispatch(skip)]
		    Internal,

		    #[dispatch(nested)]
		    Db(DbCommands),
		}
	:: rust ::

	The nested attribute is required for subcommand enums - it's not inferred
	from tuple variants.


================================================================================
THEME: Hooks
================================================================================


15. What is the hooks system?

	Hooks are functions that run at specific points in the command pipeline,
	allowing you to intercept, validate, or transform data without modifying
	handler logic.

	The Hooks struct holds three vectors of hook functions:
		pub struct Hooks {
		    pre_dispatch: Vec<PreDispatchFn>,
		    post_dispatch: Vec<PostDispatchFn>,
		    post_output: Vec<PostOutputFn>,
		}
	:: rust ::

	Hooks are registered per command path and stored in App's command_hooks
	HashMap. The key is dot-notation: "db.migrate" for the db migrate command.


16. What are the three hook phases?

	Pre-dispatch: Runs before the handler. Can abort execution.
	  - Use for: authentication, validation, logging start time
	  - Cannot modify data (handler hasn't run yet)

	Post-dispatch: Runs after handler, before rendering. Can transform data.
	  - Use for: adding timestamps, filtering sensitive fields, data enrichment
	  - Receives handler output as serde_json::Value

	Post-output: Runs after rendering. Can transform final output.
	  - Use for: adding headers/footers, compression, encryption
	  - Receives RenderedOutput (Text, Binary, or Silent)


17. What is the pre_dispatch hook signature?

	Pre-dispatch hooks validate or abort before the handler runs:
		Fn(&ArgMatches, &CommandContext) -> Result<(), HookError>
	:: rust ::

	The hook receives arguments and context but returns no data - only
	success or failure.

	Aborting execution:
		Hooks::new().pre_dispatch(|matches, ctx| {
		    if !is_authenticated() {
		        return Err(HookError::pre_dispatch("authentication required"));
		    }
		    Ok(())
		})
	:: rust ::

	Multiple pre-dispatch hooks run sequentially. The first error aborts -
	subsequent hooks don't run, and the handler never executes.


18. What is the post_dispatch hook signature?

	Post-dispatch hooks transform handler data before rendering:
		Fn(&ArgMatches, &CommandContext, serde_json::Value) -> Result<serde_json::Value, HookError>
	:: rust ::

	The handler's output is serialized to serde_json::Value before hooks run.
	This allows generic transformations regardless of the handler's output type.

	Adding a timestamp to all responses:
		Hooks::new().post_dispatch(|_matches, _ctx, mut data| {
		    if let Some(obj) = data.as_object_mut() {
		        obj.insert("timestamp".into(), json!(chrono::Utc::now().to_rfc3339()));
		    }
		    Ok(data)
		})
	:: rust ::

	Multiple post-dispatch hooks chain: each receives the output of the
	previous hook. This enables composable transformations.


19. What is the post_output hook signature?

	Post-output hooks transform the final rendered output:
		Fn(&ArgMatches, &CommandContext, RenderedOutput) -> Result<RenderedOutput, HookError>
	:: rust ::

	RenderedOutput is an enum:
		enum RenderedOutput {
		    Text(String),
		    Binary(Vec<u8>, String),  // (bytes, filename)
		    Silent,
		}
	:: rust ::

	Adding a footer to text output:
		Hooks::new().post_output(|_matches, _ctx, output| {
		    match output {
		        RenderedOutput::Text(s) => {
		            Ok(RenderedOutput::Text(format!("{}\n--\nGenerated by MyApp", s)))
		        }
		        other => Ok(other),
		    }
		})
	:: rust ::

	Post-output hooks can transform Silent into Text or Binary, enabling
	conditional output based on results or context.


20. How do hook errors work?

	HookError contains diagnostic information:
		pub struct HookError {
		    pub message: String,
		    pub phase: HookPhase,
		    pub source: Option<Box<dyn std::error::Error + Send + Sync>>,
		}
	:: rust ::

	Creating errors:
		HookError::pre_dispatch("auth required")
		HookError::post_dispatch("validation failed")
		HookError::post_output("transform failed")

		// With underlying cause:
		HookError::pre_dispatch("db error").with_source(db_error)
	:: rust ::

	Error display format: "hook error (pre-dispatch): auth required"

	When a hook returns an error:
	  - Execution stops immediately
	  - Remaining hooks in that phase don't run
	  - For pre-dispatch: handler never executes
	  - For post phases: output is discarded
	  - The error message becomes the command output


21. Can I register multiple hooks for the same phase?

	Yes. Each phase holds a Vec of hooks, and calling the builder method
	multiple times appends:
		Hooks::new()
		    .pre_dispatch(log_start)
		    .pre_dispatch(check_auth)
		    .pre_dispatch(validate_input)
		    .post_dispatch(add_metadata)
		    .post_dispatch(filter_sensitive)
	:: rust ::

	Execution order:
	  - Pre-dispatch: sequential, abort on first error
	  - Post-dispatch: chained, each transforms data from previous
	  - Post-output: chained, each transforms output from previous

	For post_dispatch and post_output, the chaining means hook order matters:
	add_metadata runs before filter_sensitive, so filter_sensitive sees the
	added metadata.


22. How do I attach hooks to commands?

	Two approaches:

	Via AppBuilder.hooks():
		App::builder()
		    .command("migrate", migrate_handler)
		    .hooks("db.migrate", Hooks::new()
		        .pre_dispatch(require_admin)
		        .post_dispatch(log_migration))
		    .build()?
	:: rust ::

	Via command_with() inline:
		App::builder()
		    .command_with("migrate", migrate_handler, |cfg| cfg
		        .pre_dispatch(require_admin)
		        .post_dispatch(log_migration))
		    .build()?
	:: rust ::

	The command path uses dot notation: "db.migrate" matches the command
	registered under .group("db", |g| g.command("migrate", ...)).


================================================================================
THEME: Rendering
================================================================================


23. What are the render functions and how do they differ?

	Outstanding provides several render functions with increasing control.
	All live in [./crates/outstanding/src/render/functions.rs].

	render(template, data, theme) -> Result<String>
	  - Simplest form. Auto-detects terminal capabilities AND color mode.
	  - Use when you want Outstanding to decide everything.

	render_with_output(template, data, theme, mode) -> Result<String>
	  - Explicit output mode (Term, Text, Auto), but auto-detects color mode.
	  - Use to honor the --output CLI flag.

	render_with_mode(template, data, theme, output_mode, color_mode) -> Result<String>
	  - Full control over both output mode AND color mode (Light/Dark).
	  - Use in tests or when forcing specific rendering behavior.

	render_auto(template, data, theme, mode) -> Result<String>
	  - Smart dispatch: structured modes skip templating entirely.
	  - JSON/YAML/XML/CSV: serializes data directly, template ignored.
	  - Term/Text/Auto: normal two-pass rendering.
	  - Use as the default for CLI commands with --output support.

	The "auto" in render_auto refers to automatic template-vs-serialization
	dispatch, not color detection.


24. How does two-pass rendering work?

	Templates are processed in two distinct passes:

	Pass 1 - MiniJinja:
	  - Variable substitution: {{ data.name }}
	  - Control flow: {% for item in items %}, {% if condition %}
	  - Filters: {{ value | upper }}
	  - Output: String with style tags still as literal text

	Pass 2 - BBParser:
	  - Parses [tag]...[/tag] bracket notation
	  - Looks up tag name in resolved styles
	  - Applies ANSI escape codes (or strips tags, depending on mode)

	Example flow:
		Template: [title]{{ name }}[/title] has {{ count }} items
		Data: { name: "Report", count: 42 }

		After Pass 1: [title]Report[/title] has 42 items
		After Pass 2: \x1b[1;32mReport\x1b[0m has 42 items
	:: text ::

	This separation keeps template logic (MiniJinja) independent from
	styling concerns (BBParser).


25. How do style tags work in templates?

	Style tags use BBCode-like bracket notation:
		[style-name]content to style[/style-name]
	:: text ::

	The style-name must match a style defined in the theme. Tags can:
	  - Nest: [outer][inner]text[/inner][/outer]
	  - Span multiple lines
	  - Contain template logic: [title]{% if x %}{{ x }}{% endif %}[/title]

	What happens based on OutputMode:
	  - Term: Tags replaced with ANSI escape codes
	  - Text: Tags stripped, plain text remains
	  - TermDebug: Tags kept as literals for debugging
	  - Structured (JSON, etc.): Tags stripped (template not used anyway)


26. What happens with unknown style tags?

	When a tag references a style not in the theme, behavior depends on mode:

	Term mode: Unknown tags get a ? marker appended:
		Template: [unknown]hello[/unknown]
		Output:   [unknown?]hello[/unknown?]
	:: text ::

	Text mode: Unknown tags are stripped like any other tag.

	TermDebug mode: Tags preserved as-is (no ? added).

	The ? marker helps catch typos during development. In production,
	validate_template() can catch these at startup or in CI.


27. How do I validate templates?

	validate_template() catches style tag errors without producing output:
		let result = validate_template(template, &data, &theme);
		if let Err(e) = result {
		    eprintln!("Template errors: {}", e);
		}
	:: rust ::

	The function:
	  1. Renders the template with MiniJinja (catches syntax errors)
	  2. Validates all style tags against the theme
	  3. Returns Err with UnknownTagErrors listing undefined tags

	Use this at application startup or in tests to fail fast on typos.


28. What template filters are available?

	Beyond MiniJinja's built-in filters, Outstanding adds:

	nl - Appends a newline:
		{{ value | nl }}
	:: jinja ::

	col(width, ...) - Format for column display:
		{{ value | col(10) }}
		{{ value | col(20, align='right') }}
		{{ value | col(15, truncate='middle', ellipsis='...') }}
	:: jinja ::

	  Arguments: width (number or "fill"), align (left/right/center),
	  truncate (start/middle/end), ellipsis (default "…")

	pad_left(width), pad_right(width), pad_center(width) - Padding:
		{{ "42" | pad_left(8) }}     ->  "      42"
		{{ "hi" | pad_right(8) }}    ->  "hi      "
		{{ "hi" | pad_center(8) }}   ->  "   hi   "
	:: jinja ::

	truncate_at(width, position, ellipsis) - Truncate with ellipsis:
		{{ long_text | truncate_at(20, 'end') }}
		{{ path | truncate_at(30, 'middle', '...') }}
	:: jinja ::

	display_width - Returns visual width (handles Unicode):
		{% if value | display_width > 20 %}...{% endif %}
	:: jinja ::


29. How does the TemplateRegistry work?

	TemplateRegistry resolves template names to content. Resolution priority:

	  1. Inline templates (added via add_inline() or add_template())
	  2. File-based templates (from directories)

	Supported extensions (in priority order): .jinja, .jinja2, .j2, .txt

	When you request "config", the registry checks:
	  - Inline template named "config"
	  - config.jinja in registered directories
	  - config.jinja2, config.j2, config.txt (lower priority)

	Cross-directory collisions (same name in multiple dirs) raise an error.
	Same-directory collisions use extension priority (no error).


30. How does hot reloading work?

	In debug builds, file-based templates are re-read from disk on each
	render. This enables editing templates without recompiling.

	In release builds, templates are cached in memory after first load.

	The Renderer struct tracks which templates are inline vs file-based:
	  - Inline: always served from cache (no file to reload)
	  - File-based in debug: re-read every render
	  - File-based in release: cached after first read

	This is automatic - no configuration needed.


31. How does context injection work?

	Context injection adds extra values to the template context beyond
	handler data. Useful for utilities, formatters, or runtime values.

	Static context (fixed values):
		App::builder()
		    .context("version", "1.0.0")
		    .context("app_name", "MyApp")
	:: rust ::

	Dynamic context (computed at render time):
		App::builder()
		    .context_fn("terminal_width", |ctx| {
		        Value::from(ctx.terminal_width.unwrap_or(80))
		    })
		    .context_fn("is_color", |ctx| {
		        Value::from(ctx.output_mode.should_use_color())
		    })
	:: rust ::

	In templates:
		{{ app_name }} v{{ version }}
		{% if terminal_width > 100 %}...{% endif %}
	:: jinja ::

	When handler data and context have the same key, handler data wins.
	Context is supplementary, not an override mechanism.


32. How do structured output modes work?

	Structured modes (Json, Yaml, Xml, Csv) bypass template rendering entirely.
	The handler's data is serialized directly.

	render_auto() implements this dispatch:
		OutputMode::Json  -> serde_json::to_string_pretty(data)
		OutputMode::Yaml  -> serde_yaml::to_string(data)
		OutputMode::Xml   -> quick_xml::se::to_string(data)
		OutputMode::Csv   -> flatten and format as CSV
	:: text ::

	This means:
	  - Template content is ignored
	  - Style tags never apply
	  - Context injection is skipped
	  - What you serialize is what you get

	For CSV, data is flattened automatically, or you can provide a
	FlatDataSpec for explicit column control.


================================================================================
THEME: Themes & Styles
================================================================================


33. What is a Theme?

	A Theme is a named collection of styles that maps style names to
	console formatting. Defined in [./crates/outstanding/src/theme/theme.rs].

	Internally, Theme holds three HashMaps:
		struct Theme {
		    base: HashMap<String, Style>,     // Default styles
		    light: HashMap<String, Style>,    // Light mode overrides
		    dark: HashMap<String, Style>,     // Dark mode overrides
		    aliases: HashMap<String, String>, // Name -> target mappings
		}
	:: rust ::

	Only overrides are stored in light/dark maps - if a style is the same
	in all modes, it only appears in base.


34. How do I create a Theme programmatically?

	Use the builder pattern:
		let theme = Theme::new()
		    .add("title", Style::new().bold().cyan())
		    .add("muted", Style::new().dim())
		    .add("error", Style::new().red().bold())
		    .add("disabled", "muted");  // Alias
	:: rust ::

	For adaptive styles (different in light/dark mode):
		theme.add_adaptive(
		    "panel",
		    Style::new().bold(),                    // base (shared)
		    Some(Style::new().fg(Color::Black)),    // light override
		    Some(Style::new().fg(Color::White)),    // dark override
		)
	:: rust ::

	The base style provides shared attributes. Mode-specific overrides
	are merged with base - Some values replace, None values preserve.


35. How do I define styles in YAML?

	YAML stylesheets support multiple formats:

	Full attribute form:
		header:
		  fg: cyan
		  bold: true
		  underline: true
	:: yaml ::

	Shorthand (single value):
		accent: cyan            # Color only
		emphasis: bold          # Attribute only
		warning: "yellow bold"  # Color + attributes (quoted)
	:: yaml ::

	Alias (string that's not a color/attribute):
		disabled: muted         # References another style
	:: yaml ::

	Adaptive (mode-specific overrides):
		panel:
		  fg: gray
		  bold: true
		  light:
		    fg: black
		  dark:
		    fg: white
	:: yaml ::


36. What style attributes are supported?

	All attributes are optional booleans or colors:

	Colors:
	  - fg: foreground color
	  - bg: background color

	Text attributes:
	  - bold: true/false
	  - dim: true/false
	  - italic: true/false
	  - underline: true/false
	  - blink: true/false
	  - reverse: true/false
	  - hidden: true/false
	  - strikethrough: true/false


37. What color formats are supported?

	Named colors (16 ANSI):
		fg: red
		fg: cyan
		fg: bright_green
	:: yaml ::

	Available names: black, red, green, yellow, blue, magenta, cyan, white,
	gray (or grey), plus bright_ variants of each.

	256-color palette (number):
		fg: 208         # Orange in 256-color palette
	:: yaml ::

	RGB hex:
		fg: "#ff6b35"   # 6-digit hex
		fg: "#f80"      # 3-digit shorthand (expands to #ff8800)
	:: yaml ::

	RGB array:
		fg: [255, 107, 53]
	:: yaml ::


38. What is style aliasing?

	Aliases let one style name refer to another:
		title:
		  fg: cyan
		  bold: true
		commit-message: title   # Alias - uses title's style
		section-header: title   # Another alias to same style
	:: yaml ::

	Benefits:
	  - Semantic names in templates ([commit-message]) that resolve to
	    visual styles (title)
	  - Change one definition, update all aliases
	  - Templates stay readable, styling stays flexible

	Aliases can chain: a -> b -> c -> concrete style. Cycles are detected
	and rejected during validation.


39. How does alias resolution work?

	When resolving a style name:
	  1. Look up name in styles map
	  2. If Concrete(Style), return it
	  3. If Alias(target), look up target (repeat)
	  4. Track visited names to detect cycles

	Resolution with cycle detection:
		styles.resolve("semantic")
		  -> "semantic" is Alias("presentation")
		  -> "presentation" is Alias("visual")
		  -> "visual" is Concrete(Style::cyan())
		  -> returns Style::cyan()
	:: text ::

	If a cycle is detected (a -> b -> a), resolution returns None and
	validation fails with CycleDetected error.


40. What happens with undefined styles?

	When a template uses [unknown]text[/unknown] and "unknown" isn't defined:

	Default behavior: Prepend "(!?)" indicator:
		Output: (!?) text
	:: text ::

	Custom indicator:
		let styles = Styles::new()
		    .missing_indicator("[MISSING] ");
	:: rust ::

	Disable indicator (silent fallback):
		let styles = Styles::new()
		    .missing_indicator("");
	:: rust ::

	With empty indicator, undefined styles render as plain unstyled text.


41. How does ColorMode detection work?

	Outstanding auto-detects the OS color scheme using the dark-light crate:
		pub fn detect_color_mode() -> ColorMode
	:: rust ::

	Returns ColorMode::Light or ColorMode::Dark based on system preferences.

	Override for testing or user preference:
		set_theme_detector(|| ColorMode::Dark);  // Force dark mode
	:: rust ::

	The detector is global (thread-local). Set it early in main() if needed.


42. How does adaptive style resolution work?

	When resolving styles, the Theme merges base with mode-specific overrides:

	Given YAML:
		panel:
		  fg: gray
		  bold: true
		  light:
		    fg: black
		  dark:
		    fg: white
	:: yaml ::

	Resolution in Dark mode:
	  - Start with base: fg=gray, bold=true
	  - Merge dark override: fg=white (replaces gray), bold unchanged
	  - Result: fg=white, bold=true

	Resolution in Light mode:
	  - Start with base: fg=gray, bold=true
	  - Merge light override: fg=black, bold unchanged
	  - Result: fg=black, bold=true

	Merging uses Option semantics: Some value in override replaces base,
	None (missing) preserves base.


43. How do I load themes from files?

	Single file:
		let theme = Theme::from_file("themes/dark.yaml")?;
	:: rust ::

	The theme name is extracted from the filename (without extension).

	Directory of themes (StylesheetRegistry):
		let mut registry = StylesheetRegistry::new();
		registry.load_directory("themes/")?;

		let dark = registry.get("dark")?;   // themes/dark.yaml
		let light = registry.get("light")?; // themes/light.yaml
	:: rust ::

	Supported extensions: .yaml, .yml


44. How do I validate a theme?

	Themes validate automatically during construction, but you can also
	validate explicitly:
		let result = theme.validate();
		match result {
		    Ok(()) => println!("Theme is valid"),
		    Err(StyleValidationError::UnresolvedAlias { from, to }) => {
		        eprintln!("Alias '{}' points to undefined '{}'", from, to);
		    }
		    Err(StyleValidationError::CycleDetected { path }) => {
		        eprintln!("Cycle detected: {}", path.join(" -> "));
		    }
		}
	:: rust ::

	Validation catches:
	  - Dangling aliases (point to undefined styles)
	  - Circular references (a -> b -> a)


================================================================================
THEME: Output Modes
================================================================================


45. What OutputMode variants exist?

	OutputMode controls how output is formatted. Eight variants total:
		pub enum OutputMode {
		    Auto,       // Auto-detect terminal capabilities
		    Term,       // Always use ANSI escape codes
		    Text,       // Never use ANSI codes (plain text)
		    TermDebug,  // Keep style tags as [name]...[/name]
		    Json,       // Serialize as JSON (skip template)
		    Yaml,       // Serialize as YAML (skip template)
		    Xml,        // Serialize as XML (skip template)
		    Csv,        // Serialize as CSV (skip template)
		}
	:: rust ::

	Three categories:
	  - Templated: Auto, Term, Text (render template, vary ANSI handling)
	  - Debug: TermDebug (template rendered, tags kept as literals)
	  - Structured: Json, Yaml, Xml, Csv (skip template, serialize directly)


46. How does Auto mode resolve?

	Auto mode queries the terminal for color support using the console crate:
		Term::stdout().features().colors_supported()
	:: rust ::

	If colors are supported, Auto behaves like Term (ANSI codes applied).
	If not supported, Auto behaves like Text (tags stripped).

	This detection happens at render time, not at startup. Piping output
	to a file or another process typically disables color support.


47. What CLI flag values map to OutputMode?

	The --output flag accepts these values (case-sensitive):
		--output=auto        -> OutputMode::Auto (default)
		--output=term        -> OutputMode::Term
		--output=text        -> OutputMode::Text
		--output=term-debug  -> OutputMode::TermDebug
		--output=json        -> OutputMode::Json
		--output=yaml        -> OutputMode::Yaml
		--output=xml         -> OutputMode::Xml
		--output=csv         -> OutputMode::Csv
	:: text ::

	The flag is global - it applies to all subcommands. Outstanding adds
	it automatically via augment_command().


48. What is TermDebug mode for?

	TermDebug preserves style tags in the output instead of applying or
	stripping them:
		Template: [title]Hello[/title]
		Output:   [title]Hello[/title]
	:: text ::

	Use cases:
	  - Debugging template issues (see which tags are applied where)
	  - Verifying style tag placement without terminal interference
	  - Automated testing of template output

	Unlike Term mode, unknown tags don't get the ? marker in TermDebug.


49. How does --output-file work?

	The --output-file-path flag redirects output to a file instead of stdout:
		myapp list --output-file-path=results.txt
	:: shell ::

	Behavior:
	  - Text output: written to file, nothing printed to stdout
	  - Binary output: written to file (same as without flag)
	  - Silent output: no-op (nothing written)

	After writing to file, the internal output becomes Silent to prevent
	double-printing. The file is overwritten if it exists.


50. Where is output_mode stored and passed?

	output_mode flows through several layers:

	1. App struct stores the default:
		pub struct App {
		    pub(crate) output_mode: OutputMode,  // Default: Auto
		}
	:: rust ::

	2. CLI parsing overrides from --output flag

	3. CommandContext carries it to handlers:
		pub struct CommandContext {
		    pub output_mode: OutputMode,
		    pub command_path: Vec<String>,
		}
	:: rust ::

	4. Render functions receive it as parameter

	Handlers can inspect ctx.output_mode to adjust behavior - for example,
	skipping interactive prompts when output is Json.


51. What do should_use_color() and is_structured() do?

	Helper methods on OutputMode for conditional logic:

	should_use_color() - Returns true if ANSI codes should be applied:
		Auto  -> depends on terminal detection
		Term  -> true
		Text  -> false
		TermDebug -> false (tags kept, not converted)
		Structured modes -> false
	:: text ::

	is_structured() - Returns true if template should be skipped:
		Json, Yaml, Xml, Csv -> true
		All others -> false
	:: text ::

	Use these in render logic to branch behavior without matching all variants.


================================================================================
THEME: App Configuration
================================================================================


52. What is the App struct?

	App is the runtime container for Outstanding configuration. It holds:
		pub struct App {
		    registry: TopicRegistry,              // Help topics
		    output_flag: Option<String>,          // --output flag name
		    output_file_flag: Option<String>,     // --output-file-path flag name
		    output_mode: OutputMode,              // Current mode
		    theme: Option<Theme>,                 // Default theme
		    command_hooks: HashMap<String, Hooks>, // Path -> hooks
		    template_registry: Option<TemplateRegistry>,
		    stylesheet_registry: Option<StylesheetRegistry>,
		}
	:: rust ::

	App is created via AppBuilder.build() and is typically used via run()
	or dispatch() methods.


53. What is AppBuilder and what methods does it have?

	AppBuilder configures Outstanding before creating the App. Key methods:

	Resource embedding:
	  .templates(embed_templates!("path"))  - Embed templates at compile time
	  .styles(embed_styles!("path"))        - Embed stylesheets at compile time

	Runtime overrides (for user customization):
	  .templates_dir("~/.myapp/templates")  - Add directory, overrides embedded
	  .styles_dir("~/.myapp/themes")        - Add directory, overrides embedded

	Theme selection:
	  .default_theme("dark")      - Select theme from stylesheet registry
	  .theme(theme)               - Set explicit Theme object

	Command registration:
	  .command(name, handler, template)      - Register closure handler
	  .command_with(name, handler, config)   - With inline hooks/template config
	  .group(name, configure)                - Create nested command group
	  .commands(dispatch_config)             - From #[derive(Dispatch)] enum

	Hooks:
	  .hooks("path.cmd", hooks)   - Attach hooks to command path

	Context:
	  .context(key, value)        - Static context for templates
	  .context_fn(key, provider)  - Dynamic context computed at render

	Flags:
	  .output_flag(Some("format"))    - Rename --output flag
	  .no_output_flag()               - Disable --output flag
	  .output_file_flag(Some("out"))  - Rename --output-file-path
	  .no_output_file_flag()          - Disable file output flag

	Topics:
	  .add_topic(topic)           - Add help topic
	  .topics_dir("docs/topics")  - Load topics from directory


54. How do embed_templates! and embed_styles! work?

	These macros run at compile time to embed files into the binary:
		App::builder()
		    .templates(embed_templates!("src/templates"))
		    .styles(embed_styles!("src/styles"))
	:: rust ::

	embed_templates! collects files matching: .jinja, .jinja2, .j2, .txt
	embed_styles! collects files matching: .yaml, .yml

	The macros produce EmbeddedSource<T> containing:
	  - Static array of (filename, content) pairs
	  - Source path for debug hot-reload

	In debug builds with source path present, files are re-read from disk
	(hot reload). In release builds, embedded content is used.


55. What happens during build()?

	AppBuilder.build() performs these steps:

	1. Theme resolution (priority order):
	   - Explicit .theme() if set
	   - Load from stylesheet_registry using .default_theme() name
	   - None if neither configured

	2. Validation:
	   - If default_theme_name set but not found: SetupError::ThemeNotFound

	3. Registry conversion:
	   - EmbeddedTemplates -> TemplateRegistry
	   - EmbeddedStyles -> StylesheetRegistry

	4. Transfer to App struct

	What's NOT validated at build():
	  - Templates aren't checked (resolved lazily at render time)
	  - Command handlers aren't validated
	  - Hook signatures verified at registration, not build


56. What's the difference between run(), run_to_string(), and parse()?

	Three entry points with different return behaviors:

	run(cmd, args) -> bool:
	  - Parses, dispatches, prints output to stdout
	  - Returns true if handled, false if no match
	  - Binary output writes to file with stderr confirmation
	  - Use for: typical CLI main()

	run_to_string(cmd, args) -> RunResult:
	  - Parses, dispatches, returns result without printing
	  - Returns RunResult::Handled(String) or RunResult::NoMatch(ArgMatches)
	  - Use for: testing, capturing output

	parse(cmd) -> ArgMatches:
	  - Only parses arguments (with help interception)
	  - Returns clap ArgMatches for manual dispatch
	  - Use for: hybrid dispatch, custom control flow


57. What does augment_command() add?

	Outstanding adds these to your clap Command:

	1. Custom help subcommand:
		myapp help           # Show main help
		myapp help topic     # Show specific topic
		myapp help --page    # Use pager for long help
	:: shell ::

	2. Global --output flag:
		myapp list --output=json
		myapp db migrate --output=yaml
	:: shell ::

	3. Global --output-file-path flag:
		myapp list --output-file-path=results.txt
	:: shell ::

	These are added via augment_command() which run() calls automatically.
	For manual control, call augment_command() on your Command before parsing.


58. How does command registration work?

	Commands are registered with a path and mapped to handlers:

	Simple registration:
		.command("list", handler, "templates/list.j2")
	:: rust ::

	Nested groups:
		.group("db", |g| g
		    .command("migrate", migrate_handler, "db/migrate.j2")
		    .command("status", status_handler, "db/status.j2"))
	:: rust ::

	This creates paths: "db.migrate", "db.status"

	From derive macro:
		.commands(Commands::dispatch_config())
	:: rust ::

	The dispatch_config() returns a closure that registers all variants.


59. How are templates resolved at runtime?

	Template resolution order:

	1. Inline templates (highest priority)
	2. Embedded templates (from embed_templates!)
	3. File templates (from .templates_dir())
	4. Convention path: template_dir + command_name + template_ext

	When resolving "db/migrate":
	  - Check inline: "db/migrate"
	  - Check embedded: "db/migrate.jinja", "db/migrate.j2", etc.
	  - Check file dirs for matching files

	If nothing found and structured mode (JSON), template is ignored anyway.


================================================================================
THEME: Partial Adoption
================================================================================


60. What is RunResult and what does NoMatch contain?

	RunResult is returned by dispatch and run_to_string:
		pub enum RunResult {
		    Handled(String),              // Handler ran, output string
		    Binary(Vec<u8>, String),      // Binary output (bytes, filename)
		    NoMatch(ArgMatches),          // No handler found
		}
	:: rust ::

	NoMatch contains the clap ArgMatches from parsing. Use this for
	fallback dispatch - the matches are fully parsed and ready for your
	own subcommand handling.

	Helper methods:
		result.is_handled()     -> bool
		result.output()         -> Option<&str>
		result.matches()        -> Option<&ArgMatches>


61. How do I fall back to my own dispatch?

	Check for NoMatch and handle unregistered commands yourself:
		let app = App::builder()
		    .command("list", list_handler, "list.j2")
		    .build()?;

		match app.run_to_string(cmd, args) {
		    RunResult::Handled(output) => println!("{}", output),
		    RunResult::Binary(bytes, filename) => {
		        std::fs::write(&filename, bytes)?;
		    }
		    RunResult::NoMatch(matches) => {
		        // Your existing dispatch logic
		        match matches.subcommand() {
		            Some(("status", sub)) => handle_status(sub),
		            Some(("config", sub)) => handle_config(sub),
		            _ => eprintln!("Unknown command"),
		        }
		    }
		}
	:: rust ::

	This allows gradual migration: add Outstanding handlers one at a time.


62. How do I use Outstanding for just rendering (no CLI)?

	The rendering layer is fully decoupled from CLI integration:
		use outstanding::{render, render_auto, Theme};
		use console::Style;

		let theme = Theme::new()
		    .add("ok", Style::new().green())
		    .add("err", Style::new().red());

		// Simple render
		let output = render(
		    "[ok]Success:[/ok] {{ message }}",
		    &data,
		    &theme,
		)?;

		// With explicit mode (honor --output flag)
		let output = render_auto(
		    "{{ items | length }} items",
		    &data,
		    &theme,
		    OutputMode::Json,  // Will serialize, not render
		)?;
	:: rust ::

	No App, no CLI integration - just templates, data, and themes.


63. How do I add Outstanding to just one command?

	Register only the commands you want Outstanding to handle:
		let app = App::builder()
		    .templates(embed_templates!("templates"))
		    .styles(embed_styles!("styles"))
		    .default_theme("default")
		    .command("list", list_handler, "list.j2")
		    // Other commands NOT registered
		    .build()?;

		let cmd = Command::new("myapp")
		    .subcommand(Command::new("list"))    // Outstanding handles
		    .subcommand(Command::new("status"))  // You handle
		    .subcommand(Command::new("config")); // You handle

		match app.run_to_string(cmd, args) {
		    RunResult::Handled(s) => println!("{}", s),
		    RunResult::Binary(bytes, filename) => {
		        std::fs::write(&filename, bytes)?;
		        eprintln!("Wrote {} bytes to {}", bytes.len(), filename);
		    }
		    RunResult::NoMatch(m) => my_dispatch(m),
		}
	:: rust ::


64. What's the minimal Outstanding setup?

	Absolute minimum for one command with inline template:
		let app = App::builder()
		    .command("info", |_m, _ctx| {
		        Ok(Output::Render(json!({"version": "1.0"})))
		    }, "Version: {{ version }}")
		    .build()?;

		app.run(cmd, std::env::args());
	:: rust ::

	No embedded files, no themes (uses empty theme), no hooks.
	Style tags without a theme will show the ? marker but still render.


65. How do I use Outstanding dispatch inside existing clap dispatch?

	Call Outstanding first, fall through on NoMatch:
		fn main() {
		    let cmd = build_cli();  // Your clap Command
		    let app = build_outstanding_app();  // Outstanding App

		    // Try Outstanding first
		    let matches = cmd.clone().get_matches();
		    match app.dispatch(matches.clone(), OutputMode::Auto) {
		        RunResult::Handled(output) => {
		            println!("{}", output);
		            return;
		        }
		        RunResult::Binary(bytes, filename) => {
		            std::fs::write(&filename, bytes).ok();
		            return;
		        }
		        RunResult::NoMatch(_) => {
		            // Fall through to existing dispatch
		        }
		    }

		    // Your existing dispatch
		    match matches.subcommand() {
		        Some(("legacy", sub)) => legacy_handler(sub),
		        _ => {}
		    }
		}
	:: rust ::


66. How do I use existing clap dispatch with Outstanding fallback?

	The reverse: try your dispatch first, use Outstanding for specific commands:
		fn main() {
		    let cmd = build_cli();
		    let matches = cmd.get_matches();

		    // Your existing dispatch
		    match matches.subcommand() {
		        Some(("legacy", sub)) => {
		            legacy_handler(sub);
		            return;
		        }
		        Some(("new-feature", _)) => {
		            // Use Outstanding for this one
		        }
		        _ => {}
		    }

		    // Outstanding handles "new-feature" and others
		    let app = build_outstanding_app();
		    match app.dispatch(matches, OutputMode::Auto) {
		        RunResult::Handled(output) => println!("{}", output),
		        RunResult::Binary(bytes, filename) => {
		             std::fs::write(filename, bytes).ok();
		        }
		        _ => {} // Handle Silent or NoMatch if needed
		    }
		}
	:: rust ::


================================================================================
THEME: Tables
================================================================================


67. What is the table formatting system?

	Outstanding provides utilities for formatting columnar output, handling
	Unicode widths, and extracting data for CSV export. The system lives in
	[./crates/outstanding/src/table/].

	Core components:
	  - Column: defines width, alignment, truncation for one column
	  - FlatDataSpec: complete table layout with columns and decorations
	  - TableFormatter: applies spec to format rows of data
	  - Utility functions: display_width, truncate_*, pad_*
	  - Template filters: col, pad_left, pad_right, etc.

	The system handles Unicode correctly (CJK characters count as 2 columns)
	and preserves ANSI escape codes without counting them toward width.


68. What is the Column struct?

	Column defines how one column behaves:
		pub struct Column {
		    width: Width,              // Fixed, Bounded, or Fill
		    align: Align,              // Left, Right, Center (default: Left)
		    truncate: TruncateAt,      // End, Start, Middle (default: End)
		    ellipsis: String,          // Truncation indicator (default: "…")
		    null_repr: String,         // For missing values (default: "-")
		    style: Option<String>,     // Theme style name
		    key: Option<String>,       // JSON path for extraction
		    header: Option<String>,    // CSV header title
		}
	:: rust ::

	Builder pattern:
		Column::new(Width::Fixed(12))
		    .align(Align::Right)
		    .truncate(TruncateAt::Middle)
		    .ellipsis("...")
		    .key("author.name")
		    .header("Author")
	:: rust ::


69. What are the Width variants?

	Width controls how column width is determined:
		pub enum Width {
		    Fixed(usize),                    // Exactly n display columns
		    Bounded { min: usize, max: usize }, // Auto-size within bounds
		    Fill,                            // Expand to fill remaining space
		}
	:: rust ::

	Fixed: Always exactly the specified width. Content truncated or padded.

	Bounded: Width calculated from actual content, clamped to min/max.
	When resolving without data, uses min. When resolving with data,
	examines all rows to find the maximum needed width.

	Fill: Takes all remaining space after Fixed and Bounded columns.
	Multiple Fill columns split remaining space evenly. If no Fill
	columns exist, the rightmost Bounded column expands (ignoring max)
	to ensure the table fills available width.


70. What are Align and TruncateAt?

	Align controls text positioning within the column:
		pub enum Align {
		    Left,    // Default - padding on right
		    Right,   // Padding on left (for numbers)
		    Center,  // Padding on both sides
		}
	:: rust ::

	TruncateAt controls where text is cut when too long:
		pub enum TruncateAt {
		    End,     // Default - "Hello Wor…"
		    Start,   // "…llo World"
		    Middle,  // "Hel…orld" (biased right)
		}
	:: rust ::

	Middle truncation keeps both start and end visible, useful for paths
	or identifiers where both prefix and suffix matter.


71. What is FlatDataSpec?

	FlatDataSpec defines a complete table layout:
		let spec = FlatDataSpec::builder()
		    .column(Column::new(Width::Fixed(7)))
		    .column(Column::new(Width::Fixed(20)))
		    .column(Column::new(Width::Fill))
		    .separator("  ")
		    .prefix("| ")
		    .suffix(" |")
		    .build();
	:: rust ::

	The spec includes:
	  - Columns: ordered list of Column definitions
	  - Separator: string between columns (default: single space)
	  - Prefix/suffix: strings at row start/end

	Key methods:
	  extract_header() - returns CSV header row from column headers
	  extract_row(data) - extracts values using column keys from JSON
	  resolve_widths(total) - calculates widths without examining data
	  resolve_widths_from_data(total, data) - calculates from actual content


72. How does width resolution work?

	Width resolution distributes available space among columns:

	1. Calculate overhead: prefix + suffix + (separator × (columns - 1))

	2. First pass - allocate known widths:
	   - Fixed: use exact width
	   - Bounded: use min (or calculate from data if provided)
	   - Fill: mark for later

	3. Calculate remaining space after Fixed and Bounded

	4. Second pass - distribute remaining:
	   - Fill columns split remaining space evenly
	   - Extra pixels distributed one per column until exhausted
	   - If no Fill columns: rightmost Bounded expands to fill

	Example with 80 columns total:
		Column::new(Width::Fixed(10))       // Gets 10
		Column::new(Width::Bounded{5, 20})  // Gets 5-20 based on content
		Column::new(Width::Fill)            // Gets remainder
	:: text ::


73. What is TableFormatter?

	TableFormatter applies a FlatDataSpec to format rows:
		let spec = FlatDataSpec::builder()
		    .column(Column::new(Width::Fixed(8)))
		    .column(Column::new(Width::Fill))
		    .separator(" | ")
		    .build();

		let formatter = TableFormatter::new(&spec, 60);  // 60 cols total

		let row = formatter.format_row(&["abc123", "Description here"]);
		// Output: "abc123   | Description here"
	:: rust ::

	Methods:
	  format_row(values) - format one row, returns String
	  format_rows(rows) - format multiple rows, returns Vec<String>

	Missing values use the column's null_repr (default "-").


74. How do the utility functions work?

	All functions handle Unicode and ANSI codes correctly.

	display_width(s) - Returns visual width in terminal columns:
		display_width("hello")      // 5
		display_width("日本語")      // 6 (CJK = 2 each)
		display_width("\x1b[31mred\x1b[0m")  // 3 (ANSI ignored)
	:: rust ::

	truncate_end(s, width, ellipsis) - Keep start, cut end:
		truncate_end("Hello World", 8, "…")  // "Hello W…"
	:: rust ::

	truncate_start(s, width, ellipsis) - Keep end, cut start:
		truncate_start("Hello World", 8, "…")  // "…o World"
	:: rust ::

	truncate_middle(s, width, ellipsis) - Keep both ends:
		truncate_middle("abcdefghij", 7, "...")  // "ab...ij"
	:: rust ::

	pad_left(s, width) - Right-align with left padding:
		pad_left("42", 6)  // "    42"
	:: rust ::

	pad_right(s, width) - Left-align with right padding:
		pad_right("hi", 6)  // "hi    "
	:: rust ::

	pad_center(s, width) - Center with padding on both sides:
		pad_center("hi", 6)  // "  hi  "
	:: rust ::


75. How do table filters work in templates?

	Template filters provide column formatting inline:

	col(width, ...) - Main column filter:
		{{ value | col(10) }}
		{{ value | col(10, align='right') }}
		{{ value | col(15, truncate='middle', ellipsis='...') }}
	:: jinja ::

	Arguments:
	  - width: number or "fill" (fill requires explicit width kwarg)
	  - align: 'left', 'right', 'center'
	  - truncate: 'end', 'start', 'middle'
	  - ellipsis: string (default "…")

	Padding filters:
		{{ value | pad_left(8) }}
		{{ value | pad_right(8) }}
		{{ value | pad_center(8) }}
	:: jinja ::

	Truncation filter:
		{{ value | truncate_at(20) }}
		{{ value | truncate_at(20, 'middle') }}
		{{ value | truncate_at(20, 'start', '...') }}
	:: jinja ::

	Width measurement:
		{% if value | display_width > 20 %}...{% endif %}
	:: jinja ::


76. How do tables integrate with CSV output?

	FlatDataSpec provides structured CSV extraction via render_auto_with_spec:
		let spec = FlatDataSpec::builder()
		    .column(Column::new(Width::Fixed(10))
		        .key("name")
		        .header("Name"))
		    .column(Column::new(Width::Fixed(10))
		        .key("meta.role")
		        .header("Role"))
		    .build();

		let output = render_auto_with_spec(
		    "unused template",
		    &data,
		    &theme,
		    OutputMode::Csv,
		    Some(&spec),
		)?;
	:: rust ::

	The key field uses dot notation for nested JSON:
	  "name" extracts data["name"]
	  "meta.role" extracts data["meta"]["role"]

	extract_header() returns the header row from column headers.
	extract_row(item) extracts values for one data item.

	Without a spec, CSV mode flattens JSON automatically, but column
	order and headers are less predictable.


77. How do I format a complete table in a template?

	For simple tables, use filters directly:
		{% for entry in entries %}
		{{ entry.hash | col(7) }}  {{ entry.author | col(15) }}  {{ entry.message | col(50) }}
		{% endfor %}
	:: jinja ::

	For more control, create a formatter and pass it to the template:
		let spec = FlatDataSpec::builder()
		    .column(Column::new(Width::Fixed(7)))
		    .column(Column::new(Width::Fixed(15)))
		    .column(Column::new(Width::Fill))
		    .separator("  ")
		    .build();

		let formatter = TableFormatter::new(&spec, terminal_width);
		// Pass formatter to template context
	:: rust ::

	TableFormatter implements MiniJinja's Object trait, so in templates:
		{% for entry in entries %}
		{{ table.row([entry.hash, entry.author, entry.message]) }}
		{% endfor %}
	:: jinja ::


78. How does Unicode handling work?

	All table functions are Unicode-aware:

	Display width: CJK characters count as 2 columns, combining marks as 0:
		display_width("café")   // 4 (é is 1 column)
		display_width("日本")    // 4 (each is 2 columns)
	:: rust ::

	ANSI codes: Escape sequences are preserved but don't count toward width:
		let styled = "\x1b[31mred\x1b[0m";
		display_width(styled)   // 3 (just "red")
		truncate_end(styled, 2, "…")  // "\x1b[31mre…\x1b[0m" (codes preserved)
	:: rust ::

	Truncation: Cuts at grapheme boundaries, never mid-character:
		truncate_end("café", 3, "…")  // "ca…" (not "caf" + partial é)
	:: rust ::


79. What happens when content doesn't fit?

	When content exceeds column width:

	1. Truncation applied based on TruncateAt setting
	2. Ellipsis inserted at truncation point
	3. Ellipsis width counted toward column width

	Example with Width::Fixed(8) and ellipsis "…" (1 column):
		"Hello World" -> "Hello W…"  (7 chars + 1 ellipsis = 8)
	:: text ::

	When content is shorter than column width:
	1. Padding applied based on Align setting
	2. Left: "hi      " (padding right)
	3. Right: "      hi" (padding left)
	4. Center: "   hi   " (padding both, extra goes right if odd)


80. How do I handle missing or null values?

	Each Column has a null_repr field (default "-"):
		Column::new(Width::Fixed(10))
		    .null_repr("N/A")
	:: rust ::

	When extract_row encounters a missing key or null value:
		data: { "name": "Alice" }  // No "role" field
		spec with key("role")
		extract_row(data) -> uses null_repr for role column
	:: text ::

	In templates, use Jinja's default filter for inline handling:
		{{ value | default("-") | col(10) }}
	:: jinja ::

